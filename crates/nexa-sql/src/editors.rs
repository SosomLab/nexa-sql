//! 편집기 탭(사용자 09-14) — nexa-ctl `TabBar`(dir2 이식 · U-2) + 탭마다 `TextBox` 버퍼.
//!
//! - 줄 수 = 설정 `tabs.rows`(**multi 기본** · single = ◀ ▶ 스크롤 + 드래그 이동).
//! - 툴팁 = 설정 `tabs.tooltip`(기본 켬) — 탭 위에 1초 머물면 카드(제목 · 문장 수 · 글자 수 · 접속). 내용은 호스트가 [`Editors::set_conn_desc`]로 준다.
//! - 세션 분리(`session.mode = per-editor`)는 T-54 — 지금은 모든 탭이 한 세션.

use crate::eol::Eol;
use crate::syntax::SyntaxRegistry;
use nexa_ctl::draw::{draw_tooltip, DrawCtx};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{
    Control, InputEvent, Invalidations, SyntaxSpec, TabAction, TabBar, TextBox, WhitespaceStyle,
    Widget,
};
use nsql_i18n::{t, Msg};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub(crate) struct Editors {
    tabs: TabBar,
    bufs: Vec<TextBox>,
    titles: Vec<String>,
    active: usize,
    counter: usize,
    /// 탭별 안정 id(닫혀도 재사용 없음) — 호스트가 결과 그리드를 탭과 짝지을 때 쓴다(사용자 09-16).
    ids: Vec<u64>,
    next_id: u64,
    bounds: Rect,
    scale: f32,
    line_numbers: bool,
    tooltip_on: bool,
    /// (탭 index · 머문 시작) — 1초 뒤 카드.
    hover: Option<(usize, Instant)>,
    cursor: (i32, i32),
    conn_desc: String,
    /// 탭별 구문(기본 = 제목 확장자 · 사용자 변경 시 그 탭만).
    syntax: Vec<Rc<SyntaxSpec>>,
    registry: Rc<SyntaxRegistry>,
    rulers: Vec<usize>,
    /// 안내선 스타일·표시 · 동일 출현 외곽선(설정 · 새 탭에도 적용).
    rulers_show: bool,
    ruler_color: Option<nexa_ctl::theme::Color>,
    ruler_alpha: f32,
    occurrence_hl: bool,
    /// 첫 글자 앞 여백(설정 `editor.text_pad_left`).
    text_inset: i32,
    whitespace: WhitespaceStyle,
    /// (탭 폭, 공백 들여쓰기) **기본값**(설정 `editor.tab_size`/`editor.indent_spaces`) — 새 탭의 시작값.
    indent: (u8, bool),
    /// 탭 정지점 방식(설정 `editor.tab_stops` · 09-16).
    tab_stops: bool,
    /// 휠 스크롤을 줄 경계에 맞추는가(설정 `editor.scroll` = row · 기본 pixel · 09-16).
    scroll_snap: bool,
    /// 탭별 들여쓰기 재정의(`None` = 기본값 따름) — 상태줄 팝업은 **그 탭만** 바꾼다(Sublime 관례 · 사용자 09-15).
    indents: Vec<Option<(u8, bool)>>,
    /// 탭별 파일 경로(T-74 · `None` = 제목 없는 새 스크립트).
    paths: Vec<Option<PathBuf>>,
    /// 마지막으로 열거나 저장한 본문(더러움 판정 근거 · 새 탭 = 빈 문자열).
    saved: Vec<String>,
    /// 탭별 줄끝이 CRLF였나(저장 때 원래대로 되돌린다 · 새 탭 = OS 기본).
    eol: Vec<Eol>,
    /// 마지막 열기/저장 시점의 줄끝(줄끝만 바꿔도 더러움 표시 · 09-16).
    saved_eol: Vec<Eol>,
    /// 새 탭의 줄끝 기본(설정 `file.eol_new` · docs/38).
    default_eol: Eol,
    /// 탭별 인코딩(`utf8|utf8bom|utf16le|utf16be` · 열 때 감지/선택 · 저장 기본값).
    encs: Vec<String>,
    /// 탭 바에 마지막으로 보낸 표시 제목(더러움 `*` 포함) — 바뀔 때만 다시 보낸다.
    shown_titles: Vec<String>,
    /// 더러운 탭 닫기 2단(같은 탭을 3초 안에 다시 닫으면 버림).
    pending_close: Option<(usize, Instant)>,
    /// 호스트 상태줄에 전할 1회성 안내.
    notice: Option<Msg>,
}

