//! ★ **Insert Sequence — 내장 확장**(148차 · 사용자 10-27 "SDK를 보완해 확장으로" · [docs/124 §7](../../../../docs/124-insert-sequence.md) 권장안 B):
//! 엔진 `nsql-seq` 위의 얇은 정책 층 = 명령 하나(`edit.insert_sequence` · 기본 단축키 Ctrl+Alt+N) + 프롬프트 선언(자리표시자 설정 · 안내 ·
//! 치트 시트) + 설정 선언(`ext.insert_sequence.default`) + `transform`(표현식 → 값 n개). 세션(구간 읽기 · 교체 · 복원 · 되돌리기 · 프롬프트
//! UI)은 호스트 `app/insseq.rs`가 전부 든다 — 확장은 값만 **제안**.
//!
//! - ★ 149차(사용자 10-28 "단축키·설정값을 모두 확장 영역으로"): 정적 키맵·레지스트리·팔레트 항목을 걷고 **이 확장이 선언**한다 — 단축키는
//!   `commands()[].key`(→ 호스트가 `key.edit.insert_sequence` 설정을 확장 분류에 등재 · 사용자 지정 가능) · 기본 표현식은 `settings_defs()`
//!   (→ Preferences ▸ Extensions ▸ Insert Sequence · 옛 `editor.seq_default`는 RENAMED로 이주). wasm 판(SDK 샘플)과 같은 키·같은 선언.
//! - 같은 id의 wasm 패키지(`extensions/insert-sequence`)를 설치하면 그쪽이 앞선다(Rainbow Pairs 방식 · `Registry::active_indices`) · 로드 실패면
//!   이 내장이 폴백.
//! - 설치 기록 없이 **기본 켜짐**(D-368) — 끄기/켜기만(`extensions.disabled`) · 확장 패널에는 설치됨(builtin · 앱 버전)으로 보인다.
//! - Edit 메뉴 항목(menus.rs)은 앱의 정적 메뉴 — 단축키 표시는 동적 키맵에서 온다.

use super::{
    Command, EditorOps, Extension, ExtensionEffect, Label, MenuContribution, Prompt, SettingDef,
    SettingDefKind, TransformOut, TransformReq,
};
use nsql_i18n::{t, tf, Msg};
use nsql_settings::Settings;

pub(crate) struct InsertSequence;

/// 확장 id · 프롬프트(명령) id · 기본 표현식 설정 키(wasm 샘플과 같다).
pub(crate) const ID: &str = "insert-sequence";
pub(crate) const CMD: &str = "edit.insert_sequence";
pub(crate) const KEY_DEFAULT: &str = "ext.insert_sequence.default";

impl Extension for InsertSequence {
    fn id(&self) -> &str {
        ID
    }

    fn settings_prefix(&self) -> &str {
        "ext.insert_sequence."
    }

    fn name(&self) -> &str {
        "Insert Sequence"
    }

    fn default_on(&self) -> bool {
        true
    }

    /// 명령 = 프롬프트 열기(팔레트 "Insert Sequence: Open" · 기본 단축키 Ctrl+Alt+N / Cmd+Alt+N · 149차 = 정적 키맵에서 여기로).
    fn commands(&self) -> Vec<Command> {
        vec![Command {
            id: CMD.to_string(),
            label: Label::Msg(Msg::ExtSeqCmdOpen),
            key: Some("ctrl+alt+n".to_string()),
            key_mac: Some("cmd+alt+n".to_string()),
        }]
    }

    fn menus(&self) -> Vec<MenuContribution> {
        Vec::new()
    }

    /// 설정 선언(149차 = 레지스트리 `editor.seq_default`에서 여기로): 빈 입력의 기본 표현식.
    fn settings_defs(&self) -> Vec<SettingDef> {
        vec![SettingDef {
            key: KEY_DEFAULT.to_string(),
            label: Label::Msg(Msg::LblSeqDefault),
            desc: Label::Msg(Msg::DescSeqDefault),
            kind: SettingDefKind::Text,
            default: nsql_seq::DEFAULT_EXPR.to_string(),
        }]
    }

    fn on_settings(&mut self, _s: &Settings) -> ExtensionEffect {
        ExtensionEffect::default()
    }

