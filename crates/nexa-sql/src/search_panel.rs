//! 파일 검색 패널(T-81a · docs/36 §1 · VS Code Search 차용 · 사용자 09-15/16) — 활동 막대 두 번째 패널.
//!
//! ```text
//! [🔍 검색어                Aa ab .*]   ← Enter = 검색 · 토글은 찾기 위젯과 같은 부품(FindBtn)
//! [Where: <open files>, D:\a, -*.log ]   ← 비면 열린 탭 + 활성 파일 폴더 · `-` = 제외
//!  n files · m matches · 0.42s          ← 진행/결과 · 검색 중이면 취소 가능(Esc)
//!  ▾ Script_3 (메모리 탭)            2  ← 파일 행(접기/펼치기 · 일치 수)
//!      12:  SELECT * FROM DMS_I002040 A ← 일치 행(줄 번호 · 일치 구간 강조) · 클릭 = 열고 그 자리로
//! ```
//! 엔진 = `nsql-search`(스트리밍 배치 · 취소) · 열린 탭은 `search_text`로 즉시(저장 안 된 본문 우선 · 같은 경로 파일은 중복 제외).
//! 호스트가 하는 것: Enter → [`SearchPanel::take_request`] → 열린 탭·활성 폴더·설정을 모아 [`SearchPanel::start`] ·
//! 매 틱 [`SearchPanel::poll`] · 클릭 → [`SearchPanel::take_open`]으로 파일/탭을 열고 줄로 이동.

use crate::findbar::{BtnKind, FindBtn};
use crate::search_history::{Recall, RecallEvent, SharedHistory};
use crate::toolicons;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};
use nsql_search::{search_text, Batch, CancelHandle, Matcher, Search, SearchOpts, SkipReason};
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
    /// 포함 패턴(`*.sql` · `+패턴` · gitignore 문법 · 파일에만) — 있으면 하나라도 맞는 파일만(OR) · 제외는 그 뒤(AND NOT).
    pub includes: Vec<String>,
    /// 열린 탭(메모리 본문)도 검색.
    pub open_tabs: bool,
}

/// Where 문자열 해석 — 콤마 구분 · `<open files>` · `<current file>`(활성 파일 폴더) · `-패턴` = 제외 · **`*.sql`/`?`가 든 토큰이나
/// `+패턴` = 포함**(사용자 09-23 "특정 확장자만 조회 · 제외 · 2개 이상 조건" — `*.sql, *.txt, -*_test.sql, D:\proj` = sql 또는 txt이면서
/// _test가 아닌 파일을 D:\proj 아래에서) · 그 밖 = 경로. 조건은 몇 개든 콤마로 잇는다.
/// 비어 있으면 **열린 탭 + 작업 모드의 기본 범위**(`defaults` · 사용자 09-23): 파일 모드 = 열린 탭만 · 폴더 모드 = + 그 폴더 ·
/// 프로젝트 모드 = + 프로젝트 폴더와 프로젝트에 추가한 폴더(호스트가 [`SearchCtx::default_roots`]로 준다).
/// 패턴만 있고 경로가 없으면 기본 범위에 패턴을 얹는다.
pub(crate) fn parse_where(
    text: &str,
    current_dir: Option<&Path>,
    defaults: &[PathBuf],
) -> WhereSpec {
    let mut spec = WhereSpec::default();
    let items: Vec<&str> = text
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() {
        spec.open_tabs = true;
        spec.roots.extend(defaults.iter().cloned());
        return spec;
    }
    let mut any_scope = false;
    for it in items {
        let low = it.to_ascii_lowercase();
        if low == "<open files>" || low == "<open tabs>" {
            spec.open_tabs = true;
            any_scope = true;
        } else if low == "<current file>" || low == "<current folder>" {
            if let Some(d) = current_dir {
                spec.roots.push(d.to_path_buf());
            }
            any_scope = true;
        } else if let Some(pat) = it.strip_prefix('-') {
            if !pat.is_empty() {
                spec.excludes.push(pat.to_string());
            }
        } else if let Some(pat) = it.strip_prefix('+') {
            if !pat.is_empty() {
                spec.includes.push(pat.to_string());
            }
        } else if is_glob_token(it) {
            spec.includes.push(it.to_string());
        } else {
            let p = PathBuf::from(it.trim_matches('"'));
            spec.roots.push(p);
            any_scope = true;
        }
    }
    if !any_scope {
        // 패턴만 적었다 — 범위는 기본(열린 탭 + 작업 모드 폴더)에 패턴을 얹는다.
        spec.open_tabs = true;
        spec.roots.extend(defaults.iter().cloned());
    }
    spec
}

/// 글롭 토큰인가 — `*`/`?`가 들어 있고 드라이브·절대 경로 꼴이 아닌 것(`*.sql` · `src/**/*.txt` · `test?.sql`).
fn is_glob_token(s: &str) -> bool {
    if !(s.contains('*') || s.contains('?')) {
        return false;
    }
    let bytes = s.as_bytes();
    let drive = bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic();
    !(drive || s.starts_with('/') || s.starts_with('\\'))
}

/// 검색 시작에 필요한 호스트 컨텍스트.
pub(crate) struct SearchCtx {
    /// 열린 탭 (id · 라벨 · 본문 · 경로).
    pub tabs: Vec<(u64, String, String, Option<PathBuf>)>,
    pub current_dir: Option<PathBuf>,
    /// 범위 상자가 비었을 때의 폴더 루트(작업 모드별 · 파일 모드 = 없음 · 폴더 = 그 폴더 · 프로젝트 = 프로젝트 폴더 + 추가 폴더).
    pub default_roots: Vec<PathBuf>,
    pub max_file_kb: usize,
    pub threads: usize,
    pub gitignore: bool,
    /// 설정 `search.excludes`(콤마 구분).
    pub excludes: Vec<String>,
    /// ★ T-81e: UTF-8이 아닌 파일의 변환기(호스트 EUC-KR 등 · 없으면 바이트 그대로).
    pub transcode: Option<nsql_search::Transcoder>,
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
    /// 정식 탭으로(더블클릭 · Enter) · 아니면 미리보기 탭(프로젝트 탐색기와 같은 규칙 · 사용자 09-23).
    pub permanent: bool,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    File(usize),
    Match(usize, usize),
    /// 제외 로그 머리("제외됨(N)" · 접기/펼치기 · 사용자 09-23).
    SkipHeader,
    /// 제외 로그 한 줄(`skipped[i]` = 경로 — 이유).
    Skip(usize),
}

/// ←/→ 한 번이 할 일(사용자 10-20 "파일 검색에서 키보드 좌/우 접기·펼치기가 안 됨" · 북마크·프로젝트 탐색기와 같은 규칙).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FoldStep {
    /// 그 행으로 선택 이동.
    Select(usize),
    /// 그 행(파일/제외 머리)을 접거나 펼침.
    Toggle(usize),
    None,
}

