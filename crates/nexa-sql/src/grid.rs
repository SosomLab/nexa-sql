//! 결과 그리드 — 최소 가상화(보이는 행만 그린다) · **픽셀 단위 스크롤**(사용자 09-14) · 오버레이 스크롤바(필요할 때만 ·
//! 호버 두껍게 · 자동 숨김 — nexa-ctl `ScrollBars` 공용) · 컬럼 폭은 앞 200행 실측 · 메시지 모드.
//! `nexa-grid` 크레이트(U-3 · nexa-ui 21)가 오면 교체한다. 고정폭 층에서 그려진다.

use crate::toolicons;
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::{
    Control, InputEvent, Invalidations, Key, ScrollBars, TextBox, ToolIcon, ToolItem, Toolbar,
    Widget,
};
use nsql_core::{fmt_bytes, Dialect, ResultSet, Value};
use nsql_i18n::{t, Msg};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

/// 텍스트 보기 변환 스레드 → 그리드 메시지.
enum TextMsg {
    Chunk {
        lines: Vec<String>,
        done: usize,
        total: usize,
    },
    Finished,
}

/// 진행 중인 텍스트 변환(취소 깃발은 스레드가 블록마다 본다).
struct TextJob {
    rx: mpsc::Receiver<TextMsg>,
    cancel: Arc<AtomicBool>,
    done: usize,
    total: usize,
}

impl Drop for TextJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// 결과 보기 모드(사용자 09-16 · DBeaver 결과 패널 그룹 1): 그리드 · 텍스트 표 · Markdown · JSON · TSV · CSV · SQL 5종.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResultView {
    Grid,
    Text,
    Markdown,
    Json,
    Tsv,
    Csv,
    Sql(SqlKind),
}

/// 그리드가 호스트에 부탁하는 페치(docs/43): 다음 세그먼트 · 전체 · 건수.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FetchReq {
    Next { offset: usize, limit: usize },
    All,
    Count,
}

/// 복사 형식(사용자 09-15 기본 기능) — Ctrl+C = TSV(머리글 없음) · 메뉴로 나머지.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyKind {
    Tsv,
    TsvWithHeaders,
    Csv,
    /// 고정폭 정렬 텍스트 표(머리글 포함).
    Text,
    /// Markdown 표.
    Markdown,
    /// JSON 배열(객체마다 컬럼 = 값 · 숫자/불리언/NULL은 리터럴).
    Json,
}

/// SQL 문 종류·테이블 추정은 nsql-io(CLI와 공용 · docs/41).
pub(crate) use nsql_io::SqlKind;
use nsql_io::{generate, guess_table, Format, GridOpts, KeySpec};

/// 드래그 선택 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragSel {
    /// 셀 사각 범위.
    Cells,
    /// 행번호 열에서 시작 = 행 전체 범위.
    Rows,
}

pub(crate) struct Grid {
    pub bounds: Rect,
    rs: Option<ResultSet>,
    messages: Vec<String>,
    /// 탑재(set_result)·마지막 렌더 소요 — 푸터에 표시(docs/26 Load·Render).
    load: std::time::Duration,
    render: std::time::Duration,
    approx_bytes: u64,
    /// 세로 스크롤(픽셀 · 0 = 첫 행 상단).
    scroll_y: i32,
    /// 가로 스크롤(픽셀).
    scroll_x: i32,
    col_w: Vec<i32>,
    row_h: i32,
    header_h: i32,
    /// 오버레이 스크롤바(필요할 때만 · 휠/호버 시 표시 · 호버 두껍게 · 자동 숨김).
    bars: ScrollBars,
    /// 설정 `grid.scroll` = row 이면 세로 스크롤을 행 경계에 맞춘다(기본 pixel · 사용자 09-14).
    row_snap: bool,
    /// 설정 `grid.row_numbers`(기본 켬) — 왼쪽 고정 행번호 열(가로 스크롤 무관).
    row_numbers: bool,
    /// 설정 `grid.copy_null` — 복사 시 null을 `NULL`로(기본 끔 = 빈 칸).
    copy_null: bool,
    /// 행 높이 비율(% · 글꼴 높이 대비 · 설정 `grid.row_height_pct`).
    row_pct: i32,
    gutter_w: i32,
    /// 표시 순서 → 원본 컬럼 index(드래그 이동 · 재조회 시 초기화 — 사용자 09-14).
    col_order: Vec<usize>,
    /// 표시 순서 → 원본 행 index(정렬은 인덱스 벡터 · 행 복제 0 — docs/26 §4-4).
    row_order: Vec<usize>,
    /// 결합 정렬 키(원본 컬럼 · 오름차순) — 클릭 = 단일 키 3단(▲→▼→해제) · Shift+클릭 = 키 추가/토글(dir2 방식).
    sort_keys: Vec<(usize, bool)>,
    /// 헤더 드래그(표시 위치 · 시작 x · 현재 x · 4px 이상 움직임 · Shift).
    hdr_drag: Option<(usize, i32, i32, bool, bool)>,
    /// 헤더 경계 드래그 = 컬럼 폭 조절(원본 컬럼 · 시작 x · 시작 폭 — 사용자 09-14).
    hdr_resize: Option<(usize, i32, i32)>,
    /// 헤더 경계 직전 클릭(컬럼 · 시각) — 400ms 안에 같은 경계면 더블클릭 = 자동 맞춤(사용자 09-16).
    edge_click: Option<(usize, std::time::Instant)>,
    /// 다음 페인트에서 자동 맞춤할 컬럼(글꼴 측정은 페인트에서).
    autofit: Option<usize>,
    /// 자동 너비 한계(논리 px · 설정 `grid.col_min_width`/`grid.col_max_width`).
    col_min: i32,
    col_max: i32,
    /// 마우스가 올라간 행(표시 index)의 **서서히 진해지는** 강조 — `IntentFade`(70ms 머문 마지막 목표만 · 진입 = `grid.hover_fade` · 사용자 09-14).
    hover: IntentFade,
    /// ★ 선택 구간 목록(표시 행 r0..=r1 · 표시 컬럼 c0..=c1 · 마지막 = 주 구간) — 셀·범위·행 전체·Ctrl 개별(사용자 09-15 · dir2 규약).
    regions: Vec<(usize, usize, usize, usize)>,
    /// 범위 확장의 기준 셀(Shift+클릭/드래그/Shift+방향키).
    sel_anchor: Option<(usize, usize)>,
    /// 포커스 셀(테두리 · 키 이동 기준). 선택과 별개로 움직일 수 있다(Ctrl+방향키).
    sel_cur: Option<(usize, usize)>,
    /// 드래그 중(셀 범위 / 행번호 열 = 행 범위).
    drag_sel: Option<DragSel>,
    /// 우클릭 메뉴(복사 형식 · 전체 선택).
    menu: CtxMenu,
    /// 호스트가 가져갈 복사 텍스트(셀 수와 함께).
    pending_copy: Option<(String, usize)>,
    /// 메뉴에서 고른 SQL 복사 종류(호스트가 키 정보를 받아 `copy_sql`로 완성 · docs/41).
    pending_sql: Option<SqlKind>,
    /// 메뉴 단축키 문구(복사 · 전체 선택 · 호스트 주입).
    sc_copy: String,
    sc_all: String,
    /// 직전 MouseDown을 메뉴가 먹었다(항목 선택) — 호스트가 통과 여부를 판단하는 1회성 신호.
    menu_click_consumed: bool,
    /// INSERT 복사용 방언(접속 시 호스트가 알려 준다).
    dialect: Dialect,
    /// SQL 복사의 대상 테이블(실행문에서 추정 · 없으면 `T`).
    source_table: Option<String>,
    /// 실행문 원문(새로고침 · 추가 페치 · COUNT의 근거).
    source_sql: String,
    // ── 결과 도구줄(사용자 09-16 · docs/43 §4-2): 보기 모드 · 새로고침 · 편집(예정) · 세그먼트 상자 · 전체/건수 · 상태
    tb_view: Toolbar,
    tb_refresh: Toolbar,
    tb_edit: Toolbar,
    tb_fetch: Toolbar,
    /// 세그먼트 크기 입력(숫자만 · Enter 적용 · 0 = 전체).
    page_box: TextBox,
    /// 이 결과 탭의 세그먼트 크기(전역 `grid.max_rows`로 시작 · 탭 생명주기 동안 유지).
    page_rows: usize,
    /// 전역 기본(`grid.max_rows`) — 새 탭의 시작값.
    default_page_rows: usize,
    /// 설정 `grid.auto_fetch` — 스크롤 끝에서 다음 세그먼트 자동 요청.
    auto_fetch: bool,
    /// 푸터(도구줄) 높이 — 페인트가 잰다.
    footer_h: i32,
    /// 서버에 행이 더 있다(마지막 페치가 상한에서 잘림).
    more: bool,
    /// COUNT(*) 결과.
    total: Option<u64>,
    /// 추가 페치/전체/건수 요청이 워커에 나가 있다(중복 요청 금지).
    fetching: bool,
    fetch_req: Option<FetchReq>,
    want_refresh: bool,
    /// 마지막 실행 시각(`YYYY-MM-DD HH:MM:SS.mmm`).
    last_run_at: Option<String>,
    view: ResultView,
    /// SQL 보기는 호스트가 키를 받아 완성한다(`finish_view_sql`).
    pending_view: Option<SqlKind>,
    /// 텍스트 보기 본문(줄) · 가장 넓은 줄 폭(0 = 아직 안 잼) · 스크롤.
    text_lines: Vec<String>,
    text_w: i32,
    text_scroll: (i32, i32),
    /// 가장 긴 줄(문자 수 기준)의 index — 폭은 이 한 줄만 잰다(전 줄 글꼴 측정이 병목이었다 · 09-16).
    text_longest: Option<usize>,
    /// 진행 중인 변환(백그라운드 스레드 · 블록 채널 · 취소 깃발 · 진척).
    text_job: Option<TextJob>,
    /// 다음 텍스트 변환 시작 때 스크롤을 유지(추가 페치 뒤 · 처음부터 다시 그리지 않게).
    text_keep_scroll: bool,
    /// 텍스트 보기 행번호 거터 폭(페인트가 잰다 · 설정 `grid.row_numbers` · 사용자 09-16 "다른 보기에서도 행번호").
    text_gutter_w: i32,
    /// 다음 페인트 뒤 렌더·메모리 보고 1회(호스트가 로그로).
    perf_report: bool,
    /// 도구줄 상태 글자와 그 영역 — UI 글꼴 패스(상태줄과 같은 얼굴·크기)에서 호스트가 그린다(사용자 09-16).
    footer_info: Option<(String, Rect)>,
}

impl Default for Grid {
    fn default() -> Self {
        Grid {
            bounds: Rect::new(0, 0, 0, 0),
            rs: None,
            messages: Vec::new(),
            load: std::time::Duration::ZERO,
            render: std::time::Duration::ZERO,
            approx_bytes: 0,
            scroll_y: 0,
            scroll_x: 0,
            col_w: Vec::new(),
            row_h: 0,
            header_h: 0,
            bars: ScrollBars::new(),
            row_snap: false,
            row_numbers: true,
            copy_null: false,
            row_pct: 150,
            gutter_w: 0,
            col_order: Vec::new(),
            row_order: Vec::new(),
            sort_keys: Vec::new(),
            hdr_drag: None,
            hdr_resize: None,
            edge_click: None,
            autofit: None,
            col_min: 40,
            col_max: 420,
            hover: IntentFade::with_speed(FadeSpeed::Slow),
            regions: Vec::new(),
            sel_anchor: None,
            sel_cur: None,
            drag_sel: None,
            menu: CtxMenu::new(),
            pending_copy: None,
            pending_sql: None,
            sc_copy: String::new(),
            sc_all: String::new(),
            menu_click_consumed: false,
            dialect: Dialect::Oracle,
            source_table: None,
            source_sql: String::new(),
            tb_view: Self::bar(vec![ToolItem::new("view", toolicons::view_mode())
                .with_dropdown()
                .tip(t(Msg::TipViewMode))]),
            tb_refresh: Self::bar(vec![
                ToolItem::new("refresh", toolicons::refresh()).tip(t(Msg::TipRefresh))
            ]),
            tb_edit: Self::bar(vec![
                ToolItem::new("row.add", ToolIcon::Glyph("+".into()))
                    .tip(t(Msg::TipRowAdd))
                    .disabled(),
                ToolItem::new("row.del", ToolIcon::Glyph("−".into()))
                    .tip(t(Msg::TipRowDel))
                    .disabled(),
                ToolItem::new("row.dup", ToolIcon::Glyph("⧉".into()))
                    .tip(t(Msg::TipRowDup))
                    .disabled(),
                ToolItem::new("row.save", ToolIcon::Glyph("✓".into()))
                    .tip(t(Msg::TipRowSave))
                    .disabled(),
                ToolItem::new("row.cancel", ToolIcon::Glyph("✕".into()))
                    .tip(t(Msg::TipRowCancel))
                    .disabled(),
            ]),
            tb_fetch: Self::bar(vec![
                ToolItem::new("fetch.all", toolicons::fetch_all()).tip(t(Msg::TipFetchAll)),
                ToolItem::new("count", ToolIcon::Glyph("Σ".into())).tip(t(Msg::TipCount)),
            ]),
            page_box: Self::page_box(200),
            page_rows: 200,
            default_page_rows: 200,
            auto_fetch: true,
            footer_h: 0,
            more: false,
            total: None,
            fetching: false,
            fetch_req: None,
            want_refresh: false,
            last_run_at: None,
            view: ResultView::Grid,
            pending_view: None,
            text_lines: Vec::new(),
            text_w: 0,
            text_scroll: (0, 0),
            text_longest: None,
            text_job: None,
            text_keep_scroll: false,
            text_gutter_w: 0,
            perf_report: false,
            footer_info: None,
        }
    }
}