    /// 프롬프트 명령은 호스트가 세션으로 연다(`App::prompt_open`) — 여기서 할 일 없음.
    fn run(&mut self, _id: &str, _ed: &mut dyn EditorOps) -> bool {
        false
    }

    fn prompts(&self) -> Vec<Prompt> {
        vec![Prompt {
            id: CMD.to_string(),
            label: Label::Msg(Msg::MnInsertSequence),
            placeholder: nsql_seq::DEFAULT_EXPR.to_string(),
            placeholder_key: Some(KEY_DEFAULT.to_string()),
            hint: Label::Msg(Msg::SeqHint),
            key: None,
            help: nsql_seq::HELP
                .iter()
                .map(|(e, en, ko)| {
                    (
                        e.to_string(),
                        Label::Text(en.to_string(), Some(ko.to_string())),
                    )
                })
                .collect(),
        }]
    }

    /// 표현식 → 값 n개(오류 = 호스트 언어의 문구 · 본문은 호스트가 공백으로).
    fn transform(&self, req: &TransformReq, _s: &Settings) -> Result<TransformOut, String> {
        Ok(
            match nsql_seq::parse(&req.text).and_then(|p| p.values(req.n)) {
                Ok(values) => TransformOut {
                    values,
                    ..TransformOut::default()
                },
                Err(e) => TransformOut {
                    error: Some(seq_err_text(&e)),
                    ..TransformOut::default()
                },
            },
        )
    }
}

/// 엔진 오류 → 사용자 문구(i18n).
fn seq_err_text(e: &nsql_seq::SeqErr) -> String {
    use nsql_seq::SeqErr as E;
    match e {
        E::Parse(s) => tf(Msg::SeqErrParse, &[s]),
        E::ProductNeedsLists => t(Msg::SeqErrProduct).into(),
        E::EmptyList => t(Msg::SeqErrEmptyList).into(),
        E::BaseNeedsInteger => t(Msg::SeqErrBase).into(),
        E::Overflow => t(Msg::SeqErrOverflow).into(),
        E::RangeTooLarge(n) => tf(Msg::SeqErrRange, &[&n.to_string()]),
        E::UnitNeedsTimeOrDate(c) => tf(Msg::SeqErrUnit, &[&c.to_string()]),
        E::PatternNeedsTimeOrDate => t(Msg::SeqErrPattern).into(),
        E::BadRepeat => t(Msg::SeqErrRepeat).into(),
        E::WidthTooLarge => t(Msg::SeqErrWidth).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 명령(키 포함) · 설정 선언(접두 · 기본 1+1) · 프롬프트(자리표시자 키 = 그 설정) · 치트 시트 · 기본 켜짐 · 변환 = 값 n개 / 오류 문구.
    #[test]
    fn prompt_meta_and_transform() {
        let x = InsertSequence;
        assert!(x.default_on());
        let c = x.commands();
        assert_eq!(c.len(), 1);
        assert_eq!(
            (
                c[0].id.as_str(),
                c[0].key.as_deref(),
                c[0].key_mac.as_deref()
            ),
            (CMD, Some("ctrl+alt+n"), Some("cmd+alt+n"))
        );
        let d = x.settings_defs();
        assert_eq!(d.len(), 1);
        assert!(d[0].key.starts_with(x.settings_prefix()) && d[0].default == "1+1");
        let p = x.prompts();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].id, CMD);
        assert_eq!(p[0].placeholder, "1+1");
        assert_eq!(p[0].placeholder_key.as_deref(), Some(KEY_DEFAULT));
        assert_eq!(p[0].help.len(), nsql_seq::HELP.len());
        let s = Settings::from_text(std::path::PathBuf::from("x"), "");
        let req = TransformReq {
            id: CMD.into(),
            text: "a+1".into(),
            n: 3,
            ..TransformReq::default()
        };
        let out = x.transform(&req, &s).expect("transform");
        assert_eq!(out.values, vec!["a", "b", "c"]);
        assert!(out.error.is_none());
        let bad = TransformReq {
            text: "[a]x1".into(),
            n: 2,
            ..req
        };
        let out = x.transform(&bad, &s).expect("transform");
        assert!(out.values.is_empty() && out.error.is_some());
    }
}
