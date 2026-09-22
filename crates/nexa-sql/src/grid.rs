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
use nsql_core::{fmt_bytes, Dialect, ResultData, ResultSet, RowSource, Value, View};
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
    /// 시작 시각(개발자 모드 `load` 층 — 변환 소요).
    started: std::time::Instant,
}

impl Drop for TextJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// 페인트 뒤 1회 보고: (렌더 소요, 탑재 소요, 추정 바이트, 렌더 시작·종료 시각(개발자 모드)).
pub(crate) type PerfReport = (
    std::time::Duration,
    std::time::Duration,
    u64,
    Option<(String, String)>,
);

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
/// 텍스트 보기 드래그 종류(본문 = 문자 범위 · 거터 = 줄 범위).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextDrag {
    Body,
    Gutter,
}

/// 천 단위 구분(`155312` → `155,312`) — 푸터 진행 표시용.
fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 텍스트 보기 히트 요청 — 글꼴이 있는 페인트에서 (줄, 문자)로 푼다(로그 창과 같은 규약 · 09-16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextHit {
    Anchor { shift: bool },
    Head,
    GutterDown { shift: bool, ctrl: bool },
    GutterHead,
}

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
use nsql_io::{guess_table, Format, GridOpts, KeySpec};

/// 드래그 선택 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragSel {
    /// 셀 사각 범위.
    Cells,
    /// 행번호 열에서 시작 = 행 전체 범위.
    Rows,
}

/// 헤더 드래그 상태(컬럼 이동 · nexa-dir2 `ColDrag`와 같은 UX · 사용자 09-19):
/// 4px 이상 움직이면 활성 → **잡은 컬럼이 고스트로 커서를 따라가고**(가로만 · y = 헤더 행) · 가장 가까운 경계로
/// **즉시 재배열해 보여 준다**(라이브 미리보기) · MouseUp = 확정 · Esc = `orig`로 복원.
#[derive(Clone, Debug)]
struct HdrDrag {
    /// 지금 표시 위치(미리보기로 옮겨 다닌다).
    pos: usize,
    press_x: i32,
    cur_x: i32,
    active: bool,
    shift: bool,
    /// 잡은 점의 컬럼 왼쪽 기준 오프셋(고스트가 손 아래 그대로).
    grab_dx: i32,
    /// 시작 때 `col_order`(Esc 복원).
    orig: Vec<usize>,
}

pub(crate) struct Grid {
    pub bounds: Rect,
    /// ★ 결과 데이터 **한 세트**(DR-33) — 페치 세그먼트 `Arc`. 그리드·텍스트 보기 7종·복사·정렬은 전부 이것 하나에서
    ///   `View`(행·열 인덱스)로 파생한다(형식별 사본 0 · 변환 스레드 공유 복사 0).
    rs: Option<ResultData>,
    messages: Vec<String>,
    /// 탑재(set_result)·마지막 렌더 소요 — 푸터에 표시(docs/26 Load·Render).
    load: std::time::Duration,
    render: std::time::Duration,
    /// 텍스트 보기 파생 캐시(`text_lines`)의 바이트 — 예산(D-72)·푸터에 데이터와 함께 센다.
    text_bytes: u64,
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
    /// 설정 `grid.null_text` — NULL 셀 글자(그리드·텍스트 보기·복사 **공통** · 기본 `NULL` · 사용자 09-17).
    null_text: String,
    /// ★ 행 포커스 배경(사용자 09-22 · 설정 `grid.row_focus`): 셀을 골라도 그 행 전체(다중 행 포함)에 셀 선택색보다 연한 배경.
    row_focus: bool,
    /// 위쪽 경계선을 그릴지 — 결과 탭 줄이 바로 위에 있으면 탭 줄의 아래선과 겹쳐 2px가 되므로 호스트가 끈다(사용자 09-22).
    top_border: bool,
    /// 행 포커스 색·알파 덮어쓰기(설정 `grid.row_focus_color` `#RRGGBB[AA]` · 비면 `sel_bg` 35 %).
    row_focus_color: (Option<nexa_ctl::Color>, Option<f32>),
    /// 행 높이 비율(% · 글꼴 높이 대비 · 설정 `grid.row_height_pct`).
    row_pct: i32,
    gutter_w: i32,
    /// 표시 순서 → 원본 컬럼 index(드래그 이동 · 재조회 시 초기화 — 사용자 09-14).
    col_order: Vec<usize>,
    /// 표시 순서 → 원본 행 index(정렬은 인덱스 벡터 · 행 복제 0 — docs/26 §4-4).
    row_order: Vec<usize>,
    /// 결합 정렬 키(원본 컬럼 · 오름차순) — 클릭 = 단일 키 3단(▲→▼→해제) · Shift+클릭 = 키 추가/토글(dir2 방식).
    sort_keys: Vec<(usize, bool)>,
    /// 헤더 드래그 — 컬럼 이동(고스트 · 라이브 미리보기 · Esc 취소 · 09-19).
    hdr_drag: Option<HdrDrag>,
    /// 헤더 경계 드래그 = 컬럼 폭 조절(원본 컬럼 · 시작 x · 시작 폭 — 사용자 09-14).
    hdr_resize: Option<(usize, i32, i32)>,
    /// 헤더 경계 직전 클릭(컬럼 · 시각) — 400ms 안에 같은 경계면 더블클릭 = 자동 맞춤(사용자 09-16).
    edge_click: Option<(usize, std::time::Instant)>,
    /// 다음 페인트에서 자동 맞춤할 컬럼(글꼴 측정은 페인트에서).
    autofit: Option<usize>,
    /// 자동 너비 한계(논리 px · 설정 `grid.col_min_width`/`grid.col_max_width`).
    /// 자동 컬럼 너비 하한(논리 px · `grid.col_min_width`).
    col_min: i32,
    /// 자동 컬럼 너비 상한 = **글자 수**(`grid.col_max_mode`/`col_max_chars` · 사용자 09-17 "폰트 기준 24자") — 페인트가 그리드 글꼴의
    ///   숫자 폭을 단위로 px로 바꾼다(한글 등 전각은 글꼴에서 약 2배라 2자로 셈 · CLI `disp_width`와 같은 뜻).
    col_max_chars: i32,
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
    /// ★ 건수(Σ)를 셀 수 있는 결과인가(docs/43 §4-4 · 사용자 09-19) — 이 그리드의 결과가 **조회 문장 하나**에서 왔고
    /// 그 문장이 `source_sql`에 들어 있을 때만. 실행 시작 · DDL/DML만 실행 · PRINT/REF CURSOR 결과 · 빈 탭 = 거짓.
    countable: bool,
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
    /// ★ 이 결과가 속한 **세션이 다른 작업 중**(docs/52 §3) — 호스트가 알린다. 켜져 있는 동안 추가 페치·전체 조회·건수·새로고침을
    ///   요청하지 않는다(자동 페치 포함 · 도구줄 흐림). 진행 중인 전체 조회의 ■는 그대로(막힌 상태를 푸는 버튼).
    session_blocked: bool,
    /// 세션이 연결돼 있는가(호스트 · 유휴 닫힘은 조용히 재접속하므로 연결로 본다) — 아니면 새로고침·전체 조회·건수 전부 비활성(사용자 09-19).
    session_connected: bool,
    /// 전체 조회가 나가 있다 — 그 사이 도착하는 늦은 세그먼트는 버린다(사용자 09-17: 자동 페치 중 누른 전체 조회가 무시되던 결함).
    fetch_all_pending: bool,
    fetch_req: Option<FetchReq>,
    /// 전체 조회 진행(행, 바이트 · T-48b) — 푸터 "가져오는 중… n행 · MB".
    fetch_progress: Option<(u64, u64)>,
    /// 사용자가 가져오기 중지를 눌렀다(호스트가 워커 깃발로).
    cancel_req: bool,
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
    /// 텍스트 변환 완료 보고(줄 수 · 소요 · 1회성 · 호스트가 상세 로그로).
    text_report: Option<(usize, std::time::Duration)>,
    /// 변환 중 콘텐츠 높이 하한(변환 전 높이) — 줄이 다시 채워지는 동안 스크롤이 0으로 잘리지 않게(사용자 09-17 CSV 1행 점프).
    text_ch_hold: i32,
    /// 텍스트 보기 행번호 거터 폭(페인트가 잰다 · 설정 `grid.row_numbers` · 사용자 09-16 "다른 보기에서도 행번호").
    text_gutter_w: i32,
    /// 다음 페인트 뒤 렌더·메모리 보고 1회(호스트가 로그로).
    perf_report: bool,
    /// 렌더 시작·종료 시각(개발자 모드 Render 층이 켜져 있을 때만 · 보고 1회).
    render_at: Option<(String, String)>,
    /// 텍스트 보기 선택(사용자 09-16 "JSON·Markdown도 드래그 복사"): 앵커·헤드 = (줄, 문자 인덱스).
    text_sel: Option<((usize, usize), (usize, usize))>,
    /// 거터 Ctrl+클릭으로 모은 개별 줄(그리드 행번호 열과 같은 규약).
    text_lines_sel: Vec<usize>,
    text_drag: Option<TextDrag>,
    /// 페인트가 풀 히트 요청(x, y, 종류) — **큐**: macOS는 클릭 직후 CursorMoved를 보내므로 슬롯 하나면 앵커 요청이
    /// 헤드로 덮여 옛 앵커가 남는다(사용자 09-17 "Shift 누른 것처럼 확장"). 순서대로 전부 푼다.
    text_hit: Vec<(i32, i32, TextHit)>,
    /// 거터 드래그의 기준 줄.
    text_gutter_anchor: Option<usize>,
    /// 도구줄 상태 글자와 그 영역 — UI 글꼴 패스(상태줄과 같은 얼굴·크기)에서 호스트가 그린다(사용자 09-16).
    footer_info: Option<(String, Rect)>,
}