/// 정렬 비교 — 숫자는 수치로 · 문자열은 대소문자 무관 · NULL은 항상 뒤.
fn cmp_value(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    fn num(v: &Value) -> Option<f64> {
        match v {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Decimal(s) => s.parse().ok(),
            Value::Bool(b) => Some(f64::from(*b)),
            _ => None,
        }
    }
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        _ => match (num(a), num(b)) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => a.display().to_lowercase().cmp(&b.display().to_lowercase()),
        },
    }
}

impl Grid {
    /// 결과·선택·스크롤은 비우고 **설정만**(영역 · 행번호 · 스크롤 단위 · 단축키 문구 · 방언) 물려받은 새 그리드 — 새 편집기 탭의 짝(사용자 09-16).
    pub(crate) fn fresh_like(&self) -> Grid {
        Grid {
            bounds: self.bounds,
            row_snap: self.row_snap,
            row_numbers: self.row_numbers,
            copy_null: self.copy_null,
            row_pct: self.row_pct,
            sc_copy: self.sc_copy.clone(),
            sc_all: self.sc_all.clone(),
            dialect: self.dialect,
            col_min: self.col_min,
            col_max: self.col_max,
            page_box: Self::page_box(self.default_page_rows),
            page_rows: self.default_page_rows,
            default_page_rows: self.default_page_rows,
            auto_fetch: self.auto_fetch,
            ..Grid::default()
        }
    }

    fn bar(items: Vec<ToolItem>) -> Toolbar {
        let mut tb = Toolbar::new(items);
        // Golden 하단 바(≈22px) 기준: 아이콘 16 + 슬롯 2 + 바 1 → 22(사용자 09-16 "이미지는 최대한 크게 · 여백 최소").
        tb.set_icon_size(16);
        tb.set_padding(2, 1);
        tb.set_tooltip_above(true);
        tb
    }

    fn page_box(n: usize) -> TextBox {
        let mut tb = TextBox::new("200").with_text(&n.to_string());
        tb.set_char_filter(Some(|c: char| c.is_ascii_digit()));
        tb.set_max_chars(7);
        tb.set_focus_ring(true);
        tb
    }

    // ── 도구줄·페치 상태(호스트 연동)

    /// 이 결과 탭의 세그먼트 크기(0 = 전체).
    /// 결과 행의 대략 바이트(메모리 예산 D-72 · 탭 합계용).
    pub(crate) fn approx_bytes(&self) -> u64 {
        if self.rs.is_some() {
            self.approx_bytes
        } else {
            0
        }
    }

    pub(crate) fn page_rows(&self) -> usize {
        self.page_rows
    }

    /// 전역 기본(`grid.max_rows`) 변경 — 모든 결과 탭에 적용(탭에서 바꾼 값은 다음 변경 전까지 유지되지 않는다).
    pub(crate) fn set_default_page_rows(&mut self, n: usize) {
        self.default_page_rows = n;
        self.page_rows = n;
        self.page_box.set_text(&n.to_string());
    }

    /// 설정 `grid.auto_fetch`.
    pub(crate) fn set_auto_fetch(&mut self, on: bool) {
        self.auto_fetch = on;
    }

    /// 지금 가진 행 수.
    pub(crate) fn row_count(&self) -> usize {
        self.rows()
    }

    /// 실행문 원문.
    pub(crate) fn source_sql(&self) -> &str {
        &self.source_sql
    }

    /// 마지막 페치가 상한에서 잘렸다(서버에 더 있음).
    pub(crate) fn set_more(&mut self, more: bool) {
        self.more = more;
    }

    /// COUNT(*) 결과.
    pub(crate) fn set_total(&mut self, n: u64) {
        self.total = Some(n);
        self.fetching = false;
    }

    /// 추가 페치 실패 — 요청 상태만 푼다.
    pub(crate) fn fetch_failed(&mut self) {
        self.fetching = false;
    }

    /// 다음 세그먼트 이어 붙이기(정렬 중이면 다시 정렬 · 스크롤 유지).
    pub(crate) fn append_page(&mut self, page: ResultSet, more: bool) {
        self.fetching = false;
        self.more = more;
        self.approx_bytes += page.approx_bytes();
        let Some(rs) = self.rs.as_mut() else {
            return;
        };
        let start = rs.rows.len();
        rs.rows.extend(page.rows);
        self.row_order.extend(start..self.rows());
        if !self.sort_keys.is_empty() {
            self.apply_sort();
        }
        self.perf_report = true;
        if self.view != ResultView::Grid {
            // 텍스트 보기는 같은 ResultSet에서 다시 파생(표 폭이 새 행으로 바뀔 수 있어 전체 재변환 · 배경 · 스크롤 유지).
            self.text_keep_scroll = true;
            self.refresh_text_view();
        }
    }

    /// 호스트가 가져가는 페치 요청(1회성) — 가져가는 순간 진행 중으로 표시.
    pub(crate) fn take_fetch_request(&mut self) -> Option<FetchReq> {
        let r = self.fetch_req.take();
        if r.is_some() {
            self.fetching = true;
        }
        r
    }

    /// 새로고침 버튼(1회성).
    pub(crate) fn take_refresh(&mut self) -> bool {
        std::mem::take(&mut self.want_refresh)
    }

    /// SQL 보기 요청(1회성) — 호스트가 키를 받아 [`Self::finish_view_sql`].
    pub(crate) fn take_pending_view(&mut self) -> Option<SqlKind> {
        self.pending_view.take()
    }

    /// 다음 페인트 뒤 1회: 렌더·탑재 소요와 메모리(호스트가 로그로).
    pub(crate) fn take_perf_report(
        &mut self,
    ) -> Option<(std::time::Duration, std::time::Duration, u64)> {
        std::mem::take(&mut self.perf_report).then_some((self.render, self.load, self.approx_bytes))
    }

    /// 보기 모드 전환 — SQL은 키가 필요해 호스트에 미룬다.
    pub(crate) fn set_view(&mut self, view: ResultView) {
        self.view = view;
        self.text_scroll = (0, 0);
        match view {
            ResultView::Sql(k) => self.pending_view = Some(k),
            _ => self.refresh_text_view(),
        }
    }

    /// SQL 보기 완성(호스트가 키 규칙을 적용해 준다 · docs/41).
    pub(crate) fn finish_view_sql(&mut self, kind: SqlKind, key: &KeySpec) {
        if self.view != ResultView::Sql(kind) {
            return;
        }
        self.start_text_job(Format::Sql(kind), key.clone());
    }

    /// 텍스트 계열 보기 본문 다시 만들기(결과가 바뀌었을 때) — SQL은 키가 필요해 호스트에 미룬다.
    fn refresh_text_view(&mut self) {
        match self.view {
            ResultView::Grid => self.cancel_text_job(),
            ResultView::Sql(k) => {
                self.cancel_text_job();
                self.pending_view = Some(k);
            }
            v => {
                let fmt = match v {
                    ResultView::Markdown => Format::Markdown,
                    ResultView::Json => Format::Json,
                    ResultView::Tsv => Format::Tsv,
                    ResultView::Csv => Format::Csv,
                    _ => Format::Grid,
                };
                let names = self.all_col_names();
                let key = nsql_io::choose_key(nsql_io::KeyMode::All, None, &names);
                self.start_text_job(fmt, key);
            }
        }
    }

    /// ★ 지연 변환(사용자 09-16): 틀은 즉시 바꾸고 본문은 **백그라운드 스레드**가 500행 블록으로 순차 변환해 채널로
    ///   보낸다 · 다른 보기로 바꾸면 취소 깃발로 즉시 중단 · 다른 탭으로 가도 스레드는 이어서 완성(호스트 tick이 회수).
    fn start_text_job(&mut self, fmt: Format, key: KeySpec) {
        self.cancel_text_job();
        self.text_lines.clear();
        self.text_w = 0;
        self.text_longest = None;
        if !std::mem::take(&mut self.text_keep_scroll) {
            self.text_scroll = (0, 0);
        }
        let Some(rs) = self.rs.as_ref() else {
            return;
        };
        let ordered = self.ordered_rs(rs);
        let total = ordered.rows.len();
        let dialect = self.dialect;
        let table = self.source_table.clone().unwrap_or_else(|| "T".into());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<TextMsg>();
        let flag = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("nsql-textview".into())
            .spawn(move || {
                let layout = nsql_io::block_layout(&ordered, &GridOpts::default());
                let block = 500usize;
                let mut start = 0usize;
                loop {
                    if flag.load(Ordering::Relaxed) {
                        return;
                    }
                    let end = (start + block).min(total);
                    let text = nsql_io::render_block(
                        &ordered,
                        &fmt,
                        dialect,
                        &layout,
                        start..end,
                        start == 0,
                        end >= total,
                        &table,
                        &key,
                    );
                    let lines: Vec<String> = text.lines().map(str::to_string).collect();
                    if tx
                        .send(TextMsg::Chunk {
                            lines,
                            done: end,
                            total,
                        })
                        .is_err()
                    {
                        return;
                    }
                    if end >= total {
                        let _ = tx.send(TextMsg::Finished);
                        return;
                    }
                    start = end;
                }
            });
        if spawned.is_ok() {
            self.text_job = Some(TextJob {
                rx,
                cancel,
                done: 0,
                total,
            });
        }
    }

    fn cancel_text_job(&mut self) {
        if let Some(job) = self.text_job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// 변환 스레드가 보낸 블록을 거둔다(호스트 tick · 33ms) — 새 줄이 있으면 true.
    pub(crate) fn poll_text(&mut self) -> bool {
        let Some(job) = self.text_job.as_mut() else {
            return false;
        };
        let mut changed = false;
        let mut finished = false;
        while let Ok(msg) = job.rx.try_recv() {
            match msg {
                TextMsg::Chunk { lines, done, total } => {
                    let base = self.text_lines.len();
                    for (i, l) in lines.iter().enumerate() {
                        let n = l.chars().count();
                        let cur = self
                            .text_longest
                            .and_then(|j| self.text_lines.get(j))
                            .map_or(0, |t| t.chars().count());
                        if n > cur {
                            self.text_longest = Some(base + i);
                            self.text_w = 0; // 다음 페인트에서 이 줄만 잰다.
                        }
                    }
                    self.text_lines.extend(lines);
                    job.done = done;
                    job.total = total;
                    changed = true;
                }
                TextMsg::Finished => {
                    finished = true;
                    changed = true;
                }
            }
        }
        if finished {
            self.text_job = None;
        }
        changed
    }

    /// 변환이 진행 중인가(호스트가 tick을 돌릴 근거).
    pub(crate) fn text_pending(&self) -> bool {
        self.text_job.is_some()
    }

    /// 표시 순서(정렬·컬럼 이동)대로 복제한 결과 — 텍스트 보기·SQL 보기의 원천.
    fn ordered_rs(&self, rs: &ResultSet) -> ResultSet {
        let cols: Vec<usize> = self.col_order.clone();
        ResultSet {
            columns: cols
                .iter()
                .filter_map(|&ci| rs.columns.get(ci).cloned())
                .collect(),
            rows: self
                .row_order
                .iter()
                .filter_map(|&ri| rs.rows.get(ri))
                .map(|row| {
                    cols.iter()
                        .map(|&ci| row.get(ci).cloned().unwrap_or(Value::Null))
                        .collect()
                })
                .collect(),
        }
    }

    /// 전 컬럼 이름(SQL 보기의 키 선택 근거).
    pub(crate) fn all_col_names(&self) -> Vec<String> {
        let Some(rs) = self.rs.as_ref() else {
            return Vec::new();
        };
        self.col_order
            .iter()
            .filter_map(|&ci| rs.columns.get(ci).map(|c| c.name.clone()))
            .collect()
    }

    /// 도구줄 툴팁·우클릭 메뉴 — 호스트가 최상위 패스(UI 글꼴)에서 그린다.
    pub(crate) fn paint_overlays(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        for tb in [
            &self.tb_view,
            &self.tb_refresh,
            &self.tb_edit,
            &self.tb_fetch,
        ] {
            tb.paint_tooltip(dc, th);
        }
        self.menu.paint(dc, th);
    }

    fn footer_rect(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x, b.bottom() - self.footer_h, b.w, self.footer_h)
    }