/// 더러운 탭 닫기 확인 유효 시간.
const CLOSE_CONFIRM: Duration = Duration::from_secs(3);

const HOVER_MS: u128 = 900;

impl Editors {
    pub(crate) fn new(
        line_numbers: bool,
        multiline: bool,
        tooltip_on: bool,
        registry: Rc<SyntaxRegistry>,
    ) -> Self {
        let mut tabs = TabBar::new();
        tabs.set_multiline(multiline);
        tabs.set_show_new(true);
        let mut e = Editors {
            tabs,
            bufs: Vec::new(),
            titles: Vec::new(),
            active: 0,
            counter: 0,
            bounds: Rect::new(0, 0, 0, 0),
            scale: 1.0,
            line_numbers,
            tooltip_on,
            hover: None,
            cursor: (0, 0),
            conn_desc: String::new(),
            syntax: Vec::new(),
            registry,
            rulers: Vec::new(),
            rulers_show: true,
            ruler_color: None,
            ruler_alpha: 0.25,
            occurrence_hl: true,
            text_inset: 3,
            whitespace: WhitespaceStyle::default(),
            indent: (4, true),
            indents: Vec::new(),
            paths: Vec::new(),
            saved: Vec::new(),
            eol: Vec::new(),
            saved_eol: Vec::new(),
            default_eol: Eol::os(),
            ids: Vec::new(),
            next_id: 1,
            tab_stops: true,
            scroll_snap: false,
            encs: Vec::new(),
            shown_titles: Vec::new(),
            pending_close: None,
            notice: None,
        };
        e.new_tab(None);
        e
    }

    fn make_box(&self, text: &str, syntax: &Rc<SyntaxSpec>) -> TextBox {
        let mut tb = TextBox::new(t(Msg::PhEditor))
            .with_multiline()
            .with_text(text);
        tb.set_line_numbers(self.line_numbers);
        // Golden식 표시 띠(줄번호 오른쪽 4px · 색 막대 자리) + 첫 글자 앞 2px(사용자 09-16).
        tb.set_gutter_marks(true);
        tb.set_scale(self.scale);
        tb.set_highlighter(Some(syntax.clone()));
        tb.set_rulers(self.rulers.clone());
        tb.set_rulers_visible(self.rulers_show);
        tb.set_ruler_style(self.ruler_color, self.ruler_alpha);
        tb.set_occurrence_highlight(self.occurrence_hl);
        tb.set_text_inset(self.text_inset);
        tb.set_whitespace(self.whitespace);
        tb.set_indent(self.indent.0, self.indent.1);
        tb.set_tab_stops(self.tab_stops);
        tb.set_scroll_snap(self.scroll_snap);
        // 편집기는 거의 항상 포커스라 링이 늘 보여 거슬린다(사용자 09-16) — 캐럿만으로 충분.
        tb.set_focus_ring(false);
        tb
    }

    /// 안내선 표시·색·투명도 · 동일 출현 외곽선(설정 4종 · 전 탭).
    pub(crate) fn set_ruler_style(
        &mut self,
        show: bool,
        color: Option<nexa_ctl::theme::Color>,
        alpha: f32,
        occurrence: bool,
    ) {
        self.rulers_show = show;
        self.ruler_color = color;
        self.ruler_alpha = alpha;
        self.occurrence_hl = occurrence;
        for b in &mut self.bufs {
            b.set_rulers_visible(show);
            b.set_ruler_style(color, alpha);
            b.set_occurrence_highlight(occurrence);
        }
    }

    /// 첫 글자 앞 여백(설정 `editor.text_pad_left` · 전 탭).
    pub(crate) fn set_text_inset(&mut self, px: i32) {
        self.text_inset = px;
        for b in &mut self.bufs {
            b.set_text_inset(px);
        }
    }

    /// 세로 안내선 열 목록(설정 `editor.rulers`).
    pub(crate) fn set_rulers(&mut self, cols: Vec<usize>) {
        self.rulers = cols;
        for b in &mut self.bufs {
            b.set_rulers(self.rulers.clone());
        }
    }

