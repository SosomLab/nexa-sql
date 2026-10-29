//! `nsql shell` 줄 편집기(T-453 · D-393 · 외부 crate 0 = DR-3) — 키 디코더 · 편집 상태 · 그리기 · 기록 · raw 모드 · 읽기 루프.
//!
//! 순수 부분(디코더 [`Decoder`] · 편집 [`LineEditor`] · 그리기 [`render`] · 후보 표 [`format_columns`])은 시험으로 덮고,
//! 터미널 I/O는 [`read_interactive`] 한 곳. 터미널이 아니면(배치 · 파이프) 호출자가 종전 `read_line`을 쓴다.
//! 키 = 좌우·Home/End·Backspace/Delete·↑↓ 기록·Tab 완성·Ctrl+A/E/B/F/U/K/W/L·Ctrl+C(줄 취소)·Ctrl+D(빈 줄 = 끝).
//! 그리기 = ANSI(`\r` · `ESC[K` · `ESC[J` · 커서 이동) — Windows는 raw 모드에서 VT 처리를 켠다(Windows 10+).

use std::io::{self, IsTerminal, Read, Write};

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

    /// `ESC …` — `[`/`O` 뒤 종료 글자(알파벳 · `~`)까지 모은다. ESC 뒤 다른 글자(Alt 조합)는 버린다.
    fn finish_esc(&mut self) -> Option<Key> {
        let key = match self.esc.as_slice() {
            [0x1b, b'['] | [0x1b, b'O'] => return None,
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
                let first_num: u32 = params
                    .split(|c| *c == b';')
                    .next()
                    .and_then(|p| std::str::from_utf8(p).ok())
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0);
                match (last, first_num) {
                    (b'A', _) => Key::Up,
                    (b'B', _) => Key::Down,
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

/// 한 줄 편집 상태(글자 인덱스 · 기록 탐색은 지금 줄의 초안을 보존).
pub(crate) struct LineEditor {
    buf: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    hist_max: usize,
    hist_pos: Option<usize>,
    draft: Vec<char>,
    search: Option<Search>,
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
        }
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
        match key {
            Key::SearchHistory => {
                self.search = Some(Search {
                    query: String::new(),
                    pos: None,
                    saved: self.buf.clone(),
                    saved_cursor: self.cursor,
                });
                Action::Redraw
            }
            Key::Char(c) => {
                self.buf.insert(self.cursor, c);
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
            Key::Home => {
                self.cursor = 0;
                Action::Redraw
            }
            Key::End => {
                self.cursor = self.buf.len();
                Action::Redraw
            }
            Key::Backspace => {
                if self.cursor == 0 {
                    return Action::None;
                }
                self.cursor -= 1;
                self.buf.remove(self.cursor);
                Action::Redraw
            }
            Key::Delete => {
                if self.cursor >= self.buf.len() {
                    return Action::None;
                }
                self.buf.remove(self.cursor);
                Action::Redraw
            }
            Key::KillToStart => {
                self.buf.drain(..self.cursor);
                self.cursor = 0;
                Action::Redraw
            }
            Key::KillToEnd => {
                self.buf.truncate(self.cursor);
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
                self.buf.drain(i..self.cursor);
                self.cursor = i;
                Action::Redraw
            }
            Key::Up => self.history_prev(),
            Key::Down => self.history_next(),
            Key::Tab => Action::Complete,
            Key::Enter => {
                let s = self.text();
                self.buf.clear();
                self.cursor = 0;
                self.hist_pos = None;
                Action::Submit(s)
            }
            Key::Cancel => {
                self.buf.clear();
                self.cursor = 0;
                self.hist_pos = None;
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
        self.buf = self.history[pos].chars().collect();
        self.cursor = self.buf.len();
        self.hist_pos = Some(pos);
        Action::Redraw
    }

    fn history_next(&mut self) -> Action {
        let Some(p) = self.hist_pos else {
            return Action::None;
        };
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

/// 줄 그리기(순수) — 이전 그리기의 커서 행 `prev_row`에서 출발해 프롬프트+본문을 다시 쓰고 커서를 놓는다. 폭 `cols`로 접힌다.
/// 반환 = (출력 문자열 · 커서가 놓인 행 · 마지막 행) — 다음 그리기와 줄 마감이 쓴다.
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
    s.push_str(text);
    let total = width(prompt) + width(text);
    let mut end_row = total / cols;
    let end_col = total % cols;
    if end_col == 0 && total > 0 {
        // 폭에 딱 맞게 끝나면 터미널마다 다음 행 이동이 다르다 → 공백으로 접고 처음으로.
        s.push_str(" \r");
    } else {
        end_row = total / cols;
    }
    let cur_w = width(prompt) + text.chars().take(cursor).map(char_width).sum::<usize>();
    let cur_row = cur_w / cols;
    let cur_col = cur_w % cols;
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

/// 터미널에서 한 줄을 읽는다(프롬프트·에코 = stderr · 키 주입 없음). 완성 = [`Completer`].
/// raw 모드를 못 켜면 종전 `read_line`과 같이 동작한다.
pub(crate) fn read_interactive(
    prompt: &str,
    ed: &mut LineEditor,
    complete: &mut Completer<'_>,
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
    let mut bytes = [0u8; 64];
    loop {
        let n = match stdin.read(&mut bytes) {
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
        for &b in &bytes[..n] {
            let Some(key) = dec.push(b) else {
                continue;
            };
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
                            ed.replace_range(start, &cands[0]);
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
        // 조합키가 붙은 화살표(`ESC [ 1;5C`)도 방향만.
        assert_eq!(feed(&mut d, b"\x1b[1;5C"), vec![Key::Right]);
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
        assert_eq!(feed(&mut d, b"\x1bx"), vec![Key::Ignore], "Alt 조합 = 무시");
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
