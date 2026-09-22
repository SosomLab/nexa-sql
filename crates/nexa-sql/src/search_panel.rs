//! 파일 검색 패널(T-81a · docs/36 §1 · VS Code Search 차용 · 사용자 09-15/16) — 활동 막대 두 번째 패널.
//!
//! ```text
//! [🔍 검색어                Aa ab .*]   ← Enter = 검색 · 토글은 찾기 위젯과 같은 부품(FindBtn)
//! [Where: <open files>, D:\a, -*.log ]   ← 비면 열린 탭 + 활성 파일 폴더 · `-` = 제외
//!  n files · m matches · 0.42s          ← 진행/결과 · 검색 중이면 취소 가능(Esc)
//!  ▾ Script_3 (메모리 탭)            2  ← 파일 행(접기/펼치기 · 일치 수)
//!      12:  SELECT * FROM M4S_I002040 A ← 일치 행(줄 번호 · 일치 구간 강조) · 클릭 = 열고 그 자리로
//! ```
//! 엔진 = `nsql-search`(스트리밍 배치 · 취소) · 열린 탭은 `search_text`로 즉시(저장 안 된 본문 우선 · 같은 경로 파일은 중복 제외).
//! 호스트가 하는 것: Enter → [`SearchPanel::take_request`] → 열린 탭·활성 폴더·설정을 모아 [`SearchPanel::start`] ·
//! 매 틱 [`SearchPanel::poll`] · 클릭 → [`SearchPanel::take_open`]으로 파일/탭을 열고 줄로 이동.

use crate::findbar::{BtnKind, FindBtn};
use crate::toolicons;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};
use nsql_search::{search_text, Batch, CancelHandle, Matcher, Search, SearchOpts};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Instant;

/// Where 상자 해석 결과.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct WhereSpec {
    /// 폴더·파일 루트.
    pub roots: Vec<PathBuf>,
    /// `-` 접두 제외 패턴(gitignore 문법).
    pub excludes: Vec<String>,
    /// 열린 탭(메모리 본문)도 검색.
    pub open_tabs: bool,
}

/// Where 문자열 해석 — 콤마 구분 · `<open files>` · `<current file>`(활성 파일 폴더) · `-패턴` = 제외 · 그 밖 = 경로.
/// 비어 있으면 열린 탭 + 활성 파일 폴더.
pub(crate) fn parse_where(text: &str, current_dir: Option<&Path>) -> WhereSpec {
    let mut spec = WhereSpec::default();
    let items: Vec<&str> = text
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        spec.open_tabs = true;
        if let Some(d) = current_dir {
            spec.roots.push(d.to_path_buf());
        }
        return spec;
    }
    for it in items {
        let low = it.to_ascii_lowercase();
        if low == "<open files>" || low == "<open tabs>" {
            spec.open_tabs = true;
        } else if low == "<current file>" || low == "<current folder>" {
            if let Some(d) = current_dir {
                spec.roots.push(d.to_path_buf());
            }
        } else if let Some(pat) = it.strip_prefix('-') {
            if !pat.is_empty() {
                spec.excludes.push(pat.to_string());
            }
        } else {
            let p = PathBuf::from(it.trim_matches('"'));
            spec.roots.push(p);
        }
    }
    spec
}

/// 검색 시작에 필요한 호스트 컨텍스트.
pub(crate) struct SearchCtx {
    /// 열린 탭 (id · 라벨 · 본문 · 경로).
    pub tabs: Vec<(u64, String, String, Option<PathBuf>)>,
    pub current_dir: Option<PathBuf>,
    pub max_file_kb: usize,
    pub threads: usize,
    pub gitignore: bool,
    /// 설정 `search.excludes`(콤마 구분).
    pub excludes: Vec<String>,
}

/// 결과를 열 때 호스트에 넘기는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenReq {
    pub path: Option<PathBuf>,
    pub tab: Option<u64>,
    /// 1 기준 줄 · 0 기준 문자 열 · 강조 길이(문자).
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

struct MatchRow {
    line_no: usize,
    /// 문자 열(0 기준)과 길이(문자).
    col: usize,
    len: usize,
    text: String,
}

struct FileEntry {
    path: Option<PathBuf>,
    tab: Option<u64>,
    label: String,
    expanded: bool,
    matches: Vec<MatchRow>,
}

/// 평탄화한 표시 행.
#[derive(Clone, Copy)]
enum Row {
    File(usize),
    Match(usize, usize),
}

