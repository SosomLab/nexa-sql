//! 단축키 맵(사용자 09-15) — **Sublime Text 기본값**(Windows/Linux · macOS) + 설정 재정의(`key.<명령 id>`).
//!
//! - 명령 표([`COMMANDS`])가 단일 원천: id(메뉴·툴바·팔레트와 같은 어휘) · 라벨 · 플랫폼별 기본 코드.
//!   → T-67 Command 레지스트리의 씨앗(메뉴·팔레트·툴바·키맵이 같은 표를 읽는 방향).
//! - 코드 문법 = `ctrl+shift+p` · `cmd+shift+p` · `f5` · `ctrl+enter` · `alt+1`. `ctrl`/`cmd`/`primary`는 모두 **주 조합키**
//!   (Windows/Linux = Ctrl · macOS = ⌘)로 정규화한다. 여러 코드는 `|`로(`ctrl+y|ctrl+shift+z`).
//! - 설정 `key.<id>`(비어 있음 = 플랫폼 기본) · 단축키 캡처 창([`crate::keys_win`])이 쓴다.

use nsql_i18n::{t, Msg};
use nsql_settings::Settings;
use std::collections::{HashMap, HashSet};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// 명령 하나 — 기본 코드는 Sublime Text 관례(없는 것은 `""`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Command {
    pub id: &'static str,
    pub label: Msg,
    pub win: &'static str,
    pub mac: &'static str,
    /// Linux 프리셋(Sublime `Default (Linux).sublime-keymap` — 대부분 Windows와 같다).
    pub linux: &'static str,
}

/// 기본 세트 프리셋(Sublime의 OS별 keymap 파일과 같은 구조 · 설정 `key.preset` = auto|windows|macos|linux · 사용자 09-16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Preset {
    Windows,
    Macos,
    Linux,
}

impl Preset {
    /// 실행 OS의 프리셋.
    pub(crate) fn os() -> Preset {
        if cfg!(target_os = "macos") {
            Preset::Macos
        } else if cfg!(target_os = "linux") {
            Preset::Linux
        } else {
            Preset::Windows
        }
    }

    /// 설정 값(`auto` = OS).
    pub(crate) fn parse(v: &str) -> Preset {
        match v {
            "windows" => Preset::Windows,
            "macos" => Preset::Macos,
            "linux" => Preset::Linux,
            _ => Preset::os(),
        }
    }

    pub(crate) fn from_settings(s: &Settings) -> Preset {
        Preset::parse(s.get("key.preset").unwrap_or("auto"))
    }
}

