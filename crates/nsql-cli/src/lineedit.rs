//! `nsql shell` 줄 편집기(T-453 · D-393 · 외부 crate 0 = DR-3) — 키 디코더 · 편집 상태 · 그리기 · 기록 · raw 모드 · 읽기 루프.
//!
//! 순수 부분(디코더 [`Decoder`] · 편집 [`LineEditor`] · 그리기 [`render`] · 후보 표 [`format_columns`])은 시험으로 덮고,
//! 터미널 I/O는 [`read_interactive`] 한 곳. 터미널이 아니면(배치 · 파이프) 호출자가 종전 `read_line`을 쓴다.
//! 키 = 좌우·Home/End·Backspace/Delete·↑↓ 기록·Tab 완성·Ctrl+A/E/B/F/U/K/W/L·Ctrl+C(줄 취소)·Ctrl+D(빈 줄 = 끝)·Ctrl+R(기록 찾기)
//! · 단어 = Alt+B/F(`ESC b`/`ESC f`) · Ctrl/Alt+←/→(`ESC [ 1;5D` 등) · Alt+D(앞쪽 단어 지우기) · Alt+Backspace(뒤쪽 단어 지우기)
//! · 되돌리기 = Ctrl+Z(·Ctrl+_) · 다시 = Ctrl+Y(GUI `edit.undo`/`edit.redo`와 같은 키 · 사용자 10-29) · 붙이기(yank) = Alt+Y(마지막으로 지운 글).
//!   되돌리기 단위 = 이어 친 글자 묶음 · 이어 지운 글자 묶음 · 그 밖 편집(지우기·완성·기록·붙이기)은 하나씩 · Enter·Ctrl+C로 비움.
//!   Windows는 콘솔을 직접 읽는다(`term::read_console_utf8` · 표준 `Stdin`이 Ctrl+Z를 EOF로 바꾸기 때문).
//! · 다중 줄(T-453 잔여 · 10-30) = 본문에 `\n`이 있을 수 있다(기록에서 불러온 여러 줄 문장 · Alt+Enter = 줄 바꿈 삽입) · 그리기는 줄마다
//!   `   -> ` 이음 프롬프트 · ↑↓ = 본문 안에서는 줄 이동, 첫/끝 줄에서는 기록 · Home/End = 그 줄 안 · 기록 한 항목 = 문장 하나(셸이
//!   이어지는 줄을 `amend_last_history`로 붙인다 · 파일은 `\u{1}` 접기 그대로).
//! 키 진단 = `NSQL_KEY_TRACE=<파일>`이면 받은 바이트와 해석한 키를 그 파일에 덧붙인다(원격·맥 Option 판정용).
//! 그리기 = ANSI(`\r` · `ESC[K` · `ESC[J` · 커서 이동) — Windows는 raw 모드에서 VT 처리를 켠다(Windows 10+).

#[cfg(not(windows))]
use std::io::Read;
use std::io::{self, IsTerminal, Write};

/// 디코더가 낸 키.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Char(char),
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    Up,
    Down,
    Tab,
    Enter,
    /// Ctrl+C — 지금 줄을 버린다.
    Cancel,
    /// Ctrl+D — 빈 줄이면 끝(EOF) · 아니면 Delete.
    Eof,
    /// Ctrl+L.
    ClearScreen,
    /// Ctrl+U.
    KillToStart,
    /// Ctrl+K.
    KillToEnd,
    /// Ctrl+W.
    KillWord,
    /// Ctrl+R — 기록 거꾸로 찾기(검색 모드 · 반복 = 더 오래된 일치).
    SearchHistory,
    /// Alt+B · Ctrl/Alt+← — 단어 처음으로.
    WordLeft,
    /// Alt+F · Ctrl/Alt+→ — 다음 단어 끝으로.
    WordRight,
    /// Alt+Backspace — 커서 앞 단어 지우기(단어 경계 = 영숫자·`_`·비ASCII).
    KillWordLeft,
    /// Alt+D — 커서 뒤 단어 지우기.
    KillWordRight,
    /// Ctrl+Z · Ctrl+_ — 되돌리기.
    Undo,
    /// Ctrl+Y — 다시 실행.
    Redo,
    /// Alt+Y — 마지막으로 지운 글 붙이기(yank).
    Yank,
    /// Alt+Enter — 줄 바꿈 삽입(여러 줄 문장 편집).
    NewLine,
    /// 모르는 제어·이스케이프 = 버린다.
    Ignore,
}

/// 바이트 → 키(UTF-8 다중 바이트 · `ESC [ …` 시퀀스 누적).
#[derive(Default)]
pub(crate) struct Decoder {
    esc: Vec<u8>,
    utf8: Vec<u8>,
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 0,
    }
}

impl Decoder {
    /// 바이트 하나를 넣고 키가 완성되면 돌려준다.
    pub(crate) fn push(&mut self, b: u8) -> Option<Key> {
        if !self.esc.is_empty() {
            self.esc.push(b);
            return self.finish_esc();
        }
        if !self.utf8.is_empty() {
            self.utf8.push(b);
            let need = utf8_len(self.utf8[0]);
            if self.utf8.len() < need {
                return None;
            }
            let c = std::str::from_utf8(&self.utf8)
                .ok()
                .and_then(|s| s.chars().next());
            self.utf8.clear();
            return Some(c.map_or(Key::Ignore, Key::Char));
        }
        Some(match b {
            0x1b => {
                self.esc.push(b);
                return None;
            }
            b'\r' | b'\n' => Key::Enter,
            0x7f | 0x08 => Key::Backspace,
            b'\t' => Key::Tab,
            0x03 => Key::Cancel,
            0x04 => Key::Eof,
            0x01 => Key::Home,
            0x05 => Key::End,
            0x02 => Key::Left,
            0x06 => Key::Right,
            0x0e => Key::Down,
            0x10 => Key::Up,
            0x0c => Key::ClearScreen,
            0x0b => Key::KillToEnd,
            0x15 => Key::KillToStart,
            0x17 => Key::KillWord,
            0x12 => Key::SearchHistory,
            0x1a | 0x1f => Key::Undo,
            0x19 => Key::Redo,
            b if b < 0x20 => Key::Ignore,
            b if b < 0x80 => Key::Char(b as char),
            b => {
                if utf8_len(b) < 2 {
                    return Some(Key::Ignore);
                }
                self.utf8.push(b);
                return None;
            }
        })
    }

    /// `ESC …` — `[`/`O` 뒤 종료 글자(알파벳 · `~`)까지 모은다. ESC 뒤 한 글자 = Alt 조합(`b`/`f`/`d`/Backspace만 · 나머지 버림).
    fn finish_esc(&mut self) -> Option<Key> {
        let key = match self.esc.as_slice() {
            [0x1b, b'['] | [0x1b, b'O'] => return None,
            [0x1b, b'b' | b'B'] => Key::WordLeft,
            [0x1b, b'f' | b'F'] => Key::WordRight,
            [0x1b, b'd' | b'D'] => Key::KillWordRight,
            [0x1b, b'y' | b'Y'] => Key::Yank,
            [0x1b, b'\r' | b'\n'] => Key::NewLine,
            [0x1b, 0x7f | 0x08] => Key::KillWordLeft,
            [0x1b, b'[' | b'O', rest @ ..] => {
                let &last = rest.last()?;
                if !(last.is_ascii_alphabetic() || last == b'~') {
                    if rest.len() > 12 {
                        self.esc.clear();
                        return Some(Key::Ignore);
                    }
                    return None;
                }
                let params = &rest[..rest.len() - 1];
                let num = |i: usize| -> u32 {
                    params
                        .split(|c| *c == b';')
                        .nth(i)
                        .and_then(|p| std::str::from_utf8(p).ok())
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(0)
                };
                let first_num = num(0);
                // xterm 수식 키 = 둘째 매개변수 1+비트(1 Shift · 2 Alt · 4 Ctrl) — Alt나 Ctrl이 섞이면 단어 이동.
                let word = (num(1).saturating_sub(1) & 0b110) != 0;
                match (last, first_num) {
                    (b'A', _) => Key::Up,
                    (b'B', _) => Key::Down,
                    (b'C', _) if word => Key::WordRight,
                    (b'D', _) if word => Key::WordLeft,
                    (b'C', _) => Key::Right,
                    (b'D', _) => Key::Left,
                    (b'H', _) | (b'~', 1 | 7) => Key::Home,
                    (b'F', _) | (b'~', 4 | 8) => Key::End,
                    (b'~', 3) => Key::Delete,
                    _ => Key::Ignore,
                }
            }
            _ => Key::Ignore,
        };
        self.esc.clear();
        Some(key)
    }
}

