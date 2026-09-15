//! 단축키 맵(사용자 09-15) — **Sublime Text 기본값**(Windows/Linux · macOS) + 설정 재정의(`key.<명령 id>`).
//!
//! - 명령 표([`COMMANDS`])가 단일 원천: id(메뉴·툴바·팔레트와 같은 어휘) · 라벨 · 플랫폼별 기본 코드.
//!   → T-67 Command 레지스트리의 씨앗(메뉴·팔레트·툴바·키맵이 같은 표를 읽는 방향).
//! - 코드 문법 = `ctrl+shift+p` · `cmd+shift+p` · `f5` · `ctrl+enter` · `alt+1`. `ctrl`/`cmd`/`primary`는 모두 **주 조합키**
//!   (Windows/Linux = Ctrl · macOS = ⌘)로 정규화한다. 여러 코드는 `|`로(`ctrl+y|ctrl+shift+z`).
//! - 설정 `key.<id>`(비어 있음 = 플랫폼 기본) · 단축키 캡처 창([`crate::keys_win`])이 쓴다.

use nsql_i18n::{t, Msg};
use nsql_settings::Settings;
use std::collections::HashMap;
use winit::keyboard::{Key, NamedKey};

/// 명령 하나 — 기본 코드는 Sublime Text 관례(없는 것은 `""`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Command {
    pub id: &'static str,
    pub label: Msg,
    pub win: &'static str,
    pub mac: &'static str,
}

/// 단축키가 붙는 명령 표(순서 = 캡처 창 목록 순서).
pub(crate) const COMMANDS: &[Command] = &[
    Command {
        id: "view.palette",
        label: Msg::MnCommandPalette,
        win: "ctrl+shift+p",
        mac: "cmd+shift+p",
    },
    Command {
        id: "file.new",
        label: Msg::MnNew,
        win: "ctrl+n",
        mac: "cmd+n",
    },
    Command {
        id: "file.close_tab",
        label: Msg::MnCloseTab,
        win: "ctrl+w",
        mac: "cmd+w",
    },
    Command {
        id: "tab.next",
        label: Msg::MnNextTab,
        win: "ctrl+tab|ctrl+pagedown",
        mac: "ctrl+tab|cmd+alt+right",
    },
    Command {
        id: "tab.prev",
        label: Msg::MnPrevTab,
        win: "ctrl+shift+tab|ctrl+pageup",
        mac: "ctrl+shift+tab|cmd+alt+left",
    },
    Command {
        id: "run.statement",
        label: Msg::MnRunStatement,
        win: "ctrl+enter",
        mac: "cmd+enter",
    },
    Command {
        id: "run.all",
        label: Msg::MnRunAll,
        win: "f5",
        mac: "f5",
    },
    Command {
        id: "conn.toggle",
        label: Msg::MnConnect,
        win: "ctrl+shift+c",
        mac: "cmd+shift+c",
    },
    Command {
        id: "view.log",
        label: Msg::MnLogWindow,
        win: "ctrl+`",
        mac: "ctrl+`",
    },
    Command {
        id: "view.theme",
        label: Msg::MnTheme,
        win: "ctrl+alt+t",
        mac: "cmd+alt+t",
    },
    Command {
        id: "view.lang",
        label: Msg::MnLanguage,
        win: "ctrl+alt+l",
        mac: "cmd+alt+l",
    },
    Command {
        id: "view.colors",
        label: Msg::MnColors,
        win: "",
        mac: "",
    },
    Command {
        id: "view.keys",
        label: Msg::MnKeys,
        win: "",
        mac: "",
    },
    Command {
        id: "view.explorer",
        label: Msg::MnExplorer,
        win: "ctrl+shift+e",
        mac: "cmd+shift+e",
    },
    Command {
        id: "edit.undo",
        label: Msg::MnUndo,
        win: "ctrl+z",
        mac: "cmd+z",
    },
    Command {
        id: "edit.redo",
        label: Msg::MnRedo,
        win: "ctrl+y|ctrl+shift+z",
        mac: "cmd+shift+z",
    },
    Command {
        id: "edit.cut",
        label: Msg::MnCut,
        win: "ctrl+x",
        mac: "cmd+x",
    },
    Command {
        id: "edit.copy",
        label: Msg::MnCopy,
        win: "ctrl+c",
        mac: "cmd+c",
    },
    Command {
        id: "edit.paste",
        label: Msg::MnPaste,
        win: "ctrl+v",
        mac: "cmd+v",
    },
    Command {
        id: "edit.select_all",
        label: Msg::MnSelectAll,
        win: "ctrl+a",
        mac: "cmd+a",
    },
];