pub(crate) struct SearchPanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    query: TextBox,
    where_box: TextBox,
    btns: Vec<FindBtn>,
    files: Vec<FileEntry>,
    rows: Vec<Row>,
    job: Option<(Receiver<Batch>, CancelHandle, Instant)>,
    status: String,
    error: Option<String>,
    scroll_y: i32,
    bars: ScrollBars,
    sel: Option<usize>,
    hover: Option<usize>,
    /// 배치 영역(페인트가 갱신).
    list_rect: Rect,
    row_h: i32,
    request: bool,
    open: Option<OpenReq>,
    /// 파일 행 클릭 = 접기/펼치기 대상(1회성).
    tooltip_ms: u128,
    clamp_w: i32,
    /// 마지막 검색어(같은 경로 파일은 열린 탭 우선 중복 제외).
    tab_paths: Vec<PathBuf>,
}

const ROW_H: f32 = 22.0;
const INPUT_H: f32 = 25.0;
const PAD: f32 = 8.0;

impl SearchPanel {
    pub(crate) fn new() -> Self {
        let mut query = TextBox::new(t(Msg::PhSearchQuery));
        query.set_focus_ring(false);
        let mut where_box = TextBox::new(t(Msg::PhSearchWhere));
        where_box.set_focus_ring(false);
        SearchPanel {
            visible: false,
            bounds: Rect::default(),
            scale: 1.0,
            query,
            where_box,
            btns: vec![
                FindBtn::new(BtnKind::Case, toolicons::mi_match_case),
                FindBtn::new(BtnKind::Word, toolicons::mi_match_word),
                FindBtn::new(BtnKind::Regex, toolicons::mi_regex),
            ],
            files: Vec::new(),
            rows: Vec::new(),
            job: None,
            status: String::new(),
            error: None,
            scroll_y: 0,
            bars: ScrollBars::new(),
            sel: None,
            hover: None,
            list_rect: Rect::default(),
            row_h: 22,
            request: false,
            open: None,
            tooltip_ms: 600,
            clamp_w: i32::MAX / 2,
            tab_paths: Vec::new(),
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.set_focused(false);
        }
    }

    pub(crate) fn bounds(&self) -> Rect {
        if self.visible {
            self.bounds
        } else {
            Rect::default()
        }
    }