/// 단어 글자 = 영숫자 · `_` · 비ASCII(한글 등). 그 밖(공백 · 구두점 · 연산자)은 구분자.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || !c.is_ascii()
}

/// 편집기가 호출자에게 요구하는 것.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Redraw,
    /// Enter — 줄 확정(편집기는 비워진다).
    Submit(String),
    /// Ctrl+C — 줄 버림.
    Cancel,
    /// 빈 줄에서 Ctrl+D.
    Eof,
    /// Tab — 호출자가 후보를 구해 [`LineEditor::replace_range`]로 넣는다.
    Complete,
    /// Ctrl+L.
    Clear,
}

/// 기록 거꾸로 찾기(Ctrl+R · readline `reverse-i-search`) 상태 — 질의 · 지금 일치한 기록 인덱스 · 들어오기 전 줄.
struct Search {
    query: String,
    pos: Option<usize>,
    saved: Vec<char>,
    saved_cursor: usize,
}

/// 되돌리기 묶음 — 이어 친 글자·이어 지운 글자는 한 단계로 합친다(readline과 같은 감각).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    None,
    Insert,
    Delete,
}

/// 되돌리기 스택 상한(줄 하나의 편집 · 넘으면 가장 오래된 것부터).
const UNDO_MAX: usize = 200;

/// 한 줄 편집 상태(글자 인덱스 · 기록 탐색은 지금 줄의 초안을 보존).
pub(crate) struct LineEditor {
    buf: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    hist_max: usize,
    hist_pos: Option<usize>,
    draft: Vec<char>,
    search: Option<Search>,
    undo: Vec<(Vec<char>, usize)>,
    redo: Vec<(Vec<char>, usize)>,
    group: Group,
    /// 마지막으로 지운 글(Ctrl+K/U/W · Alt+D · Alt+Backspace) — Alt+Y가 붙인다.
    kill: Vec<char>,
}

impl LineEditor {
    pub(crate) fn new(hist_max: usize) -> Self {
        LineEditor {
            buf: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            hist_max: hist_max.max(1),
            hist_pos: None,
            draft: Vec::new(),
            search: None,
            undo: Vec::new(),
            redo: Vec::new(),
            group: Group::None,
            kill: Vec::new(),
        }
    }