/// 단축키가 붙는 명령 표(순서 = 캡처 창 목록 순서).
pub(crate) const COMMANDS: &[Command] = &[
    Command {
        id: "view.palette",
        label: Msg::MnCommandPalette,
        win: "ctrl+shift+p",
        mac: "cmd+shift+p",
        linux: "ctrl+shift+p",
    },
    // 새 편집기 = Sublime `ctrl+n` + 브라우저식 `ctrl+t`(사용자 09-15 요청 · 36차 프리셋 정렬 때 `ctrl+t`가 빠졌던 것을 09-16 복구).
    Command {
        id: "file.new",
        label: Msg::MnNew,
        win: "ctrl+n|ctrl+t",
        mac: "cmd+n|cmd+t",
        linux: "ctrl+n|ctrl+t",
    },
    Command {
        id: "file.open",
        label: Msg::MnOpen,
        win: "ctrl+o",
        mac: "cmd+o",
        linux: "ctrl+o",
    },
    Command {
        id: "file.save",
        label: Msg::MnSave,
        win: "ctrl+s",
        mac: "cmd+s",
        linux: "ctrl+s",
    },
    Command {
        id: "file.save_as",
        label: Msg::MnSaveAs,
        win: "ctrl+shift+s",
        mac: "cmd+shift+s",
        linux: "ctrl+shift+s",
    },
    Command {
        id: "file.close_tab",
        label: Msg::MnCloseTab,
        win: "ctrl+w",
        mac: "cmd+w",
        linux: "ctrl+w",
    },
    Command {
        id: "tab.next",
        label: Msg::MnNextTab,
        win: "ctrl+tab|ctrl+pagedown",
        mac: "ctrl+tab|cmd+alt+right|cmd+shift+]",
        linux: "ctrl+tab|ctrl+pagedown",
    },
    Command {
        id: "tab.prev",
        label: Msg::MnPrevTab,
        win: "ctrl+shift+tab|ctrl+pageup",
        mac: "ctrl+shift+tab|cmd+alt+left|cmd+shift+[",
        linux: "ctrl+shift+tab|ctrl+pageup",
    },
    Command {
        id: "run.statement",
        label: Msg::MnRunStatement,
        win: "ctrl+enter",
        mac: "cmd+enter",
        linux: "ctrl+enter",
    },
    // 결과 탭(T-93 · docs/43 §4-3): 새 결과 탭에 실행 · 결과 탭 닫기/이동.
    Command {
        id: "run.statement_new_tab",
        label: Msg::MnRunStatementNewTab,
        win: "ctrl+\\",
        mac: "cmd+\\",
        linux: "ctrl+\\",
    },
    Command {
        id: "result.tab.close",
        label: Msg::MnResultCloseTab,
        win: "ctrl+shift+w",
        mac: "cmd+shift+w",
        linux: "ctrl+shift+w",
    },
    Command {
        id: "result.tab.next",
        label: Msg::MnResultNextTab,
        win: "ctrl+alt+right",
        mac: "control+alt+right",
        linux: "ctrl+alt+right",
    },
    Command {
        id: "result.tab.prev",
        label: Msg::MnResultPrevTab,
        win: "ctrl+alt+left",
        mac: "control+alt+left",
        linux: "ctrl+alt+left",
    },
    Command {
        id: "run.all",
        label: Msg::MnRunAll,
        win: "f5",
        mac: "f5",
        linux: "f5",
    },
    Command {
        id: "run.explain",
        label: Msg::MnExplain,
        win: "ctrl+shift+x",
        mac: "cmd+shift+x",
        linux: "ctrl+shift+x",
    },
    Command {
        id: "run.commit",
        label: Msg::MnCommit,
        win: "ctrl+alt+c",
        mac: "cmd+alt+c",
        linux: "ctrl+alt+c",
    },
    Command {
        id: "run.rollback",
        label: Msg::MnRollback,
        win: "ctrl+alt+r",
        mac: "cmd+alt+r",
        linux: "ctrl+alt+r",
    },
    Command {
        id: "conn.toggle",
        label: Msg::MnConnect,
        // ⌘⇧C는 nexa-clip 전역 단축키와 충돌(사용자 09-17) → DBeaver "새 접속"과 같은 Ctrl/⌘+Shift+N.
        win: "ctrl+shift+n",
        mac: "cmd+shift+n",
        linux: "ctrl+shift+n",
    },
    Command {
        id: "view.log",
        label: Msg::MnLogWindow,
        win: "f10",
        mac: "f10",
        linux: "f10",
    },
    Command {
        id: "view.theme",
        label: Msg::MnTheme,
        win: "ctrl+alt+t",
        mac: "cmd+alt+t",
        linux: "ctrl+alt+t",
    },
    Command {
        id: "view.lang",
        label: Msg::MnLanguage,
        win: "ctrl+alt+l",
        mac: "cmd+alt+l",
        linux: "ctrl+alt+l",
    },
    Command {
        id: "view.colors",
        label: Msg::MnColors,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "view.keys",
        label: Msg::MnKeys,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "view.variables",
        label: Msg::MnVariables,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "view.search",
        label: Msg::MnSearchPanel,
        win: "ctrl+shift+f",
        mac: "cmd+shift+f",
        linux: "ctrl+shift+f",
    },
    Command {
        id: "view.explorer",
        label: Msg::MnExplorer,
        win: "ctrl+shift+e",
        mac: "cmd+shift+e",
        linux: "ctrl+shift+e",
    },
    // ★ 다음/이전 문장(`;` 기준 · 사용자 09-16) — Alt+↓/↑: Sublime 기본 맵에서 비어 있고(Ctrl+Shift+↑/↓ = 줄 교체 ·
    //   Ctrl+Alt+↑/↓ = 커서 추가) 방향키라 "이동"으로 읽힌다.
    Command {
        id: "edit.next_statement",
        label: Msg::MnNextStatement,
        win: "alt+down",
        mac: "alt+down",
        linux: "alt+down",
    },
    Command {
        id: "edit.prev_statement",
        label: Msg::MnPrevStatement,
        win: "alt+up",
        mac: "alt+up",
        linux: "alt+up",
    },
    Command {
        id: "edit.expand_selection",
        label: Msg::MnExpandSelection,
        win: "ctrl+d",
        mac: "cmd+d",
        linux: "ctrl+d",
    },
    Command {
        id: "edit.goto_bracket",
        label: Msg::MnGotoBracket,
        win: "ctrl+m",
        mac: "ctrl+m",
        linux: "ctrl+m",
    },
    Command {
        id: "edit.expand_brackets",
        label: Msg::MnExpandBrackets,
        win: "ctrl+shift+m",
        mac: "ctrl+shift+m",
        linux: "ctrl+shift+m",
    },
    // Rainbow Pairs 확장(docs/51 · D-93): 형제 , . · 상위 [ · 하위 ].
    Command {
        id: "edit.bracket_prev",
        label: Msg::MnBracketPrev,
        win: "ctrl+alt+,",
        mac: "ctrl+alt+,",
        linux: "ctrl+alt+,",
    },
    Command {
        id: "edit.bracket_next",
        label: Msg::MnBracketNext,
        win: "ctrl+alt+.",
        mac: "ctrl+alt+.",
        linux: "ctrl+alt+.",
    },
    Command {
        id: "edit.bracket_parent",
        label: Msg::MnBracketParent,
        win: "ctrl+alt+[",
        mac: "ctrl+alt+[",
        linux: "ctrl+alt+[",
    },
    Command {
        id: "edit.bracket_child",
        label: Msg::MnBracketChild,
        win: "ctrl+alt+]",
        mac: "ctrl+alt+]",
        linux: "ctrl+alt+]",
    },
    // Extension Manager(docs/50 §10 · 팔레트 전용 · 단축키 없음).
    Command {
        id: "ext.enable_mgr",
        label: Msg::MnExtEnableManager,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "file.run_file",
        label: Msg::MnRunFile,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "file.large_force",
        label: Msg::MnLargeForce,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "mem.trim_now",
        label: Msg::MnMemTrimNow,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.disable_mgr",
        label: Msg::MnExtDisableManager,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "view.extensions",
        label: Msg::MnExtensionsPanel,
        win: "ctrl+shift+x",
        mac: "cmd+shift+x",
        linux: "ctrl+shift+x",
    },
    Command {
        id: "ext.install",
        label: Msg::MnExtInstall,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.remove",
        label: Msg::MnExtRemove,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.list",
        label: Msg::MnExtList,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.enable",
        label: Msg::MnExtEnable,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.disable",
        label: Msg::MnExtDisable,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.repo_add",
        label: Msg::MnExtRepoAdd,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.repo_list",
        label: Msg::MnExtRepoList,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "ext.repo_remove",
        label: Msg::MnExtRepoRemove,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "edit.select_all_occurrences",
        label: Msg::MnSelectAllOccurrences,
        win: "alt+f3",
        mac: "ctrl+cmd+g",
        linux: "alt+f3",
    },
    // ★ Sublime 줄·선택 편집(T-98 · 사용자 09-16). 2단 코드는 `ctrl+k,ctrl+u`처럼 쉼표로.
    //   macOS의 Control 단독은 `control+…`(⌘와 다른 키 · 예 ⌃G = 줄 이동).
    Command {
        id: "edit.duplicate_line",
        label: Msg::MnDuplicateLine,
        win: "ctrl+shift+d",
        mac: "cmd+shift+d",
        linux: "ctrl+shift+d",
    },
    Command {
        id: "edit.delete_line",
        label: Msg::MnDeleteLine,
        win: "ctrl+shift+k",
        mac: "control+shift+k",
        linux: "ctrl+shift+k",
    },
    Command {
        id: "edit.join_lines",
        label: Msg::MnJoinLines,
        win: "ctrl+j",
        mac: "cmd+j",
        linux: "ctrl+j",
    },
    Command {
        id: "edit.swap_line_up",
        label: Msg::MnSwapLineUp,
        win: "ctrl+shift+up",
        mac: "ctrl+cmd+up",
        linux: "ctrl+shift+up",
    },
    Command {
        id: "edit.swap_line_down",
        label: Msg::MnSwapLineDown,
        win: "ctrl+shift+down",
        mac: "ctrl+cmd+down",
        linux: "ctrl+shift+down",
    },
    Command {
        id: "edit.toggle_comment",
        label: Msg::MnToggleComment,
        win: "ctrl+/",
        mac: "cmd+/",
        linux: "ctrl+/",
    },
    Command {
        id: "edit.indent",
        label: Msg::MnIndent,
        win: "ctrl+]",
        mac: "cmd+]",
        linux: "ctrl+]",
    },
    Command {
        id: "edit.unindent",
        label: Msg::MnUnindent,
        win: "ctrl+[|shift+tab",
        mac: "cmd+[|shift+tab",
        linux: "ctrl+[|shift+tab",
    },
    Command {
        id: "edit.select_line",
        label: Msg::MnSelectLine,
        win: "ctrl+l",
        mac: "cmd+l",
        linux: "ctrl+l",
    },
    Command {
        id: "edit.split_lines",
        label: Msg::MnSplitLines,
        win: "ctrl+shift+l",
        mac: "cmd+shift+l",
        linux: "ctrl+shift+l",
    },
    Command {
        id: "edit.add_caret_up",
        label: Msg::MnAddCaretUp,
        win: "ctrl+alt+up",
        mac: "control+shift+up",
        linux: "ctrl+alt+up",
    },
    Command {
        id: "edit.add_caret_down",
        label: Msg::MnAddCaretDown,
        win: "ctrl+alt+down",
        mac: "control+shift+down",
        linux: "ctrl+alt+down",
    },
    Command {
        id: "edit.upper_case",
        label: Msg::MnUpperCase,
        win: "ctrl+k,ctrl+u",
        mac: "cmd+k,cmd+u",
        linux: "ctrl+k,ctrl+u",
    },
    Command {
        id: "edit.lower_case",
        label: Msg::MnLowerCase,
        win: "ctrl+k,ctrl+l",
        mac: "cmd+k,cmd+l",
        linux: "ctrl+k,ctrl+l",
    },
    Command {
        id: "edit.goto_line",
        label: Msg::MnGotoLine,
        win: "ctrl+g",
        mac: "control+g",
        linux: "ctrl+g",
    },
    // Goto Anything(탭 검색 · 최근 파일 · `:줄` · T-96).
    Command {
        id: "view.goto_anything",
        label: Msg::MnGotoAnything,
        win: "ctrl+p",
        mac: "cmd+p",
        linux: "ctrl+p",
    },
    // 찾기 패널 안 단축키(Sublime Text 기준 · 패널에 포커스일 때만 동작 · 09-16).
    Command {
        id: "find.case",
        label: Msg::MnFindCase,
        win: "alt+c",
        mac: "alt+c",
        linux: "alt+c",
    },
    Command {
        id: "find.word",
        label: Msg::MnFindWord,
        win: "alt+w",
        mac: "alt+w",
        linux: "alt+w",
    },
    Command {
        id: "find.regex",
        label: Msg::MnFindRegex,
        win: "alt+r",
        mac: "alt+r",
        linux: "alt+r",
    },
    Command {
        id: "find.selection",
        label: Msg::MnFindSelection,
        win: "alt+l",
        mac: "alt+l",
        linux: "alt+l",
    },
    Command {
        id: "find.preserve",
        label: Msg::MnFindPreserve,
        win: "alt+a",
        mac: "alt+a",
        linux: "alt+a",
    },
    Command {
        id: "find.replace_one",
        label: Msg::MnFindReplaceOne,
        win: "ctrl+shift+h",
        mac: "cmd+alt+e",
        linux: "ctrl+shift+h",
    },
    Command {
        id: "find.replace_all",
        label: Msg::MnFindReplaceAll,
        win: "ctrl+alt+enter",
        mac: "cmd+alt+enter",
        linux: "ctrl+alt+enter",
    },
    Command {
        id: "find.all",
        label: Msg::MnFindAll,
        win: "alt+enter",
        mac: "alt+enter",
        linux: "alt+enter",
    },
    Command {
        id: "edit.prefs",
        label: Msg::MnPreferences,
        win: "ctrl+,",
        mac: "cmd+,",
        linux: "ctrl+,",
    },
    Command {
        id: "conn.disconnect",
        label: Msg::TipDisconnect,
        win: "",
        mac: "",
        linux: "",
    },
    Command {
        id: "edit.find",
        label: Msg::MnFind,
        win: "ctrl+f",
        mac: "cmd+f",
        linux: "ctrl+f",
    },
    Command {
        id: "edit.replace",
        label: Msg::MnReplace,
        win: "ctrl+h",
        mac: "cmd+alt+f",
        linux: "ctrl+h",
    },
    Command {
        id: "edit.find_next",
        label: Msg::MnFindNext,
        win: "f3",
        mac: "cmd+g",
        linux: "f3",
    },
    Command {
        id: "edit.find_prev",
        label: Msg::MnFindPrev,
        win: "shift+f3",
        mac: "cmd+shift+g",
        linux: "shift+f3",
    },
    Command {
        id: "edit.undo",
        label: Msg::MnUndo,
        win: "ctrl+z",
        mac: "cmd+z",
        linux: "ctrl+z",
    },
    Command {
        id: "edit.redo",
        label: Msg::MnRedo,
        win: "ctrl+y|ctrl+shift+z",
        mac: "cmd+shift+z",
        linux: "ctrl+y|ctrl+shift+z",
    },
    Command {
        id: "edit.cut",
        label: Msg::MnCut,
        win: "ctrl+x",
        mac: "cmd+x",
        linux: "ctrl+x",
    },
    Command {
        id: "edit.copy",
        label: Msg::MnCopy,
        win: "ctrl+c",
        mac: "cmd+c",
        linux: "ctrl+c",
    },
    Command {
        id: "edit.paste",
        label: Msg::MnPaste,
        win: "ctrl+v",
        mac: "cmd+v",
        linux: "ctrl+v",
    },
    Command {
        id: "edit.select_all",
        label: Msg::MnSelectAll,
        win: "ctrl+a",
        mac: "cmd+a",
        linux: "ctrl+a",
    },
];