    /// 공백 표시 스타일(설정 `editor.whitespace*`).
    pub(crate) fn set_whitespace(&mut self, ws: WhitespaceStyle) {
        self.whitespace = ws;
        for b in &mut self.bufs {
            b.set_whitespace(ws);
        }
    }

    /// 활성 탭의 구문 이름.
    pub(crate) fn syntax_name(&self) -> String {
        self.syntax
            .get(self.active)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    /// 활성 탭 구문 변경(팔레트 `Set Syntax`). 모르는 이름이면 false.
    pub(crate) fn set_syntax(&mut self, name: &str) -> bool {
        let Some(spec) = self.registry.get(name) else {
            return false;
        };
        if let Some(slot) = self.syntax.get_mut(self.active) {
            *slot = spec.clone();
        }
        self.cur_mut().set_highlighter(Some(spec));
        true
    }

    /// 일반 선택(구간 1개)의 요약 — (걸친 줄 수, 문자 수). 없거나 비었으면 None(상태줄 Sublime식 · 사용자 09-16).
    pub(crate) fn selection_summary(&self) -> Option<(usize, usize)> {
        let tb = self.cur();
        let (a, b) = tb.selection()?;
        if a == b {
            return None;
        }
        let text = tb.text();
        let sel: String = text.chars().skip(a).take(b - a).collect();
        let lines = sel.split('\n').count();
        Some((lines, sel.chars().count()))
    }

    /// 캐럿 위치(1-기준 줄 · 열).
    pub(crate) fn caret_line_col(&self) -> (usize, usize) {
        let tb = self.cur();
        let text = tb.text();
        let caret = tb.caret();
        let mut line = 1;
        let mut col = 1;
        for (i, c) in text.chars().enumerate() {
            if i >= caret {
                break;
            }
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// 설정 화면(T-39)에서 바꿀 때 — 지금은 부팅 값만.
    #[allow(dead_code)]
    pub(crate) fn set_line_numbers(&mut self, on: bool) {
        self.line_numbers = on;
        for b in &mut self.bufs {
            b.set_line_numbers(on);
        }
    }

    /// 들여쓰기 **기본값**(설정) — 재정의가 없는 탭에 적용하고 새 탭의 시작값이 된다.
    /// 탭/공백 적용 방식(설정 `editor.tab_stops` · 모든 탭 공통).
    pub(crate) fn set_tab_stops(&mut self, on: bool) {
        self.tab_stops = on;
        for b in &mut self.bufs {
            b.set_tab_stops(on);
        }
    }

    /// 휠 스크롤 단위(설정 `editor.scroll` · row = 줄 경계에 맞춤 · pixel = 부드럽게) — 전 탭 + 새 탭.
    pub(crate) fn set_scroll_snap(&mut self, on: bool) {
        self.scroll_snap = on;
        for b in &mut self.bufs {
            b.set_scroll_snap(on);
        }
    }

    pub(crate) fn set_indent(&mut self, tab_size: u8, spaces: bool) {
        self.indent = (tab_size, spaces);
        for (i, b) in self.bufs.iter_mut().enumerate() {
            if self.indents.get(i).copied().flatten().is_none() {
                b.set_indent(tab_size, spaces);
            }
        }
    }

    /// 활성 탭만 들여쓰기 재정의(상태줄 팝업) — 붙여넣기의 탭→공백 변환도 탭마다 따로 간다.
    pub(crate) fn set_tab_indent(&mut self, tab_size: u8, spaces: bool) {
        let i = self.active;
        if let Some(slot) = self.indents.get_mut(i) {
            *slot = Some((tab_size, spaces));
        }
        self.cur_mut().set_indent(tab_size, spaces);
    }

    /// 열(블록) 선택 모드 — 수식키 상태를 전 탭에 전달(활성 탭이 바뀌어도 일관).
    pub(crate) fn set_column_mode(&mut self, on: bool) {
        for b in &mut self.bufs {
            b.set_column_mode(on);
        }
    }

    /// 활성 탭의 선택 구간 수(다중 선택 표시).
    pub(crate) fn selection_count(&self) -> usize {
        self.cur().selection_count()
    }

    /// 활성 탭의 실제 들여쓰기(탭 폭 · 공백 여부) — 상태줄 세그먼트·팝업 표시 근거.
    pub(crate) fn indent(&self) -> (u8, bool) {
        self.indents
            .get(self.active)
            .copied()
            .flatten()
            .unwrap_or(self.indent)
    }

    /// 활성 탭 본문의 줄머리 들여쓰기 변환.
    pub(crate) fn convert_indent(&mut self, to_spaces: bool) {
        self.cur_mut().convert_indent(to_spaces);
    }

    #[allow(dead_code)]
    /// 설정 `tabs.rows`(single/multi) 즉시 반영(사용자 09-16: 바꿔도 반영이 안 됐다 — 시작 때만 읽었다).
    pub(crate) fn set_multiline_tabs(&mut self, on: bool) {
        self.tabs.set_multiline(on);
    }

    pub(crate) fn set_tooltip(&mut self, on: bool) {
        self.tooltip_on = on;
    }

    pub(crate) fn set_conn_desc(&mut self, d: impl Into<String>) {
        self.conn_desc = d.into();
    }

    /// 탭이 하나도 없으면 새 탭을 만든다(접속 성공을 활성 탭에 적용할 때 · 사용자 09-14).
    pub(crate) fn ensure_tab(&mut self) {
        if self.bufs.is_empty() {
            self.new_tab(None);
        }
    }

    pub(crate) fn cur(&self) -> &TextBox {
        &self.bufs[self.active]
    }

    pub(crate) fn cur_mut(&mut self) -> &mut TextBox {
        &mut self.bufs[self.active]
    }

    pub(crate) fn editor_bounds(&self) -> Rect {
        self.cur().bounds()
    }

    /// 새 탭(제목 없으면 `Script_N`). 활성으로.
    pub(crate) fn active(&self) -> usize {
        self.active
    }

    pub(crate) fn len(&self) -> usize {
        self.bufs.len()
    }

    pub(crate) fn new_tab(&mut self, title: Option<String>) {
        self.counter += 1;
        let title = title.unwrap_or_else(|| format!("Script_{}", self.counter));
        let syntax = self.registry.for_title(&title);
        let tb = self.make_box("", &syntax);
        self.bufs.push(tb);
        self.indents.push(None);
        self.paths.push(None);
        self.saved.push(String::new());
        self.eol.push(self.default_eol);
        self.saved_eol.push(self.default_eol);
        self.encs.push("utf8".into());
        self.syntax.push(syntax);
        self.titles.push(title);
        self.ids.push(self.next_id);
        self.next_id += 1;
        self.active = self.bufs.len() - 1;
        self.sync_tabs();
    }

    /// 탭의 안정 id.
    pub(crate) fn tab_id(&self, i: usize) -> u64 {
        self.ids.get(i).copied().unwrap_or(0)
    }

    /// 활성 탭의 안정 id.
    pub(crate) fn active_id(&self) -> u64 {
        self.tab_id(self.active)
    }

    /// 살아 있는 탭 id 전부(닫힌 탭의 결과 그리드 회수용).
    pub(crate) fn tab_ids(&self) -> Vec<u64> {
        self.ids.clone()
    }

    // ───────────────────────── 파일(T-74) ─────────────────────────

    /// 탭 `i`가 마지막 열기/저장 뒤 바뀌었나.
    pub(crate) fn is_dirty(&self, i: usize) -> bool {
        match (self.bufs.get(i), self.saved.get(i)) {
            (Some(b), Some(s)) => b.text() != *s || self.eol.get(i) != self.saved_eol.get(i),
            _ => false,
        }
    }

    /// 활성 탭의 파일 경로.
    pub(crate) fn active_path(&self) -> Option<PathBuf> {
        self.paths.get(self.active).cloned().flatten()
    }

    /// 활성 탭 제목(저장 대화상자 기본 이름).
    pub(crate) fn active_title(&self) -> String {
        self.titles.get(self.active).cloned().unwrap_or_default()
    }

    /// 활성 탭의 인코딩.
    pub(crate) fn active_encoding(&self) -> String {
        self.encs
            .get(self.active)
            .cloned()
            .unwrap_or_else(|| "utf8".into())
    }

    /// 활성 탭 인코딩 지정(열기 감지 · 저장 선택).
    pub(crate) fn set_active_encoding(&mut self, enc: &str) {
        if let Some(e) = self.encs.get_mut(self.active) {
            *e = enc.to_string();
        }
    }

    /// 새 탭의 줄끝 기본(설정 `file.eol_new`).
    pub(crate) fn set_default_eol(&mut self, eol: Eol) {
        self.default_eol = eol;
    }

    /// 활성 탭의 줄끝 변경(상태줄 세그먼트 · 저장 때 반영 · 저장 시점과 다르면 더러움).
    pub(crate) fn set_active_eol(&mut self, eol: Eol) {
        if let Some(c) = self.eol.get_mut(self.active) {
            *c = eol;
        }
    }

    /// 활성 탭의 줄끝.
    pub(crate) fn active_eol(&self) -> Eol {
        self.eol.get(self.active).copied().unwrap_or(Eol::os())
    }

    /// 파일을 탭에 연다 — 이미 열린 파일이면 그 탭으로 · 활성 탭이 빈 새 스크립트면 그 탭을 재사용 · 아니면 새 탭.
    pub(crate) fn open_file(&mut self, path: &Path, text: &str, eol: Eol) {
        if let Some(i) = self.paths.iter().position(|p| p.as_deref() == Some(path)) {
            self.switch(i);
            return;
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let reuse = self.paths.get(self.active).is_some_and(Option::is_none)
            && self.cur().text().is_empty()
            && !self.is_dirty(self.active);
        if !reuse {
            self.new_tab(Some(name.clone()));
        }
        let i = self.active;
        let syntax = self.registry.for_title(&name);
        let focused = self.cur().is_focused();
        let mut tb = self.make_box(text, &syntax);
        tb.set_focused(focused);
        if let Some((ts, sp)) = self.indents.get(i).copied().flatten() {
            tb.set_indent(ts, sp);
        }
        let mut inv = Invalidations::default();
        tb.set_bounds(self.editor_bounds(), &mut inv);
        self.bufs[i] = tb;
        self.syntax[i] = syntax;
        self.titles[i] = name;
        self.paths[i] = Some(path.to_path_buf());
        self.saved[i] = text.to_string();
        self.eol[i] = eol;
        self.saved_eol[i] = eol;
        self.sync_tabs();
    }

    /// 활성 탭을 `path`에 저장한 뒤 — 경로·제목·스냅샷 갱신(구문은 새 확장자 기준).
    pub(crate) fn mark_saved(&mut self, path: &Path) {
        let i = self.active;
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if self.titles[i] != name {
            self.titles[i] = name.clone();
            let syntax = self.registry.for_title(&name);
            self.cur_mut().set_highlighter(Some(syntax.clone()));
            self.syntax[i] = syntax;
        }
        self.paths[i] = Some(path.to_path_buf());
        self.saved[i] = self.cur().text();
        self.saved_eol[i] = self.eol[i];
        self.sync_tabs();
    }

    /// 표시 제목(더러우면 `*` 접두) — 탭 바에 바뀐 것만 보낸다. 바뀌었으면 true(호스트가 다시 그린다).
    pub(crate) fn refresh_dirty(&mut self) -> bool {
        let shown: Vec<String> = (0..self.titles.len())
            .map(|i| self.shown_title(i))
            .collect();
        if shown == self.shown_titles {
            return false;
        }
        self.sync_tabs();
        true
    }

    fn shown_title(&self, i: usize) -> String {
        if self.is_dirty(i) {
            format!("*{}", self.titles[i])
        } else {
            self.titles[i].clone()
        }
    }

    /// 1회성 안내(상태줄).
    pub(crate) fn take_notice(&mut self) -> Option<Msg> {
        self.notice.take()
    }

    pub(crate) fn close_tab(&mut self, i: usize) {
        if i >= self.bufs.len() {
            return;
        }
        // ★ 저장하지 않은 변경 = 2단 닫기(같은 탭을 3초 안에 다시 닫으면 버린다 · Delete 2단과 같은 관례).
        if self.is_dirty(i) {
            let again = matches!(self.pending_close, Some((j, t)) if j == i && t.elapsed() <= CLOSE_CONFIRM);
            if !again {
                self.pending_close = Some((i, Instant::now()));
                self.notice = Some(Msg::StUnsavedCloseAgain);
                return;
            }
        }
        self.pending_close = None;
        if self.bufs.len() <= 1 {
            // 마지막 탭은 비우기만(제목 없는 새 스크립트로).
            if let Some(b) = self.bufs.get_mut(i) {
                b.set_text("");
            }
            self.paths[i] = None;
            self.saved[i] = String::new();
            self.eol[i] = self.default_eol;
            self.saved_eol[i] = self.default_eol;
            self.counter += 1;
            self.titles[i] = format!("Script_{}", self.counter);
            // 새 스크립트가 됐으니 id도 새로(짝 결과 그리드 비움).
            self.ids[i] = self.next_id;
            self.next_id += 1;
            self.sync_tabs();
            return;
        }
        self.bufs.remove(i);
        self.titles.remove(i);
        self.syntax.remove(i);
        self.paths.remove(i);
        self.saved.remove(i);
        self.eol.remove(i);
        self.saved_eol.remove(i);
        self.encs.remove(i);
        self.ids.remove(i);
        if i < self.indents.len() {
            self.indents.remove(i);
        }
        if self.active >= self.bufs.len() {
            self.active = self.bufs.len() - 1;
        } else if i < self.active {
            self.active -= 1;
        }
        self.sync_tabs();
    }

    /// 탭 메뉴용 목록 — (id · 제목 · 활성) 탭 순서대로.
    pub(crate) fn tab_list(&self) -> Vec<(u64, String, bool)> {
        self.titles
            .iter()
            .enumerate()
            .map(|(i, t)| (self.tab_id(i), t.clone(), i == self.active))
            .collect()
    }

    /// 안정 id로 탭 전환(탭 메뉴 · 없으면 무시).
    pub(crate) fn switch_to_id(&mut self, id: u64) {
        if let Some(i) = self.ids.iter().position(|x| *x == id) {
            self.switch(i);
        }
    }

    pub(crate) fn switch(&mut self, i: usize) {
        if i < self.bufs.len() {
            let focused = self.cur().is_focused();
            self.cur_mut().set_focused(false);
            self.active = i;
            self.cur_mut().set_focused(focused);
            self.sync_tabs();
        }
    }

    fn sync_tabs(&mut self) {
        let mut inv = Invalidations::default();
        let shown: Vec<String> = (0..self.titles.len())
            .map(|i| self.shown_title(i))
            .collect();
        self.tabs.set_tabs(shown.clone(), self.active, &mut inv);
        self.shown_titles = shown;
        self.layout(&mut inv);
    }

    /// 언어 전환 등으로 placeholder를 다시 만들 때 — 본문 보존 재생성.
    pub(crate) fn rebuild_boxes(&mut self) {
        let texts: Vec<String> = self.bufs.iter().map(TextBox::text).collect();
        let focused = self.cur().is_focused();
        self.bufs = texts
            .iter()
            .zip(self.syntax.iter())
            .map(|(s, syn)| self.make_box(s, syn))
            .collect();
        for (i, b) in self.bufs.iter_mut().enumerate() {
            if let Some((ts, sp)) = self.indents.get(i).copied().flatten() {
                b.set_indent(ts, sp);
            }
        }
        self.cur_mut().set_focused(focused);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        self.tabs.set_scale(scale);
        for tb in &mut self.bufs {
            tb.set_scale(scale);
        }
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    fn layout(&mut self, inv: &mut Invalidations) {
        let b = self.bounds;
        // 논리 px(nexa-ctl 규약) → 물리 px(맥 2x 실기 09-16: 탭 줄이 Windows의 절반 높이였다).
        let th = (self.tabs.preferred_height() as f32 * self.scale)
            .round()
            .max(1.0) as i32;
        self.tabs.set_bounds(Rect::new(b.x, b.y, b.w, th), inv);
        let ed = Rect::new(b.x, b.y + th, b.w, (b.h - th).max(0));
        for tb in &mut self.bufs {
            tb.set_bounds(ed, inv);
        }
    }

    /// 탭 줄 수가 페인트에서 바뀌면(멀티라인 접힘) 다시 배치.
    pub(crate) fn relayout_if_needed(&mut self) -> bool {
        if self.tabs.take_lines_changed() {
            let mut inv = Invalidations::default();
            self.layout(&mut inv);
            return true;
        }
        false
    }

    /// 탭 바 이벤트(마우스가 탭 영역에 있거나 드래그 중). 소비했으면 true.
    pub(crate) fn route_tabs(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> bool {
        let p = match *ev {
            InputEvent::MouseMove { x, y }
            | InputEvent::MouseDown { x, y, .. }
            | InputEvent::MouseUp { x, y }
            | InputEvent::RightDown { x, y } => Some(Point { x, y }),
            _ => None,
        };
        if let Some(p) = p {
            self.cursor = (p.x, p.y);
            let over = self.tabs.bounds().contains(p);
            // 툴팁 호버 추적
            let idx = if over && self.tooltip_on {
                self.tabs.tab_index_at(p.x, p.y)
            } else {
                None
            };
            match (idx, self.hover) {
                (Some(i), Some((h, _))) if h == i => {}
                (Some(i), _) => self.hover = Some((i, Instant::now())),
                (None, _) => self.hover = None,
            }
            if !over && self.tabs.dragging().is_none() {
                return false;
            }
            if !matches!(ev, InputEvent::MouseMove { .. }) {
                self.hover = None;
            }
        } else {
            let is_wheel = matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
            let over = self.tabs.bounds().contains(Point {
                x: self.cursor.0,
                y: self.cursor.1,
            });
            if !is_wheel || !over {
                return false;
            }
        }
        self.tabs.on_event(ev, inv);
        if let Some(a) = self.tabs.take_action() {
            match a {
                TabAction::Switch(i) => self.switch(i),
                TabAction::Close(i) => self.close_tab(i),
                TabAction::New => self.new_tab(None),
                TabAction::Move { from, to } => {
                    if from < self.bufs.len() && to < self.bufs.len() {
                        let b = self.bufs.remove(from);
                        let t = self.titles.remove(from);
                        let sy = self.syntax.remove(from);
                        let id = self.indents.remove(from);
                        let pa = self.paths.remove(from);
                        let sv = self.saved.remove(from);
                        let cr = self.eol.remove(from);
                        let scr = self.saved_eol.remove(from);
                        let en = self.encs.remove(from);
                        self.bufs.insert(to, b);
                        self.titles.insert(to, t);
                        self.syntax.insert(to, sy);
                        self.indents.insert(to, id);
                        self.paths.insert(to, pa);
                        self.saved.insert(to, sv);
                        self.eol.insert(to, cr);
                        self.saved_eol.insert(to, scr);
                        self.encs.insert(to, en);
                        self.active = to;
                        self.sync_tabs();
                    }
                }
                TabAction::Context(_) => {}
            }
        }
        inv.push(self.bounds);
        true
    }

    /// 툴팁 타이머 — 카드가 뜰 시각이 되면 true(호스트가 다시 그린다).
    pub(crate) fn tick(&self) -> bool {
        matches!(self.hover, Some((_, t)) if t.elapsed().as_millis() >= HOVER_MS && t.elapsed().as_millis() < HOVER_MS + 40)
    }

    pub(crate) fn tooltip_pending(&self) -> bool {
        self.tooltip_on && self.hover.is_some()
    }

    pub(crate) fn paint_tabs(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.tabs.paint(dc, th);
    }

    /// 툴팁 카드(최상위 — 호스트가 맨 마지막에 부른다).
    pub(crate) fn paint_tooltip(&self, dc: &mut dyn DrawCtx, th: &Theme, clamp_w: i32) {
        let Some((i, since)) = self.hover else { return };
        if !self.tooltip_on || since.elapsed().as_millis() < HOVER_MS {
            return;
        }
        let Some(r) = self.tabs.tab_rect(i) else {
            return;
        };
        let text = self.bufs[i].text();
        let stmts = nsql_script::split_script(&text).len();
        let lines = text.split('\n').count();
        let head = match self.paths.get(i).and_then(|p| p.as_ref()) {
            Some(p) => format!("{}\n{}", self.titles[i], p.display()),
            None => self.titles[i].clone(),
        };
        let mut card = format!(
            "{}\n{}: {} · {}: {} · {}: {}",
            head,
            t(Msg::TipStatements),
            stmts,
            t(Msg::TipLines),
            lines,
            t(Msg::TipChars),
            text.chars().count()
        );
        card.push_str(&format!(
            "\n{}: {}",
            t(Msg::PalSetSyntax),
            self.syntax.get(i).map(|s| s.name.as_str()).unwrap_or("")
        ));
        if !self.conn_desc.is_empty() {
            card.push('\n');
            card.push_str(t(Msg::TipConnection));
            card.push_str(": ");
            card.push_str(&self.conn_desc);
        }
        draw_tooltip(dc, th, r, clamp_w, &card, self.scale);
    }
}
