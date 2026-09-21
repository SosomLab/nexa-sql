//! **세션 자격 금고**(사용자 09-21) — 입력 창으로 한 번 받은 비밀번호를 **이 실행 동안만** 다시 쓸 수 있게 든다.
//!
//! 왜: 일회성 비밀번호([`nsql_core::Secret`])는 접속에 쓰고 지운다. 그러면 같은 서버에 **두 번째 연결**(이 연결로 새 탭의 전용 세션 ·
//! 끊긴 뒤 재접속 · 탐색기 메타 세션의 유휴 재개 · 개별 모드의 탭마다 접속)을 열 때마다 다시 물어야 한다.
//!
//! 어떻게: 평문을 들고 있지 않는다. 받은 값은 곧바로 **봉투**(ChaCha20-Poly1305 · 프로필 비밀번호와 같은 [`crate::sealed`])에 넣고,
//! 봉투의 키는 **이 프로세스가 시작할 때 뽑은 난수 32바이트**다 — 디스크에 쓰지 않는다 · 앱이 끝나면 키도 봉투도 사라진다 ·
//! 다른 프로세스·다음 실행에서는 열 수 없다. 꺼낼 때만 잠깐 평문(`Secret`)이 생기고, 쓴 쪽이 버리면 0으로 덮인다.
//! 도메인 = 접속 문자열(비밀번호 없는 `dialect://user@host:port/db`) → 한 서버·계정의 봉투를 다른 자리에 끼워도 열리지 않는다.
//!
//! **한계(정직하게)**: 키와 봉투가 같은 프로세스 메모리에 있다 — 메모리에서 평문 글자열을 찾는 탐지(문자열 스캔 · 덤프 속 비밀번호
//! 검색)에는 걸리지 않지만, 이 프로세스의 메모리를 읽고 형식을 아는 공격자까지 막는 것은 아니다(같은 프로세스 안의 어떤 방식도 같다).
//! 디스크에 남기고 싶으면 그것은 "프로필로 저장"이다([`crate::Vault::save`] — 기기 키 봉투).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use nsql_core::Secret;

struct MemVault {
    key: [u8; 32],
    items: HashMap<String, Vec<u8>>,
}

fn vault() -> Option<&'static Mutex<MemVault>> {
    static V: OnceLock<Option<Mutex<MemVault>>> = OnceLock::new();
    V.get_or_init(|| {
        // 난수를 못 얻으면 금고 없이 돈다(= 매번 묻는다 · fail-closed).
        crate::devkey::random32().ok().map(|key| {
            Mutex::new(MemVault {
                key,
                items: HashMap::new(),
            })
        })
    })
    .as_ref()
}

/// 봉투에 넣어 든다(같은 자리는 **덮어쓴다** — 한 서버·계정의 자리는 하나다). 빈 값도 든다: `user:@host`로 **명시한 빈
/// 비밀번호**도 그 서버의 "지금 자격"이다(사용자 09-21).
pub fn remember(id: &str, secret: &Secret) {
    let Some(v) = vault() else { return };
    let Ok(mut g) = v.lock() else { return };
    if let Ok(sealed) = crate::sealed::seal(id.as_bytes(), &g.key, secret.expose().as_bytes()) {
        g.items.insert(id.to_string(), sealed);
    }
}

/// 꺼낸다 — 없거나 열리지 않으면 `None`. 평문은 `Secret` 안에만 있다(버려지면 0으로 덮인다).
#[must_use]
pub fn recall(id: &str) -> Option<Secret> {
    let g = vault()?.lock().ok()?;
    let plain = crate::sealed::open(id.as_bytes(), &g.key, g.items.get(id)?)?;
    // 바이트열을 **옮겨** 담는다(사본 없음).
    String::from_utf8(plain).ok().map(Secret::new)
}

/// 그 자리에 든 값이 `candidate`와 같은가(자리가 비었으면 `false`) — "같은 서버에 **다른 자격**으로 다시 접속하려는가"의 판정.
#[must_use]
pub fn matches(id: &str, candidate: &str) -> bool {
    recall(id).is_some_and(|s| s.expose() == candidate)
}

/// 그 자리를 잊는다(틀린 비밀번호였다 · 서버를 목록에서 뺐다).
pub fn forget(id: &str) {
    if let Some(Ok(mut g)) = vault().map(Mutex::lock) {
        g.items.remove(id);
    }
}

/// 전부 잊는다(설정을 껐다) — 봉투 바이트도 0으로 덮고 버린다.
pub fn clear() {
    if let Some(Ok(mut g)) = vault().map(Mutex::lock) {
        for sealed in g.items.values_mut() {
            sealed.fill(0);
        }
        std::hint::black_box(&g.items);
        g.items.clear();
    }
}

/// **프로그램 종료**(사용자 09-21): 봉투를 전부 덮어써 버리고 **키도 0으로** 덮는다 → 이 뒤로는 어떤 봉투도 열리지 않는다.
/// (프로세스가 끝나면 메모리는 어차피 사라지지만, 종료 직전의 덤프·스왑에도 키가 남지 않게 명시적으로 지운다.)
pub fn shutdown() {
    clear();
    if let Some(Ok(mut g)) = vault().map(Mutex::lock) {
        g.key.fill(0);
        std::hint::black_box(&g.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_per_server_and_never_holds_plaintext() {
        let a = "oracle://scott@test-session-a:1521/orcl";
        let b = "oracle://scott@test-session-b:1521/orcl";
        remember(a, &Secret::new("tiger-한글".into()));
        assert_eq!(recall(a).unwrap().expose(), "tiger-한글");
        assert!(recall(b).is_none(), "다른 서버의 자리는 비어 있다");
        // 봉투에는 평문이 없다 · 다른 자리에 끼우면 열리지 않는다(도메인 분리).
        {
            let mut g = vault().unwrap().lock().unwrap();
            let sealed = g.items.get(a).unwrap().clone();
            assert!(!sealed.windows(5).any(|w| w == b"tiger"));
            g.items.insert(b.to_string(), sealed);
        }
        assert!(recall(b).is_none());
        forget(a);
        forget(b);
        assert!(recall(a).is_none());
        // 명시한 빈 비밀번호도 자리를 차지한다 · 자리는 하나(덮어쓴다) · 같은 값인가.
        remember(a, &Secret::new("old".into()));
        remember(a, &Secret::new(String::new()));
        assert_eq!(recall(a).unwrap().expose(), "");
        assert!(matches(a, "") && !matches(a, "old") && !matches(b, ""));
        forget(a);
    }
}
