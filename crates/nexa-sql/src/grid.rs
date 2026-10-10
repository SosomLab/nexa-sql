//! 결과 그리드 — 최소 가상화(보이는 행만 그린다) · **픽셀 단위 스크롤**(사용자 09-14) · 오버레이 스크롤바(필요할 때만 ·
//! 호버 두껍게 · 자동 숨김 — nexa-ctl `ScrollBars` 공용) · 컬럼 폭은 앞 200행 실측 · 메시지 모드.
//! `nexa-grid` 크레이트(U-3 · nexa-ui 21)가 오면 교체한다. 고정폭 층에서 그려진다.

use crate::editable::{self, Policy, Tier};
use crate::gridedit_sql::{
    self, BlobMap, ColMeta, Concurrency, EditTarget, GenInput, HiddenCol, KeyKind, ReadOnly,
};
use crate::toolicons;
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::gridedit::{
    self, CellKind, CellSpec, ChangeSet, EditAction, EditStart, LiveEditor, LiveEvent, Move,
    PasteAnchor, PasteOpts, RowRef, RowStatus,
};
use nexa_ctl::theme::Theme;
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::{
    Control, InputEvent, Invalidations, Key, ScrollBars, TextBox, ToolIcon, ToolItem, Toolbar,
    Widget,
};
use nsql_core::{fmt_bytes, Dialect, ResultData, ResultSet, RowSource, Value, View};
use nsql_i18n::{t, tf, Msg};
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

/// ★ 그리드 편집 설정(호스트가 설정 레지스트리에서 주입 · docs/87 §8).
#[derive(Clone, Debug)]
pub(crate) struct EditCfg {
    pub on: bool,
    pub empty_as_null: bool,
    pub paste_max: usize,
    /// 키 열이 결과에 없으면 숨은 키 열을 주입해 재조회(D-214 · `grid.edit_hidden_keys`).
    pub hidden_keys: bool,
    /// 키가 없으면 물리 행 식별자(ROWID 등)를 주입해 재조회(`grid.edit_rowid`).
    pub rowid: bool,
    /// 마지막 폴백 = 비교 가능한 전 열(D-216 · `grid.edit_all_cols`).
    pub all_cols: bool,
    /// 낙관적 동시성(`grid.edit_concurrency`).
    pub concurrency: Concurrency,
}

impl Default for EditCfg {
    fn default() -> Self {
        EditCfg {
            on: true,
            empty_as_null: true,
            paste_max: 10_000,
            hidden_keys: false, // 09-27 D-225
            rowid: false,       // 09-27 D-225
            all_cols: true,
            concurrency: Concurrency::Key,
        }
    }
}

/// 그리드가 호스트에 부탁하는 편집 관련 일(1회성 · `take_edit_requests`).
pub(crate) enum EditRequest {
    /// 테이블 키(PK/UK) 조회(키 캐시 → 없으면 `Cmd::Keys`) — 답은 `set_keys`.
    NeedKeys {
        table: String,
    },
    /// 변경 적용(바인드 문장 묶음 + 사전 검사문 · 한 트랜잭션) — 답은 `apply_done_rows`(행 단위 재조회) 또는 `apply_done`.
    Apply {
        table: String,
        stmts: Vec<nsql_run::ApplyStmt>,
        preview: String,
        /// 적용 성공 뒤 같은 세션에서 돌릴 행 단위 재조회(수정·추가 행마다 하나 · 비어 있으면 전체 재조회 · 87 §12-4).
        refetch: Vec<nsql_core::ExecRequest>,
    },
    /// 읽기 전용 미리보기 창(SQL 미리보기).
    Preview {
        title: String,
        text: String,
    },
    /// ★ 값 보기 창(87 §5 · LOB): 글/이진 · 편집 가능 여부 — 호스트가 값 창(`sqlprev_win` 값 모드)을 연다.
    ViewCell(ValueReq),
    /// 셀 편집기 우클릭 메뉴의 붙여넣기 — 호스트가 클립보드를 읽어 `live_paste`.
    ClipboardPaste,
    /// ★ 숨은 열 주입 재조회(87 §13 · T-231) — 호스트는 출처 문장을 이것으로 바꿔 다시 실행한다(실패 = `requery_failed`).
    Requery {
        sql: String,
    },
    Status(String),
}

/// 값 보기 창에 넘기는 셀 하나(87 §5).
#[derive(Clone, Debug)]
pub(crate) struct ValueReq {
    pub row: RowRef,
    pub col: usize,
    /// 열 이름(창 제목 · 저장 파일 이름).
    pub name: String,
    /// 글 값(이진이면 빈 문자열).
    pub text: String,
    /// 이진 값(원본 또는 파일에서 넣은 것).
    pub bytes: Option<Vec<u8>>,
    /// 편집 가능(편집 상태 · 읽기 전용/숨은 열 아님 · 적용 중 아님).
    pub editable: bool,
    /// 이진 열(파일에서 넣기 = 늘 바이트).
    pub binary: bool,
}

/// 주입 재조회 계획 — 결과가 이 문장으로 돌아오면 뒤쪽 `hidden.len()` 열은 숨은 열.
#[derive(Clone, Debug)]
struct InjectPlan {
    /// 사용자의 원래 문장(편집 판정·복귀용).
    orig: String,
    /// 주입한 문장(= 새 출처 문장).
    sql: String,
    hidden: Vec<HiddenCol>,
}

/// 편집 세션 상태(결과 하나에 하나 · 결과가 바뀌면 버린다).
struct GridEdit {
    target: EditTarget,
    /// 원본 열마다 이름 + 명세(`set_col_specs`로 메타에서 보강).
    cols: Vec<ColMeta>,
    key_cols: Vec<usize>,
    key_kind: KeyKind,
    keys_ready: bool,
    /// 카탈로그 키(재조회 실패 뒤 다시 판정할 때).
    keys: Option<nsql_core::KeyInfo>,
    /// 뒤쪽 숨은 열 수(화면·복사에서 제외 · `col_order`에 없다).
    hidden: usize,
    cs: ChangeSet,
    live: LiveEditor,
    /// 편집 중인 셀의 표시 좌표(행·열 위치) — 페인트가 상자 자리를 맞춘다.
    live_at: Option<(usize, usize)>,
    last_click: Option<((usize, usize), std::time::Instant)>,
    applying: bool,
    /// 적용에 실패한 문장의 행(붉은 표시).
    error_row: Option<RowRef>,
    /// 보낸 문장의 행(실패 index → 행).
    sent_rows: Vec<RowRef>,
    /// 행 단위 재조회를 부탁한 행(요청 순서 · `apply_done_rows`가 결과를 맞춘다).
    sent_refetch: Vec<RowRef>,
    /// 수정·추가 행 전부의 재조회 문장을 만들 수 있었다(false = 전체 재조회 필요).
    sent_refetch_ok: bool,
    /// 마지막 제자리 갱신(수정·추가·삭제 행 수 · 덤프용).
    last_patch: Option<(usize, usize, usize)>,
    /// 파일에서 넣은 이진 값(셀 → (라벨, 바이트) · 87 §5).
    blobs: BlobMap,
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
    /// ★ 고속 스크롤(설정 `scroll.*` · 사용자 09-30): 휠은 `bars`(nexa-ctl 전역 설정 + 결과 그리드 override) · ↑/↓ 키 자동 반복은
    ///   이 가속기 — 그리드·텍스트 등 모든 보기 모드(같은 `bars`·같은 키 길) · HUD도 `bars`가 그린다.
    key_accel: nexa_ctl::ScrollAccel,
    /// 결과 도구줄 보기 모드 ▾ 메뉴가 열려 있는가(다른 우클릭 메뉴와 구별 · 사용자 09-30 "열린 상태에서 다시 누르면 닫힘").
    menu_is_view: bool,
    /// 보기 모드 버튼 위 MouseDown이 열린 메뉴를 닫았다 → 이어지는 클릭은 다시 열지 않는다(토글).
    view_toggle_off: bool,
    /// 설정 `grid.row_numbers`(기본 켬) — 왼쪽 고정 행번호 열(가로 스크롤 무관).
    row_numbers: bool,
    /// 설정 `grid.null_text` — NULL 셀 글자(그리드·텍스트 보기·복사 **공통** · 기본 `NULL` · 사용자 09-17).
    null_text: String,
    /// 설정 `grid.filter_list_max` — 정규식 필터 조회 SQL ③단계 값 목록 상한(기본 1,000 · 사용자 09-30).
    filter_list_max: usize,
    /// 설정 `grid.filter_strip` — 필터가 있을 때 그리드 위에 칩 한 줄(77 §2-2 · T-181).
    filter_strip: bool,
    /// 설정 `grid.filter_pick_max` — 필터 ▸ 값 고르기 하위 메뉴의 고유값 수(기본 30).
    filter_pick_max: usize,
    /// 필터 줄 높이(없으면 0) — `bounds`는 이만큼 아래에서 시작한다(줄은 `bounds` 바로 위).
    strip_h: i32,
    /// 그 줄의 구성: 조건 바 높이(위) · 필터 칩 줄 높이(아래 · 설정 `grid.filter_strip`이 켜지고 필터가 있을 때만 · 10-06).
    cond_h: i32,
    chip_h: i32,
    /// 조건 바를 호스트가 **편집기 글꼴 컨텍스트**로 따로 그린다(`paint_cond` · 사용자 10-07 "편집기와 줄 간격·글꼴 일치") —
    /// 참이면 `paint_filter_strip`은 조건 바를 건너뛴다(시험·헤드리스 = 거짓 → 그리드가 그린다).
    cond_host_paint: bool,
    /// 필터 줄의 칩(술어의 열 · 칩 사각형 · × 사각형) — 그릴 때 채운다 · 맞히기는 이 사각형으로.
    chips: Vec<(usize, Rect, Rect)>,
    /// 오른쪽 끝 "모두 지우기" ×.
    chips_clear: Option<Rect>,
    /// 커서가 올라간 ×(칩 번호 · `usize::MAX` = 모두 지우기).
    chip_hover: Option<usize>,
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
    /// ★ 필터 술어 AND 목록(T-181) — `apply_sort` 뒤 `row_order`를 거른다.
    filters: Vec<Predicate>,
    /// 우클릭한 셀 (원본 열, 글 · NULL = None) — 필터 메뉴의 대상.
    menu_cell: Option<(usize, Option<String>)>,
    /// "포함…" 입력 요청(열) — 호스트가 팔레트로 받아 `add_filter`.
    pending_filter_prompt: Option<(usize, FilterOp, String)>,
    /// 열 머리 메뉴 "객체 탐색기에서 보기"(T-180 ⑤) — 고른 열의 이름(호스트가 출처 테이블과 묶어 찾는다 · 1회성).
    pending_reveal: Option<String>,
    /// 외래 키 열 이름(대문자 · 호스트가 메타에서 채움 · T-180 ⑥) — 셀 메뉴 "참조 행 보기" 활성 판정.
    fk_cols: Vec<String>,
    /// 우클릭한 셀의 **원본** 행 번호(필터·정렬 뒤 투영이 아니라 `row_order`를 거친 값).
    menu_row: Option<usize>,
    /// 셀 메뉴 "참조 행 보기"가 남긴 (원본 행, 열 이름) — 호스트가 부모 행을 조회한다(1회성).
    pending_follow: Option<(usize, String)>,
    /// 필터 메뉴 "필터로 서버 재조회"가 남긴 조회용 Query(77 §2-2 · T-181) — 호스트가 새 결과 탭으로 실행(1회성).
    pending_requery: Option<String>,
    /// ★ 필터 중 자동 페치(T-285 · D-257/258 · 103 §4): 서버 승격 방식 · 채움 상한(페이지) · 남은 채움 수 · 지금 출처가 승격 결과인가.
    filter_server: FilterServerMode,
    fill_pages: usize,
    fill_pages_left: usize,
    filter_promoted: bool,
    /// `ask` 안내를 이 결과에서 한 번 했는가.
    filter_asked: bool,
    /// ★ 술어 결합(T-181): 거짓 = AND(기본 · 전부 맞아야) · 참 = OR(하나만 맞아도) — 투영·조회용 Query·칩 줄이 같이 본다.
    filter_or: bool,
    /// 필터 오류(정규식 컴파일 실패 등 · 1회성).
    pending_error: Option<String>,
    /// ★ 헤더 빗금 표식 위 hover(열, 시작 ms) — 머물면 적용된 술어 툴팁(사용자 09-29 "표식 위에 올렸을 때만").
    mark_hover: Option<(usize, u64)>,
    /// 마지막 그리기에서 기록한 표식 사각형(열 원본 번호, 화면 좌표).
    mark_rects: Vec<(usize, Rect)>,
    /// ★ 열 머리 깔때기 아이콘 사각형(열, rect) — 클릭 = 값 목록 팝업(T-181 후속 · 10-06 · 설정 `grid.filter_funnel`).
    funnel_rects: Vec<(usize, Rect)>,
    /// 깔때기 표시 방법(설정 `grid.filter_funnel` = always · hover · none).
    funnel_mode: FunnelMode,
    /// 결과 필터 사용 여부(설정 `grid.filter_enabled` · 끄면 메뉴·깔때기·값 목록 없음).
    filter_enabled: bool,
    /// 마우스가 올라간 열 머리(원본 index · hover 모드의 깔때기 자리).
    funnel_hover: Option<usize>,
    /// ★ 마우스 아래 열 머리(원본 index · 머리 칸 사각형) — 호스트의 열 머리 hover 카드(사용자 10-07 · 편집기 링크 카드와 같은 카드).
    hdr_hover: Option<(usize, Rect)>,
    /// 호스트가 가져가는 "다시 그려 달라" 깃발(헤더 hover 변화처럼 사건이 없이 그림만 바뀔 때).
    dirty: bool,
    /// 값 목록 팝업의 표시 행 수(설정 `grid.filter_popup_rows` · 고정).
    filter_popup_rows: usize,
    /// 값 목록 범위(설정 `grid.filter_values_scope`): true = **다른 열의 필터를 통과한 행**의 값만(종속 목록 · 기본) · false = 받은 행 전체.
    values_scope_others: bool,
    /// 결과 그리드만의 고속 스크롤 설정(`scroll.fast_grid_extra` · None = 전역) — 새 결과 탭 그리드(`fresh_like`)에도 물려준다
    /// (사용자 10-06 "그리드도 ×16에서 제한" = 첫 그리드에만 걸려 있었다).
    fast_override: Option<nexa_ctl::FastScroll>,
    /// 값 목록 팝업(검색 상자 + 체크 목록 · 엑셀 자동 필터 꼴).
    vpick: crate::valuepick::ValuePick,
    /// ★ 인라인 조건 입력란(DBeaver 조건 바 · 사용자 10-06) — 필터 줄 자리 · 설정 `grid.condition_bar`.
    cond: crate::condbar::CondBar,
    cond_on: bool,
    /// 조건으로 감싸기 전의 **원래 출처 문장**(조건을 바꿔도 다시 이것을 감싼다) · 우리가 낸 감싼 문장(새 실행과 구별).
    cond_base: Option<String>,
    cond_last_sql: Option<String>,
    /// 조건 실행을 보내기 직전의 출처 문장(실패하면 되돌린다 · 결과는 그대로 둔다 · 사용자 10-06).
    cond_prev_sql: Option<String>,
    /// 호스트가 가져가는 조건 실행 요청(감싼 SQL).
    pending_cond_run: Option<String>,
    /// 그리드 안 글 상자(조건 바·값 목록 검색)의 우클릭 편집 메뉴 동작(호스트가 `clip_action`으로).
    pending_edit_ctx: Option<nexa_ctl::EditCtxAction>,
    /// ★ 추가 행의 표시 순서(추가 행 번호 k · 사용자 09-29 "추가 행에서 +도 그 바로 아래") — 같은 기존 행 아래의 추가 행끼리의 순서.
    ins_order: Vec<usize>,
    now_ms: u64,
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
    /// 편집 툴바를 마지막으로 맞출 때의 "선택 있음" — 선택이 생기거나 사라지면 그리는 시점에 다시 맞춘다(사용자 09-26 복제 버튼 비활성).
    tools_has_sel: bool,
    /// 드래그 중(셀 범위 / 행번호 열 = 행 범위).
    drag_sel: Option<DragSel>,
    /// 우클릭 메뉴(복사 형식 · 전체 선택).
    menu: CtxMenu,
    /// ★ 우클릭 메뉴 배치 영역(창 전체 · 호스트가 배치 때 넣음 · 0 = 그리드 영역) — 메뉴가 그리드 밖으로 펼쳐지되 창은 안 넘는다(사용자 09-29).
    menu_area: Rect,
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
    /// 조건 바 열 이름 인용 정책(설정 `editor.quote_idents` · true = 항상).
    cond_quote_always: bool,
    /// ★ 열 머리 유형 표시(설정 `grid.col_type_icons` · 성능 향상 모드 = 끔 · 사용자 10-07) — T-301 = 2글자 배지.
    type_icons: bool,
    /// 열별 배지 캐시(글자 · 색조) — 결과가 바뀌면 비운다(그리기마다 표본을 다시 보지 않게).
    type_badges: Vec<(&'static str, BadgeTone)>,
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
    /// ★ 자동 페치 재발 방지(사용자 09-29 · 필터 뒤 재질의 반복): 마지막 자동 요청 때의 **원본** 행 수 — 그 뒤 원본이 늘지 않았으면
    ///   같은 오프셋으로 다시 요청하지 않는다(서버가 같은 페이지를 되돌리거나 필터가 전부 거를 때의 무한 루프 차단).
    auto_fetch_at: Option<usize>,
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
    /// ★ 증분 표시 예약(사용자 10-09): 첫 세그먼트가 왔지만 세션이 아직 busy(Done 전)라 바로 못 청한다 → Done 뒤 호스트가 거둔다
    ///   (협업 bin119 = 결과 도착 시점의 `request_fetch_all`이 `session_blocked`에 막혀 버려졌다).
    auto_fetch_all: bool,
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
    // ── ★ 데이터 편집(docs/87 · T-182)
    edit_cfg: EditCfg,
    edit: Option<GridEdit>,
    /// 편집 불가 이유(결과가 있을 때 · 없으면 편집 가능 또는 결과 없음).
    read_only: Option<ReadOnly>,
    edit_reqs: Vec<EditRequest>,
    /// 숨은 열 주입 재조회 계획(출처 문장이 이 계획의 문장이면 살아 있다).
    inject: Option<InjectPlan>,
    /// 주입을 시도했다가 실패한 원문(같은 문장에 다시 시도하지 않는다).
    inject_tried: Option<String>,
    /// ★ 주입 재조회가 **나가 있다**(요청을 밀었고 결과·실패·중단이 아직 안 왔다 · 09-28) — 실행 오류를 "재조회 실패"로 볼지의 기준.
    ///   (종전 판정 `inject.is_some() && !keys_ready`는 이미 3급 판정이 끝난 뒤의 **수동** 재조회를 놓쳤다.)
    inject_sent: bool,
    /// 운영(PROD) 접속 — 편집 불가.
    prod: bool,
    /// 호스트가 알려 주는 Shift 상태(Tab 방향 · `Char('\t')`에는 수식키가 없다).
    shift: bool,
    /// 호스트가 알려 주는 키보드 포커스(그리드 = 활성 선택색 · 다른 곳 = 비활성 선택색 · 88 §3 2 · T-232).
    focused: bool,
    /// 마지막 on_event/paint 배율(편집 상자 배율).
    live_scale: f32,
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
            key_accel: nexa_ctl::ScrollAccel::new(),
            menu_is_view: false,
            view_toggle_off: false,
            row_numbers: true,
            row_focus: true,
            top_border: true,
            row_focus_color: (None, None),
            null_text: "NULL".into(),
            filter_list_max: 1000,
            filter_strip: true,
            filter_pick_max: 30,
            strip_h: 0,
            cond_h: 0,
            chip_h: 0,
            cond_host_paint: false,
            chips: Vec::new(),
            chips_clear: None,
            chip_hover: None,
            row_pct: 150,
            gutter_w: 0,
            col_order: Vec::new(),
            row_order: Vec::new(),
            sort_keys: Vec::new(),
            filters: Vec::new(),
            menu_cell: None,
            pending_filter_prompt: None,
            pending_reveal: None,
            fk_cols: Vec::new(),
            menu_row: None,
            pending_follow: None,
            pending_requery: None,
            filter_server: FilterServerMode::Auto,
            fill_pages: 3,
            fill_pages_left: 3,
            filter_promoted: false,
            filter_asked: false,
            filter_or: false,
            pending_error: None,
            mark_hover: None,
            mark_rects: Vec::new(),
            funnel_rects: Vec::new(),
            funnel_mode: FunnelMode::Always,
            filter_enabled: true,
            funnel_hover: None,
            hdr_hover: None,
            dirty: false,
            filter_popup_rows: 12,
            values_scope_others: true,
            fast_override: None,
            vpick: crate::valuepick::ValuePick::default(),
            cond: crate::condbar::CondBar::new(),
            cond_on: true,
            cond_base: None,
            cond_last_sql: None,
            cond_prev_sql: None,
            pending_cond_run: None,
            pending_edit_ctx: None,
            ins_order: Vec::new(),
            now_ms: 0,
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
            tools_has_sel: false,
            drag_sel: None,
            menu: CtxMenu::new(),
            menu_area: Rect::default(),
            pending_copy: None,
            pending_sql: None,
            sc_copy: String::new(),
            sc_all: String::new(),
            menu_click_consumed: false,
            dialect: Dialect::Oracle,
            cond_quote_always: false,
            type_icons: false,
            type_badges: Vec::new(),
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
                // 09-27 D-225: 행 식별 열 가져오기(재조회 1회) — 재조회는 기본 안 하므로 사용자가 이 결과에 한해 켠다.
                ToolItem::new("row.identify", ToolIcon::Glyph("⚿".into()))
                    .tip(t(Msg::TipRowIdentify))
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
            auto_fetch_at: None,
            footer_h: 0,
            more: false,
            total: None,
            fetching: false,
            session_blocked: false,
            session_connected: true,
            fetch_all_pending: false,
            auto_fetch_all: false,
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
            edit_cfg: EditCfg::default(),
            edit: None,
            read_only: None,
            edit_reqs: Vec::new(),
            inject: None,
            inject_tried: None,
            inject_sent: false,
            prod: false,
            shift: false,
            focused: true,
            live_scale: 1.0,
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
            // ★ 글자 값 = 자연 정렬 부품(nexa-ctl `natural::cmp_names` · 전역 `ui.sort_natural` · 사용자 10-09): 켜짐 = 숫자 구간은 수로
            //   (`ITEM2 < ITEM10`) · 꺼짐 = 대소문자 무시 글자 순(종전).
            _ => nexa_ctl::natural::cmp_names(&a.display(), &b.display()),
        },
    }
}

impl Grid {
    /// 결과·선택·스크롤은 비우고 **설정만**(영역 · 행번호 · 스크롤 단위 · 단축키 문구 · 방언) 물려받은 새 그리드 — 새 편집기 탭의 짝(사용자 09-16).
    pub(crate) fn fresh_like(&self) -> Grid {
        let mut g = Grid {
            bounds: self.bounds,
            row_snap: self.row_snap,
            row_numbers: self.row_numbers,
            row_focus: self.row_focus,
            top_border: self.top_border,
            row_focus_color: self.row_focus_color,
            null_text: self.null_text.clone(),
            filter_list_max: self.filter_list_max,
            filter_strip: self.filter_strip,
            filter_pick_max: self.filter_pick_max,
            funnel_mode: self.funnel_mode,
            filter_enabled: self.filter_enabled,
            filter_popup_rows: self.filter_popup_rows,
            values_scope_others: self.values_scope_others,
            cond_on: self.cond_on,
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
            edit_cfg: self.edit_cfg.clone(),
            prod: self.prod,
            // 10-07 bin47 결함: 새 설정은 여기도 함께(안 그러면 새 탭 그리드만 기본값) — 열 이름 인용 · 유형 아이콘.
            cond_quote_always: self.cond_quote_always,
            type_icons: self.type_icons,
            filter_server: self.filter_server,
            fill_pages: self.fill_pages,
            fill_pages_left: self.fill_pages,
            ..Grid::default()
        };
        g.set_fast_override(self.fast_override);
        // 조건 바 설정(펼침 줄 수 · DnD 틀 · 완성 기준)도 물려준다 — 종전엔 기본값과 같아 티가 안 났다.
        g.cond.copy_cfg_from(&self.cond);
        g
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
        let (d, t) = self.mem_parts();
        d + t
    }

    /// (결과 데이터, 텍스트 보기 캐시) 바이트 — 메모리 맵 카테고리 보고(docs/80).
    /// ★ 전체 조회 **진행 중**에는 받은 행이 워커 버퍼에 쌓이고 `rs`에는 끝나야 붙는다 → 진행 수치(`fetch_progress` = 받은 바이트 +
    ///   기존)를 데이터로 친다(사용자 09-24 "Fetch 중 실행 카드는 늘어나는데 메모리 사용량은 그대로").
    pub(crate) fn mem_parts(&self) -> (u64, u64) {
        let held = self.rs.as_ref().map_or(0, ResultData::approx_bytes);
        let in_flight = self
            .fetch_progress
            .map_or(0, |(_, b)| b.saturating_sub(held));
        (held + in_flight, self.text_bytes)
    }

    pub(crate) fn page_rows(&self) -> usize {
        self.page_rows
    }

