//! **Rainbow Pairs(WASM)** — 앱 안의 `rainbow_pairs.rs`(in-process · builtin)와 같은 정책 층을 SDK 위에 쓴 것.
//! 쌍 표·색칠·이동·자동 닫기는 앱 코어(nexa-ctl)가 하고, 이 확장은 "무엇을 켤지 · 어떤 색 · 어떤 명령/메뉴"만 정한다.
//! 설정 키 = `rainbowpair.*`(앱 레지스트리에 있는 것 · 호스트가 접두 키만 넘겨 준다).

use nexa_ext_sdk::{BracketEffect, Command, Editor, Effect, Extension, Label, Menu, Meta, Settings};

struct RainbowPairs;

fn cmd(id: &str, en: &str, ko: &str) -> Command {
    Command {
        id: id.into(),
        label: Label::new(en, ko),
    }
}

/// `rainbowpair.colors` = `#RRGGBB, …` → 유효한 항목만(앱이 다시 검증한다).
fn parse_colors(spec: &str) -> Vec<String> {
    spec.split(',')
        .map(|c| c.trim().trim_start_matches('#'))
        .filter(|h| h.len() >= 6 && h[..6].chars().all(|c| c.is_ascii_hexdigit()))
        .map(|h| format!("#{}", &h[..6].to_ascii_uppercase()))
        .collect()
}

impl Extension for RainbowPairs {
    fn meta() -> Meta {
        let commands = vec![
            cmd("edit.goto_bracket", "Go to Matching Bracket", "짝 괄호로"),
            cmd("edit.bracket_prev", "Previous Sibling Bracket", "이전 형제 괄호"),
            cmd("edit.bracket_next", "Next Sibling Bracket", "다음 형제 괄호"),
            cmd("edit.bracket_parent", "Parent Bracket", "상위 괄호"),
            cmd("edit.bracket_child", "First Child Bracket", "하위 괄호"),
            cmd("edit.expand_brackets", "Expand Selection to Brackets", "괄호까지 선택 확장"),
        ];
        let items = commands.iter().map(|c| c.id.clone()).collect();
        Meta {
            id: "rainbow-pairs".into(),
            name: "Rainbow Pairs".into(),
            settings_prefix: "rainbowpair.".into(),
            commands,
            menus: vec![Menu {
                id: "brackets".into(),
                label: Label::new("Bracket Navigation", "괄호 이동"),
                items,
            }],
        }
    }

    /// 확장이 정하는 것 = 색 층뿐(깊이 색 켬 · 짝 없음 색 · 색 목록 · 상한). 쌍 종류·문자열 안·현재 쌍·자동 닫기는
    /// 편집 코어 설정(`editor.pair_*`)이라 호스트가 넣는다.
    fn on_settings(s: &Settings) -> Effect {
        let kb = s.int("rainbowpair.max_kb");
        Effect {
            bracket: Some(BracketEffect {
                rainbow: s.flag("rainbowpair.enabled"),
                unmatched: s.flag("rainbowpair.unmatched"),
                colors: parse_colors(s.get("rainbowpair.colors").unwrap_or("")),
                max_chars: if kb <= 0 { 0 } else { (kb.max(64) as u64) * 1024 },
            }),
        }
    }

    /// 꺼짐 = 일반 편집 모드(괄호에 색을 입히지 않는다).
    fn disabled() -> Effect {
        Effect {
            bracket: Some(BracketEffect {
                rainbow: false,
                unmatched: false,
                colors: Vec::new(),
                max_chars: 0,
            }),
        }
    }

    fn run(cmd: &str, ed: &mut Editor) -> bool {
        match cmd {
            "edit.goto_bracket" => ed.goto_bracket(false),
            "edit.bracket_prev" => ed.goto_bracket_sibling(false, false),
            "edit.bracket_next" => ed.goto_bracket_sibling(true, false),
            "edit.bracket_parent" => ed.goto_bracket_parent(false),
            "edit.bracket_child" => ed.goto_bracket_child(false),
            "edit.expand_brackets" => ed.expand_to_brackets(),
            _ => false,
        }
    }
}

nexa_ext_sdk::export_extension!(RainbowPairs);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_effect() {
        assert_eq!(parse_colors("#ff0000, bad, 00FF00AA"), vec!["#FF0000", "#00FF00"]);
        let s = Settings::from_json(r#"{"rainbowpair.enabled":"on","rainbowpair.unmatched":"off","rainbowpair.max_kb":"128"}"#);
        let e = RainbowPairs::on_settings(&s).bracket.expect("bracket");
        assert!(e.rainbow && !e.unmatched && e.max_chars == 128 * 1024);
        assert_eq!(RainbowPairs::meta().commands.len(), 6);
    }
}