    /// 도구줄 4묶음·세그먼트 상자에 마우스/키를 배선한다(마우스 라우팅 규칙: 커서 아래 컨트롤에만). 소비되면 true.
    fn footer_event(&mut self, ev: &InputEvent, scale: f32) -> bool {
        if self.footer_h <= 0 {
            return false;
        }
        let footer = self.footer_rect();
        let mut inv = Invalidations::default();
        let at = |x: i32, y: i32| Point { x, y };
        match *ev {
            InputEvent::MouseMove { x, y } => {
                for tb in [
                    &mut self.tb_view,
                    &mut self.tb_refresh,
                    &mut self.tb_edit,
                    &mut self.tb_fetch,
                ] {
                    tb.on_event(ev, &mut inv);
                }
                if self.page_box.is_focused() || self.page_box.bounds().contains(at(x, y)) {
                    self.page_box.on_event(ev, &mut inv);
                }
                return false;
            }
            InputEvent::MouseDown { x, y, .. } | InputEvent::MouseUp { x, y } => {
                let p = at(x, y);
                let down = matches!(ev, InputEvent::MouseDown { .. });
                if down && !self.page_box.bounds().contains(p) {
                    self.page_box.set_focused(false);
                }
                if !footer.contains(p) {
                    return false;
                }
                let mut clicked: Option<&'static str> = None;
                for (tb, ids) in [
                    (&mut self.tb_view, &["view"][..]),
                    (&mut self.tb_refresh, &["refresh"][..]),
                    (&mut self.tb_edit, &[][..]),
                    (&mut self.tb_fetch, &["fetch.all", "count"][..]),
                ] {
                    if tb.bounds().contains(p) || !down {
                        tb.on_event(ev, &mut inv);
                        if let Some(id) = tb.take_clicked() {
                            clicked = ids.iter().copied().find(|s| *s == id);
                        }
                    }
                }
                if self.page_box.bounds().contains(p) || (!down && self.page_box.is_focused()) {
                    self.page_box.on_event(ev, &mut inv);
                }
                match clicked {
                    Some("view") => {
                        let r = self.tb_view.bounds();
                        self.open_view_menu(r.x, r.y, scale);
                    }
                    Some("refresh") => self.want_refresh = true,
                    Some("fetch.all") if !self.fetching && self.rs.is_some() => {
                        self.fetch_req = Some(FetchReq::All);
                    }
                    Some("count") if !self.fetching && !self.source_sql.trim().is_empty() => {
                        self.fetch_req = Some(FetchReq::Count);
                    }
                    _ => {}
                }
                return true;
            }
            _ => {}
        }
        // 키·문자 = 세그먼트 상자에 포커스가 있을 때만.
        if self.page_box.is_focused() {
            self.page_box.on_event(ev, &mut inv);
            if let Some(text) = self.page_box.take_committed() {
                self.page_rows = text.trim().parse().unwrap_or(self.page_rows);
                self.page_box.set_text(&self.page_rows.to_string());
                self.page_box.set_focused(false);
            } else if let Some(text) = self.page_box.take_changed() {
                if let Ok(n) = text.trim().parse::<usize>() {
                    self.page_rows = n;
                }
            }
            return true;
        }
        false
    }

    /// 보기 모드 메뉴(도구줄 그룹 1 · ▦ 버튼 아래).
    fn open_view_menu(&mut self, x: i32, y: i32, scale: f32) {
        self.menu.set_scale(scale);
        let cur = self.view;
        // 이미지 아이콘 + 현재 모드 = 강조색(사용자 09-16 · 토글 도형 대신).
        let it = |id: &str, m: Msg, v: ResultView, ic: nexa_ctl::MenuIcon| {
            CtxItem::item(id, t(m))
                .with_icon(Some(ic))
                .with_active(cur == v)
        };
        let sql_kinds = [
            ("view:sql_select", Msg::MnCopySqlSelect, SqlKind::Select),
            ("view:sql_insert", Msg::MnCopySqlInsert, SqlKind::Insert),
            ("view:sql_update", Msg::MnCopySqlUpdate, SqlKind::Update),
            ("view:sql_delete", Msg::MnCopySqlDelete, SqlKind::Delete),
            ("view:sql_merge", Msg::MnCopySqlMerge, SqlKind::Merge),
        ];
        let sql: Vec<CtxItem> = sql_kinds
            .iter()
            .map(|(id, m, k)| it(id, *m, ResultView::Sql(*k), toolicons::mi_db()))
            .collect();
        let items = vec![
            it(
                "view:grid",
                Msg::MnViewGrid,
                ResultView::Grid,
                toolicons::mi_table(),
            ),
            it(
                "view:text",
                Msg::MnViewText,
                ResultView::Text,
                toolicons::mi_files(),
            ),
            it(
                "view:markdown",
                Msg::MnViewMarkdown,
                ResultView::Markdown,
                toolicons::mi_files(),
            ),
            it(
                "view:json",
                Msg::MnViewJson,
                ResultView::Json,
                toolicons::mi_braces(),
            ),
            it(
                "view:tsv",
                Msg::MnViewTsv,
                ResultView::Tsv,
                toolicons::mi_table(),
            ),
            it(
                "view:csv",
                Msg::MnViewCsv,
                ResultView::Csv,
                toolicons::mi_table(),
            ),
            CtxItem::submenu("view:sql", t(Msg::MnViewSql), sql)
                .with_icon(Some(toolicons::mi_db()))
                .with_active(matches!(cur, ResultView::Sql(_))),
        ];
        let text_w = (self.row_h * 8).max(140);
        self.menu.open_at(x, y, items, self.bounds, text_w);
    }

    /// 텍스트 계열 보기의 스크롤·키(그리드 대신).
    fn text_view_event(&mut self, ev: &InputEvent, scale: f32) {
        if self.row_h <= 0 {
            return;
        }
        let body = self.text_body_rect();
        let (cw, ch) = self.text_content_size();
        let (nx, ny, consumed) = self.bars.on_event(
            ev,
            body,
            cw.max(body.w),
            ch.max(body.h),
            self.text_scroll.0,
            self.text_scroll.1,
            scale,
        );
        self.text_scroll = (nx, ny);
        if consumed {
            return;
        }
        let page = body.h.max(self.row_h);
        match ev {
            InputEvent::Key {
                key: Key::PageDown, ..
            } => self.text_scroll.1 += page,
            InputEvent::Key {
                key: Key::PageUp, ..
            } => self.text_scroll.1 -= page,
            InputEvent::Key { key: Key::Home, .. } => self.text_scroll.1 = 0,
            InputEvent::Key { key: Key::End, .. } => self.text_scroll.1 = i32::MAX / 2,
            InputEvent::Key { key: Key::Down, .. } => self.text_scroll.1 += self.row_h,
            InputEvent::Key { key: Key::Up, .. } => self.text_scroll.1 -= self.row_h,
            InputEvent::Key { key: Key::Left, .. } => self.text_scroll.0 -= self.row_h * 2,
            InputEvent::Key {
                key: Key::Right, ..
            } => self.text_scroll.0 += self.row_h * 2,
            _ => {}
        }
        let mx = (cw - body.w).max(0);
        let my = (ch - body.h).max(0);
        self.text_scroll = (
            self.text_scroll.0.clamp(0, mx),
            self.text_scroll.1.clamp(0, my),
        );
        // ★ 텍스트 계열 보기도 스크롤 끝 = 다음 세그먼트(사용자 09-16 · 데이터 원천은 그리드와 같은 ResultSet) · 변환 중이면 미룸.
        if self.auto_fetch
            && self.more
            && !self.fetching
            && self.fetch_req.is_none()
            && self.text_job.is_none()
            && self.page_rows > 0
            && my > 0
            && self.text_scroll.1 >= my - self.row_h.max(1)
        {
            self.fetch_req = Some(FetchReq::Next {
                offset: self.rows(),
                limit: self.page_rows,
            });
        }
    }

    /// 텍스트 보기 본문(행번호 거터 제외 · 스크롤바 뷰포트).
    fn text_body_rect(&self) -> Rect {
        let b = self.bounds;
        Rect::new(
            b.x + self.text_gutter_w,
            b.y + 1,
            (b.w - self.text_gutter_w).max(0),
            (b.h - 1 - self.footer_h).max(0),
        )
    }

    fn text_content_size(&self) -> (i32, i32) {
        (self.text_w, self.row_h * self.text_lines.len() as i32)
    }

    /// 자동 컬럼 너비 한계(설정 `grid.col_min_width`/`grid.col_max_width` · 논리 px).
    pub(crate) fn set_col_limits(&mut self, min: i32, max: i32) {
        self.col_min = min.max(1);
        self.col_max = max.max(self.col_min);
    }

    pub(crate) fn set_row_numbers(&mut self, on: bool) {
        self.row_numbers = on;
    }

    /// 복사할 때 null을 `NULL` 글자로(설정 `grid.copy_null` · 기본 끔 = 빈 칸 · 사용자 09-16).
    /// 행 높이 비율(% · 설정 `grid.row_height_pct`).
    pub(crate) fn set_row_pct(&mut self, pct: i32) {
        self.row_pct = pct.clamp(110, 300);
    }

    pub(crate) fn set_copy_null(&mut self, on: bool) {
        self.copy_null = on;
    }

    /// 스크롤 단위 — `true` = 항목(행) 단위 · `false` = 픽셀(기본).
    pub(crate) fn set_row_snap(&mut self, on: bool) {
        self.row_snap = on;
        self.clamp();
    }

    pub(crate) fn set_bounds(&mut self, b: Rect) {
        self.bounds = b;
        self.clamp();
    }

    pub(crate) fn set_result(&mut self, rs: ResultSet) {
        let t = std::time::Instant::now();
        self.approx_bytes = rs.approx_bytes();
        // 재조회 = 조회 컬럼 순서·정렬 초기화(이동·정렬 결과 무시 — 사용자 09-14).
        self.col_order = (0..rs.columns.len()).collect();
        self.row_order = (0..rs.rows.len()).collect();
        self.sort_keys.clear();
        self.hdr_drag = None;
        self.hdr_resize = None;
        self.rs = Some(rs);
        self.messages.clear();
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.col_w.clear();
        self.regions.clear();
        self.sel_anchor = None;
        self.sel_cur = None;
        self.drag_sel = None;
        self.menu.close();
        self.more = false;
        self.total = None;
        self.fetching = false;
        self.fetch_req = None;
        self.last_run_at = Some(nsql_log::now_local().stamp());
        self.perf_report = true;
        self.text_scroll = (0, 0);
        self.load = t.elapsed();
        if self.view != ResultView::Grid {
            self.refresh_text_view();
        }
    }

    /// 실행 직전 호스트가 알려 주는 원본 문장 — SQL 복사의 테이블 이름 근거.
    pub(crate) fn set_source_sql(&mut self, sql: &str) {
        self.source_table = guess_table(sql);
        self.source_sql = sql.to_string();
    }

    pub(crate) fn set_dialect(&mut self, d: Dialect) {
        self.dialect = d;
    }

    /// 직전 클릭을 메뉴가 먹었는가(1회성) — 참이면 호스트는 그 클릭을 아래로 흘리지 않는다.
    pub(crate) fn take_menu_click(&mut self) -> bool {
        std::mem::take(&mut self.menu_click_consumed)
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// 호스트가 클립보드에 쓸 텍스트(셀 수).
    pub(crate) fn take_copy(&mut self) -> Option<(String, usize)> {
        self.pending_copy.take()
    }

    /// 메뉴에서 고른 SQL 종류(1회성) — 호스트가 키를 정한 뒤 [`Self::copy_sql`].
    pub(crate) fn take_pending_sql(&mut self) -> Option<SqlKind> {
        self.pending_sql.take()
    }

    /// 실행문에서 추정한 대상 테이블.
    pub(crate) fn source_table(&self) -> Option<String> {
        self.source_table.clone()
    }

    /// 선택 구간의 컬럼 이름(표시 순서 · 중복 없이) — 키 선택의 입력.
    pub(crate) fn selected_col_names(&self) -> Vec<String> {
        let Some(rs) = self.rs.as_ref() else {
            return Vec::new();
        };
        let mut regions = self.regions.clone();
        regions.sort_unstable();
        let mut names: Vec<String> = Vec::new();
        for (_, _, c0, c1) in regions {
            for &ci in &self.col_order[c0..=c1.min(self.col_order.len().saturating_sub(1))] {
                if let Some(c) = rs.columns.get(ci) {
                    if !names.iter().any(|n| n.eq_ignore_ascii_case(&c.name)) {
                        names.push(c.name.clone());
                    }
                }
            }
        }
        names
    }

    /// 선택 구간 → SQL 문(구간마다 · 행마다 · 공용 생성기 · 키 = 호스트가 고른 `key`).
    pub(crate) fn copy_sql(&self, kind: SqlKind, key: &KeySpec) -> Option<(String, usize)> {
        let rs = self.rs.as_ref()?;
        let table = self.source_table.clone().unwrap_or_else(|| "T".into());
        let mut regions = self.regions.clone();
        regions.sort_unstable();
        regions.dedup();
        let mut out = String::new();
        let mut cells = 0usize;
        for (i, (r0, r1, c0, c1)) in regions.into_iter().enumerate() {
            let cols: Vec<usize> = self.col_order[c0..=c1.min(self.col_order.len() - 1)].to_vec();
            let names: Vec<String> = cols
                .iter()
                .map(|&ci| {
                    rs.columns
                        .get(ci)
                        .map(|c| c.name.clone())
                        .unwrap_or_default()
                })
                .collect();
            let mut rows: Vec<Vec<Value>> = Vec::new();
            for di in r0..=r1.min(self.rows().saturating_sub(1)) {
                let ri = self.row_order.get(di).copied().unwrap_or(di);
                let Some(row) = rs.rows.get(ri) else { continue };
                rows.push(
                    cols.iter()
                        .map(|&ci| row.get(ci).cloned().unwrap_or(Value::Null))
                        .collect(),
                );
                cells += cols.len();
            }
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&generate(self.dialect, &table, &names, &rows, kind, key));
        }
        (cells > 0).then_some((out, cells))
    }