    /// 편집 전에 지금 상태를 되돌리기 스택에 남긴다 — 같은 묶음이 이어지면 합친다 · 새 편집은 다시 실행 스택을 비운다.
    fn snapshot(&mut self, group: Group) {
        if group != Group::None && group == self.group {
            return;
        }
        self.undo.push((self.buf.clone(), self.cursor));
        if self.undo.len() > UNDO_MAX {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.group = group;
    }

    /// 지운 글을 붙이기 버퍼에(빈 것은 무시).
    fn remember_kill(&mut self, text: Vec<char>) {
        if !text.is_empty() {
            self.kill = text;
        }
    }

    fn clear_undo(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.group = Group::None;
    }

    /// 검색 모드면 그 프롬프트(`(reverse-i-search)'질의': `) — 호출자가 보통 프롬프트 대신 그린다.
    pub(crate) fn search_prompt(&self) -> Option<String> {
        self.search
            .as_ref()
            .map(|s| format!("(reverse-i-search)'{}': ", s.query))
    }

    pub(crate) fn text(&self) -> String {
        self.buf.iter().collect()
    }

    /// 커서(글자 인덱스).
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    #[cfg(test)]
    pub(crate) fn history(&self) -> &[String] {
        &self.history
    }

    /// 키 하나 처리(순수).
    pub(crate) fn handle(&mut self, key: Key) -> Action {
        if self.search.is_some() {
            if let Some(a) = self.handle_search(&key) {
                return a;
            }
        }
        // 글자·지우기 밖의 키는 되돌리기 묶음을 끊는다(이동 뒤 친 글자는 새 단계).
        if !matches!(key, Key::Char(_) | Key::Backspace | Key::Delete | Key::Eof) {
            self.group = Group::None;
        }
        match key {
            Key::Undo => {
                let Some((b, c)) = self.undo.pop() else {
                    return Action::None;
                };
                let cur = (std::mem::replace(&mut self.buf, b), self.cursor);
                self.redo.push(cur);
                self.cursor = c.min(self.buf.len());
                Action::Redraw
            }
            Key::Redo => {
                let Some((b, c)) = self.redo.pop() else {
                    return Action::None;
                };
                let cur = (std::mem::replace(&mut self.buf, b), self.cursor);
                self.undo.push(cur);
                self.cursor = c.min(self.buf.len());
                Action::Redraw
            }
            Key::Yank => {
                if self.kill.is_empty() {
                    return Action::None;
                }
                self.snapshot(Group::None);
                let k = self.kill.clone();
                let n = k.len();
                self.buf.splice(self.cursor..self.cursor, k);
                self.cursor += n;
                Action::Redraw
            }
            Key::SearchHistory => {
                self.snapshot(Group::None);
                self.search = Some(Search {
                    query: String::new(),
                    pos: None,
                    saved: self.buf.clone(),
                    saved_cursor: self.cursor,
                });
                Action::Redraw
            }
            Key::Char(c) => {
                self.snapshot(Group::Insert);
                self.buf.insert(self.cursor, c);
                self.cursor += 1;
                Action::Redraw
            }
            Key::NewLine => {
                self.snapshot(Group::None);
                self.buf.insert(self.cursor, '\n');
                self.cursor += 1;
                Action::Redraw
            }
            Key::Left => {
                if self.cursor == 0 {
                    return Action::None;
                }
                self.cursor -= 1;
                Action::Redraw
            }
            Key::Right => {
                if self.cursor >= self.buf.len() {
                    return Action::None;
                }
                self.cursor += 1;
                Action::Redraw
            }
            Key::WordLeft => {
                let i = self.word_left();
                if i == self.cursor {
                    return Action::None;
                }
                self.cursor = i;
                Action::Redraw
            }
            Key::WordRight => {
                let i = self.word_right();
                if i == self.cursor {
                    return Action::None;
                }
                self.cursor = i;
                Action::Redraw
            }
            Key::KillWordLeft => {
                let i = self.word_left();
                if i == self.cursor {
                    return Action::None;
                }
                self.snapshot(Group::None);
                let k: Vec<char> = self.buf.drain(i..self.cursor).collect();
                self.remember_kill(k);
                self.cursor = i;
                Action::Redraw
            }
            Key::KillWordRight => {
                let i = self.word_right();
                if i == self.cursor {
                    return Action::None;
                }
                self.snapshot(Group::None);
                let k: Vec<char> = self.buf.drain(self.cursor..i).collect();
                self.remember_kill(k);
                Action::Redraw
            }
            Key::Home => {
                self.cursor = self.line_bounds(self.cursor).0;
                Action::Redraw
            }
            Key::End => {
                self.cursor = self.line_bounds(self.cursor).1;
                Action::Redraw
            }
            Key::Backspace => {
                if self.cursor == 0 {
                    return Action::None;
                }
                self.snapshot(Group::Delete);
                self.cursor -= 1;
                self.buf.remove(self.cursor);
                Action::Redraw
            }
            Key::Delete => {
                if self.cursor >= self.buf.len() {
                    return Action::None;
                }
                self.snapshot(Group::Delete);
                self.buf.remove(self.cursor);
                Action::Redraw
            }
            Key::KillToStart => {
                if self.cursor == 0 {
                    return Action::None;
                }
                self.snapshot(Group::None);
                let k: Vec<char> = self.buf.drain(..self.cursor).collect();
                self.remember_kill(k);
                self.cursor = 0;
                Action::Redraw
            }
            Key::KillToEnd => {
                if self.cursor >= self.buf.len() {
                    return Action::None;
                }
                self.snapshot(Group::None);
                let k = self.buf.split_off(self.cursor);
                self.remember_kill(k);
                Action::Redraw
            }
            Key::KillWord => {
                let mut i = self.cursor;
                while i > 0 && self.buf[i - 1].is_whitespace() {
                    i -= 1;
                }
                while i > 0 && !self.buf[i - 1].is_whitespace() {
                    i -= 1;
                }
                if i == self.cursor {
                    return Action::None;
                }
                self.snapshot(Group::None);
                let k: Vec<char> = self.buf.drain(i..self.cursor).collect();
                self.remember_kill(k);
                self.cursor = i;
                Action::Redraw
            }
            // 여러 줄 본문 안에서는 줄 이동 · 첫/끝 줄이면 기록 탐색.
            Key::Up => {
                if self.move_line(true) {
                    Action::Redraw
                } else {
                    self.history_prev()
                }
            }
            Key::Down => {
                if self.move_line(false) {
                    Action::Redraw
                } else {
                    self.history_next()
                }
            }
            Key::Tab => Action::Complete,
            Key::Enter => {
                let s = self.text();
                self.buf.clear();
                self.cursor = 0;
                self.hist_pos = None;
                self.clear_undo();
                Action::Submit(s)
            }
            Key::Cancel => {
                self.buf.clear();
                self.cursor = 0;
                self.hist_pos = None;
                self.clear_undo();
                Action::Cancel
            }
            Key::Eof => {
                if self.buf.is_empty() {
                    Action::Eof
                } else {
                    self.handle(Key::Delete)
                }
            }
            Key::ClearScreen => Action::Clear,
            Key::Ignore => Action::None,
        }
    }

    /// 검색 모드의 키 — 글자·Backspace = 질의를 고치고 **최신부터** 다시 찾기 · Ctrl+R = 지금 일치보다 **오래된** 일치 ·
    /// Ctrl+C = 들어오기 전 줄로 복원하고 끝 · 그 밖(Enter·이동·Tab…) = 검색을 끝내고 그 키를 일반 처리(`None` = 계속 처리하라).
    fn handle_search(&mut self, key: &Key) -> Option<Action> {
        match key {
            Key::Char(c) => {
                if let Some(s) = self.search.as_mut() {
                    s.query.push(*c);
                }
                self.search_find(None);
                Some(Action::Redraw)
            }
            Key::Backspace => {
                if let Some(s) = self.search.as_mut() {
                    s.query.pop();
                }
                self.search_find(None);
                Some(Action::Redraw)
            }
            Key::SearchHistory => {
                let from = self.search.as_ref().and_then(|s| s.pos);
                match from {
                    Some(0) => {}
                    Some(p) => self.search_find(Some(p - 1)),
                    None => self.search_find(None),
                }
                Some(Action::Redraw)
            }
            Key::Cancel => {
                if let Some(s) = self.search.take() {
                    self.buf = s.saved;
                    self.cursor = s.saved_cursor;
                }
                Some(Action::Redraw)
            }
            Key::Ignore => Some(Action::None),
            _ => {
                self.search = None;
                None
            }
        }
    }

    /// `start`(포함)부터 거꾸로 질의를 품은 기록을 찾는다(대소문자 무시 · `None` = 최신부터). 찾으면 줄 = 그 기록 · 커서 = 끝 ·
    /// 못 찾으면 그대로. 질의가 비면 들어오기 전 줄로.
    fn search_find(&mut self, start: Option<usize>) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        if s.query.is_empty() {
            s.pos = None;
            self.buf = s.saved.clone();
            self.cursor = s.saved_cursor;
            return;
        }
        let q = s.query.to_lowercase();
        let Some(top) = start.or(self.history.len().checked_sub(1)) else {
            return;
        };
        let hit = (0..=top.min(self.history.len().saturating_sub(1)))
            .rev()
            .find(|&i| self.history[i].to_lowercase().contains(&q));
        if let Some(i) = hit {
            s.pos = Some(i);
            self.buf = self.history[i].chars().collect();
            self.cursor = self.buf.len();
        }
    }

    /// `pos`가 속한 줄의 [시작, 끝)(글자 인덱스 · 끝 = 그 줄의 `\n` 자리 또는 본문 끝).
    fn line_bounds(&self, pos: usize) -> (usize, usize) {
        let pos = pos.min(self.buf.len());
        let start = self.buf[..pos]
            .iter()
            .rposition(|&c| c == '\n')
            .map_or(0, |i| i + 1);
        let end = self.buf[pos..]
            .iter()
            .position(|&c| c == '\n')
            .map_or(self.buf.len(), |i| pos + i);
        (start, end)
    }

    /// 여러 줄 본문에서 한 줄 위/아래로(같은 열 · 짧은 줄이면 끝) — 갈 줄이 없으면 false(호출자 = 기록 탐색).
    fn move_line(&mut self, up: bool) -> bool {
        let (s, e) = self.line_bounds(self.cursor);
        let col = self.cursor - s;
        let (ns, ne) = if up {
            if s == 0 {
                return false;
            }
            self.line_bounds(s - 1)
        } else {
            if e >= self.buf.len() {
                return false;
            }
            self.line_bounds(e + 1)
        };
        self.cursor = ns + col.min(ne - ns);
        true
    }

    /// 이어지는 줄을 마지막 기록 항목에 붙인다(여러 줄 문장 = 기록 한 항목 · 셸이 버퍼가 비지 않았을 때 부른다).
    pub(crate) fn amend_last_history(&mut self, line: &str) {
        let line = line.trim_end();
        match self.history.last_mut() {
            Some(last) => {
                last.push('\n');
                last.push_str(line);
            }
            None => self.push_history(line),
        }
    }