/// 설정 키(`key.<id>`).
pub(crate) fn setting_key(id: &str) -> String {
    format!("key.{id}")
}

/// 플랫폼 기본 코드.
pub(crate) fn platform_default(c: &Command) -> &'static str {
    if cfg!(target_os = "macos") {
        c.mac
    } else {
        c.win
    }
}

/// 정규화된 조합(대소문자 무관 · 주 조합키 통일).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Chord {
    pub primary: bool,
    pub shift: bool,
    pub alt: bool,
    /// 소문자 글자 또는 이름(`enter` `tab` `f5` `pageup` …).
    pub key: String,
}

impl Chord {
    /// `ctrl+shift+p` 같은 문자열 파싱(빈 문자열·모르는 토큰 = None).
    pub(crate) fn parse(s: &str) -> Option<Chord> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut c = Chord {
            primary: false,
            shift: false,
            alt: false,
            key: String::new(),
        };
        let toks: Vec<&str> = s.split('+').map(str::trim).collect();
        let (mods, key) = toks.split_at(toks.len().saturating_sub(1));
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" | "super" | "meta" | "primary" | "win" => {
                    c.primary = true
                }
                "shift" => c.shift = true,
                "alt" | "option" | "opt" => c.alt = true,
                _ => return None,
            }
        }
        let k = key.first()?.to_ascii_lowercase();
        if k.is_empty() {
            return None;
        }
        c.key = k;
        Some(c)
    }

    /// 표시용(`Ctrl+Shift+P` · macOS `⌘⇧P`).
    pub(crate) fn display(&self) -> String {
        let key = match self.key.as_str() {
            "enter" => "Enter",
            "tab" => "Tab",
            "space" => "Space",
            "escape" => "Esc",
            "backspace" => "Backspace",
            "delete" => "Del",
            "pageup" => "PgUp",
            "pagedown" => "PgDn",
            "home" => "Home",
            "end" => "End",
            "up" => "↑",
            "down" => "↓",
            "left" => "←",
            "right" => "→",
            other => {
                return self.prefix() + &other.to_ascii_uppercase();
            }
        };
        self.prefix() + key
    }

    fn prefix(&self) -> String {
        let mac = cfg!(target_os = "macos");
        let mut s = String::new();
        if self.primary {
            s.push_str(if mac { "⌘" } else { "Ctrl+" });
        }
        if self.alt {
            s.push_str(if mac { "⌥" } else { "Alt+" });
        }
        if self.shift {
            s.push_str(if mac { "⇧" } else { "Shift+" });
        }
        s
    }

    /// 저장용 문자열(`ctrl+shift+p` · macOS면 `cmd+…`).
    pub(crate) fn code(&self) -> String {
        let mut s = String::new();
        if self.primary {
            s.push_str(if cfg!(target_os = "macos") {
                "cmd+"
            } else {
                "ctrl+"
            });
        }
        if self.alt {
            s.push_str("alt+");
        }
        if self.shift {
            s.push_str("shift+");
        }
        s + &self.key
    }

    /// winit 키 → 조합(조합키만 눌린 경우 None).
    pub(crate) fn from_winit(key: &Key, primary: bool, shift: bool, alt: bool) -> Option<Chord> {
        let name = match key {
            Key::Character(t) => {
                let c = t.chars().next()?;
                if c.is_control() || c.is_whitespace() {
                    return None;
                }
                c.to_ascii_lowercase().to_string()
            }
            Key::Named(n) => match n {
                NamedKey::Enter => "enter".into(),
                NamedKey::Tab => "tab".into(),
                NamedKey::Space => "space".into(),
                NamedKey::Escape => "escape".into(),
                NamedKey::Backspace => "backspace".into(),
                NamedKey::Delete => "delete".into(),
                NamedKey::PageUp => "pageup".into(),
                NamedKey::PageDown => "pagedown".into(),
                NamedKey::Home => "home".into(),
                NamedKey::End => "end".into(),
                NamedKey::ArrowUp => "up".into(),
                NamedKey::ArrowDown => "down".into(),
                NamedKey::ArrowLeft => "left".into(),
                NamedKey::ArrowRight => "right".into(),
                NamedKey::F1 => "f1".into(),
                NamedKey::F2 => "f2".into(),
                NamedKey::F3 => "f3".into(),
                NamedKey::F4 => "f4".into(),
                NamedKey::F5 => "f5".into(),
                NamedKey::F6 => "f6".into(),
                NamedKey::F7 => "f7".into(),
                NamedKey::F8 => "f8".into(),
                NamedKey::F9 => "f9".into(),
                NamedKey::F10 => "f10".into(),
                NamedKey::F11 => "f11".into(),
                NamedKey::F12 => "f12".into(),
                _ => return None,
            },
            _ => return None,
        };
        Some(Chord {
            primary,
            shift,
            alt,
            key: name,
        })
    }
}