    fn in_sel(&self, di: usize, pos: usize) -> bool {
        self.regions
            .iter()
            .any(|&(r0, r1, c0, c1)| di >= r0 && di <= r1 && pos >= c0 && pos <= c1)
    }

    /// 행이 선택에 걸리는가(행번호 열 강조).
    fn row_in_sel(&self, di: usize) -> bool {
        self.regions
            .iter()
            .any(|&(r0, r1, _, _)| di >= r0 && di <= r1)
    }

    /// 컬럼(표시 위치)이 선택에 걸리는가(헤더 강조).
    fn col_in_sel(&self, pos: usize) -> bool {
        self.regions
            .iter()
            .any(|&(_, _, c0, c1)| pos >= c0 && pos <= c1)
    }

    fn rect_of(a: (usize, usize), b: (usize, usize)) -> (usize, usize, usize, usize) {
        (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1))
    }

    fn last_col(&self) -> usize {
        self.col_order.len().saturating_sub(1)
    }

    /// 단일 셀/범위 선택(기존 해제) — 평 클릭·평 이동.
    fn select_only(&mut self, cell: (usize, usize)) {
        self.regions = vec![Self::rect_of(cell, cell)];
        self.sel_anchor = Some(cell);
        self.sel_cur = Some(cell);
    }

    /// 앵커~셀 범위로 주 구간을 바꾼다(Shift).
    fn extend_to(&mut self, cell: (usize, usize)) {
        let a = self.sel_anchor.unwrap_or(cell);
        let r = Self::rect_of(a, cell);
        match self.regions.last_mut() {
            Some(last) => *last = r,
            None => self.regions.push(r),
        }
        self.sel_cur = Some(cell);
    }

    /// 구간 토글(Ctrl) — 정확히 같은 구간이 있으면 빼고, 아니면 추가(주 구간이 된다).
    fn toggle_region(&mut self, r: (usize, usize, usize, usize)) {
        if let Some(i) = self.regions.iter().position(|x| *x == r) {
            self.regions.remove(i);
        } else {
            self.regions.push(r);
        }
    }

    /// 행 전체 구간.
    fn row_region(&self, r0: usize, r1: usize) -> (usize, usize, usize, usize) {
        (r0.min(r1), r0.max(r1), 0, self.last_col())
    }

    pub(crate) fn select_all(&mut self) {
        let n = self.rows();
        if n == 0 || self.col_order.is_empty() {
            return;
        }
        self.regions = vec![(0, n - 1, 0, self.last_col())];
        self.sel_anchor = Some((0, 0));
        self.sel_cur = Some((n - 1, self.last_col()));
    }

    /// 선택 요약(행 수 · 컬럼 수 · 셀 수) — 상태줄용. 구간이 겹치면 셀은 합집합으로 센다.
    pub(crate) fn selection_summary(&self) -> Option<(usize, usize, usize)> {
        if self.regions.is_empty() {
            return None;
        }
        let mut rows = std::collections::BTreeSet::new();
        let mut cols = std::collections::BTreeSet::new();
        let mut cells = std::collections::BTreeSet::new();
        for &(r0, r1, c0, c1) in &self.regions {
            for r in r0..=r1 {
                rows.insert(r);
                for c in c0..=c1 {
                    cols.insert(c);
                    cells.insert((r, c));
                }
            }
        }
        Some((rows.len(), cols.len(), cells.len()))
    }

    /// 선택 셀을 형식대로 텍스트로(없으면 None). 표시 순서(정렬·컬럼 이동 반영).
    pub(crate) fn copy_selection(&self, kind: CopyKind) -> Option<(String, usize)> {
        if self.view != ResultView::Grid {
            let text = self.text_lines.join("\n");
            return (!text.is_empty()).then_some((text, self.text_lines.len()));
        }
        if self.regions.is_empty() {
            return None;
        }
        let mut out = String::new();
        let mut cells = 0usize;
        // 구간은 행 순서로(Ctrl로 모은 개별 행이 원래 순서로 나온다) · 구간 사이 빈 줄.
        let mut regions = self.regions.clone();
        regions.sort_unstable();
        regions.dedup();
        for (i, r) in regions.iter().enumerate() {
            if let Some((text, n)) = self.copy_region(kind, *r, i == 0) {
                if i > 0 {
                    out.push('\n');
                }
                out.push_str(&text);
                cells += n;
            }
        }
        (cells > 0).then_some((out, cells))
    }

    /// 구간 하나를 형식대로(첫 구간만 헤더).
    fn copy_region(
        &self,
        kind: CopyKind,
        region: (usize, usize, usize, usize),
        first: bool,
    ) -> Option<(String, usize)> {
        let rs = self.rs.as_ref()?;
        let (r0, r1, c0, c1) = region;
        let cols: Vec<usize> = self.col_order[c0..=c1.min(self.col_order.len() - 1)].to_vec();
        let names: Vec<String> = cols
            .iter()
            .map(|&ci| {
                rs.columns
                    .get(ci)
                    .map(|c| c.name.clone())
                    .unwrap_or_default()
            })
            .collect();
        let mut out = String::new();
        let mut cells = 0usize;
        let copy_null = self.copy_null;
        let plain = |v: &Value| match v {
            Value::Null if copy_null => "NULL".into(),
            Value::Null => String::new(),
            other => other.display(),
        };
        match kind {
            CopyKind::Tsv | CopyKind::TsvWithHeaders => {
                if kind == CopyKind::TsvWithHeaders && first {
                    out.push_str(&names.join("\t"));
                    out.push('\n');
                }
                for di in r0..=r1.min(self.rows().saturating_sub(1)) {
                    let ri = self.row_order.get(di).copied().unwrap_or(di);
                    let Some(row) = rs.rows.get(ri) else { continue };
                    let line: Vec<String> = cols
                        .iter()
                        .map(|&ci| row.get(ci).map(plain).unwrap_or_default())
                        .collect();
                    cells += line.len();
                    out.push_str(&line.join("\t"));
                    out.push('\n');
                }
            }
            CopyKind::Csv => {
                if first {
                    out.push_str(
                        &names
                            .iter()
                            .map(|n| nsql_io::quote_field(n, b','))
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                    out.push('\n');
                }
                for di in r0..=r1.min(self.rows().saturating_sub(1)) {
                    let ri = self.row_order.get(di).copied().unwrap_or(di);
                    let Some(row) = rs.rows.get(ri) else { continue };
                    let line: Vec<String> = cols
                        .iter()
                        .map(|&ci| {
                            nsql_io::quote_field(&row.get(ci).map(plain).unwrap_or_default(), b',')
                        })
                        .collect();
                    cells += line.len();
                    out.push_str(&line.join(","));
                    out.push('\n');
                }
            }
            CopyKind::Text => {
                // 고정폭 정렬(표시 폭 기준 · 숫자 우측 정렬) — 첫 구간만 머리글.
                let mut lines: Vec<Vec<String>> = Vec::new();
                let mut numeric: Vec<bool> = vec![false; cols.len()];
                for di in r0..=r1.min(self.rows().saturating_sub(1)) {
                    let ri = self.row_order.get(di).copied().unwrap_or(di);
                    let Some(row) = rs.rows.get(ri) else { continue };
                    let line: Vec<String> = cols
                        .iter()
                        .enumerate()
                        .map(|(k, &ci)| {
                            if let Some(v) = row.get(ci) {
                                if matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_))
                                {
                                    numeric[k] = true;
                                }
                            }
                            row.get(ci).map(plain).unwrap_or_default()
                        })
                        .collect();
                    cells += line.len();
                    lines.push(line);
                }
                let mut widths: Vec<usize> = names.iter().map(|n| nsql_io::disp_width(n)).collect();
                for l in &lines {
                    for (k, c) in l.iter().enumerate() {
                        widths[k] = widths[k].max(nsql_io::disp_width(c));
                    }
                }
                let pad = |s: &str, w: usize, right: bool| -> String {
                    let fill = w.saturating_sub(nsql_io::disp_width(s));
                    if right {
                        format!("{}{}", " ".repeat(fill), s)
                    } else {
                        format!("{}{}", s, " ".repeat(fill))
                    }
                };
                if first {
                    let h: Vec<String> = names
                        .iter()
                        .enumerate()
                        .map(|(k, n)| pad(n, widths[k], false))
                        .collect();
                    out.push_str(h.join("  ").trim_end());
                    out.push('\n');
                    let u: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
                    out.push_str(&u.join("  "));
                    out.push('\n');
                }
                for l in &lines {
                    let cells_s: Vec<String> = l
                        .iter()
                        .enumerate()
                        .map(|(k, c)| pad(c, widths[k], numeric[k]))
                        .collect();
                    out.push_str(cells_s.join("  ").trim_end());
                    out.push('\n');
                }
            }
            CopyKind::Markdown => {
                let esc = |s: &str| s.replace('|', "\\|").replace('\n', " ");
                if first {
                    out.push_str(&format!(
                        "| {} |\n",
                        names.iter().map(|n| esc(n)).collect::<Vec<_>>().join(" | ")
                    ));
                    out.push_str(&format!(
                        "|{}|\n",
                        names.iter().map(|_| " --- ").collect::<Vec<_>>().join("|")
                    ));
                }
                for di in r0..=r1.min(self.rows().saturating_sub(1)) {
                    let ri = self.row_order.get(di).copied().unwrap_or(di);
                    let Some(row) = rs.rows.get(ri) else { continue };
                    let line: Vec<String> = cols
                        .iter()
                        .map(|&ci| esc(&row.get(ci).map(plain).unwrap_or_default()))
                        .collect();
                    cells += line.len();
                    out.push_str(&format!("| {} |\n", line.join(" | ")));
                }
            }
            CopyKind::Json => {
                // 구간마다 배열 하나(여러 구간이면 배열이 여러 개 — 각각 독립 문서).
                out.push_str("[\n");
                let mut rows_out: Vec<String> = Vec::new();
                for di in r0..=r1.min(self.rows().saturating_sub(1)) {
                    let ri = self.row_order.get(di).copied().unwrap_or(di);
                    let Some(row) = rs.rows.get(ri) else { continue };
                    let fields: Vec<String> = cols
                        .iter()
                        .zip(names.iter())
                        .map(|(&ci, n)| {
                            let v = row
                                .get(ci)
                                .map(nsql_io::json_value)
                                .unwrap_or_else(|| "null".into());
                            format!("    {}: {}", nsql_io::json_str(n), v)
                        })
                        .collect();
                    cells += fields.len();
                    rows_out.push(format!("  {{\n{}\n  }}", fields.join(",\n")));
                }
                out.push_str(&rows_out.join(",\n"));
                out.push_str("\n]\n");
            }
        }
        (cells > 0).then_some((out, cells))
    }

    /// 마우스 아래 셀(표시 행 · 표시 컬럼 위치). 행번호 열 위면 컬럼 0.
    fn cell_at_point(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let di = self.row_at_point(x, y)?;
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        if x < self.bounds.x + self.gutter_w {
            return Some((di, 0));
        }
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            if x >= cx && x < cx + cw {
                return Some((di, pos));
            }
            cx += cw;
        }
        Some((di, self.col_order.len().saturating_sub(1)))
    }

    /// 현재 셀이 보이도록 스크롤.
    fn ensure_cell_visible(&mut self, di: usize, pos: usize) {
        if self.row_h <= 0 {
            return;
        }
        let top = di as i32 * self.row_h;
        let vis_h = self.body_h();
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + vis_h {
            self.scroll_y = top + self.row_h - vis_h;
        }
        let mut cx = 0;
        for (p, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            if p == pos {
                let vis_w = self.bounds.w - self.gutter_w;
                if cx < self.scroll_x {
                    self.scroll_x = cx;
                } else if cx + cw > self.scroll_x + vis_w {
                    self.scroll_x = cx + cw - vis_w;
                }
                break;
            }
            cx += cw;
        }
        self.clamp();
    }

    /// 포커스 셀을 `(nr, nc)`로 — `shift` = 앵커~셀 범위 · `ctrl` = 포커스만(선택 유지) · 평 = 단일 선택.
    fn go_to(&mut self, nr: usize, nc: usize, shift: bool, ctrl: bool) {
        if shift {
            if self.sel_anchor.is_none() {
                self.sel_anchor = self.sel_cur.or(Some((nr, nc)));
            }
            self.extend_to((nr, nc));
        } else if ctrl {
            self.sel_cur = Some((nr, nc));
        } else {
            self.select_only((nr, nc));
        }
        self.ensure_cell_visible(nr, nc);
    }

    fn move_sel(&mut self, dr: i32, dc: i32, shift: bool, ctrl: bool) {
        let n = self.rows();
        let m = self.col_order.len();
        if n == 0 || m == 0 {
            return;
        }
        let (r, c) = self.sel_cur.unwrap_or((0, 0));
        let nr = (r as i32 + dr).clamp(0, n as i32 - 1) as usize;
        let nc = (c as i32 + dc).clamp(0, m as i32 - 1) as usize;
        self.go_to(nr, nc, shift, ctrl);
    }

    /// 행번호 열(고정) 위인가.
    fn in_gutter(&self, x: i32) -> bool {
        self.gutter_w > 0 && x >= self.bounds.x && x < self.bounds.x + self.gutter_w
    }

    /// 단축키 문구(복사 · 전체 선택) — 호스트 키맵에서 주입(표시 전용).
    pub(crate) fn set_shortcuts(&mut self, copy: String, select_all: String) {
        self.sc_copy = copy;
        self.sc_all = select_all;
    }

    /// 우클릭 메뉴(DBeaver Advanced Copy 구조 · 사용자 09-15): 복사(아이콘·단축키) · 머리글 포함 · **Advanced Copy ▸**
    /// (CSV · 텍스트 · Markdown · JSON · **SQL ▸** SELECT/INSERT/UPDATE/DELETE/MERGE) · 전체 선택 — 진짜 하위 메뉴(nexa-ctl).
    fn open_menu(&mut self, x: i32, y: i32, scale: f32) {
        // ★ 배율을 메뉴에 넘긴다 — 빠져 있어서 맥 2x에서 행 높이·여백이 1x 값(절반)으로 계산돼 항목이 겹쳐 보였다(09-16).
        self.menu.set_scale(scale);
        let has = !self.regions.is_empty();
        let sql = vec![
            CtxItem::item("sql_select", t(Msg::MnCopySqlSelect)),
            CtxItem::item("sql_insert", t(Msg::MnCopySqlInsert)),
            CtxItem::item("sql_update", t(Msg::MnCopySqlUpdate)),
            CtxItem::item("sql_delete", t(Msg::MnCopySqlDelete)),
            CtxItem::item("sql_merge", t(Msg::MnCopySqlMerge)),
        ];
        let adv = vec![
            CtxItem::item("copy_csv", t(Msg::MnCopyCsv)).with_icon(Some(toolicons::mi_table())),
            CtxItem::item("copy_txt", t(Msg::MnCopyText)).with_icon(Some(toolicons::mi_table())),
            CtxItem::item("copy_md", t(Msg::MnCopyMarkdown)).with_icon(Some(toolicons::mi_table())),
            CtxItem::item("copy_json", t(Msg::MnCopyJson)).with_icon(Some(toolicons::mi_braces())),
            CtxItem::submenu("copy_sql", t(Msg::MnCopySql), sql)
                .with_icon(Some(toolicons::mi_db())),
        ];
        let mut adv_item = CtxItem::submenu("adv", t(Msg::MnAdvancedCopy), adv);
        if let CtxItem::Item { enabled, .. } = &mut adv_item {
            *enabled = has;
        }
        let items = vec![
            CtxItem::maybe("copy", t(Msg::MnCopy), has)
                .with_icon(Some(toolicons::mi_copy()))
                .with_shortcut(self.sc_copy.clone()),
            CtxItem::maybe("copy_h", t(Msg::MnCopyWithHeaders), has)
                .with_icon(Some(toolicons::mi_copy())),
            adv_item,
            CtxItem::Separator,
            CtxItem::item("all", t(Msg::MnSelectAll))
                .with_icon(Some(toolicons::mi_select_all()))
                .with_shortcut(self.sc_all.clone()),
        ];
        let text_w = (self.row_h * 10).max(180);
        self.menu.open_at(x, y, items, self.bounds, text_w);
    }

    fn menu_pick(&mut self, id: &str) {
        if let Some(v) = id.strip_prefix("view:") {
            let view = match v {
                "grid" => ResultView::Grid,
                "text" => ResultView::Text,
                "markdown" => ResultView::Markdown,
                "json" => ResultView::Json,
                "tsv" => ResultView::Tsv,
                "csv" => ResultView::Csv,
                other => match other.strip_prefix("sql_").and_then(SqlKind::parse) {
                    Some(k) => ResultView::Sql(k),
                    None => return,
                },
            };
            self.set_view(view);
            return;
        }
        // SQL 종류는 호스트가 키(PK/유니크)를 받아 완성한다(docs/41).
        if let Some(k) = id.strip_prefix("sql_").and_then(SqlKind::parse) {
            self.pending_sql = Some(k);
            return;
        }
        let kind = match id {
            "copy" => CopyKind::Tsv,
            "copy_h" => CopyKind::TsvWithHeaders,
            "copy_csv" => CopyKind::Csv,
            "copy_txt" => CopyKind::Text,
            "copy_md" => CopyKind::Markdown,
            "copy_json" => CopyKind::Json,
            "all" => {
                self.select_all();
                return;
            }
            _ => return,
        };
        self.pending_copy = self.copy_selection(kind);
    }

    /// 결합 정렬 적용(인덱스 벡터만 재배열 · 안정 정렬이라 같은 값은 원본 순서).
    fn apply_sort(&mut self) {
        let Some(rs) = self.rs.as_ref() else { return };
        let mut order: Vec<usize> = (0..rs.rows.len()).collect();
        if !self.sort_keys.is_empty() {
            let keys = self.sort_keys.clone();
            order.sort_by(|&a, &b| {
                for (col, asc) in &keys {
                    let (va, vb) = (&rs.rows[a][*col], &rs.rows[b][*col]);
                    let o = cmp_value(va, vb);
                    if o != std::cmp::Ordering::Equal {
                        return if *asc { o } else { o.reverse() };
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        self.row_order = order;
    }

    /// 헤더 클릭 정렬. `additive`(Shift) = 결합 키 추가/토글 · 아니면 단일 키 3단(▲ → ▼ → 해제).
    fn toggle_sort(&mut self, col: usize, additive: bool) {
        let pos = self.sort_keys.iter().position(|(c, _)| *c == col);
        match (additive, pos) {
            (false, Some(0)) if self.sort_keys.len() == 1 => {
                if self.sort_keys[0].1 {
                    self.sort_keys[0].1 = false;
                } else {
                    self.sort_keys.clear();
                }
            }
            (false, _) => self.sort_keys = vec![(col, true)],
            (true, Some(i)) => {
                if self.sort_keys[i].1 {
                    self.sort_keys[i].1 = false;
                } else {
                    self.sort_keys.remove(i);
                }
            }
            (true, None) => self.sort_keys.push((col, true)),
        }
        self.apply_sort();
    }

    /// 헤더 영역(x → 표시 위치). 행번호 열은 제외.
    fn header_pos_at(&self, x: i32) -> Option<usize> {
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            if x >= cx && x < cx + cw {
                return Some(pos);
            }
            cx += cw;
        }
        None
    }

    /// 헤더에서 컬럼 오른쪽 경계 ±4px 안이면 그 컬럼(원본 index) — 폭 조절 손잡이.
    fn header_edge_at(&self, x: i32) -> Option<usize> {
        let grip = 6;
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        for &ci in &self.col_order {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            cx += cw;
            if (x - cx).abs() <= grip {
                return Some(ci);
            }
        }
        None
    }

    /// 커서가 헤더의 컬럼 경계(폭 조절 손잡이) 위인가 — 호스트가 커서 모양을 바꾼다.
    pub(crate) fn header_edge_hover(&self, x: i32, y: i32) -> bool {
        self.hdr_resize.is_some()
            || (self.rs.is_some()
                && self.row_h > 0
                && self.header_rect().contains(Point { x, y })
                && self.header_edge_at(x).is_some())
    }

    fn header_rect(&self) -> Rect {
        Rect::new(self.bounds.x, self.bounds.y + 1, self.bounds.w, self.row_h)
    }

    /// 드래그 목표 위치(현재 x 기준 · 컬럼 중앙을 넘으면 그 다음).
    fn drop_pos_at(&self, x: i32) -> usize {
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            if x < cx + cw / 2 {
                return pos;
            }
            cx += cw;
        }
        self.col_order.len().saturating_sub(1)
    }

    pub(crate) fn set_messages(&mut self, m: Vec<String>) {
        self.messages = m;
    }

    fn rows(&self) -> usize {
        self.rs.as_ref().map_or(0, |r| r.rows.len())
    }

    /// 행 영역 높이(헤더·푸터 제외).
    fn body_h(&self) -> i32 {
        (self.bounds.h - self.header_h - self.footer_h).max(0)
    }

    /// 콘텐츠 크기(스크롤 범위) — 헤더 + 전 행 · 컬럼 폭 합.
    fn content_size(&self) -> (i32, i32) {
        let w: i32 = self.col_w.iter().sum();
        let h = self.header_h + self.row_h * self.rows() as i32;
        (w, h)
    }

    /// 그릴 때 쓰는 세로 오프셋 — 행 단위 모드면 행 경계로 내림(맨 아래는 마지막 행이 다 보이도록 그대로).
    fn view_y(&self) -> i32 {
        if self.row_snap && self.row_h > 0 {
            let (_, my) = self.max_scroll();
            if self.scroll_y < my {
                return self.scroll_y - self.scroll_y % self.row_h;
            }
        }
        self.scroll_y
    }

    fn max_scroll(&self) -> (i32, i32) {
        let (cw, ch) = self.content_size();
        (
            (cw - (self.bounds.w - self.gutter_w)).max(0),
            (ch - (self.bounds.h - self.footer_h)).max(0),
        )
    }

    fn clamp(&mut self) {
        let (mx, my) = self.max_scroll();
        self.scroll_x = self.scroll_x.clamp(0, mx);
        self.scroll_y = self.scroll_y.clamp(0, my);
        // 행 단위(`grid.scroll = row`)는 **표시 시점**에만 맞춘다([`Self::view_y`]) — 저장값에서 나머지를 버리면 트랙패드의
        // 느린 스크롤(사건당 1~3px)이 한 행을 영원히 못 넘는다(사용자 09-16).
        // ★ 스크롤이 끝에 닿았고 서버에 더 있으면 다음 세그먼트 1회(진행 중이면 무시 · docs/43 §3-5 자동 페치).
        if self.auto_fetch
            && self.more
            && !self.fetching
            && self.fetch_req.is_none()
            && self.page_rows > 0
            && my > 0
            && self.scroll_y >= my - self.row_h.max(1)
        {
            self.fetch_req = Some(FetchReq::Next {
                offset: self.rows(),
                limit: self.page_rows,
            });
        }
    }

    /// 페이드 타이머(스크롤바 · 호버 행) — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let a = self.bars.tick(now_ms);
        let b = self.hover.tick(now_ms);
        let c = self.poll_text();
        a || b || c
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.bars.is_visible()
    }

    /// 호버 페이드가 움직이는 중인가(호스트가 ≈30ms 프레임을 예약).
    pub(crate) fn hover_animating(&self) -> bool {
        self.hover.is_animating()
    }

    /// 마우스 아래 행(표시 index) — 본문 영역 안, 실제 행 범위 안일 때만.
    fn row_at_point(&self, x: i32, y: i32) -> Option<usize> {
        if self.row_h <= 0 || self.rs.is_none() {
            return None;
        }
        let b = self.bounds;
        let body = Rect::new(b.x, b.y + self.header_h, b.w, self.body_h());
        if !body.contains(Point { x, y }) {
            return None;
        }
        let di = ((y - body.y + self.view_y()) / self.row_h) as usize;
        (di < self.rows()).then_some(di)
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent, scale: f32) {
        // 열린 우클릭 메뉴가 먼저(바깥 클릭 = 닫고 통과).
        if self.menu.is_open() {
            let consumed = self.menu.on_event(ev);
            if let Some(id) = self.menu.take_picked() {
                // ★ 항목 선택 = 그 클릭은 메뉴가 먹었다 — 호스트가 아래(셀 선택)로 흘리지 않게 표시(사용자 09-15).
                self.menu_click_consumed = true;
                self.menu_pick(&id);
                return;
            }
            if consumed {
                // 메뉴 안(비활성 행·여백) 클릭도 전파하지 않는다 · 바깥 클릭은 닫고 통과.
                if self.menu.is_open() {
                    self.menu_click_consumed = true;
                }
                return;
            }
        }
        // 결과 도구줄(푸터)이 먼저 — 커서 아래 컨트롤에만(마우스 라우팅 규칙).
        if self.footer_event(ev, scale) {
            return;
        }
        if self.view != ResultView::Grid && self.rs.is_some() {
            self.text_view_event(ev, scale);
            return;
        }
        // ★ 스크롤바가 **선택보다 먼저**(휠 = 픽셀 · 썸/트랙 클릭 · 드래그 · 호버). 소비되면 셀 선택·키 처리로 흘리지 않는다
        //   (09-16: 가로 바 트랙을 눌렀는데 뒤의 셀이 선택됐다 — 선택 판정이 먼저 return했다).
        if self.row_h > 0 && self.rs.is_some() {
            let (cw, ch) = self.content_size();
            let b = self.bounds;
            // ★ 뷰포트 = 행번호 열(고정)·헤더(고정) 제외 — 바는 데이터 영역에만(09-16: 세로 바가 헤더까지 걸쳤다) ·
            //   가로 바의 끝이 키보드 `max_scroll`과 같은 자리.
            let b = Rect::new(
                b.x + self.gutter_w,
                b.y + self.header_h,
                b.w - self.gutter_w,
                b.h - self.header_h - self.footer_h,
            );
            let ch = ch - self.header_h;
            let (nx, ny, consumed) = self.bars.on_event(
                ev,
                b,
                cw.max(b.w),
                ch.max(b.h),
                self.scroll_x,
                self.scroll_y,
                scale,
            );
            self.scroll_x = nx;
            self.scroll_y = ny;
            self.clamp();
            if consumed {
                return;
            }
        }
        // ★ 선택(사용자 09-15 · dir2 탐색기 규약): 클릭 = 셀 · 드래그 = 사각 범위 · Shift+클릭 = 앵커~셀 ·
        //   Ctrl+클릭 = 구간 추가/제거 · 행번호 클릭 = 행 전체(Shift 연속 · Ctrl 개별 · 드래그 = 행 범위) ·
        //   좌상단 모서리 = 전체 · 방향키 = 이동 · Shift+방향키 = 범위 · Ctrl+방향키 = 포커스만 · Esc = 해제.
        if self.row_h > 0 && self.rs.is_some() {
            let hdr = self.header_rect();
            match *ev {
                // 좌상단 모서리(행번호 열 × 헤더) = 전체 선택.
                InputEvent::MouseDown { x, y, .. }
                    if hdr.contains(Point { x, y }) && self.in_gutter(x) =>
                {
                    self.select_all();
                    return;
                }
                // 행번호 열 = 행 전체.
                InputEvent::MouseDown {
                    x,
                    y,
                    shift,
                    primary,
                } if !hdr.contains(Point { x, y }) && self.in_gutter(x) => {
                    if let Some(row) = self.row_at_point(x, y) {
                        let last = self.last_col();
                        if shift {
                            let a = self.sel_anchor.map_or(row, |a| a.0);
                            let r = self.row_region(a, row);
                            match self.regions.last_mut() {
                                Some(l) => *l = r,
                                None => self.regions.push(r),
                            }
                            self.sel_cur = Some((row, 0));
                        } else if primary {
                            let r = self.row_region(row, row);
                            self.toggle_region(r);
                            self.sel_anchor = Some((row, 0));
                            self.sel_cur = Some((row, 0));
                        } else {
                            self.regions = vec![self.row_region(row, row)];
                            self.sel_anchor = Some((row, 0));
                            self.sel_cur = Some((row, last));
                        }
                        self.drag_sel = Some(DragSel::Rows);
                    }
                    return;
                }
                InputEvent::MouseDown {
                    x,
                    y,
                    shift,
                    primary,
                } if !hdr.contains(Point { x, y }) => {
                    if let Some(cell) = self.cell_at_point(x, y) {
                        if shift {
                            if self.sel_anchor.is_none() {
                                self.sel_anchor = Some(cell);
                            }
                            self.extend_to(cell);
                        } else if primary {
                            self.toggle_region(Self::rect_of(cell, cell));
                            self.sel_anchor = Some(cell);
                            self.sel_cur = Some(cell);
                        } else {
                            self.select_only(cell);
                        }
                        self.drag_sel = Some(DragSel::Cells);
                    }
                }
                InputEvent::MouseMove { x, y } if self.drag_sel.is_some() => {
                    let kind = self.drag_sel.unwrap_or(DragSel::Cells);
                    // 영역 밖으로 끌어도 가장 가까운 셀로(자동 확장).
                    let b = self.bounds;
                    let cx = x.clamp(b.x + self.gutter_w, b.right() - 1);
                    let cy = y.clamp(b.y + self.header_h, b.y + self.header_h + self.body_h() - 1);
                    if let Some(cell) = self.cell_at_point(cx, cy) {
                        match kind {
                            DragSel::Cells => self.extend_to(cell),
                            DragSel::Rows => {
                                let a = self.sel_anchor.map_or(cell.0, |a| a.0);
                                let r = self.row_region(a, cell.0);
                                match self.regions.last_mut() {
                                    Some(l) => *l = r,
                                    None => self.regions.push(r),
                                }
                                self.sel_cur = Some((cell.0, self.sel_cur.map_or(0, |c| c.1)));
                            }
                        }
                        self.ensure_cell_visible(cell.0, cell.1);
                    }
                }
                InputEvent::MouseUp { .. } if self.drag_sel.is_some() => {
                    self.drag_sel = None;
                }
                InputEvent::RightDown { x, y } => {
                    if let Some(cell) = self.cell_at_point(x, y) {
                        if !self.in_sel(cell.0, cell.1) {
                            self.select_only(cell);
                        }
                        self.open_menu(x, y, scale);
                        return;
                    }
                }
                InputEvent::Key {
                    key: Key::Escape, ..
                } => {
                    self.regions.clear();
                    self.sel_anchor = None;
                    self.sel_cur = None;
                    return;
                }
                InputEvent::Key {
                    key,
                    shift,
                    primary,
                } if self.sel_cur.is_some()
                    && matches!(key, Key::Up | Key::Down | Key::Left | Key::Right) =>
                {
                    let (dr, dc) = match key {
                        Key::Up => (-1, 0),
                        Key::Down => (1, 0),
                        Key::Left => (0, -1),
                        _ => (0, 1),
                    };
                    self.move_sel(dr, dc, shift, primary);
                    return;
                }
                // Home/End = 행의 처음/끝 컬럼(Ctrl = 첫/마지막 행) · PageUp/Down = 한 화면.
                InputEvent::Key {
                    key: Key::Home,
                    shift,
                    primary,
                } if self.sel_cur.is_some() => {
                    let (r, _) = self.sel_cur.unwrap_or((0, 0));
                    let nr = if primary { 0 } else { r };
                    self.go_to(nr, 0, shift, false);
                    return;
                }
                InputEvent::Key {
                    key: Key::End,
                    shift,
                    primary,
                } if self.sel_cur.is_some() => {
                    let (r, _) = self.sel_cur.unwrap_or((0, 0));
                    let nr = if primary {
                        self.rows().saturating_sub(1)
                    } else {
                        r
                    };
                    let nc = self.last_col();
                    self.go_to(nr, nc, shift, false);
                    return;
                }
                InputEvent::Key {
                    key: Key::PageDown,
                    shift,
                    primary,
                } if self.sel_cur.is_some() => {
                    let page = (self.body_h() / self.row_h.max(1)).max(1);
                    self.move_sel(page, 0, shift, primary);
                    return;
                }
                InputEvent::Key {
                    key: Key::PageUp,
                    shift,
                    primary,
                } if self.sel_cur.is_some() => {
                    let page = (self.body_h() / self.row_h.max(1)).max(1);
                    self.move_sel(-page, 0, shift, primary);
                    return;
                }
                // Space: Shift = 포커스 행 선택 · Ctrl = 포커스 행 토글(dir2 탐색기의 Ctrl+Space).
                InputEvent::Key {
                    key: Key::Space,
                    shift,
                    primary,
                } if self.sel_cur.is_some() && (shift || primary) => {
                    let (r, _) = self.sel_cur.unwrap_or((0, 0));
                    let reg = self.row_region(r, r);
                    if primary {
                        self.toggle_region(reg);
                    } else {
                        self.regions = vec![reg];
                    }
                    self.sel_anchor = Some((r, 0));
                    return;
                }
                _ => {}
            }
        }
        // 헤더: 클릭 = 정렬(Shift = 결합) · 드래그 = 컬럼 이동.
        if self.row_h > 0 && self.rs.is_some() && !self.col_order.is_empty() {
            let hdr = self.header_rect();
            match *ev {
                InputEvent::MouseDown { x, y, shift, .. } if hdr.contains(Point { x, y }) => {
                    if let Some(ci) = self.header_edge_at(x) {
                        // 같은 경계를 400ms 안에 다시 누르면 자동 맞춤(내용 폭 · 한계 안에서).
                        let now = std::time::Instant::now();
                        let dbl = self.edge_click.is_some_and(|(c, t)| {
                            c == ci && now.duration_since(t).as_millis() < 400
                        });
                        if dbl {
                            self.autofit = Some(ci);
                            self.edge_click = None;
                            self.hdr_resize = None;
                            return;
                        }
                        self.edge_click = Some((ci, now));
                        let w0 = self.col_w.get(ci).copied().unwrap_or(80);
                        self.hdr_resize = Some((ci, x, w0));
                    } else if let Some(pos) = self.header_pos_at(x) {
                        self.hdr_drag = Some((pos, x, x, false, shift));
                    }
                    return;
                }
                InputEvent::MouseMove { x, .. } if self.hdr_resize.is_some() => {
                    if let Some((ci, x0, w0)) = self.hdr_resize {
                        // 끌었으면 더블클릭 후보에서 뺀다(끌기 → 바로 재클릭은 새 조절).
                        if (x - x0).abs() > 3 {
                            self.edge_click = None;
                        }
                        if let Some(w) = self.col_w.get_mut(ci) {
                            *w = (w0 + (x - x0)).max(24);
                        }
                        // ★ 오른쪽 경계가 뷰포트 밖으로 나가면 그만큼 가로 스크롤 — 마지막 컬럼을 창 밖으로 키워도 경계가 보인다
                        //   (사용자 09-16 · 줄일 때와 같은 방식).
                        if let Some(pos) = self.col_order.iter().position(|&c| c == ci) {
                            let left: i32 = self.col_order[..pos]
                                .iter()
                                .map(|&c| self.col_w.get(c).copied().unwrap_or(80))
                                .sum();
                            let edge = left + self.col_w.get(ci).copied().unwrap_or(80);
                            let vp_w = (self.bounds.w - self.gutter_w).max(1);
                            if edge - self.scroll_x > vp_w {
                                self.scroll_x = edge - vp_w;
                            }
                        }
                        self.clamp();
                    }
                    return;
                }
                InputEvent::MouseUp { .. } if self.hdr_resize.is_some() => {
                    self.hdr_resize = None;
                    return;
                }
                InputEvent::MouseMove { x, .. } if self.hdr_drag.is_some() => {
                    if let Some(d) = self.hdr_drag.as_mut() {
                        d.2 = x;
                        if (x - d.1).abs() > 4 {
                            d.3 = true;
                        }
                    }
                    return;
                }
                InputEvent::MouseUp { x, .. } if self.hdr_drag.is_some() => {
                    let (pos, _, _, moved, shift) =
                        self.hdr_drag.take().unwrap_or((0, 0, 0, false, false));
                    if moved {
                        let to = self.drop_pos_at(x);
                        if pos < self.col_order.len() && to != pos {
                            let c = self.col_order.remove(pos);
                            self.col_order.insert(to.min(self.col_order.len()), c);
                        }
                    } else if let Some(&col) = self.col_order.get(pos) {
                        self.toggle_sort(col, shift);
                    }
                    return;
                }
                _ => {}
            }
        }
        // 호버 행 — 목표만 바꾼다(진행도는 `tick`이 흘린다 · 드래그/폭 조절 중엔 위에서 이미 돌아갔다).
        if let InputEvent::MouseMove { x, y } = *ev {
            self.hover.set(self.row_at_point(x, y));
        }
        let page = self.body_h().max(self.row_h);
        match ev {
            InputEvent::Key {
                key: Key::PageDown, ..
            } => self.scroll_y += page,
            InputEvent::Key {
                key: Key::PageUp, ..
            } => self.scroll_y -= page,
            InputEvent::Key { key: Key::Home, .. } => self.scroll_y = 0,
            InputEvent::Key { key: Key::End, .. } => self.scroll_y = i32::MAX / 2,
            InputEvent::Key { key: Key::Down, .. } => self.scroll_y += self.row_h,
            InputEvent::Key { key: Key::Up, .. } => self.scroll_y -= self.row_h,
            InputEvent::Key { key: Key::Left, .. } => self.scroll_x -= self.row_h * 2,
            InputEvent::Key {
                key: Key::Right, ..
            } => self.scroll_x += self.row_h * 2,
            _ => {}
        }
        self.clamp();
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        let t_render = std::time::Instant::now();
        self.paint_inner(dc, th, s);
        self.render = t_render.elapsed();
    }

    fn paint_inner(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        let b = self.bounds;
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.x, b.y, b.w, 1), th.border);
        dc.select_font(FontSlot::Base, false);
        let pad = (6.0 * s).round() as i32;
        // 행 높이 = 글꼴 높이 × 비율(설정 `grid.row_height_pct` · 기본 150% · 사용자 09-16 "폰트 크기에 적당한 비율") — 글자는
        // 잉크 기준 세로 중앙(`text_center_y`) · 헤더는 행보다 위아래 1px씩 2px 더.
        let th_px = dc.text_height();
        self.row_h = ((th_px as f32 * self.row_pct as f32 / 100.0).round() as i32).max(th_px + 2);
        // 푸터(결과 도구줄) 높이 = 아이콘 16 + 여백 — 보기 모드·결과 유무와 무관하게 늘 보인다(사용자 09-16).
        self.tb_view.set_scale(s);
        self.footer_h =
            ((self.tb_view.preferred_height() as f32 * s).round() as i32).max(self.row_h);
        let footer = self.footer_rect();
        let above = Rect::new(b.x, b.y, b.w, (b.h - self.footer_h).max(0));
        // 메시지 모드(오류 · PRINT) — 결과가 없고 메시지가 있을 때.
        if self.rs.is_none() && !self.messages.is_empty() {
            let mut y = b.y + pad;
            for m in self
                .messages
                .iter()
                .rev()
                .take(200)
                .collect::<Vec<_>>()
                .iter()
                .rev()
            {
                if y > above.bottom() {
                    break;
                }
                dc.text(b.x + pad, y, above, m, th.text);
                y += self.row_h;
            }
            self.paint_footer(dc, th, s, footer, 0, 0, 0);
            return;
        }
        // No Records(결과 없음 · 0행) — 헤더 한 줄 + 1행(DBeaver 모양 · 사용자 09-16).
        let empty = self.rs.as_ref().is_none_or(|r| r.rows.is_empty());
        if empty && self.view == ResultView::Grid {
            // Golden 방식(사용자 09-16 2번 이미지): 행번호 칸 + `No Records` 폭만큼의 작은 셀 하나 — 나머지는 빈 바탕.
            let label = t(Msg::GridNoRecords);
            let gw = dc.text_width("0") * 2 + pad * 2;
            let cw = dc.text_width(label) + pad * 2;
            let header = Rect::new(b.x, b.y + 1, gw + cw, self.row_h + 2);
            self.header_h = header.h + 1;
            dc.fill_rect(header, th.chrome_bg);
            dc.fill_rect(Rect::new(b.x, header.bottom() - 1, header.w, 1), th.border);
            dc.fill_rect(
                Rect::new(header.right() - 1, header.y, 1, header.h),
                th.border,
            );
            let ry = header.bottom();
            let row = Rect::new(b.x, ry, gw + cw, self.row_h);
            dc.fill_rect(Rect::new(b.x, ry, gw, self.row_h), th.chrome_bg);
            let cy = dc.text_center_y(ry, self.row_h);
            dc.text(b.x + pad, cy, row, "1", th.text_dim);
            dc.text(b.x + gw + pad, cy, row, label, th.text_dim);
            dc.fill_rect(Rect::new(b.x + gw, ry, 1, self.row_h), th.border);
            dc.fill_rect(Rect::new(row.right() - 1, ry, 1, self.row_h), th.border);
            dc.fill_rect(Rect::new(b.x, row.bottom() - 1, row.w, 1), th.border);
            self.paint_footer(dc, th, s, footer, 0, 0, 0);
            return;
        }
        // 텍스트 계열 보기(그리드 대신 본문을 줄 단위로 · 스크롤바 공용).
        if self.view != ResultView::Grid {
            self.paint_text_view(dc, th, s, pad);
            let n = self.rows();
            self.paint_footer(dc, th, s, footer, 0, 0, n);
            return;
        }
        let Some(rs) = self.rs.as_ref() else {
            self.paint_footer(dc, th, s, footer, 0, 0, 0);
            return;
        };
        if self.col_w.is_empty() {
            self.col_w = rs
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let mut w = dc.text_width(&c.name);
                    for row in rs.rows.iter().take(200) {
                        if let Some(v) = row.get(i) {
                            w = w.max(dc.text_width(&cell_text(v)));
                        }
                    }
                    (w + pad * 2).clamp(
                        (self.col_min as f32 * s).round() as i32,
                        (self.col_max as f32 * s).round() as i32,
                    )
                })
                .collect();
        }
        if let Some(ci) = self.autofit.take() {
            // 헤더 이름 + 전 행(최대 5,000행) 중 가장 넓은 값 → [최소, 최대].
            let mut w = rs.columns.get(ci).map_or(0, |c| dc.text_width(&c.name));
            for row in rs.rows.iter().take(5000) {
                if let Some(v) = row.get(ci) {
                    w = w.max(dc.text_width(&cell_text(v)));
                }
            }
            let w = (w + pad * 2).clamp(
                (self.col_min as f32 * s).round() as i32,
                (self.col_max as f32 * s).round() as i32,
            );
            if let Some(cw) = self.col_w.get_mut(ci) {
                *cw = w;
            }
        }
        let header = Rect::new(b.x, b.y + 1, b.w, self.row_h + 2);
        self.header_h = header.h + 1;
        // 행번호 열 폭(자릿수 × 숫자 폭 + 여백) — 가로 스크롤과 무관한 고정 열.
        self.gutter_w = if self.row_numbers {
            let digits = rs.rows.len().max(1).to_string().len().max(2) as i32;
            digits * dc.text_width("0") + pad * 2
        } else {
            0
        };
        let gx0 = b.x + self.gutter_w;
        // 크기가 정해진 뒤 범위 재확인(창 리사이즈 · 첫 페인트).
        let (mx, my) = self.max_scroll();
        self.scroll_x = self.scroll_x.clamp(0, mx);
        self.scroll_y = self.scroll_y.clamp(0, my);

        // ── 행(픽셀 오프셋: 첫 행이 부분적으로 잘려 올라간다)
        // 푸터(위치·계측) 한 줄은 맨 아래 — 헤더와 겹치지 않는다(09-14 사용자 지적).
        let body = Rect::new(
            b.x,
            header.bottom(),
            b.w,
            (footer.y - header.bottom()).max(0),
        );
        let vy = self.view_y();
        let first = (vy / self.row_h.max(1)) as usize;
        let sub = vy % self.row_h.max(1);
        let mut y = body.y - sub;
        let mut last = first;
        let n = rs.rows.len();
        for di in first..n {
            if y >= body.bottom() {
                break;
            }
            let ri = self.row_order.get(di).copied().unwrap_or(di);
            let Some(row) = rs.rows.get(ri) else { break };
            last = di + 1;
            let rr = Rect::new(b.x, y, b.w, self.row_h).intersection(&body);
            if di % 2 == 1 {
                dc.fill_rect(rr, th.panel_bg_alt);
            }
            // 호버 강조 — 색을 새로 만들지 않고 전경색을 알파로 덮는다(진행도 × 토큰 알파 · 서서히).
            let ha = hover_alpha(false, self.hover.value(di));
            if ha > 0.0 {
                dc.fill_rect_alpha(rr, th.text, ha);
            }
            let cells = Rect::new(gx0, body.y, (b.right() - gx0).max(0), body.h);
            let mut x = gx0 - self.scroll_x;
            for (pos, &ci) in self.col_order.iter().enumerate() {
                let Some(v) = row.get(ci) else { continue };
                let cw = self.col_w.get(ci).copied().unwrap_or(80);
                let clip = Rect::new(x, y, cw - 1, self.row_h).intersection(&cells);
                if clip.w > 0 && self.in_sel(di, pos) {
                    dc.fill_rect_alpha(clip, th.sel_bg, 0.85);
                }
                if clip.w > 0 && self.sel_cur == Some((di, pos)) {
                    dc.stroke_round_rect(clip, 0, th.accent, 1.0);
                }
                if clip.w > 0 && clip.h > 0 {
                    let txt = cell_text(v);
                    let numeric = matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_));
                    let color = if matches!(v, Value::Null) {
                        th.text_dim
                    } else {
                        th.text
                    };
                    if numeric {
                        let tw = dc.text_width(&txt);
                        let ty = dc.text_center_y(y, self.row_h);
                        dc.text(x + cw - pad - tw, ty, clip, &txt, color);
                    } else {
                        let ty = dc.text_center_y(y, self.row_h);
                        dc.text(x + pad, ty, clip, &txt, color);
                    }
                }
                x += cw;
            }
            // 행번호(고정 열 · 우측 정렬 · 흐리게 · 선택 행은 선택색으로 표시).
            if self.gutter_w > 0 {
                let num = (di + 1).to_string();
                let nw = dc.text_width(&num);
                let gclip = Rect::new(b.x, y, self.gutter_w, self.row_h).intersection(&body);
                dc.fill_rect(gclip, th.chrome_bg);
                let selected_row = self.row_in_sel(di);
                if selected_row {
                    dc.fill_rect_alpha(gclip, th.sel_bg, 0.85);
                }
                dc.text(
                    gx0 - pad - nw,
                    y + pad / 2,
                    gclip,
                    &num,
                    if selected_row { th.text } else { th.text_dim },
                );
            }
            y += self.row_h;
        }
        if self.gutter_w > 0 {
            dc.fill_rect(Rect::new(gx0 - 1, body.y, 1, body.h), th.border);
        }
        // ── 헤더(행 위에 덮어 그린다 — 부분 스크롤된 첫 행이 헤더 아래로 들어간다)
        dc.fill_rect(header, th.chrome_bg);
        let hcells = Rect::new(gx0, header.y, (b.right() - gx0).max(0), header.h);
        let mut x = gx0 - self.scroll_x;
        let dragging = self.hdr_drag.filter(|d| d.3);
        let drop_pos = dragging.map(|d| self.drop_pos_at(d.2));
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let Some(c) = rs.columns.get(ci) else {
                continue;
            };
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            let clip = Rect::new(x, header.y, cw, header.h).intersection(&hcells);
            if dragging.is_some_and(|d| d.0 == pos) {
                dc.fill_rect(clip, th.sel_bg);
            } else if self.col_in_sel(pos) {
                // 선택에 걸린 컬럼 헤더는 옅게 표시(행번호 강조와 짝).
                dc.fill_rect_alpha(clip, th.sel_bg, 0.35);
            }
            // 정렬 배지: ▲/▼ + 결합 순번(키가 2개 이상일 때)
            let badge = self.sort_keys.iter().position(|(k, _)| *k == ci).map(|i| {
                let arrow = if self.sort_keys[i].1 { "▲" } else { "▼" };
                if self.sort_keys.len() > 1 {
                    format!("{arrow}{}", i + 1)
                } else {
                    arrow.to_string()
                }
            });
            let name_clip = if let Some(bd) = &badge {
                let bw = dc.text_width(bd);
                let hy = dc.text_center_y(header.y, header.h);
                dc.text(x + cw - pad - bw, hy, clip, bd, th.accent);
                Rect::new(x, header.y, (cw - bw - pad * 2).max(0), header.h).intersection(&hcells)
            } else {
                clip
            };
            let hy = dc.text_center_y(header.y, header.h);
            dc.text(x + pad, hy, name_clip, &c.name, th.text);
            // 헤더 세로 경계선 강조(사용자 09-16 · 원복 요청 가능 = `th.border` 1px로 되돌리면 된다).
            dc.fill_rect_alpha(
                Rect::new(x + cw - 1, header.y, 1, header.h),
                th.text_dim,
                0.55,
            );
            if let Some(dp) = drop_pos {
                if dp == pos {
                    dc.fill_rect(Rect::new(x, header.y, 2, header.h), th.accent);
                }
            }
            x += cw;
        }
        if self.gutter_w > 0 {
            dc.fill_rect(
                Rect::new(b.x, header.y, self.gutter_w, header.h),
                th.chrome_bg,
            );
            let hy = dc.text_center_y(header.y, header.h);
            dc.text(b.x + pad, hy, header, "#", th.text_dim);
        }
        dc.fill_rect(Rect::new(b.x, header.bottom() - 1, b.w, 1), th.border);
        // 오버레이 스크롤바(필요할 때만 · 스크롤/호버 시 · 반투명) — 헤더 아래부터 푸터 위까지(데이터 영역만).
        let (cw, ch) = self.content_size();
        let ch = ch - self.header_h;
        let vp = Rect::new(
            b.x + self.gutter_w,
            b.y + self.header_h,
            b.w - self.gutter_w,
            b.h - self.header_h - self.footer_h,
        );
        self.bars.paint(
            dc,
            th,
            vp,
            cw.max(vp.w),
            ch.max(vp.h),
            self.scroll_x,
            self.view_y(),
            s,
        );
        let n = rs.rows.len();
        self.paint_footer(dc, th, s, footer, first, last, n);
    }

    /// 결과 도구줄(사용자 09-16 · docs/43 §4-2) — [보기 ▦] | [↻] | [+ − ⧉ ✓ ✕] | [200] [⇊] [Σ] … 상태(위치 · 메모리 · 실행 시각).
    #[allow(clippy::too_many_arguments)]
    fn paint_footer(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        s: f32,
        footer: Rect,
        first: usize,
        last: usize,
        n: usize,
    ) {
        let pad = (6.0 * s).round() as i32;
        dc.fill_rect(footer, th.chrome_bg);
        dc.fill_rect(Rect::new(footer.x, footer.y, footer.w, 1), th.border);
        let mut inv = Invalidations::default();
        let bar_y = footer.y + 1;
        let bar_h = footer.h - 1;
        let gap = pad;
        let mut x = footer.x;
        // 묶음 사이 구분선.
        let sep = |dc: &mut dyn DrawCtx, x: i32| {
            dc.fill_rect(Rect::new(x, bar_y + pad / 2, 1, bar_h - pad), th.border);
        };
        for (i, tb) in [&mut self.tb_view, &mut self.tb_refresh, &mut self.tb_edit]
            .into_iter()
            .enumerate()
        {
            if i > 0 {
                sep(dc, x);
                x += gap;
            }
            tb.set_scale(s);
            tb.set_bounds(Rect::new(x, bar_y, footer.w, bar_h), &mut inv);
            let end = tb.left_items_end();
            tb.set_bounds(Rect::new(x, bar_y, end - x, bar_h), &mut inv);
            tb.paint(dc, th);
            x = end + gap;
        }
        // 그룹 4: 세그먼트 상자 + 전체 조회 + 건수.
        sep(dc, x);
        x += gap;
        self.page_box.set_scale(s);
        let pb_h = (self.row_h + 2).min(bar_h - 2);
        let pb_w = (64.0 * s).round() as i32;
        self.page_box.set_bounds(
            Rect::new(x, bar_y + (bar_h - pb_h) / 2, pb_w, pb_h),
            &mut inv,
        );
        dc.select_font(FontSlot::Status, false);
        self.page_box.paint(dc, th);
        x += pb_w + gap / 2;
        self.tb_fetch.set_scale(s);
        self.tb_fetch
            .set_bounds(Rect::new(x, bar_y, footer.w, bar_h), &mut inv);
        let end = self.tb_fetch.left_items_end();
        self.tb_fetch
            .set_bounds(Rect::new(x, bar_y, end - x, bar_h), &mut inv);
        self.tb_fetch.paint(dc, th);
        x = end + gap;
        // 그룹 5: 상태 — `192–200 / 200+ · 총 12,345 · ~291 KB · 2026-09-16 16:20:07.123`.
        let mut info = format!(
            "{}–{} / {}{}",
            if n == 0 { 0 } else { first + 1 },
            last,
            n,
            if self.more { "+" } else { "" }
        );
        if let Some(total) = self.total {
            info.push_str(&format!(
                " · {}",
                nsql_i18n::tf(Msg::StGridTotal, &[&total.to_string()])
            ));
        }
        if self.fetching {
            info.push_str(&format!(" · {}", t(Msg::StFetching)));
        }
        info.push_str(&format!(" · ~{}", fmt_bytes(self.approx_bytes)));
        if let Some(at) = &self.last_run_at {
            info.push_str(&format!(" · {at}"));
        }
        // 글자는 여기서 그리지 않는다 — 그리드 글꼴(Calibri) 패스가 아니라 상태줄과 같은 UI 글꼴 패스에서(`paint_footer_text`).
        self.footer_info = Some((
            info,
            Rect::new(x, footer.y, footer.right() - x - pad, footer.h),
        ));
    }

    /// 도구줄 상태 글자(`7–17 / 200+ · ~291 KB · 시각`) — 호스트의 UI 글꼴 패스에서 `FontSlot::Status`(상태줄과 같은
    /// 얼굴·크기)로 오른쪽 정렬.
    pub(crate) fn paint_footer_text(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        let Some((info, area)) = &self.footer_info else {
            return;
        };
        if area.w <= 0 || area.h <= 0 {
            return;
        }
        dc.select_font(FontSlot::Status, false);
        let iw = dc.text_width(info);
        let iy = dc.text_center_y(area.y, area.h);
        let ix = (area.right() - iw).max(area.x);
        dc.text(ix, iy, *area, info, th.text_dim);
        dc.select_font(FontSlot::Base, false);
    }

    /// 텍스트 계열 보기 — 줄 단위 · 고정폭 · 가로/세로 스크롤.
    fn paint_text_view(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32, pad: i32) {
        // 행번호 거터(그리드와 같은 설정 · 자릿수 × 숫자 폭 + 여백 · 가로 스크롤 무관).
        self.text_gutter_w = if self.row_numbers {
            let digits = self.text_lines.len().max(1).to_string().len().max(2) as i32;
            digits * dc.text_width("0") + pad * 2
        } else {
            0
        };
        let body = self.text_body_rect();
        if self.text_w == 0 {
            // 가장 긴 줄(문자 수) 하나만 잰다 — 전 줄 측정은 수천 줄에서 수백 ms였다(09-16).
            let w = self
                .text_longest
                .and_then(|i| self.text_lines.get(i))
                .map_or(0, |l| dc.text_width(l));
            self.text_w = w + pad * 2;
        }
        let (cw, ch) = self.text_content_size();
        let mx = (cw - body.w).max(0);
        let my = (ch - body.h).max(0);
        self.text_scroll = (
            self.text_scroll.0.clamp(0, mx),
            self.text_scroll.1.clamp(0, my),
        );
        let rh = self.row_h.max(1);
        let first = (self.text_scroll.1 / rh) as usize;
        let sub = self.text_scroll.1 % rh;
        let mut y = body.y - sub;
        let x = body.x + pad - self.text_scroll.0;
        let gw = self.text_gutter_w;
        let gutter = Rect::new(self.bounds.x, body.y, gw, body.h);
        if gw > 0 {
            dc.fill_rect(gutter, th.chrome_bg);
            dc.fill_rect(Rect::new(gutter.right() - 1, body.y, 1, body.h), th.border);
        }
        for (i, line) in self.text_lines.iter().enumerate().skip(first) {
            if y >= body.bottom() {
                break;
            }
            dc.text(x, y, body, line, th.text);
            if gw > 0 {
                let num = (i + 1).to_string();
                let nw = dc.text_width(&num);
                dc.text(gutter.right() - pad - nw, y, gutter, &num, th.text_dim);
            }
            y += rh;
        }
        self.bars.paint(
            dc,
            th,
            body,
            cw.max(body.w),
            ch.max(body.h),
            self.text_scroll.0,
            self.text_scroll.1,
            s,
        );
        // 진척 카드(우하단 · 반투명 · 변환 중에만).
        if let Some(job) = &self.text_job {
            let pct = (job.done * 100)
                .checked_div(job.total)
                .map_or(100, |p| p.min(100));
            let name = match self.view {
                ResultView::Markdown => "Markdown",
                ResultView::Json => "JSON",
                ResultView::Tsv => "TSV",
                ResultView::Csv => "CSV",
                ResultView::Sql(_) => "SQL",
                _ => "Text",
            };
            let msg = nsql_i18n::tf(Msg::StTextRender, &[name, &pct.to_string()]);
            dc.select_font(FontSlot::Status, false);
            let tw = dc.text_width(&msg);
            let th_px = dc.text_height();
            let cw2 = tw + pad * 3;
            let ch2 = th_px + pad * 2;
            let r = Rect::new(
                body.right() - cw2 - pad * 2,
                body.bottom() - ch2 - pad * 2,
                cw2,
                ch2,
            );
            dc.fill_round_rect_alpha(r, pad, th.text, 0.78);
            // 진행 막대(카드 아래 2px).
            let bar_w = (cw2 - pad * 2) * pct as i32 / 100;
            dc.fill_rect(Rect::new(r.x + pad, r.bottom() - 3, bar_w, 2), th.accent);
            let ty = dc.text_center_y(r.y, ch2) - 1;
            dc.text(r.x + pad + pad / 2, ty, r, &msg, th.panel_bg);
            dc.select_font(FontSlot::Base, false);
        }
    }
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Bytes(b) => format!("<{} bytes>", b.len()),
        other => other.display(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Column, ResultSet};

    fn grid_with(cols: &[i32]) -> Grid {
        let mut g = Grid::default();
        let rs = ResultSet {
            columns: cols
                .iter()
                .enumerate()
                .map(|(i, _)| Column {
                    name: format!("c{i}"),
                    type_name: String::new(),
                })
                .collect(),
            rows: vec![vec![Value::Int(1); cols.len()]],
        };
        g.set_result(rs);
        g.bounds = Rect::new(0, 0, 400, 300);
        g.row_h = 20;
        g.header_h = 21;
        g.gutter_w = 30;
        g.col_w = cols.to_vec();
        g
    }

    /// 느린 트랙패드 휠(사건당 -3 = 1px)이 누적된다 — 픽셀 모드는 1px씩, 행 모드는 저장값은 누적되되 표시는 행 경계(사용자 09-16).
    #[test]
    fn slow_wheel_accumulates_in_both_scroll_modes() {
        let mut g = grid_with(&[100, 80]);
        let rs = ResultSet {
            columns: vec![Column {
                name: "c0".into(),
                type_name: String::new(),
            }],
            rows: (0..100).map(|i| vec![Value::Int(i)]).collect(),
        };
        g.set_result(rs);
        g.row_h = 20;
        g.header_h = 21;
        g.gutter_w = 30;
        g.col_w = vec![100];
        for _ in 0..7 {
            g.on_event(&InputEvent::Wheel { delta: -3 }, 1.0);
        }
        assert_eq!(g.scroll_y, 7, "픽셀 모드: 1px씩");
        assert_eq!(g.view_y(), 7);
        g.set_row_snap(true);
        assert_eq!(g.view_y(), 0, "행 모드 표시 = 행 경계로 내림");
        for _ in 0..13 {
            g.on_event(&InputEvent::Wheel { delta: -3 }, 1.0);
        }
        assert_eq!(g.scroll_y, 20, "저장값은 계속 누적");
        assert_eq!(g.view_y(), 20, "한 행 높이를 채우면 표시가 넘어간다");
        for _ in 0..20 {
            g.on_event(&InputEvent::Wheel { delta: 3 }, 1.0);
        }
        assert_eq!(g.scroll_y, 0, "반대 방향도 같은 걸음");
    }

    /// Advanced Copy ▸ SQL 5종이 전부 문장을 만든다(사용자 09-16 "SQL 유형 모두 복사"). 키 = 첫 컬럼 · 테이블 = 원본 SQL 추정.
    #[test]
    fn sql_copy_all_kinds_produce_statements() {
        let mut g = grid_with(&[100, 80]);
        g.set_source_sql("SELECT * FROM T1 WHERE 1 = 1");
        g.select_all();
        let key = nsql_io::choose_key(nsql_io::KeyMode::Pk, None, &g.selected_col_names());
        assert_eq!(
            key.cols,
            vec!["c0", "c1"],
            "카탈로그 없음 = 앞 컬럼(최대 3)"
        );
        let key1 = KeySpec {
            cols: vec!["c0".into()],
            source: nsql_io::KeySource::Pk,
        };
        let out = |k: SqlKind| {
            g.copy_sql(k, &key1)
                .expect("복사 결과")
                .0
                .trim_end()
                .to_string()
        };
        assert_eq!(out(SqlKind::Select), "SELECT c0, c1 FROM T1 WHERE c0 = 1;");
        assert_eq!(
            out(SqlKind::Insert),
            "INSERT INTO T1 (c0, c1) VALUES (1, 1);"
        );
        assert_eq!(out(SqlKind::Update), "UPDATE T1 SET c1 = 1 WHERE c0 = 1;");
        assert_eq!(out(SqlKind::Delete), "DELETE FROM T1 WHERE c0 = 1;");
        let merge = out(SqlKind::Merge);
        assert!(merge.starts_with("MERGE INTO T1 t USING ("), "{merge}");
        assert!(
            merge.contains("WHEN MATCHED THEN UPDATE SET t.c1 = s.c1"),
            "{merge}"
        );
        assert!(
            merge.contains("WHEN NOT MATCHED THEN INSERT (c0, c1) VALUES (s.c0, s.c1)"),
            "{merge}"
        );
    }

    #[test]
    fn header_edge_drag_resizes_column() {
        let mut g = grid_with(&[100, 80]);
        // 첫 컬럼 오른쪽 경계 = 30 + 100 = 130
        assert!(g.header_edge_hover(131, 5));
        assert!(!g.header_edge_hover(80, 5));
        let down = InputEvent::MouseDown {
            x: 129,
            y: 5,
            shift: false,
            primary: false,
        };
        g.on_event(&down, 1.0);
        assert!(g.hdr_resize.is_some(), "경계에서 폭 조절 시작");
        g.on_event(&InputEvent::MouseMove { x: 169, y: 5 }, 1.0);
        assert_eq!(g.col_w[0], 140);
        g.on_event(&InputEvent::MouseUp { x: 169, y: 5 }, 1.0);
        assert!(g.hdr_resize.is_none());
        // 최소 폭 24 (경계는 이제 30 + 140 = 170)
        g.on_event(
            &InputEvent::MouseDown {
                x: 170,
                y: 5,
                shift: false,
                primary: false,
            },
            1.0,
        );
        assert!(g.hdr_resize.is_some());
        g.on_event(&InputEvent::MouseMove { x: -500, y: 5 }, 1.0);
        assert_eq!(g.col_w[0], 24);
    }

    #[test]
    fn header_click_sorts_and_shift_adds_key() {
        let mut g = grid_with(&[100, 80]);
        let click = |g: &mut Grid, x: i32, shift: bool| {
            g.on_event(
                &InputEvent::MouseDown {
                    x,
                    y: 5,
                    shift,
                    primary: false,
                },
                1.0,
            );
            g.on_event(&InputEvent::MouseUp { x, y: 5 }, 1.0);
        };
        click(&mut g, 80, false);
        assert_eq!(g.sort_keys, vec![(0, true)]);
        click(&mut g, 80, false);
        assert_eq!(g.sort_keys, vec![(0, false)]);
        click(&mut g, 170, true);
        assert_eq!(g.sort_keys, vec![(0, false), (1, true)]);
        click(&mut g, 80, false);
        assert_eq!(g.sort_keys, vec![(0, true)], "일반 클릭 = 단일 키로");
    }
}