/// 설정 키(`key.<id>`).
pub(crate) fn setting_key(id: &str) -> String {
    format!("key.{id}")
}

/// 프리셋별 기본 코드.
pub(crate) fn preset_default(c: &Command, p: Preset) -> &'static str {
    match p {
        Preset::Windows => c.win,
        Preset::Macos => c.mac,
        Preset::Linux => c.linux,
    }
}

/// 정규화된 조합(대소문자 무관 · 주 조합키 통일).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Chord {
    pub primary: bool,
    pub shift: bool,
    pub alt: bool,
    /// macOS **Control**(⌘와 별개 · Sublime `ctrl+cmd+g`). 코드에 `cmd`와 `ctrl`이 함께 있을 때만 참.
    pub ctrl: bool,
    /// 소문자 글자 또는 이름(`enter` `tab` `f5` `pageup` …).
    pub key: String,
}

/// 물리 키 → 키맵 이름(US 배열 위치 · 글자·숫자·문장부호만). IME 모드에서 논리 키가 비ASCII일 때의 폴백.
fn physical_name(p: &PhysicalKey) -> Option<&'static str> {
    let PhysicalKey::Code(code) = p else {
        return None;
    };
    Some(match code {
        KeyCode::KeyA => "a",
        KeyCode::KeyB => "b",
        KeyCode::KeyC => "c",
        KeyCode::KeyD => "d",
        KeyCode::KeyE => "e",
        KeyCode::KeyF => "f",
        KeyCode::KeyG => "g",
        KeyCode::KeyH => "h",
        KeyCode::KeyI => "i",
        KeyCode::KeyJ => "j",
        KeyCode::KeyK => "k",
        KeyCode::KeyL => "l",
        KeyCode::KeyM => "m",
        KeyCode::KeyN => "n",
        KeyCode::KeyO => "o",
        KeyCode::KeyP => "p",
        KeyCode::KeyQ => "q",
        KeyCode::KeyR => "r",
        KeyCode::KeyS => "s",
        KeyCode::KeyT => "t",
        KeyCode::KeyU => "u",
        KeyCode::KeyV => "v",
        KeyCode::KeyW => "w",
        KeyCode::KeyX => "x",
        KeyCode::KeyY => "y",
        KeyCode::KeyZ => "z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Backslash => "\\",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::Backquote => "`",
        _ => return None,
    })
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
            ctrl: false,
            key: String::new(),
        };
        let toks: Vec<&str> = s.split('+').map(str::trim).collect();
        let (mods, key) = toks.split_at(toks.len().saturating_sub(1));
        let mut saw_ctrl = false;
        let mut saw_cmd = false;
        let mut saw_control = false;
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" => saw_ctrl = true,
                // `control` = macOS Control **단독**(⌘ 없이 · 예 ⌃G · 09-16). 다른 OS에선 오지 않는 조합.
                "control" => saw_control = true,
                "cmd" | "command" | "super" | "meta" | "primary" | "win" => saw_cmd = true,
                "shift" => c.shift = true,
                "alt" | "option" | "opt" => c.alt = true,
                _ => return None,
            }
        }
        // `ctrl`만 = 주 조합키(Windows/Linux 코드) · `cmd`와 함께면 macOS Control(⌘와 별개) · `control` = Control 단독.
        c.primary = saw_cmd || saw_ctrl;
        c.ctrl = (saw_cmd && saw_ctrl) || saw_control;
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
        if self.ctrl {
            s.push_str(if mac { "⌃" } else { "Ctrl+" });
        }
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
        if self.ctrl && !self.primary {
            s.push_str("control+");
        } else if self.ctrl {
            s.push_str("ctrl+");
        }
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
    ///
    /// ★ 논리 키가 ASCII가 아니면(한글·일본어 등 IME 모드 — 맥에서 ⌘+T가 `Character("ㅅ")`로 온다 · 사용자 09-16)
    /// **물리 키**(US 배열 위치 · [`physical_name`])로 이름을 정한다 → Windows처럼 IME 모드와 무관하게 `cmd+t`.
    pub(crate) fn from_winit(
        key: &Key,
        physical: &PhysicalKey,
        primary: bool,
        shift: bool,
        alt: bool,
        ctrl: bool,
    ) -> Option<Chord> {
        let name = match key {
            Key::Character(t) => {
                let c = t.chars().next()?;
                if c.is_control() || c.is_whitespace() {
                    return None;
                }
                if c.is_ascii() {
                    c.to_ascii_lowercase().to_string()
                } else if let Some(n) = physical_name(physical) {
                    n.to_string()
                } else {
                    c.to_lowercase().to_string()
                }
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
            // macOS Control은 ⌘와 함께(`ctrl+cmd+g`)든 단독(`control+g`)이든 그대로(09-16 · 다른 OS는 늘 false).
            ctrl,
            key: name,
        })
    }
}

/// 조합 → 명령 id 표(설정 반영본).
#[derive(Debug, Default)]
pub(crate) struct Keymap {
    map: HashMap<Chord, &'static str>,
    /// 2단 코드(`ctrl+k,ctrl+u` · Sublime의 키 시퀀스) — (첫 조합, 둘째 조합) → 명령.
    seq: HashMap<(Chord, Chord), &'static str>,
    /// 2단 코드의 첫 조합들(누르면 다음 키를 기다린다).
    prefixes: HashSet<Chord>,
    /// 명령별 현재 코드(표시용 · 설정 or 기본).
    codes: HashMap<&'static str, String>,
}

/// 2단 코드 분리 — 쉼표 앞 글자가 `+`이면 그 쉼표는 **키**(`cmd+,` · `ctrl+alt+,`)라 자르지 않는다(사용자 09-17 ⌘, 결함).
fn split_seq(code: &str) -> Option<(&str, &str)> {
    let b = code.as_bytes();
    (0..b.len())
        .filter(|&i| b[i] == b',' && i > 0 && b[i - 1] != b'+')
        .map(|i| (&code[..i], &code[i + 1..]))
        .next()
}

impl Keymap {
    /// 설정에서 조립 — `key.<id>`가 비어 있으면 플랫폼 기본.
    pub(crate) fn from_settings(s: &Settings) -> Self {
        let preset = Preset::from_settings(s);
        let mut km = Keymap::default();
        for c in COMMANDS {
            let code = s
                .get(&setting_key(c.id))
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map_or_else(|| preset_default(c, preset).to_string(), String::from);
            for part in code.split('|') {
                // `none` = 단축키 없음(비우기).
                if part.trim().eq_ignore_ascii_case("none") {
                    continue;
                }
                if let Some((a, b)) = split_seq(part) {
                    if let (Some(a), Some(b)) = (Chord::parse(a), Chord::parse(b)) {
                        km.prefixes.insert(a.clone());
                        km.seq.insert((a, b), c.id);
                    }
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

    /// 이 조합이 2단 코드의 첫 키인가(호스트가 다음 키를 기다린다).
    pub(crate) fn is_prefix(&self, ch: &Chord) -> bool {
        self.prefixes.contains(ch)
    }

    /// 2단 코드 조회.
    pub(crate) fn lookup_seq(&self, first: &Chord, second: &Chord) -> Option<&'static str> {
        self.seq.get(&(first.clone(), second.clone())).copied()
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
            .filter_map(|p| {
                // 2단 코드는 `Ctrl+K, Ctrl+U`.
                let parts: Vec<String> = p
                    .split(',')
                    .filter_map(Chord::parse)
                    .map(|c| c.display())
                    .collect();
                (!parts.is_empty() && parts.len() == p.split(',').count()).then(|| parts.join(", "))
            })
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

/// 키를 **누르고 있을 때의 자동 반복**을 그대로 실행해도 되는 명령인가(사용자 09-17: Ctrl+T를 누르고 있자 탭 80개 —
/// 반복 사건이 전부 명령이 되고, 하나하나가 느려 릴리스 뒤에도 밀린 만큼 계속 만들어졌다).
/// 허용 = 편집·이동·되돌리기(`edit.*`) · 찾기 이동(`find.*`) · 탭 넘기기 · 문장 이동. 그 밖의 한 번짜리 동작(새 탭 · 닫기 · 실행 ·
/// 커밋/롤백 · 창 열기 · 접속)은 첫 사건만 받는다 — 밀린 반복 사건은 즉시 버려지므로 UI가 막히지 않는다.
pub(crate) fn repeatable(id: &str) -> bool {
    id.starts_with("edit.")
        || id.starts_with("find.")
        || matches!(
            id,
            "tab.next" | "tab.prev" | "run.next_statement" | "run.prev_statement"
        )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// 자동 반복 허용 표(사용자 09-17 Ctrl+T 80개): 편집·이동·찾기·탭 이동만 · 새 탭/닫기/실행/커밋/창 열기는 한 번만.
    #[test]
    fn auto_repeat_only_for_editing_and_navigation() {
        for id in [
            "edit.undo",
            "edit.redo",
            "edit.delete_line",
            "find.next",
            "tab.next",
            "tab.prev",
        ] {
            assert!(super::repeatable(id), "{id}");
        }
        for id in [
            "file.new",
            "file.close_tab",
            "run.statement",
            "run.all",
            "run.commit",
            "run.rollback",
            "view.log",
            "view.palette",
            "conn.toggle",
            "conn.disconnect",
            "file.save",
        ] {
            assert!(!super::repeatable(id), "{id}");
        }
    }

    /// 한글 IME: ⌘+T가 `Character("ㅅ")` + 물리 KeyT → `cmd+t`(사용자 09-16). ASCII면 논리 키 우선.
    #[test]
    fn ime_hangul_uses_physical_key() {
        use winit::keyboard::SmolStr;
        let ch = |k: &str, code: KeyCode| {
            Chord::from_winit(
                &Key::Character(SmolStr::new(k)),
                &PhysicalKey::Code(code),
                true,
                false,
                false,
                false,
            )
            .expect("chord")
            .key
        };
        assert_eq!(ch("ㅅ", KeyCode::KeyT), "t");
        assert_eq!(ch("T", KeyCode::KeyY), "t", "ASCII는 논리 키 우선");
        assert_eq!(ch("、", KeyCode::Comma), ",");
    }

    /// 2단 코드(`ctrl+k,ctrl+u`)와 macOS Control 단독(`control+g`) — 09-16.
    #[test]
    fn two_key_sequences_and_mac_control_only() {
        let s = Settings::open(std::path::PathBuf::from(
            "__keymap_test_nonexistent2__.conf",
        ));
        let km = Keymap::from_settings(&s);
        let (k, u) = if cfg!(target_os = "macos") {
            ("cmd+k", "cmd+u")
        } else {
            ("ctrl+k", "ctrl+u")
        };
        let (k, u) = (Chord::parse(k).unwrap(), Chord::parse(u).unwrap());
        assert!(km.is_prefix(&k));
        assert_eq!(km.lookup_seq(&k, &u), Some("edit.upper_case"));
        assert_eq!(km.lookup(&k), None, "첫 키 단독은 명령이 아니다");
        assert!(km.display_of("edit.upper_case").contains(", "));
        let g = Chord::parse("control+g").unwrap();
        assert!(g.ctrl && !g.primary, "Control 단독");
        assert_eq!(Chord::parse(&g.code()), Some(g.clone()), "코드 왕복");
        assert_ne!(
            g,
            Chord::parse("ctrl+g").unwrap(),
            "Windows ctrl = 주 조합키와 다르다"
        );
    }

    /// ⌘T/Ctrl+T = 새 편집기(사용자 09-15 · 36차에 유실 · 09-16 회귀 테스트).
    #[test]
    fn ctrl_t_opens_new_editor_on_every_preset() {
        let s = Settings::open(std::path::PathBuf::from(
            "__keymap_test_nonexistent3__.conf",
        ));
        let km = Keymap::from_settings(&s);
        let code = if cfg!(target_os = "macos") {
            "cmd+t"
        } else {
            "ctrl+t"
        };
        assert_eq!(km.lookup(&Chord::parse(code).unwrap()), Some("file.new"));
        for p in [Preset::Windows, Preset::Macos, Preset::Linux] {
            let c = COMMANDS.iter().find(|c| c.id == "file.new").unwrap();
            assert!(preset_default(c, p).ends_with("+t"), "{p:?}");
        }
    }

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

    /// ⌘,/Ctrl+, = 설정 창(사용자 09-17 확인).
    #[test]
    fn comma_opens_preferences() {
        let s = Settings::open(std::path::PathBuf::from("__keymap_test_nonexistent__.conf"));
        let km = Keymap::from_settings(&s);
        let code = if cfg!(target_os = "macos") {
            "cmd+,"
        } else {
            "ctrl+,"
        };
        assert_eq!(
            km.lookup(&Chord::parse(code).expect("parse")),
            Some("edit.prefs")
        );
        // 쉼표 키 vs 2단 코드 구분.
        assert_eq!(split_seq("cmd+,"), None);
        assert_eq!(split_seq("ctrl+alt+,"), None);
        assert_eq!(split_seq("ctrl+k,ctrl+u"), Some(("ctrl+k", "ctrl+u")));
        assert_eq!(split_seq("ctrl+k,ctrl+,"), Some(("ctrl+k", "ctrl+,")));
        assert_eq!(
            km.lookup(&Chord::parse("ctrl+alt+,").expect("parse")),
            Some("edit.bracket_prev")
        );
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

    #[test]
    fn presets_and_mac_control_chord() {
        // 프리셋은 OS와 무관하게 고를 수 있다(설정 `key.preset`).
        let c = COMMANDS
            .iter()
            .find(|c| c.id == "edit.select_all_occurrences")
            .unwrap();
        assert_eq!(preset_default(c, Preset::Windows), "alt+f3");
        assert_eq!(preset_default(c, Preset::Linux), "alt+f3");
        assert_eq!(preset_default(c, Preset::Macos), "ctrl+cmd+g");
        // `ctrl+cmd+g`(맥 Control + ⌘)는 `cmd+g`와 다른 조합.
        let a = Chord::parse("ctrl+cmd+g").unwrap();
        let b = Chord::parse("cmd+g").unwrap();
        assert!(a.ctrl && a.primary && !b.ctrl);
        assert_ne!(a, b);
        assert_eq!(
            a.code(),
            if cfg!(target_os = "macos") {
                "ctrl+cmd+g"
            } else {
                "ctrl+ctrl+g"
            }
            .replace("ctrl+ctrl+", "ctrl+cmd+")
            .replace(
                "ctrl+cmd+",
                if cfg!(target_os = "macos") {
                    "ctrl+cmd+"
                } else {
                    "ctrl+ctrl+"
                }
            )
        );
        // `ctrl`만 = 주 조합키(Windows 코드).
        let w = Chord::parse("ctrl+shift+p").unwrap();
        assert!(w.primary && !w.ctrl);
    }
}