    /// 커서 앞 단어의 처음(readline `backward-word`: 구분자를 건너뛴 뒤 단어 글자를 건너뛴다).
    fn word_left(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && !is_word_char(self.buf[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word_char(self.buf[i - 1]) {
            i -= 1;
        }
        i
    }

    /// 커서 뒤 단어의 끝(readline `forward-word`).
    fn word_right(&self) -> usize {
        let n = self.buf.len();
        let mut i = self.cursor;
        while i < n && !is_word_char(self.buf[i]) {
            i += 1;
        }
        while i < n && is_word_char(self.buf[i]) {
            i += 1;
        }
        i
    }

    fn history_prev(&mut self) -> Action {
        if self.history.is_empty() {
            return Action::None;
        }
        let pos = match self.hist_pos {
            None => {
                self.draft = self.buf.clone();
                self.history.len() - 1
            }
            Some(0) => return Action::None,
            Some(p) => p - 1,
        };
        self.snapshot(Group::None);
        self.buf = self.history[pos].chars().collect();
        self.cursor = self.buf.len();
        self.hist_pos = Some(pos);
        Action::Redraw
    }

    fn history_next(&mut self) -> Action {
        let Some(p) = self.hist_pos else {
            return Action::None;
        };
        self.snapshot(Group::None);
        if p + 1 >= self.history.len() {
            self.buf = std::mem::take(&mut self.draft);
            self.hist_pos = None;
        } else {
            self.buf = self.history[p + 1].chars().collect();
            self.hist_pos = Some(p + 1);
        }
        self.cursor = self.buf.len();
        Action::Redraw
    }

    /// 기록에 넣는다(빈 줄·바로 앞과 같은 줄은 빼고 · 상한을 넘으면 앞을 버린다).
    pub(crate) fn push_history(&mut self, line: &str) {
        let line = line.trim_end();
        if line.trim().is_empty() || self.history.last().is_some_and(|l| l == line) {
            return;
        }
        self.history.push(line.to_string());
        if self.history.len() > self.hist_max {
            let n = self.history.len() - self.hist_max;
            self.history.drain(..n);
        }
    }

    /// `[start, cursor)`(글자 인덱스)를 `text`로 바꾸고 커서를 그 뒤에.
    pub(crate) fn replace_range(&mut self, start: usize, text: &str) {
        self.snapshot(Group::None);
        let start = start.min(self.cursor);
        let tail: Vec<char> = self.buf.split_off(self.cursor);
        self.buf.truncate(start);
        self.buf.extend(text.chars());
        self.cursor = self.buf.len();
        self.buf.extend(tail);
    }

    /// 기록 파일 읽기(한 줄 = 한 항목 · `\n`은 `\u{1}`로 접어 저장).
    pub(crate) fn load_history(&mut self, path: &std::path::Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        for l in text.lines() {
            let l = l.replace('\u{1}', "\n");
            if !l.trim().is_empty() {
                self.history.push(l);
            }
        }
        if self.history.len() > self.hist_max {
            let n = self.history.len() - self.hist_max;
            self.history.drain(..n);
        }
    }

    pub(crate) fn save_history(&self, path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let body: String = self
            .history
            .iter()
            .map(|l| l.replace('\n', "\u{1}") + "\n")
            .collect();
        let _ = std::fs::write(path, body);
    }
}

/// 글자의 칸 수(동아시아 넓은 글자 = 2 · 제어 = 0 · 그 밖 1).
pub(crate) fn char_width(c: char) -> usize {
    let u = c as u32;
    if u < 0x20 || (0x7f..0xa0).contains(&u) {
        return 0;
    }
    let wide = matches!(
        u,
        0x1100..=0x115f
            | 0x2e80..=0x303e
            | 0x3041..=0x33ff
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xa000..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe30..=0xfe4f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f300..=0x1f64f
            | 0x1f900..=0x1f9ff
            | 0x20000..=0x3fffd
    );
    if wide {
        2
    } else {
        1
    }
}

fn width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// 여러 줄 본문의 둘째 줄부터 앞에 붙는 이음 프롬프트(셸의 `   -> `와 같은 꼴).
const CONT_PROMPT: &str = "   -> ";

/// 줄 그리기(순수) — 이전 그리기의 커서 행 `prev_row`에서 출발해 프롬프트+본문을 다시 쓰고 커서를 놓는다. 폭 `cols`로 접힌다.
/// 본문의 `\n`은 줄 바꿈(`\r\n` + [`CONT_PROMPT`]) · 반환 = (출력 문자열 · 커서가 놓인 행 · 마지막 행) — 다음 그리기와 줄 마감이 쓴다.
pub(crate) fn render(
    prompt: &str,
    text: &str,
    cursor: usize,
    cols: usize,
    prev_row: usize,
) -> (String, usize, usize) {
    let cols = cols.max(2);
    let mut s = String::new();
    if prev_row > 0 {
        s.push_str(&format!("\x1b[{prev_row}A"));
    }
    s.push_str("\r\x1b[J");
    s.push_str(prompt);
    // (row, col) = 논리 줄의 시작 행 + 그 줄에서 쌓인 폭 → 실제 행 = row + col / cols · 열 = col % cols.
    let mut row = 0usize;
    let mut col = width(prompt);
    let mut cur: Option<(usize, usize)> = None;
    for (i, c) in text.chars().enumerate() {
        if i == cursor {
            cur = Some((row + col / cols, col % cols));
        }
        if c == '\n' {
            // 폭에 딱 맞게 찬 줄은 터미널 커서가 마지막 칸에 머물러 있어 `\r\n`이 한 행만 내린다.
            row += if col > 0 && col.is_multiple_of(cols) {
                col / cols
            } else {
                col / cols + 1
            };
            s.push_str("\r\n");
            s.push_str(CONT_PROMPT);
            col = width(CONT_PROMPT);
        } else {
            s.push(c);
            col += char_width(c);
        }
    }
    let (cur_row, cur_col) = cur.unwrap_or((row + col / cols, col % cols));
    let end_row = row + col / cols;
    if col > 0 && col.is_multiple_of(cols) {
        // 폭에 딱 맞게 끝나면 터미널마다 다음 행 이동이 다르다 → 공백으로 접고 처음으로.
        s.push_str(" \r");
    }
    if end_row > cur_row {
        s.push_str(&format!("\x1b[{}A", end_row - cur_row));
    }
    s.push('\r');
    if cur_col > 0 {
        s.push_str(&format!("\x1b[{cur_col}C"));
    }
    (s, cur_row, end_row)
}

/// 후보 목록을 열 맞춰 표로(순수) — 너비 `cols` · 최대 `max`개(넘치면 마지막 줄에 `… (+n)`).
pub(crate) fn format_columns(items: &[String], cols: usize, max: usize) -> String {
    if items.is_empty() {
        return String::new();
    }
    let shown = &items[..items.len().min(max)];
    let w = shown.iter().map(|s| width(s)).max().unwrap_or(1) + 2;
    let per = (cols.max(w) / w).max(1);
    let mut out = String::new();
    for (i, it) in shown.iter().enumerate() {
        out.push_str(it);
        if (i + 1) % per == 0 || i + 1 == shown.len() {
            out.push_str("\r\n");
        } else {
            let pad = w - width(it);
            out.extend(std::iter::repeat_n(' ', pad));
        }
    }
    if items.len() > max {
        out.push_str(&format!("… (+{})\r\n", items.len() - max));
    }
    out
}

/// 후보들의 공통 접두(대소문자 무시 · 글자는 첫 후보 것).
pub(crate) fn common_prefix(items: &[String]) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let mut n = first.chars().count();
    for it in &items[1..] {
        let m = first
            .chars()
            .zip(it.chars())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b) || a == b)
            .count();
        n = n.min(m);
    }
    first.chars().take(n).collect()
}

/// 읽기 결과.
pub(crate) enum Outcome {
    Line(String),
    /// Ctrl+C — 줄 버림(호출자는 다시 묻는다).
    Cancel,
    Eof,
}

