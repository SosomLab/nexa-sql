//! **Insert Sequence(WASM)** — 앱 안의 내장 확장(`crates/nexa-sql/src/extensions/insert_sequence.rs`)과 같은 정책 층을 SDK 위에 쓴 것
//! (148차 · 10-27 · docs/124 §7 권장안 B "프롬프트 변환" API · ABI v1.2).
//!
//! - 엔진 = 앱과 같은 `nsql-seq`(path 의존 · 문법 docs/124 §2) · 세션(다중 캐럿 구간 읽기 · 값 교체 · Enter 확정 · Esc 복원 · 되돌리기 ·
//!   프롬프트 UI)은 **호스트**가 든다 — 이 확장은 키 입력마다 `transform`으로 **표현식 → 값 n개**만 낸다(본문 쓰기 권한 0).
//! - 설치하면 같은 id(`insert-sequence`)의 내장을 대체한다(Rainbow Pairs 방식) · 설정 `ext.insert_sequence.default` = 빈 입력의 기본 표현식.
//!
//! 빌드: `extensions/sdk`에서 `cargo build --release` → `target/wasm32-unknown-unknown/release/insert_sequence_ext.wasm`
//! → `pwsh scripts/ext-build.ps1 -Only insert-sequence`(패키지 폴더 배치 + sha256) → 확장 패널에서 설치/업데이트.

use nexa_ext_sdk::{
    Editor, Effect, Extension, Label, Meta, Prompt, SettingDef, Settings, TransformRequest,
    TransformResponse,
};

/// 확장 id · 프롬프트(명령) id(앱의 Edit ▸ Insert Sequence · Ctrl+Alt+N과 같은 id = 내장 대체).
const ID: &str = "insert-sequence";
const CMD: &str = "edit.insert_sequence";
/// 설정 키(접두 = `settings_prefix`) — 빈 입력의 기본 표현식.
const KEY_DEFAULT: &str = "ext.insert_sequence.default";

struct InsertSequence;

/// 엔진 오류 → 사용자 문구(영어 = `Display` · 한국어 = 앱 i18n과 같은 글).
fn err_text(e: &nsql_seq::SeqErr, ko: bool) -> String {
    use nsql_seq::SeqErr as E;
    // 앱 i18n(`Msg::SeqErr*`)과 같은 글 — 내장과 wasm이 같은 문구를 보이게(148차 V1 ④).
    match (e, ko) {
        (E::Parse(s), false) => format!("Cannot read '{s}' - type ? for help"),
        (E::Parse(s), true) => format!("'{s}'을(를) 읽을 수 없음 - ? 도움말"),
        (E::ProductNeedsLists, false) => "x needs lists on both sides, e.g. [a,b]x[1,2]".into(),
        (E::ProductNeedsLists, true) => "x는 양쪽이 목록이어야 함(예: [a,b]x[1,2])".into(),
        (E::EmptyList, false) => "Empty list".into(),
        (E::EmptyList, true) => "빈 목록".into(),
        (E::BaseNeedsInteger, false) => "Base formats (b o x) need an integer value".into(),
        (E::BaseNeedsInteger, true) => "진수 형식(b o x)은 정수 값에만".into(),
        (E::Overflow, false) => "Number too large".into(),
        (E::Overflow, true) => "수가 너무 큼".into(),
        (E::RangeTooLarge(n), false) => format!("Range too large (max {n})"),
        (E::RangeTooLarge(n), true) => format!("범위가 너무 큼(최대 {n})"),
        (E::UnitNeedsTimeOrDate(c), false) => format!("Unit {c} needs a time or date start"),
        (E::UnitNeedsTimeOrDate(c), true) => format!("단위 {c}은(는) 시각·날짜 시작값에만"),
        (E::PatternNeedsTimeOrDate, false) => "Pattern formats need a time or date start".into(),
        (E::PatternNeedsTimeOrDate, true) => "패턴 형식은 시각·날짜 시작값에만".into(),
        (E::BadRepeat, false) => "Repeat must be 1..1000".into(),
        (E::BadRepeat, true) => "반복은 1~1000".into(),
        (E::WidthTooLarge, false) => "Width must be at most 64".into(),
        (E::WidthTooLarge, true) => "자리수는 최대 64".into(),
    }
}