/// ← = 자식 행 → 부모 행 · 펼친 부모 → 접기 · → = 접힌 부모 → 펼치기(순수 · `expanded` = 그 부모 행이 펼쳐졌는가).
fn fold_step(rows: &[Row], expanded: &dyn Fn(Row) -> bool, sel: usize, left: bool) -> FoldStep {
    let Some(&row) = rows.get(sel) else {
        return FoldStep::None;
    };
    let parent_of = |want: &dyn Fn(Row) -> bool| (0..sel).rev().find(|&k| want(rows[k]));
    match (row, left) {
        (Row::Match(fi, _), true) => {
            parent_of(&|r| r == Row::File(fi)).map_or(FoldStep::None, FoldStep::Select)
        }
        (Row::Skip(_), true) => {
            parent_of(&|r| r == Row::SkipHeader).map_or(FoldStep::None, FoldStep::Select)
        }
        (Row::File(_) | Row::SkipHeader, true) if expanded(row) => FoldStep::Toggle(sel),
        (Row::File(_) | Row::SkipHeader, false) if !expanded(row) => FoldStep::Toggle(sel),
        _ => FoldStep::None,
    }
}

pub(crate) struct SearchPanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    query: TextBox,
    where_box: TextBox,
    btns: Vec<FindBtn>,
    /// ★ T-81d(10-30 · 36 §1): 바꾸기 줄 — `›` 접기/펼치기(검색어 틀 왼쪽) · 바꾸기 상자 · [모두 바꾸기](틀 안 오른쪽).
    fold_btn: FindBtn,
    replace: TextBox,
    replace_open: bool,
    repl_btns: Vec<FindBtn>,
    /// [모두 바꾸기] 요청(바꿀 글 · 호스트가 `take_replace_all`로 가져가 적용).
    replace_req: Option<String>,
    /// 지금 결과를 만든 매처(미리보기 `→ 새 줄` 계산용 · 검색마다 갱신).
    matcher: Option<Matcher>,
    /// 위치(Where) 상자의 토글 = Aa 하나(패턴 대소문자 구분 · 기본 끔 = OS 무관 무시 · 사용자 10-13).
    where_btns: Vec<FindBtn>,
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
    /// 더블클릭 감지(행 · 첫 클릭 시각) · 간격 = `ui.dblclick_ms`(프로젝트 탐색기·북마크 패널과 같은 규칙).
    last_click: Option<(usize, Instant)>,
    dblclick_ms: u128,
    /// 검색어 이력(전역 · `search.query`/`search.where` · ↑/↓ 되부르기 · 실행 = 기록 · 사용자 09-23).
    history: Option<SharedHistory>,
    rq: Recall,
    rw: Recall,
    /// 상자 드래그가 상자 밖으로 나갔다(놓임 = 드롭다운 항목 선택이 아니라 닫기 · FilterBar `drag_out`과 같은 규칙).
    drag_out: bool,
    /// ★ 검색 로그 = 건너뛴 파일(표시 경로 · 이유 글) — 워커가 여럿이어도 채널 하나로 **도착 순 병합**(사용자 09-23
    ///   "용량이 크거나 텍스트 검색이 불가능한 대상은 건너뛰고 로그에 제외 파일과 이유"). 결과 아래 접이식 절.
    skipped: Vec<(String, String)>,
    skipped_expanded: bool,
    /// 가로 스크롤(긴 경로·이유 · Alt 전체 경로 보기 · 사용자 09-23) — 내용 폭은 그린 행의 최대(행 집합이 바뀌면 다시 0부터).
    scroll_x: i32,
    content_w: std::cell::Cell<i32>,
}

const ROW_H: f32 = 22.0;
const INPUT_H: f32 = 25.0;
const PAD: f32 = 8.0;

/// 틀 하나 + 안쪽 상자(토글 자리는 호출자가 그린다): 바탕 · 상자의 hover 덧칠을 틀 전체에 · 테두리 · 상자 · 상자 오른쪽 테두리를 합성
/// 배경으로 덮기(곧은 부분 + 위·아래 모서리 호 — 필터 틀 09-28 · 10-13) · 포커스 테두리. 검색어·위치 상자가 같은 그림을 쓴다.
fn paint_box_frame(dc: &mut dyn DrawCtx, th: &Theme, frame: Rect, tb: &TextBox, r: i32) {
    dc.fill_round_rect(frame, r, th.field_bg);
    let hov = tb.hover_overlay_alpha();
    if hov > 0.0 {
        dc.fill_round_rect_alpha(frame, r, th.text, hov);
    }
    let bg = th.field_bg.lerp(th.text, hov);
    dc.stroke_round_rect(frame, r, th.border, 1.0);
    tb.paint(dc, th);
    let tbb = tb.bounds();
    if tbb.w > 0 {
        dc.fill_rect(
            Rect::new(tbb.right() - 2, tbb.y + 1, 3, (tbb.h - 2).max(0)),
            bg,
        );
        let cw = r + 3;
        dc.fill_rect(Rect::new(tbb.right() - r - 1, tbb.y + 1, cw, r), bg);
        dc.fill_rect(
            Rect::new(tbb.right() - r - 1, tbb.bottom() - r - 1, cw, r),
            bg,
        );
    }
    if tb.is_focused() {
        dc.stroke_round_rect(frame, r, th.accent, 1.0);
    }
}

impl SearchPanel {
    /// 상자 팝업(검색어·위치 편집 메뉴 · 이력 드롭다운)이 열려 있는가 — 우클릭 메뉴 배타 규칙(10-13).
    pub(crate) fn popup_open(&self) -> bool {
        self.query.popup_open()
            || self.where_box.popup_open()
            || self.rq.is_open()
            || self.rw.is_open()
    }
    pub(crate) fn popup_bounds(&self) -> nexa_ctl::geom::Rect {
        let mut r = nexa_ctl::geom::Rect::default();
        let mut add = |x: nexa_ctl::geom::Rect| {
            if x.w > 0 && x.h > 0 {
                r = if r.w > 0 { r.union(&x) } else { x };
            }
        };
        // 소유 상자 포함(상자 클릭 = 다른 영역이 아니다 · 10-13).
        if self.query.popup_open() || self.rq.is_open() {
            add(self.query.popup_bounds());
            add(self.query.bounds());
        }
        if self.where_box.popup_open() || self.rw.is_open() {
            add(self.where_box.popup_bounds());
            add(self.where_box.bounds());
        }
        if self.rq.is_open() {
            add(self.rq.bounds());
        }
        if self.rw.is_open() {
            add(self.rw.bounds());
        }
        r
    }
    pub(crate) fn close_popups(&mut self) {
        self.query.close_menu();
        self.where_box.close_menu();
        self.rq.close();
        self.rw.close();
    }