/// 터미널 raw 모드 가드(끝나면 복원) — Windows = 콘솔 모드(줄 입력·에코·처리 끔 + VT 입력/출력 켬) · unix = `stty`.
struct RawGuard {
    #[cfg(windows)]
    modes: (u32, u32),
    #[cfg(not(windows))]
    saved: String,
}

#[cfg(windows)]
fn raw_mode() -> Option<RawGuard> {
    crate::term::raw_on().map(|modes| RawGuard { modes })
}

#[cfg(windows)]
impl Drop for RawGuard {
    fn drop(&mut self) {
        crate::term::raw_restore(self.modes);
    }
}

#[cfg(not(windows))]
fn raw_mode() -> Option<RawGuard> {
    let saved = std::process::Command::new("stty")
        .arg("-g")
        .stdin(std::process::Stdio::inherit())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())?;
    let ok = std::process::Command::new("stty")
        .args(["-icanon", "-echo", "-isig", "min", "1", "time", "0"])
        .stdin(std::process::Stdio::inherit())
        .status()
        .ok()
        .is_some_and(|s| s.success());
    ok.then_some(RawGuard { saved })
}

#[cfg(not(windows))]
impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("stty")
            .arg(&self.saved)
            .stdin(std::process::Stdio::inherit())
            .status();
    }
}

/// 완성 함수 — `(본문, 커서 글자 인덱스) → (바꿀 시작 글자 인덱스, 후보들)`.
pub(crate) type Completer<'a> = dyn FnMut(&str, usize) -> (usize, Vec<String>) + 'a;
/// 후보 하나로 확정될 때 그 글을 꾸민다(줄 · 커서 · 후보 → 넣을 글 · 예 = 테이블 뒤 자동 별칭 · 10-29 ⑥).
pub(crate) type Decorator<'a> = dyn FnMut(&str, usize, &str) -> String + 'a;

