//! **되돌리기 기록 파일**(docs/60 D-129 · Vim `undofile`과 같은 쓰임): 파일을 **저장할 때** 그 탭의 되돌리기·다시 실행 기록을
//! `<설정 폴더>/undo/<경로 해시>.nsqu`에 쓰고, 그 파일을 **다시 열 때** 본문이 기록의 것과 같으면(길이 + 해시) 들인다 —
//! 앱을 껐다 켜도 Ctrl+Z가 이어진다. 본문이 밖에서 한 글자라도 바뀌었으면 기록은 조용히 버려진다(부분 복구 없음).
//!
//! - 형식·검증은 nexa-ctl `EditState::export_history`/`import_history`(매직 · 판 · 본문 길이·해시 · 꼬리 해시). 여기는 **어디에 · 언제**만.
//! - 기록에는 **지운 글자**가 평문으로 들어 있다(타이핑한 글자는 없다) — 사용자 설정 폴더 안에만 두고, 끄면(`editor.undo_persist`)
//!   쓰지도 읽지도 않으며, 오래된 파일은 `editor.undo_persist_days` 뒤에 지운다. 부하원 원장 = docs/39 §3.
//! - 큰 파일 모드 탭은 쓰지 않는다(본문 해시 65 MB · 기록도 클 수 있다).
//! - 폴더는 인자로 받는다(`*_in`) — 단위 테스트가 사용자의 실제 설정 폴더를 건드리지 않게. 앱은 [`dir`]을 넘긴다.

use std::path::{Path, PathBuf};

/// 기록 폴더(`<설정 폴더>/undo`).
pub(crate) fn dir() -> Option<PathBuf> {
    nsql_settings::config_dir().map(|d| d.join("undo"))
}

/// 파일 경로 → 기록 파일 이름. 열쇠 = 정규화한 경로 문자열의 해시(Windows는 대소문자를 가리지 않는다).
fn name_for(file: &Path) -> String {
    let full = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let mut key = full.to_string_lossy().into_owned();
    if cfg!(windows) {
        key = key.to_lowercase();
    }
    let mut h = nexa_ctl::edit::Hash64::new();
    h.write(key.as_bytes());
    format!("{:016x}.nsqu", h.finish())
}

/// 기록을 쓴다(임시 파일 → 이름 바꾸기) — `None`이면 옛 기록을 지운다(기록이 없는 상태로 저장했다).
pub(crate) fn store_in(dir: &Path, file: &Path, bytes: Option<&[u8]>) {
    let p = dir.join(name_for(file));
    let Some(bytes) = bytes else {
        let _ = std::fs::remove_file(&p);
        return;
    };
    let _ = std::fs::create_dir_all(dir);
    let tmp = p.with_extension("nsqu-tmp");
    if std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, &p))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// 기록을 읽는다(없으면 `None`). `max_bytes`보다 크면 읽지 않는다(설정을 줄인 뒤의 옛 파일).
pub(crate) fn load_in(dir: &Path, file: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let p = dir.join(name_for(file));
    let len = std::fs::metadata(&p).ok()?.len();
    if len == 0 || len > max_bytes {
        return None;
    }
    std::fs::read(&p).ok()
}

/// 오래된 기록 파일을 지운다(`days` = 0이면 아무것도 안 함). 돌려주는 값 = 지운 수.
pub(crate) fn prune_in(dir: &Path, days: u64) -> usize {
    if days == 0 {
        return 0;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let limit = std::time::Duration::from_secs(days * 86_400);
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "nsqu") {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 같은 파일 = 같은 기록 이름(대소문자 — Windows) · 다른 파일 = 다른 이름 · 쓰기 → 읽기 → 상한 → 지우기 · 오래된 것 치우기.
    /// (임시 폴더 안에서만 — 사용자의 설정 폴더는 건드리지 않는다.)
    #[test]
    fn store_load_remove_round_trip() {
        let home = std::env::temp_dir().join(format!("nsql-undofile-{}", std::process::id()));
        let undo = home.join("undo");
        let _ = std::fs::create_dir_all(&home);
        let a = home.join("a.sql");
        let b = home.join("b.sql");
        std::fs::write(&a, "select 1;").expect("write");
        std::fs::write(&b, "select 2;").expect("write");
        assert_ne!(name_for(&a), name_for(&b));
        assert_eq!(name_for(&a), name_for(&a));
        if cfg!(windows) {
            let upper = PathBuf::from(a.to_string_lossy().to_uppercase());
            assert_eq!(name_for(&upper), name_for(&a));
        }
        assert!(
            load_in(&undo, &a, 1 << 20).is_none(),
            "폴더가 없어도 조용히"
        );
        store_in(&undo, &a, Some(b"NSQU-test"));
        assert_eq!(
            load_in(&undo, &a, 1 << 20).as_deref(),
            Some(&b"NSQU-test"[..])
        );
        assert!(load_in(&undo, &b, 1 << 20).is_none());
        assert!(
            load_in(&undo, &a, 4).is_none(),
            "상한보다 큰 옛 파일은 읽지 않는다"
        );
        assert_eq!(prune_in(&undo, 30), 0, "방금 쓴 것은 남는다");
        assert_eq!(prune_in(&undo, 0), 0, "0 = 치우지 않는다");
        store_in(&undo, &a, None);
        assert!(load_in(&undo, &a, 1 << 20).is_none());
        let _ = std::fs::remove_dir_all(&home);
    }
}