    pub(crate) fn set_tooltip_delay(&mut self, ms: u128) {
        self.tooltip_ms = ms;
    }

    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
    }

    /// 검색 중인가(취소 가능).
    pub(crate) fn searching(&self) -> bool {
        self.job.is_some()
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        if on {
            if !self.where_box.is_focused() {
                self.query.set_focused(true);
            }
        } else {
            self.query.set_focused(false);
            self.where_box.set_focused(false);
            for b in &mut self.btns {
                b.focused = false;
            }
        }
    }

    /// 열기(Ctrl+Shift+F) — 검색어 상자에 포커스 · 씨앗.
    pub(crate) fn focus_query(&mut self, seed: Option<String>) {
        if let Some(s) = seed.filter(|s| !s.is_empty() && !s.contains('\n')) {
            self.query.set_text(&s);
        }
        self.query.set_focused(true);
        self.where_box.set_focused(false);
    }

    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.where_box.is_focused() {
            Some(&mut self.where_box)
        } else if self.query.is_focused() {
            Some(&mut self.query)
        } else {
            None
        }
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let s = scale;
        let px = |v: f32| (v * s).round() as i32;
        let pad = px(PAD);
        let ih = px(INPUT_H);
        let mut inv = Invalidations::default();
        let y1 = b.y + pad;
        let tg = px(20.0);
        let tgap = px(2.0);
        // 검색어 상자 = 폭 전체 · 안쪽 오른쪽 토글 3개(찾기 위젯과 같은 배치).
        let frame = Rect::new(b.x + pad, y1, (b.w - pad * 2).max(px(80.0)), ih);
        let mut tx = frame.right() - px(2.0) - tg;
        for k in [BtnKind::Regex, BtnKind::Word, BtnKind::Case] {
            if let Some(btn) = self.btns.iter_mut().find(|x| x.kind == k) {
                btn.rect = Rect::new(tx, y1 + px(3.0), tg, tg);
            }
            tx -= tg + tgap;
        }
        self.query.set_scale(s);
        self.query.set_bounds(
            Rect::new(frame.x, y1, (tx + tg + tgap - frame.x).max(px(40.0)), ih),
            &mut inv,
        );
        let y2 = y1 + ih + px(4.0);
        self.where_box.set_scale(s);
        self.where_box
            .set_bounds(Rect::new(frame.x, y2, frame.w, ih), &mut inv);
        self.row_h = px(ROW_H);
        let list_top = y2 + ih + px(4.0) + self.row_h; // 상태 한 줄
        self.list_rect = Rect::new(b.x, list_top, b.w, (b.bottom() - list_top).max(0));
    }

    /// 사용자가 Enter로 검색을 요청했다(1회성).
    pub(crate) fn take_request(&mut self) -> bool {
        std::mem::take(&mut self.request)
    }

    pub(crate) fn take_open(&mut self) -> Option<OpenReq> {
        self.open.take()
    }

    fn case(&self) -> bool {
        self.btns
            .iter()
            .any(|b| b.kind == BtnKind::Case && b.checked)
    }
    fn word(&self) -> bool {
        self.btns
            .iter()
            .any(|b| b.kind == BtnKind::Word && b.checked)
    }
    fn regex(&self) -> bool {
        self.btns
            .iter()
            .any(|b| b.kind == BtnKind::Regex && b.checked)
    }

    /// 검색 시작 — 열린 탭은 즉시, 폴더는 스트리밍.
    pub(crate) fn start(&mut self, ctx: SearchCtx) {
        self.cancel();
        self.files.clear();
        self.rows.clear();
        self.sel = None;
        self.scroll_y = 0;
        self.error = None;
        self.tab_paths.clear();
        let q = self.query.text();
        if q.trim().is_empty() {
            self.status.clear();
            return;
        }
        let spec = parse_where(&self.where_box.text(), ctx.current_dir.as_deref());
        let matcher = match Matcher::new(&q, self.case(), self.word(), self.regex()) {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                self.status = tf(Msg::StSearchError, &[&e.to_string()]);
                return;
            }
        };
        let t0 = Instant::now();
        let mut tab_matches = 0usize;
        if spec.open_tabs {
            for (id, label, text, path) in &ctx.tabs {
                let ms = search_text(text, label, &matcher);
                if let Some(p) = path {
                    self.tab_paths.push(p.clone());
                }
                if ms.is_empty() {
                    continue;
                }
                tab_matches += ms.len();
                self.files.push(FileEntry {
                    path: path.clone(),
                    tab: Some(*id),
                    label: match path {
                        Some(p) => nexa_fs::path::display(p),
                        None => format!("{label} ({})", t(Msg::SearchMemoryTab)),
                    },
                    expanded: true,
                    matches: ms
                        .iter()
                        .map(|m| MatchRow {
                            line_no: m.line_no,
                            col: byte_to_char_col(&m.line, m.span.0),
                            len: m.line[m.span.0..m.span.1].chars().count(),
                            text: m.line.to_string(),
                        })
                        .collect(),
                });
            }
        }
        if spec.roots.is_empty() {
            self.rebuild_rows();
            self.status = tf(
                Msg::StSearchDone,
                &[
                    &ctx.tabs.len().to_string(),
                    &tab_matches.to_string(),
                    &format!("{:.2}", t0.elapsed().as_secs_f32()),
                ],
            );
            return;
        }
        let mut excludes = ctx.excludes;
        excludes.extend(spec.excludes);
        let opts = SearchOpts {
            query: q,
            case: self.case(),
            word: self.word(),
            regex: self.regex(),
            roots: spec.roots,
            excludes,
            max_file_kb: ctx.max_file_kb,
            threads: ctx.threads,
            gitignore: ctx.gitignore,
        };
        match Search::spawn(opts) {
            Ok((rx, cancel)) => {
                self.job = Some((rx, cancel, t0));
                self.status = t(Msg::StSearching).into();
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.status = tf(Msg::StSearchError, &[&e.to_string()]);
            }
        }
        self.rebuild_rows();
    }

    pub(crate) fn cancel(&mut self) {
        if let Some((_, c, _)) = self.job.take() {
            c.cancel();
        }
    }

    /// 배치 수신(호스트 틱) — 새 결과가 있으면 `true`.
    pub(crate) fn poll(&mut self) -> bool {
        let Some((rx, _, t0)) = self.job.as_ref() else {
            return false;
        };
        let mut changed = false;
        let mut done = None;
        while let Ok(b) = rx.try_recv() {
            changed = true;
            match b {
                Batch::Matches(ms) => {
                    let Some(first) = ms.first() else { continue };
                    let path: PathBuf = first.path.to_path_buf();
                    // 열린 탭으로 이미 검색한 파일은 버퍼 결과를 남긴다(중복 제외).
                    if self.tab_paths.iter().any(|p| p == &path) {
                        continue;
                    }
                    let rows: Vec<MatchRow> = ms
                        .iter()
                        .map(|m| MatchRow {
                            line_no: m.line_no,
                            col: byte_to_char_col(&m.line, m.span.0),
                            len: m.line[m.span.0..m.span.1].chars().count(),
                            text: m.line.to_string(),
                        })
                        .collect();
                    if let Some(f) = self
                        .files
                        .iter_mut()
                        .find(|f| f.path.as_deref() == Some(&path))
                    {
                        f.matches.extend(rows);
                    } else {
                        self.files.push(FileEntry {
                            label: nexa_fs::path::display(&path),
                            path: Some(path),
                            tab: None,
                            expanded: true,
                            matches: rows,
                        });
                    }
                }
                Batch::Error { path, message } => {
                    self.error = Some(format!("{}: {message}", path.display()));
                }
                Batch::Done(p) => {
                    done = Some(p);
                }
            }
        }
        if let Some(p) = done {
            let secs = format!("{:.2}", t0.elapsed().as_secs_f32());
            let total: usize = self.files.iter().map(|f| f.matches.len()).sum();
            self.status = if total == 0 {
                tf(Msg::StSearchNone, &[&p.files_searched.to_string(), &secs])
            } else {
                tf(
                    Msg::StSearchDone,
                    &[&p.files_searched.to_string(), &total.to_string(), &secs],
                )
            };
            self.job = None;
        } else if changed {
            if let Some((_, c, _)) = self.job.as_ref() {
                let p = c.progress();
                let total: usize = self.files.iter().map(|f| f.matches.len()).sum();
                self.status = tf(
                    Msg::StSearchProgress,
                    &[&p.files_searched.to_string(), &total.to_string()],
                );
            }
        }
        if changed {
            self.rebuild_rows();
        }
        changed
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        for (fi, f) in self.files.iter().enumerate() {
            self.rows.push(Row::File(fi));
            if f.expanded {
                for mi in 0..f.matches.len() {
                    self.rows.push(Row::Match(fi, mi));
                }
            }
        }
        let max = (self.rows.len() as i32 * self.row_h - self.list_rect.h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if !self.visible {
            return false;
        }
        let mut any =
            self.query.tick(now_ms) | self.where_box.tick(now_ms) | self.bars.tick(now_ms);
        for b in &mut self.btns {
            any |= b.hover.tick(now_ms);
        }
        any
    }

    pub(crate) fn animating(&self) -> bool {
        self.visible
            && (self.query.is_animating()
                || self.where_box.is_animating()
                || self.btns.iter().any(|b| b.hover.is_animating())
                || self.job.is_some())
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list_rect.contains(p) || self.row_h <= 0 {
            return None;
        }
        let i = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h) as usize;
        (i < self.rows.len()).then_some(i)
    }

    fn activate_row(&mut self, i: usize) {
        match self.rows.get(i).copied() {
            Some(Row::File(fi)) => {
                if let Some(f) = self.files.get_mut(fi) {
                    f.expanded = !f.expanded;
                }
                self.rebuild_rows();
            }
            Some(Row::Match(fi, mi)) => {
                if let Some(f) = self.files.get(fi) {
                    if let Some(m) = f.matches.get(mi) {
                        self.open = Some(OpenReq {
                            path: f.path.clone(),
                            tab: f.tab,
                            line: m.line_no,
                            col: m.col,
                            len: m.len,
                        });
                    }
                }
            }
            None => {}
        }
    }

    /// 이벤트(마우스 = 패널 안 · 키 = 포커스일 때) — 다시 그려야 하면 `true`.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        // 목록 스크롤바(휠 포함).
        let content_h = self.rows.len() as i32 * self.row_h;
        let (_, ny, consumed) = self.bars.on_event(
            ev,
            self.list_rect,
            self.list_rect.w,
            content_h.max(self.list_rect.h),
            0,
            self.scroll_y,
            self.scale,
        );
        if ny != self.scroll_y {
            self.scroll_y = ny;
            return true;
        }
        if consumed {
            return true;
        }
        match *ev {
            InputEvent::Key { key, shift, .. } => {
                match key {
                    CtlKey::Enter => {
                        if self.query.is_focused() || self.where_box.is_focused() {
                            self.request = true;
                            return true;
                        }
                        if let Some(i) = self.sel {
                            self.activate_row(i);
                            return true;
                        }
                    }
                    CtlKey::Escape => {
                        if self.job.is_some() {
                            self.cancel();
                            self.status = t(Msg::StSearchCancelled).into();
                            return true;
                        }
                    }
                    CtlKey::Down | CtlKey::Up
                        if !self.query.is_focused() && !self.where_box.is_focused() =>
                    {
                        let n = self.rows.len();
                        if n > 0 {
                            let cur = self.sel.unwrap_or(0);
                            let next = if key == CtlKey::Down {
                                (cur + 1).min(n - 1)
                            } else {
                                cur.saturating_sub(1)
                            };
                            self.sel = Some(next);
                            self.ensure_visible(next);
                            return true;
                        }
                    }
                    _ => {}
                }
                let _ = shift;
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                let in_q = self.query.bounds().contains(p);
                let in_w = self.where_box.bounds().contains(p);
                self.query.set_focused(in_q);
                self.where_box.set_focused(in_w);
                let hit = self.btns.iter().position(|b| b.rect.contains(p));
                for (i, b) in self.btns.iter_mut().enumerate() {
                    b.focused = Some(i) == hit;
                }
                if let Some(i) = self.row_at(p) {
                    self.sel = Some(i);
                    self.activate_row(i);
                }
            }
            InputEvent::MouseMove { x, y } => {
                let h = self.row_at(Point { x, y });
                if h != self.hover {
                    self.hover = h;
                }
            }
            _ => {}
        }
        self.query.on_event(ev, &mut inv);
        self.where_box.on_event(ev, &mut inv);
        for b in &mut self.btns {
            b.on_event(ev);
        }
        let _ = self.query.take_changed();
        let _ = self.where_box.take_changed();
        let mut redraw = true;
        for b in &mut self.btns {
            if b.take_clicked() {
                b.checked = !b.checked;
                // 토글을 바꾸면 결과가 있을 때 다시 검색(다음 Enter 없이).
                if !self.query.text().trim().is_empty() {
                    self.request = true;
                }
                redraw = true;
            }
        }
        redraw
    }

    fn ensure_visible(&mut self, i: usize) {
        let top = i as i32 * self.row_h;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + self.list_rect.h {
            self.scroll_y = top + self.row_h - self.list_rect.h;
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let s = self.scale;
        let px = |v: f32| (v * s).round() as i32;
        let b = self.bounds;
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        dc.select_font(FontSlot::Base, false);
        // 검색어 틀(토글 포함) — 찾기 위젯과 같은 그림.
        let qb = self.query.bounds();
        let frame = Rect::new(qb.x, qb.y, b.w - px(PAD) * 2, qb.h);
        let r = px(6.0);
        dc.fill_round_rect(frame, r, th.field_bg);
        dc.stroke_round_rect(frame, r, th.border, 1.0);
        self.query.paint(dc, th);
        dc.fill_rect(
            Rect::new(qb.right() - 2, qb.y + r, 3, (qb.h - r * 2).max(0)),
            th.field_bg,
        );
        if self.query.is_focused() {
            dc.stroke_round_rect(frame, r, th.accent, 1.0);
        }
        for btn in &self.btns {
            btn.paint(dc, th, s);
        }
        self.where_box.paint(dc, th);
        // 상태 한 줄.
        dc.select_font(FontSlot::Status, false);
        let sy = self.where_box.bounds().bottom() + px(4.0);
        let color = if self.error.is_some() {
            th.danger
        } else {
            th.text_dim
        };
        let sty = dc.text_center_y(sy, self.row_h);
        dc.text(
            b.x + px(PAD),
            sty,
            Rect::new(b.x, sy, b.w, self.row_h),
            &self.status,
            color,
        );
        // 결과 목록.
        dc.select_font(FontSlot::Base, false);
        let lr = self.list_rect;
        let rh = self.row_h.max(1);
        let first = (self.scroll_y / rh) as usize;
        let mut y = lr.y - self.scroll_y % rh;
        let indent = px(PAD);
        let sel_w = dc.text_width("M");
        for i in first..self.rows.len() {
            if y >= lr.bottom() {
                break;
            }
            let row_rect = Rect::new(lr.x, y, lr.w, rh);
            if self.sel == Some(i) {
                dc.fill_rect(row_rect, th.sel_bg);
            } else if self.hover == Some(i) {
                dc.fill_rect_alpha(row_rect, th.text, 0.06);
            }
            let ty = dc.text_center_y(y, rh);
            match self.rows[i] {
                Row::File(fi) => {
                    let f = &self.files[fi];
                    let chev = if f.expanded { "▾" } else { "▸" };
                    dc.text(lr.x + indent, ty, row_rect, chev, th.text_dim);
                    let count = f.matches.len().to_string();
                    let cw = dc.text_width(&count);
                    let label_clip = Rect::new(lr.x, y, (lr.w - cw - indent * 3).max(0), rh);
                    let shown = nexa_ctl::draw::ellipsize_middle(
                        dc,
                        &f.label,
                        label_clip.right() - (lr.x + indent + sel_w),
                    );
                    dc.text(lr.x + indent + sel_w, ty, label_clip, &shown, th.text);
                    dc.text(lr.right() - indent - cw, ty, row_rect, &count, th.text_dim);
                }
                Row::Match(fi, mi) => {
                    let m = &self.files[fi].matches[mi];
                    let num = format!("{}:", m.line_no);
                    let nw = dc.text_width(&num);
                    let x0 = lr.x + indent * 2 + sel_w;
                    dc.text(x0, ty, row_rect, &num, th.text_dim);
                    let tx = x0 + nw + px(6.0);
                    let line = m.text.trim_end();
                    // 일치 구간 강조(반투명) — 앞 문맥이 길면 일치가 보이도록 왼쪽을 자른다.
                    let chars: Vec<char> = line.chars().collect();
                    let cut = m.col.saturating_sub(24);
                    let shown: String = chars.iter().skip(cut).collect();
                    let mut w = Vec::new();
                    dc.text_prefix_widths(&shown, &mut w);
                    let a = m.col - cut;
                    let e = (a + m.len).min(chars.len().saturating_sub(cut));
                    let hx0 = tx + w.get(a).copied().unwrap_or(0);
                    let hx1 = tx + w.get(e).copied().unwrap_or(0);
                    if hx1 > hx0 {
                        dc.fill_round_rect_alpha(
                            Rect::new(hx0, y + 2, (hx1 - hx0).min(lr.right() - hx0), rh - 4),
                            px(2.0),
                            th.warn,
                            0.35,
                        );
                    }
                    dc.text(
                        tx,
                        ty,
                        Rect::new(tx, y, (lr.right() - tx).max(0), rh),
                        &shown,
                        th.text,
                    );
                }
            }
            y += rh;
        }
        self.bars.paint(
            dc,
            th,
            lr,
            lr.w,
            (self.rows.len() as i32 * rh).max(lr.h),
            0,
            self.scroll_y,
            s,
        );
        self.query.paint_popup(dc, th);
        self.where_box.paint_popup(dc, th);
    }

    /// 토글 툴팁(팝업 층).
    pub(crate) fn paint_tooltip(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let Some(btn) = self.btns.iter().find(|b| {
            b.hover_on
                && b.hover_since
                    .is_some_and(|t0| t0.elapsed().as_millis() >= self.tooltip_ms)
        }) else {
            return;
        };
        let s = self.scale;
        dc.select_font(FontSlot::Status, false);
        let h = dc.text_height() + (8.0 * s).round() as i32;
        let anchor = Rect::new(
            btn.rect.x,
            btn.rect.y - h - (12.0 * s).round() as i32,
            btn.rect.w,
            0,
        );
        nexa_ctl::draw::draw_tooltip_in(dc, th, anchor, (0, self.clamp_w), t(btn.kind.tip()), s);
    }
}

/// 바이트 오프셋 → 문자 열(줄 안).
fn byte_to_char_col(line: &str, byte: usize) -> usize {
    line.get(..byte.min(line.len()))
        .map_or(0, |s| s.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_parsing_rules() {
        let cur = PathBuf::from("/proj/sql");
        let w = parse_where("", Some(&cur));
        assert!(w.open_tabs);
        assert_eq!(w.roots, vec![cur.clone()]);
        let w = parse_where("<open files>, /a/b, -*.log, \"/c d\"", Some(&cur));
        assert!(w.open_tabs);
        assert_eq!(w.roots, vec![PathBuf::from("/a/b"), PathBuf::from("/c d")]);
        assert_eq!(w.excludes, vec!["*.log".to_string()]);
        let w = parse_where("<current file>", None);
        assert!(!w.open_tabs);
        assert!(w.roots.is_empty());
        assert_eq!(byte_to_char_col("가나다x", 6), 2);
    }
}
