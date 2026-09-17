//! **첫 in-process 플러그인 — 레인보우 괄호 + 괄호 이동**(docs/51 · D-91~95 · 사용자 09-17).
//!
//! 설정 `rainbow.*` → 편집기 [`nexa_ctl::BracketOpts`] · 명령 4(형제 이전/다음 · 상위 · 하위 · 짝/확장은 편집 코어 명령을 재사용) ·
//! 우클릭 편집 메뉴 "괄호 이동 ▸". 쌍 표·색·자동 닫기는 nexa-ctl 코어(같은 표) — 이 파일은 "옵션·명령·메뉴"만 든다.

use super::{Command, EditorOps, MenuContribution, Plugin, PluginEffect};
use nexa_ctl::{BracketOpts, PairOpts};
use nsql_i18n::Msg;
use nsql_settings::Settings;

pub(crate) struct RainbowPlugin;

/// 설정 `rainbow.colors`(`#RRGGBB,...`) 파싱 — 잘못된 항목은 건너뜀.
fn parse_colors(spec: &str) -> Vec<nexa_ctl::Color> {
    spec.split(',')
        .map(|c| c.trim().trim_start_matches('#'))
        .filter_map(|h| nexa_ctl::color_from_hex(h.get(..6).unwrap_or(h)))
        .collect()
}

impl Plugin for RainbowPlugin {
    fn id(&self) -> &'static str {
        "rainbow-brackets"
    }

    fn settings_prefix(&self) -> &'static str {
        "rainbow."
    }

    fn commands(&self) -> Vec<Command> {
        vec![
            Command {
                id: "edit.goto_bracket",
                label: Msg::MnGotoBracket,
            },
            Command {
                id: "edit.bracket_prev",
                label: Msg::MnBracketPrev,
            },
            Command {
                id: "edit.bracket_next",
                label: Msg::MnBracketNext,
            },
            Command {
                id: "edit.bracket_parent",
                label: Msg::MnBracketParent,
            },
            Command {
                id: "edit.bracket_child",
                label: Msg::MnBracketChild,
            },
            Command {
                id: "edit.expand_brackets",
                label: Msg::MnExpandBrackets,
            },
        ]
    }

    fn menus(&self) -> Vec<MenuContribution> {
        vec![MenuContribution {
            id: "brackets",
            label: Msg::MnBracketMenu,
            items: self.commands(),
        }]
    }

    fn on_settings(&mut self, s: &Settings) -> PluginEffect {
        let opts = BracketOpts {
            rainbow: s.flag("rainbow.enabled"),
            pairs: PairOpts {
                quotes: s.flag("rainbow.quotes"),
                angle: s.flag("rainbow.angle"),
            },
            unmatched: s.flag("rainbow.unmatched"),
            match_mode: match s.get("rainbow.match").unwrap_or("near") {
                "off" => 0,
                "always" => 2,
                _ => 1,
            },
            colors: parse_colors(s.get("rainbow.colors").unwrap_or("")),
            auto_close: s.flag("rainbow.auto_close"),
            max_chars: (s.int("rainbow.max_kb").max(64) as usize) * 1024,
        };
        PluginEffect {
            bracket_opts: Some(opts),
        }
    }

    fn run(&mut self, id: &str, ed: &mut dyn EditorOps) -> bool {
        match id {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_and_commands_registered() {
        let c = parse_colors("#FF0000, bad, #00FF00AA");
        assert_eq!(
            c,
            vec![nexa_ctl::Color(0x00FF_0000), nexa_ctl::Color(0x0000_FF00)]
        );
        let p = RainbowPlugin;
        assert_eq!(p.commands().len(), 6);
        assert_eq!(p.menus()[0].items.len(), 6);
    }
}