/// 조합 → 명령 id 표(설정 반영본).
#[derive(Debug, Default)]
pub(crate) struct Keymap {
    map: HashMap<Chord, &'static str>,
    /// 명령별 현재 코드(표시용 · 설정 or 기본).
    codes: HashMap<&'static str, String>,
}

impl Keymap {
    /// 설정에서 조립 — `key.<id>`가 비어 있으면 플랫폼 기본.
    pub(crate) fn from_settings(s: &Settings) -> Self {
        let mut km = Keymap::default();
        for c in COMMANDS {
            let code = s
                .get(&setting_key(c.id))
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map_or_else(|| platform_default(c).to_string(), String::from);
            for part in code.split('|') {
                // `none` = 단축키 없음(비우기).
                if part.trim().eq_ignore_ascii_case("none") {
                    continue;
                }
                if let Some(ch) = Chord::parse(part) {
                    km.map.insert(ch, c.id);
                }
            }
            km.codes.insert(c.id, code);
        }
        km
    }

    pub(crate) fn lookup(&self, ch: &Chord) -> Option<&'static str> {
        self.map.get(ch).copied()
    }

    /// 명령의 현재 코드(표시용 · 여러 개면 첫 것).
    pub(crate) fn code_of(&self, id: &str) -> String {
        self.codes.get(id).cloned().unwrap_or_default()
    }

    /// 표시 문자열(`Ctrl+Shift+P` · 여러 개면 ` / `로).
    pub(crate) fn display_of(&self, id: &str) -> String {
        self.code_of(id)
            .split('|')
            .filter(|p| !p.trim().eq_ignore_ascii_case("none"))
            .filter_map(Chord::parse)
            .map(|c| c.display())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// 이 조합을 이미 쓰는 다른 명령(충돌 표시).
    pub(crate) fn conflict(&self, ch: &Chord, except: &str) -> Option<&'static str> {
        self.lookup(ch).filter(|id| *id != except)
    }
}

/// 명령 라벨(i18n).
pub(crate) fn label_of(id: &str) -> String {
    COMMANDS
        .iter()
        .find(|c| c.id == id)
        .map(|c| t(c.label).to_string())
        .unwrap_or_else(|| id.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_display_and_round_trip() {
        let c = Chord::parse("Ctrl+Shift+P").expect("parse");
        assert!(c.primary && c.shift && !c.alt && c.key == "p");
        assert_eq!(
            Chord::parse("cmd+enter").map(|c| c.key),
            Some("enter".into())
        );
        assert_eq!(
            Chord::parse("f5").map(|c| (c.primary, c.key)),
            Some((false, "f5".into()))
        );
        assert!(Chord::parse("").is_none());
        assert!(Chord::parse("hyper+p").is_none());
        let back = Chord::parse(&c.code()).expect("code round trip");
        assert_eq!(back, c);
        assert!(c.display().to_lowercase().contains('p'));
    }

    #[test]
    fn defaults_are_sublime_and_multi_codes_all_bind() {
        let s = Settings::open(std::path::PathBuf::from("__keymap_test_nonexistent__.conf"));
        let km = Keymap::from_settings(&s);
        let p = Chord::parse(if cfg!(target_os = "macos") {
            "cmd+shift+p"
        } else {
            "ctrl+shift+p"
        })
        .unwrap();
        assert_eq!(km.lookup(&p), Some("view.palette"));
        if !cfg!(target_os = "macos") {
            assert_eq!(
                km.lookup(&Chord::parse("ctrl+shift+z").unwrap()),
                Some("edit.redo"),
                "둘째 코드도 바인딩"
            );
            assert_eq!(
                km.lookup(&Chord::parse("ctrl+pagedown").unwrap()),
                Some("tab.next")
            );
        }
        assert_eq!(km.conflict(&p, "view.palette"), None);
        assert_eq!(km.conflict(&p, "file.new"), Some("view.palette"));
    }
}
