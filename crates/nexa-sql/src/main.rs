//! Nexa SQL GUI 진입점 — M0 자리표시. M1 수직 슬라이스(docs/02)에서 `nexa-ctl`로 창을 열고
//! 편집기 → 엔진 → 드라이버 → 그리드 한 줄을 잇는다.

fn main() {
    println!(
        "Nexa SQL {} — GUI는 M1에서 시작합니다. 지금은 `nsql plan`을 쓰세요.",
        env!("CARGO_PKG_VERSION")
    );
    let _ = nsql_script::Engine::new(nsql_core::Dialect::Oracle);
}
