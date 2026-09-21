//! **한 번 쓰고 버리는 비밀 값**(사용자 09-21 — "접속에만 쓰고 버린다 · 메모리 탐지 포함").
//!
//! [`Secret`]은 비밀번호 같은 글을 쥐고 있다가 **버려질 때 그 바이트를 0으로 덮어쓴다**(컴파일러가 지우지 못하게 volatile 쓰기).
//! 복제(`Clone`)가 없고 `Debug`/`Display`는 내용을 찍지 않는다 → 로그·오류문·패닉 메시지로 새지 않는다.
//!
//! **한계(정직하게)**: 우리가 쥔 버퍼만 지울 수 있다. 드라이버 크레이트(tiberius · postgres · ODPI-C)가 접속 중에 만든 내부 사본,
//! OS의 입력 메시지 큐, 페이지 파일로 내려간 메모리는 이 타입의 손이 닿지 않는다. 그래서 규칙은 "우리 쪽 사본을 **최소로** ·
//! 쓰자마자 **지운다** · 어디에도 **저장하지 않는다**"이다(docs/21 §일회성 비밀번호).

use std::sync::atomic::{compiler_fence, Ordering};

/// 글자열의 바이트를 0으로 덮어쓰고 비운다(용량 전체 — 길이 뒤의 남는 칸에 앞선 내용이 있을 수 있다).
pub fn wipe_string(s: &mut String) {
    // SAFETY: 0은 유효한 UTF-8이고, 바로 뒤에 길이를 0으로 만든다.
    let v = unsafe { s.as_mut_vec() };
    let cap = v.capacity();
    let p = v.as_mut_ptr();
    for i in 0..cap {
        // SAFETY: `p..p+cap`은 이 Vec이 할당받은 범위다(초기화 여부와 무관하게 쓰기는 안전).
        unsafe { std::ptr::write_volatile(p.add(i), 0) };
    }
    compiler_fence(Ordering::SeqCst);
    v.clear();
}

/// `Option<String>`을 덮어쓰고 `None`으로.
pub fn wipe_opt(s: &mut Option<String>) {
    if let Some(v) = s.as_mut() {
        wipe_string(v);
    }
    *s = None;
}

/// 버려질 때 0으로 덮어쓰는 비밀 글.
pub struct Secret(String);

impl Secret {
    /// 글을 **옮겨** 담는다(사본을 만들지 않는다).
    #[must_use]
    pub fn new(s: String) -> Secret {
        Secret(s)
    }

    /// 내용 — 쓰는 자리에서만 잠깐 빌린다(받은 쪽이 사본을 만들면 그 사본도 [`wipe_string`]으로 지울 것).
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe_string(&mut self.0);
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wipe_overwrites_the_whole_allocation() {
        let mut s = String::with_capacity(32);
        s.push_str("tiger-한글");
        let (p, cap) = (s.as_ptr(), s.capacity());
        wipe_string(&mut s);
        assert!(s.is_empty());
        assert_eq!(s.capacity(), cap, "할당은 그대로(해제 전 덮어쓰기 확인용)");
        // SAFETY: 아직 `s`가 살아 있어 같은 할당을 가리킨다.
        let bytes = unsafe { std::slice::from_raw_parts(p, cap) };
        assert!(bytes.iter().all(|b| *b == 0));
    }

    #[test]
    fn secret_never_prints_its_content() {
        let s = Secret::new("hunter2".into());
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(s.expose(), "hunter2");
        let mut o = Some(String::from("x"));
        wipe_opt(&mut o);
        assert!(o.is_none());
    }
}