impl Default for Grid {
    fn default() -> Self {
        let mut g = Grid {
            bounds: Rect::new(0, 0, 0, 0),
            rs: None,
            messages: Vec::new(),
            load: std::time::Duration::ZERO,
            render: std::time::Duration::ZERO,
            text_bytes: 0,
            scroll_y: 0,
            scroll_x: 0,
            col_w: Vec::new(),
            row_h: 0,
            header_h: 0,
            bars: ScrollBars::new(),
            row_snap: false,
            row_numbers: true,
            row_focus: true,
            top_border: true,
            row_focus_color: (None, None),
            null_text: "NULL".into(),
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
            col_max_chars: 24,
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
            countable: false,
            tb_view: Self::bar(vec![ToolItem::new("view", toolicons::view_mode())
                .with_dropdown()
                .tip(t(Msg::TipViewMode))]),
            tb_refresh: Self::bar(vec![ToolItem::new("refresh", toolicons::refresh())
                .tip(t(Msg::TipRefresh))
                .disabled()]),
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
                ToolItem::new("fetch.all", toolicons::fetch_all())
                    .tip(t(Msg::TipFetchAll))
                    .disabled(),
                ToolItem::new("fetch.stop", toolicons::fetch_stop())
                    .tip(t(Msg::TipFetchCancel))
                    .disabled(),
                ToolItem::new("count", ToolIcon::Glyph("Σ".into()))
                    .tip(t(Msg::TipCount))
                    .disabled(),
            ]),
            page_box: Self::page_box(200),
            page_rows: 200,
            default_page_rows: 200,
            auto_fetch: true,
            footer_h: 0,
            more: false,
            total: None,
            fetching: false,
            session_blocked: false,
            session_connected: true,
            fetch_all_pending: false,
            fetch_req: None,
            fetch_progress: None,
            cancel_req: false,
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
            text_report: None,
            text_ch_hold: 0,
            text_gutter_w: 0,
            perf_report: false,
            render_at: None,
            text_sel: None,
            text_lines_sel: Vec::new(),
            text_drag: None,
            text_hit: Vec::new(),
            text_gutter_anchor: None,
            footer_info: None,
        };
        // 도구줄의 처음 상태도 판정 함수로(`.disabled()` 표기에 기대지 않는다) — Σ가 `.disabled()` 없이 만들어져
        // 결과가 오기 전까지 켜진 것처럼 보였다(동기화는 상태가 바뀔 때만 돈다 · mac 09-21).
        g.sync_fetch_tools();
        g
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
            row_focus: self.row_focus,
            top_border: self.top_border,
            row_focus_color: self.row_focus_color,
            null_text: self.null_text.clone(),
            row_pct: self.row_pct,
            sc_copy: self.sc_copy.clone(),
            sc_all: self.sc_all.clone(),
            dialect: self.dialect,
            col_min: self.col_min,
            col_max_chars: self.col_max_chars,
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
    /// 이 탭이 드는 대략 바이트 = 결과 데이터(세그먼트 누적) + 텍스트 보기 파생 캐시(메모리 예산 D-72 · 푸터).
    pub(crate) fn approx_bytes(&self) -> u64 {
        self.rs.as_ref().map_or(0, ResultData::approx_bytes) + self.text_bytes
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
        self.sync_fetch_tools();
    }

    /// COUNT(*) 결과.
    pub(crate) fn set_total(&mut self, n: u64) {
        self.total = Some(n);
        self.fetching = false;
        self.sync_fetch_tools();
    }

    /// 추가 페치 실패 — 요청 상태만 푼다.
    pub(crate) fn fetch_failed(&mut self) {
        self.fetching = false;
        self.end_fetch_all();
        self.sync_fetch_tools();
    }

    /// 전체 조회가 끝났다(교체 결과 도착 · 실패 · 중지) — 진행·취소 상태를 비우고 도구줄을 맞춘다.
    ///   ★ 완료 뒤에도 ■가 켜져 있던 결함(사용자 09-17): `set_result`가 깃발만 내리고 도구줄을 안 맞췄다.
    fn end_fetch_all(&mut self) {
        self.fetch_all_pending = false;
        self.fetch_progress = None;
        self.cancel_req = false;
        self.sync_fetch_tools();
    }

    /// 전체 조회 진행(워커 배치마다) — `rows` = 전체 행 수(기존 + 받은) · `bytes` = 이번에 받은 바이트(기존 데이터는 여기서 더한다).
    pub(crate) fn set_fetch_progress(&mut self, rows: u64, bytes: u64) {
        if self.fetch_all_pending {
            let held = self.rs.as_ref().map_or(0, ResultData::approx_bytes);
            self.fetch_progress = Some((rows, bytes + held));
        }
    }

    /// 중지 요청(도구줄 ■ · Esc) — 1회성.
    pub(crate) fn take_cancel_request(&mut self) -> bool {
        std::mem::take(&mut self.cancel_req)
    }

    /// 전체 조회 요청(도구줄 ⇊ · 09-17 "나머지 이어 받기"): 더 있을 때만 · 자동 페치가 나가 있으면 **큐에 두었다가** 그 세그먼트가
    /// 붙은 뒤 정확한 offset으로 보낸다([`Self::take_fetch_request`]) — 종전엔 교체라 늦은 세그먼트를 버렸다.
    fn request_fetch_all(&mut self) {
        if self.session_blocked || self.fetch_all_pending || self.rs.is_none() || !self.more {
            return;
        }
        self.fetch_req = Some(FetchReq::All);
        self.fetch_all_pending = true;
        self.fetch_progress = None;
        self.cancel_req = false;
        self.sync_fetch_tools();
    }

    /// 중지(■ · Esc): 아직 큐에만 있으면 요청을 지우고 끝 · 나가 있으면 워커 깃발(호스트가 가져간다).
    fn request_cancel(&mut self) {
        if !self.fetch_all_pending {
            return;
        }
        if matches!(self.fetch_req, Some(FetchReq::All)) {
            self.fetch_req = None;
            self.end_fetch_all();
        } else {
            self.cancel_req = true;
        }
    }

    /// 전체 조회 결과(나머지) 이어 붙이기 — 진행·■ 상태를 닫고 [`Self::append_page`].
    pub(crate) fn append_all(&mut self, page: ResultSet, more: bool) {
        self.end_fetch_all();
        self.append_page(page, more);
    }

    /// 페치 도구줄 활성 상태 = 그리드 상태의 함수 — ■ 중지는 전체 조회가 나가 있는 동안만 · 전체 조회는 결과가 있고
    ///   나가 있지 않을 때만(클릭 guard와 같은 조건 · 눌러도 아무 일 없는 버튼을 켜 두지 않는다).
    fn sync_fetch_tools(&mut self) {
        let mut inv = Invalidations::default();
        let open = self.session_open();
        self.tb_fetch
            .set_item_enabled("fetch.stop", self.fetch_all_pending, &mut inv);
        // 전체 조회 = 나머지 이어 받기라 **더 있을 때만**(전부 받았으면 흐림 · 09-17) · 세션이 한가할 때만(docs/52 §3).
        self.tb_fetch.set_item_enabled(
            "fetch.all",
            open && !self.fetch_all_pending && self.rs.is_some() && self.more,
            &mut inv,
        );
        // Σ 건수 = 결과가 있고 · 조회 문장에서 왔고 · 세션이 연결·한가하고 · 다른 페치/건수가 나가 있지 않고 ·
        //   **서버에 더 있을 때만**(전부 받았으면 건수 = 행 수라 불필요 · 사용자 09-19)(09-19 규정 · `can_count`와 같은 식).
        let count = self.can_count();
        self.tb_fetch.set_item_enabled("count", count, &mut inv);
        // 새로고침 = 다시 실행할 문장이 있는 결과가 있을 때만(눌러도 아무 일 없는 버튼을 켜 두지 않는다).
        self.tb_refresh
            .set_item_enabled("refresh", self.can_refresh(), &mut inv);
    }

    /// 세션이 연결돼 있고 한가한가 — 서버에 무언가를 보내는 버튼(새로고침·전체 조회·건수)의 공통 전제.
    fn session_open(&self) -> bool {
        !self.session_blocked && self.session_connected
    }

    /// 건수를 셀 수 있는 결과인가(도구줄·호스트 공용 판정).
    pub(crate) fn can_count(&self) -> bool {
        self.session_open() && self.rs.is_some() && self.countable && !self.fetching && self.more
    }

    /// 새로고침할 수 있는가 — 결과와 그 출처 문장이 있고 세션이 열려 있을 때.
    fn can_refresh(&self) -> bool {
        self.session_open() && self.rs.is_some() && !self.source_sql.trim().is_empty()
    }

    /// 결과가 도착했을 때 호스트가 알려 준다: 이 결과를 만든 문장(`source_sql`이 된다)과 그것이 조회 문장인가.
    pub(crate) fn set_result_origin(&mut self, stmt: &str, is_query: bool) {
        self.set_source_sql(stmt);
        self.countable = is_query;
        self.sync_fetch_tools();
    }

    /// 세션 통제 상태(호스트 · docs/52 §3) — 바뀔 때만 도구줄을 다시 맞춘다.
    pub(crate) fn set_session_blocked(&mut self, blocked: bool) {
        if self.session_blocked != blocked {
            self.session_blocked = blocked;
            self.sync_fetch_tools();
        }
    }

    /// 세션 연결 여부(호스트 · 바뀔 때만) — 연결될 때까지 서버로 나가는 버튼은 전부 비활성(사용자 09-19).
    pub(crate) fn set_session_connected(&mut self, connected: bool) {
        if self.session_connected != connected {
            self.session_connected = connected;
            self.sync_fetch_tools();
        }
    }

    /// 전체 조회 도구줄 상태(테스트·검증용): (전체 조회 활성, 중지 활성).
    #[cfg(test)]
    fn fetch_tools_enabled(&self) -> (bool, bool) {
        (
            self.tb_fetch.item_enabled("fetch.all"),
            self.tb_fetch.item_enabled("fetch.stop"),
        )
    }

    /// 다음 세그먼트 이어 붙이기(정렬 중이면 다시 정렬 · 스크롤 유지). 전체 조회가 큐에 있어도 버리지 않는다(그 뒤 offset으로 나간다).
    pub(crate) fn append_page(&mut self, page: ResultSet, more: bool) {
        self.fetching = false;
        self.more = more;
        self.sync_fetch_tools();
        let Some(rs) = self.rs.as_mut() else {
            return;
        };
        let start = rs.len();
        // 세그먼트 덧붙이기 = 이동(복사 0 · DR-33) — 변환 스레드가 옛 세그먼트를 공유 중이어도 서로 간섭 없음.
        rs.push(page);
        let end = rs.len();
        self.row_order.extend(start..end);
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

    /// ★ 엄격 일관성 교체(docs/43 §9): 처음부터 다시 받은 결과로 **행 전체를 바꾸되** 스크롤·정렬·컬럼 폭·순서·보기 모드는 그대로.
    pub(crate) fn replace_rows(&mut self, rs: ResultSet, more: bool) {
        self.end_fetch_all();
        self.fetching = false;
        let same_cols = self
            .rs
            .as_ref()
            .is_some_and(|r| r.columns().len() == rs.columns.len());
        let n = rs.rows.len();
        self.rs = Some(ResultData::new(rs));
        self.row_order = (0..n).collect();
        if !same_cols {
            self.col_order = (0..self.rs.as_ref().map_or(0, |r| r.columns().len())).collect();
            self.col_w.clear();
            self.sort_keys.clear();
        }
        if !self.sort_keys.is_empty() {
            self.apply_sort();
        }
        self.regions.clear();
        self.sel_anchor = None;
        self.sel_cur = None;
        self.drag_sel = None;
        self.more = more;
        self.total = None;
        self.text_lines = Vec::new();
        self.text_bytes = 0;
        self.perf_report = true;
        self.sync_fetch_tools();
        if self.view != ResultView::Grid {
            self.text_keep_scroll = true;
            self.refresh_text_view();
        }
    }

    /// 호스트가 가져가는 페치 요청(1회성) — 가져가는 순간 진행 중으로 표시. 전체 조회는 다른 페치가 나가 있는 동안 기다린다
    /// (그 세그먼트가 붙은 뒤 `rows()`가 정확한 offset).
    pub(crate) fn take_fetch_request(&mut self) -> Option<FetchReq> {
        if matches!(self.fetch_req, Some(FetchReq::All)) && self.fetching {
            return None;
        }
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
    pub(crate) fn take_perf_report(&mut self) -> Option<PerfReport> {
        let at = self.render_at.take();
        std::mem::take(&mut self.perf_report).then_some((
            self.render,
            self.load,
            self.approx_bytes(),
            at,
        ))
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
        self.text_sel = None;
        self.text_lines_sel.clear();
        self.text_drag = None;
        self.text_hit.clear();
        match self.view {
            ResultView::Grid => {
                // 그리드로 돌아오면 파생 캐시는 버린다(원본 한 세트만 남김 · 메모리 회수).
                self.cancel_text_job();
                self.text_lines = Vec::new();
                self.text_bytes = 0;
                self.text_longest = None;
            }
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
        // 스크롤 유지(추가 페치 뒤): 줄을 비우는 동안 콘텐츠 높이 하한을 종전 값으로 잡아 클램프가 0으로 끌어내리지 않게 —
        // 변환이 끝나면 새 높이(더 긴 쪽)로 자연히 이어진다(그리드와 같은 동작 · 사용자 09-17).
        let keep = std::mem::take(&mut self.text_keep_scroll);
        self.text_ch_hold = if keep {
            self.row_h * self.text_lines.len() as i32
        } else {
            0
        };
        self.text_lines = Vec::new();
        self.text_bytes = 0;
        // 가로 최대 폭은 커지는 쪽으로만(짧아져도 유지 · 사용자 09-17) — 새 변환에서도 이전 값을 하한으로.
        if !keep {
            self.text_w = 0;
        }
        self.text_longest = None;
        if !keep {
            self.text_scroll = (0, 0);
        }
        let Some(rs) = self.rs.as_ref() else {
            return;
        };
        // ★ 복사 0(DR-33): 데이터는 Arc 세그먼트 공유 · 정렬/열 순서는 인덱스 벡터만 넘긴다 → 스레드가 뷰를 만들어 읽는다.
        //   (종전 `ordered_rs`는 전 행·문자열을 딥 카피해 380MB 표면 순간 2배 · 09-17 검토)
        let data = rs.clone();
        let row_order = self.row_order.clone();
        let col_order = self.col_order.clone();
        let total = row_order.len();
        let opts = GridOpts {
            null: self.null_text.clone(),
            ..GridOpts::default()
        };
        let dialect = self.dialect;
        let table = self.source_table.clone().unwrap_or_else(|| "T".into());
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<TextMsg>();
        let flag = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("nsql-textview".into())
            .spawn(move || {
                let ordered = View::new(&data).rows(&row_order).cols(&col_order);
                let layout = nsql_io::block_layout(&ordered, &opts);
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
                started: std::time::Instant::now(),
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
                    self.text_bytes += lines
                        .iter()
                        .map(|l| (l.capacity() + std::mem::size_of::<String>()) as u64)
                        .sum::<u64>();
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
            if let Some(job) = self.text_job.take() {
                self.text_report = Some((self.text_lines.len(), job.started.elapsed()));
            }
            self.text_ch_hold = 0;
        }
        changed
    }

    /// 텍스트 변환 완료 보고(1회성).
    pub(crate) fn take_text_report(&mut self) -> Option<(usize, std::time::Duration)> {
        self.text_report.take()
    }

    /// 변환이 진행 중인가(호스트가 tick을 돌릴 근거).
    pub(crate) fn text_pending(&self) -> bool {
        self.text_job.is_some()
    }

    /// 표시 순서(정렬·컬럼 이동)대로 복제한 결과 — 텍스트 보기·SQL 보기의 원천.
    /// 전 컬럼 이름(SQL 보기의 키 선택 근거).
    pub(crate) fn all_col_names(&self) -> Vec<String> {
        let Some(rs) = self.rs.as_ref() else {
            return Vec::new();
        };
        self.col_order
            .iter()
            .filter_map(|&ci| rs.columns().get(ci).map(|c| c.name.clone()))
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
                    (
                        &mut self.tb_fetch,
                        &["fetch.all", "fetch.stop", "count"][..],
                    ),
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
                    Some("refresh") if self.can_refresh() => self.want_refresh = true,
                    // 전체 조회 = 나머지 이어 받기(자동 페치가 나가 있으면 큐).
                    Some("fetch.all") => self.request_fetch_all(),
                    Some("fetch.stop") => self.request_cancel(),
                    Some("count") if self.can_count() => {
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
            // ★ 휠·스크롤바로 끝에 닿아도 다음 세그먼트(사용자 09-17 CSV "200행 뒤로 스크롤 시 자동 조회 안 됨" —
            //   종전엔 키보드 스크롤만 아래 판정을 지났다).
            self.text_auto_fetch(body, ch);
            return;
        }
        // 선택(사용자 09-16): 본문 드래그 = 문자 범위 · 거터 클릭/드래그 = 줄 범위 · Ctrl+클릭 = 개별 줄 · Shift = 확장.
        let gutter = Rect::new(self.bounds.x, body.y, self.text_gutter_w, body.h);
        match *ev {
            InputEvent::MouseDown {
                x,
                y,
                shift,
                primary,
            } => {
                let p = Point { x, y };
                if gutter.w > 0 && gutter.contains(p) {
                    self.text_hit.push((
                        x,
                        y,
                        TextHit::GutterDown {
                            shift,
                            ctrl: primary,
                        },
                    ));
                    self.text_drag = Some(TextDrag::Gutter);
                } else if body.contains(p) {
                    self.text_hit.push((x, y, TextHit::Anchor { shift }));
                    self.text_drag = Some(TextDrag::Body);
                }
            }
            InputEvent::MouseMove { x, y } => {
                let head = match self.text_drag {
                    Some(TextDrag::Body) => TextHit::Head,
                    Some(TextDrag::Gutter) => TextHit::GutterHead,
                    None => return,
                };
                // 연속 이동은 마지막 헤드만 남긴다(다운 요청은 덮지 않음).
                match self.text_hit.last_mut() {
                    Some(l) if matches!(l.2, TextHit::Head | TextHit::GutterHead) => {
                        *l = (x, y, head)
                    }
                    _ => self.text_hit.push((x, y, head)),
                }
            }
            InputEvent::MouseUp { .. } => self.text_drag = None,
            InputEvent::SelectAll => self.select_all(),
            InputEvent::Key {
                key: Key::Escape, ..
            } => {
                if self.fetch_all_pending {
                    self.request_cancel();
                } else {
                    self.text_sel = None;
                    self.text_lines_sel.clear();
                }
            }
            _ => {}
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
        self.text_auto_fetch(body, ch);
    }

    /// ★ 텍스트 계열 보기도 스크롤 끝 = 다음 세그먼트(사용자 09-16 · 데이터 원천은 그리드와 같은 결과 세트) · 변환 중이면 미룸.
    ///   그리드의 [`Self::clamp`]와 같은 규칙 — 휠·스크롤바·키 어느 경로든 스크롤이 바뀌면 한 번 판정.
    fn text_auto_fetch(&mut self, body: Rect, ch: i32) {
        let my = (ch - body.h).max(0);
        if self.auto_fetch
            && self.more
            && !self.session_blocked
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

    /// 마우스 점 → (줄, 문자) → 선택 갱신(페인트 안 · `text_x` = 본문 텍스트 시작 x).
    fn resolve_text_hit(
        &mut self,
        dc: &mut dyn DrawCtx,
        hx: i32,
        hy: i32,
        kind: TextHit,
        body: Rect,
        text_x: i32,
    ) {
        let n = self.text_lines.len();
        if n == 0 || self.row_h <= 0 {
            return;
        }
        let line = (((hy - body.y + self.text_scroll.1).max(0)) / self.row_h) as usize;
        let line = line.min(n - 1);
        let col_at = |dc: &mut dyn DrawCtx, s: &str| -> usize {
            let mut w = Vec::new();
            dc.text_prefix_widths(s, &mut w);
            let rel = hx - text_x;
            let mut best = 0usize;
            let mut best_d = i32::MAX;
            for (i, px) in w.iter().enumerate() {
                let d = (rel - px).abs();
                if d < best_d {
                    best_d = d;
                    best = i;
                }
            }
            best
        };
        let col = match kind {
            TextHit::Anchor { .. } | TextHit::Head => col_at(dc, &self.text_lines[line]),
            _ => 0,
        };
        self.apply_text_hit(line, col, kind);
    }

    /// 텍스트 보기 선택 규약(그리드 행번호 열과 동일 · 사용자 09-17):
    /// 본문 클릭 = 문자 앵커(평 = 새로 · Shift = 확장) · 거터 평 클릭/드래그 = 줄 범위(기존 해제) ·
    /// 거터 Ctrl+클릭 = 줄 토글(기존 유지) · 거터 Ctrl+드래그 = 범위 **추가** · 거터 Shift+클릭 = 앵커~줄(기존 집합 유지).
    /// `text_lines_sel` = 확정된 줄 집합 · `text_sel` = 현재 구간 — 그리기·복사는 둘의 합집합.
    fn apply_text_hit(&mut self, line: usize, col: usize, kind: TextHit) {
        let n = self.text_lines.len();
        if n == 0 {
            return;
        }
        let line = line.min(n - 1);
        let len_of = |s: &Self, l: usize| s.text_lines[l].chars().count();
        match kind {
            TextHit::Anchor { shift } => match (shift, self.text_sel) {
                (true, Some((a, _))) => self.text_sel = Some((a, (line, col))),
                _ => {
                    self.text_lines_sel.clear();
                    self.text_sel = Some(((line, col), (line, col)));
                }
            },
            TextHit::Head => {
                if let Some((a, _)) = self.text_sel {
                    self.text_sel = Some((a, (line, col)));
                }
            }
            TextHit::GutterDown { shift, ctrl } => {
                if ctrl {
                    // 현재 구간을 집합으로 확정한 뒤 이 줄을 토글 — 그리드 `toggle_region`과 같은 결과.
                    self.commit_text_range();
                    if let Some(i) = self.text_lines_sel.iter().position(|&l| l == line) {
                        self.text_lines_sel.remove(i);
                        self.text_sel = None;
                    } else {
                        self.text_sel = Some(((line, 0), (line, len_of(self, line))));
                    }
                    self.text_gutter_anchor = Some(line);
                } else if shift {
                    let a = self.text_gutter_anchor.unwrap_or(line);
                    let (lo, hi) = (a.min(line), a.max(line));
                    self.text_sel = Some(((lo, 0), (hi, len_of(self, hi))));
                } else {
                    self.text_lines_sel.clear();
                    self.text_sel = Some(((line, 0), (line, len_of(self, line))));
                    self.text_gutter_anchor = Some(line);
                }
            }
            TextHit::GutterHead => {
                let a = self.text_gutter_anchor.unwrap_or(line);
                let (lo, hi) = (a.min(line), a.max(line));
                self.text_sel = Some(((lo, 0), (hi, len_of(self, hi))));
            }
        }
    }

    /// 현재 구간(`text_sel`)의 줄들을 확정 집합(`text_lines_sel`)에 합친다(Ctrl 누적).
    fn commit_text_range(&mut self) {
        if let Some((a, b)) = self.text_sel.take() {
            let (l0, l1) = (a.0.min(b.0), a.0.max(b.0));
            self.text_lines_sel
                .extend(l0..=l1.min(self.text_lines.len().saturating_sub(1)));
            self.text_lines_sel.sort_unstable();
            self.text_lines_sel.dedup();
        }
    }

    /// 텍스트 보기 복사 — 줄 집합 ∪ 현재 구간(집합이 있으면 구간은 줄 단위) > 문자 범위 > 전체(선택 없음). 반환 = (텍스트, 줄 수).
    fn copy_text_selection(&self) -> Option<(String, usize)> {
        if !self.text_lines_sel.is_empty() {
            let mut set = self.text_lines_sel.clone();
            if let Some((a, b)) = self.text_sel {
                set.extend(a.0.min(b.0)..=a.0.max(b.0));
                set.sort_unstable();
                set.dedup();
            }
            let lines: Vec<&str> = set
                .iter()
                .filter_map(|&i| self.text_lines.get(i).map(String::as_str))
                .collect();
            let text = lines.join("\n");
            return (!text.is_empty()).then_some((text, lines.len()));
        }
        if let Some((a, b)) = self.text_sel {
            let ((l0, c0), (l1, c1)) = if a <= b { (a, b) } else { (b, a) };
            if (l0, c0) != (l1, c1) {
                let mut out = String::new();
                for i in l0..=l1 {
                    let Some(line) = self.text_lines.get(i) else {
                        break;
                    };
                    let len = line.chars().count();
                    let s0 = if i == l0 { c0.min(len) } else { 0 };
                    let s1 = if i == l1 { c1.min(len) } else { len };
                    out.extend(line.chars().skip(s0).take(s1.saturating_sub(s0)));
                    if i < l1 {
                        out.push('\n');
                    }
                }
                return (!out.is_empty()).then_some((out, l1 - l0 + 1));
            }
        }
        let text = self.text_lines.join("\n");
        (!text.is_empty()).then_some((text, self.text_lines.len()))
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
        let h = self.row_h * self.text_lines.len() as i32;
        let hold = if self.text_job.is_some() {
            self.text_ch_hold
        } else {
            0
        };
        (self.text_w, h.max(hold))
    }

    /// 자동 컬럼 너비 한계 — 하한 논리 px(`grid.col_min_width`) · 상한 글자 수(`grid.col_max_mode`/`col_max_chars`).
    /// 바뀌면 폭을 다시 잰다(다음 페인트).
    pub(crate) fn set_col_limits(&mut self, min_px: i32, max_chars: i32) {
        let (min_px, max_chars) = (min_px.max(1), max_chars.max(1));
        if (self.col_min, self.col_max_chars) != (min_px, max_chars) {
            self.col_min = min_px;
            self.col_max_chars = max_chars;
            self.col_w.clear();
        }
    }

    /// 자동 컬럼 너비의 [하한, 상한](물리 px) — 상한 = 글자 수 × 그리드 글꼴 숫자 폭 + 여백.
    fn col_bounds(&self, dc: &mut dyn DrawCtx, s: f32, pad: i32) -> (i32, i32) {
        let lo = (self.col_min as f32 * s).round() as i32;
        let unit = dc.text_width("0").max(1);
        let hi = self.col_max_chars * unit + pad * 2;
        (lo, hi.max(lo))
    }

    pub(crate) fn set_row_numbers(&mut self, on: bool) {
        self.row_numbers = on;
    }

    /// 행 높이 비율(% · 설정 `grid.row_height_pct`).
    pub(crate) fn set_row_pct(&mut self, pct: i32) {
        self.row_pct = pct.clamp(110, 300);
    }

    /// NULL 셀 글자(설정 `grid.null_text` · 그리드·텍스트 보기·복사 공통 · 사용자 09-17 "뷰마다 다르면 안 된다").
    /// 위쪽 경계선(결과 탭 줄이 위에 있으면 끈다 — 1px 유지).
    pub(crate) fn set_top_border(&mut self, on: bool) {
        self.top_border = on;
    }

    /// 행 포커스 배경(켬/끔 · 색 · 알파 덮어쓰기).
    pub(crate) fn set_row_focus(
        &mut self,
        on: bool,
        color: Option<nexa_ctl::Color>,
        alpha: Option<f32>,
    ) {
        self.row_focus = on;
        self.row_focus_color = (color, alpha);
    }

    pub(crate) fn set_null_text(&mut self, text: &str) {
        if self.null_text == text {
            return;
        }
        self.null_text = text.to_string();
        self.col_w.clear(); // 폭은 셀 글자로 재므로 다시.
        if self.view != ResultView::Grid {
            self.refresh_text_view();
        }
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
        // 재조회 = 조회 컬럼 순서·정렬 초기화(이동·정렬 결과 무시 — 사용자 09-14).
        self.col_order = (0..rs.columns.len()).collect();
        self.row_order = (0..rs.rows.len()).collect();
        self.sort_keys.clear();
        self.hdr_drag = None;
        self.hdr_resize = None;
        // 한 세트(DR-33): 덩어리를 이동해 첫 세그먼트로(복사 0). 옛 데이터는 여기서 drop(변환 스레드가 쥔 세그먼트는 그쪽이 끝나면).
        self.rs = Some(ResultData::new(rs));
        self.text_lines = Vec::new();
        self.text_bytes = 0;
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
        // 전체 조회의 교체 결과가 여기로 온다 — 진행·취소·■ 상태를 함께 닫는다(사용자 09-17).
        self.end_fetch_all();
        self.last_run_at = Some(nsql_log::now_local().stamp());
        self.perf_report = true;
        self.text_scroll = (0, 0);
        self.load = t.elapsed();
        if self.view != ResultView::Grid {
            self.refresh_text_view();
        }
    }

    /// 실행 직전 호스트가 알려 주는 원본 문장(스크립트 전체 · 제목·테이블 이름 근거) — 결과가 오면 `set_result_origin`이
    /// 그 결과를 만든 **문장 하나**로 바꾼다. 실행이 시작되면 건수는 셀 수 없다(결과가 올 때 다시 판정).
    pub(crate) fn set_source_sql(&mut self, sql: &str) {
        self.source_table = guess_table(sql);
        self.source_sql = sql.to_string();
        self.countable = false;
        self.sync_fetch_tools();
    }

    pub(crate) fn set_dialect(&mut self, d: Dialect) {
        self.dialect = d;
    }

    /// 직전 클릭을 메뉴가 먹었는가(1회성) — 참이면 호스트는 그 클릭을 아래로 흘리지 않는다.
    pub(crate) fn take_menu_click(&mut self) -> bool {
        std::mem::take(&mut self.menu_click_consumed)
    }

    /// 우클릭 메뉴 닫기(풀다운과 배타 · 09-22).
    pub(crate) fn close_menu(&mut self) {
        self.menu.close();
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
                if let Some(c) = rs.columns().get(ci) {
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
        for (i, region) in regions.into_iter().enumerate() {
            // 선택 구간 = 뷰(행 인덱스 · 열 부분집합) — 값 복사 0(DR-33) · 문장 생성기는 CLI `-f sql:*`와 같은 코드.
            let (rows, cols) = self.region_index(region);
            if rows.is_empty() || cols.is_empty() {
                continue;
            }
            let view = View::new(rs).rows(&rows).cols(&cols);
            let names = view.col_names();
            cells += rows.len() * cols.len();
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&nsql_io::generate_src(
                self.dialect,
                &table,
                &names,
                &view,
                0..rows.len(),
                kind,
                key,
            ));
        }
        (cells > 0).then_some((out, cells))
    }

    /// 선택 구간 → (원본 행 번호 목록(표시 순서), 원본 열 번호 목록(표시 순서)) — 뷰의 입력. 범위는 결과 크기로 자른다.
    fn region_index(&self, region: (usize, usize, usize, usize)) -> (Vec<usize>, Vec<usize>) {
        let (r0, r1, c0, c1) = region;
        let n = self.rows();
        let rows: Vec<usize> = if n == 0 || r0 > r1 {
            Vec::new()
        } else {
            (r0..=r1.min(n - 1))
                .map(|di| self.row_order.get(di).copied().unwrap_or(di))
                .collect()
        };
        let nc = self.col_order.len();
        let cols: Vec<usize> = if nc == 0 || c0 > c1 || c0 >= nc {
            Vec::new()
        } else {
            self.col_order[c0..=c1.min(nc - 1)].to_vec()
        };
        (rows, cols)
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

    /// 행 **전체**가 선택인가(행번호로 고른 행 · 사용자 09-22 "행번호로 골라도 1번 이미지처럼") — 그러면 셀마다 진한 채움 대신
    /// 행 포커스 배경만 칠한다(셀로 고른 것과 같은 모습).
    fn row_fully_selected(&self, di: usize) -> bool {
        let last = self.col_order.len().saturating_sub(1);
        self.regions
            .iter()
            .any(|&(r0, r1, c0, c1)| di >= r0 && di <= r1 && c0 == 0 && c1 >= last)
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
        if self.view != ResultView::Grid {
            if let Some(last) = self.text_lines.len().checked_sub(1) {
                let len = self.text_lines[last].chars().count();
                self.text_sel = Some(((0, 0), (last, len)));
                self.text_lines_sel.clear();
            }
            return;
        }
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
            return self.copy_text_selection();
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
                    // 구간 사이 빈 줄(구간 텍스트에는 끝 줄바꿈이 없으므로 둘).
                    out.push_str("\n\n");
                }
                out.push_str(&text);
                cells += n;
            }
        }
        (cells > 0).then_some((out, cells))
    }

    /// 구간 하나를 형식대로(첫 구간만 헤더) — 선택 = 뷰 → 렌더는 nsql-io 하나(텍스트 보기·CLI와 같은 코드 · DR-33). 값 복사 0.
    fn copy_region(
        &self,
        kind: CopyKind,
        region: (usize, usize, usize, usize),
        first: bool,
    ) -> Option<(String, usize)> {
        let rs = self.rs.as_ref()?;
        let (rows, cols) = self.region_index(region);
        if rows.is_empty() || cols.is_empty() {
            return None;
        }
        let view = View::new(rs).rows(&rows).cols(&cols);
        let opts = GridOpts {
            max_col: 0, // 복사는 자르지 않는다.
            null: self.null_text.clone(),
            ..GridOpts::default()
        };
        let (fmt, header) = match kind {
            CopyKind::Tsv => (Format::Tsv, false),
            CopyKind::TsvWithHeaders => (Format::Tsv, first),
            CopyKind::Csv => (Format::Csv, first),
            // 고정폭 정렬(표시 폭 기준 · 숫자 우측 정렬) — 첫 구간만 머리글.
            CopyKind::Text => (Format::Grid, first),
            CopyKind::Markdown => (Format::Markdown, first),
            // 구간마다 배열 하나(여러 구간이면 배열이 여러 개 — 각각 독립 문서).
            CopyKind::Json => (Format::Json, true),
        };
        let layout = nsql_io::block_layout(&view, &opts);
        let out = nsql_io::render_block(
            &view,
            &fmt,
            self.dialect,
            &layout,
            0..rows.len(),
            header,
            true,
            "T",
            &KeySpec::default(),
        );
        // ★ 복사에는 끝 줄바꿈을 붙이지 않는다(사용자 09-17): 렌더러는 블록(줄마다 `\n`)이라 마지막 행 뒤에도 `\n`이 남는데,
        //   단일 셀/행을 붙여넣을 때 줄바꿈이 따라오면 안 된다 · 여러 행은 행 사이 줄바꿈만.
        let out = out.trim_end_matches(['\n', '\r']).to_string();
        Some((out, rows.len() * cols.len()))
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
        let mut order: Vec<usize> = (0..rs.len()).collect();
        if !self.sort_keys.is_empty() {
            let keys = self.sort_keys.clone();
            order.sort_by(|&a, &b| {
                for (col, asc) in &keys {
                    let (va, vb) = (rs.cell(a, *col), rs.cell(b, *col));
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

    /// 컬럼(표시 위치)의 왼쪽 x(창 좌표 · 스크롤 반영).
    fn col_x(&self, pos: usize) -> i32 {
        let mut cx = self.bounds.x + self.gutter_w - self.scroll_x;
        for &ci in self.col_order.iter().take(pos) {
            cx += self.col_w.get(ci).copied().unwrap_or(80);
        }
        cx
    }

    /// 커서 x에 가장 가까운 컬럼 경계(0..=n · 삽입 위치).
    fn nearest_boundary(&self, x: i32) -> usize {
        let n = self.col_order.len();
        let mut best = 0;
        let mut best_d = i32::MAX;
        for k in 0..=n {
            let d = (self.col_x(k) - x).abs();
            if d < best_d {
                best_d = d;
                best = k;
            }
        }
        best
    }

    /// 드래그 중 라이브 미리보기 — 고스트 중앙에 가장 가까운 경계로 잡은 컬럼을 옮긴다.
    fn preview_col_drag(&mut self) {
        let Some(d) = self.hdr_drag.clone() else {
            return;
        };
        if !d.active || d.pos >= self.col_order.len() {
            return;
        }
        let w = self.col_w.get(self.col_order[d.pos]).copied().unwrap_or(80);
        let center = d.cur_x - d.grab_dx + w / 2;
        let ins = self.nearest_boundary(center);
        let to = if ins > d.pos { ins - 1 } else { ins };
        if to != d.pos && to < self.col_order.len() {
            let c = self.col_order.remove(d.pos);
            self.col_order.insert(to, c);
            if let Some(d) = self.hdr_drag.as_mut() {
                d.pos = to;
            }
        }
    }

    /// Esc = 컬럼 이동 취소(시작 때 순서로 복원). 드래그 중이었으면 true(호스트가 키를 소비).
    pub(crate) fn cancel_col_drag(&mut self) -> bool {
        let Some(d) = self.hdr_drag.take() else {
            return false;
        };
        if !d.active {
            return false;
        }
        if d.orig.len() == self.col_order.len() {
            self.col_order = d.orig;
        }
        true
    }

    /// 헤더 드래그(컬럼 이동)가 진행 중인가.
    pub(crate) fn col_dragging(&self) -> bool {
        self.hdr_drag.as_ref().is_some_and(|d| d.active)
    }

    #[allow(dead_code)]
    pub(crate) fn set_messages(&mut self, m: Vec<String>) {
        self.messages = m;
    }

    /// 결과 비우기 — 오류가 나도 결과 영역은 **기본 형태**(행번호 1 · `** No Records **`)를 유지한다(사용자 09-17).
    /// 오류·메시지 본문은 로그 창으로만 간다.
    /// 전체 조회가 진행 중인가(툴바/카드 ■ 활성 판정).
    pub(crate) fn fetch_all_active(&self) -> bool {
        self.fetch_all_pending
    }

    pub(crate) fn clear_result(&mut self) {
        self.rs = None;
        self.col_order.clear();
        self.row_order.clear();
        self.sort_keys.clear();
        self.text_lines = Vec::new();
        self.text_bytes = 0;
        self.messages.clear();
        self.scroll_y = 0;
        self.scroll_x = 0;
        self.col_w.clear();
        self.regions.clear();
        self.sel_anchor = None;
        self.sel_cur = None;
        self.drag_sel = None;
        self.more = false;
        self.total = None;
        self.fetching = false;
        self.fetch_req = None;
        self.end_fetch_all();
        self.text_scroll = (0, 0);
        self.sync_fetch_tools();
    }

    fn rows(&self) -> usize {
        self.rs.as_ref().map_or(0, |r| r.len())
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
            && !self.session_blocked
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
                    // 컬럼 이동 중 Esc = 취소(시작 순서로).
                    if self.cancel_col_drag() {
                        return;
                    }
                    // 전체 조회 중 Esc = 가져오기 중지(선택 해제보다 먼저 · T-48b).
                    if self.fetch_all_pending {
                        self.request_cancel();
                        return;
                    }
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
                        self.hdr_drag = Some(HdrDrag {
                            pos,
                            press_x: x,
                            cur_x: x,
                            active: false,
                            shift,
                            grab_dx: x - self.col_x(pos),
                            orig: self.col_order.clone(),
                        });
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
                        d.cur_x = x;
                        if (x - d.press_x).abs() > 4 {
                            d.active = true;
                        }
                    }
                    self.preview_col_drag();
                    return;
                }
                InputEvent::MouseUp { .. } if self.hdr_drag.is_some() => {
                    // 미리보기가 곧 결과 — 놓으면 확정 · 안 움직였으면 정렬 클릭.
                    if let Some(d) = self.hdr_drag.take() {
                        if !d.active {
                            if let Some(&col) = self.col_order.get(d.pos) {
                                self.toggle_sort(col, d.shift);
                            }
                        }
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
        // 렌더 시작/종료 시각 스탬프 = 보고 대기 중 + Render 층 상세가 켜진 경우에만(게이트 = 원자 load 1회 · docs/48).
        let stamp = self.perf_report
            && nsql_log::wants(nsql_log::LogLayer::Render, nsql_log::LogLevel::Timing);
        let started = stamp.then(|| nsql_log::now_local().stamp());
        self.paint_inner(dc, th, s);
        self.render = t_render.elapsed();
        if let Some(st) = started {
            self.render_at = Some((st, nsql_log::now_local().stamp()));
        }
    }

    fn paint_inner(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        let b = self.bounds;
        dc.fill_rect(b, th.panel_bg);
        if self.top_border {
            dc.fill_rect(Rect::new(b.x, b.y, b.w, 1), th.border);
        }
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
        // No Records(결과 없음 · 0행 · **컬럼도 없음**) — 작은 셀 하나(Golden 모양 · 사용자 09-16).
        // 컬럼이 있는 0행(빈 테이블 SELECT)은 아래 일반 경로로 **헤더를 그대로** 그리고 1행에 `** No Records **`(사용자 09-17 캡처).
        let empty = self.rs.as_ref().is_none_or(|r| r.is_empty());
        let no_cols = self.rs.as_ref().is_none_or(|r| r.columns().is_empty());
        if empty && no_cols && self.view == ResultView::Grid {
            // Golden 방식(사용자 09-16 2번 이미지): 행번호 칸 + `No Records` 폭만큼의 작은 셀 하나 — 나머지는 빈 바탕.
            let label = t(Msg::GridNoRecords);
            dc.select_font(FontSlot::Mono, false);
            let gw = dc.text_width("0") * 2 + pad * 2;
            dc.select_font(FontSlot::Base, false);
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
            dc.select_font(FontSlot::Mono, false);
            dc.text(b.x + pad, cy, row, "1", th.text_dim);
            dc.select_font(FontSlot::Base, false);
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
        let null = self.null_text.clone();
        let (lo, hi) = self.col_bounds(dc, s, pad);
        if self.col_w.is_empty() {
            self.col_w = rs
                .columns()
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let mut w = dc.text_width(&c.name);
                    for row in rs.rows().take(200) {
                        if let Some(v) = row.get(i) {
                            w = w.max(dc.text_width(&cell_text(v, &null)));
                        }
                    }
                    (w + pad * 2).clamp(lo, hi)
                })
                .collect();
        }
        if let Some(ci) = self.autofit.take() {
            // 헤더 이름 + 전 행(최대 5,000행) 중 가장 넓은 값 → [최소, 최대].
            let mut w = rs.columns().get(ci).map_or(0, |c| dc.text_width(&c.name));
            for row in rs.rows().take(5000) {
                if let Some(v) = row.get(ci) {
                    w = w.max(dc.text_width(&cell_text(v, &null)));
                }
            }
            let w = (w + pad * 2).clamp(lo, hi);
            if let Some(cw) = self.col_w.get_mut(ci) {
                *cw = w;
            }
        }
        let header = Rect::new(b.x, b.y + 1, b.w, self.row_h + 2);
        self.header_h = header.h + 1;
        // 행번호 열 폭(자릿수 × 숫자 폭 + 여백) — 가로 스크롤과 무관한 고정 열.
        self.gutter_w = if self.row_numbers {
            let digits = rs.len().max(1).to_string().len().max(2) as i32;
            dc.select_font(FontSlot::Mono, false);
            let w = digits * dc.text_width("0") + pad * 2;
            dc.select_font(FontSlot::Base, false);
            w
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
        let n = rs.len();
        for di in first..n {
            if y >= body.bottom() {
                break;
            }
            let ri = self.row_order.get(di).copied().unwrap_or(di);
            let Some(row) = rs.row(ri) else { break };
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
            // ★ 행 포커스 배경(사용자 09-22): 선택에 걸린 행 전체(다중 행 선택도 같은 규칙) = 셀 선택색보다 연하게.
            if self.row_focus && self.row_in_sel(di) {
                let (c, a) = self.row_focus_color;
                dc.fill_rect_alpha(rr, c.unwrap_or(th.sel_bg), a.unwrap_or(0.35));
            }
            let cells = Rect::new(gx0, body.y, (b.right() - gx0).max(0), body.h);
            let mut x = gx0 - self.scroll_x;
            for (pos, &ci) in self.col_order.iter().enumerate() {
                let Some(v) = row.get(ci) else { continue };
                let cw = self.col_w.get(ci).copied().unwrap_or(80);
                let clip = Rect::new(x, y, cw - 1, self.row_h).intersection(&cells);
                if clip.w > 0
                    && self.in_sel(di, pos)
                    && !(self.row_focus && self.row_fully_selected(di))
                {
                    dc.fill_rect_alpha(clip, th.sel_bg, 0.85);
                }
                if clip.w > 0 && self.sel_cur == Some((di, pos)) {
                    dc.stroke_round_rect(clip, 0, th.accent, 1.0);
                }
                if clip.w > 0 && clip.h > 0 {
                    let txt = cell_text(v, &null);
                    let numeric = matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_));
                    // NULL은 흐린 글자보다 **더 흐리게**(배경 쪽으로 45 % · 사용자 09-22).
                    let color = if matches!(v, Value::Null) {
                        th.text_dim.lerp(th.panel_bg, 0.45)
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
                dc.select_font(FontSlot::Mono, false);
                let nw = dc.text_width(&num);
                let gclip = Rect::new(b.x, y, self.gutter_w, self.row_h).intersection(&body);
                dc.fill_rect(gclip, th.chrome_bg);
                let selected_row = self.row_in_sel(di);
                if selected_row {
                    dc.fill_rect_alpha(gclip, th.sel_bg, 0.85);
                }
                let ny = dc.text_center_y(y, self.row_h);
                dc.text(
                    gx0 - pad - nw,
                    ny,
                    gclip,
                    &num,
                    if selected_row { th.text } else { th.text_dim },
                );
                dc.select_font(FontSlot::Base, false);
            }
            y += self.row_h;
        }
        // 0행 + 컬럼 있음 = 헤더는 구조 그대로 · 1행에 `** No Records **`(첫 컬럼 · 흐리게 · 선택/편집 대상 아님).
        if n == 0 && !self.col_order.is_empty() && body.h > 0 {
            let rr = Rect::new(b.x, body.y, b.w, self.row_h).intersection(&body);
            let cy = dc.text_center_y(body.y, self.row_h);
            let first_w = self
                .col_order
                .first()
                .and_then(|&ci| self.col_w.get(ci).copied())
                .unwrap_or(80);
            let cells = Rect::new(gx0, body.y, (b.right() - gx0).max(0), body.h);
            let clip = Rect::new(gx0 - self.scroll_x, body.y, first_w - 1, self.row_h)
                .intersection(&cells);
            dc.text(
                gx0 - self.scroll_x + pad,
                cy,
                clip,
                t(Msg::GridNoRecords),
                th.text_dim,
            );
            if self.gutter_w > 0 {
                let gclip = Rect::new(b.x, body.y, self.gutter_w, self.row_h).intersection(&body);
                dc.fill_rect(gclip, th.chrome_bg);
                dc.select_font(FontSlot::Mono, false);
                let nw = dc.text_width("1");
                dc.text(gx0 - pad - nw, cy, gclip, "1", th.text_dim);
                dc.select_font(FontSlot::Base, false);
            }
            dc.fill_rect(Rect::new(b.x, rr.bottom() - 1, b.w, 1), th.border);
        }
        if self.gutter_w > 0 {
            dc.fill_rect(Rect::new(gx0 - 1, body.y, 1, body.h), th.border);
        }
        // ── 헤더(행 위에 덮어 그린다 — 부분 스크롤된 첫 행이 헤더 아래로 들어간다)
        dc.fill_rect(header, th.chrome_bg);
        let hcells = Rect::new(gx0, header.y, (b.right() - gx0).max(0), header.h);
        let mut x = gx0 - self.scroll_x;
        let dragging = self.hdr_drag.as_ref().filter(|d| d.active);
        let mut ghost: Option<(Rect, String)> = None;
        for (pos, &ci) in self.col_order.iter().enumerate() {
            let Some(c) = rs.columns().get(ci) else {
                continue;
            };
            let cw = self.col_w.get(ci).copied().unwrap_or(80);
            let clip = Rect::new(x, header.y, cw, header.h).intersection(&hcells);
            if let Some(d) = dragging.filter(|d| d.pos == pos) {
                // 잡은 컬럼의 제자리(= 놓일 자리 · 라이브 미리보기) = 자리 표시 · 본체는 고스트로 커서 아래.
                dc.fill_rect_alpha(clip, th.accent, 0.12);
                dc.fill_rect(Rect::new(clip.x, header.y, 1, header.h), th.accent);
                dc.fill_rect(
                    Rect::new(clip.right() - 1, header.y, 1, header.h),
                    th.accent,
                );
                let gx = (d.cur_x - d.grab_dx).clamp(hcells.x, (hcells.right() - cw).max(hcells.x));
                ghost = Some((Rect::new(gx, header.y, cw, header.h), c.name.clone()));
                x += cw;
                continue;
            } else if self.col_in_sel(pos) {
                // 선택에 걸린 컬럼 헤더 = 행번호 강조와 **같은 색**(사용자 09-22 · n×m 선택이면 걸린 열 전부).
                dc.fill_rect_alpha(clip, th.sel_bg, 0.85);
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
        // 드래그 고스트 헤더(커서 x 추종 · 세로는 헤더 행 고정 · 뷰 안 클램프) — 헤더 층 맨 마지막.
        if let Some((g, name)) = ghost {
            dc.fill_rect(g, th.chrome_bg);
            dc.fill_rect(Rect::new(g.x, g.y, g.w, 1), th.accent);
            dc.fill_rect(Rect::new(g.x, g.bottom() - 1, g.w, 1), th.accent);
            dc.fill_rect(Rect::new(g.x, g.y, 1, g.h), th.accent);
            dc.fill_rect(Rect::new(g.right() - 1, g.y, 1, g.h), th.accent);
            let hy = dc.text_center_y(g.y, g.h);
            dc.text(
                g.x + pad,
                hy,
                Rect::new(g.x + 1, g.y + 1, (g.w - 2).max(0), (g.h - 2).max(0)),
                &name,
                th.text,
            );
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
        let n = rs.len();
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
            match self.fetch_progress {
                Some((rows, bytes)) => info.push_str(&format!(
                    " · {}",
                    nsql_i18n::tf(
                        Msg::StFetchingProgress,
                        &[&group_digits(rows), &fmt_bytes(bytes)]
                    )
                )),
                None => info.push_str(&format!(" · {}", t(Msg::StFetching))),
            }
        }
        info.push_str(&format!(" · ~{}", fmt_bytes(self.approx_bytes())));
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
            dc.select_font(FontSlot::Mono, false);
            let w = digits * dc.text_width("0") + pad * 2;
            dc.select_font(FontSlot::Base, false);
            w
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
        // 히트 요청(마우스) → (줄, 문자): 글꼴 실측이 있는 여기서 한 번.
        for (hx, hy, kind) in std::mem::take(&mut self.text_hit) {
            self.resolve_text_hit(dc, hx, hy, kind, body, x);
        }
        let sel = self
            .text_sel
            .map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        let space_w = dc.text_width(" ");
        for (i, line) in self.text_lines.iter().enumerate().skip(first) {
            if y >= body.bottom() {
                break;
            }
            // 선택 배경(줄 집합 = 전체 · 범위 = 문자 구간 · 다음 줄로 이어지면 줄 끝 + 한 칸).
            let line_len = line.chars().count();
            let range = if self.text_lines_sel.contains(&i) {
                Some((0, line_len, true))
            } else if let Some(((l0, c0), (l1, c1))) = sel {
                if i >= l0 && i <= l1 {
                    let s0 = if i == l0 { c0.min(line_len) } else { 0 };
                    let s1 = if i == l1 { c1.min(line_len) } else { line_len };
                    Some((s0, s1, i < l1))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((s0, s1, spans_next)) = range {
                let mut w = Vec::new();
                dc.text_prefix_widths(line, &mut w);
                let x0 = x + w.get(s0).copied().unwrap_or(0);
                let mut x1 = x + w.get(s1).copied().unwrap_or(0);
                if spans_next {
                    x1 += space_w;
                }
                let (x0, x1) = (x0.max(body.x), x1.min(body.right()));
                if x1 > x0 {
                    dc.fill_rect(Rect::new(x0, y, x1 - x0, rh), th.sel_bg);
                }
            }
            dc.text(x, y, body, line, th.text);
            // ★ 가로 스크롤 끝 = 실측 최대 폭(사용자 09-17): 글자 수 기준 후보는 비례 글꼴·한글 폭에서 빗나가고 추가
            //   페치로 더 긴 줄이 올 수 있다 → 보이는 줄을 잴 때마다 **커지는 쪽으로만** 갱신(짧아지면 유지).
            let lw = dc.text_width(line) + pad * 2;
            if lw > self.text_w {
                self.text_w = lw;
            }
            if gw > 0 {
                let num = (i + 1).to_string();
                dc.select_font(FontSlot::Mono, false);
                let nw = dc.text_width(&num);
                dc.text(gutter.right() - pad - nw, y, gutter, &num, th.text_dim);
                dc.select_font(FontSlot::Base, false);
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

/// 그리드 셀 글자 — NULL은 설정 글자(`grid.null_text` · 텍스트 보기·복사와 같은 값) · 바이트는 길이만(셀 폭 보호 · 텍스트 계열은 nsql-io가 16진).
fn cell_text(v: &Value, null: &str) -> String {
    match v {
        Value::Null => null.to_string(),
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

    fn text_grid() -> Grid {
        let mut g = grid_with(&[100]);
        g.text_lines = (0..10).map(|i| format!("line{i:02}")).collect();
        g.text_gutter_w = 30;
        g.footer_h = 0;
        g
    }

    /// Σ 건수 활성 규정(사용자 09-19 · MC/DC): 결과 있음 · 조회 문장에서 옴 · 세션 연결·한가 · 페치 중 아님 · **서버에 더 있음** —
    /// 하나라도 아니면 꺼짐. 새로고침은 결과+출처 문장+연결.
    /// 새 그리드는 서버로 나가는 버튼이 전부 꺼져 있다(결과 없음) — 생성자의 `.disabled()` 표기가 아니라 판정 함수로.
    #[test]
    fn fresh_grid_tools_are_disabled() {
        let g = Grid::default();
        for id in ["fetch.all", "fetch.stop", "count"] {
            assert!(!g.tb_fetch.item_enabled(id), "{id}");
        }
        assert!(!g.tb_refresh.item_enabled("refresh"));
        assert!(!g.can_count());
        assert!(!g.can_refresh());
    }

    #[test]
    fn count_button_rules() {
        let mut g = Grid::default();
        assert!(!g.can_count(), "실행한 적 없음(결과 없음)");
        assert!(
            !g.tb_refresh.item_enabled("refresh"),
            "결과 없음 = 새로고침도 꺼짐"
        );
        g = grid_with(&[100]);
        assert!(!g.can_count(), "결과는 있으나 출처 문장을 모른다");
        assert!(
            !g.tb_refresh.item_enabled("refresh"),
            "출처 문장 없음 = 새로고침 꺼짐"
        );
        g.set_result_origin("SELECT * FROM T", true);
        assert!(
            !g.can_count(),
            "전부 받았으면(더 없음) 건수 = 행 수 → 불필요"
        );
        assert!(g.tb_refresh.item_enabled("refresh"));
        g.set_more(true);
        assert!(g.can_count());
        g.set_session_connected(false);
        assert!(!g.can_count(), "연결 전");
        assert!(
            !g.tb_refresh.item_enabled("refresh"),
            "연결 전 = 새로고침도 꺼짐"
        );
        assert!(
            !g.tb_fetch.item_enabled("fetch.all"),
            "연결 전 = 전체 조회도 꺼짐"
        );
        g.set_session_connected(true);
        assert!(g.can_count());
        assert!(g.tb_fetch.item_enabled("count"));
        g.set_result_origin("CREATE TABLE X (A INT)", false);
        assert!(!g.can_count(), "DDL 결과");
        assert!(!g.tb_fetch.item_enabled("count"));
        g.set_result_origin("SELECT 1", true);
        g.set_session_blocked(true);
        assert!(!g.can_count(), "세션 작업 중");
        g.set_session_blocked(false);
        g.fetch_req = Some(FetchReq::Count);
        let _ = g.take_fetch_request();
        assert!(!g.can_count(), "건수 요청이 나가 있음");
        g.set_total(10);
        assert!(g.can_count());
        // 다음 실행이 시작되면 결과가 올 때까지 셀 수 없다.
        g.set_source_sql("CREATE TABLE Y (A INT)");
        assert!(!g.can_count());
    }

    /// 헤더 드래그 = 라이브 미리보기(가장 가까운 경계로 즉시) · 놓으면 확정 · Esc = 시작 순서로(사용자 09-19).
    #[test]
    fn header_drag_previews_and_escape_restores() {
        let mut g = grid_with(&[100, 100, 100]);
        let hy = g.header_rect().y + 5;
        // c0(30..130)를 x=80에서 잡고(잡은 오프셋 50) 190으로 → 고스트 중앙 190 · 가장 가까운 경계 230 → [1, 0, 2].
        g.on_event(
            &InputEvent::MouseDown {
                x: 80,
                y: hy,
                shift: false,
                primary: false,
            },
            1.0,
        );
        g.on_event(&InputEvent::MouseMove { x: 190, y: hy }, 1.0);
        assert!(g.col_dragging());
        assert_eq!(g.col_order, vec![1, 0, 2], "미리보기");
        assert!(g.cancel_col_drag());
        assert_eq!(g.col_order, vec![0, 1, 2], "Esc 복원");
        assert!(!g.col_dragging());
        // 다시 끌고 놓으면 확정 · 정렬은 바뀌지 않는다.
        g.on_event(
            &InputEvent::MouseDown {
                x: 80,
                y: hy,
                shift: false,
                primary: false,
            },
            1.0,
        );
        g.on_event(&InputEvent::MouseMove { x: 290, y: hy }, 1.0);
        g.on_event(&InputEvent::MouseUp { x: 290, y: hy }, 1.0);
        assert_eq!(g.col_order, vec![1, 2, 0]);
        assert!(g.sort_keys.is_empty());
        // 안 움직인 클릭 = 정렬.
        g.on_event(
            &InputEvent::MouseDown {
                x: 80,
                y: hy,
                shift: false,
                primary: false,
            },
            1.0,
        );
        g.on_event(&InputEvent::MouseUp { x: 80, y: hy }, 1.0);
        assert_eq!(g.sort_keys.len(), 1);
    }

    /// 다운 요청 뒤 이동은 큐에 **덧붙여** 앵커를 덮지 않는다(사용자 09-17: macOS 클릭 직후 CursorMoved).
    #[test]
    fn text_hit_queue_keeps_anchor_before_head() {
        let mut g = text_grid();
        g.view = ResultView::Text;
        g.on_event(
            &InputEvent::MouseDown {
                x: 100,
                y: 50,
                shift: false,
                primary: false,
            },
            1.0,
        );
        g.on_event(&InputEvent::MouseMove { x: 120, y: 50 }, 1.0);
        g.on_event(&InputEvent::MouseMove { x: 130, y: 60 }, 1.0);
        let kinds: Vec<TextHit> = g.text_hit.iter().map(|h| h.2).collect();
        assert_eq!(kinds, vec![TextHit::Anchor { shift: false }, TextHit::Head]);
        assert_eq!(g.text_hit[1].0, 130);
        g.on_event(&InputEvent::MouseUp { x: 130, y: 60 }, 1.0);
        g.on_event(&InputEvent::MouseMove { x: 10, y: 10 }, 1.0);
        assert_eq!(
            g.text_hit.len(),
            2,
            "드래그가 끝나면 이동은 요청을 만들지 않는다"
        );
    }

    /// 드래그 뒤 평 클릭 = 새 앵커(옛 앵커 유지 = 보고된 결함).
    #[test]
    fn text_plain_click_after_drag_starts_fresh() {
        let mut g = text_grid();
        g.apply_text_hit(2, 1, TextHit::Anchor { shift: false });
        g.apply_text_hit(5, 3, TextHit::Head);
        assert_eq!(g.text_sel, Some(((2, 1), (5, 3))));
        g.apply_text_hit(7, 0, TextHit::Anchor { shift: false });
        g.apply_text_hit(7, 2, TextHit::Head);
        assert_eq!(g.text_sel, Some(((7, 0), (7, 2))));
        g.apply_text_hit(1, 0, TextHit::Anchor { shift: true });
        assert_eq!(g.text_sel, Some(((7, 0), (1, 0))), "Shift 클릭만 확장");
    }

    /// 거터 = 그리드 행번호 열 규약: 평 드래그 범위 · Ctrl 클릭 토글 · Ctrl 드래그 추가 · Shift 범위(집합 유지).
    #[test]
    fn text_gutter_matches_grid_row_header() {
        let mut g = text_grid();
        let down = |s: bool, c: bool| TextHit::GutterDown { shift: s, ctrl: c };
        g.apply_text_hit(1, 0, down(false, false));
        g.apply_text_hit(3, 0, TextHit::GutterHead);
        assert_eq!(g.text_sel, Some(((1, 0), (3, 6))));
        assert_eq!(g.copy_text_selection().map(|c| c.1), Some(3));
        // Ctrl+클릭 = 기존(1~3) 유지 + 5 추가.
        g.apply_text_hit(5, 0, down(false, true));
        assert_eq!(g.text_lines_sel, vec![1, 2, 3]);
        assert_eq!(g.text_sel, Some(((5, 0), (5, 6))));
        assert_eq!(g.copy_text_selection().map(|c| c.1), Some(4));
        // Ctrl+드래그 7→8 = 범위 추가(집합 1,2,3,5 유지).
        g.apply_text_hit(7, 0, down(false, true));
        g.apply_text_hit(8, 0, TextHit::GutterHead);
        assert_eq!(g.text_lines_sel, vec![1, 2, 3, 5]);
        assert_eq!(g.text_sel, Some(((7, 0), (8, 6))));
        let (txt, n) = g.copy_text_selection().unwrap_or_default();
        assert_eq!(n, 6);
        assert_eq!(txt, "line01\nline02\nline03\nline05\nline07\nline08");
        // Ctrl+클릭으로 선택된 줄 = 제거.
        g.apply_text_hit(2, 0, down(false, true));
        assert_eq!(g.text_lines_sel, vec![1, 3, 5, 7, 8]);
        assert_eq!(g.text_sel, None);
        // Shift+클릭 = 앵커(2)~4 구간 · 집합 유지.
        g.apply_text_hit(4, 0, down(true, false));
        assert_eq!(g.text_sel, Some(((2, 0), (4, 6))));
        assert_eq!(g.text_lines_sel, vec![1, 3, 5, 7, 8]);
        // 평 클릭 = 전부 해제 후 한 줄.
        g.apply_text_hit(9, 0, down(false, false));
        assert!(g.text_lines_sel.is_empty());
        assert_eq!(g.text_sel, Some(((9, 0), (9, 6))));
    }

    /// 푸터 진행 표시의 천 단위 구분.
    /// 전체 조회 도구줄: 요청이 나가면 ■만 켜지고, 교체 결과가 오면(완료) ■는 꺼지고 전체 조회가 다시 켜진다 —
    /// 완료 뒤에도 ■가 켜져 있던 결함(사용자 09-17). 실패·중지 경로도 같다. 결과가 없으면 둘 다 꺼짐.
    #[test]
    fn stop_button_follows_fetch_all_lifecycle() {
        let g = Grid::default();
        assert_eq!(
            g.fetch_tools_enabled(),
            (false, false),
            "결과 없음 = 둘 다 꺼짐"
        );
        let mut g = grid_with(&[100]);
        assert_eq!(
            g.fetch_tools_enabled(),
            (false, false),
            "더 없음 = 전체 조회도 꺼짐"
        );
        g.set_more(true);
        assert_eq!(
            g.fetch_tools_enabled(),
            (true, false),
            "더 있음 = 전체 조회만"
        );
        g.request_fetch_all();
        assert_eq!(g.take_fetch_request(), Some(FetchReq::All));
        assert_eq!(
            g.fetch_tools_enabled(),
            (false, true),
            "나가 있는 동안 = ■만"
        );
        g.cancel_req = true;
        // 완료(나머지 도착 · 이어 붙임) — 늦게 눌린 취소 요청도 함께 버린다 · 스크롤은 그대로.
        g.scroll_y = 40;
        g.append_all(
            ResultSet {
                columns: vec![],
                rows: vec![vec![Value::Int(7)]],
            },
            false,
        );
        assert_eq!((g.rows(), g.scroll_y), (2, 40), "이어 붙고 위치 유지");
        assert_eq!(
            g.fetch_tools_enabled(),
            (false, false),
            "완료 = ■ 꺼짐 · 전부 받았으니 전체 조회도 꺼짐"
        );
        assert!(!g.take_cancel_request(), "완료 뒤 취소 요청은 남지 않는다");
        assert!(g.fetch_progress.is_none());
        // 실패 경로도 같다.
        g.set_more(true);
        g.request_fetch_all();
        g.take_fetch_request();
        assert_eq!(g.fetch_tools_enabled(), (false, true));
        g.fetch_failed();
        assert_eq!(g.fetch_tools_enabled(), (true, false));
        // 자동 페치가 나가 있으면 전체 조회는 큐에서 기다렸다가 그 세그먼트 뒤에 나간다(offset 정확).
        g.fetch_req = Some(FetchReq::Next {
            offset: 2,
            limit: 1,
        });
        assert!(g.take_fetch_request().is_some());
        g.request_fetch_all();
        assert_eq!(
            g.take_fetch_request(),
            None,
            "다른 페치가 나가 있는 동안은 대기"
        );
        assert!(g.fetch_tools_enabled().1, "큐에 있어도 ■는 켜짐");
        g.append_page(
            ResultSet {
                columns: vec![],
                rows: vec![vec![Value::Int(8)]],
            },
            true,
        );
        assert_eq!(g.rows(), 3, "늦은 세그먼트를 버리지 않는다");
        assert_eq!(g.take_fetch_request(), Some(FetchReq::All));
        // 큐에만 있는 전체 조회의 중지 = 요청 취소(워커 깃발 없이).
        g.fetch_failed();
        g.request_fetch_all();
        g.request_cancel();
        assert_eq!((g.fetch_req, g.take_cancel_request()), (None, false));
        assert_eq!(g.fetch_tools_enabled(), (true, false));
    }

    /// DR-33: 복사 = 선택 뷰 → nsql-io 공용 렌더(형식 5종) · NULL 글자는 설정 하나 · 정렬·열 순서(표시 순서) 반영 ·
    /// 추가 페치는 세그먼트로 이어 붙어 같은 세트에서 파생된다.
    /// 복사 결과 끝에 줄바꿈이 없다(단일 셀 = 값만 · 여러 행 = 행 사이만 · 사용자 09-17).
    #[test]
    fn copy_has_no_trailing_newline() {
        let mut g = grid_with(&[100, 80]);
        let rs = ResultSet {
            columns: vec![Column {
                name: "c0".into(),
                type_name: String::new(),
            }],
            rows: (0..3).map(|i| vec![Value::Int(i)]).collect(),
        };
        g.set_result(rs);
        g.regions = vec![(1, 1, 0, 0)]; // (r0, r1, c0, c1)
        let (one, n) = g.copy_selection(CopyKind::Tsv).expect("one cell");
        assert_eq!((one.as_str(), n), ("1", 1));
        g.regions = vec![(0, 2, 0, 0)];
        let (many, n) = g.copy_selection(CopyKind::Tsv).expect("three rows");
        assert_eq!((many.as_str(), n), ("0\n1\n2", 3));
        assert!(!many.ends_with('\n'));
    }

    #[test]
    fn copy_uses_shared_renderer_null_text_and_segments() {
        let mut g = Grid::default();
        g.set_result(ResultSet {
            columns: vec![
                Column {
                    name: "id".into(),
                    type_name: String::new(),
                },
                Column {
                    name: "nm".into(),
                    type_name: String::new(),
                },
            ],
            rows: vec![vec![Value::Int(2), Value::Null]],
        });
        g.append_page(
            ResultSet {
                columns: vec![],
                rows: vec![vec![Value::Int(1), Value::Str("a|b".into())]],
            },
            false,
        );
        assert_eq!(
            (g.rows(), g.rs.as_ref().map_or(0, ResultData::segments)),
            (2, 2)
        );
        g.set_null_text("∅");
        g.sort_keys = vec![(0, true)];
        g.apply_sort();
        g.col_order = vec![1, 0]; // 열 이동: nm, id
        g.select_all();
        let (csv, cells) = g.copy_selection(CopyKind::Csv).expect("Csv");
        assert_eq!(csv, "nm,id\na|b,1\n∅,2", "끝 줄바꿈 없음(09-17)");
        assert_eq!(cells, 4);
        let (md, _) = g.copy_selection(CopyKind::Markdown).expect("Markdown");
        assert_eq!(md, "| nm | id |\n| --- | ---: |\n| a\\|b | 1 |\n| ∅ | 2 |");
        let (txt, _) = g.copy_selection(CopyKind::Text).expect("Text");
        assert_eq!(txt, "nm   id\n---  --\na|b   1\n∅     2");
        let (tsv, _) = g.copy_selection(CopyKind::Tsv).expect("Tsv");
        assert_eq!(tsv, "a|b\t1\n∅\t2");
        let (json, _) = g.copy_selection(CopyKind::Json).expect("Json");
        assert_eq!(
            json,
            "[\n  {\"nm\": \"a|b\", \"id\": 1},\n  {\"nm\": null, \"id\": 2}\n]"
        );
        // 예산 회계: 데이터 + 텍스트 캐시(그리드 보기 = 0).
        assert!(g.approx_bytes() > 0 && g.text_bytes == 0);
    }

    /// 텍스트 보기: 휠(스크롤바가 소비)로 끝에 닿아도 다음 세그먼트 요청이 나간다(사용자 09-17 CSV).
    #[test]
    fn text_view_wheel_to_end_requests_next_page() {
        let mut g = text_grid();
        g.view = ResultView::Csv;
        g.bounds = Rect::new(0, 0, 400, 100);
        g.text_w = 100;
        g.more = true;
        g.page_rows = 200;
        let mut n = 0;
        while g.fetch_req.is_none() && n < 50 {
            g.on_event(&InputEvent::Wheel { delta: -120 }, 1.0);
            n += 1;
        }
        assert_eq!(
            g.fetch_req,
            Some(FetchReq::Next {
                offset: 1,
                limit: 200
            }),
            "휠 {n}회 뒤"
        );
    }

    /// ★ 페치 행 수 0(= 전체 조회)에서는 스크롤 자동 페치가 **절대** 나가지 않는다(사용자 09-17: 0으로 조회 중 중지 → 부분 행 상태에서
    ///   스크롤해도 자동 페치 금지) — 그리드·텍스트 보기 모두 · `more`가 true여도.
    #[test]
    fn zero_page_rows_never_auto_fetches() {
        // 텍스트 보기
        let mut g = text_grid();
        g.view = ResultView::Csv;
        g.bounds = Rect::new(0, 0, 400, 100);
        g.text_w = 100;
        g.more = true;
        g.page_rows = 0;
        for _ in 0..50 {
            g.on_event(&InputEvent::Wheel { delta: -120 }, 1.0);
        }
        assert_eq!(g.fetch_req, None, "텍스트 보기 · 0행이면 자동 페치 없음");
        // 그리드
        let mut g = grid_with(&[100, 80]);
        let rs = ResultSet {
            columns: vec![Column {
                name: "c0".into(),
                type_name: String::new(),
            }],
            rows: (0..100).map(|i| vec![Value::Int(i)]).collect(),
        };
        g.set_result(rs);
        g.set_more(true);
        g.page_rows = 0;
        g.bounds = Rect::new(0, 0, 400, 100);
        g.row_h = 20;
        for _ in 0..200 {
            g.on_event(&InputEvent::Wheel { delta: -120 }, 1.0);
        }
        assert_eq!(g.fetch_req, None, "그리드 · 0행이면 자동 페치 없음");
        assert!(
            g.fetch_tools_enabled().0,
            "⇊ 전체 조회(이어 받기)는 더 있으면 활성"
        );
        // 1 이상이면 나간다(기존 동작 유지)
        g.page_rows = 200;
        g.scroll_y = 0;
        for _ in 0..200 {
            g.on_event(&InputEvent::Wheel { delta: -120 }, 1.0);
        }
        assert!(matches!(
            g.fetch_req,
            Some(FetchReq::Next { limit: 200, .. })
        ));
    }

    #[test]
    fn group_digits_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(155312), "155,312");
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