    /// 언어 전환 뒤 재라벨(사용자 10-13 "플레이스홀더·버튼·토글 다국어").
    pub(crate) fn relabel(&mut self) {
        self.query.set_placeholder(t(Msg::PhSearchQuery));
        self.where_box.set_placeholder(t(Msg::PhSearchWhere));
    }

    pub(crate) fn new() -> Self {
        // × 지우기 = 검색 입력란 공통(글이 있을 때만 · 사용자 09-23).
        let mut query = TextBox::new(t(Msg::PhSearchQuery)).with_clearable();
        query.set_focus_ring(false);
        // ★ 진행 표시 링(혜성)은 상자 속성이되 **틀 둘레**에 그린다(필터 틀과 같은 모양 · 사용자 10-13 "입력창 일부만 탄다").
        query.set_busy_host_painted(true);
        let mut where_box = TextBox::new(t(Msg::PhSearchWhere)).with_clearable();
        where_box.set_focus_ring(false);
        let mut replace = TextBox::new(t(Msg::PhSearchReplace)).with_clearable();
        replace.set_focus_ring(false);
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
            fold_btn: FindBtn::new(BtnKind::Fold, toolicons::mi_chevron_right),
            replace,
            replace_open: false,
            repl_btns: vec![FindBtn::new(BtnKind::ReplaceAll, toolicons::mi_replace_all)],
            replace_req: None,
            matcher: None,
            where_btns: vec![FindBtn::new(BtnKind::Case, toolicons::mi_match_case)],
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
            last_click: None,
            dblclick_ms: 400,
            history: None,
            rq: Recall::new("search.query"),
            rw: Recall::new("search.where"),
            drag_out: false,
            skipped: Vec::new(),
            skipped_expanded: false,
            scroll_x: 0,
            content_w: std::cell::Cell::new(0),
        }
    }

    /// 제외 사유 → 사용자 글.
    fn skip_text(reason: &SkipReason) -> String {
        match reason {
            SkipReason::TooLarge { bytes, limit } => tf(
                Msg::SearchSkipTooLarge,
                &[&(bytes / 1024).to_string(), &(limit / 1024).to_string()],
            ),
            SkipReason::Binary => t(Msg::SearchSkipBinary).into(),
            SkipReason::Unreadable(e) => tf(Msg::SearchSkipUnreadable, &[e]),
            SkipReason::NonUtf8Name => t(Msg::SearchSkipNonUtf8).into(),
        }
    }

    pub(crate) fn set_dblclick_ms(&mut self, ms: u128) {
        self.dblclick_ms = ms.max(1);
    }

    /// 검색어 이력 잇기(호스트).
    pub(crate) fn set_history(&mut self, h: SharedHistory) {
        self.history = Some(h);
    }

    /// 자체 시험용: 검색어(와 범위)를 넣고 실행을 요청한다(`search.run:<글>[|<범위>]` 기동 명령 · 키 주입 없이 · 제외 로그·범위 필터 캡처 09-23).
    pub(crate) fn run_query(&mut self, q: &str, where_text: Option<&str>) {
        self.query.set_text(q);
        let _ = self.query.take_changed();
        if let Some(w) = where_text {
            self.where_box.set_text(w);
            let _ = self.where_box.take_changed();
        }
        self.request = true;
    }

    /// 검색어·범위를 이력에 올린다(검색을 실행하는 순간).
    fn history_commit(&mut self) {
        if let Some(h) = &self.history {
            self.rq.commit(&self.query, h);
            self.rw.commit(&self.where_box, h);
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

    /// 검색어(프로젝트 파일에 저장 · 사용자 09-23 "좌측 기능별 복원").
    pub(crate) fn query_text(&self) -> String {
        self.query.text()
    }

    pub(crate) fn set_query_text(&mut self, s: &str) {
        if self.query.text() != s {
            self.query.set_text(s);
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
            for b in self.btns.iter_mut().chain(self.where_btns.iter_mut()) {
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
        } else if self.replace_open && self.replace.is_focused() {
            Some(&mut self.replace)
        } else if self.query.is_focused() {
            Some(&mut self.query)
        } else {
            None
        }
    }

    /// ★ T-81d: 바꾸기 줄 열고 바꾸기 상자에 포커스(Ctrl+Shift+H 후보 · 팔레트).
    pub(crate) fn focus_replace(&mut self) {
        self.replace_open = true;
        self.fold_btn.checked = true;
        let (b, s) = (self.bounds, self.scale);
        self.set_bounds(b, s);
        self.query.set_focused(false);
        self.where_box.set_focused(false);
        self.replace.set_focused(true);
    }

    /// [모두 바꾸기] 요청(1회성) — 바꿀 글(빈 글 = 삭제).
    pub(crate) fn take_replace_all(&mut self) -> Option<String> {
        self.replace_req.take()
    }

    /// 바꾸기 대상 = 지금 결과의 파일들(탭 id · 경로) — 호스트가 탭은 버퍼로, 파일은 디스크로 바꾼다.
    pub(crate) fn replace_targets(&self) -> Vec<(Option<u64>, Option<PathBuf>)> {
        self.files.iter().map(|f| (f.tab, f.path.clone())).collect()
    }

    /// 지금 검색 조건으로 매처를 새로 만든다(호스트의 바꾸기 적용용).
    pub(crate) fn make_matcher(&self) -> Option<Matcher> {
        Matcher::new(&self.query.text(), self.case(), self.word(), self.regex()).ok()
    }

    /// 자체 시험·덤프용(기동 명령 `search.dump:<파일>`): 결과 파일마다 `라벨<TAB>일치 수` 한 줄 + 바꾸기 줄 상태 + 상태 글.
    pub(crate) fn dump_text(&self) -> String {
        let mut out = String::new();
        for f in &self.files {
            out.push_str(&format!(
                "{}	{}
",
                f.label,
                f.matches.len()
            ));
        }
        out.push_str(&format!(
            "replace_open={} replace_text={:?} files={} status={}
",
            self.replace_open,
            self.replace.text(),
            self.files.len(),
            self.status
        ));
        out
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
        // ★ T-81d: `›` 접기 토글(검색어 틀 왼쪽 · 찾기 위젯과 같은 자리) — 틀은 그만큼 오른쪽에서 시작.
        let fold_w = tg;
        self.fold_btn.rect = Rect::new(b.x + pad, y1 + px(3.0), fold_w, tg);
        let fx = b.x + pad + fold_w + tgap;
        // 검색어 상자 = 폭 전체 · 안쪽 오른쪽 토글 3개(찾기 위젯과 같은 배치).
        let frame = Rect::new(fx, y1, (b.right() - pad - fx).max(px(80.0)), ih);
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
        // 위·아래 여백 = 다른 패널과 같은 부품 상수(사용자 09-22 "모두 동일하게").
        let mut y2 = y1 + ih + px(crate::filterbar::GAP_Y);
        // ★ T-81d: 바꾸기 줄(펼쳤을 때만 · 검색어 틀과 같은 x·폭 · 틀 안 오른쪽 [모두 바꾸기]).
        if self.replace_open {
            let yr = y2;
            let mut rx = frame.right() - px(2.0) - tg;
            for btn in &mut self.repl_btns {
                btn.rect = Rect::new(rx, yr + px(3.0), tg, tg);
                rx -= tg + tgap;
            }
            self.replace.set_scale(s);
            self.replace.set_bounds(
                Rect::new(frame.x, yr, (rx + tg + tgap - frame.x).max(px(40.0)), ih),
                &mut inv,
            );
            y2 = yr + ih + px(crate::filterbar::GAP_Y);
        } else {
            self.replace.set_bounds(Rect::default(), &mut inv);
            for btn in &mut self.repl_btns {
                btn.rect = Rect::default();
            }
        }
        // 위치 상자 = 같은 틀 · 안쪽 오른쪽 Aa 토글(사용자 10-13).
        let mut wx = frame.right() - px(2.0) - tg;
        for btn in &mut self.where_btns {
            btn.rect = Rect::new(wx, y2 + px(3.0), tg, tg);
            wx -= tg + tgap;
        }
        self.where_box.set_scale(s);
        self.where_box.set_bounds(
            Rect::new(frame.x, y2, (wx + tg + tgap - frame.x).max(px(40.0)), ih),
            &mut inv,
        );
        self.row_h = px(ROW_H);
        let list_top = y2 + ih + px(crate::filterbar::GAP_Y) + self.row_h; // 상태 한 줄
                                                                           // 오른쪽 테두리 한 줄(`b.right() - 1`)은 목록 밖 — 머리·선택 행 배경이 스플리터 선을 덮지 않게(사용자 10-13 · 패널 공통).
        self.list_rect = Rect::new(
            b.x,
            list_top,
            (b.w - 1).max(0),
            (b.bottom() - list_top).max(0),
        );
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
    /// 위치(Where) 패턴의 대소문자 구분(Aa 토글 · 기본 끔 = 접기 · 사용자 10-13).
    fn where_case(&self) -> bool {
        self.where_btns
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
        self.skipped.clear();
        self.sel = None;
        self.scroll_y = 0;
        self.error = None;
        self.tab_paths.clear();
        let q = self.query.text();
        if q.trim().is_empty() {
            self.status.clear();
            return;
        }
        let spec = parse_where(
            &self.where_box.text(),
            ctx.current_dir.as_deref(),
            &ctx.default_roots,
        );
        let matcher = match Matcher::new(&q, self.case(), self.word(), self.regex()) {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                self.status = tf(Msg::StSearchError, &[&e.to_string()]);
                return;
            }
        };
        self.matcher = Matcher::new(&q, self.case(), self.word(), self.regex()).ok();
        let t0 = Instant::now();
        let mut tab_matches = 0usize;
        if spec.open_tabs {
            for (id, label, text, path) in &ctx.tabs {
                // 포함/제외 패턴은 열린 탭에도(폴더 걷기와 같은 규칙 · 사용자 09-23 "+*.yaml인데 .sql 탭이 남는다") — 파일 탭은 경로 ·
                //   이름 없는 탭은 제목으로 판정 · 사용자 `-pat`와 설정 제외를 함께 본다.
                let rel = path
                    .as_ref()
                    .map_or_else(|| label.clone(), |p| p.to_string_lossy().into_owned());
                let mut exc = ctx.excludes.clone();
                exc.extend(spec.excludes.iter().cloned());
                if !nsql_search::path_allowed_case(&spec.includes, &exc, &rel, self.where_case()) {
                    continue;
                }
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
            includes: spec.includes,
            max_file_kb: ctx.max_file_kb,
            threads: ctx.threads,
            gitignore: ctx.gitignore,
            pattern_case: self.where_case(),
            transcode: ctx.transcode,
        };
        match Search::spawn(opts) {
            Ok((rx, cancel)) => {
                self.job = Some((rx, cancel, t0));
                self.status = t(Msg::StSearching).into();
                // ★ 진행 표시(혜성) = 검색어 상자의 속성(nexa-ctl 170 · 객체/프로젝트 탐색기·팔레트와 같은 부품) — 파일 검색만 상태를
                //   안 올리고 있었다(사용자 10-13 "파일 검색의 검색창에서 혜성 꼬리가 동작하지 않아").
                self.query.set_busy(nexa_ctl::BusyState::Running);
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.status = tf(Msg::StSearchError, &[&e.to_string()]);
                self.query.set_busy(nexa_ctl::BusyState::Idle);
            }
        }
        self.rebuild_rows();
    }

    pub(crate) fn cancel(&mut self) {
        if let Some((_, c, _)) = self.job.take() {
            c.cancel();
            // 취소 = 완료 표시 없이 원복.
            self.query.set_busy(nexa_ctl::BusyState::Idle);
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
                    // 폴더 실패도 검색 로그에(검색은 계속) · 상태줄에는 마지막 것.
                    self.skipped.push((
                        nexa_fs::path::display(&path),
                        tf(Msg::SearchSkipDirError, &[&message]),
                    ));
                    self.error = Some(format!("{}: {message}", path.display()));
                }
                Batch::Skipped { path, reason } => {
                    self.skipped
                        .push((nexa_fs::path::display(&path), Self::skip_text(&reason)));
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
            if !self.skipped.is_empty() {
                self.status.push_str(&tf(
                    Msg::StSearchSkippedTail,
                    &[&self.skipped.len().to_string()],
                ));
            }
            self.job = None;
            // 다 끝남 = 완료 시그니처(두 번 깜빡임 뒤 완료 테두리 · 글을 바꾸면 원복 = 부품 규칙).
            if self.query.busy_state() == nexa_ctl::BusyState::Running {
                self.query.set_busy(nexa_ctl::BusyState::Done);
            }
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
        // 검색 로그(제외 파일)는 결과 아래 접이식 절 — 기본 접힘(개수만).
        if !self.skipped.is_empty() {
            self.rows.push(Row::SkipHeader);
            if self.skipped_expanded {
                for i in 0..self.skipped.len() {
                    self.rows.push(Row::Skip(i));
                }
            }
        }
        let max = (self.rows.len() as i32 * self.row_h - self.list_rect.h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
        // 행 집합이 바뀌면 내용 폭은 다시 잰다(그리면서 자란다) · 가로 위치는 유지하되 다음 그리기에서 폭 안으로.
        self.content_w.set(0);
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if !self.visible {
            return false;
        }
        let mut any =
            self.query.tick(now_ms) | self.where_box.tick(now_ms) | self.bars.tick(now_ms);
        for b in self.btns.iter_mut().chain(self.where_btns.iter_mut()) {
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

    /// 행 실행 — 파일 행 = 접기/펼치기 · 일치 행 = 열기 요청(`permanent` = 정식 탭 · 아니면 미리보기).
    fn activate_row(&mut self, i: usize, permanent: bool) {
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
                            permanent,
                        });
                    }
                }
            }
            Some(Row::SkipHeader) => {
                self.skipped_expanded = !self.skipped_expanded;
                self.rebuild_rows();
            }
            Some(Row::Skip(_)) | None => {}
        }
    }

    /// 이벤트(마우스 = 패널 안 · 키 = 포커스일 때) — 다시 그려야 하면 `true`.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        // ★ 휠 = 열린 팝업(이력 드롭다운·편집 메뉴)이 먼저(사용자 10-13 "이력 위 스크롤이 뒤 결과 목록도 스크롤 · 속도 HUD") —
        //   아래 목록 스크롤바보다 앞에서 끝낸다(필터 틀 `popup_takes`와 같은 규칙).
        if matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. }) {
            if let Some(h) = self.history.clone() {
                if self.rq.is_open() {
                    let _ = self.rq.on_event(ev, &mut self.query, &h, &mut inv);
                    return true;
                }
                if self.rw.is_open() {
                    let _ = self.rw.on_event(ev, &mut self.where_box, &h, &mut inv);
                    return true;
                }
            }
            if self.query.popup_open() {
                self.query.on_event(ev, &mut inv);
                return true;
            }
            if self.where_box.popup_open() {
                self.where_box.on_event(ev, &mut inv);
                return true;
            }
        }
        // 목록 스크롤바(휠 포함).
        let content_h = self.rows.len() as i32 * self.row_h;
        let (nx, ny, consumed) = self.bars.on_event(
            ev,
            self.list_rect,
            self.content_w.get().max(self.list_rect.w),
            content_h.max(self.list_rect.h),
            self.scroll_x,
            self.scroll_y,
            self.scale,
        );
        if ny != self.scroll_y || nx != self.scroll_x {
            self.scroll_y = ny;
            self.scroll_x = nx.max(0);
            return true;
        }
        if consumed {
            return true;
        }
        // ★ 상자의 우클릭 편집 메뉴가 떠 있으면 마우스는 **그 상자에만**(팝업 최상위 · 61 §2-2-b 5항): 종전엔 두 상자에 다 넘겨 메뉴
        //   (아래 상자 위에 그려짐)의 "잘라내기" 클릭이 위치 상자에도 들어가 이력 드롭다운이 열렸다(사용자 10-13). 바깥 누름으로 닫혔으면
        //   그 클릭은 통과(팝업 UX 규칙) · 놓임·이동은 여기서 끝.
        if matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
                | InputEvent::RightDown { .. }
                | InputEvent::Key { .. }
                | InputEvent::Char { .. }
        ) {
            // 우클릭 = 열린 이력 드롭다운(어느 상자든)은 즉시 닫는다(사용자 09-25 규칙 · 10-13 "우클릭이 눌리면 History는 닫혀야").
            if matches!(ev, InputEvent::RightDown { .. }) {
                self.rq.close();
                self.rw.close();
            }
            let owner = if self.query.popup_open() {
                Some(&mut self.query)
            } else if self.where_box.popup_open() {
                Some(&mut self.where_box)
            } else {
                None
            };
            if let Some(tb) = owner {
                tb.on_event(ev, &mut inv);
                // 키(Esc·Enter·↑↓·글자)는 메뉴 몫 — 닫혔어도 다른 컨트롤로 흘리지 않는다(사용자 10-13).
                if tb.popup_open()
                    || matches!(
                        ev,
                        InputEvent::MouseUp { .. }
                            | InputEvent::MouseMove { .. }
                            | InputEvent::Key { .. }
                            | InputEvent::Char { .. }
                    )
                {
                    return true;
                }
            }
        }
        // ★ 상자가 드래그 선택 중이면 이동·놓임은 **상자가 먼저**(사용자 10-13 "검색에서 마우스가 놓이지 않는다" · FilterBar의 09-30 규칙과
        //   같다): 클릭으로 연 이력 드롭다운이 MouseMove/MouseUp을 `Consumed`로 먹어 상자가 놓임을 못 받고 드래그가 남았다(이 패널은
        //   FilterBar를 안 써서 그 수정이 빠졌다). 상자 밖으로 끌어 놓은 것은 항목 선택이 아니라 드롭다운 닫기 · 단순 클릭은 종전대로.
        let mut tb_done: Option<bool> = None; // Some(true) = query · Some(false) = where
        if matches!(
            ev,
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. }
        ) {
            let (dq, dw) = (self.query.is_dragging(), self.where_box.is_dragging());
            if dq || dw {
                let (tb, rc) = if dq {
                    (&mut self.query, &mut self.rq)
                } else {
                    (&mut self.where_box, &mut self.rw)
                };
                if let InputEvent::MouseMove { x, y } = *ev {
                    if !tb.bounds().contains(Point { x, y }) {
                        self.drag_out = true;
                    }
                }
                tb.on_event(ev, &mut inv);
                tb_done = Some(dq);
                if self.drag_out {
                    if matches!(ev, InputEvent::MouseUp { .. }) {
                        if rc.is_open() {
                            rc.close();
                        }
                        self.drag_out = false;
                    }
                    return true;
                }
            }
        }
        // 검색어 이력(드롭다운/Flat)이 먼저 · 끝에서 ↓/Tab = 결과 목록 첫 행으로(사용자 09-23).
        if let Some(h) = &self.history {
            let mut leave = false;
            // 편집 메뉴가 떠 있으면 키(↓ 등)는 메뉴(상자) 몫 — 이력이 먼저 받으면 메뉴 위로 드롭다운이 열렸다(사용자 10-13).
            if !self.query.popup_open() && (self.query.is_focused() || self.rq.is_open()) {
                match self.rq.on_event(ev, &mut self.query, h, &mut inv) {
                    RecallEvent::Consumed => return true,
                    RecallEvent::Changed => {
                        let _ = self.query.take_changed();
                        return true;
                    }
                    RecallEvent::LeaveDown => leave = true,
                    RecallEvent::Pass => {}
                }
            }
            if !leave
                && !self.where_box.popup_open()
                && (self.where_box.is_focused() || self.rw.is_open())
            {
                match self.rw.on_event(ev, &mut self.where_box, h, &mut inv) {
                    RecallEvent::Consumed => return true,
                    RecallEvent::Changed => {
                        let _ = self.where_box.take_changed();
                        return true;
                    }
                    RecallEvent::LeaveDown => leave = true,
                    RecallEvent::Pass => {}
                }
            }
            if leave {
                self.query.set_focused(false);
                self.where_box.set_focused(false);
                if !self.rows.is_empty() {
                    self.sel = Some(0);
                    self.ensure_visible(0);
                }
                return true;
            }
        }
        match *ev {
            InputEvent::Key { key, shift, .. } => {
                match key {
                    CtlKey::Enter => {
                        if self.query.is_focused()
                            || self.where_box.is_focused()
                            || (self.replace_open && self.replace.is_focused())
                        {
                            self.history_commit();
                            self.request = true;
                            return true;
                        }
                        if let Some(i) = self.sel {
                            // Enter = 정식 탭(프로젝트 탐색기와 같은 규칙).
                            self.activate_row(i, true);
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
                    // ★ ←/→ = 접기·펼치기·부모로(사용자 10-20) — 입력 상자 포커스 중엔 글자 캐럿 몫.
                    CtlKey::Left | CtlKey::Right
                        if !self.query.is_focused() && !self.where_box.is_focused() =>
                    {
                        if let Some(cur) = self.sel {
                            let files = &self.files;
                            let skipped_expanded = self.skipped_expanded;
                            let expanded = |r: Row| match r {
                                Row::File(fi) => files.get(fi).is_some_and(|f| f.expanded),
                                Row::SkipHeader => skipped_expanded,
                                _ => false,
                            };
                            match fold_step(&self.rows, &expanded, cur, key == CtlKey::Left) {
                                FoldStep::Select(r) => {
                                    self.sel = Some(r);
                                    self.ensure_visible(r);
                                }
                                // 접고 펴도 앞쪽 행 번호는 그대로 = 선택 유지.
                                FoldStep::Toggle(r) => self.activate_row(r, false),
                                FoldStep::None => {}
                            }
                            return true;
                        }
                    }
                    _ => {}
                }
                let _ = shift;
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                self.drag_out = false;
                let in_q = self.query.bounds().contains(p);
                let in_w = self.where_box.bounds().contains(p);
                let in_r = self.replace_open && self.replace.bounds().contains(p);
                // 상자 편집 메뉴가 떠 있으면(메뉴 항목 클릭) 포커스는 그대로 — 메뉴 동작(잘라내기·붙여넣기)의 대상이 포커스 상자라서
                // (`focused_textbox` → `clip_action` · 사용자 10-13 "파일 검색에서 동작 안 함").
                if !(self.query.popup_open() || self.where_box.popup_open()) {
                    self.query.set_focused(in_q);
                    self.where_box.set_focused(in_w);
                    self.replace.set_focused(in_r);
                }
                let hit = self.btns.iter().position(|b| b.rect.contains(p));
                for (i, b) in self.btns.iter_mut().enumerate() {
                    b.focused = Some(i) == hit;
                }
                let whit = self.where_btns.iter().position(|b| b.rect.contains(p));
                for (i, b) in self.where_btns.iter_mut().enumerate() {
                    b.focused = Some(i) == whit;
                }
                if let Some(i) = self.row_at(p) {
                    self.sel = Some(i);
                    // 한 번 클릭 = 미리보기 탭 · 같은 행을 `dblclick_ms` 안에 다시 = 정식 탭(프로젝트 탐색기·북마크와 같은 규칙 ·
                    //   사용자 09-23 "검색 결과는 미리보기 탭으로 · 이후 동작은 프로젝트 탐색기와 동일") · 세 번째는 새 시작.
                    let now = Instant::now();
                    let dbl = matches!(self.last_click, Some((j, t0)) if j == i && now.duration_since(t0).as_millis() < self.dblclick_ms);
                    self.last_click = if dbl { None } else { Some((i, now)) };
                    self.activate_row(i, dbl);
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
        if tb_done != Some(true) {
            self.query.on_event(ev, &mut inv);
        }
        if tb_done != Some(false) {
            self.where_box.on_event(ev, &mut inv);
        }
        if self.replace_open {
            self.replace.on_event(ev, &mut inv);
            let _ = self.replace.take_changed();
        }
        self.fold_btn.on_event(ev);
        for b in &mut self.repl_btns {
            b.on_event(ev);
        }
        if self.fold_btn.take_clicked() {
            self.replace_open = !self.replace_open;
            self.fold_btn.checked = self.replace_open;
            let (b, s) = (self.bounds, self.scale);
            self.set_bounds(b, s);
            if !self.replace_open {
                self.replace.set_focused(false);
            }
        }
        for b in &mut self.repl_btns {
            if b.take_clicked() && b.kind == BtnKind::ReplaceAll {
                // 결과가 있을 때만(없으면 호스트가 "먼저 검색" 안내).
                self.replace_req = Some(self.replace.text());
            }
        }
        // 상자 클릭 = 이력 드롭다운(보기 방식 Dropdown · 이력 있을 때 · 사용자 09-23).
        if let (InputEvent::MouseDown { x, y, .. }, Some(h)) = (ev, &self.history) {
            let p = Point { x: *x, y: *y };
            let host = Rect::new(0, 0, self.clamp_w, i32::MAX / 4);
            if self.query.bounds().contains(p) {
                self.rq.on_click(&self.query, h, host, self.scale);
            } else if self.where_box.bounds().contains(p) {
                self.rw.on_click(&self.where_box, h, host, self.scale);
            }
        }
        for b in self.btns.iter_mut().chain(self.where_btns.iter_mut()) {
            b.on_event(ev);
        }
        // 검색어를 ×(또는 전부 지움)로 비우면 결과·제외 로그·상태도 비운다(VS Code와 같음 · 사용자 09-23 "×로 제거해도 결과가 안 바뀜").
        let q_changed = self.query.take_changed().is_some();
        if q_changed {
            if let Some(h) = &self.history {
                self.rq.after_edit(&self.query, h);
            }
        }
        if q_changed && self.query.text().trim().is_empty() {
            self.cancel();
            self.files.clear();
            self.skipped.clear();
            self.rows.clear();
            self.status.clear();
            self.error = None;
            self.sel = None;
            self.scroll_y = 0;
        }
        if self.where_box.take_changed().is_some() {
            if let Some(h) = &self.history {
                self.rw.after_edit(&self.where_box, h);
            }
        }
        let mut redraw = true;
        for b in self.btns.iter_mut().chain(self.where_btns.iter_mut()) {
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
        paint_box_frame(dc, th, frame, &self.query, r);
        // 진행 표시 링 = 틀 둘레(상자는 `set_busy_host_painted`로 생략 · 필터 틀과 같다).
        self.query.busy_ring().paint(dc, th, frame, r, s);
        for btn in &self.btns {
            btn.paint(dc, th, s);
        }
        // ★ T-81d: `›` 토글 + 바꾸기 줄(펼쳤을 때).
        self.fold_btn.paint(dc, th, s);
        if self.replace_open {
            let rb = self.replace.bounds();
            let rframe = Rect::new(rb.x, rb.y, frame.w, rb.h);
            paint_box_frame(dc, th, rframe, &self.replace, r);
            for btn in &self.repl_btns {
                btn.paint(dc, th, s);
            }
        }
        // 위치 상자 = 같은 틀 + Aa 토글(사용자 10-13).
        let wb = self.where_box.bounds();
        let wframe = Rect::new(wb.x, wb.y, frame.w, wb.h);
        paint_box_frame(dc, th, wframe, &self.where_box, r);
        for btn in &self.where_btns {
            btn.paint(dc, th, s);
        }
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
        // ★ 가로 스크롤(사용자 09-23): 내용은 `lx`(= lr.x − scroll_x) 기준으로 그리고 목록 영역 `lr`로 클립 · 내용 폭은 그린 행의 최대로
        //   자란다(Alt 전체 경로 보기에서는 축약하지 않으므로 그만큼 넓어져 스크롤로 닿는다 — 종전엔 잘라 그려 빈칸으로 보였다).
        let sx = self.scroll_x;
        let lx = lr.x - sx;
        let mut content_w = self.content_w.get();
        let clip_to = |r: Rect| r.intersection(&lr);
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
            let full = nexa_ctl::draw::show_full();
            let right = match self.rows[i] {
                Row::File(fi) => {
                    let f = &self.files[fi];
                    // 셰브론 = 프로젝트 탐색기·북마크 패널과 같은 부품(`draw_chevron_90_in` · 접힘 흐림 · 펼침/호버 본문색 · 사용자 09-23).
                    let chev_w = dc.text_height().max(10);
                    let color = if f.expanded || self.hover == Some(i) {
                        th.text
                    } else {
                        th.text_dim
                    };
                    nexa_ctl::controls::draw_chevron_90_in(
                        dc,
                        Rect::new(lx + indent, y + (rh - chev_w) / 2, chev_w, chev_w),
                        color,
                        f.expanded,
                        Some(row_rect),
                    );
                    let lab_x = lx + indent + chev_w + px(4.0);
                    let count = f.matches.len().to_string();
                    let cw = dc.text_width(&count);
                    // 개수는 오른쪽 끝에 고정 · 라벨은 개수 앞까지(전체 보기면 축약 없이 스크롤로).
                    let label_max = (lr.right() - indent - cw - px(6.0) - lab_x).max(px(40.0));
                    let shown = nexa_ctl::draw::ellipsize_middle(dc, &f.label, label_max);
                    let sw = dc.text_width(&shown);
                    let label_w = if full { sw } else { sw.min(label_max) };
                    dc.text(
                        lab_x,
                        ty,
                        clip_to(Rect::new(lab_x, y, label_w, rh)),
                        &shown,
                        th.text,
                    );
                    dc.text(lr.right() - indent - cw, ty, row_rect, &count, th.text_dim);
                    lab_x + sw + px(6.0) + cw + indent
                }
                Row::Match(fi, mi) => {
                    let m = &self.files[fi].matches[mi];
                    let num = format!("{}:", m.line_no);
                    let nw = dc.text_width(&num);
                    let x0 = lx + indent * 2 + sel_w;
                    dc.text(x0, ty, clip_to(Rect::new(x0, y, nw, rh)), &num, th.text_dim);
                    let tx = x0 + nw + px(6.0);
                    let line = m.text.trim_end();
                    // 일치 구간 강조(반투명) — 앞 문맥이 길면 일치가 보이도록 왼쪽을 자른다.
                    let chars: Vec<char> = line.chars().collect();
                    let cut = m.col.saturating_sub(24);
                    let shown: String = chars.iter().skip(cut).collect();
                    let mut w = Vec::new();
                    dc.text_prefix_widths(&shown, &mut w);
                    let total = w.last().copied().unwrap_or(0);
                    let a = m.col - cut;
                    let e = (a + m.len).min(chars.len().saturating_sub(cut));
                    let hx0 = tx + w.get(a).copied().unwrap_or(0);
                    let hx1 = tx + w.get(e).copied().unwrap_or(0);
                    if hx1 > hx0 {
                        let hl = clip_to(Rect::new(hx0, y + 2, hx1 - hx0, rh - 4));
                        if !hl.is_empty() {
                            dc.fill_round_rect_alpha(hl, px(2.0), th.warn, 0.35);
                        }
                    }
                    dc.text(
                        tx,
                        ty,
                        clip_to(Rect::new(tx, y, total, rh)),
                        &shown,
                        th.text,
                    );
                    // ★ T-81d 미리보기: 바꾸기 줄이 열려 있고 글이 있으면 `→ 바뀐 줄`(흐린 색 · 왼쪽 자름 없이 그 자리부터).
                    let mut end = tx + total;
                    if self.replace_open {
                        let with = self.replace.text();
                        if !with.is_empty() {
                            if let Some((newline, _)) = self
                                .matcher
                                .as_ref()
                                .and_then(|m| m.replace_all(line, &with))
                            {
                                let arrow = " → ";
                                let aw = dc.text_width(arrow);
                                dc.text(
                                    end,
                                    ty,
                                    clip_to(Rect::new(end, y, aw, rh)),
                                    arrow,
                                    th.text_dim,
                                );
                                end += aw;
                                let nw = dc.text_width(&newline);
                                dc.text(
                                    end,
                                    ty,
                                    clip_to(Rect::new(end, y, nw, rh)),
                                    &newline,
                                    th.text_dim,
                                );
                                end += nw;
                            }
                        }
                    }
                    end + indent
                }
                Row::SkipHeader => {
                    let chev_w = dc.text_height().max(10);
                    let color = if self.skipped_expanded || self.hover == Some(i) {
                        th.text
                    } else {
                        th.text_dim
                    };
                    nexa_ctl::controls::draw_chevron_90_in(
                        dc,
                        Rect::new(lx + indent, y + (rh - chev_w) / 2, chev_w, chev_w),
                        color,
                        self.skipped_expanded,
                        Some(row_rect),
                    );
                    let lab_x = lx + indent + chev_w + px(4.0);
                    let label = tf(Msg::SearchSkippedHeader, &[&self.skipped.len().to_string()]);
                    let lw = dc.text_width(&label);
                    dc.text(
                        lab_x,
                        ty,
                        clip_to(Rect::new(lab_x, y, lw, rh)),
                        &label,
                        th.warn,
                    );
                    lab_x + lw + indent
                }
                Row::Skip(si) => {
                    // 경로 — 이유: 보통은 둘 다 행 안에 맞춘다(이유가 길면 경로는 최소 폭 · 이유도 가운데 축약) ·
                    //   Alt 전체 보기(`show_full`)면 축약 없이 그리고 내용 폭이 자라 가로 스크롤로 닿는다(사용자 09-23 빈칸 결함).
                    let (path, why) = &self.skipped[si];
                    let x0 = lx + indent * 2 + sel_w;
                    let gap = px(8.0);
                    let max_w = (lr.right() - indent - x0).max(px(40.0));
                    let why_w = dc.text_width(why);
                    let path_avail = (max_w - why_w - gap).clamp(px(60.0).min(max_w), max_w);
                    let shown = nexa_ctl::draw::ellipsize_middle(dc, path, path_avail);
                    let sw = dc.text_width(&shown);
                    dc.text(
                        x0,
                        ty,
                        clip_to(Rect::new(x0, y, sw, rh)),
                        &shown,
                        th.text_dim,
                    );
                    let wx = x0 + sw + gap;
                    let why_shown = if full {
                        why.clone()
                    } else {
                        nexa_ctl::draw::ellipsize_middle(dc, why, (lr.right() - indent - wx).max(0))
                    };
                    let ww = dc.text_width(&why_shown);
                    if ww > 0 {
                        dc.text(
                            wx,
                            ty,
                            clip_to(Rect::new(wx, y, ww, rh)),
                            &why_shown,
                            th.text_dim,
                        );
                    }
                    wx + ww + indent
                }
            };
            content_w = content_w.max(right - lx);
            y += rh;
        }
        self.content_w.set(content_w);
        self.bars.paint(
            dc,
            th,
            lr,
            content_w.max(lr.w),
            (self.rows.len() as i32 * rh).max(lr.h),
            sx,
            self.scroll_y,
            s,
        );
        self.query.paint_popup(dc, th);
        self.where_box.paint_popup(dc, th);
        self.rq.paint_popup(dc, th);
        self.rw.paint_popup(dc, th);
    }

    /// 토글 툴팁(팝업 층).
    pub(crate) fn paint_tooltip(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        // 검색어 토글 + 위치(Where) 토글(사용자 10-13 "Where의 기능 버튼에도 툴팁") — 위치 Aa는 단축키가 없어 제 문구(협업 V1 관찰).
        let hot = |b: &&FindBtn| {
            b.hover_on
                && b.hover_since
                    .is_some_and(|t0| t0.elapsed().as_millis() >= self.tooltip_ms)
        };
        let (btn, tip) = if let Some(b) = self.btns.iter().find(hot) {
            (b, b.kind.tip())
        } else if let Some(b) = self.where_btns.iter().find(hot) {
            (b, Msg::TipSearchWhereCase)
        } else {
            return;
        };
        // ★ 앵커 = 버튼 사각형 그대로(공용 규칙이 "항목 위"로 올린다 · nexa-ui 171): 종전엔 캡슐 높이만큼 미리 올린 앵커를 넘겨
        //   **두 번** 올라가 툴바에 묻혔다(사용자 10-13 "툴바 상단에 잘려 보임").
        nexa_ctl::draw::draw_tooltip_in(dc, th, btn.rect, (0, self.clamp_w), t(tip), self.scale);
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

    /// ←/→ 규칙(사용자 10-20): 일치 행 ← = 부모 파일 · 펼친 파일 ← = 접기 · 접힌 파일 ← = 없음 · 접힌 파일 → = 펼치기 ·
    /// 펼친 파일 → = 없음 · 제외 줄 ← = 제외 머리 · 제외 머리 접기/펼치기.
    #[test]
    fn fold_step_rules() {
        let rows = [
            Row::File(0),
            Row::Match(0, 0),
            Row::Match(0, 1),
            Row::File(1),
            Row::SkipHeader,
            Row::Skip(0),
        ];
        // 파일 0 펼침 · 파일 1 접힘 · 제외 머리 펼침.
        let exp = |r: Row| matches!(r, Row::File(0) | Row::SkipHeader);
        assert_eq!(fold_step(&rows, &exp, 2, true), FoldStep::Select(0));
        assert_eq!(fold_step(&rows, &exp, 0, true), FoldStep::Toggle(0));
        assert_eq!(fold_step(&rows, &exp, 3, true), FoldStep::None);
        assert_eq!(fold_step(&rows, &exp, 3, false), FoldStep::Toggle(3));
        assert_eq!(fold_step(&rows, &exp, 0, false), FoldStep::None);
        assert_eq!(fold_step(&rows, &exp, 5, true), FoldStep::Select(4));
        assert_eq!(fold_step(&rows, &exp, 4, true), FoldStep::Toggle(4));
        assert_eq!(fold_step(&rows, &exp, 1, false), FoldStep::None);
        assert_eq!(fold_step(&rows, &exp, 9, true), FoldStep::None);
    }

    #[test]
    fn where_parsing_rules() {
        let cur = PathBuf::from("/proj/sql");
        // 비면 열린 탭 + 작업 모드 기본 루트(파일 모드 = 없음 · 폴더/프로젝트 = 호스트가 준 것).
        let w = parse_where("", Some(&cur), &[]);
        assert!(w.open_tabs);
        assert!(w.roots.is_empty(), "파일 모드 = 열린 파일만");
        let w = parse_where(
            "",
            Some(&cur),
            &[PathBuf::from("/ws"), PathBuf::from("/extra")],
        );
        assert!(w.open_tabs);
        assert_eq!(w.roots, vec![PathBuf::from("/ws"), PathBuf::from("/extra")]);
        let w = parse_where("<open files>, /a/b, -*.log, \"/c d\"", Some(&cur), &[]);
        assert!(w.open_tabs);
        assert_eq!(w.roots, vec![PathBuf::from("/a/b"), PathBuf::from("/c d")]);
        assert_eq!(w.excludes, vec!["*.log".to_string()]);
        let w = parse_where("<current file>", None, &[]);
        assert!(!w.open_tabs);
        assert!(w.roots.is_empty());
        assert_eq!(byte_to_char_col("가나다x", 6), 2);
    }

    /// 범위 필터(사용자 09-23): `*.sql`/`?` 글롭·`+pat` = 포함(OR) · `-pat` = 제외 · 경로는 그대로 루트 · 패턴만 있으면 기본 범위에 얹음 ·
    /// 드라이브/절대 경로 꼴의 `*`는 경로로 본다.
    #[test]
    fn where_include_globs() {
        let w = parse_where(
            "*.sql, *.txt, -*_test.sql, D:\\proj, +docs/**/*.md, test?.sql",
            None,
            &[PathBuf::from("/ws")],
        );
        assert_eq!(
            w.includes,
            vec!["*.sql", "*.txt", "docs/**/*.md", "test?.sql"]
        );
        assert_eq!(w.excludes, vec!["*_test.sql"]);
        assert_eq!(
            w.roots,
            vec![PathBuf::from("D:\\proj")],
            "경로가 있으면 기본 범위를 안 얹는다"
        );
        assert!(!w.open_tabs);
        // 패턴만 → 열린 탭 + 기본 폴더.
        let w = parse_where("*.sql", None, &[PathBuf::from("/ws")]);
        assert!(w.open_tabs);
        assert_eq!(w.roots, vec![PathBuf::from("/ws")]);
        assert_eq!(w.includes, vec!["*.sql"]);
        // `C:\x\*.sql`처럼 드라이브 꼴은 경로(루트)로.
        assert!(!is_glob_token("C:\\x\\*.sql"));
        assert!(!is_glob_token("/abs/*.sql"));
        assert!(is_glob_token("src/**/*.sql"));
    }
}