    /// 이 탭의 세그먼트 크기(푸터 입력란 Enter와 같은 길 · 기동 명령 `grid.page:<n>`).
    pub(crate) fn set_page_rows(&mut self, n: usize) {
        self.page_rows = n;
        self.page_box.set_text(&n.to_string());
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
    pub(crate) fn request_fetch_all(&mut self) {
        if self.session_blocked {
            return;
        }
        self.request_fetch_all_force();
    }

    /// 세션 가드 없이 전체 조회 요청(증분 자동 이어 받기 · 호스트가 Done 뒤 세션이 빈 것을 확인하고 부른다 · 문지기는 제출 때 다시 본다).
    pub(crate) fn request_fetch_all_force(&mut self) {
        if self.fetch_all_pending || self.rs.is_none() || !self.more {
            return;
        }
        self.fetch_req = Some(FetchReq::All);
        self.fetch_all_pending = true;
        self.fetch_progress = None;
        self.cancel_req = false;
        self.sync_fetch_tools();
    }

    /// 증분 표시 예약(결과 도착 때 · Done 뒤 [`Self::take_auto_fetch_all`]).
    pub(crate) fn arm_auto_fetch_all(&mut self) {
        self.auto_fetch_all = true;
    }

    pub(crate) fn take_auto_fetch_all(&mut self) -> bool {
        std::mem::take(&mut self.auto_fetch_all)
    }

    /// 자체 시험 훅 `grid.sort:<열>`(열 머리 클릭과 같은 토글).
    pub(crate) fn sort_by_col_cmd(&mut self, col: usize) {
        self.toggle_sort(col, false);
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
        // 우리가 낸 감싼 문장이 아니면 = 새 실행 → 조건 바와 바탕 문장 초기화.
        // ★ 비교는 정규화해서(양끝 공백 · 끝 `;`): 분할기가 다듬은 `stmt`와 우리가 낸 글(승격 SQL은 끝에 개행)이 글자 그대로는 달라
        //   승격 상태가 초기화되던 결함(협업 bin49/50 a2 · `cond.dump last=` 빈 값).
        self.cond_prev_sql = None;
        let same = self
            .cond_last_sql
            .as_deref()
            .is_some_and(|l| same_stmt(l, stmt));
        if !same {
            self.cond_base = None;
            self.cond_last_sql = None;
            if self.filter_promoted && !self.filters.is_empty() {
                // 승격 중 다른 새 실행 = 필터도 새 조회 규칙대로 비움.
                self.filters.clear();
                self.apply_sort();
            }
            self.filter_promoted = false;
            self.filter_asked = false;
            self.cond.set_text("");
        }
        self.set_source_sql(stmt);
        self.countable = is_query;
        self.sync_fetch_tools();
        self.edit_prepare(stmt, is_query);
    }

    // ───────────────────────── ★ 데이터 편집(docs/87 · T-182) ─────────────────────────

    /// 결과 출처가 정해질 때 편집 가능 판정 → 편집 상태 준비(키는 호스트에 부탁).
    fn edit_prepare(&mut self, stmt: &str, is_query: bool) {
        self.edit = None;
        self.read_only = None;
        // 결과가 왔다 = 나가 있던 재조회는 끝났다(주입 결과든 원문 재실행이든).
        self.inject_sent = false;
        // 주입 계획: 결과가 우리 재조회 문장이면 판정은 원문으로 · 뒤쪽 열은 숨은 열. 다른 문장이 오면 계획·시도 기록을 버린다.
        let plan = match self.inject.as_ref() {
            Some(p) if gridedit_sql::same_stmt(&p.sql, stmt) => Some(p.clone()),
            _ => {
                self.inject = None;
                if self
                    .inject_tried
                    .as_deref()
                    .is_some_and(|o| !gridedit_sql::same_stmt(o, stmt))
                {
                    self.inject_tried = None;
                }
                None
            }
        };
        let analyze_src = plan.as_ref().map_or(stmt, |p| p.orig.as_str());
        let Some(rs) = self.rs.as_ref() else { return };
        if !self.edit_cfg.on {
            self.read_only = Some(ReadOnly::Disabled);
        } else if self.prod {
            self.read_only = Some(ReadOnly::Prod);
        } else if !is_query {
            self.read_only = Some(ReadOnly::NotQuery);
        } else {
            match gridedit_sql::analyze(analyze_src) {
                Ok(target) => {
                    let mut cols: Vec<ColMeta> = rs
                        .columns()
                        .iter()
                        .map(|c| {
                            let mut kind = CellKind::from_type_name(&c.type_name);
                            if self.dialect == Dialect::Oracle && kind == CellKind::Date {
                                kind = CellKind::DateTime; // Oracle DATE = 시각 포함
                            }
                            let mut spec = CellSpec::new(c.name.clone(), kind);
                            spec.max_len = CellSpec::max_len_from_type_name(&c.type_name);
                            ColMeta::new(c.name.clone(), spec, c.type_name.clone())
                        })
                        .collect();
                    // 숨은 열 = 뒤쪽 n개(주입 순서 그대로) — 화면 열 순서에서 뺀다.
                    let mut hidden = 0;
                    if let Some(p) = plan.as_ref() {
                        let n = p.hidden.len();
                        if cols.len() > n {
                            let first = cols.len() - n;
                            for (i, h) in p.hidden.iter().enumerate() {
                                cols[first + i].mark_hidden(h);
                            }
                            hidden = n;
                            self.col_order.retain(|&c| c < first);
                        }
                    }
                    let mut live = LiveEditor::new();
                    live.empty_as_null = self.edit_cfg.empty_as_null;
                    let table = target.table.clone();
                    self.edit = Some(GridEdit {
                        target,
                        cs: ChangeSet::new(cols.len()),
                        cols,
                        key_cols: Vec::new(),
                        key_kind: KeyKind::AllColumns,
                        keys_ready: false,
                        keys: None,
                        hidden,
                        live,
                        live_at: None,
                        last_click: None,
                        applying: false,
                        error_row: None,
                        sent_rows: Vec::new(),
                        sent_refetch: Vec::new(),
                        sent_refetch_ok: false,
                        last_patch: None,
                        blobs: BlobMap::new(),
                    });
                    self.edit_reqs.push(EditRequest::NeedKeys { table });
                }
                Err(r) => self.read_only = Some(r),
            }
        }
        self.sync_edit_tools();
    }

    /// 호스트 주입: 편집 설정.
    pub(crate) fn set_edit_cfg(&mut self, cfg: EditCfg) {
        if let Some(e) = self.edit.as_mut() {
            e.live.empty_as_null = cfg.empty_as_null;
        }
        self.edit_cfg = cfg;
    }

    /// 호스트 주입: 운영 접속 여부(편집 불가).
    pub(crate) fn set_prod(&mut self, on: bool) {
        self.prod = on;
    }

    /// 호스트 주입: 테이블 키(`Cmd::Keys` 답 · 41 규칙) → 결과에 **모든 키 열이 있는** PK → 첫 유니크 → 전체 열(D-198).
    pub(crate) fn set_keys(&mut self, info: Option<&nsql_core::KeyInfo>) {
        let Some(e) = self.edit.as_mut() else { return };
        e.keys = info.cloned();
        let injected = self.inject.is_some() || self.inject_tried.is_some();
        self.classify_apply(injected);
    }

    /// ★ 행 식별 등급 판정 → 행동(87 §13 · T-231): 1급/2급/3급 = 키 열 확정 · 주입 필요 = 재조회 요청 · 불가 = 읽기 전용.
    fn classify_apply(&mut self, injected: bool) {
        let policy = Policy {
            hidden_keys: self.edit_cfg.hidden_keys,
            rowid: self.edit_cfg.rowid,
            all_cols: self.edit_cfg.all_cols,
        };
        self.classify_apply_with(injected, policy);
    }

    /// ★ 온디맨드 행 식별 재조회(09-27 사용자 결정 D-225): 기본 정책이 재조회를 막았어도 **이 결과에 한해** 1급-보완/2급
    /// 재조회를 한 번 시도한다(우클릭 ▸ 행 식별 열 가져오기 · 자체 시험 `grid.edit.cmd:grid.edit.identify`). 이미 시도했으면 false.
    pub(crate) fn edit_identify(&mut self) -> bool {
        if self.edit.is_none() || self.inject.is_some() {
            return false;
        }
        // 🔧 수동 시도는 앞선 실패 표식을 지운다(사용자 09-28 "SELECT해도 버튼이 안 켜짐" — 자동 정책의 재주입 방지 표식이
        //   사용자의 버튼까지 영영 껐다): 실패하면 다시 표식이 붙고, 다음 누름에서 또 시도한다(사용자 행동 1회당 1번).
        self.inject_tried = None;
        let before = self.edit_reqs.len();
        self.classify_apply_with(false, Policy::eager());
        self.edit_reqs.len() > before
    }

    /// 재조회 없이 3급으로 판정됐지만 올릴 길(카탈로그 키 · 물리 식별자)이 있는가 — 상태줄 힌트·메뉴 활성의 기준.
    pub(crate) fn can_identify(&self) -> bool {
        let Some(e) = self.edit.as_ref() else {
            return false;
        };
        // (`inject_tried`는 보지 않는다 — 자동 재주입 방지 표식일 뿐, 사용자의 수동 시도는 늘 열려 있다 · 09-28.)
        e.keys_ready
            && e.key_kind == KeyKind::AllColumns
            && self.inject.is_none()
            && !e.applying
            && (e.keys.is_some() || !gridedit_sql::physical_cols(self.dialect).is_empty())
    }

    fn classify_apply_with(&mut self, injected: bool, policy: Policy) {
        let Some(e) = self.edit.as_mut() else { return };
        let tier = editable::classify(self.dialect, &e.cols, e.keys.as_ref(), &policy, injected);
        // 3급인데 정책이 재조회를 막은 것이면 상태줄로 알린다(09-27 D-225 · 사용자가 판단해 우클릭 ▸ 행 식별 열 가져오기).
        let hint = matches!(tier, Tier::AllColumns(_)) && (!policy.hidden_keys || !policy.rowid);
        match tier {
            Tier::Constraint(v) => {
                e.key_cols = v;
                e.key_kind = KeyKind::Constraint;
                e.keys_ready = true;
            }
            Tier::Physical(v) => {
                e.key_cols = v;
                e.key_kind = KeyKind::Physical;
                e.keys_ready = true;
            }
            Tier::AllColumns(v) => {
                e.key_cols = v;
                e.key_kind = KeyKind::AllColumns;
                e.keys_ready = true;
            }
            Tier::NeedHiddenKeys(names) => {
                let hidden: Vec<HiddenCol> = names.into_iter().map(HiddenCol::Key).collect();
                self.begin_inject(hidden);
                return;
            }
            Tier::NeedPhysical => {
                let hidden = gridedit_sql::physical_cols(self.dialect);
                self.begin_inject(hidden);
                return;
            }
            Tier::None => {
                self.edit = None;
                self.read_only = Some(ReadOnly::NoKey);
                let s = tf(Msg::StGeReadOnly, &[&ReadOnly::NoKey.text()]);
                self.status(s);
            }
        }
        // 키 열은 읽기 전용이 아니다(값을 고치면 WHERE는 원본 값으로) · 이진 열은 인라인 편집 없음.
        if hint && self.can_identify() {
            let s = t(Msg::StGeIdentityHint).to_string();
            self.status(s);
        }
        self.sync_edit_tools();
    }

    /// 숨은 열 주입 재조회 시작 — 문장을 만들 수 없으면(인용 별칭 · 구조 못 찾음) 주입 없이 다시 판정(3급/읽기 전용).
    fn begin_inject(&mut self, hidden: Vec<HiddenCol>) {
        let Some(e) = self.edit.as_ref() else { return };
        let orig = self
            .inject
            .as_ref()
            .map_or_else(|| self.source_sql.clone(), |p| p.orig.clone());
        match gridedit_sql::inject(&orig, &e.target, self.dialect, &hidden) {
            Some(sql) => {
                self.inject = Some(InjectPlan {
                    orig,
                    sql: sql.clone(),
                    hidden,
                });
                self.inject_sent = true;
                self.edit_reqs.push(EditRequest::Requery { sql });
                let s = t(Msg::StGeRequery).to_string();
                self.status(s);
                self.sync_edit_tools();
            }
            None => {
                self.inject_tried = Some(orig);
                self.classify_apply(true);
            }
        }
    }

    /// 호스트: 주입 재조회가 실패했다(오류 · 게이트 닫힘) → 원문으로 되돌리고 주입 없이 다시 판정.
    pub(crate) fn requery_failed(&mut self) {
        self.inject_sent = false;
        let Some(p) = self.inject.take() else { return };
        self.source_table = guess_table(&p.orig);
        self.source_sql = p.orig.clone();
        self.inject_tried = Some(p.orig);
        let s = t(Msg::StGeRequeryFailed).to_string();
        self.status(s);
        if self.edit.as_ref().is_some_and(|e| !e.keys_ready) {
            self.classify_apply(true);
        }
    }

    /// 주입 재조회가 나가 있는가(호스트 오류 분기·시험) — 이 값이 참일 때만 실행 오류를 "재조회 실패"로 본다(09-28).
    pub(crate) fn inject_pending(&self) -> bool {
        self.inject_sent
    }

    /// 호스트: 주입 재조회를 **보내지 못했다**(세션 바쁨 · 게이트 닫힘) → 원문으로 되돌리되 실패 표식은 남기지 않는다(다시 누르면
    /// 다시 시도 · 09-28). 판정은 그대로(3급 · 키 준비됨)라 버튼이 다시 켜진다.
    pub(crate) fn requery_aborted(&mut self) {
        self.inject_sent = false;
        let Some(p) = self.inject.take() else { return };
        self.source_table = guess_table(&p.orig);
        self.source_sql = p.orig;
        let s = t(Msg::StGeRequeryBusy).to_string();
        self.status(s);
        self.sync_edit_tools();
    }

    /// 호스트 주입: 메타(카탈로그)에서 온 열 명세 — 이름으로 맞춘다(길이 · NOT NULL · 기본값 · 종류).
    pub(crate) fn set_col_specs(&mut self, specs: &[CellSpec]) {
        let Some(e) = self.edit.as_mut() else { return };
        for c in &mut e.cols {
            if let Some(sp) = specs.iter().find(|s| s.name.eq_ignore_ascii_case(&c.name)) {
                let mut sp = sp.clone();
                sp.name = c.name.clone();
                if c.hidden {
                    sp.read_only = true; // 숨은 키 열은 카탈로그 명세가 와도 편집·INSERT 대상이 아니다
                }
                c.spec = sp;
            }
        }
    }

    pub(crate) fn take_edit_requests(&mut self) -> Vec<EditRequest> {
        std::mem::take(&mut self.edit_reqs)
    }

    pub(crate) fn has_edit_requests(&self) -> bool {
        !self.edit_reqs.is_empty()
    }

    /// 호스트가 지금 키를 조회할 수 없을 때(세션 바쁨) 요청을 되돌려 놓는다 — 다음 기회에 다시.
    pub(crate) fn requeue_keys(&mut self, table: String) {
        if self.edit.as_ref().is_some_and(|e| !e.keys_ready) {
            self.edit_reqs.push(EditRequest::NeedKeys { table });
        }
    }

    pub(crate) fn edit_dirty(&self) -> bool {
        self.edit.as_ref().is_some_and(|e| e.cs.is_dirty())
    }

    pub(crate) fn editing_cell(&self) -> bool {
        self.edit.as_ref().is_some_and(|e| e.live.is_open())
    }

    /// 상태줄/푸터용 편집 상태 한 줄(편집 가능 · 읽기 전용 이유 · 변경 수).
    pub(crate) fn edit_status_text(&self) -> Option<String> {
        if let Some(e) = self.edit.as_ref() {
            let (m, i, d) = e.cs.counts();
            if m + i + d > 0 {
                return Some(tf(
                    Msg::StGeDirty,
                    &[&m.to_string(), &i.to_string(), &d.to_string()],
                ));
            }
            return None;
        }
        self.read_only
            .as_ref()
            .filter(|_| self.rs.is_some())
            .map(|r| tf(Msg::StGeReadOnly, &[&r.text()]))
    }

    fn status(&mut self, s: String) {
        self.edit_reqs.push(EditRequest::Status(s));
    }

    /// 편집 툴바 활성(추가/삭제/복제 = 편집 가능 · 적용/취소 = 변경 있음).
    fn sync_edit_tools(&mut self) {
        let mut inv = Invalidations::default();
        let (editable, dirty, applying, ready) = match self.edit.as_ref() {
            Some(e) => (true, e.cs.is_dirty(), e.applying, e.keys_ready),
            None => (false, false, false, false),
        };
        let can = editable && !applying && self.rs.is_some();
        self.tools_has_sel = self.sel_cur.is_some();
        self.tb_edit.set_item_enabled("row.add", can, &mut inv);
        self.tb_edit
            .set_item_enabled("row.dup", can && self.sel_cur.is_some(), &mut inv);
        self.tb_edit
            .set_item_enabled("row.del", can && self.sel_cur.is_some(), &mut inv);
        self.tb_edit.set_item_enabled(
            "row.save",
            can && dirty && ready && self.session_open(),
            &mut inv,
        );
        self.tb_edit
            .set_item_enabled("row.cancel", can && dirty, &mut inv);
        let ident = self.can_identify();
        self.tb_edit
            .set_item_enabled("row.identify", ident, &mut inv);
    }

    /// 원본 셀 값(문자열 · NULL = None) — 편집 상자·키 비교·붙여넣기 Clean 판정.
    fn src_text(&self, row: usize, col: usize) -> Option<String> {
        self.rs
            .as_ref()
            .and_then(|rs| rs.row(row))
            .and_then(|r| r.get(col))
            .and_then(gridedit_sql::value_text)
    }

    /// 셀의 지금 값(덧그림 우선) — 표시·복사·편집 진입용.
    fn cur_text(&self, rref: RowRef, col: usize) -> Option<String> {
        if let Some(e) = self.edit.as_ref() {
            if let Some(v) = e.cs.cell(rref, col) {
                return v.clone();
            }
        }
        match rref {
            RowRef::Existing(r) => self.src_text(r, col),
            RowRef::Inserted(_) => None,
        }
    }

    /// 표시 순서 재구성: 원본 행 순서는 그대로 두고, 추가 행을 그 `after` 원본 행 바로 아래(없으면 끝)에 끼운다.
    fn rebuild_row_order(&mut self) {
        let n = self.src_len();
        let Some(e) = self.edit.as_ref() else {
            self.row_order.retain(|&ri| ri < n);
            return;
        };
        let existing: Vec<usize> = self
            .row_order
            .iter()
            .copied()
            .filter(|&ri| ri < n)
            .collect();
        let mut out = Vec::with_capacity(existing.len() + 8);
        let mut tail = Vec::new();
        let mut by_after: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        // 추가 행의 표시 순서 = `ins_order`(없는 k는 뒤에 · 사라진 k는 버림).
        let live: Vec<usize> = e.cs.inserted_rows().map(|(k, _)| k).collect();
        self.ins_order.retain(|k| live.contains(k));
        for &k in &live {
            if !self.ins_order.contains(&k) {
                self.ins_order.push(k);
            }
        }
        let rank = |k: usize| {
            self.ins_order
                .iter()
                .position(|&x| x == k)
                .unwrap_or(usize::MAX)
        };
        let mut ins: Vec<(usize, Option<usize>)> =
            e.cs.inserted_rows().map(|(k, ir)| (k, ir.after)).collect();
        ins.sort_by_key(|(k, _)| rank(*k));
        for (k, after) in ins {
            match after.filter(|a| *a < n) {
                Some(a) => by_after.entry(a).or_default().push(n + k),
                None => tail.push(n + k),
            }
        }
        for ri in existing {
            out.push(ri);
            if let Some(v) = by_after.remove(&ri) {
                out.extend(v);
            }
        }
        out.extend(tail);
        self.row_order = out;
        let rows = self.rows();
        if let Some((r, c)) = self.sel_cur {
            if r >= rows {
                self.sel_cur = rows.checked_sub(1).map(|last| (last, c));
            }
        }
        self.regions.retain(|&(r0, _, _, _)| r0 < rows);
        for reg in &mut self.regions {
            reg.1 = reg.1.min(rows.saturating_sub(1));
        }
    }

    /// 셀 사각형(표시 좌표 · 창 좌표) — 보이지 않으면 None.
    fn cell_rect(&self, di: usize, pos: usize) -> Option<Rect> {
        let ci = *self.col_order.get(pos)?;
        let cw = self.col_w.get(ci).copied().unwrap_or(80);
        let x = self.col_x(pos);
        let y = self.bounds.y + self.header_h + di as i32 * self.row_h - self.view_y();
        let body = Rect::new(
            self.bounds.x + self.gutter_w,
            self.bounds.y + self.header_h,
            (self.bounds.w - self.gutter_w).max(0),
            self.body_h(),
        );
        let r = Rect::new(x, y, cw - 1, self.row_h).intersection(&body);
        (r.w > 0 && r.h > 0).then_some(r)
    }

    /// 편집 진입(현재 포커스 셀) — `replace` = 타이핑한 첫 글자.
    fn begin_edit(&mut self, replace: Option<char>, select_all: bool) {
        let Some((di, pos)) = self.sel_cur else {
            return;
        };
        if self.edit.is_none() {
            if let Some(r) = self.read_only.clone() {
                self.status(tf(Msg::StGeReadOnly, &[&r.text()]));
            }
            return;
        }
        let Some(rref) = self.rref_at(di) else { return };
        let Some(&ci) = self.col_order.get(pos) else {
            return;
        };
        let text = self.cur_text(rref, ci).unwrap_or_default();
        let Some(rect) = self.cell_rect(di, pos) else {
            return;
        };
        let scale = self.menu_scale();
        let e = self.edit.as_mut().expect("checked");
        if e.applying || e.cs.is_deleted(rref) {
            return;
        }
        let spec = e.cols.get(ci).map(|c| c.spec.clone()).unwrap_or_default();
        if spec.read_only || !spec.kind.inline_editable() {
            let why = if spec.read_only {
                gridedit::EditError::ReadOnly
            } else {
                gridedit::EditError::Binary
            };
            self.status(why.message());
            return;
        }
        e.live_at = Some((di, pos));
        e.live.begin(EditStart {
            cell: (rref, ci),
            spec,
            text: &text,
            rect,
            scale,
            replace,
            select_all,
            // 셀 글자 여백(페인트의 `pad = 6 × 배율`)과 같게 — 편집에 들어가도 글자가 제자리(사용자 09-26).
            pad: Some((6.0 * scale).round() as i32),
        });
    }

    fn menu_scale(&self) -> f32 {
        // 행 높이는 글꼴 높이 × 배율 — 상자 배율은 호스트가 on_event로 넘기는 값과 같아야 하나, 여기서는 페인트 때 재설정된다.
        self.live_scale
    }

    /// 편집기가 확정한 값을 변경 집합에 놓는다.
    fn commit_cell(&mut self, cell: (RowRef, usize), value: Option<String>) {
        let orig = match cell.0 {
            RowRef::Existing(r) => Some(self.src_text(r, cell.1)),
            RowRef::Inserted(_) => None,
        };
        if let Some(e) = self.edit.as_mut() {
            e.cs.set_cell(cell.0, cell.1, value, orig.as_ref());
            e.live_at = None;
            e.error_row = None;
        }
        self.sync_edit_tools();
    }

    fn move_after_commit(&mut self, mv: Move) {
        match mv {
            Move::Down => self.move_sel(1, 0, false, false),
            Move::Up => self.move_sel(-1, 0, false, false),
            Move::Right => self.move_sel(0, 1, false, false),
            Move::Left => self.move_sel(0, -1, false, false),
            Move::None => {}
        }
    }

    /// 선택 셀 전부 비움(NULL 또는 빈 문자열 · 설정) — 되돌리기 한 묶음.
    fn clear_selected_cells(&mut self, force_null: bool) {
        if self.edit.is_none() || self.regions.is_empty() {
            return;
        }
        let cells: Vec<(RowRef, usize)> = {
            let mut v = Vec::new();
            for &reg in &self.regions {
                let (r0, r1, c0, c1) = reg;
                for di in r0..=r1.min(self.rows().saturating_sub(1)) {
                    let Some(rref) = self.rref_at(di) else {
                        continue;
                    };
                    for pos in c0..=c1.min(self.last_col()) {
                        if let Some(&ci) = self.col_order.get(pos) {
                            v.push((rref, ci));
                        }
                    }
                }
            }
            v
        };
        let empty_as_null = self.edit_cfg.empty_as_null;
        let mut rejected = 0;
        let origs: Vec<Option<Option<String>>> = cells
            .iter()
            .map(|(r, c)| match r {
                RowRef::Existing(i) => Some(self.src_text(*i, *c)),
                RowRef::Inserted(_) => None,
            })
            .collect();
        if let Some(e) = self.edit.as_mut() {
            e.cs.begin_group();
            for ((rref, ci), orig) in cells.iter().zip(origs.iter()) {
                let spec = e.cols.get(*ci).map(|c| &c.spec);
                let value: Option<String> = if force_null || empty_as_null {
                    None
                } else {
                    Some(String::new())
                };
                let ok = spec.is_none_or(|sp| sp.validate(value.as_deref()).is_ok());
                if !ok {
                    rejected += 1;
                    continue;
                }
                e.cs.set_cell(*rref, *ci, value, orig.as_ref());
            }
            e.cs.end_group();
        }
        if rejected > 0 {
            self.status(gridedit::EditError::NotNull.message());
        }
        self.sync_edit_tools();
    }

    /// 새 행(선택 행 아래 · 선택이 추가 행이어도 그 바로 아래 · 선택 없으면 끝 — 사용자 09-29).
    fn insert_row_here(&mut self) {
        let sel = self.sel_cur.and_then(|(di, _)| self.rref_at(di));
        let Some(e) = self.edit.as_mut() else { return };
        if e.applying {
            return;
        }
        let (after, below) = match sel {
            Some(RowRef::Existing(r)) => (Some(r), None),
            Some(RowRef::Inserted(k0)) => (
                e.cs.inserted_rows()
                    .find(|(k, _)| *k == k0)
                    .and_then(|(_, ir)| ir.after),
                Some(k0),
            ),
            None => (None, None),
        };
        let k = e.cs.insert_row(after, vec![]);
        self.ins_order.retain(|&x| x != k);
        match below.and_then(|k0| self.ins_order.iter().position(|&x| x == k0)) {
            Some(i) => self.ins_order.insert(i + 1, k),
            None => self.ins_order.push(k),
        }
        self.rebuild_row_order();
        self.focus_inserted(k);
    }

    /// 선택 행 복제(키 열은 비운다 · D-211).
    fn duplicate_row_here(&mut self) {
        let Some((di, _)) = self.sel_cur else { return };
        let Some(src) = self.rref_at(di) else { return };
        let ncols = self
            .col_order
            .len()
            .max(self.edit.as_ref().map_or(0, |e| e.cols.len()));
        let mut cells: Vec<Option<String>> = (0..ncols).map(|c| self.cur_text(src, c)).collect();
        let Some(e) = self.edit.as_mut() else { return };
        if e.applying || e.cs.is_deleted(src) {
            return;
        }
        if e.key_kind == KeyKind::Constraint {
            for &k in &e.key_cols {
                if let Some(c) = cells.get_mut(k) {
                    *c = None;
                }
            }
        }
        for (i, c) in e.cols.iter().enumerate() {
            if c.spec.kind == CellKind::Binary || c.spec.read_only {
                if let Some(v) = cells.get_mut(i) {
                    *v = None;
                }
            }
        }
        let k = e.cs.duplicate_row(src, cells);
        self.rebuild_row_order();
        self.focus_inserted(k);
    }

    fn focus_inserted(&mut self, k: usize) {
        let n = self.src_len();
        if let Some(di) = self.row_order.iter().position(|&ri| ri == n + k) {
            let pos = self.sel_cur.map_or(0, |c| c.1);
            self.select_only((di, pos));
            self.ensure_cell_visible(di, pos);
        }
        self.sync_edit_tools();
    }

    /// 선택 행 삭제 표식 토글(추가 행은 제거).
    fn delete_rows_here(&mut self) {
        let rows: Vec<usize> = if self.regions.is_empty() {
            self.sel_cur.map(|c| c.0).into_iter().collect()
        } else {
            let mut v: Vec<usize> = self
                .regions
                .iter()
                .flat_map(|&(r0, r1, _, _)| r0..=r1)
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let refs: Vec<RowRef> = rows.iter().filter_map(|&di| self.rref_at(di)).collect();
        let Some(e) = self.edit.as_mut() else { return };
        if e.applying {
            return;
        }
        e.cs.begin_group();
        for r in refs {
            e.cs.toggle_delete(r);
        }
        e.cs.end_group();
        self.rebuild_row_order();
        self.sync_edit_tools();
    }

    fn edit_undo(&mut self, redo: bool) {
        if let Some(e) = self.edit.as_mut() {
            if e.live.is_open() {
                return;
            }
            let ok = if redo { e.cs.redo() } else { e.cs.undo() };
            if !ok {
                return;
            }
        }
        self.rebuild_row_order();
        self.sync_edit_tools();
    }

    /// 전부 되돌림(✗).
    fn revert_edits(&mut self) {
        if let Some(e) = self.edit.as_mut() {
            e.live.close();
            e.live_at = None;
            e.cs.clear();
            e.blobs.clear();
            e.error_row = None;
        }
        self.rebuild_row_order();
        self.sync_edit_tools();
    }

    /// 변경 집합 → 문장(호스트가 실행).
    fn generated(&self) -> Result<(Vec<gridedit_sql::EditStmt>, String, String), String> {
        let e = self.edit.as_ref().ok_or_else(String::new)?;
        let inp = GenInput {
            dialect: self.dialect,
            table: &e.target.table,
            cols: &e.cols,
            key_cols: &e.key_cols,
            concurrency: self.edit_cfg.concurrency,
            blobs: Some(&e.blobs),
        };
        let orig = |r: usize, c: usize| -> Value {
            self.rs
                .as_ref()
                .and_then(|rs| rs.row(r))
                .and_then(|row| row.get(c))
                .cloned()
                .unwrap_or(Value::Null)
        };
        let stmts = gridedit_sql::generate(&inp, &e.cs, &orig)?;
        let preview = gridedit_sql::preview_text(&stmts, &e.target.table, e.key_kind);
        Ok((stmts, preview, e.target.table.clone()))
    }

    fn request_preview(&mut self) {
        if !self.edit_dirty() {
            let s = t(Msg::StGeNothing).to_string();
            self.status(s);
            return;
        }
        match self.generated() {
            Ok((_, preview, _)) => self.edit_reqs.push(EditRequest::Preview {
                title: t(Msg::WinGePreview).to_string(),
                text: preview,
            }),
            Err(m) => self.status(m),
        }
    }

    /// 변경 목록(사용자 09-26 "컬럼·변경값을 따로 · 원본과의 차이 확인 · 전체 건수 = 트랜잭션 범위"): 행 키 · 열 · 원본 → 새 값 · 추가/삭제 행.
    fn request_changes(&mut self) {
        let Some(e) = self.edit.as_ref() else { return };
        let (m, i, d) = e.cs.counts();
        if m + i + d == 0 {
            let s = t(Msg::GeChangesNone).to_string();
            self.status(s);
            return;
        }
        let key_names: Vec<String> = e
            .key_cols
            .iter()
            .filter_map(|&k| e.cols.get(k))
            .map(|c| c.name.clone())
            .collect();
        let key_label = if e.key_kind == KeyKind::AllColumns {
            "*".to_string()
        } else {
            key_names.join(", ")
        };
        let mut out = format!(
            "-- {}\n",
            tf(
                Msg::GeChangesHead,
                &[
                    &m.to_string(),
                    &i.to_string(),
                    &d.to_string(),
                    &e.target.table,
                    &key_label
                ]
            )
        );
        let key_of = |row: usize| -> String {
            e.key_cols
                .iter()
                .filter_map(|&k| e.cols.get(k).map(|c| (c, k)))
                .map(|(c, k)| {
                    format!(
                        "{}={}",
                        c.name,
                        self.src_text(row, k).unwrap_or_else(|| "NULL".into())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let show = |v: &Option<String>| -> String { v.clone().unwrap_or_else(|| "NULL".into()) };
        for row in e.cs.deleted_rows() {
            out.push_str(&format!("DELETE  #{} ({})\n", row + 1, key_of(row)));
        }
        let mut last: Option<usize> = None;
        for ((row, col), v) in e.cs.edits() {
            if last != Some(*row) {
                out.push_str(&format!("UPDATE  #{} ({})\n", row + 1, key_of(*row)));
                last = Some(*row);
            }
            let name = e
                .cols
                .get(*col)
                .map_or_else(|| col.to_string(), |c| c.name.clone());
            out.push_str(&format!(
                "        {name}: {} → {}\n",
                show(&self.src_text(*row, *col)),
                show(v)
            ));
        }
        for (k, ir) in e.cs.inserted_rows() {
            let vals: Vec<String> = ir
                .cells
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_some())
                .map(|(c, v)| {
                    format!(
                        "{}={}",
                        e.cols
                            .get(c)
                            .map_or_else(|| c.to_string(), |cc| cc.name.clone()),
                        show(v)
                    )
                })
                .collect();
            out.push_str(&format!("INSERT  +{} ({})\n", k + 1, vals.join(", ")));
        }
        self.edit_reqs.push(EditRequest::Preview {
            title: t(Msg::WinGeChanges).to_string(),
            text: out,
        });
    }

    fn request_apply(&mut self) {
        // 편집 중이던 셀 = 먼저 커밋(값을 잃지 않게) · 검증 실패면 적용하지 않는다.
        if !self.commit_live(Move::None) {
            return;
        }
        if !self.edit_dirty() {
            let s = t(Msg::StGeNothing).to_string();
            self.status(s);
            return;
        }
        let ready = self
            .edit
            .as_ref()
            .is_some_and(|e| e.keys_ready && !e.applying);
        if !ready {
            let s = t(Msg::StGeKeyWait).to_string();
            self.status(s);
            return;
        }
        match self.generated() {
            Ok((stmts, preview, table)) => {
                // ★ 행 단위 재조회(87 §12-4 · T-230): 수정·추가 행마다 키로 다시 읽는 문장 — 하나라도 못 만들면 전부 비움(전체 재조회).
                let mut refetch_rows: Vec<RowRef> = Vec::new();
                let mut refetch: Vec<nsql_core::ExecRequest> = Vec::new();
                let mut refetch_ok = true;
                if let Some(e) = self.edit.as_ref() {
                    let inp = GenInput {
                        dialect: self.dialect,
                        table: &e.target.table,
                        cols: &e.cols,
                        key_cols: &e.key_cols,
                        concurrency: self.edit_cfg.concurrency,
                        blobs: Some(&e.blobs),
                    };
                    let orig = |r: usize, c: usize| -> Value {
                        self.rs
                            .as_ref()
                            .and_then(|rs| rs.row(r))
                            .and_then(|row| row.get(c))
                            .cloned()
                            .unwrap_or(Value::Null)
                    };
                    for s in &stmts {
                        if s.kind == gridedit_sql::StmtKind::Delete {
                            continue;
                        }
                        match gridedit_sql::refetch_stmt(&inp, &e.cs, &orig, s.row) {
                            Some(req) => {
                                refetch_rows.push(s.row);
                                refetch.push(req);
                            }
                            None => {
                                refetch_rows.clear();
                                refetch.clear();
                                refetch_ok = false;
                                break;
                            }
                        }
                    }
                }
                if let Some(e) = self.edit.as_mut() {
                    e.applying = true;
                    e.error_row = None;
                    e.sent_rows = stmts.iter().map(|s| s.row).collect();
                    e.sent_refetch = refetch_rows;
                    e.sent_refetch_ok = refetch_ok;
                }
                let reqs: Vec<nsql_run::ApplyStmt> = stmts
                    .into_iter()
                    .map(|s| nsql_run::ApplyStmt {
                        req: s.req,
                        guard: s.guard,
                        label: s.label,
                    })
                    .collect();
                self.edit_reqs.push(EditRequest::Apply {
                    table,
                    stmts: reqs,
                    preview,
                    refetch,
                });
                self.sync_edit_tools();
            }
            Err(m) => self.status(m),
        }
    }

    /// ★ 적용 성공 + 행 단위 재조회 결과(87 §12-4 · T-230 · `grid.edit_refresh=rows`) — 수정 행은 **제자리 교체**, 추가 행은
    /// 실제 행으로 덧붙임(자리 유지), 삭제 행은 제거 · 스크롤·정렬·열 폭·선택은 그대로. 결과가 하나라도 1행이 아니면(다른 세션
    /// 삭제 · PG ctid 변경 · 키 모호) false → 호스트가 전체 재조회로 내려간다. 삭제만 있었으면 재조회 없이 제거한다.
    pub(crate) fn apply_done_rows(&mut self, refetched: Vec<Result<ResultSet, String>>) -> bool {
        let Some(e) = self.edit.as_ref() else {
            return false;
        };
        let ncols = self.rs.as_ref().map_or(0, |rs| rs.columns().len());
        if !e.sent_refetch_ok || refetched.len() != e.sent_refetch.len() {
            return false;
        }
        let mut updates: Vec<(usize, Vec<Value>)> = Vec::new();
        let mut inserts: Vec<(usize, Vec<Value>)> = Vec::new();
        for (rref, res) in e.sent_refetch.iter().zip(refetched) {
            let Ok(rs) = res else {
                return false;
            };
            if rs.rows.len() != 1 || rs.columns.len() != ncols {
                return false;
            }
            let row = rs.rows.into_iter().next().unwrap_or_default();
            match *rref {
                RowRef::Existing(r) => updates.push((r, row)),
                RowRef::Inserted(k) => inserts.push((k, row)),
            }
        }
        let deleted: Vec<usize> = e.cs.deleted_rows().collect();
        self.patch_rows(updates, inserts, deleted)
    }

    /// `grid.edit_refresh=local`: 서버를 다시 읽지 않고 **편집한 값을 세트에 굳힌다**(종류대로 값 변환 · 서버 기본값·트리거 결과는 모른다).
    pub(crate) fn apply_done_local(&mut self) -> bool {
        let Some(e) = self.edit.as_ref() else {
            return false;
        };
        let Some(rs) = self.rs.as_ref() else {
            return false;
        };
        let ncols = e.cols.len();
        let typed = |text: &Option<String>, c: usize| -> Value {
            if let Some((label, b)) = e.blobs.iter().find_map(|((_, cc), v)| {
                (*cc == c && text.as_deref() == Some(v.0.as_str())).then_some(v)
            }) {
                let _ = label;
                return Value::Bytes(b.clone());
            }
            match gridedit_sql::bound_of(text, e.cols[c].spec.kind, self.dialect) {
                gridedit_sql::Bound::Val(v, _) => v,
                gridedit_sql::Bound::Expr(_) => text.clone().map_or(Value::Null, Value::Str),
            }
        };
        let mut rows: Vec<usize> = e.cs.edits().map(|((r, _), _)| *r).collect();
        rows.sort_unstable();
        rows.dedup();
        let mut updates: Vec<(usize, Vec<Value>)> = Vec::new();
        for r in rows {
            if e.cs.is_deleted(RowRef::Existing(r)) {
                continue;
            }
            let Some(orig) = rs.row(r) else {
                continue;
            };
            let row: Vec<Value> = (0..ncols)
                .map(|c| match e.cs.cell(RowRef::Existing(r), c) {
                    Some(t) => typed(t, c),
                    None => orig.get(c).cloned().unwrap_or(Value::Null),
                })
                .collect();
            updates.push((r, row));
        }
        let inserts: Vec<(usize, Vec<Value>)> =
            e.cs.inserted_rows()
                .map(|(k, ir)| {
                    (
                        k,
                        (0..ncols)
                            .map(|c| typed(&ir.cells.get(c).cloned().unwrap_or(None), c))
                            .collect(),
                    )
                })
                .collect();
        let deleted: Vec<usize> = e.cs.deleted_rows().collect();
        self.patch_rows(updates, inserts, deleted)
    }

    /// 제자리 갱신의 공통 몸통 — 데이터 교체·추가·제거 · `row_order` 재매김(자리 유지) · Σ 보정 · 변경 집합 비움.
    fn patch_rows(
        &mut self,
        updates: Vec<(usize, Vec<Value>)>,
        inserts: Vec<(usize, Vec<Value>)>,
        deleted: Vec<usize>,
    ) -> bool {
        let n_src = self.src_len();
        let Some(rs) = self.rs.as_mut() else {
            return false;
        };
        for (r, row) in &updates {
            if !rs.set_row(*r, row.clone()) {
                return false;
            }
        }
        // 추가 행 = 실제 행으로(가상 index n+k → 새 번호).
        let mut new_idx: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for (k, row) in inserts.iter() {
            new_idx.insert(*k, rs.push_row(row.clone()));
        }
        // 표시 순서를 지금 자리 그대로 다시 매긴다(삭제 행은 빼고 · 뒤 번호는 삭제 수만큼 당김).
        let mut del_sorted = deleted;
        del_sorted.sort_unstable();
        del_sorted.dedup();
        let shift = |ri: usize| ri - del_sorted.partition_point(|&d| d < ri);
        let order: Vec<usize> = self
            .row_order
            .iter()
            .filter_map(|&ri| {
                if ri < n_src {
                    (!del_sorted.contains(&ri)).then(|| shift(ri))
                } else {
                    new_idx.get(&(ri - n_src)).map(|&ni| shift(ni))
                }
            })
            .collect();
        rs.remove_rows(&del_sorted);
        self.row_order = order;
        if let Some(t) = self.total.as_mut() {
            *t = (*t + inserts.len() as u64).saturating_sub(del_sorted.len() as u64);
        }
        if let Some(e) = self.edit.as_mut() {
            e.applying = false;
            e.error_row = None;
            e.sent_refetch.clear();
            e.last_patch = Some((updates.len(), inserts.len(), del_sorted.len()));
            e.cs = ChangeSet::new(e.cols.len());
            e.blobs.clear();
        }
        self.text_lines = Vec::new();
        self.text_bytes = 0;
        self.regions.clear();
        // 정렬 **또는 필터**가 있으면 새 행에 다시 투영(필터만 있을 때 새 페이지가 걸러지지 않던 결함 · 사용자 09-29).
        if !self.sort_keys.is_empty() || !self.filters.is_empty() {
            self.apply_sort();
        }
        if self.view != ResultView::Grid {
            self.text_keep_scroll = true;
            self.refresh_text_view();
        }
        self.sync_edit_tools();
        true
    }

    /// 적용 결과(호스트) — 성공 = 변경 집합 비움(재조회는 호스트) · 실패 = 그 문장의 행 표시.
    pub(crate) fn apply_done(&mut self, done: usize, error: Option<(usize, String)>) {
        let Some(e) = self.edit.as_mut() else { return };
        e.applying = false;
        match error {
            None => {
                e.cs.clear();
                e.blobs.clear();
                e.blobs.clear();
                e.error_row = None;
                self.rebuild_row_order();
            }
            Some((i, _)) => {
                e.error_row = e.sent_rows.get(i).copied();
                if done > 0 && e.error_row.is_none() {
                    // 수동 모드에서 일부가 들어갔다 — 변경 집합은 그대로(사용자가 커밋/롤백 뒤 정리).
                }
            }
        }
        self.sync_edit_tools();
    }

    /// 붙여넣기(호스트 클립보드 → 앵커 셀부터 · 자동 확장).
    pub(crate) fn paste_text(&mut self, text: &str) {
        if self.edit.is_none() {
            if let Some(r) = self.read_only.clone() {
                self.status(tf(Msg::StGeReadOnly, &[&r.text()]));
            }
            return;
        }
        let Some((di, pos)) = self.sel_cur else {
            return;
        };
        let Some(&ci) = self.col_order.get(pos) else {
            return;
        };
        let matrix = gridedit::parse_matrix(text);
        if matrix.is_empty() {
            return;
        }
        // 표시 순서를 행 참조 배열로(앵커 표시 행부터).
        let layout: Vec<RowRef> = (0..self.rows()).filter_map(|d| self.rref_at(d)).collect();
        // 열 매핑: 표시 열 순서(`col_order`)를 따라 앵커부터 — 부품은 연속 열을 가정하므로 표시 순서가 원본과 다르면 원본 순서로 붙인다.
        let opts = PasteOpts {
            null_token: Some(self.null_text.clone()),
            empty_as_null: self.edit_cfg.empty_as_null,
            auto_extend: true,
            max_rows: self.edit_cfg.paste_max,
        };
        let specs: Vec<CellSpec> = self
            .edit
            .as_ref()
            .map(|e| e.cols.iter().map(|c| c.spec.clone()).collect())
            .unwrap_or_default();
        let rs = self.rs.clone();
        let orig = move |r: usize, c: usize| -> Option<String> {
            rs.as_ref()
                .and_then(|rs| rs.row(r))
                .and_then(|row| row.get(c))
                .and_then(gridedit_sql::value_text)
        };
        let rep = {
            let Some(e) = self.edit.as_mut() else { return };
            gridedit::paste_apply(
                &mut e.cs,
                PasteAnchor {
                    layout: &layout,
                    row: di,
                    col: ci,
                },
                &matrix,
                &specs,
                &orig,
                &opts,
            )
        };
        self.rebuild_row_order();
        self.status(tf(
            Msg::StGePasted,
            &[
                &rep.set.to_string(),
                &rep.added_rows.to_string(),
                &rep.rejected.len().to_string(),
            ],
        ));
        self.sync_edit_tools();
    }

    /// 값 보기(긴 텍스트 · 이진 = 16진수 덤프) — 읽기 전용 창.
    fn request_view_value(&mut self) {
        let Some((di, pos)) = self.sel_cur else {
            return;
        };
        let Some(rref) = self.rref_at(di) else { return };
        let Some(&ci) = self.col_order.get(pos) else {
            return;
        };
        let name = self
            .rs
            .as_ref()
            .and_then(|rs| rs.columns().get(ci))
            .map_or_else(String::new, |c| c.name.clone());
        // 파일에서 넣은 이진이 있으면 그것 · 아니면 원본 이진 · 아니면 글(덧그림 우선).
        let pending_blob = self.edit.as_ref().and_then(|e| {
            let (label, b) = e.blobs.get(&(rref, ci))?;
            (e.cs.cell(rref, ci).and_then(|c| c.as_deref()) == Some(label.as_str()))
                .then(|| b.clone())
        });
        let orig = match rref {
            RowRef::Existing(r) => self
                .rs
                .as_ref()
                .and_then(|rs| rs.row(r))
                .and_then(|row| row.get(ci))
                .cloned(),
            RowRef::Inserted(_) => None,
        };
        let over = self
            .edit
            .as_ref()
            .and_then(|e| e.cs.cell(rref, ci).cloned());
        let (text, bytes) = match (pending_blob, &orig, over) {
            (Some(b), _, _) => (String::new(), Some(b)),
            (None, Some(Value::Bytes(b)), None) => (String::new(), Some(b.clone())),
            (None, _, Some(Some(s))) => (s, None),
            (None, _, Some(None)) => (String::new(), None),
            (None, Some(v), None) => (cell_text(v, &self.null_text), None),
            (None, None, None) => (self.cur_text(rref, ci).unwrap_or_default(), None),
        };
        let (editable, binary) = match self.edit.as_ref() {
            Some(e) => {
                let c = e.cols.get(ci);
                (
                    !e.applying && c.is_some_and(|c| !c.spec.read_only && !c.hidden),
                    c.is_some_and(|c| c.spec.kind == CellKind::Binary) || bytes.is_some(),
                )
            }
            None => (false, bytes.is_some()),
        };
        self.edit_reqs.push(EditRequest::ViewCell(ValueReq {
            row: rref,
            col: ci,
            name,
            text,
            bytes,
            editable,
            binary,
        }));
    }

    /// 값 창·자체 시험: 셀에 글 값을 넣는다(명세 검증 · NULL = None). 이진 열은 [`Self::set_cell_bytes`].
    pub(crate) fn set_cell_value(
        &mut self,
        rref: RowRef,
        col: usize,
        value: Option<String>,
    ) -> Result<(), String> {
        let Some(e) = self.edit.as_ref() else {
            return Err(self
                .read_only
                .as_ref()
                .map_or_else(String::new, |r| tf(Msg::StGeReadOnly, &[&r.text()])));
        };
        let Some(c) = e.cols.get(col) else {
            return Err(String::new());
        };
        if c.spec.read_only || c.hidden || e.applying {
            return Err(t(Msg::GeCellReadOnly).to_string());
        }
        let checked = c.spec.validate(value.as_deref()).map_err(|e| e.message())?;
        if let Some(e) = self.edit.as_mut() {
            e.blobs.remove(&(rref, col));
        }
        self.commit_cell((rref, col), checked);
        Ok(())
    }

    /// 파일에서 넣기(87 §5): 이진 값을 셀에 — 표시는 `<라벨 · n bytes>` · 적용 때 `Value::Bytes`(BLOB) 바인드.
    pub(crate) fn set_cell_bytes(
        &mut self,
        rref: RowRef,
        col: usize,
        bytes: Vec<u8>,
        label: &str,
    ) -> Result<(), String> {
        let Some(e) = self.edit.as_ref() else {
            return Err(t(Msg::GeCellReadOnly).to_string());
        };
        let Some(c) = e.cols.get(col) else {
            return Err(String::new());
        };
        if c.spec.read_only || c.hidden || e.applying {
            return Err(t(Msg::GeCellReadOnly).to_string());
        }
        let text = gridedit_sql::blob_label(label, bytes.len());
        if let Some(e) = self.edit.as_mut() {
            e.blobs.insert((rref, col), (text.clone(), bytes));
        }
        self.commit_cell((rref, col), Some(text));
        Ok(())
    }

    /// 셀의 지금 이진 값(파일에서 넣은 것 우선 · 시험·덤프용).
    #[cfg(test)]
    pub(crate) fn cell_bytes(&self, rref: RowRef, col: usize) -> Option<Vec<u8>> {
        self.edit
            .as_ref()
            .and_then(|e| e.blobs.get(&(rref, col)).map(|(_, b)| b.clone()))
    }

    /// 편집 동작 실행(키·메뉴·툴바 공통).
    fn edit_action(&mut self, a: EditAction) {
        // 행 단위 동작·적용 전에는 열린 셀 편집을 먼저 커밋한다(값을 잃지 않게).
        if matches!(
            a,
            EditAction::DuplicateRow
                | EditAction::InsertRow
                | EditAction::DeleteRow
                | EditAction::Apply
                | EditAction::PreviewSql
                | EditAction::ShowChanges
        ) && !self.commit_live(Move::None)
        {
            return;
        }
        match a {
            EditAction::BeginEdit => self.begin_edit(None, true),
            EditAction::BeginEditWith(c) => self.begin_edit(Some(c), false),
            EditAction::ClearCells => self.clear_selected_cells(false),
            EditAction::SetNull => self.clear_selected_cells(true),
            EditAction::DuplicateRow => self.duplicate_row_here(),
            EditAction::InsertRow => self.insert_row_here(),
            EditAction::DeleteRow => self.delete_rows_here(),
            EditAction::Undo => self.edit_undo(false),
            EditAction::Redo => self.edit_undo(true),
            EditAction::Apply => self.request_apply(),
            EditAction::Revert => self.revert_edits(),
            EditAction::ViewValue => self.request_view_value(),
            EditAction::PreviewSql => self.request_preview(),
            EditAction::ShowChanges => self.request_changes(),
            EditAction::Copy | EditAction::Paste => {}
        }
    }

    /// 자체 시험(기동 명령 `grid.edit.set:<행>,<열>,<글>` · 키 주입 0): 표시 좌표의 셀에 값을 놓는다(`NULL` 토큰 = NULL).
    pub(crate) fn set_cell_for_test(&mut self, di: usize, pos: usize, text: &str) -> bool {
        if self.edit.is_none() || di >= self.rows() {
            return false;
        }
        let Some(&ci) = self.col_order.get(pos) else {
            return false;
        };
        let Some(rref) = self.rref_at(di) else {
            return false;
        };
        let value = if text == self.null_text || text == "NULL" {
            None
        } else {
            Some(text.to_string())
        };
        let checked = match self.edit.as_ref().and_then(|e| e.cols.get(ci)) {
            Some(c) => match c.spec.validate(value.as_deref()) {
                Ok(v) => v,
                Err(e) => {
                    self.status(e.message());
                    return false;
                }
            },
            None => value,
        };
        self.select_only((di, pos));
        self.commit_cell((rref, ci), checked);
        true
    }

    /// 자체 시험: 표시 좌표 → (행 참조, 원본 열, 이진 열인가).
    pub(crate) fn cell_at_for_test(&self, di: usize, pos: usize) -> Option<(RowRef, usize, bool)> {
        let rref = self.rref_at(di)?;
        let &ci = self.col_order.get(pos)?;
        let binary = self
            .edit
            .as_ref()
            .and_then(|e| e.cols.get(ci))
            .is_some_and(|c| c.spec.kind == CellKind::Binary);
        Some((rref, ci, binary))
    }

    /// 자체 시험: 셀 선택(표시 좌표).
    pub(crate) fn select_cell_for_test(&mut self, di: usize, pos: usize) {
        if di < self.rows() && pos < self.col_order.len() {
            self.select_only((di, pos));
        }
    }

    /// 자체 시험 덤프: 표시 행마다 `상태|셀…`(덧그림 반영) + 편집 상태 줄.
    pub(crate) fn dump_edit(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "rows={} src={} editable={} dirty={} identify={} status={}\n",
            self.rows(),
            self.src_len(),
            self.edit.is_some(),
            self.edit_dirty(),
            self.can_identify(),
            self.edit_status_text().unwrap_or_default()
        ));
        // 페치 상태(T-285 진단 · 협업 bin50 b1): 자동 페치 조건의 재료 전부.
        let (_, my) = self.max_scroll();
        out.push_str(&format!(
            "fetch more={} fetching={} req={} auto={} at={:?} fill_left={} filters={} promoted={} my={} scroll_y={} row_h={} view_h={} page={} scroll_end={}\n",
            self.more,
            self.fetching,
            self.fetch_req.is_some(),
            self.auto_fetch,
            self.auto_fetch_at,
            self.fill_pages_left,
            self.filters.len(),
            self.filter_promoted,
            my,
            self.scroll_y,
            self.row_h,
            self.bounds.h - self.footer_h,
            self.page_rows,
            // 세로 끝에 닿았나(S95 = 큰 결과 썸 드래그 뒤 참이어야 · 넘침이면 0 근처라 거짓) · 스크롤할 것이 없으면 거짓.
            my > 0 && self.scroll_y >= my
        ));
        out.push_str(&format!(
            "tools add={} dup={} del={} save={} cancel={} sel={:?}\n",
            self.tb_edit.item_enabled("row.add"),
            self.tb_edit.item_enabled("row.dup"),
            self.tb_edit.item_enabled("row.del"),
            self.tb_edit.item_enabled("row.save"),
            self.tb_edit.item_enabled("row.cancel"),
            self.sel_cur
        ));
        if let Some(e) = self.edit.as_ref() {
            out.push_str(&format!(
                "table={} key={:?} kind={:?} ready={} hidden={} inject={} patched={} cols={}\n",
                e.target.table,
                e.key_cols,
                e.key_kind,
                e.keys_ready,
                e.hidden,
                self.inject.is_some(),
                e.last_patch
                    .map_or("-".to_string(), |(u, i, d)| format!("{u}/{i}/{d}")),
                e.cols
                    .iter()
                    .map(|c| format!("{}:{:?}", c.name, c.spec.kind))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        for di in 0..self.rows().min(200) {
            let Some(rref) = self.rref_at(di) else {
                continue;
            };
            let st = self
                .edit
                .as_ref()
                .map_or(RowStatus::Clean, |e| e.cs.status(rref));
            let cells: Vec<String> = self
                .col_order
                .iter()
                .map(|&ci| self.cur_text(rref, ci).unwrap_or_else(|| "NULL".into()))
                .collect();
            out.push_str(&format!("{di}|{st:?}|{}\n", cells.join("|")));
        }
        out
    }

    /// 호스트 명령 id(팔레트·키맵 · `grid.edit.*` · 툴바 id) → 편집 동작.
    pub(crate) fn edit_command(&mut self, id: &str) -> bool {
        if id == "grid.edit.identify" || id == "row.identify" {
            return self.edit_identify();
        }
        match gridedit::action_for_command(id) {
            Some(a) => {
                self.edit_action(a);
                true
            }
            None => false,
        }
    }

    /// 편집 상자의 메뉴/단축키가 남긴 클립보드 요청 — 복사·잘라내기는 그리드의 복사 통로(`pending_copy`)로 · 붙여넣기는 호스트에.
    fn live_edit_ctx(&mut self) {
        let Some(e) = self.edit.as_mut() else { return };
        let Some(act) = e.live.textbox_mut().take_edit_ctx() else {
            return;
        };
        let mut inv = Invalidations::default();
        match act {
            nexa_ctl::EditCtxAction::Copy => {
                if let Some(t) = e.live.textbox().copy_selection() {
                    let n = t.chars().count();
                    self.pending_copy = Some((t, n));
                }
            }
            nexa_ctl::EditCtxAction::Cut => {
                if let Some(t) = e.live.textbox_mut().cut_selection(&mut inv) {
                    let n = t.chars().count();
                    self.pending_copy = Some((t, n));
                }
            }
            nexa_ctl::EditCtxAction::Paste => self.edit_reqs.push(EditRequest::ClipboardPaste),
            nexa_ctl::EditCtxAction::Custom(_) => {}
        }
    }

    /// 편집 상자의 열린 우클릭 메뉴 영역(호스트 라우팅용 · 닫혀 있으면 빈 Rect).
    pub(crate) fn live_popup_bounds(&self) -> Rect {
        self.edit
            .as_ref()
            .filter(|e| e.live.is_open())
            .map_or(Rect::default(), |e| e.live.textbox().popup_bounds())
    }

    /// 호스트가 읽은 클립보드 글을 편집 상자에(메뉴 붙여넣기).
    pub(crate) fn live_paste(&mut self, text: &str) {
        if let Some(e) = self.edit.as_mut() {
            if e.live.is_open() {
                let mut inv = Invalidations::default();
                e.live.textbox_mut().paste(text, &mut inv);
            }
        }
    }

    /// 열린 편집 상자를 커밋한다(적용·툴바·명령 전) — 닫혀 있으면 true · 검증 실패 = false(상태줄 안내).
    fn commit_live(&mut self, mv: Move) -> bool {
        let Some(e) = self.edit.as_mut() else {
            return true;
        };
        if !e.live.is_open() {
            return true;
        }
        let cell = e.live.cell();
        match e.live.try_commit(mv) {
            LiveEvent::Commit { value, mv } => {
                if let Some(cell) = cell {
                    self.commit_cell(cell, value);
                }
                self.move_after_commit(mv);
                true
            }
            LiveEvent::Invalid(err) => {
                self.status(err.message());
                false
            }
            _ => true,
        }
    }

    /// 편집 상자 안 클립보드/전체 선택(호스트 ⌘X/C/V/A · 편집 중에는 그리드가 아니라 상자에 한정 · 09-26).
    pub(crate) fn live_copy(&mut self) -> Option<String> {
        self.edit.as_ref()?.live.textbox().copy_selection()
    }
    pub(crate) fn live_cut(&mut self) -> Option<String> {
        let mut inv = Invalidations::default();
        self.edit
            .as_mut()?
            .live
            .textbox_mut()
            .cut_selection(&mut inv)
    }
    pub(crate) fn live_select_all(&mut self) {
        if let Some(e) = self.edit.as_mut() {
            let mut inv = Invalidations::default();
            e.live
                .textbox_mut()
                .on_event(&InputEvent::SelectAll, &mut inv);
        }
    }

    /// 살아 있는 편집기가 사건을 먹었는가(열려 있을 때만).
    fn live_event(&mut self, ev: &InputEvent) -> bool {
        let Some(e) = self.edit.as_mut() else {
            return false;
        };
        if !e.live.is_open() {
            return false;
        }
        let mut inv = Invalidations::default();
        // ★ 편집 상자의 우클릭 메뉴가 열려 있으면 **모든 사건이 그 상자로**(항목 클릭 = 기능만 · 아래 셀로 새지 않는다 · 사용자 09-26).
        if e.live.textbox().popup_open() {
            e.live.textbox_mut().on_event(ev, &mut inv);
            self.live_edit_ctx();
            return true;
        }
        // ★ 커밋 대상 셀은 사건 **전에** 읽는다 — `try_commit`이 상자를 닫으면서 셀을 지우므로(09-26 "값이 반영 안 됨" 원인).
        let cell = e.live.cell();
        let inside = match *ev {
            InputEvent::MouseDown { x, y, .. }
            | InputEvent::RightDown { x, y }
            | InputEvent::MouseMove { x, y } => e.live.rect().contains(Point { x, y }),
            InputEvent::MouseUp { .. } => true,
            _ => false,
        };
        let r = match *ev {
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. } => {
                e.live.textbox_mut().on_event(ev, &mut inv);
                return inside;
            }
            InputEvent::MouseDown { .. } | InputEvent::RightDown { .. } => {
                if inside {
                    e.live.textbox_mut().on_event(ev, &mut inv);
                    self.live_edit_ctx();
                    return true;
                }
                // 바깥 클릭 = 커밋 시도 · 실패면 클릭을 막는다(상자에 붉은 띠).
                e.live.try_commit(Move::None)
            }
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => return false,
            _ => e.live.on_event(ev, self.shift, &mut inv),
        };
        match r {
            LiveEvent::Commit { value, mv } => {
                if let Some(cell) = cell {
                    self.commit_cell(cell, value);
                }
                self.move_after_commit(mv);
                !matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                )
            }
            LiveEvent::Cancel => {
                if let Some(e) = self.edit.as_mut() {
                    e.live_at = None;
                }
                true
            }
            LiveEvent::Invalid(err) => {
                self.status(err.message());
                true
            }
            LiveEvent::None => true,
        }
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
        // 채움 카운터(T-285): 필터가 걸린 채 받은 페이지마다 −1 · 0이 되고도 더 있으면 "가져온 N행 기준" 안내(오해 방지 · 43 §9).
        if !self.filters.is_empty() {
            self.fill_pages_left = self.fill_pages_left.saturating_sub(1);
            if self.fill_pages_left == 0 && more {
                let n = (self.src_rows() + page.rows.len()).to_string();
                self.status(tf(Msg::StFilterFillStopped, &[&n]));
            }
        }
        self.push_segment(page);
        // 로컬 채움 연쇄(T-285 · D-258): 아직 상한 안이고 화면이 덜 찼으면 다음 페이지를 바로(이벤트 없이 · 협업 bin49 b1).
        if !self.filters.is_empty() && more && self.fill_pages_left > 0 {
            self.clamp();
        }
    }

    /// ★ 증분 표시 배치(사용자 10-09): 진행·■ 상태(`fetch_all_pending`)는 **그대로** 두고 행만 이어 붙인다 — 열 너비는 첫 세그먼트 기준
    /// (늦게 온 긴 값에 흔들리지 않음) · 정렬·필터가 있으면 새 행만 재투영 · 마지막 빈 `append_all`이 상태를 닫는다.
    pub(crate) fn append_live(&mut self, page: ResultSet) {
        if page.rows.is_empty() || self.rs.is_none() {
            return;
        }
        self.push_segment(page);
    }

    /// 세그먼트 덧붙이기(복사 0 · DR-33 · 빈 세그먼트는 넣지 않음) + 정렬·필터 재투영 + 텍스트 보기 재파생.
    fn push_segment(&mut self, page: ResultSet) {
        let Some(rs) = self.rs.as_mut() else {
            return;
        };
        if page.rows.is_empty() {
            return;
        }
        let start = rs.len();
        // 세그먼트 덧붙이기 = 이동(복사 0 · DR-33) — 변환 스레드가 옛 세그먼트를 공유 중이어도 서로 간섭 없음.
        rs.push(page);
        let end = rs.len();
        self.row_order.extend(start..end);
        // 정렬 **또는 필터**가 있으면 새 행에 다시 투영(필터만 있을 때 새 페이지가 걸러지지 않던 결함 · 사용자 09-29).
        if !self.sort_keys.is_empty() || !self.filters.is_empty() {
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
        self.edit = None;
        self.row_order = (0..n).collect();
        // 같은 문장의 교체 = 정렬·필터 투영은 유지하되 새 행에 다시 적용.
        self.auto_fetch_at = None;
        self.apply_sort();
        if !same_cols {
            self.col_order = (0..self.rs.as_ref().map_or(0, |r| r.columns().len())).collect();
            self.col_w.clear();
            self.sort_keys.clear();
        }
        // 편집 상태를 다시 준비(숨은 열·키 판정은 `edit_prepare`가 계획에서 복원 · 키는 호스트 캐시에서 즉시).
        let src = self.source_sql.clone();
        let countable = self.countable;
        self.edit_prepare(&src, countable);
        // 정렬 **또는 필터**가 있으면 새 행에 다시 투영(필터만 있을 때 새 페이지가 걸러지지 않던 결함 · 사용자 09-29).
        if !self.sort_keys.is_empty() || !self.filters.is_empty() {
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
        // ★ 필터 표식 툴팁(빗금 위에 머물렀을 때만 · 적용된 술어).
        if let Some((ci, since)) = self.mark_hover {
            if self.now_ms.saturating_sub(since) >= MARK_TIP_MS && !self.menu.is_open() {
                if let (Some(text), Some(r)) = (self.filter_tip(ci), self.mark_rect_of(ci)) {
                    let clamp = dc
                        .surface_size()
                        .map_or(self.bounds, |(w, h)| Rect::new(0, 0, w, h));
                    nexa_ctl::draw::draw_tooltip_in(
                        dc,
                        th,
                        r,
                        (clamp.x, clamp.w),
                        &text,
                        self.live_scale,
                    );
                }
            }
        }
        self.menu.paint(dc, th);
        // ★ 값 목록 팝업(깔때기 · 팝업 층 · 메뉴 뒤) · 조건 바 완성 팝업.
        self.vpick.paint(dc, th);
        if self.cond_on {
            self.cond.paint_popup(dc, th);
        }
        // 셀 편집기의 우클릭 메뉴 = 최상위(편집 테두리·이웃 셀 위 · UX 규칙 09-26).
        if let Some(e) = self.edit.as_ref() {
            e.live.paint_popup(dc, th);
        }
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
                // 이번 놓임이 "열린 보기 메뉴를 닫은 클릭"의 끝이면 다시 열지 않는다(토글).
                let toggle_off = !down && std::mem::take(&mut self.view_toggle_off);
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
                    (
                        &mut self.tb_edit,
                        &[
                            "row.add",
                            "row.del",
                            "row.dup",
                            "row.save",
                            "row.cancel",
                            "row.identify",
                        ][..],
                    ),
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
                    Some("view") if !toggle_off => {
                        let r = self.tb_view.bounds();
                        self.open_view_menu(r.x, r.y, scale);
                    }
                    Some("view") => {}
                    Some("refresh") if self.can_refresh() => self.want_refresh = true,
                    Some(id @ ("row.add" | "row.del" | "row.dup" | "row.save" | "row.cancel")) => {
                        self.edit_command(id);
                    }
                    // 전체 조회 = 나머지 이어 받기(자동 페치가 나가 있으면 큐).
                    Some("row.identify") => {
                        self.edit_identify();
                    }
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
        self.menu_is_view = true;
        self.menu.open_at(x, y, items, self.menu_host(), text_w);
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
            InputEvent::Key { key: Key::Down, .. } => {
                self.text_scroll.1 += self.row_h * self.key_step(1)
            }
            InputEvent::Key { key: Key::Up, .. } => {
                self.text_scroll.1 -= self.row_h * self.key_step(-1)
            }
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
            && ((my > 0 && self.text_scroll.1 >= my - self.row_h.max(1))
                || (my == 0 && !self.filters.is_empty() && self.fill_pages_left > 0))
            && self.auto_fetch_at != Some(self.src_rows())
        {
            let offset = self.src_rows();
            self.auto_fetch_at = Some(offset);
            self.fetch_req = Some(FetchReq::Next {
                offset,
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

    /// 정규식 필터 값 목록 상한(설정 `grid.filter_list_max`).
    pub(crate) fn set_filter_list_max(&mut self, n: usize) {
        self.filter_list_max = n.max(1);
    }

    /// 스크롤 단위 — `true` = 항목(행) 단위 · `false` = 픽셀(기본).
    /// 결과 그리드만의 고속 스크롤 설정(None = 전역 · `scroll.fast_grid_extra`).
    pub(crate) fn set_fast_override(&mut self, cfg: Option<nexa_ctl::FastScroll>) {
        self.fast_override = cfg;
        self.bars.set_fast_override(cfg);
        self.key_accel.reset();
    }

    /// ↑/↓ 한 번의 이동 배수(고속 스크롤이 켜져 있고 같은 방향이 빨리 이어질 때만 > 1) + 속도 HUD.
    fn key_step(&mut self, dir: i32) -> i32 {
        let cfg = self.bars.fast_cfg();
        let k = self.key_accel.factor_cfg(dir, &cfg);
        self.bars.note_fast(k);
        k
    }

    pub(crate) fn set_row_snap(&mut self, on: bool) {
        self.row_snap = on;
        self.clamp();
    }

    pub(crate) fn set_bounds(&mut self, b: Rect) {
        // 필터 줄이 있으면 그만큼 아래에서 시작한다(줄은 받은 영역의 맨 위).
        let h = self.strip_h.min(b.h.max(0));
        self.bounds = Rect::new(b.x, b.y + h, b.w, b.h - h);
        self.clamp();
    }

    /// 그리드 본문(머리 줄 포함 · 조건 바·칩 줄 제외) — 글꼴 크기 HUD처럼 조건 바를 덮으면 안 되는 겹침 그림의 영역.
    pub(crate) fn body_bounds(&self) -> Rect {
        self.bounds
    }

    /// 호스트가 준 **바깥** 영역(필터 줄/조건 바 포함) — 탭을 바꿀 때 다른 그리드에 넘길 사각형은 이것(안쪽 `bounds`를 넘기면
    /// 줄 높이가 한 번 더 깎여 왕복마다 공백이 자랐다 · 사용자 10-06 Output 탭 왕복).
    pub(crate) fn outer_bounds(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x, b.y - self.strip_h, b.w, b.h + self.strip_h)
    }

    /// 값 고르기 메뉴 항목 수(설정 `grid.filter_pick_max`).
    pub(crate) fn set_filter_pick_max(&mut self, n: usize) {
        self.filter_pick_max = n.clamp(5, 200);
    }

    /// 열 머리 깔때기 표시 방법(설정 `grid.filter_funnel` = `always` · `hover` · `none`).
    pub(crate) fn set_filter_funnel(&mut self, mode: &str) {
        self.funnel_mode = FunnelMode::parse(mode);
    }

    /// 결과 필터 사용 여부(설정 `grid.filter_enabled`) — 끄면 걸려 있던 필터도 푼다(값 목록 팝업도 닫는다).
    pub(crate) fn set_filter_enabled(&mut self, on: bool) {
        self.filter_enabled = on;
        if !on {
            self.clear_filters();
            self.vpick.close();
        }
    }

    /// 호스트가 가져가는 다시 그리기 깃발(한 번).
    pub(crate) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// 깔때기 글리프 한 변 — 행 높이의 35 %(사용자 10-06 "70 % 수준으로") · 최소 6.
    fn funnel_g(&self) -> i32 {
        (self.row_h * 7 / 20).max(6)
    }

    /// 열 폭에 더하는 깔때기 여유(아이콘 + 간격) — 필터가 켜져 있으면 표시 방법과 무관하게 늘(hover는 잠깐 나타나고 · 필터가
    /// 걸리면 빗금 깔때기가 그 자리에 서므로 · 이름이 `nan`으로 잘리던 것 · 협업 V1 ③b/d 10-06).
    fn funnel_allow(&self) -> i32 {
        if self.filter_enabled {
            let g = self.funnel_g();
            g + (g / 3).max(2)
        } else {
            0
        }
    }

    /// 이 열 머리에 민무늬 깔때기(값 목록 버튼)를 그릴까 — 필터 켜짐 · 표시 방법 · 필터 안 걸림(걸리면 빗금 깔때기).
    /// 🔧 사용자 10-07 "우클릭 메뉴 중 깔때기가 사라짐": 종전엔 `!menu.is_open()`이 always에도 걸렸다(의도 = 메뉴가 열린 동안 hover 변화를
    /// 차단) → always는 설정대로 늘 · hover만 메뉴 중 숨김(메뉴가 마우스를 가져가 hover가 뜻이 없다).
    fn want_plain_funnel(&self, ci: usize, filtered: bool) -> bool {
        self.filter_enabled
            && !filtered
            && match self.funnel_mode {
                FunnelMode::Always => true,
                FunnelMode::Hover => !self.menu.is_open() && self.funnel_hover == Some(ci),
                FunnelMode::None => false,
            }
    }

    /// 값 목록 팝업 표시 행 수(설정 `grid.filter_popup_rows`).
    pub(crate) fn set_filter_popup_rows(&mut self, n: usize) {
        self.filter_popup_rows = n.clamp(3, 40);
    }

    /// 값 목록 범위(설정 `grid.filter_values_scope` · `others` = 다른 필터 통과 행 · `all` = 전체).
    pub(crate) fn set_filter_values_scope(&mut self, scope: &str) {
        self.values_scope_others = scope.trim() != "all";
    }

    /// 설정 `grid.filter_server`(auto | local | ask) — 부분 결과에 필터를 걸 때 서버에서 걸러 다시 조회할지(T-285 · D-257).
    pub(crate) fn set_filter_server(&mut self, mode: &str) {
        self.filter_server = FilterServerMode::parse(mode);
    }

    /// 설정 `grid.filter_fill_pages` — 로컬 필터 중 자동 페치가 더 가져올 최대 페이지 수(T-285 · D-258 · 0 = 안 가져옴).
    pub(crate) fn set_fill_pages(&mut self, n: usize) {
        self.fill_pages = n.min(20);
        self.fill_pages_left = self.fill_pages_left.min(self.fill_pages);
    }

    /// 자체 시험: 남은 채움 페이지 수.
    #[cfg(test)]
    pub(crate) fn fill_pages_left(&self) -> usize {
        self.fill_pages_left
    }

    /// 설정 `grid.col_type_icons` — 열 머리 이름 왼쪽에 유형 아이콘(글/숫자/날짜/참거짓).
    pub(crate) fn set_type_icons(&mut self, on: bool) {
        if self.type_icons != on {
            self.type_icons = on;
            self.dirty = true;
        }
    }

    /// 설정 `editor.quote_idents` — 조건 바에 넣는 열 이름 인용(`always` = 늘 · 아니면 필요할 때만 · `identq`).
    pub(crate) fn set_cond_quote_always(&mut self, on: bool) {
        self.cond_quote_always = on;
    }

    /// 설정 `grid.cond_drop_template` — 열 머리 DnD 때 `AND` 연결 + 타입별 기본값.
    pub(crate) fn set_cond_drop_template(&mut self, on: bool) {
        self.cond.set_drop_template(on);
    }

    /// 설정 `grid.cond_max_lines` — 조건 바를 펼쳤을 때 보이는 최대 줄 수.
    pub(crate) fn set_cond_max_lines(&mut self, n: usize) {
        self.cond.set_max_lines(n);
    }

    /// 조건 바 완성 규칙 = 편집기 인텔리센스 설정(`intel.key_passthrough` · `intel.min_chars`).
    pub(crate) fn set_cond_intel(&mut self, key_passthrough: bool, min_chars: usize) {
        self.cond.set_intel_cfg(key_passthrough, min_chars);
    }

    /// 인라인 조건 입력란 보이기(설정 `grid.condition_bar`) — 높이는 다음 그리기에서 맞춘다.
    pub(crate) fn set_condition_bar(&mut self, on: bool) {
        self.cond_on = on;
        if !on {
            self.cond.set_focused(false);
        }
    }

    /// 조건 바 글(자체 시험 `grid.cond:`).
    pub(crate) fn cond_set_text(&mut self, text: &str) {
        self.cond.set_text(text);
    }

    /// 조건 바 Enter와 같은 길(자체 시험 `grid.cond.run`).
    pub(crate) fn cond_run_now(&mut self) {
        self.cond.request_run();
        self.cond_take_requests();
    }

    pub(crate) fn cond_dump(&self) -> String {
        format!(
            "{} base={} last={}",
            self.cond.dump(),
            self.cond_base.is_some(),
            self.cond_last_sql.as_deref().unwrap_or("")
        )
    }

    /// 조건 바가 남긴 요청을 거둔다 — Enter = 감싼 SQL을 호스트에(같은 탭 재실행) · 복사 = 감싼 SQL 클립보드.
    fn cond_take_requests(&mut self) {
        if let Some(e) = self.cond.take_error() {
            // 검증 실패 = 실행하지 않고 원인만(상자 테두리 빨강 + 상태줄 · 사용자 10-06).
            self.status(tf(Msg::StCondInvalid, &[&e]));
        }
        if let Some(c) = self.cond.take_run() {
            let base = self
                .cond_base
                .clone()
                .unwrap_or_else(|| self.source_sql.clone());
            let sql = crate::condbar::wrap_condition(&base, &c);
            self.cond_base = Some(base);
            self.cond_prev_sql = Some(self.source_sql.clone());
            self.cond_last_sql = Some(sql.clone());
            self.pending_cond_run = Some(sql);
        }
        if self.cond.take_copy_req() {
            let base = self
                .cond_base
                .clone()
                .unwrap_or_else(|| self.source_sql.clone());
            let sql = crate::condbar::wrap_condition(&base, &self.cond.text());
            self.pending_copy = Some((sql, 1));
        }
    }

    /// 호스트: 조건 실행 요청(감싼 SQL · 한 번).
    pub(crate) fn take_cond_run(&mut self) -> Option<String> {
        self.pending_cond_run.take()
    }

    /// ★ 호스트가 걷어 갈 요청이 남아 있는가(페치 · 승격/복귀 재조회 · 재질의) — 입력 사건 없이 생긴 요청(필터 변경 · 페이지 연쇄 ·
    /// 기동 명령)을 틱에서 수거하기 위한 판정(협업 bin50/51 a2·b1 = `req=true fetching=false`로 멈춰 있던 원인).
    pub(crate) fn has_pending_requests(&self) -> bool {
        self.fetch_req.is_some()
            || self.pending_cond_run.is_some()
            || self.pending_requery.is_some()
    }

    /// 호스트: 글 상자 우클릭 메뉴 동작(한 번) — `clip_action`으로 잇는다(복사·잘라내기·붙여넣기 · 사용자 10-06).
    pub(crate) fn take_text_edit_ctx(&mut self) -> Option<nexa_ctl::EditCtxAction> {
        self.pending_edit_ctx.take()
    }

    /// 조건 바 실행이 나가 있는가(오류가 오면 결과를 유지할 것 · Output 탭으로 넘어가지 않을 근거 · 사용자 10-06).
    pub(crate) fn cond_run_pending(&self) -> bool {
        self.cond_prev_sql.is_some()
            && self.cond_last_sql.as_deref() == Some(self.source_sql.as_str())
    }

    /// 호스트: 실행 오류가 왔다 — 그것이 **조건 바 실행**이었으면 결과는 그대로 두고 출처 문장만 직전 것으로 되돌린다(true).
    /// 사용자 10-06 "구문 오류 뒤 결과 창이 비는 경우" — 오류는 토스트·상태줄로만.
    pub(crate) fn cond_run_failed(&mut self) -> bool {
        let Some(prev) = self.cond_prev_sql.take() else {
            return false;
        };
        if self.cond_last_sql.as_deref() != Some(self.source_sql.as_str()) {
            return false;
        }
        self.cond_last_sql = if prev == self.cond_base.clone().unwrap_or_default() {
            None
        } else {
            Some(prev.clone())
        };
        self.set_source_sql(&prev);
        true
    }

    /// 조건 바 실행이 서버에서 거부됐다 — 상자 테두리 빨강(토스트·카드와 별개 · 사용자 10-06).
    pub(crate) fn cond_server_error(&mut self, msg: &str) {
        self.cond.set_server_error(msg);
        self.dirty = true;
    }

    /// 지금 글 입력이 그리드 안 상자(값 목록 검색 · 조건 바)로 가는 상태인가 — 호스트 라우팅·클립보드·표 편집 키 제외의 근거.
    pub(crate) fn text_input_active(&self) -> bool {
        self.vpick.is_open() || (self.cond_on && self.cond.is_focused())
    }

    /// 그 상자가 IME를 바라는가.
    pub(crate) fn text_input_wants_ime(&self) -> bool {
        self.vpick.wants_ime() || (self.cond_on && self.cond.is_focused())
    }

    /// 그 상자(호스트 `focused_textbox`).
    pub(crate) fn text_input_textbox(&mut self) -> Option<&mut TextBox> {
        if self.vpick.is_open() {
            self.vpick.textbox_mut()
        } else if self.cond_on && self.cond.is_focused() {
            Some(self.cond.textbox_mut())
        } else {
            None
        }
    }

    /// 그리드가 포커스를 잃었다 — 조건 바 상자 포커스를 거둔다(값 목록 팝업은 자기 바깥 클릭 규칙으로 닫힌다).
    pub(crate) fn blur_text_input(&mut self) {
        if self.cond.is_focused() {
            self.cond.set_focused(false);
            self.dirty = true;
        }
    }

    /// 상자 글이 상자 밖 길로 바뀐 뒤(IME 확정·붙여넣기) — 다시 거르기/완성.
    pub(crate) fn text_input_query_changed(&mut self) {
        if self.vpick.is_open() {
            self.vpick.query_changed();
            self.dirty = true;
        } else if self.cond_on && self.cond.is_focused() {
            self.cond.query_changed();
            self.dirty = true;
        }
    }

    /// 값 목록 팝업이 떠 있는가(호스트 라우팅 = 팝업이 열려 있으면 마우스·키를 그리드가 먼저 받는다 · 시험).
    pub(crate) fn value_pick_open(&self) -> bool {
        self.vpick.is_open()
    }

    /// 포인터가 그리드를 떠났다(호스트가 그리드 밖 MouseMove에서) — hover 툴팁·칩 hover를 지운다(hover 효과 즉시 취소 규칙 ·
    /// 포커스가 다른 컨트롤일 때는 그리드가 밖의 MouseMove를 못 받아 빗금 표식 툴팁이 남던 잔상 · 10-06). 돌려주는 값 = 지운 것이 있다.
    pub(crate) fn clear_hover_tips(&mut self) -> bool {
        let had =
            self.mark_hover.is_some() || self.chip_hover.is_some() || self.funnel_hover.is_some();
        self.mark_hover = None;
        self.chip_hover = None;
        self.funnel_hover = None;
        self.hdr_hover = None;
        had
    }

    /// 마우스 아래 열 머리 — (열 이름, 머리 칸 사각형) · 호스트의 hover 카드 재료(사용자 10-07).
    pub(crate) fn header_hover(&self) -> Option<(String, Rect)> {
        let (ci, r) = self.hdr_hover?;
        let name = self.rs.as_ref()?.columns().get(ci)?.name.clone();
        Some((name, r))
    }

    pub(crate) fn value_pick_mut(&mut self) -> &mut crate::valuepick::ValuePick {
        &mut self.vpick
    }

    /// 팝업 [적용]과 같은 길(기동 명령 · 시험) — 결과를 바로 소비한다.
    pub(crate) fn value_pick_apply(&mut self) {
        self.vpick.apply();
        if let Some(crate::valuepick::PickResult::Apply { col, values }) = self.vpick.take_result()
        {
            self.set_pick_values(col, &values);
        }
    }

    /// ★ 열 `ci`의 값 목록 팝업 열기(깔때기 클릭 · 빗금 표식 클릭 · 메뉴 "값 목록…" · 기동 명령 `grid.funnel:<열>`) —
    /// 고유값 = 받은 행 전체 · 처음 나온 순서 · NULL 제외 · 상한+1에서 멈춤(넘침 = "값이 더 있음") · 지금 걸린 `=`/IN 값은 체크.
    pub(crate) fn open_value_pick(&mut self, ci: usize) {
        if !self.filter_enabled {
            return;
        }
        let Some(rs) = self.rs.as_ref() else {
            return;
        };
        if ci >= rs.columns().len() || self.row_h <= 0 {
            return;
        }
        // ★ D-256(T-284 1단계 · 10-07): 값 목록 = 가져온 행 **전부**(상한 없음 · 팝업은 보이는 행만 그린다 = 길이와 그리기 비용 무관) ·
        //   "더 있음" = 서버에 아직 안 가져온 행이 있다(부분 결과). 남은 단계 = DistinctIndex(문자열 복사 0 · 시간 예산) · [서버에서 값 읽기].
        // ★ 값 목록 범위(사용자 10-06): 기본 = **다른 열**의 필터(AND)를 통과한 행의 값만 — 1번 열을 고르고 나면 2번 열 목록은
        //   그 안에서 고를 수 있는 값만 보인다(엑셀 자동 필터와 같다). 자기 열의 필터는 빼야 이미 고른 값도 체크된 채 보인다.
        //   OR 결합이면 "다른 필터 통과"가 뜻이 없어 전체.
        let others: Vec<&Predicate> = self
            .filters
            .iter()
            .filter(|p| p.col != ci && p.col < rs.columns().len())
            .collect();
        let cascade = self.values_scope_others && !self.filter_or && !others.is_empty();
        let values = distinct_capped(
            (0..rs.len())
                .filter(|&r| !cascade || others.iter().all(|p| p.pass_value(rs.cell(r, p.col))))
                .map(|r| rs.cell(r, ci))
                .filter(|v| !matches!(v, Value::Null))
                .map(|v| cell_text(v, "")),
            usize::MAX - 1,
        );
        let more = self.more;
        let chosen: Vec<String> = self
            .filters
            .iter()
            .find(|p| p.col == ci && matches!(p.op, FilterOp::Eq | FilterOp::In))
            .map(list_items)
            .unwrap_or_default();
        let pos = self.col_order.iter().position(|&c| c == ci).unwrap_or(0);
        // 열이 화면 밖이면 보이게 가로 스크롤(기동 명령·메뉴 경로 · 협업 V1 ⑥ 10-06).
        let mut cx = 0;
        for (p, &c) in self.col_order.iter().enumerate() {
            let w = self.col_w.get(c).copied().unwrap_or(80);
            if p == pos {
                let vis_w = (self.bounds.w - self.gutter_w).max(1);
                if cx < self.scroll_x {
                    self.scroll_x = cx;
                } else if cx + w > self.scroll_x + vis_w {
                    self.scroll_x = cx + w - vis_w;
                }
                break;
            }
            cx += w;
        }
        self.clamp();
        let hdr = self.header_rect();
        let cw = self.col_w.get(ci).copied().unwrap_or(80);
        let anchor = Rect::new(self.col_x(pos), hdr.y, cw, hdr.h);
        let host = self.menu_host();
        self.menu.close();
        self.mark_hover = None;
        self.vpick.open(
            ci,
            anchor,
            values,
            more,
            &chosen,
            host,
            self.live_scale,
            self.row_h,
            self.filter_popup_rows,
        );
    }

    /// 값 목록 팝업의 결과 — 열 `col`의 값 필터를 `values`로(하나 = `=` · 여럿 = IN · 비면 그 열 필터 제거).
    pub(crate) fn set_pick_values(&mut self, col: usize, values: &[String]) {
        self.filters.retain(|p| p.col != col);
        if values.is_empty() {
            self.apply_sort();
            self.after_filters_changed();
            return;
        }
        let op = if values.len() == 1 {
            FilterOp::Eq
        } else {
            FilterOp::In
        };
        // 값에 쉼표·`|`가 있으면 인용해 묶는다(`"BOX,POWER PULSE"` · 사용자 10-06 "1개 골랐는데 0건").
        let value = if op == FilterOp::Eq {
            values[0].clone()
        } else {
            join_list(values)
        };
        self.add_filter(col, op, value);
    }

    /// 열의 고유값(받은 행 전체 · 처음 나온 순서 · NULL 제외) — 상한+1개까지(넘침 표시용). 메뉴를 열 때와 고를 때 같은 계산이라
    /// 번호가 맞는다(그 사이 행이 바뀌면 다음 메뉴에서 다시 센다).
    fn column_values(&self, ci: usize) -> Vec<String> {
        let Some(rs) = self.rs.as_ref() else {
            return Vec::new();
        };
        if ci >= rs.columns().len() {
            return Vec::new();
        }
        distinct_capped(
            (0..rs.len())
                .map(|r| rs.cell(r, ci))
                .filter(|v| !matches!(v, Value::Null))
                .map(|v| cell_text(v, "")),
            self.filter_pick_max,
        )
    }

    /// 필터 줄 켜기/끄기(설정 `grid.filter_strip`) — 높이는 다음 그리기에서 맞춘다.
    pub(crate) fn set_filter_strip(&mut self, on: bool) {
        self.filter_strip = on;
    }

    /// 필터 줄 영역(없으면 높이 0).
    fn strip_rect(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x, b.y - self.strip_h, b.w, self.strip_h)
    }

    /// 조건 바 자리(줄의 위쪽).
    fn cond_rect(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x, b.y - self.strip_h, b.w, self.cond_h)
    }

    /// 필터 칩 줄 자리(조건 바 **아래** · 그리드 바로 위 · 사용자 10-06 "조건 바 밑에 필터 바").
    fn chips_rect(&self) -> Rect {
        let b = self.bounds;
        Rect::new(b.x, b.y - self.chip_h, b.w, self.chip_h)
    }

    /// 필터 유무·설정에 맞춰 필터 줄 높이를 맞춘다(바뀔 때만 `bounds`를 옮긴다 · 그리기 직전 한 곳).
    fn sync_strip(&mut self, s: f32) {
        // 위 = 조건 바(켜져 있을 때) · 아래 = 필터 칩 줄(설정 + 필터 있음) — 둘 다 결과가 있을 때만.
        let cond_h = if self.rs.is_some() && self.cond_on {
            self.cond.wanted_height(s)
        } else {
            0
        };
        let chip_h = if self.rs.is_some() && self.filter_strip && !self.filters.is_empty() {
            (26.0 * s).round() as i32
        } else {
            0
        };
        let want = cond_h + chip_h;
        if want == self.strip_h && cond_h == self.cond_h {
            self.chip_h = chip_h;
            return;
        }
        self.cond_h = cond_h;
        self.chip_h = chip_h;
        let o = self.strip_rect();
        let outer = Rect::new(o.x, o.y, o.w, self.bounds.h + self.strip_h);
        self.strip_h = want;
        self.set_bounds(outer);
        if want == 0 {
            self.chips.clear();
            self.chips_clear = None;
            self.chip_hover = None;
        }
    }

    /// 필터 줄 그리기 — 술어마다 칩 하나(`열 조건 ×`) · 넘치면 `+N` · 오른쪽 끝 × = 모두 지우기.
    fn paint_filter_strip(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        self.chips.clear();
        self.chips_clear = None;
        if self.strip_h <= 0 {
            return;
        }
        // ★ 인라인 조건 입력란(사용자 10-06) = 위 · 필터 칩 줄(설정 `grid.filter_strip` · 필터 있을 때만) = 그 **아래** 두 번째 줄.
        //   호스트가 편집기 글꼴로 따로 그리면(`cond_host_paint`) 여기서는 건너뛴다.
        if !self.cond_host_paint {
            self.paint_cond(dc, th, s);
        }
        let strip = self.chips_rect();
        if strip.h <= 0 {
            return;
        }
        let px = |v: f32| (v * s).round() as i32;
        dc.fill_rect(strip, th.chrome_bg);
        dc.fill_rect(
            Rect::new(strip.x, strip.bottom() - 1, strip.w, 1),
            th.border,
        );
        dc.select_font(FontSlot::Status, false);
        let (pad, gap, m) = (px(8.0), px(6.0), px(3.0));
        let ch = strip.h - 1 - m * 2;
        let cy = strip.y + m;
        let xw = ch; // × 자리 = 정사각형
                     // 오른쪽 끝 "모두 지우기".
        let clear = Rect::new(strip.right() - pad - xw, cy, xw, ch);
        let right = clear.x - gap;
        let ty = dc.text_center_y(cy, ch);
        let draw_x = |dc: &mut dyn DrawCtx, r: Rect, hot: bool| {
            if hot {
                dc.fill_round_rect_alpha(r, r.h / 2, th.text, 0.14);
            }
            let w = dc.text_width("×");
            dc.text(
                r.x + (r.w - w) / 2,
                ty,
                r,
                "×",
                if hot { th.text } else { th.text_dim },
            );
        };
        let labels: Vec<(usize, String)> = self
            .filters
            .iter()
            .map(|p| (p.col, self.chip_label(p)))
            .collect();
        let mut x = strip.x + pad;
        let mut hidden = 0usize;
        for (i, (col, label)) in labels.iter().enumerate() {
            let tw = dc.text_width(label);
            let w = pad + tw + px(2.0) + xw + px(2.0);
            if x + w > right && i > 0 || hidden > 0 {
                hidden += 1;
                continue;
            }
            let w = w.min((right - x).max(xw * 2));
            let chip = Rect::new(x, cy, w, ch);
            // 머티리얼 입력 칩 — 옅은 강조색 채움 · 외곽선 없음.
            dc.fill_round_rect_alpha(chip, ch / 2, th.accent, 0.16);
            let close = Rect::new(chip.right() - xw - px(2.0), cy, xw, ch);
            let text_clip = Rect::new(chip.x + pad, cy, (close.x - chip.x - pad).max(0), ch);
            dc.text(text_clip.x, ty, text_clip, label, th.text);
            draw_x(dc, close, self.chip_hover == Some(i));
            self.chips.push((*col, chip, close));
            x = chip.right() + gap;
            // OR 결합이면 칩 사이에 작은 "OR"(AND는 표시하지 않는다 = 기본).
            if self.filter_or && i + 1 < labels.len() {
                let orw = dc.text_width("OR");
                dc.text(x, ty, Rect::new(x, cy, orw, ch), "OR", th.text_dim);
                x += orw + gap;
            }
        }
        if hidden > 0 {
            let more = format!("+{hidden}");
            dc.text(
                x,
                ty,
                Rect::new(x, cy, (right - x).max(0), ch),
                &more,
                th.text_dim,
            );
        }
        draw_x(dc, clear, self.chip_hover == Some(usize::MAX));
        self.chips_clear = Some(clear);
        dc.select_font(FontSlot::Base, false);
    }

    /// 조건 바 그리기 — 호스트가 **편집기 글꼴 컨텍스트**(고정폭 · `editor.font_size`)로 부른다(사용자 10-07: 편집기 탭과 글자 크기·
    /// 줄 간격이 같아 보이게 · 줄 높이 20×배율은 TextBox 공통이라 글꼴만 맞추면 된다). 결과가 없거나 꺼져 있으면 아무것도 안 한다.
    pub(crate) fn paint_cond(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        if !(self.cond_on && self.cond_h > 0) {
            return;
        }
        let host = self.menu_host();
        self.cond.set_host(host);
        self.cond.set_rect(self.cond_rect(), s);
        self.cond.paint(dc, th, std::time::Instant::now());
    }

    /// 호스트가 조건 바를 따로 그린다고 알린다(`paint_cond` · 매 프레임 그리기 전에).
    pub(crate) fn set_cond_host_paint(&mut self, on: bool) {
        self.cond_host_paint = on;
    }

    /// 칩 글 = `열 이름  조건`(값이 길면 줄인다).
    fn chip_label(&self, p: &Predicate) -> String {
        let name = self
            .rs
            .as_ref()
            .and_then(|rs| rs.columns().get(p.col).map(|c| c.name.clone()))
            .unwrap_or_default();
        let mut cond = pred_text(p);
        if cond.chars().count() > 40 {
            cond = cond.chars().take(39).collect::<String>() + "…";
        }
        format!("{name} {cond}")
    }

    /// 필터 줄의 마우스 — 커서 아래 ×만 반응 · 줄 안의 클릭은 셀로 흘리지 않는다. 돌려주는 값 = 먹었는가.
    fn strip_event(&mut self, ev: &InputEvent) -> bool {
        if self.strip_h <= 0 {
            return false;
        }
        if self.cond_on {
            let mut inv = Invalidations::default();
            let hit = self.cond.on_event(ev, &mut inv);
            if let Some(a) = self.cond.take_edit_ctx() {
                self.pending_edit_ctx = Some(a);
            }
            self.cond_take_requests();
            if self.cond.menu_open() || hit {
                self.dirty = true;
            }
            if hit {
                return true;
            }
        }
        // 필터 칩 줄(조건 바 아래).
        let strip = self.chips_rect();
        if strip.h <= 0 {
            return false;
        }
        let hit = |p: Point, chips: &[(usize, Rect, Rect)], clear: Option<Rect>| -> Option<usize> {
            if clear.is_some_and(|r| r.contains(p)) {
                return Some(usize::MAX);
            }
            chips.iter().position(|(_, _, x)| x.contains(p))
        };
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                self.chip_hover = if strip.contains(p) {
                    hit(p, &self.chips, self.chips_clear)
                } else {
                    None
                };
                strip.contains(p)
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if !strip.contains(p) {
                    return false;
                }
                match hit(p, &self.chips, self.chips_clear) {
                    Some(usize::MAX) => self.clear_filters(),
                    Some(i) => {
                        if let Some(&(col, _, _)) = self.chips.get(i) {
                            self.remove_filter(col);
                        }
                    }
                    None => {}
                }
                self.chip_hover = None;
                true
            }
            InputEvent::MouseUp { x, y, .. } => strip.contains(Point { x, y }),
            _ => false,
        }
    }

    /// 한 열의 필터 지우기(재투영 · 맨 위로).
    pub(crate) fn remove_filter(&mut self, col: usize) {
        let n = self.filters.len();
        self.filters.retain(|p| p.col != col);
        if self.filters.len() != n {
            self.apply_sort();
            self.after_filters_changed();
            self.scroll_y = 0;
        }
    }

    /// 자체 시험 덤프 — 필터 줄 칩 글(줄마다 하나) · 줄이 없으면 빈 글.
    pub(crate) fn dump_chips(&self) -> String {
        if self.strip_h <= 0 {
            return String::new();
        }
        self.filters
            .iter()
            .map(|p| self.chip_label(p) + "\n")
            .collect()
    }

    pub(crate) fn set_result(&mut self, rs: ResultSet) {
        self.auto_fetch_all = false;
        self.cond
            .set_columns(rs.columns.iter().map(|c| c.name.clone()).collect());
        self.cond.set_dialect(self.dialect);
        let t = std::time::Instant::now();
        // 재조회 = 조회 컬럼 순서·정렬 초기화(이동·정렬 결과 무시 — 사용자 09-14).
        self.col_order = (0..rs.columns.len()).collect();
        self.row_order = (0..rs.rows.len()).collect();
        self.sort_keys.clear();
        // 새 조회 = 필터도 초기화(사용자 09-29 "모든 컬럼이 초기화") · 자동 페치 잠금 해제.
        // ★ 단 **서버 승격 재조회**(T-285)의 결과면 필터 모델(칩)은 그대로 — 지우면 복귀가 성립해야 한다(협업 bin49 a2). 다른 새 실행이
        //   들어오면 `set_result_origin`이 비운다.
        if !self.filter_promoted {
            self.filters.clear();
        }
        self.menu_cell = None;
        self.type_badges.clear();
        self.auto_fetch_at = None;
        self.hdr_drag = None;
        self.hdr_resize = None;
        // 한 세트(DR-33): 덩어리를 이동해 첫 세그먼트로(복사 0). 옛 데이터는 여기서 drop(변환 스레드가 쥔 세그먼트는 그쪽이 끝나면).
        self.rs = Some(ResultData::new(rs));
        self.edit = None;
        self.read_only = None;
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

    /// 호스트 주입: Shift 눌림(Tab = 오른쪽 · Shift+Tab = 왼쪽 · 셀 편집기 안팎 공통 · 87 §11).
    pub(crate) fn set_shift(&mut self, on: bool) {
        self.shift = on;
    }

    /// 호스트 주입: 키보드 포커스가 그리드에 있는가 — 없으면 선택색을 비활성(절반 알파)으로(WinUI·IntelliJ·KDE · 88 §3 2).
    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
    }

    /// 선택 배경 알파(행 포커스 띠 · 셀 선택) — 포커스 밖이면 절반(순수 함수 · 시험용).
    pub(crate) fn sel_alphas(focused: bool, row_alpha: f32) -> (f32, f32) {
        if focused {
            (row_alpha, 0.85)
        } else {
            (row_alpha * 0.5, 0.45)
        }
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

    /// 열린 우클릭 메뉴 영역(하위 메뉴 포함 · 닫혀 있으면 빈 Rect).
    pub(crate) fn menu_bounds(&self) -> Rect {
        self.menu.bounds()
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
        // ★ 편집 중이면 덧그린 값·추가 행을 포함한 **작은 스냅숏**을 만들어 같은 렌더러로(선택 크기만큼만 · DR-33 세트는 그대로).
        let n_src = rs.len();
        let overlay: Option<ResultSet> = if self.edit_dirty() || rows.iter().any(|&r| r >= n_src) {
            let columns: Vec<nsql_core::Column> = cols
                .iter()
                .filter_map(|&c| rs.columns().get(c).cloned())
                .collect();
            let data: Vec<Vec<Value>> = rows
                .iter()
                .map(|&ri| {
                    let rref = if ri < n_src {
                        RowRef::Existing(ri)
                    } else {
                        RowRef::Inserted(ri - n_src)
                    };
                    cols.iter()
                        .map(
                            |&c| match self.edit.as_ref().and_then(|e| e.cs.cell(rref, c)) {
                                Some(Some(s)) => Value::Str(s.clone()),
                                Some(None) => Value::Null,
                                None => rs
                                    .row(ri)
                                    .and_then(|row| row.get(c))
                                    .cloned()
                                    .unwrap_or(Value::Null),
                            },
                        )
                        .collect()
                })
                .collect();
            Some(ResultSet {
                columns,
                rows: data,
            })
        } else {
            None
        };
        if let Some(snap) = overlay.as_ref() {
            let view = View::new(snap);
            return self.render_copy(kind, &view, rows.len(), first);
        }
        let view = View::new(rs).rows(&rows).cols(&cols);
        self.render_copy(kind, &view, rows.len(), first)
    }

    fn render_copy<S: nsql_core::RowSource + ?Sized>(
        &self,
        kind: CopyKind,
        view: &S,
        nrows: usize,
        first: bool,
    ) -> Option<(String, usize)> {
        let rows_len = nrows;
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
        let layout = nsql_io::block_layout(view, &opts);
        let out = nsql_io::render_block(
            view,
            &fmt,
            self.dialect,
            &layout,
            0..rows_len,
            header,
            true,
            "T",
            &KeySpec::default(),
        );
        // ★ 복사에는 끝 줄바꿈을 붙이지 않는다(사용자 09-17): 렌더러는 블록(줄마다 `\n`)이라 마지막 행 뒤에도 `\n`이 남는데,
        //   단일 셀/행을 붙여넣을 때 줄바꿈이 따라오면 안 된다 · 여러 행은 행 사이 줄바꿈만.
        let out = out.trim_end_matches(['\n', '\r']).to_string();
        Some((out, rows_len * view.ncols()))
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

    /// 셀/행 드래그 선택 중인가 — 호스트가 어디서 놓든 MouseUp을 넘겨 준다(편집기와 같은 규칙 · 사용자 09-29).
    pub(crate) fn dragging(&self) -> bool {
        self.drag_sel.is_some()
    }

    /// 필터 항목(셀 메뉴의 하위 · 헤더 메뉴의 본문 공용): 값이 있으면 "이 값만/제외" · 포함… · NULL · 열/전체 지우기 · 조회 SQL 복사.
    /// 조건 ▸ 항목(사용자 10-07 · 필터 메뉴와 같은 꼴): 값이 있으면 `= 값` · `<> 값` · (문자열) 시작/포함/끝 · 늘 IS NULL / IS NOT NULL.
    fn cond_menu_items(&self, ci: usize, val: Option<&str>) -> Vec<CtxItem> {
        let short = |s: &str| {
            let mut t: String = s.chars().take(24).collect();
            if s.chars().count() > 24 {
                t.push('…');
            }
            t
        };
        let kind = self.col_kind(ci);
        let mut f = Vec::new();
        if let Some(v) = val.filter(|_| kind != ColKind::Bool) {
            f.push(CtxItem::item("cond.eq", tf(Msg::MnCondEq, &[&short(v)])));
            f.push(CtxItem::item("cond.ne", tf(Msg::MnCondNe, &[&short(v)])));
            if kind == ColKind::Text {
                f.push(CtxItem::item(
                    "cond.starts",
                    tf(Msg::MnCondStarts, &[&short(v)]),
                ));
                f.push(CtxItem::item(
                    "cond.contains",
                    tf(Msg::MnCondContains, &[&short(v)]),
                ));
                f.push(CtxItem::item(
                    "cond.ends",
                    tf(Msg::MnCondEnds, &[&short(v)]),
                ));
            }
            f.push(CtxItem::Separator);
        }
        f.push(CtxItem::item("cond.null", t(Msg::MnCondNull)));
        f.push(CtxItem::item("cond.notnull", t(Msg::MnCondNotNull)));
        f
    }

    /// 조건 ▸ 선택 — 술어 글을 만들어 조건 바 끝에 AND로 붙이고 상자에 포커스(실행은 Enter · 여러 개를 모아 한 번에 보낼 수 있게).
    fn cond_pick(&mut self, what: &str) {
        let Some((ci, val)) = self.menu_cell.clone() else {
            return;
        };
        let Some(name) = self
            .rs
            .as_ref()
            .and_then(|rs| rs.columns().get(ci).map(|c| c.name.clone()))
        else {
            return;
        };
        let kind = self.col_kind(ci);
        // 열 이름 인용 = DnD와 **같은 정책**(`identq::quote` · 사용자 10-07 "동일한 기준").
        let col = crate::identq::quote(self.dialect, &name, self.cond_quote_always);
        if let Some(sql) = cond_pred_sql(&col, kind, what, val.as_deref()) {
            self.cond.append_condition(&sql);
            self.dirty = true;
        }
    }

    fn filter_menu_items(&self, ci: usize, val: Option<&str>) -> Vec<CtxItem> {
        let short = |s: &str| {
            let mut t: String = s.chars().take(24).collect();
            if s.chars().count() > 24 {
                t.push('…');
            }
            t
        };
        let has_col = self.filters.iter().any(|p| p.col == ci);
        let any = !self.filters.is_empty();
        let kind = self.col_kind(ci);
        let mut f = Vec::new();
        // 값 기준(우클릭한 셀) — 불리언은 참/거짓 항목이 대신한다.
        // ★ 여러 셀을 골랐으면(2행 이상) 그 열의 **선택된 값 N개**를 한꺼번에(사용자 10-07 "이 값만: 3개 선택") = IN / NOT IN.
        let sel_vals = self.selected_values(ci);
        if kind != ColKind::Bool && sel_vals.len() > 1 {
            let n = sel_vals.len().to_string();
            f.push(CtxItem::item(
                "filter.eq_sel",
                tf(Msg::MnFilterEqMany, &[&n]),
            ));
            f.push(CtxItem::item(
                "filter.ne_sel",
                tf(Msg::MnFilterNeMany, &[&n]),
            ));
        } else if let Some(v) = val.filter(|_| kind != ColKind::Bool) {
            f.push(CtxItem::item(
                "filter.eq",
                tf(Msg::MnFilterEq, &[&short(v)]),
            ));
            f.push(CtxItem::item(
                "filter.ne",
                tf(Msg::MnFilterNe, &[&short(v)]),
            ));
        }
        match kind {
            ColKind::Text => {
                f.push(CtxItem::item("filter.contains", t(Msg::MnFilterContains)));
                f.push(CtxItem::item("filter.starts", t(Msg::MnFilterStarts)));
            }
            ColKind::Number => {
                f.push(CtxItem::item("filter.gt", t(Msg::MnFilterGt)));
                f.push(CtxItem::item("filter.lt", t(Msg::MnFilterLt)));
                f.push(CtxItem::item("filter.between", t(Msg::MnFilterBetween)));
            }
            ColKind::Date => {
                f.push(CtxItem::item("filter.ge", t(Msg::MnFilterAfter)));
                f.push(CtxItem::item("filter.le", t(Msg::MnFilterBefore)));
                f.push(CtxItem::item("filter.between", t(Msg::MnFilterBetween)));
            }
            ColKind::Bool => {
                f.push(CtxItem::item("filter.true", t(Msg::MnFilterTrue)));
                f.push(CtxItem::item("filter.false", t(Msg::MnFilterFalse)));
            }
        }
        if kind != ColKind::Bool {
            f.push(CtxItem::item("filter.in", t(Msg::MnFilterIn)));
            // ★ 값 고르기(T-181 · 77 §2-2 "깔때기 → 값 목록"): 열의 고유값을 체크 항목으로 — 고르면 `=`/값 목록(IN) 술어에 넣고 빼기.
            let values = self.column_values(ci);
            if !values.is_empty() {
                let chosen: Vec<String> = self
                    .filters
                    .iter()
                    .find(|p| p.col == ci && matches!(p.op, FilterOp::Eq | FilterOp::In))
                    .map(list_items)
                    .unwrap_or_default();
                let mut kids: Vec<CtxItem> = values
                    .iter()
                    .take(self.filter_pick_max)
                    .enumerate()
                    .map(|(n, v)| {
                        CtxItem::item(format!("filter.pick:{n}"), short(v))
                            .with_checked(chosen.iter().any(|c| c.eq_ignore_ascii_case(v)))
                    })
                    .collect();
                if values.len() > self.filter_pick_max {
                    kids.push(CtxItem::maybe(
                        "filter.pick_more",
                        t(Msg::MnFilterPickMore),
                        false,
                    ));
                }
                f.push(CtxItem::submenu("filter.pick", t(Msg::MnFilterPick), kids));
            }
        }
        // ★ 값 목록 팝업(검색 상자 + 체크 목록 · 깔때기 아이콘과 같은 길).
        f.push(CtxItem::item("filter.values", t(Msg::MnFilterValues)));
        f.push(CtxItem::item("filter.regex", t(Msg::MnFilterRegex)));
        f.push(CtxItem::item("filter.null", t(Msg::MnFilterNull)));
        f.push(CtxItem::item("filter.notnull", t(Msg::MnFilterNotNull)));
        f.push(CtxItem::Separator);
        // 결합 방식(T-181 · OR): 술어가 둘 이상일 때 뜻이 있다 · 체크 = 지금 OR.
        f.push(
            CtxItem::maybe(
                "filter.or",
                t(Msg::MnFilterOr),
                self.filters.len() > 1 || self.filter_or,
            )
            .with_checked(self.filter_or),
        );
        f.push(CtxItem::maybe(
            "filter.clear_col",
            t(Msg::MnFilterClearCol),
            has_col,
        ));
        f.push(CtxItem::maybe(
            "filter.clear",
            t(Msg::MnFilterClearAll),
            any,
        ));
        f.push(CtxItem::maybe(
            "filter.copy_query",
            t(Msg::MnFilterCopyQuery),
            any && !self.source_sql().trim().is_empty(),
        ));
        // ★ 서버 재조회(T-181 · 77 §2-2): 같은 조회용 Query를 복사 대신 **새 결과 탭**으로 실행 — 받은 행까지만 보던 필터를 서버 전체에.
        f.push(CtxItem::maybe(
            "filter.requery",
            t(Msg::MnFilterRequery),
            any && !self.source_sql().trim().is_empty(),
        ));
        f
    }

    /// ★ 헤더(컬럼명) 우클릭 = 그 열의 필터 메뉴(사용자 09-29): 값 항목 없이 포함…/NULL/지우기/조회 SQL 복사 + 정렬.
    fn open_header_menu(&mut self, ci: usize, x: i32, y: i32, scale: f32) {
        self.menu.set_scale(scale);
        self.menu_cell = Some((ci, None));
        // 필터 기능을 껐으면(`grid.filter_enabled`) 필터 항목 전부 없음(사용자 10-06 · 협업 V1 ③a).
        let mut items = if self.filter_enabled {
            let mut f = self.filter_menu_items(ci, None);
            f.push(CtxItem::Separator);
            f
        } else {
            Vec::new()
        };
        let sorted = self
            .sort_keys
            .iter()
            .find(|(c, _)| *c == ci)
            .map(|(_, asc)| *asc);
        items.push(CtxItem::item("sort.asc", t(Msg::MnSortAsc)).with_checked(sorted == Some(true)));
        items.push(
            CtxItem::item("sort.desc", t(Msg::MnSortDesc)).with_checked(sorted == Some(false)),
        );
        items.push(CtxItem::maybe(
            "sort.clear",
            t(Msg::MnSortClear),
            !self.sort_keys.is_empty(),
        ));
        // 출처 테이블을 아는 결과(단일 테이블 조회)만 — 조인·식 결과는 흐리게(T-180 ⑤).
        items.push(CtxItem::Separator);
        items.push(CtxItem::maybe(
            "obj.reveal",
            t(Msg::MnObjLinkReveal),
            self.reveal_table().is_some(),
        ));
        let text_w = (self.row_h * 10).max(180);
        self.menu_is_view = false;
        self.menu.open_at(x, y, items, self.menu_host(), text_w);
    }

    /// 열 머리 → 객체 탐색기의 대상 테이블 — 출처 문장이 **단일 테이블 SELECT**일 때만(편집 판정과 같은 분석
    /// `gridedit_sql::analyze`). `source_table`(SQL 복사용 추정)은 조인이어도 첫 테이블을 주므로 그대로 쓰면
    /// `SELECT d.name FROM emp e JOIN dept d …`의 `name`이 emp의 열로 풀린다(10-05 자체 시험에서 드러남).
    pub(crate) fn reveal_table(&self) -> Option<String> {
        gridedit_sql::analyze(&self.source_sql).ok()?;
        self.source_table.clone()
    }

    /// 외래 키 열 목록(호스트가 메타에서 · 모르면 빈 목록).
    pub(crate) fn set_fk_cols(&mut self, cols: Vec<String>) {
        self.fk_cols = cols.into_iter().map(|c| c.to_ascii_uppercase()).collect();
    }

    /// 술어 결합을 바꾼다(AND ↔ OR) → 재투영 · 맨 위로.
    pub(crate) fn set_filter_or(&mut self, or: bool) {
        if self.filter_or != or {
            self.filter_or = or;
            self.apply_sort();
            self.scroll_y = 0;
        }
    }

    /// 필터 메뉴 "필터로 서버 재조회"가 남긴 조회용 Query(1회성).
    pub(crate) fn take_requery(&mut self) -> Option<String> {
        self.pending_requery.take()
    }

    /// 셀 메뉴 "참조 행 보기"가 남긴 (원본 행, 열 이름)(1회성).
    pub(crate) fn take_follow(&mut self) -> Option<(usize, String)> {
        self.pending_follow.take()
    }

    /// 원본 행 `r`의 이름 있는 열 값들(이름 대소문자 무시 · 없는 열은 뺀다) — 외래 키 조회의 입력.
    pub(crate) fn row_values(&self, r: usize, cols: &[String]) -> Vec<(String, Value)> {
        let Some(rs) = self.rs.as_ref() else {
            return Vec::new();
        };
        if r >= rs.len() {
            return Vec::new();
        }
        cols.iter()
            .filter_map(|want| {
                rs.columns()
                    .iter()
                    .position(|c| c.name.eq_ignore_ascii_case(want))
                    .map(|ci| (want.clone(), rs.cell(r, ci).clone()))
            })
            .collect()
    }

    /// 열 머리 메뉴 "객체 탐색기에서 보기"가 남긴 열 이름(1회성) — 호스트가 `source_table()`과 묶어 탐색기에서 찾는다.
    pub(crate) fn take_reveal(&mut self) -> Option<String> {
        self.pending_reveal.take()
    }

    /// 헤더의 정렬/필터 표식 사각형(열 원본 번호 → 마지막 그리기의 화면 좌표 · 표식이 없으면 None).
    fn mark_rect_of(&self, ci: usize) -> Option<Rect> {
        self.mark_rects
            .iter()
            .find(|(c, _)| *c == ci)
            .map(|(_, r)| *r)
    }

    /// 표식 툴팁 본문: 열 이름 + 적용된 술어 한 줄씩.
    fn filter_tip(&self, ci: usize) -> Option<String> {
        let rs = self.rs.as_ref()?;
        let name = rs.columns().get(ci)?.name.clone();
        let lines: Vec<String> = self
            .filters
            .iter()
            .filter(|p| p.col == ci)
            .map(pred_text)
            .collect();
        if lines.is_empty() {
            return None;
        }
        Some(format!(
            "{}\n{}",
            tf(Msg::TipFilterHead, &[&name]),
            lines.join("\n")
        ))
    }

    /// 메뉴 배치 영역(창 전체 · 없으면 그리드 영역).
    fn menu_host(&self) -> Rect {
        if self.menu_area.h > 0 {
            self.menu_area
        } else {
            self.bounds
        }
    }

    /// 우클릭 메뉴가 펼쳐질 수 있는 영역(창 전체 · 호스트가 배치 때).
    pub(crate) fn set_menu_area(&mut self, r: Rect) {
        self.menu_area = r;
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
        // ★ 서브메뉴는 1단(88 §3 15 · T-233): Copy SQL ▸ 은 Advanced ▸ 안이 아니라 같은 층에.
        let adv = vec![
            // 형식마다 다른 도형(T-303 첫 묶음 · 사용자 10-07 "CSV·TEXT·Markdown부터" · 종전 셋 다 표 아이콘).
            CtxItem::item("copy_csv", t(Msg::MnCopyCsv)).with_icon(Some(toolicons::mi_csv())),
            CtxItem::item("copy_txt", t(Msg::MnCopyText)).with_icon(Some(toolicons::mi_text())),
            CtxItem::item("copy_md", t(Msg::MnCopyMarkdown))
                .with_icon(Some(toolicons::mi_markdown())),
            CtxItem::item("copy_json", t(Msg::MnCopyJson)).with_icon(Some(toolicons::mi_braces())),
        ];
        let mut adv_item = CtxItem::submenu("adv", t(Msg::MnAdvancedCopy), adv);
        if let CtxItem::Item { enabled, .. } = &mut adv_item {
            *enabled = has;
        }
        let mut sql_item = CtxItem::submenu("copy_sql", t(Msg::MnCopySql), sql)
            .with_icon(Some(toolicons::mi_db()));
        if let CtxItem::Item { enabled, .. } = &mut sql_item {
            *enabled = has;
        }
        let mut items = vec![
            CtxItem::maybe("copy", t(Msg::MnCopy), has)
                .with_icon(Some(toolicons::mi_copy()))
                .with_shortcut(self.sc_copy.clone()),
            CtxItem::maybe("copy_h", t(Msg::MnCopyWithHeaders), has)
                .with_icon(Some(toolicons::mi_copy())),
            adv_item,
            sql_item,
            CtxItem::Separator,
            CtxItem::item("all", t(Msg::MnSelectAll))
                .with_icon(Some(toolicons::mi_select_all()))
                .with_shortcut(self.sc_all.clone()),
        ];
        // ★ 필터 ▸(T-181 · 77 §2-2): 우클릭한 셀의 열·값 기준 · 술어 AND 목록 · 조회용 SQL 복사.
        if let Some((ci, val)) = self.menu_cell.clone() {
            // 필터 기능을 껐으면(`grid.filter_enabled`) 필터 ▸ 자체가 없다(사용자 10-06).
            if self.filter_enabled {
                let any = !self.filters.is_empty();
                let f = self.filter_menu_items(ci, val.as_deref());
                items.push(CtxItem::Separator);
                let mut fi = CtxItem::submenu("filter", t(Msg::MnFilter), f);
                if let CtxItem::Item { emph, .. } = &mut fi {
                    *emph = any;
                }
                items.push(fi);
            }
            // ★ 조건 ▸(사용자 10-07): 셀 값 기준 술어를 **조건 바**(서버 WHERE)에 AND로 덧붙인다 — 조건 바가 켜져 있을 때만.
            if self.cond_on {
                let c = self.cond_menu_items(ci, val.as_deref());
                if !c.is_empty() {
                    items.push(CtxItem::submenu("cond", t(Msg::MnCond), c));
                }
            }
            // ★ 외래 키 따라가기(T-180 ⑥): 단일 테이블 결과 · 외래 키 열 · 값 있음일 때만 활성.
            if let Some(table) = self.reveal_table() {
                let _ = table;
                let is_fk = self
                    .rs
                    .as_ref()
                    .and_then(|rs| rs.columns().get(ci))
                    .is_some_and(|c| self.fk_cols.iter().any(|f| f.eq_ignore_ascii_case(&c.name)));
                items.push(CtxItem::maybe(
                    "obj.follow_fk",
                    t(Msg::MnFollowFk),
                    is_fk && val.is_some(),
                ));
            }
        }
        // ★ 편집(docs/87 §6): 편집 가능한 결과에만 · 값 보기는 늘.
        items.push(CtxItem::Separator);
        items.push(CtxItem::maybe(
            "grid.edit.view_value",
            t(Msg::MnGeViewValue),
            has,
        ));
        if let Some(e) = self.edit.as_ref() {
            let can = !e.applying;
            let dirty = e.cs.is_dirty();
            items.push(CtxItem::maybe(
                "grid.edit.set_null",
                t(Msg::MnGeSetNull),
                can && has,
            ));
            items.push(CtxItem::maybe(
                "grid.edit.dup_row",
                t(Msg::MnGeDupRow),
                can && has,
            ));
            items.push(CtxItem::maybe(
                "grid.edit.insert_row",
                t(Msg::MnGeInsertRow),
                can,
            ));
            items.push(CtxItem::maybe(
                "grid.edit.delete_row",
                t(Msg::MnGeDeleteRow),
                can && has,
            ));
            items.push(CtxItem::Separator);
            items.push(CtxItem::maybe(
                "grid.edit.undo",
                t(Msg::MnGeUndo),
                can && e.cs.can_undo(),
            ));
            items.push(CtxItem::maybe(
                "grid.edit.redo",
                t(Msg::MnGeRedo),
                can && e.cs.can_redo(),
            ));
            items.push(CtxItem::maybe(
                "grid.edit.changes",
                t(Msg::MnGeChanges),
                dirty,
            ));
            items.push(CtxItem::maybe(
                "grid.edit.preview_sql",
                t(Msg::MnGePreview),
                dirty,
            ));
            items.push(CtxItem::maybe(
                "grid.edit.apply",
                t(Msg::MnGeApply),
                can && dirty && e.keys_ready && self.session_open(),
            ));
            items.push(CtxItem::maybe(
                "grid.edit.revert",
                t(Msg::MnGeRevert),
                can && dirty,
            ));
        }
        let text_w = (self.row_h * 10).max(180);
        self.menu_is_view = false;
        self.menu.open_at(x, y, items, self.menu_host(), text_w);
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
        if id.starts_with("grid.edit.") {
            self.edit_command(id);
            return;
        }
        if id == "obj.follow_fk" {
            let col = self.menu_cell.as_ref().map(|(ci, _)| *ci).and_then(|ci| {
                self.rs
                    .as_ref()
                    .and_then(|rs| rs.columns().get(ci).map(|c| c.name.clone()))
            });
            self.pending_follow = self.menu_row.zip(col);
            return;
        }
        if id == "obj.reveal" {
            self.pending_reveal = self.menu_cell.as_ref().map(|(ci, _)| *ci).and_then(|ci| {
                self.rs
                    .as_ref()
                    .and_then(|rs| rs.columns().get(ci).map(|c| c.name.clone()))
            });
            return;
        }
        if let Some(rest) = id.strip_prefix("filter.") {
            self.filter_pick(rest);
            return;
        }
        if let Some(rest) = id.strip_prefix("cond.") {
            self.cond_pick(rest);
            return;
        }
        if let Some(rest) = id.strip_prefix("sort.") {
            if let Some((ci, _)) = self.menu_cell {
                match rest {
                    "asc" => self.sort_keys = vec![(ci, true)],
                    "desc" => self.sort_keys = vec![(ci, false)],
                    _ => self.sort_keys.clear(),
                }
                self.apply_sort();
            }
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
    /// 정렬 규칙이 바뀌었을 때(설정 `ui.sort_natural`) 걸린 정렬을 다시 적용.
    pub(crate) fn resort(&mut self) {
        if !self.sort_keys.is_empty() || !self.filters.is_empty() {
            self.apply_sort();
        }
    }

    fn apply_sort(&mut self) {
        // 편집 중(변경 집합이 비어 있지 않음)에는 정렬하지 않는다 — 추가 행의 자리를 잃는다(87 §3).
        //   필터는 기존 행에 그대로 건다(사용자 09-29 "정규식과 다른 행이 보인다" = 편집 중이라 투영을 건너뛰던 결함) ·
        //   추가 행은 늘 보인다(`rebuild_row_order`).
        let dirty = self.edit_dirty();
        let Some(rs) = self.rs.as_ref() else { return };
        let n = rs.len();
        let mut order: Vec<usize> = if dirty {
            self.row_order
                .iter()
                .copied()
                .filter(|&ri| ri < n)
                .collect()
        } else {
            (0..n).collect()
        };
        if !dirty && !self.sort_keys.is_empty() {
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
        // ★ 두 번째 투영 = 필터(T-181 · 77 §2-2): 세트는 그대로, 남길 행 번호만.
        if !self.filters.is_empty() {
            let ncol = rs.columns().len();
            let filters = self.filters.clone();
            let any = self.filter_or;
            order.retain(|&r| {
                let hit = |p: &Predicate| p.col >= ncol || p.pass_value(rs.cell(r, p.col));
                if any {
                    filters.iter().any(hit)
                } else {
                    filters.iter().all(hit)
                }
            });
        }
        self.row_order = order;
        if dirty {
            self.rebuild_row_order();
        }
    }

    /// 우클릭한 셀 → (원본 열, 셀 글) — 필터 메뉴 대상(행번호 칸은 첫 열).
    /// 선택 구간들이 덮는 **그 열의 값**(표시 순 · 중복 제거 · NULL 제외) — "이 값만/제외: N개 선택"의 재료(사용자 10-07).
    fn selected_values(&self, ci: usize) -> Vec<String> {
        let Some(rs) = self.rs.as_ref() else {
            return Vec::new();
        };
        let Some(pos) = self.col_order.iter().position(|&c| c == ci) else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        let n = self.rows();
        for &(r0, r1, c0, c1) in &self.regions {
            if pos < c0 || pos > c1 || n == 0 {
                continue;
            }
            for di in r0..=r1.min(n - 1) {
                let Some(&r) = self.row_order.get(di) else {
                    continue;
                };
                if r >= self.src_len() {
                    continue;
                }
                if let Some(v) = cell_opt(rs.cell(r, ci)) {
                    if !out.iter().any(|x| x == &v) {
                        out.push(v);
                    }
                }
            }
        }
        out
    }

    fn cell_filter_target(&self, cell: (usize, usize)) -> Option<(usize, Option<String>)> {
        let ci = *self.col_order.get(cell.1)?;
        let rs = self.rs.as_ref()?;
        let r = *self.row_order.get(cell.0)?;
        if ci >= rs.columns().len() {
            return None;
        }
        Some((ci, cell_opt(rs.cell(r, ci))))
    }

    /// 필터 술어 추가(열마다 하나 · `=` 반복은 값 목록으로) → 재투영 · 맨 위로.
    pub(crate) fn add_filter(&mut self, col: usize, op: FilterOp, value: String) {
        if let Err(e) = self.try_add_filter(col, op, value) {
            self.pending_error = Some(e);
        }
    }

    /// 술어 추가(같은 열의 같은 연산은 교체 · `=`는 이미 `=`/목록이 있으면 **목록에 합침** · 참/거짓/NULL은 배타) → 재투영 · 맨 위로.
    /// 정규식이 틀리면 `Err(메시지)`(필터는 걸지 않음).
    pub(crate) fn try_add_filter(
        &mut self,
        col: usize,
        op: FilterOp,
        value: String,
    ) -> Result<(), String> {
        let kind = self.col_kind(col);
        // "이 값만" 반복 = 값 목록으로(사용자 09-29 "1 → 3 → 8 골라 보기").
        if op == FilterOp::Eq {
            if let Some(p) = self
                .filters
                .iter_mut()
                .find(|p| p.col == col && matches!(p.op, FilterOp::Eq | FilterOp::In))
            {
                let mut items = list_items(p);
                if !items.iter().any(|x| x.eq_ignore_ascii_case(&value)) {
                    items.push(value);
                }
                p.op = FilterOp::In;
                p.value = join_list(&items);
                self.apply_sort();
                self.scroll_y = 0;
                self.after_filters_changed();
                return Ok(());
            }
        }
        // ★ 열마다 술어 하나(사용자 09-29 "다중 조건은 아직 — 항상 1개") — 다른 연산을 걸면 앞 것은 취소. AND/OR 결합은 뒤로.
        let pred = Predicate::new(col, kind, op, value)?;
        self.filters.retain(|p| p.col != col);
        self.filters.push(pred);
        self.apply_sort();
        self.scroll_y = 0;
        self.after_filters_changed();
        Ok(())
    }

    /// 그리드가 알릴 오류(정규식 오류 등 · 1회성 · 호스트가 상태줄로).
    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.pending_error.take()
    }

    /// 열 `ci`의 유형 배지(캐시 · 결과당 한 번 계산 · 표본 = 앞 200행 · 방언별 규칙).
    fn ensure_type_badges(&mut self) {
        let Some(rs) = self.rs.as_ref() else {
            return;
        };
        if self.type_badges.len() != rs.columns().len() {
            let n = rs.len().min(200);
            let d = self.dialect;
            self.type_badges = rs
                .columns()
                .iter()
                .enumerate()
                .map(|(k, c)| type_badge(d, &c.type_name, (0..n).map(|r| rs.cell(r, k).clone())))
                .collect();
        }
    }

    /// 열 `ci`의 배지(캐시 읽기만 · 그리기 루프 = `rs` 차용 중이라 채움은 `ensure_type_badges`가 먼저).
    fn type_badge_at(&self, ci: usize) -> (&'static str, BadgeTone) {
        self.type_badges
            .get(ci)
            .copied()
            .unwrap_or(("EX", BadgeTone::Red))
    }

    /// 열의 데이터 종류(드라이버 타입 이름 + 앞 200행 표본).
    pub(crate) fn col_kind(&self, col: usize) -> ColKind {
        let Some(rs) = self.rs.as_ref() else {
            return ColKind::Text;
        };
        let Some(c) = rs.columns().get(col) else {
            return ColKind::Text;
        };
        let n = rs.len().min(200);
        ColKind::infer(&c.type_name, (0..n).map(|r| rs.cell(r, col).clone()))
    }

    /// 필터 전부 지우기(재투영).
    pub(crate) fn clear_filters(&mut self) {
        if !self.filters.is_empty() {
            self.filters.clear();
            self.apply_sort();
            self.after_filters_changed();
        }
    }

    /// 상태줄 "n / N행" — 필터가 있을 때만 `Some((보이는 행, 전체 행))`.
    pub(crate) fn filter_summary(&self) -> Option<(usize, usize)> {
        if self.filters.is_empty() {
            return None;
        }
        Some((
            self.row_order.len(),
            self.rs.as_ref().map_or(0, |r| r.len()),
        ))
    }

    /// "포함…" 입력 요청(1회성 · 열) — 호스트가 팔레트를 연다.
    pub(crate) fn take_filter_prompt(&mut self) -> Option<(usize, FilterOp, String)> {
        self.pending_filter_prompt.take()
    }

    /// 조회용 Query(77 §2-2): 출처 문장을 서브쿼리로 감싸 술어를 `WHERE 1=1 AND …`로 — 출처가 없거나 필터가 없으면 None.
    pub(crate) fn filter_query(&self) -> Option<String> {
        self.filter_query_on(self.source_sql())
    }

    /// ★ 필터 변경 뒤(T-285 · 103 §4-1): 채움 카운터 리셋 → 판정(`fetch_under_filter`) → 서버 승격이면 조건 바 길로 같은 탭 재조회
    /// (출처 보존 · 실패 되돌림 = `cond_run_failed`) · 필터를 다 지웠고 승격 결과였으면 출처(조건 식 포함)로 복귀 · `ask`면 안내 1회.
    fn after_filters_changed(&mut self) {
        self.fill_pages_left = self.fill_pages;
        let has_filter = !self.filters.is_empty();
        let editing = self.edit.as_ref().is_some_and(|e| e.cs.is_dirty());
        if !has_filter {
            if self.filter_promoted && !editing {
                // 복귀 = 출처(+ 조건 식)로 다시 조회.
                let base = self
                    .cond_base
                    .clone()
                    .unwrap_or_else(|| self.source_sql.clone());
                let c = self.cond.text();
                let sql = if c.trim().is_empty() {
                    base.clone()
                } else {
                    crate::condbar::wrap_condition(&base, &c)
                };
                self.cond_base = Some(base);
                self.cond_prev_sql = Some(self.source_sql.clone());
                self.cond_last_sql = Some(sql.clone());
                self.pending_cond_run = Some(sql);
                self.filter_promoted = false;
            }
            return;
        }
        let translatable = self
            .filters
            .iter()
            .all(|p| p.op != FilterOp::Regex || !p.needs_value_list(self.dialect));
        match fetch_under_filter(
            self.more || self.filter_promoted,
            has_filter,
            self.filter_server,
            translatable,
            editing,
        ) {
            FetchPlan::Server => {
                if let Some(sql) = self.promoted_sql() {
                    let base = self
                        .cond_base
                        .clone()
                        .unwrap_or_else(|| self.source_sql.clone());
                    self.cond_base = Some(base);
                    self.cond_prev_sql = Some(self.source_sql.clone());
                    self.cond_last_sql = Some(sql.clone());
                    self.pending_cond_run = Some(sql);
                    self.filter_promoted = true;
                    self.status(t(Msg::StFilterServerPromoted).to_string());
                }
            }
            FetchPlan::Ask => {
                if !self.filter_asked {
                    self.filter_asked = true;
                    self.status(t(Msg::StFilterAskServer).to_string());
                }
                // 안내 뒤 로컬과 같다 — 첫 채움 페치를 깨운다.
                self.clamp();
            }
            // 로컬 채움: 첫 페치는 이벤트 없이도 바로(협업 bin49 b1 = 마우스가 움직여야 `clamp`가 돌았다) · 다음 페이지는 `append_page`가.
            FetchPlan::Fill => self.clamp(),
        }
    }

    /// 서버 승격 SQL = 출처(조건 바 기준 `cond_base`) + 조건 식 + 필터 술어 — 한 문장(`WHERE 1=1 AND (조건) AND 필터…` · 103 §4-1).
    fn promoted_sql(&self) -> Option<String> {
        let base = self
            .cond_base
            .clone()
            .unwrap_or_else(|| self.source_sql.clone());
        let c = self.cond.text();
        let with_cond = if c.trim().is_empty() {
            base
        } else {
            crate::condbar::wrap_condition(&base, &c)
        };
        self.filter_query_on(&with_cond)
    }

    /// 필터 조회 SQL을 **주어진 출처** 위에(`filter_query` = 지금 출처 · 승격 = 조건 식을 입힌 출처).
    pub(crate) fn filter_query_on(&self, src_in: &str) -> Option<String> {
        let rs = self.rs.as_ref()?;
        if self.filters.is_empty() {
            return None;
        }
        let src = src_in.trim().trim_end_matches(';').trim();
        if src.is_empty() {
            return None;
        }
        let mut notes: Vec<String> = Vec::new();
        let mut preds: Vec<String> = Vec::new();
        for p in &self.filters {
            let Some(c) = rs.columns().get(p.col) else {
                continue;
            };
            if p.op == FilterOp::Regex {
                // ③단계 후보 = 지금 보이는(전 술어 통과) 행의 이 열 distinct 값 — **값 목록이 실제로 필요할 때만** 모은다
                //   (①② 방언 정규식·LIKE 번역이면 0) · 해시 집합 O(n) · 상한+1개에서 멈춤(`grid.filter_list_max` · 사용자 09-30).
                let seen = if p.needs_value_list(self.dialect) {
                    distinct_capped(
                        self.row_order
                            .iter()
                            .map(|&r| cell_text(rs.cell(r, p.col), "")),
                        self.filter_list_max,
                    )
                } else {
                    Vec::new()
                };
                let (sql, note) = p.regex_sql(&c.name, self.dialect, &seen, self.filter_list_max);
                preds.push(sql);
                notes.push(note);
            } else {
                preds.push(p.to_sql(&c.name));
            }
        }
        if preds.is_empty() {
            return None;
        }
        let head = if notes.is_empty() {
            String::new()
        } else {
            format!("{}\n", notes.join("\n"))
        };
        // OR 결합 = 괄호로 묶어 한 덩어리(`WHERE 1=1 AND (a OR b)` · 뒤에 조건을 덧붙여도 뜻이 안 바뀐다).
        Some(if self.filter_or && preds.len() > 1 {
            format!(
                "{head}SELECT *\nFROM (\n{src}\n) q\nWHERE 1=1\nAND (\n    {}\n)\n",
                preds.join("\n    OR ")
            )
        } else {
            format!(
                "{head}SELECT *\nFROM (\n{src}\n) q\nWHERE 1=1\nAND {}\n",
                preds.join("\nAND ")
            )
        })
    }

    fn filter_pick(&mut self, what: &str) {
        let target = self.menu_cell.clone();
        match what {
            "clear" => return self.clear_filters(),
            "copy_query" => {
                if let Some(sql) = self.filter_query() {
                    self.pending_copy = Some((sql, 1));
                }
                return;
            }
            "requery" => {
                self.pending_requery = self.filter_query();
                return;
            }
            "or" => {
                self.set_filter_or(!self.filter_or);
                return;
            }
            "values" => {
                if let Some((ci, _)) = target {
                    self.open_value_pick(ci);
                }
                return;
            }
            _ => {}
        }
        let Some((ci, val)) = target else { return };
        // 값 고르기 토글(`pick:<n>`): 이미 골라져 있으면 빼고(마지막이면 술어 제거) · 아니면 `=`로 넣는다(`=` 반복 = 값 목록 합침).
        if let Some(n) = what
            .strip_prefix("pick:")
            .and_then(|n| n.parse::<usize>().ok())
        {
            let Some(v) = self.column_values(ci).get(n).cloned() else {
                return;
            };
            let pos = self
                .filters
                .iter()
                .position(|p| p.col == ci && matches!(p.op, FilterOp::Eq | FilterOp::In));
            let present = pos.is_some_and(|i| {
                let p = &self.filters[i];
                let items = list_items(p);
                items.iter().any(|c| c.eq_ignore_ascii_case(&v))
            });
            if present {
                let i = pos.unwrap_or_default();
                let mut items = list_items(&self.filters[i]);
                items.retain(|c| !c.eq_ignore_ascii_case(&v));
                if items.is_empty() {
                    self.filters.remove(i);
                } else {
                    self.filters[i].op = FilterOp::In;
                    self.filters[i].value = join_list(&items);
                }
                self.apply_sort();
                self.scroll_y = 0;
                self.after_filters_changed();
            } else {
                self.add_filter(ci, FilterOp::Eq, v);
            }
            return;
        }
        if what == "clear_col" {
            self.filters.retain(|p| p.col != ci);
            self.apply_sort();
            self.after_filters_changed();
            return;
        }
        // 선택된 값 N개 한꺼번에(사용자 10-07): 1개면 `=`/`<>` · 여럿이면 값 목록 IN / NOT IN.
        if what == "eq_sel" || what == "ne_sel" {
            let vals = self.selected_values(ci);
            if vals.is_empty() {
                return;
            }
            let op = match (what, vals.len()) {
                ("eq_sel", 1) => FilterOp::Eq,
                ("eq_sel", _) => FilterOp::In,
                (_, 1) => FilterOp::Ne,
                _ => FilterOp::NotIn,
            };
            let value = if vals.len() == 1 {
                vals[0].clone()
            } else {
                join_list(&vals)
            };
            self.add_filter(ci, op, value);
            return;
        }
        let Some(op) = FilterOp::parse(what) else {
            return;
        };
        if op.needs_prompt() {
            // 값을 묻는 연산 = 팔레트 프롬프트(초기값 = 같은 열에 같은 연산이 걸려 있으면 **그 값**(수정 · 사용자 09-29) ·
            //   아니면 우클릭한 셀의 값 · 문자 포함/시작/정규식은 빈 값).
            let existing = self
                .filters
                .iter()
                .find(|p| p.col == ci && p.op == op)
                .map(|p| p.value.clone());
            let initial = match (existing, op) {
                (Some(v), _) => v,
                (None, FilterOp::Contains | FilterOp::StartsWith | FilterOp::Regex) => {
                    String::new()
                }
                (None, _) => val.unwrap_or_default(),
            };
            self.pending_filter_prompt = Some((ci, op, initial));
            return;
        }
        let value = match op {
            FilterOp::Eq | FilterOp::Ne => val.unwrap_or_default(),
            _ => String::new(),
        };
        self.add_filter(ci, op, value);
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

    /// 결과 비우기 — 오류가 나도 결과 영역은 **기본 형태**(행번호 1 · `** No Records **`)를 유지한다(사용자 09-17).
    /// 오류·메시지 본문은 로그 창으로만 간다.
    /// 전체 조회가 진행 중인가(툴바/카드 ■ 활성 판정).
    pub(crate) fn fetch_all_active(&self) -> bool {
        self.fetch_all_pending
    }

    pub(crate) fn clear_result(&mut self) {
        self.rs = None;
        self.edit = None;
        self.read_only = None;
        self.col_order.clear();
        self.row_order.clear();
        self.sort_keys.clear();
        self.filters.clear();
        self.menu_cell = None;
        self.type_badges.clear();
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
        self.auto_fetch_at = None;
        self.more = false;
        self.total = None;
        self.fetching = false;
        self.fetch_req = None;
        self.end_fetch_all();
        self.text_scroll = (0, 0);
        self.sync_fetch_tools();
    }

    /// 표시 행 수 = `row_order`(원본 행 + 편집으로 추가된 행) — 원본만 세려면 [`Self::src_len`].
    fn rows(&self) -> usize {
        self.row_order.len()
    }

    /// 원본(세트) 행 수 — 페치 오프셋·"더 가져오기"의 기준(표시 행 수 `rows()`는 정렬·필터 투영이라 다를 수 있다).
    fn src_rows(&self) -> usize {
        self.rs.as_ref().map_or(0, |r| r.len())
    }

    /// 원본 결과 행 수(추가 행 제외).
    fn src_len(&self) -> usize {
        self.rs.as_ref().map_or(0, |r| r.len())
    }

    /// 표시 행 index → 행 참조(원본 index 또는 추가 행).
    fn rref_at(&self, di: usize) -> Option<RowRef> {
        let ri = *self.row_order.get(di)?;
        let n = self.src_len();
        Some(if ri < n {
            RowRef::Existing(ri)
        } else {
            RowRef::Inserted(ri - n)
        })
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

    /// 스크롤바에 넘기는 (뷰포트 · 내용 폭 · 내용 높이) — **한 자리**(입력 처리와 자체 시험 훅이 같은 값을 쓴다 · 10-10).
    /// ★ 뷰포트 = 행번호 열(고정)·헤더(고정) 제외 — 바는 데이터 영역에만(09-16: 세로 바가 헤더까지 걸쳤다) · 가로 바의 끝이
    ///   키보드 `max_scroll`과 같은 자리. 내용은 뷰포트보다 작아도 뷰포트 크기로(바 없음 판정은 ScrollBars가).
    fn bars_geom(&self) -> (Rect, i32, i32) {
        let (cw, ch) = self.content_size();
        let b = self.bounds;
        let b = Rect::new(
            b.x + self.gutter_w,
            b.y + self.header_h,
            b.w - self.gutter_w,
            b.h - self.header_h - self.footer_h,
        );
        let ch = ch - self.header_h;
        (b, cw.max(b.w), ch.max(b.h))
    }

    /// ★ 자체 시험(기동 명령 `grid.vdrag:<dy>` · 10-10 118차 mac 크래시 회귀 = nexa-ctl `scroll.rs` 썸 드래그 i32 넘침): 세로
    /// 바를 깨우고(휠 0 = 스크롤 없이 깨움) **지금 썸의 가운데**(창 좌표)를 돌려준다 — 호스트가 그 자리에 MouseDown → dy만큼 MouseMove →
    /// MouseUp을 실제 `route` 경로로 넣는다. 결과가 없거나 바가 필요 없으면 `None`. 좌표 하드코딩 없이 어느 OS·배율에서든 같은 시험.
    pub(crate) fn test_vthumb_center(&mut self, scale: f32) -> Option<(i32, i32)> {
        if self.row_h <= 0 || self.rs.is_none() {
            return None;
        }
        let (b, cw, ch) = self.bars_geom();
        let _ = self.bars.on_event(
            &InputEvent::Wheel { delta: 0 },
            b,
            cw,
            ch,
            self.scroll_x,
            self.scroll_y,
            scale,
        );
        let t = ScrollBars::v_thumb_for_test(b, ch, self.scroll_y, scale)?;
        Some((t.x + t.w / 2, t.y + t.h / 2))
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
            // 끝에 닿았을 때 · 또는 필터로 걸러져 화면을 못 채울 때(스크롤 없음)도 이어서(사용자 09-29 "끝으로 가면 전부 로딩") —
            //   단 **채움 상한**(`grid.filter_fill_pages` · T-285 · D-258)까지만: 끝까지 자동으로 읽지 않는다(39 §2).
            && ((my > 0 && self.scroll_y >= my - self.row_h.max(1))
                || (my == 0 && !self.filters.is_empty() && self.fill_pages_left > 0))
            && self.auto_fetch_at != Some(self.src_rows())
        {
            // ★ 오프셋은 **원본 행 수**(필터로 걸러진 표시 행 수가 아님 · 사용자 09-29 재질의 반복 결함).
            let offset = self.src_rows();
            self.auto_fetch_at = Some(offset);
            self.fetch_req = Some(FetchReq::Next {
                offset,
                limit: self.page_rows,
            });
        }
    }

    /// 페이드 타이머(스크롤바 · 호버 행) — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.now_ms = now_ms;
        // 표식 툴팁: 머무름 지연이 끝나는 순간 한 번 다시 그린다.
        let tip = self
            .mark_hover
            .is_some_and(|(_, s)| now_ms.saturating_sub(s) < MARK_TIP_MS + 40);
        let a = self.bars.tick(now_ms) | tip;
        let b = self.hover.tick(now_ms);
        let c = self.poll_text();
        let d = self.vpick.tick(now_ms);
        let e = self.cond_on && self.cond.tick(now_ms);
        a || b || c || d || e
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.bars.is_visible() || (self.cond_on && self.cond.bars_visible())
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
        // ★ 값 목록 팝업이 떠 있으면 그것이 먼저(바깥 클릭 = 닫고 통과 · 결과 = 그 열 값 필터).
        if self.vpick.is_open() {
            // 팝업이 떠 있는 동안 표식 툴팁은 없다(포인터가 팝업으로 갔는데 툴팁이 남던 잔상 · 협업 V1 10-06).
            self.mark_hover = None;
            self.vpick.set_shift(self.shift);
            let mut inv = Invalidations::default();
            let consumed = self.vpick.on_event(ev, &mut inv);
            if let Some(a) = self.vpick.take_edit_ctx() {
                self.pending_edit_ctx = Some(a);
            }
            if let Some(crate::valuepick::PickResult::Apply { col, values }) =
                self.vpick.take_result()
            {
                self.set_pick_values(col, &values);
            }
            if consumed {
                return;
            }
        }
        // ★ 조건 바가 포커스면 키·글자·편집 명령(전체 선택·되돌리기)은 그 상자가 먼저(그리드 전체 선택·셀 편집 키보다 ·
        //   사용자 10-06 "Ctrl+A가 그리드 전체 선택").
        if self.cond_on
            && self.cond.is_focused()
            && matches!(
                ev,
                InputEvent::Key { .. }
                    | InputEvent::Char { .. }
                    | InputEvent::SelectAll
                    | InputEvent::Undo
                    | InputEvent::Redo
            )
            && self.strip_event(ev)
        {
            return;
        }
        // 열린 우클릭 메뉴가 먼저(바깥 클릭 = 닫고 통과).
        // ★ 필터 표식 hover(툴팁 대상) — 빗금 표식 사각형 안에서만.
        if let InputEvent::MouseMove { x, y } = *ev {
            let p = Point { x, y };
            // hover 모드 깔때기 — 마우스 아래 열 머리(원본 index) · 바뀌면 다시 그린다(사건 없는 그림 변화 = `dirty`).
            let fh = if self.funnel_mode == FunnelMode::Hover
                && self.rs.is_some()
                && self.row_h > 0
                && self.header_rect().contains(p)
            {
                self.header_pos_at(x)
                    .and_then(|pos| self.col_order.get(pos).copied())
            } else {
                None
            };
            if fh != self.funnel_hover {
                self.funnel_hover = fh;
                self.dirty = true;
            }
            // 열 머리 hover(카드용): 머리 안이면 (원본 열, 칸 사각형) · 밖이면 없음.
            self.hdr_hover =
                if self.rs.is_some() && self.row_h > 0 && self.header_rect().contains(p) {
                    self.header_pos_at(x).and_then(|pos| {
                        let ci = *self.col_order.get(pos)?;
                        let cw = self.col_w.get(ci).copied().unwrap_or(80);
                        let hr = self.header_rect();
                        Some((ci, Rect::new(self.col_x(pos), hr.y, cw, hr.h)))
                    })
                } else {
                    None
                };
            let hit = self
                .filters
                .iter()
                .map(|f| f.col)
                .find(|&ci| self.mark_rect_of(ci).is_some_and(|r| r.contains(p)));
            match (hit, self.mark_hover) {
                (Some(ci), Some((h, _))) if h == ci => {}
                (Some(ci), _) => self.mark_hover = Some((ci, self.now_ms)),
                (None, Some(_)) => self.mark_hover = None,
                _ => {}
            }
        }
        if self.menu.is_open() {
            // 메뉴가 떠 있는 동안 셀 드래그는 없다(메뉴가 MouseUp을 먹어 드래그가 남던 결함 · 사용자 09-29).
            self.drag_sel = None;
            // ★ 보기 모드 ▾ 메뉴가 열린 채 그 버튼을 다시 누르면 = 닫기만(토글 · 사용자 09-30) — 바깥 클릭이 메뉴를 닫고
            //   버튼 클릭으로 이어져 다시 열리던 것을 막는다.
            if self.menu_is_view {
                if let InputEvent::MouseDown { x, y, .. } = *ev {
                    if self.tb_view.bounds().contains(Point { x, y }) {
                        self.view_toggle_off = true;
                    }
                }
            }
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
            // 메뉴가 열려 있는 동안 마우스 이동은 메뉴 몫(셀·헤더·도구줄 hover 0 · 10-01).
            if matches!(ev, InputEvent::MouseMove { .. }) {
                return;
            }
        }
        // ★ 셀/행 드래그 선택 중의 MouseUp = **어디서 놓든 드래그 끝**(사용자 10-07 "버튼을 뗐는데 Release가 안 돼 이동마다 선택이
        //   바뀜") — 아래의 필터 줄·푸터 도구줄·셀 편집기 분기가 자기 영역의 MouseUp을 먹고 돌아가 해제 팔(아래 `MouseUp if drag_sel`)에
        //   닿지 못하던 결함. 해제만 하고 사건은 계속 흘린다(그 자리 컨트롤의 놓임 처리는 그대로).
        if matches!(ev, InputEvent::MouseUp { .. }) && self.drag_sel.is_some() {
            self.drag_sel = None;
            self.dirty = true;
        }
        // 필터 줄(칩·조건 바)이 먼저 — 줄 안의 클릭은 셀·머리로 흘리지 않는다. 단 **열 머리를 끌는 중**이면 이동·놓임은 끌기
        //   몫(조건 바 위로 끌어 놓기 · 협업 V1 C 10-06 = 조건 바가 이동을 먹어 고스트가 머리 줄에 머물렀다).
        let hdr_dragging = self.hdr_drag.is_some()
            && matches!(
                ev,
                InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. }
            );
        if !hdr_dragging && self.strip_event(ev) {
            return;
        }
        // 결과 도구줄(푸터)이 먼저 — 커서 아래 컨트롤에만(마우스 라우팅 규칙).
        if self.footer_event(ev, scale) {
            return;
        }
        if self.view != ResultView::Grid && self.rs.is_some() {
            self.text_view_event(ev, scale);
            return;
        }
        self.live_scale = scale;
        // ★ 살아 있는 셀 편집기(docs/87 §6): 열려 있으면 키·문자·안쪽 마우스는 상자로 · 바깥 클릭 = 커밋 뒤 통과.
        if self.live_event(ev) {
            return;
        }
        // Tab / Shift+Tab = 다음/이전 셀(엑셀·DBeaver 관례 · 편집기 밖 · 87 §11).
        if matches!(ev, InputEvent::Char { c: '\t', .. })
            && self.sel_cur.is_some()
            && self.rs.is_some()
        {
            let dx = if self.shift { -1 } else { 1 };
            self.move_sel(0, dx, false, false);
            return;
        }
        // 편집 동작(편집기가 닫혀 있을 때): Enter/타이핑 = 진입 · Delete/Backspace = 비움 · Undo/Redo.
        if self.edit.is_some() && self.sel_cur.is_some() && self.rs.is_some() {
            if let Some(a) = gridedit::action_for_event(ev, false) {
                self.edit_action(a);
                return;
            }
        } else if self.rs.is_some() && self.sel_cur.is_some() && self.read_only.is_some() {
            if let Some(EditAction::BeginEdit | EditAction::BeginEditWith(_)) =
                gridedit::action_for_event(ev, false)
            {
                if let Some(r) = self.read_only.clone() {
                    self.status(tf(Msg::StGeReadOnly, &[&r.text()]));
                }
                return;
            }
        }
        // ★ 스크롤바가 **선택보다 먼저**(휠 = 픽셀 · 썸/트랙 클릭 · 드래그 · 호버). 소비되면 셀 선택·키 처리로 흘리지 않는다
        //   (09-16: 가로 바 트랙을 눌렀는데 뒤의 셀이 선택됐다 — 선택 판정이 먼저 return했다).
        if self.row_h > 0 && self.rs.is_some() {
            let (b, cw, ch) = self.bars_geom();
            let (nx, ny, consumed) =
                self.bars
                    .on_event(ev, b, cw, ch, self.scroll_x, self.scroll_y, scale);
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
                        // 더블클릭(400 ms · 같은 셀 · 수식키 없음) = 편집 진입(엑셀 · 87 §6).
                        if !shift && !primary && self.edit.is_some() {
                            let now = std::time::Instant::now();
                            let dbl = self.edit.as_ref().and_then(|e| e.last_click).is_some_and(
                                |(c, t0)| c == cell && now.duration_since(t0).as_millis() < 400,
                            );
                            if let Some(e) = self.edit.as_mut() {
                                e.last_click = Some((cell, now));
                            }
                            if dbl {
                                self.select_only(cell);
                                self.drag_sel = None;
                                self.begin_edit(None, true);
                                return;
                            }
                        }
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
                    self.drag_sel = None;
                    // 헤더(컬럼명) 우클릭 = 그 열의 필터·정렬 메뉴(사용자 09-29).
                    if self.rs.is_some()
                        && y >= self.bounds.y
                        && y < self.bounds.y + self.header_h
                        && x >= self.bounds.x + self.gutter_w
                    {
                        if let Some(&ci) = self
                            .header_pos_at(x)
                            .and_then(|pos| self.col_order.get(pos))
                        {
                            self.open_header_menu(ci, x, y, scale);
                            return;
                        }
                    }
                    if let Some(cell) = self.cell_at_point(x, y) {
                        if !self.in_sel(cell.0, cell.1) {
                            self.select_only(cell);
                        }
                        self.menu_cell = self.cell_filter_target(cell);
                        self.menu_row = self.row_order.get(cell.0).copied();
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
                        Key::Up => (-self.key_step(-1), 0),
                        Key::Down => (self.key_step(1), 0),
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
                    // ★ 깔때기 아이콘 · 필터 빗금 표식 클릭 = 값 목록 팝업(정렬·드래그보다 먼저).
                    let p = Point { x, y };
                    let funnel =
                        self.funnel_rects
                            .iter()
                            .find(|(_, r)| r.contains(p))
                            .map(|(ci, _)| *ci)
                            .or_else(|| {
                                self.filters.iter().map(|f| f.col).find(|&ci| {
                                    self.mark_rect_of(ci).is_some_and(|r| r.contains(p))
                                })
                            });
                    if let Some(ci) = funnel {
                        self.open_value_pick(ci);
                        return;
                    }
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
                InputEvent::MouseMove { x, y } if self.hdr_drag.is_some() => {
                    if let Some(d) = self.hdr_drag.as_mut() {
                        d.cur_x = x;
                        if (x - d.press_x).abs() > 4 {
                            d.active = true;
                        }
                    }
                    // ★ 열 머리를 조건 바 위로 끌면 = 놓을 자리 강조(열 이름 넣기 · 사용자 10-06 "컬럼을 조건 바에 DnD").
                    let over_cond = self.cond_on
                        && self.col_dragging()
                        && self.cond.rect().contains(Point { x, y });
                    if self.cond.set_drop_hot(over_cond) {
                        self.dirty = true;
                    }
                    self.preview_col_drag();
                    return;
                }
                InputEvent::MouseUp { x, y } if self.hdr_drag.is_some() => {
                    // 미리보기가 곧 결과 — 놓으면 확정 · 안 움직였으면 정렬 클릭.
                    if let Some(d) = self.hdr_drag.take() {
                        if !d.active {
                            if let Some(&col) = self.col_order.get(d.pos) {
                                self.toggle_sort(col, d.shift);
                            }
                        } else if self.cond_on && self.cond.rect().contains(Point { x, y }) {
                            // 조건 바에 놓음 = 끈 열(라이브 미리보기로 자리가 옮겨져 있으니 **지금 순서의 `d.pos`** · 협업 V1 C =
                            //   `orig[d.pos]`를 읽어 다른 열이 들어갔다)을 읽은 뒤 열 순서는 시작 때로 되돌리고 조건 바에 넣는다.
                            let ci = self.col_order.get(d.pos).copied();
                            if d.orig.len() == self.col_order.len() {
                                self.col_order = d.orig;
                            }
                            let name = ci.and_then(|ci| {
                                self.rs
                                    .as_ref()
                                    .and_then(|rs| rs.columns().get(ci).map(|c| c.name.clone()))
                            });
                            if let (Some(ci), Some(name)) = (ci, name) {
                                let numeric = self.col_kind(ci) == ColKind::Number;
                                // 열 이름 인용 = 조건 메뉴와 같은 정책(`identq::quote`).
                                let col = crate::identq::quote(
                                    self.dialect,
                                    &name,
                                    self.cond_quote_always,
                                );
                                self.cond.insert_column(&col, numeric);
                                self.dirty = true;
                            }
                        }
                    }
                    self.cond.set_drop_hot(false);
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
            InputEvent::Key { key: Key::Down, .. } => {
                self.scroll_y += self.row_h * self.key_step(1)
            }
            InputEvent::Key { key: Key::Up, .. } => self.scroll_y -= self.row_h * self.key_step(-1),
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
        // 선택 유무가 바뀌면(클릭 · 키 이동 · 전체 선택) 복제/삭제 버튼 활성을 여기서 한 번 맞춘다 — 선택 경로마다 부르지 않고
        // 그리기 직전 한 곳에서(사용자 09-26 "우클릭 메뉴는 되는데 버튼은 비활성").
        if self.tools_has_sel != self.sel_cur.is_some() {
            self.sync_edit_tools();
        }
        self.paint_inner(dc, th, s);
        self.render = t_render.elapsed();
        if let Some(st) = started {
            self.render_at = Some((st, nsql_log::now_local().stamp()));
        }
    }

    fn paint_inner(&mut self, dc: &mut dyn DrawCtx, th: &Theme, s: f32) {
        self.sync_strip(s);
        self.paint_filter_strip(dc, th, s);
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
        // 유형 배지 캐시(T-301)는 `rs`를 빌리기 전에 채운다(그리기 루프는 읽기만).
        if self.type_icons {
            self.ensure_type_badges();
        }
        let Some(rs) = self.rs.as_ref() else {
            self.paint_footer(dc, th, s, footer, 0, 0, 0);
            return;
        };
        let null = self.null_text.clone();
        let (lo, hi) = self.col_bounds(dc, s, pad);
        // 깔때기 아이콘 여유 = 머리 이름 폭에 더한다(값 폭이 더 넓으면 그쪽이 결정 · 10-06).
        let fun = self.funnel_allow();
        if self.col_w.is_empty() {
            self.col_w = rs
                .columns()
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let mut w = dc.text_width(&c.name) + fun;
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
            // 헤더 이름(+ 정렬/필터 표식이 보이면 그 폭 · 사용자 09-29) + 전 행(최대 5,000행) 중 가장 넓은 값 → [최소, 최대].
            let mut w = rs.columns().get(ci).map_or(0, |c| dc.text_width(&c.name));
            let sort_pos = self.sort_keys.iter().position(|(k, _)| *k == ci);
            let filtered = self.filter_enabled && self.filters.iter().any(|p| p.col == ci);
            let g = self.funnel_g();
            let gap = (g / 3).max(2);
            // 깔때기 자리(필터 걸림 = 빗금 깔때기 · 아니면 표시 방법에 따른 여유) + 정렬 표식(+순번).
            w += if filtered { g + gap } else { fun };
            if let Some(i) = sort_pos {
                let nw = if self.sort_keys.len() > 1 {
                    dc.text_width(&(i + 1).to_string())
                } else {
                    0
                };
                w += g + gap + nw + if nw > 0 { gap } else { 0 };
            }
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
        // 🔧 글꼴 = 본문 셀과 같은 그리드 글꼴(`FontSlot::Base` · 사용자 09-29 "순번 글꼴·크기가 그리드와 다르다" — 종전 Mono).
        self.gutter_w = if self.row_numbers {
            let digits = rs.len().max(1).to_string().len().max(2) as i32;
            dc.select_font(FontSlot::Base, false);
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
        let n_src = rs.len();
        let n = self.rows();
        let edit = self.edit.as_ref();
        let band = (3.0 * s).round().max(2.0) as i32;
        for di in first..n {
            if y >= body.bottom() {
                break;
            }
            let ri = self.row_order.get(di).copied().unwrap_or(di);
            // ★ 편집(docs/87): 원본 행은 세트에서 · 추가 행(`ri >= n_src`)은 변경 집합에서 · 덧그림(수정 셀 색 · 새 행 띠 · 삭제 취선).
            let rref = if ri < n_src {
                RowRef::Existing(ri)
            } else {
                RowRef::Inserted(ri - n_src)
            };
            let row: Option<&[Value]> = if ri < n_src { rs.row(ri) } else { None };
            if row.is_none() && edit.is_none_or(|e| e.cs.inserted(ri - n_src).is_none()) {
                continue;
            }
            let status = edit.map_or(RowStatus::Clean, |e| e.cs.status(rref));
            let err_row = edit.is_some_and(|e| e.error_row == Some(rref));
            last = di + 1;
            let rr = Rect::new(b.x, y, b.w, self.row_h).intersection(&body);
            if di % 2 == 1 {
                dc.fill_rect(rr, th.panel_bg_alt);
            }
            if status == RowStatus::Inserted {
                dc.fill_rect_alpha(rr, th.ok, 0.08);
            }
            if err_row {
                dc.fill_rect_alpha(rr, th.danger, 0.15);
            }
            // 호버 강조 — 색을 새로 만들지 않고 전경색을 알파로 덮는다(진행도 × 토큰 알파 · 서서히).
            let ha = hover_alpha(false, self.hover.value(di));
            if ha > 0.0 {
                dc.fill_rect_alpha(rr, th.text, ha);
            }
            // ★ 행 포커스 배경(사용자 09-22): 선택에 걸린 행 전체(다중 행 선택도 같은 규칙) = 셀 선택색보다 연하게.
            if self.row_focus && self.row_in_sel(di) {
                let (c, a) = self.row_focus_color;
                let (ra, _) = Self::sel_alphas(self.focused, a.unwrap_or(0.35));
                dc.fill_rect_alpha(rr, c.unwrap_or(th.sel_bg), ra);
            }
            let cells = Rect::new(gx0, body.y, (b.right() - gx0).max(0), body.h);
            let mut x = gx0 - self.scroll_x;
            for (pos, &ci) in self.col_order.iter().enumerate() {
                let over: Option<&Option<String>> = edit.and_then(|e| e.cs.cell(rref, ci));
                let v_src: Option<&Value> = row.and_then(|r| r.get(ci));
                if over.is_none() && v_src.is_none() {
                    continue;
                }
                let cw = self.col_w.get(ci).copied().unwrap_or(80);
                let clip = Rect::new(x, y, cw - 1, self.row_h).intersection(&cells);
                // 변경 셀 = 배경은 아주 옅게 · 식별은 **글자색(경고색) + 굵게**(사용자 09-26 "배경색은 잘 안 보인다 · 폰트색으로").
                let changed = over.is_some() && status == RowStatus::Modified;
                if clip.w > 0 && changed {
                    dc.fill_rect_alpha(clip, th.warn, 0.06);
                }
                if clip.w > 0
                    && self.in_sel(di, pos)
                    && !(self.row_focus && self.row_fully_selected(di))
                {
                    dc.fill_rect_alpha(clip, th.sel_bg, Self::sel_alphas(self.focused, 0.0).1);
                }
                if clip.w > 0 && self.sel_cur == Some((di, pos)) {
                    dc.stroke_round_rect(clip, 0, th.accent, 1.0);
                }
                if clip.w > 0 && clip.h > 0 {
                    let (txt, numeric, is_null) = match (over, v_src) {
                        (Some(Some(sv)), _) => (
                            sv.clone(),
                            edit.is_some_and(|e| {
                                e.cols
                                    .get(ci)
                                    .is_some_and(|c| c.spec.kind == CellKind::Number)
                            }),
                            false,
                        ),
                        (Some(None), _) => (null.clone(), false, true),
                        (None, Some(v)) => (
                            cell_text(v, &null),
                            matches!(v, Value::Int(_) | Value::Float(_) | Value::Decimal(_)),
                            matches!(v, Value::Null),
                        ),
                        (None, None) => (String::new(), false, true),
                    };
                    // NULL은 흐린 글자보다 **더 흐리게**(배경 쪽으로 45 % · 사용자 09-22) · 삭제 행은 전체 흐림.
                    let color = if changed {
                        th.warn
                    } else if is_null {
                        th.text_dim.lerp(th.panel_bg, 0.45)
                    } else if status == RowStatus::Deleted {
                        th.text_dim
                    } else {
                        th.text
                    };
                    if changed {
                        dc.select_font(FontSlot::Base, true);
                    }
                    if numeric {
                        let tw = dc.text_width(&txt);
                        let ty = dc.text_center_y(y, self.row_h);
                        dc.text(x + cw - pad - tw, ty, clip, &txt, color);
                    } else {
                        let ty = dc.text_center_y(y, self.row_h);
                        dc.text(x + pad, ty, clip, &txt, color);
                    }
                    if changed {
                        dc.select_font(FontSlot::Base, false);
                    }
                }
                x += cw;
            }
            // 편집 상태 표식: 삭제 = 취선 · 새 행 = 왼쪽 초록 띠(87 §6).
            if status == RowStatus::Deleted {
                let mid = y + self.row_h / 2;
                dc.fill_rect(
                    Rect::new(gx0, mid, (b.right() - gx0).max(0), 1).intersection(&body),
                    th.danger,
                );
            }
            // 행 식별 띠(사용자 09-26): 새 행 = 초록 · 수정 행 = 강조색 · 삭제 행 = 빨강 — 행번호 칸 바로 오른쪽.
            let band_color = match status {
                RowStatus::Inserted => Some(th.ok),
                RowStatus::Modified => Some(th.accent),
                RowStatus::Deleted => Some(th.danger),
                RowStatus::Clean => None,
            };
            if let Some(c) = band_color {
                dc.fill_rect(Rect::new(gx0, y, band, self.row_h).intersection(&body), c);
            }
            // 행번호(고정 열 · 우측 정렬 · 흐리게 · 선택 행은 선택색으로 표시 · 글꼴 = 본문 셀과 같음).
            if self.gutter_w > 0 {
                let num = (di + 1).to_string();
                dc.select_font(FontSlot::Base, false);
                let nw = dc.text_width(&num);
                let gclip = Rect::new(b.x, y, self.gutter_w, self.row_h).intersection(&body);
                dc.fill_rect(gclip, th.chrome_bg);
                let selected_row = self.row_in_sel(di);
                if selected_row {
                    dc.fill_rect_alpha(gclip, th.sel_bg, Self::sel_alphas(self.focused, 0.0).1);
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
                dc.select_font(FontSlot::Base, false);
                let nw = dc.text_width("1");
                dc.text(gx0 - pad - nw, cy, gclip, "1", th.text_dim);
            }
            dc.fill_rect(Rect::new(b.x, rr.bottom() - 1, b.w, 1), th.border);
        }
        if self.gutter_w > 0 {
            dc.fill_rect(Rect::new(gx0 - 1, body.y, 1, body.h), th.border);
        }
        // ★ 살아 있는 셀 편집기(편집 중인 셀 한 곳 · 스크롤·열 폭을 따라간다).
        let live_rect = self
            .edit
            .as_ref()
            .filter(|e| e.live.is_open())
            .and_then(|e| e.live_at)
            .and_then(|(di, pos)| self.cell_rect(di, pos));
        self.live_scale = s;
        if let Some(e) = self.edit.as_mut() {
            if e.live.is_open() {
                if let Some(r) = live_rect {
                    e.live.set_rect(r);
                    e.live.paint(dc, th);
                }
            }
        }
        // ── 헤더(행 위에 덮어 그린다 — 부분 스크롤된 첫 행이 헤더 아래로 들어간다)
        dc.fill_rect(header, th.chrome_bg);
        let hcells = Rect::new(gx0, header.y, (b.right() - gx0).max(0), header.h);
        let mut x = gx0 - self.scroll_x;
        self.mark_rects.clear();
        self.funnel_rects.clear();
        let dragging = self.hdr_drag.as_ref().filter(|d| d.active);
        let mut ghost: Option<(Rect, String)> = None;
        // ★ 표식(깔때기·정렬 ▲/▼·순번)이 뒤에 생겨 열 이름이 잘리면 열을 **그만큼 넓힌다**(늘리기만 · 상한 안 · 협업 V1 ③d 10-06).
        let mut grow: Vec<(usize, i32)> = Vec::new();
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
                dc.fill_rect_alpha(clip, th.sel_bg, Self::sel_alphas(self.focused, 0.0).1);
            }
            // ★ 헤더 표식(사용자 10-06 "필터 표시와 정렬을 분리"): 오른쪽 끝 = **깔때기 자리**(필터 걸림 = 강조색 **빗금 깔때기** ·
            //   안 걸림 = 표시 방법(always/hover/none)에 따라 흐린 민무늬 깔때기 = 값 목록 버튼) · 그 왼쪽 = **정렬** 채운 ▲/▼(+ 결합 순번 ·
            //   민무늬) · 필터가 걸린 열은 이름을 강조색으로. 도형으로 그린다(글꼴 글리프 X · 3-OS 동일). 크기 = `funnel_g`(행 높이 35 %).
            let filtered_col = self.filter_enabled && self.filters.iter().any(|p| p.col == ci);
            let sort_pos = self.sort_keys.iter().position(|(k, _)| *k == ci);
            let want_funnel = self.want_plain_funnel(ci, filtered_col);
            // ★ 유형 배지(T-301 · 사용자 10-07): 이름 **왼쪽**에 2글자(AZ·DT·09·ID·EX·TF·BI) · 기준 글꼴 −4 px 굵게(사용자 10-07 "2단계 더") · 색 = 녹(레거시)/
            //   파(일반)/빨(특수) · 어두운 테마 = 밝게 · 뒤 공백 1 · 복사는 이름만(배지는 그리기뿐).
            let icon_w = if self.type_icons {
                let (badge, tone) = self.type_badge_at(ci);
                dc.select_font_sized(FontSlot::Base, true, -4.0);
                let bw = dc.text_width(badge);
                let sp = dc.text_width(" ");
                if x + pad + bw + sp < x + cw - pad {
                    let by = dc.text_center_y(header.y, header.h);
                    dc.text(
                        x + pad,
                        by,
                        Rect::new(x + pad, header.y, bw, header.h).intersection(&hcells),
                        badge,
                        badge_color(tone, th),
                    );
                }
                dc.select_font(FontSlot::Base, false);
                bw + sp
            } else {
                0
            };
            let name_clip = if filtered_col || sort_pos.is_some() || want_funnel {
                let g = self.funnel_g();
                let gap = (g / 3).max(2);
                let right = x + cw - pad;
                let cy = header.y + header.h / 2;
                let mut used = 0;
                // ① 깔때기 자리(오른쪽 끝).
                if filtered_col || want_funnel {
                    let fr = Rect::new(right - g, cy - g / 2, g, g);
                    if filtered_col {
                        funnel_glyph(dc, fr, th.accent);
                        // 픽셀 (px, py)가 도형 안이고 (px+py) % 3 == 0 → 바탕색으로 파냄 = 사선 빗금.
                        for py in fr.y..=fr.bottom() {
                            for px in fr.x..=fr.right() {
                                if (px + py).rem_euclid(3) == 0
                                    && funnel_contains(fr, px as f32 + 0.5, py as f32 + 0.5)
                                {
                                    dc.fill_rect(Rect::new(px, py, 1, 1), th.panel_bg);
                                }
                            }
                        }
                        // 툴팁 대상 = 깔때기 사각형 · 필터가 걸린 열만(사용자 09-29).
                        self.mark_rects.push((
                            ci,
                            Rect::new(fr.x - pad / 2, header.y, g + pad, header.h)
                                .intersection(&hcells),
                        ));
                    } else {
                        funnel_glyph(dc, fr, th.text_dim);
                    }
                    // 클릭 = 값 목록(민무늬·빗금 둘 다).
                    self.funnel_rects.push((
                        ci,
                        Rect::new(fr.x - pad / 2, header.y, g + pad, header.h)
                            .intersection(&hcells),
                    ));
                    used = g;
                }
                // ② 정렬 표식(깔때기 왼쪽 · 민무늬 · 결합 순번은 화살표 오른쪽).
                if let Some(i) = sort_pos {
                    let num = (self.sort_keys.len() > 1).then(|| (i + 1).to_string());
                    let nw = num.as_ref().map_or(0, |n| dc.text_width(n));
                    let mut sx_right = right - if used > 0 { used + gap } else { 0 };
                    if let Some(n) = &num {
                        let hy = dc.text_center_y(header.y, header.h);
                        dc.text(sx_right - nw, hy, clip, n, th.accent);
                        sx_right -= nw + gap;
                    }
                    let gx = sx_right - g;
                    let (top, bottom) = (cy - g / 2, cy + g / 2);
                    let (a, b, c) = if self.sort_keys[i].1 {
                        ((gx + g / 2, top), (gx + g, bottom), (gx, bottom))
                    } else {
                        ((gx, top), (gx + g, top), (gx + g / 2, bottom))
                    };
                    dc.fill_triangle(a, b, c, th.accent);
                    used = right - gx;
                }
                let need = dc.text_width(&c.name) + pad * 2 + used + gap + icon_w;
                if cw < need && need <= hi {
                    grow.push((ci, need));
                }
                Rect::new(
                    x + icon_w,
                    header.y,
                    (cw - pad - used - gap - icon_w).max(0),
                    header.h,
                )
                .intersection(&hcells)
            } else {
                Rect::new(clip.x + icon_w, clip.y, (clip.w - icon_w).max(0), clip.h)
            };
            let hy = dc.text_center_y(header.y, header.h);
            dc.text(
                x + pad + icon_w,
                hy,
                name_clip,
                &c.name,
                if filtered_col { th.accent } else { th.text },
            );
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
        for (ci, w) in grow {
            if let Some(cw) = self.col_w.get_mut(ci) {
                *cw = w;
                self.dirty = true;
            }
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
        if let Some(st) = self.edit_status_text() {
            info.push_str(&format!(" · {st}"));
        }
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
/// 16진수 덤프(오프셋 · 16바이트/줄 · 문자 열) — 값 보기 창.
pub(crate) fn hex_dump(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() * 4 + 64);
    out.push_str(&format!("-- {} bytes\n", b.len()));
    for (i, chunk) in b.chunks(16).enumerate() {
        out.push_str(&format!("{:08x}  ", i * 16));
        for (j, byte) in chunk.iter().enumerate() {
            out.push_str(&format!("{byte:02x} "));
            if j == 7 {
                out.push(' ');
            }
        }
        for _ in chunk.len()..16 {
            out.push_str("   ");
        }
        if chunk.len() <= 8 {
            out.push(' ');
        }
        out.push_str(" |");
        for &byte in chunk {
            out.push(if (0x20..0x7f).contains(&byte) {
                byte as char
            } else {
                '.'
            });
        }
        out.push_str("|\n");
        if i >= 65_535 {
            out.push_str("…\n");
            break;
        }
    }
    out
}

/// 헤더 표식 툴팁 머무름(ms).
const MARK_TIP_MS: u64 = 450;

/// 부분 결과에 필터를 걸 때의 방식(설정 `grid.filter_server` · D-257).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilterServerMode {
    /// 서버에서 걸러 같은 탭에 다시 조회(기본).
    Auto,
    /// 가져온 행에서만(채움 상한까지 더 가져옴).
    Local,
    /// 처음 한 번 안내(지금은 안내 뒤 로컬과 같다 · 모달은 후속).
    Ask,
}

impl FilterServerMode {
    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "local" => FilterServerMode::Local,
            "ask" => FilterServerMode::Ask,
            _ => FilterServerMode::Auto,
        }
    }
}

/// 필터 변경 뒤 할 일(T-285 · 103 §4-1 판정표).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FetchPlan {
    /// 서버 승격(같은 탭 재조회).
    Server,
    /// 로컬 = 채움 상한까지 자동 페치.
    Fill,
    /// 안내 1회 뒤 로컬.
    Ask,
}

/// 순수 판정(MC/DC): 결과가 완전(`more`=false · 승격 결과도 아님)이면 로컬이 정답 · 부분 + auto + 번역 가능 + 편집 중 아님 = 서버 ·
/// `ask` = 안내 · 그 밖(local · 번역 불가 · 편집 중) = 채움 상한.
pub(crate) fn fetch_under_filter(
    more: bool,
    has_filter: bool,
    mode: FilterServerMode,
    translatable: bool,
    editing: bool,
) -> FetchPlan {
    if !more || !has_filter || editing || !translatable {
        return FetchPlan::Fill;
    }
    match mode {
        FilterServerMode::Auto => FetchPlan::Server,
        FilterServerMode::Ask => FetchPlan::Ask,
        FilterServerMode::Local => FetchPlan::Fill,
    }
}

/// ★ 그리드 필터 연산(T-181 · 77 §2-2 · 타입별 확장 사용자 09-29).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilterOp {
    Eq,
    Ne,
    Contains,
    StartsWith,
    Gt,
    Ge,
    Lt,
    Le,
    /// 값 = `lo..hi`(또는 `lo ~ hi` · `lo, hi`) · 양끝 포함.
    Between,
    IsTrue,
    IsFalse,
    IsNull,
    NotNull,
    /// 정규식(가져온 행에 클라이언트 판정 · 기본 대소문자 무시 `(?i)` · 값 = 패턴).
    Regex,
    /// 값 목록(`a, b, c` · `|`도 구분자) — 하나라도 같으면 통과 · "이 값만"을 반복하면 여기로 모인다.
    In,
    /// 값 목록 제외(`NOT IN`) — 여러 셀을 고르고 "이 값 제외: N개 선택"(사용자 10-07) · NULL은 통과(`Ne`와 같은 규칙).
    NotIn,
}

impl FilterOp {
    /// 팔레트 프롬프트 id 조각.
    pub(crate) fn code(self) -> &'static str {
        match self {
            FilterOp::Eq => "eq",
            FilterOp::Ne => "ne",
            FilterOp::Contains => "contains",
            FilterOp::StartsWith => "starts",
            FilterOp::Gt => "gt",
            FilterOp::Ge => "ge",
            FilterOp::Lt => "lt",
            FilterOp::Le => "le",
            FilterOp::Between => "between",
            FilterOp::IsTrue => "true",
            FilterOp::IsFalse => "false",
            FilterOp::IsNull => "null",
            FilterOp::NotNull => "notnull",
            FilterOp::Regex => "regex",
            FilterOp::In => "in",
            FilterOp::NotIn => "notin",
        }
    }
    pub(crate) fn parse(s: &str) -> Option<FilterOp> {
        Some(match s {
            "eq" => FilterOp::Eq,
            "ne" => FilterOp::Ne,
            "contains" => FilterOp::Contains,
            "starts" => FilterOp::StartsWith,
            "gt" => FilterOp::Gt,
            "ge" => FilterOp::Ge,
            "lt" => FilterOp::Lt,
            "le" => FilterOp::Le,
            "between" => FilterOp::Between,
            "true" => FilterOp::IsTrue,
            "false" => FilterOp::IsFalse,
            "null" => FilterOp::IsNull,
            "notnull" => FilterOp::NotNull,
            "regex" => FilterOp::Regex,
            "in" => FilterOp::In,
            "notin" => FilterOp::NotIn,
            _ => return None,
        })
    }
    /// 값을 입력받아야 하는 연산(팔레트 프롬프트).
    fn needs_prompt(self) -> bool {
        matches!(
            self,
            FilterOp::Contains
                | FilterOp::StartsWith
                | FilterOp::Gt
                | FilterOp::Ge
                | FilterOp::Lt
                | FilterOp::Le
                | FilterOp::Between
                | FilterOp::Regex
                | FilterOp::In
        )
    }
}

/// 컬럼 데이터 종류(필터 메뉴·비교 방식 · 드라이버 타입 이름 + 값 표본으로 판정).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColKind {
    Text,
    Number,
    Date,
    Bool,
}

impl ColKind {
    /// 드라이버 타입 이름으로 먼저(NUMBER/INT/DECIMAL/… · DATE/TIME/TIMESTAMP · BOOL/BIT) · 모르면 값 표본.
    pub(crate) fn infer(type_name: &str, sample: impl Iterator<Item = Value>) -> ColKind {
        let tn = type_name.to_ascii_lowercase().replace(' ', "");
        // 판정 흠(협업 10-07 ⑥): INTERVAL·POINT·GEOMETRY는 `int`를 품지만 숫자가 아니다 · MySQL TINYINT(1)·BIT(1) = 참/거짓.
        if tn.contains("interval") || tn.contains("point") || tn.contains("geom") {
            return ColKind::Text;
        }
        if tn.contains("bool") || tn == "bit" || tn == "bit(1)" || tn == "tinyint(1)" {
            return ColKind::Bool;
        }
        if tn.contains("date") || tn.contains("time") {
            return ColKind::Date;
        }
        if tn.contains("int")
            || tn.contains("num")
            || tn.contains("dec")
            || tn.contains("float")
            || tn.contains("double")
            || tn.contains("real")
            || tn.contains("money")
        {
            return ColKind::Number;
        }
        let (mut n, mut nums, mut bools, mut dates) = (0usize, 0usize, 0usize, 0usize);
        for v in sample {
            match &v {
                Value::Null => continue,
                Value::Int(_) | Value::Float(_) | Value::Decimal(_) => nums += 1,
                Value::Bool(_) => bools += 1,
                Value::Str(s) if looks_like_date(s) => dates += 1,
                _ => {}
            }
            n += 1;
        }
        if n == 0 {
            ColKind::Text
        } else if nums == n {
            ColKind::Number
        } else if bools == n {
            ColKind::Bool
        } else if dates == n {
            ColKind::Date
        } else {
            ColKind::Text
        }
    }
}

/// `YYYY-MM-DD…` / `YYYY/MM/DD…` / `YYYYMMDD` 꼴인가.
fn looks_like_date(s: &str) -> bool {
    let b = s.trim().as_bytes();
    let digits = |r: &[u8]| !r.is_empty() && r.iter().all(u8::is_ascii_digit);
    if b.len() >= 10
        && digits(&b[..4])
        && (b[4] == b'-' || b[4] == b'/')
        && digits(&b[5..7])
        && b[7] == b[4]
        && digits(&b[8..10])
    {
        return true;
    }
    b.len() == 8 && digits(b) && (b.starts_with(b"19") || b.starts_with(b"20"))
}

/// 날짜 글 정규화(`/` → `-` · 앞뒤 공백 제거) — 사전순 비교가 시간순이 되게.
fn norm_date(s: &str) -> String {
    s.trim().replace('/', "-")
}

fn num_of(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        Value::Decimal(d) => d.trim().parse().ok(),
        Value::Str(s) => s.trim().replace(',', "").parse().ok(),
        _ => None,
    }
}

fn bool_of(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Int(i) => Some(*i != 0),
        Value::Str(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "t" | "y" | "yes" | "1" => Some(true),
            "false" | "f" | "n" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// 술어 하나를 사람이 읽는 조건 글로(`= 값` · `포함 값` · `NULL만` …) — 머리 표식 툴팁과 필터 줄 칩이 같이 쓴다.
fn pred_text(p: &Predicate) -> String {
    match p.op {
        FilterOp::Eq => format!("= {}", p.value),
        FilterOp::Ne => format!("≠ {}", p.value),
        FilterOp::Gt => format!("> {}", p.value),
        FilterOp::Ge => format!("≥ {}", p.value),
        FilterOp::Lt => format!("< {}", p.value),
        FilterOp::Le => format!("≤ {}", p.value),
        FilterOp::Contains => tf(Msg::TipFilterContains, &[&p.value]),
        FilterOp::StartsWith => tf(Msg::TipFilterStarts, &[&p.value]),
        FilterOp::Between => tf(Msg::TipFilterBetween, &[&p.value]),
        FilterOp::Regex => tf(Msg::TipFilterRegex, &[&p.value]),
        FilterOp::In => tf(Msg::TipFilterIn, &[&p.value]),
        FilterOp::NotIn => tf(Msg::TipFilterNotIn, &[&p.value]),
        FilterOp::IsTrue => t(Msg::MnFilterTrue).to_string(),
        FilterOp::IsFalse => t(Msg::MnFilterFalse).to_string(),
        FilterOp::IsNull => t(Msg::MnFilterNull).to_string(),
        FilterOp::NotNull => t(Msg::MnFilterNotNull).to_string(),
    }
}

/// 필터 술어(AND 목록의 한 항 · 원본 열 번호 · 열 종류에 따라 비교). `rx` = 정규식 컴파일 캐시(비교에서는 제외).
#[derive(Clone, Debug)]
pub(crate) struct Predicate {
    pub col: usize,
    pub kind: ColKind,
    pub op: FilterOp,
    pub value: String,
    pub rx: Option<regex::Regex>,
}

impl PartialEq for Predicate {
    fn eq(&self, o: &Self) -> bool {
        self.col == o.col && self.kind == o.kind && self.op == o.op && self.value == o.value
    }
}

/// 값 목록 구분(`,` · `|` · 앞뒤 공백 제거 · 빈 항목 제외).
fn split_list(v: &str) -> Vec<String> {
    // ★ 큰따옴표로 감싼 항목은 안의 쉼표·`|`를 구분자로 보지 않는다(`""` = 따옴표 하나 · CSV식 · 사용자 10-06 "값에 쉼표").
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = v.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' && cur.trim().is_empty() {
            cur.clear();
            quoted = true;
        } else if c == ',' || c == '|' {
            let t = cur.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    let t = cur.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    out
}

/// 값 목록을 한 글로 — 쉼표·`|`·따옴표가 든 항목은 큰따옴표로 감싼다(`split_list`가 되돌린다).
fn join_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| {
            if s.contains([',', '|', '"']) || s != s.trim() {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// 술어의 값 목록 — `=`는 값 통째로 하나(쉼표가 들어 있어도 쪼개지 않는다) · IN은 `split_list`.
fn list_items(p: &Predicate) -> Vec<String> {
    if p.op == FilterOp::Eq {
        vec![p.value.clone()]
    } else {
        split_list(&p.value)
    }
}

/// 정규식 컴파일(사용자 패턴 · `(?`로 시작하지 않으면 `(?i)` 대소문자 무시).
pub(crate) fn compile_regex(pat: &str) -> Result<regex::Regex, String> {
    let p = if pat.starts_with("(?") {
        pat.to_string()
    } else {
        format!("(?i){pat}")
    };
    regex::Regex::new(&p).map_err(|e| e.to_string())
}

impl Predicate {
    pub(crate) fn new(
        col: usize,
        kind: ColKind,
        op: FilterOp,
        value: String,
    ) -> Result<Predicate, String> {
        let rx = if op == FilterOp::Regex {
            Some(compile_regex(&value)?)
        } else {
            None
        };
        Ok(Predicate {
            col,
            kind,
            op,
            value,
            rx,
        })
    }

    /// 범위 값 `lo..hi` · `lo ~ hi` · `lo, hi`.
    fn bounds(&self) -> Option<(String, String)> {
        for sep in ["..", "~", ","] {
            if let Some((a, b)) = self.value.split_once(sep) {
                return Some((a.trim().to_string(), b.trim().to_string()));
            }
        }
        None
    }

    /// 셀 값이 술어를 통과하는가 — 종류별: 숫자 = f64 · 날짜 = 정규화 글의 **경계 길이만큼** 사전순(`2026-09-29`로 `≤`면 그날 포함) ·
    /// 문자 = 대소문자 무시 · 불리언 = true/false·1/0·Y/N.
    pub(crate) fn pass_value(&self, v: &Value) -> bool {
        use std::cmp::Ordering;
        if matches!(self.op, FilterOp::IsNull) {
            return matches!(v, Value::Null);
        }
        if matches!(self.op, FilterOp::NotNull) {
            return !matches!(v, Value::Null);
        }
        if matches!(v, Value::Null) {
            return matches!(self.op, FilterOp::Ne | FilterOp::NotIn);
        }
        match self.op {
            FilterOp::Regex => self
                .rx
                .as_ref()
                .is_some_and(|rx| rx.is_match(&cell_text(v, ""))),
            FilterOp::In => split_list(&self.value).into_iter().any(|one| {
                Predicate {
                    op: FilterOp::Eq,
                    value: one,
                    rx: None,
                    ..self.clone()
                }
                .pass_value(v)
            }),
            FilterOp::NotIn => !split_list(&self.value).into_iter().any(|one| {
                Predicate {
                    op: FilterOp::Eq,
                    value: one,
                    rx: None,
                    ..self.clone()
                }
                .pass_value(v)
            }),
            FilterOp::IsTrue => bool_of(v) == Some(true),
            FilterOp::IsFalse => bool_of(v) == Some(false),
            FilterOp::Contains => cell_text(v, "")
                .to_lowercase()
                .contains(&self.value.to_lowercase()),
            FilterOp::StartsWith => cell_text(v, "")
                .to_lowercase()
                .starts_with(&self.value.to_lowercase()),
            FilterOp::Between => {
                let Some((lo, hi)) = self.bounds() else {
                    return true;
                };
                let ge = Predicate {
                    op: FilterOp::Ge,
                    value: lo,
                    ..self.clone()
                };
                let le = Predicate {
                    op: FilterOp::Le,
                    value: hi,
                    ..self.clone()
                };
                ge.pass_value(v) && le.pass_value(v)
            }
            FilterOp::Eq
            | FilterOp::Ne
            | FilterOp::Gt
            | FilterOp::Ge
            | FilterOp::Lt
            | FilterOp::Le => {
                let ord = match self.kind {
                    ColKind::Number => match (
                        num_of(v),
                        self.value.trim().replace(',', "").parse::<f64>().ok(),
                    ) {
                        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
                        _ => cell_text(v, "")
                            .to_lowercase()
                            .cmp(&self.value.to_lowercase()),
                    },
                    ColKind::Date => {
                        let a = norm_date(&cell_text(v, ""));
                        let b = norm_date(&self.value);
                        let a: String = a.chars().take(b.chars().count()).collect();
                        a.cmp(&b)
                    }
                    ColKind::Bool => bool_of(v).cmp(&bool_of(&Value::Str(self.value.clone()))),
                    ColKind::Text => cell_text(v, "")
                        .to_lowercase()
                        .cmp(&self.value.to_lowercase()),
                };
                match self.op {
                    FilterOp::Eq => ord == Ordering::Equal,
                    FilterOp::Ne => ord != Ordering::Equal,
                    FilterOp::Gt => ord == Ordering::Greater,
                    FilterOp::Ge => ord != Ordering::Less,
                    FilterOp::Lt => ord == Ordering::Less,
                    _ => ord != Ordering::Greater,
                }
            }
            FilterOp::IsNull | FilterOp::NotNull => unreachable!(),
        }
    }

    /// 조회용 SQL 술어(`q."COL" = '…'` · 숫자 열의 숫자 값은 인용 없이 · 리터럴 `'` 두 배 · BETWEEN · LIKE).
    pub(crate) fn to_sql(&self, col: &str) -> String {
        let c = format!("q.\"{}\"", col.replace('"', "\"\""));
        let lit = |v: &str| -> String {
            if self.kind == ColKind::Number && v.trim().replace(',', "").parse::<f64>().is_ok() {
                v.trim().replace(',', "")
            } else {
                format!("'{}'", v.replace('\'', "''"))
            }
        };
        match self.op {
            FilterOp::Eq => format!("{c} = {}", lit(&self.value)),
            FilterOp::Ne => format!("{c} <> {}", lit(&self.value)),
            FilterOp::Gt => format!("{c} > {}", lit(&self.value)),
            FilterOp::Ge => format!("{c} >= {}", lit(&self.value)),
            FilterOp::Lt => format!("{c} < {}", lit(&self.value)),
            FilterOp::Le => format!("{c} <= {}", lit(&self.value)),
            FilterOp::Between => match self.bounds() {
                Some((a, b)) => format!("{c} BETWEEN {} AND {}", lit(&a), lit(&b)),
                None => format!("{c} = {}", lit(&self.value)),
            },
            FilterOp::Contains => format!("{c} LIKE '%{}%'", self.value.replace('\'', "''")),
            FilterOp::StartsWith => format!("{c} LIKE '{}%'", self.value.replace('\'', "''")),
            FilterOp::IsTrue => format!("{c} = TRUE"),
            FilterOp::IsFalse => format!("{c} = FALSE"),
            FilterOp::IsNull => format!("{c} IS NULL"),
            FilterOp::NotNull => format!("{c} IS NOT NULL"),
            FilterOp::In => {
                let items: Vec<String> = split_list(&self.value).iter().map(|s| lit(s)).collect();
                if items.is_empty() {
                    "1=1".to_string()
                } else {
                    format!("{c} IN ({})", join_list(&items))
                }
            }
            FilterOp::NotIn => {
                let items: Vec<String> = split_list(&self.value).iter().map(|s| lit(s)).collect();
                if items.is_empty() {
                    "1=1".to_string()
                } else {
                    format!("{c} NOT IN ({})", join_list(&items))
                }
            }
            // 정규식은 방언·값 목록이 필요해 [`Grid::regex_sql`]이 만든다.
            FilterOp::Regex => format!("{c} IS NOT NULL"),
        }
    }

    /// ★ 정규식 조회 SQL 3단계(사용자 09-29): ① 방언 정규식(Oracle `REGEXP_LIKE` · PostgreSQL `~*` · MySQL `REGEXP`) ②
    /// 부분집합을 LIKE로 번역(`^`·`$`·리터럴·`.`·`.*`·최상위 `(a|b)`) ③ 그것도 안 되면 통과한 값 목록 `IN (…)`(최대 `cap` =
    /// `grid.filter_list_max`). `matched`가 `cap`보다 길면 "상한에서 멈춤"으로 본다([`distinct_capped`]는 cap+1개까지만 모은다).
    /// 반환 = (술어, 설명 주석).
    pub(crate) fn regex_sql(
        &self,
        col: &str,
        dialect: Dialect,
        matched: &[String],
        cap: usize,
    ) -> (String, String) {
        let c = format!("q.\"{}\"", col.replace('"', "\"\""));
        let esc = |s: &str| s.replace('\'', "''");
        let pat = self.value.as_str();
        match dialect {
            Dialect::Oracle => {
                return (
                    format!("REGEXP_LIKE({c}, '{}', 'i')", esc(pat)),
                    format!("-- regex: {pat} → REGEXP_LIKE"),
                )
            }
            Dialect::Postgres => {
                return (
                    format!("{c} ~* '{}'", esc(pat)),
                    format!("-- regex: {pat} → ~*"),
                )
            }
            Dialect::Mysql => {
                return (
                    format!("{c} REGEXP '{}'", esc(pat)),
                    format!("-- regex: {pat} → REGEXP"),
                )
            }
            _ => {}
        }
        if let Some(likes) = regex_to_like(pat) {
            let parts: Vec<String> = likes
                .iter()
                .map(|l| format!("{c} LIKE '{}'", esc(l)))
                .collect();
            let sql = if parts.len() == 1 {
                parts[0].clone()
            } else {
                format!("({})", parts.join(" OR "))
            };
            return (sql, format!("-- regex: {pat} → LIKE"));
        }
        let cap = cap.max(1);
        let items: Vec<String> = matched
            .iter()
            .take(cap)
            .map(|v| format!("'{}'", esc(v)))
            .collect();
        let note = if matched.len() > cap {
            format!(
                "-- {}",
                tf(Msg::FilterRegexListCapped, &[pat, &cap.to_string()])
            )
        } else {
            format!(
                "-- {}",
                tf(Msg::FilterRegexList, &[pat, &matched.len().to_string()])
            )
        };
        if items.is_empty() {
            (format!("{c} IN (NULL)"), note)
        } else {
            (format!("{c} IN ({})", join_list(&items)), note)
        }
    }

    /// ③단계 값 목록이 필요한가 — 정규식 술어이고, 방언 정규식(①)도 LIKE 번역(②)도 안 될 때만. 순수.
    pub(crate) fn needs_value_list(&self, dialect: Dialect) -> bool {
        self.op == FilterOp::Regex
            && !matches!(
                dialect,
                Dialect::Oracle | Dialect::Postgres | Dialect::Mysql
            )
            && regex_to_like(&self.value).is_none()
    }
}

/// 열 머리 깔때기 표시 방법(설정 `grid.filter_funnel` · 사용자 10-06).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FunnelMode {
    Always,
    Hover,
    None,
}

impl FunnelMode {
    pub(crate) fn parse(s: &str) -> Self {
        match s.trim() {
            "hover" => FunnelMode::Hover,
            "none" | "off" => FunnelMode::None,
            _ => FunnelMode::Always,
        }
    }
}

/// 깔때기 도형의 안쪽 판정(빗금용 · `funnel_glyph`와 같은 치수) — 컵(사다리꼴) 또는 꼭지.
fn funnel_contains(r: Rect, px: f32, py: f32) -> bool {
    let g = r.w.max(6) as f32;
    let cup_h = ((r.w.max(6) * 55 / 100).max(3)) as f32;
    let stem_w = ((r.w.max(6) / 3).max(2)) as f32;
    let (x0, y0) = (r.x as f32, r.y as f32);
    let (lx, ly) = (px - x0, py - y0);
    if lx < 0.0 || lx > g || ly < 0.0 || ly > g {
        return false;
    }
    let sl = g / 2.0 - stem_w / 2.0;
    let sr = sl + stem_w;
    if ly <= cup_h {
        // 사다리꼴: 위 폭 g → 아래 폭 stem_w.
        let t = ly / cup_h;
        let left = sl * t;
        let right = g - (g - sr) * t;
        lx >= left && lx <= right
    } else {
        lx >= sl && lx <= sr
    }
}

/// 깔때기 — 위가 넓은 **사다리꼴 컵**(위 폭 g · 아래 폭 = 꼭지 폭 · 높이 55 %) + 아래로 내려오는 꼭지. 도형으로(글꼴 글리프 X ·
/// 3-OS 동일) · `r` = 정사각형 자리. 삼각형 하나로 그리면 작은 크기에서 `T`처럼 보였다(협업 V1 10-06).
/// 같은 문장인가 — 양끝 공백과 끝 `;`만 다른 글은 같은 문장(결과 도착 때 분할기가 다듬은 글 ↔ 우리가 낸 글).
fn same_stmt(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim().trim_end_matches(';').trim().to_string();
    n(a) == n(b)
}

/// 배지 색조(T-301 · 사용자 색 규칙 10-07): 녹 = 예전엔 흔했지만 지금은 드문(레거시) · 파 = 자주 쓰는 일반 · 빨 = 특수.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BadgeTone {
    Green,
    Blue,
    Red,
}

/// 배지 색 — 어두운 테마는 흰색 쪽으로 35 % 섞어 밝게.
fn badge_color(tone: BadgeTone, th: &Theme) -> nexa_ctl::Color {
    let base = match tone {
        BadgeTone::Green => nexa_ctl::Color(0x001B_7F3B),
        BadgeTone::Blue => nexa_ctl::Color(0x001F_4FBF),
        BadgeTone::Red => nexa_ctl::Color(0x00B3_261E),
    };
    if th.is_dark {
        base.lerp(nexa_ctl::Color(0x00FF_FFFF), 0.35)
    } else {
        base
    }
}

/// ★ 열 유형 **2글자 배지**(T-301 · 사용자 표 + 색 규칙 + 협업 추천 매핑 10-07): 드라이버 타입 이름(방언별) 먼저 · 모르면 표본 · 그래도 모르면 `EX` 빨.
/// `AZ` 녹 = CHAR/NCHAR · SQL Server TEXT/NTEXT · Oracle LONG | 파 = VARCHAR/VARCHAR2/NVARCHAR · PG TEXT/CITEXT | 빨 = CLOB/NCLOB ·
/// MySQL TEXT 계열 · (N)VARCHAR(MAX) · `DT` 녹 = SMALLDATETIME · YEAR | 파 = DATE/DATETIME/TIME | 빨 = TIMESTAMP(TZ) · DATETIME2 ·
/// DATETIMEOFFSET · INTERVAL · `09` 녹 = MONEY/SMALLMONEY · BIT(n>1) | 파 = 정수 · NUMBER(p,0) | 빨 = 실수(NUMBER(p,s>0) · DECIMAL ·
/// FLOAT · REAL · DOUBLE · BINARY_FLOAT/DOUBLE) · `TF` 파 = BOOLEAN · SQL Server BIT | 녹 = MySQL TINYINT(1)/BIT(1) · `BI` 녹 = IMAGE ·
/// LONG RAW · RAW | 빨 = BLOB/BYTEA/VARBINARY/BINARY/BFILE · `ID` 파 = ROWID/UUID/UNIQUEIDENTIFIER/IDENTITY/SERIAL | 녹 = UROWID ·
/// `EX` 파 = XML/JSON/JSONB | 빨 = 공간·ARRAY·ENUM/SET·RANGE·INET·TSVECTOR·HIERARCHYID·SQL_VARIANT·ROWVERSION(SQL Server TIMESTAMP)·REF CURSOR.
pub(crate) fn type_badge(
    dialect: Dialect,
    type_name: &str,
    sample: impl Iterator<Item = Value>,
) -> (&'static str, BadgeTone) {
    use BadgeTone::{Blue, Green, Red};
    let tn = type_name.to_ascii_lowercase().replace(' ', "");
    let has = |k: &str| tn.contains(k);
    let mssql = dialect == Dialect::Mssql;
    let mysql = dialect == Dialect::Mysql;
    // 식별자 · 참거짓 · 이진(이름이 겹치는 것들보다 먼저).
    if has("urowid") {
        return ("ID", Green);
    }
    if has("rowid") || has("uuid") || has("uniqueidentifier") || has("identity") || has("serial") {
        return ("ID", Blue);
    }
    if tn == "tinyint(1)" || (mysql && tn == "bit(1)") {
        return ("TF", Green);
    }
    if has("bool") || tn == "bit" || tn == "bit(1)" {
        return ("TF", Blue);
    }
    if tn == "image" || has("longraw") || tn.starts_with("raw") {
        return ("BI", Green);
    }
    if has("blob") || has("binary") || has("bytea") || has("bfile") {
        return ("BI", Red);
    }
    // SQL Server TIMESTAMP/ROWVERSION = 행 버전(일시 아님).
    if has("rowversion") || (mssql && tn == "timestamp") {
        return ("EX", Red);
    }
    if has("xml") || has("json") {
        return ("EX", Blue);
    }
    if has("geom")
        || has("geog")
        || has("point")
        || has("array")
        || has("enum")
        || tn.starts_with("set(")
        || has("range")
        || has("inet")
        || has("cidr")
        || has("macaddr")
        || has("tsvector")
        || has("hierarchyid")
        || has("sql_variant")
        || has("cursor")
        || has("sdo_")
        || has("varray")
        || has("object")
    {
        return ("EX", Red);
    }
    // 일시.
    if has("smalldatetime") || tn == "year" {
        return ("DT", Green);
    }
    if has("timestamp") || has("datetime2") || has("datetimeoffset") || has("interval") {
        return ("DT", Red);
    }
    if has("date") || has("time") {
        return ("DT", Blue);
    }
    // 문자.
    if has("varchar(max)") || has("nvarchar(max)") {
        return ("AZ", Red);
    }
    if has("clob") || (has("text") && !mssql && dialect != Dialect::Postgres) {
        return ("AZ", Red);
    }
    if has("text") && mssql {
        return ("AZ", Green);
    }
    if has("text") || has("citext") {
        return ("AZ", Blue);
    }
    if tn == "long" {
        return ("AZ", Green);
    }
    if has("varchar") || has("varying") || has("string") {
        return ("AZ", Blue);
    }
    if tn.starts_with("nchar")
        || tn.starts_with("char")
        || tn.starts_with("character")
        || tn.starts_with("bpchar")
    {
        return ("AZ", Green);
    }
    // 숫자: MONEY/BIT(n>1) = 녹 · (p,s) → s>0 = 실수 · 이름으로 정수/실수 · 자릿수 미상 = 표본.
    if has("money") || (tn.starts_with("bit(") && tn != "bit(1)") {
        return ("09", Green);
    }
    let scale = tn
        .rsplit_once(',')
        .and_then(|(_, s)| s.trim_end_matches(')').parse::<i32>().ok());
    if has("int") || has("num") || has("dec") || has("float") || has("double") || has("real") {
        if let Some(s) = scale {
            return ("09", if s > 0 { Red } else { Blue });
        }
        if has("float") || has("double") || has("real") {
            return ("09", Red);
        }
        if has("int") && !has("num") {
            return ("09", Blue);
        }
        if has("dec") || tn == "numeric" {
            return ("09", Red);
        }
        // NUMBER 자릿수 미상 = 표본에 소수점이 있으면 실수(표본 없음 = 실수로).
        let mut n = 0usize;
        let mut frac = false;
        for v in sample {
            match &v {
                Value::Null => continue,
                Value::Float(_) => frac = true,
                Value::Decimal(_) | Value::Str(_) => {
                    let s = cell_text(&v, "");
                    if s.contains('.') && !s.trim_end_matches('0').ends_with('.') {
                        frac = true;
                    }
                }
                _ => {}
            }
            n += 1;
        }
        return ("09", if frac || n == 0 { Red } else { Blue });
    }
    // 이름을 모르면 표본으로 · 그래도 모르면 EX 빨.
    let (mut n, mut ints, mut floats, mut bools, mut dates, mut strs) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    for v in sample {
        match &v {
            Value::Null => continue,
            Value::Int(_) => ints += 1,
            Value::Float(_) | Value::Decimal(_) => floats += 1,
            Value::Bool(_) => bools += 1,
            Value::Str(s) if looks_like_date(s) => dates += 1,
            Value::Str(_) => strs += 1,
            _ => {}
        }
        n += 1;
    }
    if n > 0 && ints == n {
        ("09", Blue)
    } else if n > 0 && ints + floats == n {
        ("09", Red)
    } else if n > 0 && bools == n {
        ("TF", Blue)
    } else if n > 0 && dates == n {
        ("DT", Blue)
    } else if n > 0 && strs + dates == n {
        ("AZ", Blue)
    } else {
        ("EX", Red)
    }
}

fn funnel_glyph(dc: &mut dyn DrawCtx, r: Rect, color: nexa_ctl::Color) {
    let g = r.w.max(6);
    let cup_h = (g * 55 / 100).max(3);
    let stem_w = (g / 3).max(2);
    let (l, rgt) = (r.x, r.x + g);
    let sl = r.x + g / 2 - stem_w / 2;
    let sr = sl + stem_w;
    let yb = r.y + cup_h;
    // 사다리꼴 = 삼각형 둘(왼쪽 위·오른쪽 위·꼭지 오른쪽) + (왼쪽 위·꼭지 오른쪽·꼭지 왼쪽).
    dc.fill_triangle((l, r.y), (rgt, r.y), (sr, yb), color);
    dc.fill_triangle((l, r.y), (sr, yb), (sl, yb), color);
    dc.fill_rect(Rect::new(sl, yb - 1, stem_w, r.y + g - yb + 1), color);
}

/// 처음 나온 순서를 지키는 distinct — 해시 집합 O(n) · `cap`+1개가 모이면 멈춘다(상한 초과를 알 수 있게 · 사용자 09-30). 순수.
/// 조건 ▸ 메뉴의 술어 글(순수 · 사용자 10-07): `col`은 **이미 인용 정책을 거친** 열 글(`identq::quote` · `q.` 없이 — 조건 바 검증이 결과 열
/// 이름으로 확인한다) · 값은 숫자 열이고 숫자로 읽히면 그대로, 아니면 `'…'`(`'` 두 번) · LIKE 세 가지는 글자 그대로(와일드카드 이스케이프는
/// 하지 않는다 = 필터와 같음).
pub(crate) fn cond_pred_sql(
    col: &str,
    kind: ColKind,
    op: &str,
    value: Option<&str>,
) -> Option<String> {
    let c = col;
    let esc = |v: &str| v.replace('\'', "''");
    let lit = |v: &str| -> String {
        if kind == ColKind::Number && v.trim().replace(',', "").parse::<f64>().is_ok() {
            v.trim().replace(',', "")
        } else {
            format!("'{}'", esc(v))
        }
    };
    Some(match (op, value) {
        ("eq", Some(v)) => format!("{c} = {}", lit(v)),
        ("ne", Some(v)) => format!("{c} <> {}", lit(v)),
        ("starts", Some(v)) => format!("{c} LIKE '{}%'", esc(v)),
        ("contains", Some(v)) => format!("{c} LIKE '%{}%'", esc(v)),
        ("ends", Some(v)) => format!("{c} LIKE '%{}'", esc(v)),
        ("null", _) => format!("{c} IS NULL"),
        ("notnull", _) => format!("{c} IS NOT NULL"),
        _ => return None,
    })
}

pub(crate) fn distinct_capped(values: impl Iterator<Item = String>, cap: usize) -> Vec<String> {
    let stop = cap.max(1).saturating_add(1);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for v in values {
        if seen.insert(v.clone()) {
            out.push(v);
            if out.len() >= stop {
                break;
            }
        }
    }
    out
}

/// 정규식 부분집합 → LIKE 패턴들(대안마다 하나) — `^` `$` 앵커 · 리터럴 · `.`(→ `_`) · `.*`/`.+`(→ `%`/`_%`) · 최상위 `(a|b)` ·
/// `\.` 같은 이스케이프 리터럴. 그 밖의 메타(`+ ? { } [ ]`·백슬래시 클래스 · `%`/`_` 리터럴)가 있으면 `None`.
pub(crate) fn regex_to_like(pat: &str) -> Option<Vec<String>> {
    let mut s = pat.strip_prefix("(?i)").unwrap_or(pat);
    let anchored_start = s.starts_with('^');
    if anchored_start {
        s = &s[1..];
    }
    let anchored_end = s.ends_with('$') && !s.ends_with("\\$");
    if anchored_end {
        s = &s[..s.len() - 1];
    }
    // 그룹은 최상위 하나만(`접두(a|b)접미`) · 중첩·둘 이상은 포기.
    let (pre, alts, post): (&str, Vec<&str>, &str) = match (s.find('('), s.find(')')) {
        (None, None) => {
            if s.contains('|') {
                return None;
            }
            ("", vec![s], "")
        }
        (Some(a), Some(b)) if a < b => {
            let inner = &s[a + 1..b];
            let post = &s[b + 1..];
            if inner.contains('(') || post.contains('(') || post.contains(')') {
                return None;
            }
            (&s[..a], inner.split('|').collect(), post)
        }
        _ => return None,
    };
    let pre = lit_to_like(pre)?;
    let post = lit_to_like(post)?;
    let mut out = Vec::new();
    for a in alts {
        if a.contains('|') {
            return None;
        }
        let mid = lit_to_like(a)?;
        let mut like = format!("{pre}{mid}{post}");
        if !anchored_start {
            like.insert(0, '%');
        }
        if !anchored_end {
            like.push('%');
        }
        out.push(like);
    }
    Some(out)
}

/// 정규식 리터럴 조각 → LIKE 조각(`.` `_` · `.*` `%` · `.+` `_%` · `\\x` 이스케이프 리터럴 · 그 밖의 메타·`%`/`_`는 `None`).
fn lit_to_like(seg: &str) -> Option<String> {
    let mut like = String::new();
    let mut chars = seg.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let n = chars.next()?;
                if n.is_ascii_alphanumeric() || n == '%' || n == '_' {
                    return None;
                }
                like.push(n);
            }
            '.' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    like.push('%');
                } else if chars.peek() == Some(&'+') {
                    chars.next();
                    like.push_str("_%");
                } else {
                    like.push('_');
                }
            }
            '*' | '+' | '?' | '{' | '}' | '[' | ']' | '^' | '$' | '(' | ')' | '|' => return None,
            '%' | '_' => return None,
            c => like.push(c),
        }
    }
    Some(like)
}

/// 필터 판정용 셀 글(NULL = None · 이진 = `<n bytes>`).
fn cell_opt(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        other => Some(cell_text(other, "")),
    }
}

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

    /// 사용자 10-06 — 값에 쉼표가 있어도 값 목록 적용이 그 값에 맞고(인용) 다시 열면 체크돼 있다 · 종속 목록 = 다른 열 필터 통과 행의 값만.
    #[test]
    fn value_pick_with_commas_and_cascade() {
        let mut g = grid_with(&[100, 100]);
        let rs = ResultSet {
            columns: vec![
                Column {
                    name: "t".into(),
                    type_name: String::new(),
                },
                Column {
                    name: "n".into(),
                    type_name: String::new(),
                },
            ],
            rows: vec![
                vec![
                    Value::Str("RM".into()),
                    Value::Str("BOX,POWER PULSE,SMF".into()),
                ],
                vec![Value::Str("RM".into()), Value::Str("GSL700".into())],
                vec![Value::Str("FG".into()), Value::Str("ES100".into())],
            ],
        };
        g.set_result(rs);
        g.row_h = 20;
        // 쉼표 값 하나 고르기 → `=` 그대로 → 1행.
        g.set_pick_values(1, &["BOX,POWER PULSE,SMF".to_string()]);
        assert_eq!(g.rows(), 1);
        g.open_value_pick(1);
        assert!(
            g.value_pick_mut()
                .dump()
                .contains("[x] BOX,POWER PULSE,SMF"),
            "{}",
            g.value_pick_mut().dump()
        );
        g.value_pick_mut().close();
        // 쉼표 값 + 다른 값 = IN(인용) → 2행 · 조회 SQL에도 두 항목.
        g.set_pick_values(
            1,
            &["BOX,POWER PULSE,SMF".to_string(), "GSL700".to_string()],
        );
        assert_eq!(g.rows(), 2);
        assert_eq!(
            split_list(&g.filters[0].value),
            vec!["BOX,POWER PULSE,SMF", "GSL700"]
        );
        // 종속 목록: t = FG를 걸면 n 목록은 ES100만 · 범위 = all이면 셋 다.
        g.set_pick_values(1, &[]);
        g.set_pick_values(0, &["FG".to_string()]);
        g.open_value_pick(1);
        assert!(
            g.value_pick_mut().dump().contains("visible=1/1"),
            "{}",
            g.value_pick_mut().dump()
        );
        g.value_pick_mut().close();
        g.set_filter_values_scope("all");
        g.open_value_pick(1);
        assert!(g.value_pick_mut().dump().contains("visible=3/3"));
        g.value_pick_mut().close();
        // 엑셀 검색(사용자 10-06): 검색 결과로 확인 = 그 값으로 바꿈 · "필터에 추가" = 기존 값에 더함 · 검색어는 저장하지 않는다.
        g.value_pick_mut().close();
        //   (열 0의 FG 필터는 종속 목록을 줄이므로 먼저 푼다.)
        g.set_pick_values(0, &[]);
        g.set_pick_values(1, &["ES100".to_string()]);
        g.open_value_pick(1);
        g.value_pick_mut().set_search("GSL");
        g.value_pick_apply();
        let col1 = |g: &Grid| {
            g.filters
                .iter()
                .find(|p| p.col == 1)
                .map(|p| p.value.clone())
        };
        assert_eq!(col1(&g).as_deref(), Some("GSL700"), "바꿈");
        g.open_value_pick(1);
        assert!(
            g.value_pick_mut().dump().contains("search= "),
            "검색어 저장 없음"
        );
        g.value_pick_mut().set_search("ES");
        g.value_pick_mut().toggle_add_to_filter();
        g.value_pick_apply();
        assert_eq!(
            split_list(&col1(&g).unwrap_or_default()),
            vec!["GSL700", "ES100"],
            "추가"
        );
        g.set_pick_values(1, &[]);
        g.set_pick_values(0, &["FG".to_string()]);
        // 자기 열 필터는 목록 범위에서 뺀다(고른 값이 체크된 채 다른 값도 보인다).
        g.set_filter_values_scope("others");
        g.open_value_pick(0);
        let d = g.value_pick_mut().dump();
        assert!(d.contains("[x] FG") && d.contains("[ ] RM"), "{d}");
    }

    /// 사용자 10-06 — 필터 사용 여부: 끄면 걸린 필터가 풀리고 값 목록도 열리지 않는다 · 표시 방법 파싱 · 깔때기 안쪽 판정.
    #[test]
    fn filter_enabled_off_clears_and_blocks() {
        let mut g = grid_with(&[100]);
        let rs = ResultSet {
            columns: vec![Column {
                name: "c".into(),
                type_name: String::new(),
            }],
            rows: vec![vec![Value::Str("a".into())], vec![Value::Str("b".into())]],
        };
        g.set_result(rs);
        g.row_h = 20;
        g.add_filter(0, FilterOp::Eq, "a".into());
        assert_eq!(g.rows(), 1);
        g.set_filter_enabled(false);
        assert!(g.filters.is_empty() && g.rows() == 2);
        g.open_value_pick(0);
        assert!(!g.value_pick_open(), "꺼져 있으면 값 목록도 없다");
        g.set_filter_enabled(true);
        g.open_value_pick(0);
        assert!(g.value_pick_open());
        assert_eq!(FunnelMode::parse("hover"), FunnelMode::Hover);
        assert_eq!(FunnelMode::parse("none"), FunnelMode::None);
        assert_eq!(FunnelMode::parse("anything"), FunnelMode::Always);
        let r = Rect::new(0, 0, 10, 10);
        assert!(funnel_contains(r, 5.0, 1.0), "컵 가운데 위");
        assert!(funnel_contains(r, 5.0, 9.0), "꼭지");
        assert!(!funnel_contains(r, 0.5, 9.0), "꼭지 옆 빈 곳");
        assert!(!funnel_contains(r, 12.0, 5.0), "밖");
    }

    /// T-181 후속 — 값 목록 팝업: 열면 고유값(NULL 제외) · 지금 걸린 값 체크 · 적용 = 하나 `=` / 여럿 IN / 비면 제거.
    #[test]
    fn value_pick_opens_and_applies() {
        let mut g = grid_with(&[100]);
        let rs = ResultSet {
            columns: vec![Column {
                name: "c".into(),
                type_name: String::new(),
            }],
            rows: vec![
                vec![Value::Str("b".into())],
                vec![Value::Null],
                vec![Value::Str("a".into())],
                vec![Value::Str("b".into())],
            ],
        };
        g.set_result(rs);
        g.row_h = 20;
        g.add_filter(0, FilterOp::Eq, "a".into());
        g.open_value_pick(0);
        assert!(g.value_pick_open());
        let d = g.value_pick_mut().dump();
        assert!(d.contains("[ ] b") && d.contains("[x] a"), "{d}");
        g.value_pick_mut().toggle(0); // + b → 전부 체크 = 필터 없음(엑셀)
        g.value_pick_apply();
        assert!(!g.value_pick_open());
        assert!(g.filters.is_empty(), "전부 체크 = 필터 없음");
        assert_eq!(g.rows(), 4);
        // 하나만 끄면 나머지 값 목록(IN).
        g.open_value_pick(0);
        g.value_pick_mut().toggle(1); // a 끔 → b만
        g.value_pick_apply();
        assert_eq!(g.filters.len(), 1);
        assert_eq!(g.filters[0].op, FilterOp::Eq);
        assert_eq!(g.filters[0].value, "b");
        assert_eq!(g.rows(), 2);
        g.set_pick_values(0, &[]);
        assert!(g.filters.is_empty());
        assert_eq!(g.rows(), 4);
    }

    /// T-181 값 고르기 — 고유값은 받은 행 순서 · NULL 제외 · 상한+1 · 토글 = 넣기(`=` → 목록) / 빼기(마지막 = 술어 제거).
    #[test]
    fn filter_pick_toggles_values() {
        let mut g = grid_with(&[100]);
        let rs = ResultSet {
            columns: vec![Column {
                name: "c".into(),
                type_name: String::new(),
            }],
            rows: vec![
                vec![Value::Str("b".into())],
                vec![Value::Null],
                vec![Value::Str("a".into())],
                vec![Value::Str("b".into())],
                vec![Value::Str("c".into())],
            ],
        };
        g.set_result(rs);
        g.set_filter_pick_max(5);
        assert_eq!(g.column_values(0), ["b", "a", "c"]);
        // ★ 여러 셀 선택(행 0~2 · 열 0) → "이 값만: N개" = IN(b, a) · "이 값 제외" = NOT IN · NULL은 제외 쪽에서 통과.
        g.regions = vec![(0, 2, 0, 0)];
        assert_eq!(
            g.selected_values(0),
            ["b", "a"],
            "표시 순 · 중복 없음 · NULL 제외"
        );
        g.menu_cell = Some((0, Some("b".into())));
        g.filter_pick("eq_sel");
        assert_eq!(g.filters[0].op, FilterOp::In);
        assert_eq!(g.row_order, vec![0, 2, 3]);
        g.filters.clear();
        g.apply_sort();
        g.filter_pick("ne_sel");
        assert_eq!(g.filters[0].op, FilterOp::NotIn);
        assert_eq!(g.row_order, vec![1, 4], "b·a 제외 = NULL 행 + c");
        g.filters.clear();
        g.apply_sort();
        g.regions.clear();
        g.menu_cell = Some((0, None));
        g.filter_pick("pick:0"); // b
        assert_eq!(g.row_order, vec![0, 3]);
        g.filter_pick("pick:1"); // + a → 목록
        assert_eq!(g.filters[0].op, FilterOp::In);
        assert_eq!(g.row_order, vec![0, 2, 3]);
        g.filter_pick("pick:0"); // b 빼기
        assert_eq!(g.row_order, vec![2]);
        g.filter_pick("pick:1"); // a 빼기 = 술어 제거
        assert!(g.filters.is_empty());
        assert_eq!(g.row_order.len(), 5);
        // 상한+1 = 넘침 표시 자리.
        g.set_filter_pick_max(5);
        let mut g2 = grid_with(&[100]);
        g2.set_result(ResultSet {
            columns: vec![Column {
                name: "c".into(),
                type_name: String::new(),
            }],
            rows: (0..10).map(|i| vec![Value::Int(i)]).collect(),
        });
        g2.set_filter_pick_max(5);
        assert_eq!(g2.column_values(0).len(), 6);
    }

    /// T-181 OR 결합 — 투영은 any/all · 조회용 Query는 괄호 묶음 · 술어 하나면 AND 꼴 그대로.
    #[test]
    fn filter_or_combines_predicates() {
        let mut g = grid_with(&[100, 100]);
        let rs = ResultSet {
            columns: vec![
                Column {
                    name: "a".into(),
                    type_name: String::new(),
                },
                Column {
                    name: "b".into(),
                    type_name: String::new(),
                },
            ],
            rows: vec![
                vec![Value::Int(1), Value::Int(10)],
                vec![Value::Int(2), Value::Int(20)],
                vec![Value::Int(3), Value::Int(30)],
            ],
        };
        g.set_result(rs);
        g.set_source_sql("SELECT a, b FROM t");
        g.add_filter(0, FilterOp::Eq, "1".into());
        g.add_filter(1, FilterOp::Eq, "20".into());
        assert_eq!(g.row_order, Vec::<usize>::new(), "AND = 둘 다 맞는 행 없음");
        g.set_filter_or(true);
        assert_eq!(g.row_order, vec![0, 1]);
        let q = g.filter_query().expect("query");
        assert!(q.contains("AND (\n"), "{q}");
        assert!(q.contains("\n    OR "), "{q}");
        g.set_filter_or(false);
        assert!(!g.filter_query().expect("query").contains(" OR "));
        g.remove_filter(1);
        g.set_filter_or(true);
        assert!(
            !g.filter_query().expect("query").contains(" OR "),
            "술어 하나 = 괄호 없음"
        );
    }

    /// T-180 ⑤ — 열 머리 → 탐색기의 대상은 단일 테이블 조회일 때만(조인 = 없음 · 첫 테이블로 잘못 풀지 않는다).
    #[test]
    fn reveal_table_only_for_single_table_queries() {
        let mut g = grid_with(&[100, 100]);
        g.set_source_sql("SELECT id, name FROM emp");
        assert_eq!(g.reveal_table().as_deref(), Some("emp"));
        g.set_source_sql("SELECT d.name FROM emp e JOIN dept d ON e.dept_id = d.id");
        assert_eq!(
            g.source_table().as_deref(),
            Some("emp"),
            "SQL 복사용 추정은 첫 테이블"
        );
        assert_eq!(g.reveal_table(), None);
        g.set_source_sql("SELECT a.x FROM a, b");
        assert_eq!(g.reveal_table(), None);
        g.set_source_sql("CREATE TABLE t (a INT)");
        assert_eq!(g.reveal_table(), None);
    }

    /// 조건 바(위) + 필터 칩 줄(아래 · 10-06): 둘 다 켜고 필터가 있으면 줄 높이 = 26 + 26 · 칩 줄은 조건 바 바로 아래 ·
    /// 설정을 끄면 조건 바만 · 필터가 없어도 조건 바만.
    #[test]
    fn condition_bar_above_and_chip_strip_below() {
        let mut g = grid_with(&[100, 100]);
        g.set_condition_bar(true);
        g.set_filter_strip(true);
        let outer = Rect::new(0, 0, 600, 300);
        g.set_bounds(outer);
        g.sync_strip(1.0);
        assert_eq!(
            (g.strip_h, g.cond_h, g.chip_h),
            (32, 32, 0),
            "필터 없음 = 조건 바만"
        );
        g.add_filter(0, FilterOp::NotNull, String::new());
        g.sync_strip(1.0);
        assert_eq!((g.strip_h, g.cond_h, g.chip_h), (58, 32, 26));
        assert_eq!(g.bounds, Rect::new(0, 58, 600, 242));
        assert_eq!(g.cond_rect(), Rect::new(0, 0, 600, 32));
        assert_eq!(
            g.chips_rect(),
            Rect::new(0, 32, 600, 26),
            "칩 줄 = 조건 바 아래"
        );
        g.set_filter_strip(false);
        g.sync_strip(1.0);
        assert_eq!((g.strip_h, g.chip_h), (32, 0), "설정 끔 = 조건 바만");
        g.set_condition_bar(false);
        g.sync_strip(1.0);
        assert_eq!((g.strip_h, g.cond_h, g.chip_h), (0, 0, 0));
    }

    /// T-181 필터 줄 — 필터가 생기면 그리기에서 한 줄을 차지하고(`bounds`가 그만큼 내려감) · 칩의 ×는 그 열만 ·
    /// 끝 ×는 전부 지운다 · 줄이 사라지면 영역이 되돌아온다 · 설정을 끄면 줄이 없다 · 줄 안의 클릭은 먹는다.
    #[test]
    fn filter_strip_takes_a_row_and_chips_remove_filters() {
        // 조건 바가 켜져 있으면 그 줄은 조건 바 — 칩 시험은 끈다(사용자 10-06 용도 변경).
        struct Dc;
        impl DrawCtx for Dc {
            fn fill_rect(&mut self, _r: Rect, _c: nexa_ctl::theme::Color) {}
            fn text_opaque(
                &mut self,
                _x: i32,
                _y: i32,
                _c: Rect,
                _t: &str,
                _f: nexa_ctl::theme::Color,
                _b: nexa_ctl::theme::Color,
            ) {
            }
            fn text(&mut self, _x: i32, _y: i32, _c: Rect, _t: &str, _f: nexa_ctl::theme::Color) {}
            fn text_width(&mut self, text: &str) -> i32 {
                text.chars().count() as i32 * 7
            }
        }
        let mut g = grid_with(&[100, 100]);
        g.set_condition_bar(false);
        let outer = Rect::new(0, 0, 600, 300);
        g.set_bounds(outer);
        let th = Theme::dark();
        let mut dc = Dc;
        let mut draw = |g: &mut Grid| {
            g.sync_strip(1.0);
            g.paint_filter_strip(&mut dc, &th, 1.0);
        };
        draw(&mut g);
        assert_eq!((g.strip_h, g.bounds), (0, outer), "필터 없음 = 줄 없음");
        g.add_filter(0, FilterOp::NotNull, String::new());
        g.add_filter(1, FilterOp::NotNull, String::new());
        draw(&mut g);
        assert_eq!(g.strip_h, 26);
        assert_eq!(g.bounds, Rect::new(0, 26, 600, 274));
        assert_eq!(g.chips.len(), 2);
        assert_eq!(g.dump_chips().lines().count(), 2);
        // 호스트가 같은 영역을 다시 줘도 줄은 유지된다.
        g.set_bounds(outer);
        assert_eq!(g.bounds, Rect::new(0, 26, 600, 274));
        // 칩 글 자리 클릭 = 먹기만(필터 그대로) · 첫 칩의 × = 그 열만 지움.
        let (_, chip, close) = g.chips[0];
        g.on_event(
            &InputEvent::MouseDown {
                x: chip.x + 2,
                y: chip.y + 2,
                shift: false,
                primary: false,
            },
            1.0,
        );
        assert_eq!(g.filters.len(), 2);
        g.on_event(
            &InputEvent::MouseDown {
                x: close.x + close.w / 2,
                y: close.y + close.h / 2,
                shift: false,
                primary: false,
            },
            1.0,
        );
        assert_eq!(g.filters.iter().map(|p| p.col).collect::<Vec<_>>(), [1]);
        draw(&mut g);
        let clear = g.chips_clear.expect("clear");
        g.on_event(
            &InputEvent::MouseDown {
                x: clear.x + 1,
                y: clear.y + 1,
                shift: false,
                primary: false,
            },
            1.0,
        );
        assert!(g.filters.is_empty());
        draw(&mut g);
        assert_eq!((g.strip_h, g.bounds), (0, outer), "줄이 사라지면 영역 복귀");
        // 설정 끔 = 필터가 있어도 줄 없음.
        g.set_filter_strip(false);
        g.add_filter(0, FilterOp::NotNull, String::new());
        draw(&mut g);
        assert_eq!((g.strip_h, g.bounds), (0, outer));
        assert!(g.dump_chips().is_empty());
    }

    /// T-181 필터 술어: 대소문자 무시 · NULL 판정 · 조회용 SQL(리터럴·식별자 인용).
    #[test]
    /// 정규식·값 목록·LIKE 번역·3단계 SQL(사용자 09-29).
    fn filter_regex_and_list() {
        let s = |x: &str| Value::Str(x.into());
        let rx =
            Predicate::new(0, ColKind::Text, FilterOp::Regex, "^(pcm|pcc)6".into()).expect("rx");
        assert!(rx.pass_value(&s("PCM62620")) && !rx.pass_value(&s("pcc08868")));
        assert!(Predicate::new(0, ColKind::Text, FilterOp::Regex, "(".into()).is_err());
        let list = Predicate::new(0, ColKind::Number, FilterOp::In, "1, 3, 8".into()).expect("in");
        assert!(list.pass_value(&Value::Int(3)) && !list.pass_value(&Value::Int(2)));
        assert_eq!(list.to_sql("N"), "q.\"N\" IN (1, 3, 8)");
        assert_eq!(
            Predicate::new(0, ColKind::Text, FilterOp::In, "a|b".into())
                .expect("in")
                .to_sql("C"),
            "q.\"C\" IN ('a', 'b')"
        );
        // LIKE 번역.
        assert_eq!(regex_to_like("^abc"), Some(vec!["abc%".to_string()]));
        assert_eq!(regex_to_like("abc$"), Some(vec!["%abc".to_string()]));
        assert_eq!(
            regex_to_like("^(A|B)"),
            Some(vec!["A%".to_string(), "B%".to_string()])
        );
        assert_eq!(regex_to_like("a.c"), Some(vec!["%a_c%".to_string()]));
        assert_eq!(regex_to_like("^a.*z$"), Some(vec!["a%z".to_string()]));
        assert_eq!(regex_to_like("a\\.b"), Some(vec!["%a.b%".to_string()]));
        assert_eq!(regex_to_like("^\\d+$"), None);
        assert_eq!(regex_to_like("a[bc]"), None);
        // 3단계 SQL.
        let (o, _) = rx.regex_sql("C", Dialect::Oracle, &[], 1000);
        assert_eq!(o, "REGEXP_LIKE(q.\"C\", '^(pcm|pcc)6', 'i')");
        let (pg, _) = rx.regex_sql("C", Dialect::Postgres, &[], 1000);
        assert_eq!(pg, "q.\"C\" ~* '^(pcm|pcc)6'");
        let (ms, note) = rx.regex_sql("C", Dialect::Mssql, &[], 1000);
        assert_eq!(ms, "(q.\"C\" LIKE 'pcm6%' OR q.\"C\" LIKE 'pcc6%')");
        assert!(note.contains("LIKE"), "{note}");
        let hard =
            Predicate::new(0, ColKind::Text, FilterOp::Regex, "^\\d{3}$".into()).expect("rx");
        let (ms2, note2) = hard.regex_sql("C", Dialect::Mssql, &["123".into(), "456".into()], 1000);
        assert_eq!(ms2, "q.\"C\" IN ('123', '456')");
        assert!(note2.contains("2"), "{note2}");
        // 상한(사용자 09-30 · `grid.filter_list_max`): cap+1개 = 상한에서 멈춤 안내 · 목록은 cap개.
        let (ms3, note3) = hard.regex_sql(
            "C",
            Dialect::Mssql,
            &["1".into(), "2".into(), "3".into()],
            2,
        );
        assert_eq!(ms3, "q.\"C\" IN ('1', '2')");
        assert!(note3.contains("grid.filter_list_max"), "{note3}");
        // 값 목록은 ③단계에서만 모은다.
        assert!(!rx.needs_value_list(Dialect::Oracle));
        assert!(!rx.needs_value_list(Dialect::Mssql)); // LIKE 번역 가능
        assert!(hard.needs_value_list(Dialect::Mssql));
        assert!(!hard.needs_value_list(Dialect::Postgres));
    }

    /// T-285 판정표 MC/DC(5조건): 완전 결과·필터 없음·편집 중·번역 불가 = 채움 · 부분+auto = 서버 · ask = 안내 · local = 채움.
    #[test]
    fn fetch_under_filter_rules() {
        use FilterServerMode as M;
        let f = fetch_under_filter;
        assert_eq!(f(true, true, M::Auto, true, false), FetchPlan::Server);
        assert_eq!(
            f(false, true, M::Auto, true, false),
            FetchPlan::Fill,
            "완전 결과 = 로컬이 정답"
        );
        assert_eq!(
            f(true, false, M::Auto, true, false),
            FetchPlan::Fill,
            "필터 없음"
        );
        assert_eq!(
            f(true, true, M::Auto, true, true),
            FetchPlan::Fill,
            "편집 중 = 보류"
        );
        assert_eq!(
            f(true, true, M::Auto, false, false),
            FetchPlan::Fill,
            "번역 불가"
        );
        assert_eq!(f(true, true, M::Ask, true, false), FetchPlan::Ask);
        assert_eq!(f(true, true, M::Local, true, false), FetchPlan::Fill);
        assert_eq!(FilterServerMode::parse("local"), M::Local);
        assert_eq!(FilterServerMode::parse("x"), M::Auto);
    }

    /// 채움 카운터(D-258): 필터가 바뀌면 상한으로 · 필터 중 페이지마다 −1 · 0이면 자동 페치 조건이 꺼진다.
    #[test]
    fn fill_pages_counter() {
        let mut g = grid_with(&[1, 2, 3]);
        g.set_fill_pages(2);
        g.set_filter_server("local");
        g.add_filter(0, FilterOp::Eq, "1".into());
        assert_eq!(g.fill_pages_left(), 2);
        let page = |v: i32| ResultSet {
            columns: vec![nsql_core::Column {
                name: "a".into(),
                type_name: "INT".into(),
            }],
            rows: vec![vec![Value::Int(i64::from(v))]],
        };
        g.append_page(page(1), true);
        assert_eq!(g.fill_pages_left(), 1);
        g.append_page(page(1), true);
        assert_eq!(g.fill_pages_left(), 0);
        g.clear_filters();
        assert_eq!(g.fill_pages_left(), 2, "필터 변경 = 리셋");
    }

    /// T-301 배지 표(사용자 색 규칙 녹/파/빨 + 협업 추천 매핑 · 방언별) · ColKind 판정 흠.
    #[test]
    fn type_badge_table() {
        use nsql_core::Dialect as D;
        use BadgeTone::{Blue, Green, Red};
        let none = std::iter::empty::<Value>;
        let b = |d: D, t: &str| type_badge(d, t, none());
        assert_eq!(b(D::Oracle, "CHAR(10)"), ("AZ", Green));
        assert_eq!(b(D::Oracle, "VARCHAR2(20)"), ("AZ", Blue));
        assert_eq!(b(D::Oracle, "CLOB"), ("AZ", Red));
        assert_eq!(b(D::Oracle, "LONG"), ("AZ", Green));
        assert_eq!(
            b(D::Mssql, "text"),
            ("AZ", Green),
            "SQL Server TEXT = 레거시"
        );
        assert_eq!(b(D::Postgres, "text"), ("AZ", Blue), "PG TEXT = 일반");
        assert_eq!(b(D::Mysql, "longtext"), ("AZ", Red));
        assert_eq!(b(D::Mssql, "nvarchar(max)"), ("AZ", Red));
        assert_eq!(b(D::Oracle, "DATE"), ("DT", Blue));
        assert_eq!(b(D::Oracle, "TIMESTAMP(6) WITH TIME ZONE"), ("DT", Red));
        assert_eq!(b(D::Mssql, "smalldatetime"), ("DT", Green));
        assert_eq!(b(D::Oracle, "INTERVAL DAY TO SECOND"), ("DT", Red));
        assert_eq!(
            b(D::Mssql, "timestamp"),
            ("EX", Red),
            "SQL Server TIMESTAMP = 행 버전"
        );
        assert_eq!(b(D::Oracle, "INT"), ("09", Blue));
        assert_eq!(b(D::Oracle, "NUMBER(10,0)"), ("09", Blue));
        assert_eq!(b(D::Oracle, "NUMBER(10,2)"), ("09", Red));
        assert_eq!(b(D::Mssql, "money"), ("09", Green));
        assert_eq!(b(D::Mysql, "bit(8)"), ("09", Green));
        assert_eq!(
            type_badge(D::Oracle, "NUMBER", [Value::Int(1)].into_iter()),
            ("09", Blue)
        );
        assert_eq!(
            type_badge(D::Oracle, "NUMBER", [Value::Float(1.5)].into_iter()),
            ("09", Red)
        );
        assert_eq!(b(D::Oracle, "ROWID"), ("ID", Blue));
        assert_eq!(b(D::Oracle, "UROWID"), ("ID", Green));
        assert_eq!(b(D::Mysql, "tinyint(1)"), ("TF", Green));
        assert_eq!(b(D::Mssql, "bit"), ("TF", Blue));
        assert_eq!(b(D::Mssql, "image"), ("BI", Green));
        assert_eq!(b(D::Oracle, "RAW(16)"), ("BI", Green));
        assert_eq!(b(D::Postgres, "bytea"), ("BI", Red));
        assert_eq!(b(D::Oracle, "XMLTYPE"), ("EX", Blue));
        assert_eq!(b(D::Postgres, "jsonb"), ("EX", Blue));
        assert_eq!(b(D::Postgres, "point"), ("EX", Red));
        assert_eq!(b(D::Postgres, "tsvector"), ("EX", Red));
        assert_eq!(
            type_badge(D::Sqlite, "", [Value::Int(3)].into_iter()),
            ("09", Blue)
        );
        assert_eq!(
            type_badge(D::Sqlite, "", [Value::Str("a".into())].into_iter()),
            ("AZ", Blue)
        );
        assert_eq!(b(D::Sqlite, ""), ("EX", Red), "모르면 EX 빨");
        // ColKind 판정 흠(⑥).
        assert_eq!(
            ColKind::infer("INTERVAL DAY TO SECOND", none()),
            ColKind::Text
        );
        assert_eq!(ColKind::infer("point", none()), ColKind::Text);
        assert_eq!(ColKind::infer("tinyint(1)", none()), ColKind::Bool);
    }

    /// `fresh_like` = 새 탭 그리드가 설정을 **전부** 물려받는다(10-07 bin47 결함: 인용 정책·유형 아이콘이 기본값으로).
    #[test]
    fn fresh_like_keeps_new_settings() {
        let mut g = grid_with(&[1]);
        g.set_type_icons(true);
        g.set_cond_quote_always(true);
        g.set_cond_max_lines(7);
        g.set_cond_drop_template(false);
        let f = g.fresh_like();
        assert!(f.type_icons && f.cond_quote_always);
        assert_eq!(f.cond.max_lines_for_test(), 7);
        assert!(!f.cond.drop_template_for_test());
    }

    /// distinct 상한(사용자 09-30): 처음 나온 순서 · cap+1개에서 멈춤.
    #[test]
    fn cond_pred_sql_rules() {
        // 열 글은 호출자가 인용 정책을 거쳐 준다(`identq::quote`) — 여기서는 `"ITEM_CD"`를 그대로 받는다.
        let t = |op, v: Option<&str>| cond_pred_sql("\"ITEM_CD\"", ColKind::Text, op, v);
        assert_eq!(
            t("eq", Some("O'Neil")).as_deref(),
            Some("\"ITEM_CD\" = 'O''Neil'")
        );
        assert_eq!(t("ne", Some("a")).as_deref(), Some("\"ITEM_CD\" <> 'a'"));
        assert_eq!(
            t("starts", Some("P")).as_deref(),
            Some("\"ITEM_CD\" LIKE 'P%'")
        );
        assert_eq!(
            t("contains", Some("P")).as_deref(),
            Some("\"ITEM_CD\" LIKE '%P%'")
        );
        assert_eq!(
            t("ends", Some("P")).as_deref(),
            Some("\"ITEM_CD\" LIKE '%P'")
        );
        assert_eq!(t("null", None).as_deref(), Some("\"ITEM_CD\" IS NULL"));
        assert_eq!(
            t("notnull", Some("x")).as_deref(),
            Some("\"ITEM_CD\" IS NOT NULL")
        );
        assert!(t("eq", None).is_none(), "값 없는 = 는 없음");
        assert_eq!(
            cond_pred_sql("QTY", ColKind::Number, "eq", Some("1,200")).as_deref(),
            Some("QTY = 1200"),
            "숫자 열 = 숫자 리터럴 · 열 글은 받은 그대로"
        );
        assert_eq!(
            cond_pred_sql(
                &crate::identq::quote(nsql_core::Dialect::Oracle, "a\"b", false),
                ColKind::Text,
                "null",
                None
            )
            .as_deref(),
            Some("\"a\"\"b\" IS NULL"),
            "정책을 거친 열 = 식별자 따옴표 두 번"
        );
    }

    #[test]
    fn distinct_capped_stops_after_cap_plus_one() {
        let v = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let it = ["a", "b", "a", "c", "d", "e"].iter().map(|x| x.to_string());
        assert_eq!(distinct_capped(it, 2), v(&["a", "b", "c"]));
        let it = ["a", "a", "b"].iter().map(|x| x.to_string());
        assert_eq!(distinct_capped(it, 10), v(&["a", "b"]));
        // 큰 입력도 O(n) — 20만 개 고유값에서 상한 1,000이면 1,001개.
        let it = (0..200_000).map(|i| i.to_string());
        assert_eq!(distinct_capped(it, 1000).len(), 1001);
    }

    #[test]
    fn filter_predicate_pass_and_sql() {
        let mk = |kind: ColKind, op: FilterOp, value: &str| Predicate {
            col: 0,
            kind,
            op,
            value: value.into(),
            rx: None,
        };
        let s = |x: &str| Value::Str(x.into());
        // 문자: 대소문자 무시 · 포함 · 시작 · NULL.
        let eq = mk(ColKind::Text, FilterOp::Eq, "abc");
        assert!(
            eq.pass_value(&s("ABC")) && !eq.pass_value(&s("abcd")) && !eq.pass_value(&Value::Null)
        );
        assert!(mk(ColKind::Text, FilterOp::Ne, "abc").pass_value(&Value::Null));
        assert!(mk(ColKind::Text, FilterOp::Contains, "한").pass_value(&s("대한민국")));
        assert!(mk(ColKind::Text, FilterOp::StartsWith, "PCM").pass_value(&s("pcm62620")));
        assert!(mk(ColKind::Text, FilterOp::IsNull, "").pass_value(&Value::Null));
        // 숫자: f64 비교 · 문자열 숫자도 · 범위.
        let gt = mk(ColKind::Number, FilterOp::Gt, "100");
        assert!(
            gt.pass_value(&Value::Int(288))
                && !gt.pass_value(&Value::Int(80))
                && gt.pass_value(&s("1,728"))
        );
        assert!(mk(ColKind::Number, FilterOp::Between, "20..100").pass_value(&Value::Float(80.0)));
        assert!(!mk(ColKind::Number, FilterOp::Between, "20 ~ 100").pass_value(&Value::Int(288)));
        // 날짜: 경계 길이만큼 접두 비교 = 그날 포함.
        let le = mk(ColKind::Date, FilterOp::Le, "2026-09-29");
        assert!(le.pass_value(&s("2026-09-29 15:00:00")) && !le.pass_value(&s("2026/09/30")));
        assert!(mk(ColKind::Date, FilterOp::Eq, "2026-09-29").pass_value(&s("2026-09-29 10:00")));
        // 불리언.
        assert!(mk(ColKind::Bool, FilterOp::IsTrue, "").pass_value(&Value::Bool(true)));
        assert!(mk(ColKind::Bool, FilterOp::IsFalse, "").pass_value(&s("N")));
        // 종류 추론.
        assert_eq!(
            ColKind::infer("NUMBER", std::iter::empty()),
            ColKind::Number
        );
        assert_eq!(
            ColKind::infer("timestamp", std::iter::empty()),
            ColKind::Date
        );
        assert_eq!(
            ColKind::infer("", [s("2026-09-29"), s("2026-09-30")].into_iter()),
            ColKind::Date
        );
        assert_eq!(
            ColKind::infer("", [Value::Int(1), Value::Null].into_iter()),
            ColKind::Number
        );
        assert_eq!(
            ColKind::infer("varchar2", [s("x")].into_iter()),
            ColKind::Text
        );
        // SQL: 숫자 열은 인용 없이 · 문자 인용 · BETWEEN · LIKE.
        assert_eq!(gt.to_sql("QTY"), "q.\"QTY\" > 100");
        assert_eq!(eq.to_sql("ITEM_CD"), "q.\"ITEM_CD\" = 'abc'");
        assert_eq!(
            mk(ColKind::Date, FilterOp::Between, "2026-01-01..2026-12-31").to_sql("D"),
            "q.\"D\" BETWEEN '2026-01-01' AND '2026-12-31'"
        );
        assert_eq!(
            mk(ColKind::Text, FilterOp::Contains, "o'k").to_sql("A\"B"),
            "q.\"A\"\"B\" LIKE '%o''k%'"
        );
    }

    /// 열마다 술어 하나(사용자 09-29): 다른 연산 = 앞 것 취소 · `=` 반복 = 값 목록 · 같은 연산 재진입 초기값 = 걸린 값.
    #[test]
    fn one_filter_per_column_and_prompt_prefill() {
        let mut g = Grid::default();
        g.set_result(ResultSet {
            columns: vec![Column {
                name: "A".into(),
                type_name: String::new(),
            }],
            rows: ["P52F00", "P5111F00", "x"]
                .iter()
                .map(|s| vec![Value::Str((*s).to_string())])
                .collect(),
        });
        g.add_filter(0, FilterOp::Regex, "^P[5][23].*F00$".into());
        assert_eq!(g.rows(), 1);
        g.add_filter(0, FilterOp::IsNull, String::new());
        assert_eq!(g.filters.len(), 1, "다른 연산 = 앞 것 취소");
        assert_eq!(g.filters[0].op, FilterOp::IsNull);
        g.add_filter(0, FilterOp::Eq, "x".into());
        g.add_filter(0, FilterOp::Eq, "P52F00".into());
        assert_eq!((g.filters.len(), g.filters[0].op), (1, FilterOp::In));
        assert_eq!(g.rows(), 2);
        // 같은 연산 재진입 = 프롬프트 초기값이 걸린 값.
        g.add_filter(0, FilterOp::Regex, "^P5".into());
        g.menu_cell = Some((0, Some("x".into())));
        g.filter_pick("regex");
        assert_eq!(
            g.take_filter_prompt(),
            Some((0, FilterOp::Regex, "^P5".to_string()))
        );
    }

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
        // ★ 증분 표시(사용자 10-09): 배치는 진행·■ 상태를 **유지한 채** 행만 이어 붙는다 · 빈 배치는 무시 · 열 수 그대로.
        g.append_live(ResultSet {
            columns: vec![],
            rows: vec![vec![Value::Int(5)], vec![Value::Int(6)]],
        });
        g.append_live(ResultSet {
            columns: vec![],
            rows: vec![],
        });
        assert_eq!(g.rows(), 3, "배치 2행 이어 붙음 · 빈 배치 무시");
        assert!(g.fetch_all_active(), "배치 뒤에도 전체 조회 진행 중");
        assert_eq!(g.fetch_tools_enabled(), (false, true), "배치 중에도 ■만");
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
        assert_eq!((g.rows(), g.scroll_y), (4, 40), "이어 붙고 위치 유지");
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
        assert_eq!(g.rows(), 5, "늦은 세그먼트를 버리지 않는다");
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
    /// ★ 고속 스크롤(09-30): ↓ 자동 반복이 빨리 이어지면 이동량이 배수 · 끄면 늘 한 행.
    #[test]
    fn fast_scroll_keys_accelerate() {
        let mk = |fast: bool| {
            let mut g = grid_with(&[100]);
            g.set_fast_override(Some(nexa_ctl::FastScroll {
                enabled: fast,
                ..nexa_ctl::FastScroll::default()
            }));
            let rs = ResultSet {
                columns: vec![Column {
                    name: "c0".into(),
                    type_name: String::new(),
                }],
                rows: (0..5000).map(|i| vec![Value::Int(i)]).collect(),
            };
            g.set_result(rs);
            g.row_h = 20;
            g.header_h = 21;
            g.gutter_w = 30;
            g.col_w = vec![100];
            let down = InputEvent::Key {
                key: Key::Down,
                shift: false,
                primary: false,
            };
            for _ in 0..40 {
                g.on_event(&down, 1.0);
            }
            g.scroll_y
        };
        let (slow, fast) = (mk(false), mk(true));
        assert!(slow > 0, "{slow}");
        assert!(fast > slow, "fast {fast} > slow {slow}");
    }

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

    /// ★ 자연 정렬(사용자 10-09 · 전역 `ui.sort_natural` = nexa-ctl `natural`): 글자 열은 숫자 구간을 수로(`ITEM2 < ITEM10`) · 끄면 글자 순 ·
    /// 숫자형 열은 어느 쪽이든 수 비교 · NULL은 늘 뒤 · `resort`가 걸린 정렬을 새 규칙으로 다시 적용.
    #[test]
    fn natural_sort_on_text_columns_follows_global_switch() {
        let mut g = Grid::default();
        let rs = ResultSet {
            columns: vec![
                Column {
                    name: "CODE".into(),
                    type_name: "VARCHAR2(10)".into(),
                },
                Column {
                    name: "N".into(),
                    type_name: "NUMBER".into(),
                },
            ],
            rows: vec![
                vec![Value::Str("ITEM10".into()), Value::Int(10)],
                vec![Value::Str("ITEM2".into()), Value::Int(2)],
                vec![Value::Null, Value::Null],
                vec![Value::Str("item3".into()), Value::Int(3)],
            ],
        };
        g.set_result(rs);
        let codes = |g: &Grid| -> Vec<String> {
            let rs = g.rs.as_ref().expect("result");
            (0..g.rows())
                .map(|di| match rs.cell(g.row_order[di], 0) {
                    Value::Null => "NULL".to_string(),
                    v => v.display(),
                })
                .collect()
        };
        nexa_ctl::natural::set_enabled(true);
        g.toggle_sort(0, false);
        assert_eq!(
            codes(&g),
            ["ITEM2", "item3", "ITEM10", "NULL"],
            "자연 정렬 · NULL 뒤"
        );
        nexa_ctl::natural::set_enabled(false);
        g.resort();
        assert_eq!(codes(&g), ["ITEM10", "ITEM2", "item3", "NULL"], "글자 순");
        nexa_ctl::natural::set_enabled(true);
        g.resort();
        assert_eq!(codes(&g), ["ITEM2", "item3", "ITEM10", "NULL"]);
        // 숫자형 열은 규칙과 무관하게 수 비교.
        g.toggle_sort(1, false);
        assert_eq!(codes(&g), ["ITEM2", "item3", "ITEM10", "NULL"]);
    }
}

#[cfg(test)]
mod edit_key_path_tests {
    use super::*;
    use nsql_core::{Column, ResultSet};

    fn editable_grid() -> Grid {
        let mut g = Grid::default();
        let rs = ResultSet {
            columns: (0..2)
                .map(|i| Column {
                    name: format!("C{i}"),
                    type_name: "VARCHAR2(10)".into(),
                })
                .collect(),
            rows: vec![vec![Value::Null, Value::Str("x".into())]],
        };
        g.set_result(rs);
        g.bounds = Rect::new(0, 0, 400, 300);
        g.row_h = 20;
        g.header_h = 21;
        g.gutter_w = 30;
        g.col_w = vec![100, 100];
        g.set_edit_cfg(EditCfg {
            rowid: false,
            ..EditCfg::default()
        });
        g.set_result_origin("select C0, C1 from T", true);
        g.set_keys(None);
        g
    }

    fn key(k: Key) -> InputEvent {
        InputEvent::Key {
            key: k,
            shift: false,
            primary: false,
        }
    }

    fn rs_cols(names: &[&str], row: Vec<Value>) -> ResultSet {
        ResultSet {
            columns: names
                .iter()
                .map(|n| Column {
                    name: (*n).into(),
                    type_name: "VARCHAR2(10)".into(),
                })
                .collect(),
            rows: vec![row],
        }
    }

    fn requery_of(g: &mut Grid) -> Option<String> {
        g.take_edit_requests().into_iter().find_map(|r| match r {
            EditRequest::Requery { sql } => Some(sql),
            _ => None,
        })
    }

    /// ★ 2급(87 §13 · T-231): 키 없음 → ROWID 주입 재조회 요청 → 재조회 결과의 뒤쪽 열 = 숨은 열(화면 제외) → 키 = ROWID → WHERE ROWID.
    #[test]
    fn physical_inject_requery_then_hidden_column() {
        let mut g = Grid::default(); // Oracle
        g.set_edit_cfg(EditCfg {
            rowid: true,
            hidden_keys: true,
            ..EditCfg::default()
        }); // 09-27 기본 off → 명시
        g.set_result(rs_cols(
            &["C0", "C1"],
            vec![Value::Null, Value::Str("x".into())],
        ));
        g.set_result_origin("select C0, C1 from T", true);
        g.set_keys(None);
        assert_eq!(
            requery_of(&mut g).as_deref(),
            Some("select C0, C1, ROWID from T")
        );
        assert!(g.inject_pending());
        assert!(g.dump_edit().contains("ready=false"));
        assert!(!g.edit.as_ref().is_some_and(|e| e.keys_ready));
        // 호스트가 출처를 바꿔 재실행 → 결과 도착(3열).
        g.set_source_sql("select C0, C1, ROWID from T");
        g.set_result(rs_cols(
            &["C0", "C1", "ROWID"],
            vec![
                Value::Null,
                Value::Str("x".into()),
                Value::Str("AAAB".into()),
            ],
        ));
        g.set_result_origin("select C0, C1, ROWID from T;", true);
        assert!(g
            .take_edit_requests()
            .iter()
            .any(|r| matches!(r, EditRequest::NeedKeys { .. })));
        g.set_keys(None);
        let d = g.dump_edit();
        assert!(
            d.contains("kind=Physical") && d.contains("key=[2]") && d.contains("hidden=1"),
            "{d}"
        );
        assert_eq!(g.col_order, vec![0, 1], "숨은 열은 화면 열 순서에 없다");
        assert!(!g.inject_pending());
        assert!(g.set_cell_for_test(0, 1, "y"));
        let (st, _, _) = g.generated().expect("generate");
        assert_eq!(st[0].req.sql, "UPDATE T SET \"C1\" = :P1 WHERE ROWID = :P2");
        assert_eq!(st[0].req.params[1].value, Value::Str("AAAB".into()));
        // 재조회(적용 뒤 requery)로 같은 문장이 다시 와도 계획은 살아 있다.
        g.set_result(rs_cols(
            &["C0", "C1", "ROWID"],
            vec![
                Value::Null,
                Value::Str("y".into()),
                Value::Str("AAAB".into()),
            ],
        ));
        g.set_result_origin("select C0, C1, ROWID from T", true);
        g.set_keys(None);
        assert!(g.dump_edit().contains("kind=Physical"));
        // 다른 문장이 오면 계획을 버린다.
        g.set_result(rs_cols(&["Z"], vec![Value::Null]));
        g.set_result_origin("select Z from T2", true);
        assert!(g.inject.is_none());
    }

    /// 재조회 실패(WITHOUT ROWID 표 · 게이트 닫힘) → 원문 복귀 + 주입 없이 3급 · 같은 문장에 다시 주입하지 않는다.
    /// 09-27 D-225: 기본 설정은 재조회를 요구하지 않는다(3급 + 상태줄 힌트 · identify=true) → 우클릭 ▸ 행 식별 열 가져오기가
    /// 이 결과에 한해 ROWID 재조회를 한 번 요청한다 → 그 뒤 identify=false.
    #[test]
    fn default_cfg_no_requery_then_identify_on_demand() {
        let mut g = Grid::default(); // Oracle
        g.set_result(rs_cols(
            &["C0", "C1"],
            vec![Value::Null, Value::Str("x".into())],
        ));
        g.set_result_origin("select C0, C1 from T", true);
        g.set_keys(None);
        assert_eq!(requery_of(&mut g), None, "기본 = 재조회 없음");
        let d = g.dump_edit();
        assert!(
            d.contains("kind=AllColumns") && d.contains("identify=true"),
            "{d}"
        );
        assert!(g.can_identify());
        assert!(g.edit_command("grid.edit.identify"));
        assert_eq!(
            requery_of(&mut g).as_deref(),
            Some("select C0, C1, ROWID from T"),
            "온디맨드 재조회 1회"
        );
        assert!(!g.can_identify(), "두 번째는 없다");
        assert!(!g.edit_command("grid.edit.identify"));
    }

    #[test]
    fn requery_failed_falls_back_to_all_columns() {
        let mut g = Grid::default();
        g.set_dialect(Dialect::Sqlite);
        g.set_edit_cfg(EditCfg {
            rowid: true,
            hidden_keys: true,
            ..EditCfg::default()
        }); // 09-27 기본 off → 명시
        g.set_result(rs_cols(
            &["A", "B"],
            vec![Value::Str("k".into()), Value::Str("v".into())],
        ));
        g.set_result_origin("select a, b from t", true);
        g.set_keys(None);
        assert_eq!(
            requery_of(&mut g).as_deref(),
            Some("select a, b, rowid from t")
        );
        g.set_source_sql("select a, b, rowid from t");
        g.requery_failed();
        assert_eq!(g.source_sql(), "select a, b from t", "원문 복귀");
        let d = g.dump_edit();
        assert!(
            d.contains("kind=AllColumns") && d.contains("ready=true") && d.contains("key=[0, 1]"),
            "{d}"
        );
        assert!(g
            .take_edit_requests()
            .iter()
            .all(|r| !matches!(r, EditRequest::Requery { .. })));
        // 같은 문장을 다시 실행해 결과가 와도 주입을 다시 시도하지 않는다.
        g.set_result(rs_cols(
            &["A", "B"],
            vec![Value::Str("k".into()), Value::Str("v".into())],
        ));
        g.set_result_origin("select a, b from t", true);
        g.set_keys(None);
        assert!(requery_of(&mut g).is_none());
        assert!(g.dump_edit().contains("kind=AllColumns"));
        // ★ 09-28: 자동 재주입은 막혀도 **수동 버튼**은 켜져 있고, 누르면 실패 표식을 지우고 다시 시도한다(사용자 실기 "SELECT해도 버튼이
        //   안 켜짐" = 표식이 버튼까지 껐다).
        assert!(g.can_identify(), "실패 뒤 새 결과 = 버튼 켜짐");
        assert!(g.edit_identify(), "누르면 다시 재조회 요청");
        assert_eq!(
            requery_of(&mut g).as_deref(),
            Some("select a, b, rowid from t")
        );
        assert!(g.inject_pending());
        // 보내지 못한 경우(세션 바쁨) = 표식 없이 되돌림 → 버튼 그대로 켜짐.
        g.requery_aborted();
        assert!(!g.inject_pending() && g.can_identify());
        assert_eq!(g.source_sql(), "select a, b from t");
        // 정책 끔(rowid=false · all_cols=false) = 읽기 전용 NoKey.
        g.set_edit_cfg(EditCfg {
            rowid: false,
            all_cols: false,
            ..EditCfg::default()
        });
        g.set_result(rs_cols(
            &["A", "B"],
            vec![Value::Str("k".into()), Value::Str("v".into())],
        ));
        g.set_result_origin("select a, b from t2", true);
        g.set_keys(None);
        assert!(g.edit.is_none());
        assert_eq!(g.read_only, Some(ReadOnly::NoKey));
    }

    /// 1급-보완(D-214): PK는 있는데 결과에 없다 → 키 열 주입 → 재조회 결과에서 1급 · 숨은 키 열은 명세가 와도 읽기 전용.
    #[test]
    fn hidden_key_inject_when_pk_missing() {
        let mut g = Grid::default();
        g.set_edit_cfg(EditCfg {
            rowid: true,
            hidden_keys: true,
            ..EditCfg::default()
        }); // 09-27 기본 off → 명시
        g.set_result(rs_cols(
            &["NAME", "SAL"],
            vec![Value::Str("kim".into()), Value::Int(1)],
        ));
        g.set_result_origin("select name, sal from emp", true);
        let keys = nsql_core::KeyInfo {
            pk: vec!["ID".into()],
            unique: vec![],
        };
        g.set_keys(Some(&keys));
        assert_eq!(
            requery_of(&mut g).as_deref(),
            Some("select name, sal, \"ID\" from emp")
        );
        g.set_source_sql("select name, sal, \"ID\" from emp");
        g.set_result(rs_cols(
            &["NAME", "SAL", "ID"],
            vec![Value::Str("kim".into()), Value::Int(1), Value::Int(5)],
        ));
        g.set_result_origin("select name, sal, \"ID\" from emp", true);
        g.set_col_specs(&[CellSpec::new("ID", CellKind::Number).not_null()]);
        g.set_keys(Some(&keys));
        let d = g.dump_edit();
        assert!(
            d.contains("kind=Constraint") && d.contains("key=[2]") && d.contains("hidden=1"),
            "{d}"
        );
        assert!(g
            .edit
            .as_ref()
            .is_some_and(|e| e.cols[2].hidden && e.cols[2].spec.read_only));
        assert_eq!(g.col_order, vec![0, 1]);
        assert!(g.set_cell_for_test(0, 0, "lee"));
        let (st, _, _) = g.generated().expect("generate");
        assert_eq!(
            st[0].req.sql,
            "UPDATE emp SET \"NAME\" = :P1 WHERE \"ID\" = :P2"
        );
        assert_eq!(st[0].req.params[1].value, Value::Int(5));
        // 정책 끔이면 주입 없이 3급.
        let mut g2 = Grid::default();
        g2.set_edit_cfg(EditCfg {
            hidden_keys: false,
            rowid: false,
            ..EditCfg::default()
        });
        g2.set_result(rs_cols(
            &["NAME", "SAL"],
            vec![Value::Str("kim".into()), Value::Int(1)],
        ));
        g2.set_result_origin("select name, sal from emp", true);
        g2.set_keys(Some(&keys));
        assert!(requery_of(&mut g2).is_none());
        assert!(g2.dump_edit().contains("kind=AllColumns"));
    }

    /// ★ 행 단위 재조회 적용(87 §12-4 · T-230): 수정 행 제자리 교체 · 추가 행 = 실제 행(자리 유지) · 삭제 행 제거 · Σ 보정 · 1행 아니면 false.
    #[test]
    fn apply_done_rows_patches_in_place() {
        let mut g = Grid::default();
        g.set_edit_cfg(EditCfg {
            rowid: false,
            ..EditCfg::default()
        });
        let row = |a: i64, b: &str| vec![Value::Int(a), Value::Str(b.into())];
        g.set_result(ResultSet {
            columns: vec![
                Column {
                    name: "ID".into(),
                    type_name: "NUMBER".into(),
                },
                Column {
                    name: "NAME".into(),
                    type_name: "VARCHAR2(10)".into(),
                },
            ],
            rows: vec![row(1, "a"), row(2, "b"), row(3, "c")],
        });
        g.set_result_origin("select id, name from t", true);
        let keys = nsql_core::KeyInfo {
            pk: vec!["ID".into()],
            unique: vec![],
        };
        g.set_keys(Some(&keys));
        g.set_total(3);
        // 행 1 수정 · 행 0 뒤에 추가(키 9) · 행 2 삭제.
        assert!(g.set_cell_for_test(1, 1, "B2"));
        g.select_cell_for_test(0, 0);
        g.edit_command("row.add");
        let order_before = g.row_order.clone(); // [0, 3(가상), 1, 2]
        assert_eq!(order_before, vec![0, 3, 1, 2]);
        assert!(g.set_cell_for_test(1, 0, "9"));
        assert!(g.set_cell_for_test(1, 1, "new"));
        g.select_cell_for_test(3, 0);
        g.edit_command("row.del");
        g.request_apply();
        let (refetch, sent) = {
            let reqs = g.take_edit_requests();
            let r = reqs
                .into_iter()
                .find_map(|r| match r {
                    EditRequest::Apply { refetch, .. } => Some(refetch),
                    _ => None,
                })
                .expect("apply");
            (
                r,
                g.edit
                    .as_ref()
                    .map(|e| e.sent_refetch.clone())
                    .unwrap_or_default(),
            )
        };
        assert_eq!(refetch.len(), 2, "수정 1 + 추가 1(삭제는 재조회 없음)");
        assert_eq!(sent.len(), 2);
        let rs_of = |r: Vec<Value>| -> Result<ResultSet, String> {
            Ok(ResultSet {
                columns: vec![
                    Column {
                        name: "ID".into(),
                        type_name: "NUMBER".into(),
                    },
                    Column {
                        name: "NAME".into(),
                        type_name: "VARCHAR2(10)".into(),
                    },
                ],
                rows: vec![r],
            })
        };
        // 순서 = sent_refetch 순서(UPDATE 행 1 → INSERT).
        let results: Vec<Result<ResultSet, String>> = sent
            .iter()
            .map(|rr| match rr {
                RowRef::Existing(1) => rs_of(row(2, "B2*")),
                RowRef::Inserted(_) => rs_of(row(9, "new*")),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert!(g.apply_done_rows(results));
        let d = g.dump_edit();
        assert!(
            d.contains("dirty=false") && d.contains("patched=1/1/1"),
            "{d}"
        );
        // 데이터: 행 2(c) 제거 · 행 1 = 서버 값 B2* · 추가 행 = 9/new* 가 행 0 뒤 자리.
        let names: Vec<String> = g
            .row_order
            .iter()
            .map(|&ri| {
                g.rs.as_ref()
                    .and_then(|rs| rs.row(ri))
                    .map(|r| r[1].display())
                    .expect("row")
            })
            .collect();
        assert_eq!(names, vec!["a", "new*", "B2*"]);
        assert_eq!(g.total, Some(3), "Σ = 3 + 1 − 1");
        assert_eq!(g.src_len(), 3);
        // 결과가 1행이 아니면(다른 세션이 지움) false → 호스트가 전체 재조회.
        assert!(g.set_cell_for_test(0, 1, "z"));
        g.request_apply();
        let _ = g.take_edit_requests();
        let empty = Ok(ResultSet {
            columns: vec![],
            rows: vec![],
        });
        assert!(!g.apply_done_rows(vec![empty]));
    }

    /// 삭제만 있으면 재조회 없이 제거 · `local` = 편집한 값을 종류대로 세트에 굳힘(재조회 0).
    #[test]
    fn delete_only_and_local_bake() {
        let mut g = Grid::default();
        g.set_edit_cfg(EditCfg {
            rowid: false,
            ..EditCfg::default()
        });
        let row = |a: i64, b: &str| vec![Value::Int(a), Value::Str(b.into())];
        let cols = vec![
            Column {
                name: "ID".into(),
                type_name: "NUMBER".into(),
            },
            Column {
                name: "NAME".into(),
                type_name: "VARCHAR2(10)".into(),
            },
        ];
        g.set_result(ResultSet {
            columns: cols,
            rows: vec![row(1, "a"), row(2, "b"), row(3, "c")],
        });
        g.set_result_origin("select id, name from t", true);
        let keys = nsql_core::KeyInfo {
            pk: vec!["ID".into()],
            unique: vec![],
        };
        g.set_keys(Some(&keys));
        // 삭제만 → 재조회 문장 0 · 결과 0으로도 제자리 제거.
        g.select_cell_for_test(1, 0);
        g.edit_command("row.del");
        g.request_apply();
        let _ = g.take_edit_requests();
        assert!(g.apply_done_rows(Vec::new()));
        assert_eq!(g.src_len(), 2);
        assert!(g.dump_edit().contains("patched=0/0/1"));
        // local: 숫자 열은 숫자로 굳힌다 · 추가 행도.
        assert!(g.set_cell_for_test(0, 0, "10"));
        assert!(g.set_cell_for_test(1, 1, "C2"));
        g.edit_command("row.add");
        let last = g.rows() - 1;
        assert!(g.set_cell_for_test(last, 0, "77"));
        g.request_apply();
        let _ = g.take_edit_requests();
        assert!(g.apply_done_local());
        let rs = g.rs.as_ref().expect("rs");
        assert_eq!(rs.row(0).map(|r| r[0].clone()), Some(Value::Int(10)));
        assert_eq!(
            rs.row(1).map(|r| r[1].clone()),
            Some(Value::Str("C2".into()))
        );
        assert_eq!(rs.row(2).map(|r| r[0].clone()), Some(Value::Int(77)));
        assert!(g.dump_edit().contains("patched=2/1/0"));
        assert!(!g.edit_dirty());
    }

    /// LOB(87 §5): 파일에서 넣은 이진 = 셀 라벨 + 적용 문장 `Value::Bytes`(BLOB) 바인드 · 글로 덮어쓰면 이진은 버린다.
    #[test]
    fn blob_cell_binds_bytes_and_text_replaces_it() {
        let mut g = editable_grid();
        let bytes = vec![0x42u8, 0x4d, 0, 1, 2];
        g.set_cell_bytes(RowRef::Existing(0), 1, bytes.clone(), "px.bmp")
            .expect("bytes");
        assert_eq!(g.cell_bytes(RowRef::Existing(0), 1), Some(bytes.clone()));
        assert_eq!(
            g.cur_text(RowRef::Existing(0), 1).as_deref(),
            Some("<px.bmp · 5 bytes>")
        );
        let (st, _, _) = g.generated().expect("generate");
        assert_eq!(st[0].req.params[0].value, Value::Bytes(bytes));
        assert_eq!(st[0].req.params[0].ty, nsql_core::VarType::Blob);
        // 글로 덮어쓰기 = 이진 폐기 · 5,000자는 CLOB 타입(길이 상한 없는 명세로).
        g.set_col_specs(&[CellSpec::text("C1")]);
        let long = "x".repeat(5000);
        g.set_cell_value(RowRef::Existing(0), 1, Some(long.clone()))
            .expect("text");
        assert!(g.cell_bytes(RowRef::Existing(0), 1).is_none());
        let (st, _, _) = g.generated().expect("generate");
        assert_eq!(st[0].req.params[0].ty, nsql_core::VarType::Clob);
        assert_eq!(st[0].req.params[0].value, Value::Str(long));
        // 값 보기 요청 = 편집 가능 · 이진 아님.
        g.select_cell_for_test(0, 1);
        g.edit_command("grid.edit.view_value");
        let req = g.take_edit_requests().into_iter().find_map(|r| match r {
            EditRequest::ViewCell(v) => Some(v),
            _ => None,
        });
        let v = req.expect("view");
        assert!(v.editable && !v.binary && v.bytes.is_none() && v.text.len() == 5000);
    }

    /// 88 §3 15(T-233): 그리드 우클릭 메뉴의 서브메뉴는 1단 · 상위 항목 ≤ 16.
    #[test]
    fn context_menu_is_one_level_deep() {
        let mut g = editable_grid();
        g.select_cell_for_test(0, 0);
        g.open_menu(10, 30, 1.0);
        let items = g.menu.items_for_test();
        let depth = |it: &CtxItem| -> usize {
            fn d(it: &CtxItem) -> usize {
                match it {
                    CtxItem::Item { children, .. } if !children.is_empty() => {
                        1 + children.iter().map(d).max().unwrap_or(0)
                    }
                    _ => 0,
                }
            }
            d(it)
        };
        assert!(items.iter().all(|it| depth(it) <= 1), "서브메뉴 2단 없음");
        assert!(items
            .iter()
            .any(|it| matches!(it, CtxItem::Item { id, .. } if id == "copy_sql")));
        let n = items
            .iter()
            .filter(|it| matches!(it, CtxItem::Item { .. }))
            .count();
        assert!(n <= 16, "상위 항목 {n}");
    }

    /// 88 §3 2(T-232): 포커스 밖 = 비활성 선택색(알파 절반).
    #[test]
    fn selection_alpha_halves_without_focus() {
        assert_eq!(Grid::sel_alphas(true, 0.35), (0.35, 0.85));
        let (r, c) = Grid::sel_alphas(false, 0.35);
        assert!((r - 0.175).abs() < 1e-6 && (c - 0.45).abs() < 1e-6);
    }

    /// Tab / Shift+Tab: 편집기 밖 = 다음/이전 셀 · 편집기 안 = 커밋 뒤 오른쪽/왼쪽(87 §11).
    #[test]
    fn tab_moves_selection_and_commits() {
        let mut g = editable_grid();
        g.select_cell_for_test(0, 0);
        g.on_event(&InputEvent::Char { c: '\t', now_ms: 0 }, 1.0);
        assert_eq!(g.sel_cur, Some((0, 1)));
        g.set_shift(true);
        g.on_event(&InputEvent::Char { c: '\t', now_ms: 0 }, 1.0);
        assert_eq!(g.sel_cur, Some((0, 0)));
        g.set_shift(false);
        // 편집기 안: 타이핑 → Tab = 커밋 + 오른쪽.
        g.on_event(&InputEvent::Char { c: 'Q', now_ms: 0 }, 1.0);
        assert!(g.editing_cell());
        g.on_event(&InputEvent::Char { c: '\t', now_ms: 0 }, 1.0);
        assert!(!g.editing_cell());
        assert_eq!(g.sel_cur, Some((0, 1)));
        let e = g.edit.as_ref().expect("editable");
        assert_eq!(e.cs.cell(RowRef::Existing(0), 0), Some(&Some("Q".into())));
        // Shift+Tab = 커밋 + 왼쪽.
        g.on_event(&InputEvent::Char { c: 'W', now_ms: 0 }, 1.0);
        g.set_shift(true);
        g.on_event(&InputEvent::Char { c: '\t', now_ms: 0 }, 1.0);
        assert!(!g.editing_cell());
        assert_eq!(g.sel_cur, Some((0, 0)));
    }

    /// 실제 키 경로(사용자 09-26 "값 넣고 Enter/다른 셀 클릭에도 반영 안 됨"): 타이핑 진입 → 글자 → Enter = 변경 집합에 값.
    #[test]
    fn typing_then_enter_commits() {
        let mut g = editable_grid();
        assert!(g.edit.is_some(), "편집 가능");
        g.select_cell_for_test(0, 0);
        g.on_event(&InputEvent::Char { c: 'A', now_ms: 0 }, 1.0);
        assert!(g.editing_cell(), "타이핑 = 편집 진입");
        g.on_event(&InputEvent::Char { c: 'B', now_ms: 0 }, 1.0);
        g.on_event(&key(Key::Enter), 1.0);
        assert!(!g.editing_cell(), "Enter = 커밋 · 닫힘");
        let e = g.edit.as_ref().expect("editable");
        assert_eq!(e.cs.cell(RowRef::Existing(0), 0), Some(&Some("AB".into())));
        assert!(e.cs.is_dirty());
    }

    /// 최종 값이 원본과 같으면 변경이 아니다(사용자 09-26): x → y(수정) → x(제외) · Esc 취소 = 기록 없음.
    #[test]
    fn same_as_original_is_not_a_change() {
        let mut g = editable_grid();
        g.select_cell_for_test(0, 1); // 원본 "x"
        g.on_event(&InputEvent::Char { c: 'y', now_ms: 0 }, 1.0);
        g.on_event(&key(Key::Enter), 1.0);
        assert!(g.edit_dirty());
        assert_eq!(
            g.edit_status_text().as_deref().map(|s| s.contains('1')),
            Some(true)
        );
        g.select_cell_for_test(0, 1);
        g.on_event(&InputEvent::Char { c: 'x', now_ms: 0 }, 1.0);
        g.on_event(&key(Key::Enter), 1.0);
        assert!(!g.edit_dirty(), "x → y → x = 변경 아님");
        assert_eq!(
            g.edit
                .as_ref()
                .expect("editable")
                .cs
                .cell(RowRef::Existing(0), 1),
            None
        );
        // Esc = 취소 · 기록 없음.
        g.select_cell_for_test(0, 1);
        g.on_event(&InputEvent::Char { c: 'q', now_ms: 0 }, 1.0);
        g.on_event(&key(Key::Escape), 1.0);
        assert!(!g.editing_cell());
        assert!(!g.edit_dirty());
    }

    /// 편집 상자 안 더블클릭 = 단어 · 트리플 = 전체(줄) 선택(사용자 09-26).
    #[test]
    fn double_and_triple_click_inside_editor() {
        let mut g = editable_grid();
        g.select_cell_for_test(0, 1);
        g.on_event(&InputEvent::Char { c: 'a', now_ms: 0 }, 1.0);
        for c in "b cd".chars() {
            g.on_event(&InputEvent::Char { c, now_ms: 0 }, 1.0);
        }
        // 상자 = 셀 (0,1) 사각형 · x 30+100+5 · y 21+10
        let down = |x: i32| InputEvent::MouseDown {
            x,
            y: 31,
            shift: false,
            primary: false,
        };
        g.on_event(&down(140), 1.0);
        g.on_event(&InputEvent::MouseUp { x: 140, y: 31 }, 1.0);
        g.on_event(&down(140), 1.0);
        let sel = g.live_copy();
        assert!(
            sel.is_some_and(|s| !s.is_empty() && !s.contains(' ')),
            "더블 = 단어"
        );
        g.on_event(&InputEvent::MouseUp { x: 140, y: 31 }, 1.0);
        g.on_event(&down(140), 1.0);
        assert_eq!(g.live_copy().as_deref(), Some("ab cd"), "트리플 = 전체");
    }

    /// 다른 셀 클릭 = 커밋 뒤 그 셀 선택.
    #[test]
    fn click_elsewhere_commits() {
        let mut g = editable_grid();
        g.select_cell_for_test(0, 0);
        g.on_event(&InputEvent::Char { c: 'Z', now_ms: 0 }, 1.0);
        assert!(g.editing_cell());
        // (0,1) 셀 = x 130+50 · y 21+10
        g.on_event(
            &InputEvent::MouseDown {
                x: 180,
                y: 31,
                shift: false,
                primary: false,
            },
            1.0,
        );
        assert!(!g.editing_cell(), "바깥 클릭 = 커밋");
        let e = g.edit.as_ref().expect("editable");
        assert_eq!(e.cs.cell(RowRef::Existing(0), 0), Some(&Some("Z".into())));
        assert_eq!(g.sel_cur, Some((0, 1)), "그 클릭은 선택으로 이어진다");
    }
}