/// 터미널에서 한 줄을 읽는다(프롬프트·에코 = stderr · 키 주입 없음). 완성 = [`Completer`].
/// raw 모드를 못 켜면 종전 `read_line`과 같이 동작한다.
pub(crate) fn read_interactive(
    prompt: &str,
    ed: &mut LineEditor,
    complete: &mut Completer<'_>,
    decorate: &mut Decorator<'_>,
) -> Outcome {
    if !io::stdin().is_terminal() {
        return read_plain(prompt);
    }
    let Some(_raw) = raw_mode() else {
        return read_plain(prompt);
    };
    let mut err = io::stderr();
    let mut dec = Decoder::default();
    let cols = || crate::term::columns().unwrap_or(80);
    let mut row = 0usize;
    let mut end_row = 0usize;
    let mut draw = |ed: &LineEditor, row: &mut usize, end_row: &mut usize, fresh: bool| {
        let sp = ed.search_prompt();
        let (s, r, e) = render(
            sp.as_deref().unwrap_or(prompt),
            &ed.text(),
            ed.cursor(),
            cols(),
            if fresh { 0 } else { *row },
        );
        *row = r;
        *end_row = e;
        let _ = err.write_all(s.as_bytes());
        let _ = err.flush();
    };
    draw(ed, &mut row, &mut end_row, true);
    let finish = |row: usize, end_row: usize| {
        let mut e = io::stderr();
        if end_row > row {
            let _ = write!(e, "\x1b[{}B", end_row - row);
        }
        let _ = e.write_all(b"\r\n");
        let _ = e.flush();
    };
    let mut stdin = io::stdin().lock();
    let mut bytes: Vec<u8> = vec![0; 64];
    let mut trace = std::env::var_os("NSQL_KEY_TRACE").and_then(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .ok()
    });
    loop {
        let n = match read_chunk(&mut stdin, &mut bytes) {
            Ok(0) => {
                finish(row, end_row);
                return Outcome::Eof;
            }
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                finish(row, end_row);
                return Outcome::Eof;
            }
        };
        if let Some(f) = trace.as_mut() {
            let hex: Vec<String> = bytes[..n].iter().map(|b| format!("{b:02x}")).collect();
            let _ = writeln!(f, "bytes {}", hex.join(" "));
        }
        for &b in &bytes[..n] {
            let Some(key) = dec.push(b) else {
                continue;
            };
            if let Some(f) = trace.as_mut() {
                let _ = writeln!(f, "  key {key:?}");
            }
            match ed.handle(key) {
                Action::None => {}
                Action::Redraw => draw(ed, &mut row, &mut end_row, false),
                Action::Submit(s) => {
                    finish(row, end_row);
                    return Outcome::Line(s);
                }
                Action::Cancel => {
                    let _ = io::stderr().write_all(b"^C");
                    finish(row, end_row);
                    return Outcome::Cancel;
                }
                Action::Eof => {
                    finish(row, end_row);
                    return Outcome::Eof;
                }
                Action::Clear => {
                    let _ = io::stderr().write_all(b"\x1b[2J\x1b[H");
                    draw(ed, &mut row, &mut end_row, true);
                }
                Action::Complete => {
                    let (start, cands) = complete(&ed.text(), ed.cursor());
                    match cands.len() {
                        0 => {}
                        1 => {
                            let text = decorate(&ed.text(), ed.cursor(), &cands[0]);
                            ed.replace_range(start, &text);
                            draw(ed, &mut row, &mut end_row, false);
                        }
                        _ => {
                            let common = common_prefix(&cands);
                            let typed = ed.cursor().saturating_sub(start);
                            if common.chars().count() > typed {
                                ed.replace_range(start, &common);
                                draw(ed, &mut row, &mut end_row, false);
                            } else {
                                finish(row, end_row);
                                let _ = io::stderr()
                                    .write_all(format_columns(&cands, cols(), 120).as_bytes());
                                draw(ed, &mut row, &mut end_row, true);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 입력 한 덩어리(최대 64바이트 꼴) — Windows는 콘솔을 직접(UTF-16 → UTF-8 · Ctrl+Z 보존) · 그 밖은 표준 입력.
#[cfg(windows)]
fn read_chunk(_stdin: &mut io::StdinLock<'_>, buf: &mut Vec<u8>) -> io::Result<usize> {
    crate::term::read_console_utf8(buf)
}

#[cfg(not(windows))]
fn read_chunk(stdin: &mut io::StdinLock<'_>, buf: &mut Vec<u8>) -> io::Result<usize> {
    buf.resize(64, 0);
    stdin.read(buf)
}

/// 터미널이 아닐 때(배치 · 파이프) — 프롬프트는 그대로 찍고 한 줄.
fn read_plain(prompt: &str) -> Outcome {
    use std::io::BufRead;
    eprint!("{prompt}");
    let _ = io::stderr().flush();
    let mut line = String::new();
    match io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => Outcome::Eof,
        Ok(_) => Outcome::Line(line.trim_end_matches(['\r', '\n']).to_string()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn feed(d: &mut Decoder, bytes: &[u8]) -> Vec<Key> {
        bytes.iter().filter_map(|b| d.push(*b)).collect()
    }

    /// 디코더: ASCII · UTF-8(한글 3바이트) · 화살표(`ESC [ A` · `ESC O A`) · Home/End 두 꼴 · Delete · 제어 키 · 모르는 시퀀스 = Ignore.
    #[test]
    fn decoder_handles_utf8_escapes_and_controls() {
        let mut d = Decoder::default();
        assert_eq!(feed(&mut d, b"ab"), vec![Key::Char('a'), Key::Char('b')]);
        assert_eq!(feed(&mut d, "한".as_bytes()), vec![Key::Char('한')]);
        assert_eq!(
            feed(&mut d, b"\x1b[A\x1bOB\x1b[C\x1b[D"),
            vec![Key::Up, Key::Down, Key::Right, Key::Left]
        );
        assert_eq!(
            feed(&mut d, b"\x1b[H\x1b[F\x1b[1~\x1b[4~\x1b[7~\x1b[8~\x1b[3~"),
            vec![
                Key::Home,
                Key::End,
                Key::Home,
                Key::End,
                Key::Home,
                Key::End,
                Key::Delete
            ]
        );
        // 조합키가 붙은 화살표: Shift(2)는 방향만 · Alt(3)·Ctrl(5)·Alt+Ctrl(7)은 단어 이동.
        assert_eq!(feed(&mut d, b"\x1b[1;2C"), vec![Key::Right]);
        assert_eq!(feed(&mut d, b"\x1b[1;5C"), vec![Key::WordRight]);
        assert_eq!(feed(&mut d, b"\x1b[1;3D"), vec![Key::WordLeft]);
        assert_eq!(feed(&mut d, b"\x1b[1;7D"), vec![Key::WordLeft]);
        // Alt 조합(ESC + 글자): b/f = 단어 이동 · d = 앞쪽 단어 지우기 · Backspace(0x7f·0x08) = 뒤쪽 단어 지우기.
        assert_eq!(
            feed(&mut d, b"\x1bb\x1bf\x1bd\x1b\x7f\x1b\x08\x1bF"),
            vec![
                Key::WordLeft,
                Key::WordRight,
                Key::KillWordRight,
                Key::KillWordLeft,
                Key::KillWordLeft,
                Key::WordRight
            ]
        );
        assert_eq!(
            feed(&mut d, b"\r\n\x7f\x08\t\x03\x04\x01\x05\x0c\x0b\x15\x17"),
            vec![
                Key::Enter,
                Key::Enter,
                Key::Backspace,
                Key::Backspace,
                Key::Tab,
                Key::Cancel,
                Key::Eof,
                Key::Home,
                Key::End,
                Key::ClearScreen,
                Key::KillToEnd,
                Key::KillToStart,
                Key::KillWord
            ]
        );
        assert_eq!(feed(&mut d, b"\x1b[5~"), vec![Key::Ignore], "PageUp = 무시");
        assert_eq!(
            feed(&mut d, b"\x1bx"),
            vec![Key::Ignore],
            "그 밖 Alt 조합 = 무시"
        );
        assert_eq!(
            feed(&mut d, b"\xff"),
            vec![Key::Ignore],
            "깨진 UTF-8 = 무시"
        );
    }

    /// 편집: 삽입·이동·지우기·단어 지우기·Enter(비움)·Ctrl+C·빈 줄 Ctrl+D.
    #[test]
    fn editor_edits_and_submits() {
        let mut ed = LineEditor::new(10);
        for c in "select".chars() {
            assert_eq!(ed.handle(Key::Char(c)), Action::Redraw);
        }
        assert_eq!(ed.handle(Key::Home), Action::Redraw);
        assert_eq!(ed.handle(Key::Left), Action::None);
        assert_eq!(ed.handle(Key::Delete), Action::Redraw);
        assert_eq!(ed.text(), "elect");
        assert_eq!(ed.handle(Key::End), Action::Redraw);
        assert_eq!(ed.handle(Key::Right), Action::None);
        assert_eq!(ed.handle(Key::Backspace), Action::Redraw);
        assert_eq!(ed.text(), "elec");
        for c in " from  t".chars() {
            ed.handle(Key::Char(c));
        }
        assert_eq!(ed.handle(Key::KillWord), Action::Redraw);
        assert_eq!(ed.text(), "elec from  ");
        assert_eq!(ed.handle(Key::KillWord), Action::Redraw);
        assert_eq!(ed.text(), "elec ");
        ed.handle(Key::Left);
        ed.handle(Key::Left);
        assert_eq!(ed.handle(Key::KillToEnd), Action::Redraw);
        assert_eq!(ed.text(), "ele");
        assert_eq!(ed.handle(Key::KillToStart), Action::Redraw);
        assert_eq!(ed.text(), "");
        assert_eq!(ed.handle(Key::Eof), Action::Eof);
        ed.handle(Key::Char('x'));
        assert_eq!(
            ed.handle(Key::Eof),
            Action::None,
            "비어 있지 않으면 Delete(끝이라 없음)"
        );
        assert_eq!(ed.handle(Key::Cancel), Action::Cancel);
        assert_eq!(ed.text(), "");
        ed.handle(Key::Char('1'));
        assert_eq!(ed.handle(Key::Enter), Action::Submit("1".into()));
        assert_eq!(ed.text(), "");
        assert_eq!(ed.handle(Key::Tab), Action::Complete);
    }

    /// 기록: 중복·빈 줄 제외 · 상한 · ↑↓ 탐색은 초안을 보존 · 끝을 넘으면 초안으로.
    #[test]
    fn history_navigation_keeps_draft_and_caps() {
        let mut ed = LineEditor::new(2);
        ed.push_history("a");
        ed.push_history("a");
        ed.push_history("   ");
        ed.push_history("b");
        ed.push_history("c");
        assert_eq!(ed.history(), ["b", "c"]);
        ed.handle(Key::Char('d'));
        assert_eq!(ed.handle(Key::Up), Action::Redraw);
        assert_eq!(ed.text(), "c");
        assert_eq!(ed.handle(Key::Up), Action::Redraw);
        assert_eq!(ed.text(), "b");
        assert_eq!(ed.handle(Key::Up), Action::None);
        assert_eq!(ed.handle(Key::Down), Action::Redraw);
        assert_eq!(ed.handle(Key::Down), Action::Redraw);
        assert_eq!(ed.text(), "d", "초안으로 돌아온다");
        assert_eq!(ed.handle(Key::Down), Action::None);
        // 파일 왕복(여러 줄 항목은 접힌다).
        let dir = std::env::temp_dir().join(format!("nsql-hist-{}", std::process::id()));
        let path = dir.join("shell_history");
        ed.push_history("x\ny");
        ed.save_history(&path);
        let mut e2 = LineEditor::new(10);
        e2.load_history(&path);
        assert_eq!(e2.history(), ["c", "x\ny"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 되돌리기·다시·붙이기: 이어 친 글자 = 한 단계 · 이동 뒤 글자 = 새 단계 · 지우기 = 한 단계 + 붙이기 버퍼 · Enter = 비움 · 디코더.
    #[test]
    fn undo_redo_and_yank() {
        let mut ed = LineEditor::new(10);
        for c in "abc".chars() {
            ed.handle(Key::Char(c));
        }
        ed.handle(Key::Home);
        ed.handle(Key::Char('x'));
        assert_eq!(ed.text(), "xabc");
        assert_eq!(ed.handle(Key::Undo), Action::Redraw);
        assert_eq!(
            (ed.text().as_str(), ed.cursor()),
            ("abc", 0),
            "이동 뒤 글자 = 따로"
        );
        assert_eq!(ed.handle(Key::Undo), Action::Redraw);
        assert_eq!(ed.text(), "", "이어 친 abc = 한 단계");
        assert_eq!(ed.handle(Key::Undo), Action::None, "더 없음");
        assert_eq!(ed.handle(Key::Redo), Action::Redraw);
        assert_eq!(ed.text(), "abc");
        assert_eq!(ed.handle(Key::Redo), Action::Redraw);
        assert_eq!(ed.text(), "xabc");
        assert_eq!(ed.handle(Key::Redo), Action::None);
        // Ctrl+K = 지우기 + 붙이기 버퍼 · Alt+Y = 붙이기 · 되돌리기 한 단계씩(다시 실행 뒤 커서 = 그때의 1).
        assert_eq!(ed.cursor(), 1);
        assert_eq!(ed.handle(Key::KillToEnd), Action::Redraw);
        assert_eq!(ed.text(), "x");
        assert_eq!(ed.handle(Key::Yank), Action::Redraw);
        assert_eq!((ed.text().as_str(), ed.cursor()), ("xabc", 4));
        ed.handle(Key::Undo);
        assert_eq!(ed.text(), "x");
        ed.handle(Key::Undo);
        assert_eq!(ed.text(), "xabc");
        // 새 편집은 다시 실행 스택을 비운다(되돌린 뒤 커서 = 그때의 1 → 끝으로 옮기고 친다).
        ed.handle(Key::End);
        ed.handle(Key::Char('!'));
        assert_eq!(ed.handle(Key::Redo), Action::None);
        // 이어 지운 글자 = 한 단계.
        ed.handle(Key::Backspace);
        ed.handle(Key::Backspace);
        assert_eq!(ed.text(), "xab");
        ed.handle(Key::Undo);
        assert_eq!(ed.text(), "xabc!");
        // Enter = 비움 · 붙이기 버퍼는 남는다.
        assert_eq!(ed.handle(Key::Enter), Action::Submit("xabc!".into()));
        assert_eq!(ed.handle(Key::Undo), Action::None);
        assert_eq!(ed.handle(Key::Yank), Action::Redraw);
        assert_eq!(ed.text(), "abc");
        // 디코더: Ctrl+Z · Ctrl+_ = 되돌리기 · Ctrl+Y = 다시 · Alt+Y = 붙이기.
        let mut d = Decoder::default();
        assert_eq!(
            feed(&mut d, b"\x1a\x1f\x19\x1by"),
            vec![Key::Undo, Key::Undo, Key::Redo, Key::Yank]
        );
    }

    /// 단어 이동·지우기(readline 경계 = 영숫자·`_`·비ASCII · 구두점·공백은 구분자): `select a.col_1, 한글 from t`.
    #[test]
    fn word_moves_and_kills() {
        let mut ed = LineEditor::new(10);
        for c in "select a.col_1, 한글 from t".chars() {
            ed.handle(Key::Char(c));
        }
        let n = ed.text().chars().count();
        assert_eq!(ed.cursor(), n);
        // 끝에서 Alt+B: t → from → 한글 → col_1 → a → select → 0.
        let mut stops = Vec::new();
        while ed.handle(Key::WordLeft) == Action::Redraw {
            stops.push(ed.cursor());
        }
        assert_eq!(stops, vec![n - 1, n - 6, 16, 9, 7, 0]);
        // 처음에서 Alt+F: 단어 끝으로.
        assert_eq!(ed.handle(Key::WordRight), Action::Redraw);
        assert_eq!(ed.cursor(), 6, "select|");
        ed.handle(Key::WordRight);
        assert_eq!(ed.cursor(), 8, "select a|");
        ed.handle(Key::WordRight);
        assert_eq!(ed.cursor(), 14, "select a.col_1|");
        // Alt+D = 앞쪽 단어(구분자 포함) 지우기: ", 한글" 중 한글까지.
        assert_eq!(ed.handle(Key::KillWordRight), Action::Redraw);
        assert_eq!(ed.text(), "select a.col_1 from t");
        // Alt+Backspace = 뒤쪽 단어 지우기: col_1.
        assert_eq!(ed.handle(Key::KillWordLeft), Action::Redraw);
        assert_eq!((ed.text().as_str(), ed.cursor()), ("select a. from t", 9));
        // 양 끝 = 변화 없음.
        ed.handle(Key::Home);
        assert_eq!(ed.handle(Key::WordLeft), Action::None);
        assert_eq!(ed.handle(Key::KillWordLeft), Action::None);
        ed.handle(Key::End);
        assert_eq!(ed.handle(Key::WordRight), Action::None);
        assert_eq!(ed.handle(Key::KillWordRight), Action::None);
        // Ctrl+W는 종전대로 공백 기준(a. 까지 한 덩어리).
        assert_eq!(ed.handle(Key::KillWord), Action::Redraw);
        assert_eq!(ed.text(), "select a. from ");
    }

    /// Ctrl+R 거꾸로 찾기: 질의 글자마다 최신부터 · 반복 = 더 오래된 일치 · Backspace = 다시 최신부터 · Ctrl+C = 복원 ·
    /// 이동 키 = 검색 끝(줄 유지) · Enter = 검색 끝 + 확정 · 프롬프트.
    #[test]
    fn reverse_search_walks_history() {
        let mut ed = LineEditor::new(10);
        for l in ["select 1", "insert x", "SELECT 2", "update y"] {
            ed.push_history(l);
        }
        for c in "dr".chars() {
            ed.handle(Key::Char(c));
        }
        assert_eq!(ed.handle(Key::SearchHistory), Action::Redraw);
        assert_eq!(
            ed.search_prompt().as_deref(),
            Some("(reverse-i-search)'': ")
        );
        assert_eq!(ed.text(), "dr", "질의가 비면 줄 그대로");
        ed.handle(Key::Char('s'));
        ed.handle(Key::Char('e'));
        assert_eq!(ed.text(), "SELECT 2", "최신 일치(대소문자 무시)");
        assert_eq!(
            ed.search_prompt().as_deref(),
            Some("(reverse-i-search)'se': ")
        );
        assert_eq!(ed.handle(Key::SearchHistory), Action::Redraw);
        assert_eq!(ed.text(), "insert x", "더 오래된 일치");
        ed.handle(Key::SearchHistory);
        assert_eq!(ed.text(), "select 1");
        ed.handle(Key::SearchHistory);
        assert_eq!(ed.text(), "select 1", "더 없으면 그대로");
        ed.handle(Key::Char('z'));
        assert_eq!(ed.text(), "select 1", "일치 없음 = 줄 유지");
        assert_eq!(ed.handle(Key::Backspace), Action::Redraw);
        assert_eq!(ed.text(), "SELECT 2", "질의를 줄이면 다시 최신부터");
        // 이동 키 = 검색 끝 · 줄은 그대로 · 그 키는 일반 처리(끝에서 Left = 한 칸).
        assert_eq!(ed.handle(Key::Left), Action::Redraw);
        assert!(ed.search_prompt().is_none());
        assert_eq!((ed.text().as_str(), ed.cursor()), ("SELECT 2", 7));
        // Ctrl+C = 들어오기 전 줄 복원.
        ed.handle(Key::SearchHistory);
        ed.handle(Key::Char('u'));
        assert_eq!(ed.text(), "update y");
        assert_eq!(ed.handle(Key::Cancel), Action::Redraw);
        assert_eq!((ed.text().as_str(), ed.cursor()), ("SELECT 2", 7));
        assert!(ed.search_prompt().is_none());
        // Enter = 검색 끝 + 확정.
        ed.handle(Key::SearchHistory);
        ed.handle(Key::Char('i'));
        assert_eq!(ed.handle(Key::Enter), Action::Submit("insert x".into()));
        assert!(ed.search_prompt().is_none());
        // 디코더: Ctrl+R = 0x12.
        let mut d = Decoder::default();
        assert_eq!(d.push(0x12), Some(Key::SearchHistory));
    }

    /// 바꾸기: `[start, cursor)`를 후보로 · 꼬리 보존 · 공통 접두.
    #[test]
    fn replace_range_and_common_prefix() {
        let mut ed = LineEditor::new(10);
        for c in "sel from t".chars() {
            ed.handle(Key::Char(c));
        }
        for _ in 0..7 {
            ed.handle(Key::Left);
        }
        ed.replace_range(0, "SELECT");
        assert_eq!(ed.text(), "SELECT from t");
        assert_eq!(ed.cursor(), 6);
        assert_eq!(
            common_prefix(&["EMP_A".into(), "emp_b".into(), "EMP".into()]),
            "EMP"
        );
        assert_eq!(common_prefix(&[]), "");
    }

    /// 그리기(폭 10): 접힘 행 수 · 커서 되돌림 · 이전 행에서 위로 · 폭에 딱 맞는 끝.
    #[test]
    fn render_wraps_and_positions_cursor() {
        let (s, row, end) = render("> ", "abc", 1, 10, 0);
        assert_eq!(s, "\r\x1b[J> abc\r\x1b[3C");
        assert_eq!((row, end), (0, 0));
        // "> " + 12자 = 14칸 → 2행 · 커서 끝(행 1 · 열 4).
        let (s, row, end) = render("> ", "abcdefghijkl", 12, 10, 1);
        assert!(s.starts_with("\x1b[1A\r\x1b[J> abcdefghijkl"));
        assert!(s.ends_with("\r\x1b[4C"));
        assert_eq!((row, end), (1, 1));
        // 커서가 첫 행(글자 3 = 5칸)이면 끝 행에서 1행 위로.
        let (s, row, end) = render("> ", "abcdefghijkl", 3, 10, 0);
        assert!(s.ends_with("\x1b[1A\r\x1b[5C"));
        assert_eq!((row, end), (0, 1));
        // 딱 10칸 = 공백으로 접고 끝 행 1 · 커서 열 0.
        let (s, row, end) = render("> ", "abcdefgh", 8, 10, 0);
        assert!(s.contains(" \r"));
        assert_eq!((row, end), (1, 1));
        assert!(s.ends_with('\r'));
        // 한글 = 2칸.
        assert_eq!(char_width('한'), 2);
        let (_, row, _) = render("", "한글한글한", 5, 10, 0);
        assert_eq!(row, 1);
    }

    /// 여러 줄 본문 그리기: `\n` = `\r\n` + 이음 프롬프트 · 행 수 · 커서가 둘째 줄이면 그 행 · 접힌 첫 줄 뒤 줄 바꿈 · 딱 찬 줄.
    #[test]
    fn render_handles_newlines() {
        // "> ab" / "   -> cd" · 커서 = 'c'(인덱스 3) → 행 1 · 열 6.
        let (s, row, end) = render("> ", "ab\ncd", 3, 20, 0);
        assert_eq!(s, "\r\x1b[J> ab\r\n   -> cd\r\x1b[6C");
        assert_eq!((row, end), (1, 1));
        // 커서가 첫 줄 끝(인덱스 2 = '\n' 자리) → 행 0 · 열 4 · 끝 행 1에서 한 행 위로.
        let (s, row, end) = render("> ", "ab\ncd", 2, 20, 0);
        assert!(s.ends_with("\x1b[1A\r\x1b[4C"), "{s:?}");
        assert_eq!((row, end), (0, 1));
        // 첫 줄이 접히면(폭 10 · "> " + 12자 = 2행) 둘째 논리 줄은 행 2.
        let (_, row, end) = render("> ", "abcdefghijkl\nx", 14, 10, 0);
        assert_eq!((row, end), (2, 2));
        // 첫 줄이 폭에 딱 차면(10칸) `\r\n`은 한 행만 내린다 → 둘째 줄 = 행 1.
        let (_, row, end) = render("> ", "abcdefgh\nx", 10, 10, 0);
        assert_eq!((row, end), (1, 1));
        // 빈 둘째 줄 끝 커서 = 이음 프롬프트 폭(6).
        let (s, _, _) = render("> ", "ab\n", 3, 20, 0);
        assert!(s.ends_with("\r\x1b[6C"), "{s:?}");
    }

    /// 여러 줄 편집: Alt+Enter 삽입 · ↑↓ = 줄 이동(같은 열 · 짧은 줄 끝) → 첫/끝 줄에서는 기록 · Home/End = 줄 안 ·
    /// Backspace로 줄 잇기 · 이어지는 줄은 기록 한 항목에 붙음 · 디코더 ESC CR.
    #[test]
    fn multiline_edit_navigation_and_history_amend() {
        let mut ed = LineEditor::new(10);
        ed.push_history("select 1");
        for c in "select a".chars() {
            ed.handle(Key::Char(c));
        }
        assert_eq!(ed.handle(Key::NewLine), Action::Redraw);
        for c in "from t".chars() {
            ed.handle(Key::Char(c));
        }
        assert_eq!(ed.text(), "select a\nfrom t");
        // 둘째 줄 끝(열 6) → ↑ = 첫 줄 열 6.
        assert_eq!(ed.handle(Key::Up), Action::Redraw);
        assert_eq!(ed.cursor(), 6);
        // 첫 줄에서 ↑ = 기록.
        ed.handle(Key::Up);
        assert_eq!(ed.text(), "select 1");
        ed.handle(Key::Down);
        assert_eq!(ed.text(), "select a\nfrom t", "초안 복귀");
        // 끝 줄에서 ↓ = 기록(더 없음 = 변화 없음).
        assert_eq!(ed.handle(Key::Down), Action::None);
        // Home/End = 그 줄 안.
        ed.handle(Key::Home);
        assert_eq!(ed.cursor(), 9, "둘째 줄 시작");
        ed.handle(Key::End);
        assert_eq!(ed.cursor(), 15);
        // 첫 줄 끝으로 가서 ↓ = 둘째 줄의 같은 열(짧으면 끝).
        ed.handle(Key::Up);
        ed.handle(Key::End);
        assert_eq!(ed.cursor(), 8);
        ed.handle(Key::Down);
        assert_eq!(ed.cursor(), 15, "둘째 줄은 6자 → 끝");
        // 둘째 줄 처음에서 Backspace = 줄 잇기.
        ed.handle(Key::Home);
        ed.handle(Key::Backspace);
        assert_eq!(ed.text(), "select afrom t");
        ed.handle(Key::Undo);
        assert_eq!(ed.text(), "select a\nfrom t");
        // Enter = 여러 줄 그대로 확정.
        assert_eq!(
            ed.handle(Key::Enter),
            Action::Submit("select a\nfrom t".into())
        );
        // 셸의 기록 붙이기: 첫 줄 push · 이어지는 줄 amend → 한 항목.
        ed.push_history("select x");
        ed.amend_last_history("from y;");
        assert_eq!(
            ed.history().last().map(String::as_str),
            Some("select x\nfrom y;")
        );
        let mut d = Decoder::default();
        assert_eq!(feed(&mut d, b"\x1b\r"), vec![Key::NewLine]);
    }

    /// 후보 표: 열 맞춤 · 줄 바꿈 `\r\n` · 상한 넘침 표시.
    #[test]
    fn format_columns_aligns_and_caps() {
        let items: Vec<String> = ["aa", "b", "cccc", "dd", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // 최대 폭 4+2 = 6 · 14칸이면 2열.
        assert_eq!(
            format_columns(&items, 14, 10),
            "aa    b\r\ncccc  dd\r\ne\r\n"
        );
        assert_eq!(format_columns(&items, 80, 2), "aa  b\r\n… (+3)\r\n");
        assert_eq!(format_columns(&[], 80, 2), "");
    }
}