impl Extension for InsertSequence {
    fn meta() -> Meta {
        Meta {
            id: ID.into(),
            name: "Insert Sequence".into(),
            settings_prefix: "ext.insert_sequence.".into(),
            settings_read: vec![],
            settings_write: vec![],
            // 명령은 프롬프트 선언에서 호스트가 병합한다(같은 id · 같은 키).
            commands: vec![],
            menus: vec![],
            formatter: None,
            settings: vec![SettingDef::text(
                KEY_DEFAULT,
                Label::new("Default expression", "기본 표현식"),
                Label::new(
                    "Used when the prompt is empty (e.g. 1+1 · see ? in the prompt)",
                    "입력란이 비었을 때 쓰는 표현식(예 1+1 · 프롬프트에서 ? 도움말)",
                ),
                nsql_seq::DEFAULT_EXPR,
            )],
            prompts: vec![Prompt::new(
                CMD,
                Label::new(
                    "Insert Sequence: Open (WASM)",
                    "Insert Sequence: 열기 (WASM)",
                ),
                nsql_seq::DEFAULT_EXPR,
                Label::new(
                    "Enter = insert · Esc = cancel · ? = help · empty = {0}",
                    "Enter 넣기 · Esc 취소 · ? 도움말 · 비우면 {0}",
                ),
            )
            .with_key("ctrl+alt+n")
            .with_placeholder_key(KEY_DEFAULT)
            .with_help(
                nsql_seq::HELP
                    .iter()
                    .map(|(e, en, ko)| (*e, Label::new(en, ko)))
                    .collect(),
            )],
        }
    }

    fn on_settings(_s: &Settings) -> Effect {
        Effect::default()
    }

    /// 프롬프트 명령은 호스트가 세션으로 연다 — 여기서 할 일 없음.
    fn run(_cmd: &str, _ed: &mut Editor) -> bool {
        false
    }

    /// 표현식 → 값 n개(오류 = 요청 언어의 문구 · 본문은 호스트가 공백으로).
    fn transform(req: &TransformRequest) -> TransformResponse {
        match nsql_seq::parse(&req.text).and_then(|p| p.values(req.n)) {
            Ok(v) => TransformResponse::ok(v),
            Err(e) => TransformResponse::err(&err_text(&e, req.is_ko())),
        }
    }
}

nexa_ext_sdk::export_extension!(InsertSequence);

#[cfg(test)]
mod tests {
    use super::*;

    /// 메타 = 프롬프트 하나(id · 키 · 자리표시자 키 · 치트 시트) + 설정 하나(접두) · 변환 = 값 n개 / 언어별 오류.
    #[test]
    fn meta_prompt_and_transform() {
        let m = InsertSequence::meta();
        assert_eq!(m.id, ID);
        assert_eq!(m.prompts.len(), 1);
        let p = &m.prompts[0];
        assert_eq!((p.id.as_str(), p.key.as_deref()), (CMD, Some("ctrl+alt+n")));
        assert_eq!(p.placeholder_key.as_deref(), Some(KEY_DEFAULT));
        assert_eq!(p.help.len(), nsql_seq::HELP.len());
        assert!(m
            .settings
            .iter()
            .all(|s| s.key.starts_with(&m.settings_prefix)));
        let req = TransformRequest {
            id: CMD.into(),
            text: "a+1".into(),
            n: 3,
            ..TransformRequest::default()
        };
        let out = InsertSequence::transform(&req);
        assert_eq!(out.values, vec!["a", "b", "c"]);
        let bad = TransformRequest {
            text: "[a]x1".into(),
            lang: "ko".into(),
            ..req
        };
        let out = InsertSequence::transform(&bad);
        assert!(out.error.as_deref().is_some_and(|e| e.contains("목록")));
        let bad_en = TransformRequest {
            lang: "en".into(),
            ..bad
        };
        assert!(InsertSequence::transform(&bad_en)
            .error
            .as_deref()
            .is_some_and(|e| e.contains("lists")));
    }
}
