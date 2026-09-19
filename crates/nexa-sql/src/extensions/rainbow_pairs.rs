//! **첫 in-process 확장 — 레인보우 괄호 + 괄호 이동**(docs/51 · D-91~95 · 사용자 09-17).
//!
//! 설정 `rainbowpair.*` → 편집기 [`nexa_ctl::BracketOpts`] · 명령 4(형제 이전/다음 · 상위 · 하위 · 짝/확장은 편집 코어 명령을 재사용) ·
//! 우클릭 편집 메뉴 "괄호 이동 ▸". 쌍 표·색·자동 닫기는 nexa-ctl 코어(같은 표) — 이 파일은 "옵션·명령·메뉴"만 든다.

use super::{Command, EditorOps, Extension, ExtensionEffect, MenuContribution};
use nexa_ctl::{BracketOpts, PairOpts};
use nsql_i18n::Msg;
use nsql_settings::Settings;

/// **Rainbow Pairs** — 괄호·인용부호 쌍을 깊이별 색으로 구별하고 짝·형제·상위·하위로 이동하는 확장(사용자 09-17 명명).
pub(crate) struct RainbowPairs;

/// 설정 `rainbowpair.colors`(`#RRGGBB,...`) 파싱 — 잘못된 항목은 건너뜀.
fn parse_colors(spec: &str) -> Vec<nexa_ctl::Color> {
    spec.split(',')
        .map(|c| c.trim().trim_start_matches('#'))
        .filter_map(|h| nexa_ctl::color_from_hex(h.get(..6).unwrap_or(h)))
        .collect()
}

impl Extension for RainbowPairs {
    fn id(&self) -> &'static str {
        "rainbow-pairs"
    }

    fn settings_prefix(&self) -> &'static str {
        "rainbowpair."
    }

    fn name(&self) -> &'static str {
        "Rainbow Pairs"
    }

    /// 꺼짐·미설치·확장 관리자 꺼짐 = **일반 편집 모드: 괄호에 색을 입히지 않는다**(깊이 색 · 짝 없음 빨강 모두 끔 ·
    /// 사용자 09-19 — 종전에는 nexa-ctl 기본값(`rainbow: true`)이 그대로 들어가 확장이 없어도 색이 보였다). 캐럿 옆 쌍의
    /// 밑줄(글자색 그대로 · 색 없음)·쌍 표·자동 닫기는 편집 코어 기능이라 남는다(Ctrl+M 짝 이동 · `editor.auto_close_pairs`).
    fn disabled_effect(&self) -> ExtensionEffect {
        ExtensionEffect {
            bracket_opts: Some(BracketOpts {
                rainbow: false,
                unmatched: false,
                ..BracketOpts::default()
            }),
        }
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

    fn on_settings(&mut self, s: &Settings) -> ExtensionEffect {
        let opts = BracketOpts {
            rainbow: s.flag("rainbowpair.enabled"),
            pairs: PairOpts {
                quotes: s.flag("rainbowpair.quotes"),
                angle: s.flag("rainbowpair.angle"),
            },
            unmatched: s.flag("rainbowpair.unmatched"),
            match_mode: match s.get("rainbowpair.match").unwrap_or("near") {
                "off" => 0,
                "always" => 2,
                _ => 1,
            },
            // 사용자 색 목록 = 이웃 깊이가 잘 구별되게 다시 배열(보색·색 온도·밝기 · 사용자 09-19 · 끄면 적은 순서).
            //   비면 테마 팔레트(이미 같은 규칙으로 정렬돼 있다 · nexa-ctl `Theme.rainbow`).
            colors: {
                let c = parse_colors(s.get("rainbowpair.colors").unwrap_or(""));
                if s.flag("rainbowpair.contrast_order") {
                    nexa_ctl::contrast_order(&c)
                } else {
                    c
                }
            },
            // 자동 닫기는 편집 코어 기능(`editor.auto_close_pairs`) — 확장은 관여하지 않는다(호스트가 값을 넣는다 · 사용자 09-19).
            auto_close: true,
            max_chars: (s.int("rainbowpair.max_kb").max(64) as usize) * 1024,
        };
        ExtensionEffect {
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
        let p = RainbowPairs;
        // 일반 편집 모드(확장 꺼짐/미설치) = 괄호 색 없음.
        let off = p.disabled_effect().bracket_opts.unwrap_or_default();
        assert!(!off.rainbow && !off.unmatched && off.auto_close);
        assert_eq!(p.commands().len(), 6);
        assert_eq!(p.menus()[0].items.len(), 6);
    }
}
