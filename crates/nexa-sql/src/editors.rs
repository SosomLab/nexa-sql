//! 편집기 탭(사용자 09-14) — nexa-ctl `TabBar`(dir2 이식 · U-2) + 탭마다 `TextBox` 버퍼.
//!
//! - 줄 수 = 설정 `tabs.rows`(**multi 기본** · single = ◀ ▶ 스크롤 + 드래그 이동).
//! - 툴팁 = 설정 `tabs.tooltip`(기본 켬) — 탭 위에 1초 머물면 카드(제목 · 문장 수 · 글자 수 · 접속). 내용은 호스트가 [`Editors::set_conn_desc`]로 준다.
//! - 세션 분리(`session.mode = per-editor`)는 T-54 — 지금은 모든 탭이 한 세션.

use crate::eol::Eol;
use crate::syntax::SyntaxRegistry;
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::{draw_tooltip_in, DrawCtx};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::PreparedText;
use nexa_ctl::{
    Control, InputEvent, Invalidations, SyntaxSpec, TabAction, TabBadge, TabBar, TextBox,
    WhitespaceStyle, Widget,
};
use nsql_i18n::{t, Msg};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// 탭 우클릭 메뉴 요청(호스트가 처리 · 09-17): 이름 바꾸기 · 닫기(왼쪽/오른쪽/전부) · 파일 위치 열기.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TabMenuReq {
    Rename(usize),
    Close(usize),
    CloseLeft(usize),
    CloseRight(usize),
    CloseAll,
    Reveal(usize),
}

pub(crate) struct Editors {
    tabs: TabBar,
    /// 탭 우클릭 메뉴(결과 탭과 같은 부품 · 팝업 층에 그린다).
    menu: CtxMenu,
    menu_tab: Option<usize>,
    tab_menu_req: Option<TabMenuReq>,
    bufs: Vec<TextBox>,
    titles: Vec<String>,
    active: usize,
    counter: usize,
    /// 탭별 안정 id(닫혀도 재사용 없음) — 호스트가 결과 그리드를 탭과 짝지을 때 쓴다(사용자 09-16).
    ids: Vec<u64>,
    next_id: u64,
    bounds: Rect,
    scale: f32,
    /// 탭 줄과 본문 사이에 비워 둘 높이(물리 px) — 외부 변경 확인 띠 자리(docs/58).
    top_inset: i32,
    /// ★ 동시 편집(사용자 09-22): 나란히 보이는 탭 인덱스(2개 이상일 때만 · 비면 단일 모드). 각 칸은 완전히 독립한 편집기이고
    /// 미니맵만 끈다. 탭 바에서 Shift = 연속 · Ctrl(⌘) = 개별 · 수식키 없는 클릭 = 그 탭 단일 모드. 상한 `editor.split_max`.
    split: Vec<usize>,
    split_max: usize,
    /// 본문 전체 영역(탭 줄·띠 아래) — 동시 편집 때 칸들의 합집합.
    body: Rect,
    /// **뷰 탭**(본문이 글 편집기가 아니라 호스트가 그리는 전용 뷰 — 확장 상세 등): 탭 id → 뷰 열쇠(예 `ext:<id>`).
    /// 탭 줄·전환·닫기는 보통 탭과 같고, 본문 그리기·입력만 호스트의 뷰가 맡는다.
    view_tabs: std::collections::HashMap<u64, String>,
    /// **큰 파일 모드**(docs/59 §4 1단계): 탭 id → (단계 1·2, 사용자가 "기능 강제로 켜기"를 눌렀는가). 단계는 읽을 때·저장할 때 정한다.
    large: std::collections::HashMap<u64, (u8, bool)>,
    /// 단계 기준: [(바이트, 줄 수); L1, L2] — 둘 중 하나라도 넘으면 그 단계(설정 `file.large_*`).
    large_cfg: [(usize, usize); 2],
    /// 큰 파일 단계별 기능 제한(사용자 09-21 · 0 = 제한 안 함 · 1 = L1부터 · 2 = L2부터): (확장 효과, 구문 강조).
    /// 설정 `file.large_ext_level`(기본 L1) · `file.large_syntax_level`(기본 L2).
    large_feature_levels: (u8, u8),
    /// 읽기 전용 탭(큰 파일을 보기만 · 일부만 열기).
    read_only: std::collections::HashSet<u64>,
    /// **적재 중인 탭**(사용자 09-20): 큰 파일을 작업 스레드가 읽는 동안의 자리 탭 — 비어 있고 읽기 전용이며 경로가 없다
    /// (저장해도 원본을 덮지 않는다). 다 읽으면 [`Self::fill_loaded`]가 그 탭에 본문을 옮겨 넣는다. 다른 탭은 그대로 쓴다.
    loading: std::collections::HashSet<u64>,
    /// ★ **미리보기 탭**(Sublime 차용 · 사용자 09-22 · docs/67 §4): 프로젝트 탐색기에서 한 번 클릭한 파일이 들어오는 탭 —
    /// 한 개만 두고 다음 클릭은 그 탭의 본문을 **바꿔 넣는다**(앞 파일의 버퍼는 버린다) · 본문을 고치면 정식 탭으로 승격(표식 제거 ·
    /// 다음 클릭은 새 미리보기 탭). 제목 앞 `◦`. 닫히면 없는 것.
    preview: Option<u64>,
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
    occ_style: nexa_ctl::OccurrenceStyle,
    auto_indent: (nexa_ctl::AutoIndent, nexa_ctl::IndentRules),
    /// 첫 글자 앞 여백(설정 `editor.text_pad_left`).
    text_inset: i32,
    whitespace: WhitespaceStyle,
    /// 확장이 정한 괄호 옵션(Rainbow Pairs · 전 탭 공통 · 새 탭에도).
    bracket_opts: Option<nexa_ctl::BracketOpts>,
    /// 확장이 붙인 우클릭 메뉴 서브메뉴(전 탭 공통).
    menu_extras: Vec<nexa_ctl::controls::ctxmenu::CtxItem>,
    /// (탭 폭, 공백 들여쓰기) **기본값**(설정 `editor.tab_size`/`editor.indent_spaces`) — 새 탭의 시작값.
    indent: (u8, bool),
    /// 탭 정지점 방식(설정 `editor.tab_stops` · 09-16).
    tab_stops: bool,
    /// 휠 스크롤을 줄 경계에 맞추는가(설정 `editor.scroll` = row · 기본 pixel · 09-16).
    scroll_snap: bool,
    /// 미니맵(켬 · 폭 논리 px · T-97).
    minimap: (bool, i32),
    minimap_box: (Option<nexa_ctl::theme::Color>, Option<f32>, bool),
    /// 미니맵 동작(뷰포트 hover만 · 클릭 = 글로 · 찾기 띠).
    minimap_opts: (bool, bool, bool),
    /// 실행 중인 탭 id들(탭 제목 앞 ▶ · T-108) — 세션이 여럿이면 동시에 여러 탭이 실행 중일 수 있다(docs/52).
    running: Vec<u64>,
    /// 탭 바 [+]로 새 탭이 생겼다(호스트가 거둔다).
    new_tab_created: bool,
    /// 탭 본체 더블클릭 감지(미리보기 탭 승격 · 사용자 09-22): (탭 index · 첫 클릭 시각) · 시간 = `ui.dblclick_ms`.
    last_tab_click: Option<(usize, Instant)>,
    dblclick_ms: u128,
    /// 뒤에서 끝난 실행의 표시(docs/52 D-104): 탭 id → 성공 여부 — 제목 앞 ✓/✗ · 그 탭을 보거나 다시 실행하면 지운다.
    done_marks: HashMap<u64, bool>,
    /// 탭별 세션 표식(docs/52 §7): 탭 id → (표식, 접속 설명). 없는 탭 = 공유 세션(표식 없음 · 설명은 `conn_desc`).
    sess_info: HashMap<u64, (TabBadge, String)>,
    /// 표식 클릭/우클릭(1회성) — 호스트가 그 탭의 세션 메뉴를 만든다.
    badge_req: Option<usize>,
    /// 열린 메뉴가 표식 메뉴인가(고른 id를 호스트에 그대로 넘긴다).
    menu_is_badge: bool,
    badge_pick: Option<(u64, String)>,
    /// 줄 변경 표시(설정 `editor.diff_marks` · 향상 모드 off).
    diff_marks: bool,
    /// 되돌리기 깊이 상한(`editor.undo_max`).
    undo_max: usize,
    /// 되돌리기 바이트 예산(탭마다 · 설정 `editor.undo_budget_mb`).
    undo_budget: usize,
    /// 쉬었다 치면 새 묶음(ms · `editor.undo_group_ms`) · 거대 편집 확인 기준(바이트 · `editor.undo_giant_mb`).
    undo_group_ms: u64,
    undo_giant: usize,
    /// 다중 선택 구간 수 상한(`editor.max_occurrences` · docs/72 §2 · 0 = 없음).
    max_regions: usize,
    /// 탭별 미커밋 문장 수·오래됨(수동 커밋 · DR-30 T-77) — 제목 뒤 `●n`(오래되면 `⚠n`).
    tx_badges: HashMap<u64, (usize, bool)>,
    /// 배지 모양(설정 `tx.badge`: count · dot · off).
    tx_badge_mode: String,
    /// 미커밋 탭의 닫기 요청(탭 바 ×) — 호스트가 확인 뒤 처리.
    tx_close_req: Option<usize>,
    /// 저장하지 않은 탭을 닫으려 한다 — 호스트가 묻는다(저장하고 닫기 · 저장하지 않고 닫기 · 취소 · 사용자 09-21).
    save_close_req: Option<usize>,
    /// 다음 `close_tab` 한 번은 저장 여부를 묻지 않는다(호스트가 이미 물었다).
    close_forced: bool,
    /// 탭별 들여쓰기 재정의(`None` = 기본값 따름) — 상태줄 팝업은 **그 탭만** 바꾼다(Sublime 관례 · 사용자 09-15).
    indents: Vec<Option<(u8, bool)>>,
    /// 탭별 파일 경로(T-74 · `None` = 제목 없는 새 스크립트).
    paths: Vec<Option<PathBuf>>,
    /// 마지막으로 열거나 저장한 본문(더러움 판정 근거 · 새 탭 = 빈 문자열).
    saved: Vec<String>,
    /// 탭 id → (본문 세대, 저장본과 다른가) — `is_dirty` 캐시.
    dirty_cache: std::cell::RefCell<HashMap<u64, (u64, bool)>>,
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
            menu: CtxMenu::new(),
            menu_tab: None,
            tab_menu_req: None,
            bufs: Vec::new(),
            titles: Vec::new(),
            active: 0,
            counter: 0,
            bounds: Rect::new(0, 0, 0, 0),
            scale: 1.0,
            top_inset: 0,
            split: Vec::new(),
            split_max: 3,
            body: Rect::new(0, 0, 0, 0),
            view_tabs: std::collections::HashMap::new(),
            large: std::collections::HashMap::new(),
            large_cfg: [(5 << 20, 100_000), (20 << 20, 300_000)],
            large_feature_levels: (1, 2),
            read_only: std::collections::HashSet::new(),
            loading: std::collections::HashSet::new(),
            preview: None,
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
            occ_style: nexa_ctl::OccurrenceStyle::default(),
            auto_indent: (
                nexa_ctl::AutoIndent::default(),
                nexa_ctl::IndentRules::sql(),
            ),
            text_inset: 3,
            whitespace: WhitespaceStyle::default(),
            bracket_opts: None,
            menu_extras: Vec::new(),
            indent: (4, true),
            indents: Vec::new(),
            paths: Vec::new(),
            saved: Vec::new(),
            dirty_cache: std::cell::RefCell::new(HashMap::new()),
            eol: Vec::new(),
            saved_eol: Vec::new(),
            default_eol: Eol::os(),
            ids: Vec::new(),
            next_id: 1,
            tab_stops: true,
            scroll_snap: false,
            minimap: (false, 160),
            minimap_box: (None, None, false),
            minimap_opts: (false, false, true),
            running: Vec::new(),
            done_marks: HashMap::new(),
            new_tab_created: false,
            last_tab_click: None,
            dblclick_ms: 400,
            sess_info: HashMap::new(),
            badge_req: None,
            menu_is_badge: false,
            badge_pick: None,
            diff_marks: true,
            undo_max: 1000,
            undo_budget: 64 << 20,
            undo_group_ms: 1500,
            undo_giant: 32 << 20,
            max_regions: 0,
            tx_badges: HashMap::new(),
            tx_badge_mode: "count".into(),
            tx_close_req: None,
            save_close_req: None,
            close_forced: false,
            encs: Vec::new(),
            shown_titles: Vec::new(),
            pending_close: None,
            notice: None,
        };
        e.new_tab(None);
        e
    }

    fn make_box(&self, text: &str, syntax: &Rc<SyntaxSpec>) -> TextBox {
        // 기본 편집기 placeholder 없음(사용자 09-22).
        let mut tb = TextBox::new("").with_multiline().with_text(text);
        // 우클릭 편집 메뉴는 본문 패스가 아니라 팝업 층에서(`paint_popups` · 토스트 위 · UI 글꼴 · 사용자 09-22).
        tb.set_popup_deferred(true);
        tb.set_line_numbers(self.line_numbers);
        // Golden식 표시 띠(줄번호 오른쪽 4px · 색 막대 자리) + 첫 글자 앞 2px(사용자 09-16).
        tb.set_gutter_marks(true);
        tb.set_scale(self.scale);
        tb.set_highlighter(Some(syntax.clone()));
        tb.set_rulers(self.rulers.clone());
        tb.set_rulers_visible(self.rulers_show);
        tb.set_ruler_style(self.ruler_color, self.ruler_alpha);
        tb.set_occurrence_highlight(self.occurrence_hl);
        tb.set_occurrence_style(self.occ_style);
        tb.set_auto_indent(self.auto_indent.0, self.auto_indent.1.clone());
        tb.set_text_inset(self.text_inset);
        tb.set_whitespace(self.whitespace);
        if let Some(b) = &self.bracket_opts {
            tb.set_bracket_opts(b.clone());
        }
        tb.set_menu_extras(self.menu_extras.clone());
        tb.set_indent(self.indent.0, self.indent.1);
        tb.set_tab_stops(self.tab_stops);
        tb.set_scroll_snap(self.scroll_snap);
        tb.set_minimap(self.minimap.0);
        tb.set_minimap_width(self.minimap.1);
        tb.set_minimap_box(self.minimap_box.0, self.minimap_box.1, self.minimap_box.2);
        tb.set_minimap_behavior(self.minimap_opts.0, self.minimap_opts.1);
        tb.set_minimap_find(self.minimap_opts.2);
        tb.set_history_max(self.undo_max);
        tb.set_history_budget(self.undo_budget);
        tb.set_undo_group_pause_ms(self.undo_group_ms);
        tb.set_giant_edit_limit(self.undo_giant);
        tb.set_max_regions(self.max_regions);
        // 방금 넣은 본문 = 저장된 상태(새 탭의 빈 글 · 파일에서 읽은 글) — O(1) 더러움 판정의 기준점.
        tb.mark_saved();
        tb.set_line_comment(syntax.line_comments.first().cloned());
        // 편집기는 거의 항상 포커스라 링이 늘 보여 거슬린다(사용자 09-16) — 캐럿만으로 충분.
        tb.set_focus_ring(false);
        tb
    }

    /// 안내선 표시·색·투명도 · 동일 출현 외곽선(설정 4종 · 전 탭).
    /// 확장 효과: 괄호 옵션을 전 탭에(새 탭에도).
    pub(crate) fn set_bracket_opts(&mut self, opts: nexa_ctl::BracketOpts) {
        for tb in &mut self.bufs {
            tb.set_bracket_opts(opts.clone());
        }
        self.bracket_opts = Some(opts);
        // 큰 파일 탭은 확장 효과를 받지 않는다(단계 기준 = `file.large_ext_level`).
        self.enforce_large();
    }

    /// 확장 효과: 우클릭 메뉴 추가 항목을 전 탭에(새 탭에도).
    pub(crate) fn set_menu_extras(&mut self, items: Vec<nexa_ctl::controls::ctxmenu::CtxItem>) {
        for tb in &mut self.bufs {
            tb.set_menu_extras(items.clone());
        }
        self.menu_extras = items;
    }

    /// 파일 탭 상단 강조 줄 색(None = 테마 accent).
    pub(crate) fn set_tab_accent(&mut self, c: Option<nexa_ctl::theme::Color>) {
        self.tabs.set_accent(c);
    }

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
    /// Auto indent(설정 `editor.auto_indent`/… · 전 탭 · docs/49).
    pub(crate) fn set_auto_indent(
        &mut self,
        cfg: nexa_ctl::AutoIndent,
        rules: nexa_ctl::IndentRules,
    ) {
        for b in &mut self.bufs {
            b.set_auto_indent(cfg, rules.clone());
        }
        self.auto_indent = (cfg, rules);
    }

    /// 동일 출현 상자 스타일(설정 `editor.occurrence_*` · 전 탭).
    pub(crate) fn set_occurrence_style(&mut self, st: nexa_ctl::OccurrenceStyle) {
        self.occ_style = st;
        for b in &mut self.bufs {
            b.set_occurrence_style(st);
        }
    }

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
        let comment = spec.line_comments.first().cloned();
        self.cur_mut().set_highlighter(Some(spec));
        self.cur_mut().set_line_comment(comment);
        true
    }

    /// 파일 검색용 탭 본문(id · 제목 · 본문 · 경로) — 저장 안 된 변경 포함(T-81a).
    pub(crate) fn tab_texts(&self) -> Vec<(u64, String, String, Option<PathBuf>)> {
        (0..self.bufs.len())
            .map(|i| {
                (
                    self.tab_id(i),
                    self.titles[i].clone(),
                    self.bufs[i].text(),
                    self.paths.get(i).cloned().flatten(),
                )
            })
            .collect()
    }

    /// Goto Anything 항목(id · 제목 · 경로 · 더러움 · 활성) — 탭 순서대로(T-96).
    pub(crate) fn tab_entries(&self) -> Vec<(u64, String, Option<PathBuf>, bool, bool)> {
        (0..self.titles.len())
            .map(|i| {
                (
                    self.tab_id(i),
                    self.titles[i].clone(),
                    self.paths.get(i).cloned().flatten(),
                    self.is_dirty(i),
                    i == self.active,
                )
            })
            .collect()
    }

    /// 일반 선택(구간 1개)의 요약 — (걸친 줄 수, 문자 수). 없거나 비었으면 None(상태줄 Sublime식 · 사용자 09-16).
    pub(crate) fn selection_summary(&self) -> Option<(usize, usize)> {
        let tb = self.cur();
        let (a, b) = tb.selection()?;
        if a == b {
            return None;
        }
        // 줄 표에서 바로(선택이 아무리 커도 O(log 줄 수) — 종전 = 선택 구간을 글자마다 훑었다).
        let buf = tb.buf();
        let (a, b) = (a.min(buf.len()), b.min(buf.len()));
        Some((buf.line_of(b) - buf.line_of(a) + 1, b - a))
    }

    /// 캐럿 위치(1-기준 줄 · 열) — 버퍼의 줄 표에서 이분 탐색(종전 = 캐럿 앞까지 글자마다 훑었다 · 큰 파일 끝에서 느렸다).
    pub(crate) fn caret_line_col(&self) -> (usize, usize) {
        let tb = self.cur();
        let buf = tb.buf();
        let caret = tb.caret().min(buf.len());
        let line = buf.line_of(caret);
        (line + 1, caret - buf.line_start(line) + 1)
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

    /// 미니맵(설정 `editor.minimap`/`editor.minimap_width` · T-97) — 전 탭 + 새 탭.
    pub(crate) fn set_minimap(&mut self, on: bool, width: i32) {
        self.minimap = (on, width);
        for b in &mut self.bufs {
            b.set_minimap(on);
            b.set_minimap_width(width);
        }
        self.enforce_large();
    }

    /// 미니맵 동작(설정 `editor.minimap_viewport`/`minimap_click`/`minimap_find` · 전 탭).
    pub(crate) fn set_minimap_opts(&mut self, viewport_hover: bool, click_text: bool, find: bool) {
        self.minimap_opts = (viewport_hover, click_text, find);
        for b in &mut self.bufs {
            b.set_minimap_behavior(viewport_hover, click_text);
            b.set_minimap_find(find);
        }
    }

    /// 줄 변경 표시 켬/끔 — 켜면 저장된 탭마다 기준선을 다시 준다.
    pub(crate) fn set_diff_marks(&mut self, on: bool) {
        self.diff_marks = on;
        for i in 0..self.bufs.len() {
            let base = if on && self.paths.get(i).is_some_and(|p| p.is_some()) {
                self.saved.get(i).cloned()
            } else {
                None
            };
            self.bufs[i].set_baseline(base.as_deref());
        }
    }

    fn refresh_baseline(&mut self, i: usize) {
        let base = if self.diff_marks && self.paths.get(i).is_some_and(|p| p.is_some()) {
            self.saved.get(i).cloned()
        } else {
            None
        };
        if let Some(b) = self.bufs.get_mut(i) {
            b.set_baseline(base.as_deref());
        }
    }

    /// 실행 중 탭 표시(제목 앞 ▶ · None = 없음).
    /// 뒤에서 끝난 실행 표시(`None` = 지움).
    pub(crate) fn set_done_mark(&mut self, id: u64, ok: Option<bool>) {
        let changed = match ok {
            Some(v) => self.done_marks.insert(id, v) != Some(v),
            None => self.done_marks.remove(&id).is_some(),
        };
        if changed {
            self.sync_tabs();
        }
    }

    /// 그 탭에서 실행이 진행 중인가(탭 닫기 거부용 · 09-19).
    pub(crate) fn is_running(&self, id: u64) -> bool {
        self.running.contains(&id)
    }

    pub(crate) fn set_running(&mut self, id: u64, on: bool) {
        let had = self.running.contains(&id);
        if on {
            self.done_marks.remove(&id);
        }
        if on && !had {
            self.running.push(id);
            self.sync_tabs();
        } else if !on && had {
            self.running.retain(|r| *r != id);
            self.sync_tabs();
        }
    }

    /// 탭별 세션 표식·설명 교체(호스트가 세션 상태가 바뀔 때마다 통째로 준다 · 바뀔 때만 다시 그린다).
    pub(crate) fn set_sess_info(&mut self, info: HashMap<u64, (TabBadge, String)>) {
        if self.sess_info != info {
            self.sess_info = info;
            self.sync_badges();
        }
    }

    fn sync_badges(&mut self) {
        let badges: Vec<TabBadge> = self
            .ids
            .iter()
            // 호스트가 아직 알려 주지 않은 탭(방금 만든 탭)도 **처음부터 고정 자리**(미연결 사선) — 나중에 표식이 생기며 제목이 밀리지 않는다(사용자 09-18).
            .map(|id| {
                self.sess_info
                    .get(id)
                    .map_or(TabBadge::LinkOff, |(b, _)| *b)
            })
            .collect();
        let mut inv = Invalidations::default();
        self.tabs.set_badges(badges, &mut inv);
        // 동시 편집 칸에 든 탭 = 탭 상단 줄(사용자 09-22).
        let group: Vec<bool> = (0..self.titles.len())
            .map(|i| self.split.len() > 1 && self.split.contains(&i))
            .collect();
        self.tabs.set_group(group, &mut inv);
    }

    /// 표식 클릭/우클릭 요청(탭 index · 1회성).
    pub(crate) fn take_badge_request(&mut self) -> Option<usize> {
        self.badge_req.take()
    }

    /// 표식 메뉴 열기 — 항목은 호스트가 만든다(세션 상태를 아는 쪽) · 표식 상자 바로 아래에.
    pub(crate) fn open_badge_menu(&mut self, i: usize, items: Vec<CtxItem>) {
        let (x, y) = match self.tabs.badge_rect_of(i) {
            Some(r) => (r.x, r.bottom()),
            None => self.cursor,
        };
        self.menu_tab = Some(i);
        self.menu_is_badge = true;
        let host = Rect::new(0, 0, i32::MAX / 2, i32::MAX / 2);
        let text_w = (260.0 * self.scale) as i32;
        self.menu.open_at(x, y, items, host, text_w);
    }

    /// 표식 메뉴에서 고른 항목(탭 id, 항목 id · 1회성).
    pub(crate) fn take_badge_pick(&mut self) -> Option<(u64, String)> {
        self.badge_pick.take()
    }

    /// 오류 줄 마크(논리 줄 0 기준 · 그 탭의 미니맵) — `None` = 지움.
    pub(crate) fn set_error_line(&mut self, id: u64, line: Option<usize>) {
        for i in 0..self.bufs.len() {
            if self.tab_id(i) == id {
                self.bufs[i].set_minimap_errors(line.into_iter().collect());
            }
        }
    }

    /// 미니맵 뷰포트 상자(색 · 알파 · 테두리 · 전 탭).
    pub(crate) fn set_minimap_box(
        &mut self,
        color: Option<nexa_ctl::theme::Color>,
        alpha: Option<f32>,
        border: bool,
    ) {
        self.minimap_box = (color, alpha, border);
        for b in &mut self.bufs {
            b.set_minimap_box(color, alpha, border);
        }
    }

    /// 되돌리기 깊이 상한(설정 `editor.undo_max` · T-90d) — 전 탭 + 새 탭.
    pub(crate) fn set_undo_max(&mut self, n: usize) {
        self.undo_max = n;
        for b in &mut self.bufs {
            b.set_history_max(n);
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

    /// i번째 탭의 편집 상자(북마크 거터·줄 변경 기록 소비).
    pub(crate) fn tab_box(&self, i: usize) -> Option<&TextBox> {
        self.bufs.get(i)
    }

    pub(crate) fn tab_box_mut(&mut self, i: usize) -> Option<&mut TextBox> {
        self.bufs.get_mut(i)
    }

    pub(crate) fn cur(&self) -> &TextBox {
        &self.bufs[self.active]
    }

    pub(crate) fn cur_mut(&mut self) -> &mut TextBox {
        &mut self.bufs[self.active]
    }

    pub(crate) fn editor_bounds(&self) -> Rect {
        if self.is_split() {
            self.body
        } else {
            self.cur().bounds()
        }
    }

    /// 새 탭(제목 없으면 `Script_N`). 활성으로.
    pub(crate) fn active(&self) -> usize {
        self.active
    }

    pub(crate) fn len(&self) -> usize {
        self.bufs.len()
    }

    /// 탭 바 [+]로 새 탭이 생겼다(1회성) — 호스트가 새 탭 규칙(docs/52 §7-2)을 적용한다.
    pub(crate) fn take_new_tab_created(&mut self) -> bool {
        std::mem::take(&mut self.new_tab_created)
    }

    pub(crate) fn new_tab(&mut self, title: Option<String>) {
        let title = title.unwrap_or_else(|| {
            self.counter += 1;
            format!("Script_{}", self.counter)
        });
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

    /// 탭 `i`가 마지막 열기/저장 뒤 바뀌었나 — 본문 비교는 **세대가 바뀌었을 때만**(09-19 성능: 종전엔 이벤트 루프마다
    /// 모든 탭의 본문을 String으로 만들어 비교했다 · `dirty_cache` = (세대, 결과)).
    pub(crate) fn is_dirty(&self, i: usize) -> bool {
        let (Some(b), Some(s)) = (self.bufs.get(i), self.saved.get(i)) else {
            return false;
        };
        let eol_changed = self.eol.get(i) != self.saved_eol.get(i);
        // ★ 저장 지점과 같은 상태 = 본문 비교 없이 깨끗(O(1) · docs/60). 아니면 종전대로 세대별 1회 비교
        //   (손으로 되돌려 친 경우까지 "깨끗"으로 알아보려고).
        if b.is_saved() {
            return eol_changed;
        }
        // 큰 파일 탭 = 저장본 사본이 없다 → 저장 지점과 다르면 더러움(본문 비교 없음 · docs/59).
        if self.is_large(i) {
            return true;
        }
        let rev = b.text_rev();
        let cache = self.dirty_cache.borrow();
        if let Some(&(r, d)) = cache.get(&self.tab_id(i)) {
            if r == rev {
                return d || eol_changed;
            }
        }
        drop(cache);
        // 세대가 바뀌었다 — 버퍼의 바이트와 바로 비교(String 생성 없음).
        let differs = !b.buf().eq_str(s);
        self.dirty_cache
            .borrow_mut()
            .insert(self.tab_id(i), (rev, differs));
        differs || eol_changed
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

    /// 탭의 인코딩 지정(뒤에서 적재가 끝난 탭 — 활성이 아닐 수 있다).
    pub(crate) fn set_encoding(&mut self, i: usize, enc: &str) {
        if let Some(e) = self.encs.get_mut(i) {
            *e = enc.to_string();
        }
    }

    /// 탭의 큰 파일 단계(0 = 보통).
    pub(crate) fn large_level(&self, i: usize) -> u8 {
        self.large.get(&self.tab_id(i)).map_or(0, |x| x.0)
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

    /// 파일 본문을 새 탭으로 연다(같은 경로가 열려 있으면 그 탭으로) — 작은 파일의 동기 경로. 큰 파일은 호스트가
    /// [`Self::begin_load_tab`] → (스레드) → [`Self::fill_loaded`]로 나눠 부른다. 두 길은 같은 코드다.
    #[cfg(test)]
    pub(crate) fn open_file(&mut self, path: &Path, text: String, eol: Eol) {
        if let Some(i) = self.path_tab(path) {
            self.switch(i);
            return;
        }
        let name = file_title(path);
        let id = self.begin_load_tab(&name);
        self.fill_loaded(id, Some(path), &name, &name, PreparedText::new(text), eol);
    }

    // ───────────────────────── 미리보기 탭(docs/67 §4 · 사용자 09-22) ──────────────────

    /// 살아 있는 미리보기 탭 id(닫혔거나 새 스크립트로 바뀌었으면 None).
    pub(crate) fn preview_id(&self) -> Option<u64> {
        self.preview.filter(|id| self.index_of_id(*id).is_some())
    }

    /// 탭 `i`가 미리보기 탭인가(테스트 · 탭 메뉴 "미리보기 유지"가 붙으면 호스트도).
    #[cfg(test)]
    pub(crate) fn is_preview(&self, i: usize) -> bool {
        self.preview_id() == Some(self.tab_id(i))
    }

    /// 파일을 **미리보기 탭**에 연다 — 이미 연 탭이 있으면 그 탭으로 · 미리보기 탭이 살아 있고 바뀌지 않았으면 그 본문을
    /// 바꿔 넣는다(앞 파일의 `TextBox`는 여기서 버려진다 = 메모리 회수) · 없거나 바뀌었으면(승격) 새 미리보기 탭.
    /// 돌려주는 값 = 탭 인덱스(활성으로 만든다).
    pub(crate) fn open_preview(&mut self, path: &Path, prep: PreparedText, eol: Eol) -> usize {
        if let Some(i) = self.path_tab(path) {
            self.switch(i);
            return i;
        }
        self.poll_preview();
        let name = file_title(path);
        let id = match self.preview_id() {
            Some(id) => id,
            None => {
                let id = self.begin_load_tab(&name);
                self.preview = Some(id);
                id
            }
        };
        let i = self
            .fill_loaded(id, Some(path), &name, &name, prep, eol)
            .unwrap_or(self.active);
        self.switch(i);
        i
    }

    /// 미리보기 탭의 본문이 바뀌었으면 **정식 탭으로 승격**(표식 제거 · 다음 미리보기는 새 탭) — 바뀌었으면 true.
    /// 호스트의 틱에서 부른다(세대 비교라 값싸다).
    pub(crate) fn poll_preview(&mut self) -> bool {
        let Some(id) = self.preview else {
            return false;
        };
        let Some(i) = self.index_of_id(id) else {
            self.preview = None;
            return true;
        };
        if self.is_dirty(i) {
            self.preview = None;
            self.sync_tabs();
            return true;
        }
        false
    }

    /// 탭 본체 클릭 기록 — 같은 탭을 `dblclick_ms` 안에 다시 클릭했으면 true(그러면 기록을 비운다 = 세 번째는 새 시작).
    fn note_tab_click(&mut self, i: usize, now: Instant) -> bool {
        let dbl = matches!(self.last_tab_click, Some((j, t0)) if j == i && now.duration_since(t0).as_millis() < self.dblclick_ms);
        self.last_tab_click = if dbl { None } else { Some((i, now)) };
        dbl
    }

    pub(crate) fn set_dblclick_ms(&mut self, ms: u128) {
        self.dblclick_ms = ms.max(1);
    }

    /// 탭 `i`가 미리보기 탭이면 정식 탭으로 승격(표식 제거) — 승격했으면 true.
    pub(crate) fn promote_tab(&mut self, i: usize) -> bool {
        if self.preview_id() == Some(self.tab_id(i)) {
            self.preview = None;
            self.sync_tabs();
            return true;
        }
        false
    }

    /// 이 경로를 연 탭이 있으면 정식 탭으로(미리보기였으면 승격) 전환 — 있었으면 true(더블클릭 · Enter).
    pub(crate) fn promote_path(&mut self, path: &Path) -> bool {
        let Some(i) = self.path_tab(path) else {
            return false;
        };
        if self.preview == Some(self.tab_id(i)) {
            self.preview = None;
        }
        self.switch(i);
        true
    }

    /// 이 경로를 연 탭.
    pub(crate) fn path_tab(&self, path: &Path) -> Option<usize> {
        self.paths.iter().position(|p| p.as_deref() == Some(path))
    }

    /// **적재 자리 탭**을 만들고 활성으로 한다 — 비어 있음 · 읽기 전용 · 경로 없음. 돌려주는 값 = 탭 id.
    pub(crate) fn begin_load_tab(&mut self, name: &str) -> u64 {
        self.new_tab(Some(name.to_string()));
        let i = self.active;
        let id = self.ids[i];
        self.bufs[i].set_read_only(true);
        self.loading.insert(id);
        self.sync_tabs();
        id
    }

    /// 적재 중인 탭인가.
    #[cfg(test)]
    pub(crate) fn is_loading_id(&self, id: u64) -> bool {
        self.loading.contains(&id)
    }

    /// 활성 탭이 적재 중인가(호스트: 진행 막을 그릴지 · Esc = 취소).
    pub(crate) fn active_loading(&self) -> bool {
        self.loading.contains(&self.active_id())
    }

    /// 적재가 끝난 본문을 자리 탭에 **옮겨 넣는다**(활성 탭을 바꾸지 않는다 — 사용자가 다른 탭에서 일하고 있을 수 있다).
    /// `path` = None이면 경로 없는 읽을거리("앞부분만") · `like` = 구문을 고를 파일 이름. 탭이 이미 닫혔으면 `None`.
    pub(crate) fn fill_loaded(
        &mut self,
        id: u64,
        path: Option<&Path>,
        title: &str,
        like: &str,
        prep: PreparedText,
        eol: Eol,
    ) -> Option<usize> {
        self.loading.remove(&id);
        let i = self.index_of_id(id)?;
        let syntax = self.registry.for_title(like);
        let (focused, bounds) = (self.bufs[i].is_focused(), self.bufs[i].bounds());
        // 단계를 **먼저** 정한다(글자 수·줄 수는 준비본이 이미 안다 — 다시 세지 않는다). 큰 파일이면 저장본 사본(파일 크기)과
        //   줄 변경 기준선을 아예 만들지 않는다(적재 피크 메모리).
        let level = self.level_for(prep.len_bytes(), prep.lines());
        let saved = if level == 0 {
            prep.text().into_owned()
        } else {
            String::new()
        };
        let mut tb = self.make_box("", &syntax);
        tb.set_prepared(prep);
        tb.mark_saved();
        tb.set_focused(focused);
        if let Some((ts, sp)) = self.indents.get(i).copied().flatten() {
            tb.set_indent(ts, sp);
        }
        let mut inv = Invalidations::default();
        tb.set_bounds(bounds, &mut inv);
        self.bufs[i] = tb;
        self.read_only.remove(&id);
        self.syntax[i] = syntax;
        self.titles[i] = title.to_string();
        self.paths[i] = path.map(Path::to_path_buf);
        self.saved[i] = saved;
        self.dirty_cache.borrow_mut().remove(&id);
        self.eol[i] = eol;
        self.saved_eol[i] = eol;
        if level == 0 {
            self.large.remove(&id);
            self.refresh_baseline(i);
        } else {
            self.large.insert(id, (level, false));
        }
        self.enforce_large();
        self.sync_tabs();
        Some(i)
    }

    /// **안내 탭**(파일 없는 읽을거리 · 확장 상세 등) — 같은 제목의 경로 없는 탭이 있으면 그 탭의 내용을 바꾸고, 없으면
    /// 새 탭. 내용은 "저장된 상태"로 둔다(닫을 때 저장을 묻지 않게 · 사용자 09-19 확장 패널 "선택하면 상세").
    /// 돌려주는 값 = 새 탭을 만들었는가(호스트가 그 탭을 **No connection**으로 둔다 — 읽을거리에 DB 연결은 필요 없다).
    /// `like` = 구문을 고를 파일 이름(디스크 내용 보기 = 원래 파일의 구문) · `None` = Plain Text.
    pub(crate) fn open_info_tab_like(
        &mut self,
        title: &str,
        text: &str,
        like: Option<&str>,
    ) -> bool {
        let found =
            (0..self.titles.len()).find(|&i| self.titles[i] == title && self.paths[i].is_none());
        match found {
            Some(i) => self.switch(i),
            None => self.new_tab(Some(title.to_string())),
        }
        let i = self.active;
        // 읽을거리는 SQL이 아니다 — **Plain Text** 구문으로(키워드 색·괄호 규칙 없음 · 사용자 09-19 "and/in/to가 칠해진다").
        let syntax = match like {
            Some(name) => self.registry.for_title(name),
            None => self
                .registry
                .get("Plain Text")
                .unwrap_or_else(|| Rc::new(SyntaxSpec::plain())),
        };
        let focused = self.cur().is_focused();
        let mut tb = self.make_box(text, &syntax);
        tb.set_focused(focused);
        let mut inv = Invalidations::default();
        tb.set_bounds(self.editor_bounds(), &mut inv);
        self.bufs[i] = tb;
        self.syntax[i] = syntax;
        self.saved[i] = text.to_string();
        self.dirty_cache.borrow_mut().remove(&self.tab_id(i));
        self.refresh_baseline(i);
        self.sync_tabs();
        found.is_none()
    }

    /// 방금 읽은 탭에 되돌리기 기록을 들인다(docs/60 D-129) — 들인 단계 수(본문이 기록과 다르면 0).
    pub(crate) fn import_history(&mut self, i: usize, bytes: &[u8]) -> usize {
        let Some(b) = self.bufs.get_mut(i) else {
            return 0;
        };
        if !b.import_history(bytes) {
            return 0;
        }
        self.dirty_cache.borrow_mut().remove(&self.ids[i]);
        b.history_stats().0
    }

    /// 되돌리기 묶음의 정지 기준(ms)과 거대 편집 확인 기준(바이트) — 전 탭 + 새 탭(docs/60 D-130·131).
    /// 다중 선택 구간 수 상한 → 전 탭(docs/72 §2).
    pub(crate) fn set_max_regions(&mut self, n: usize) {
        self.max_regions = n;
        for b in &mut self.bufs {
            b.set_max_regions(n);
        }
    }

    pub(crate) fn set_undo_rules(&mut self, group_ms: u64, giant_bytes: usize) {
        self.undo_group_ms = group_ms;
        self.undo_giant = giant_bytes;
        for b in &mut self.bufs {
            b.set_undo_group_pause_ms(group_ms);
            b.set_giant_edit_limit(giant_bytes);
        }
    }

    /// 되돌리기 바이트 예산(설정 `editor.undo_budget_mb`) — 전 탭 + 새 탭.
    pub(crate) fn set_undo_budget(&mut self, bytes: usize) {
        self.undo_budget = bytes;
        for b in &mut self.bufs {
            b.set_history_budget(bytes);
        }
    }

    /// 메모리 회수: **보이지 않는 탭**의 그리기 캐시(본문 문자열·줄 표·행 폭·구문 상태·미니맵 픽셀)를 놓는다 — 다시 보면
    /// 다시 만들어진다. 돌려주는 값 = 놓은 탭 수.
    pub(crate) fn release_inactive_caches(&mut self) -> usize {
        let active = self.active;
        let mut n = 0;
        for (i, b) in self.bufs.iter().enumerate() {
            if i != active {
                b.release_caches();
                n += 1;
            }
        }
        n
    }

    // ───────────── 큰 파일 모드 · 읽기 전용(docs/59 §4) ─────────────

    /// 자체 캡처용(기동 명령 `tab.drag_demo`): 활성 탭을 잡아 오른쪽으로 조금 끈 **상태로 둔다** — 드래그 고스트·자리 표시를
    /// OS 입력 주입 없이 찍으려고(컨트롤에 직접 사건을 준다 · 격리 실행에서만 쓴다).
    pub(crate) fn capture_drag_demo(&mut self) {
        let i = self.active;
        let Some(r) = self.tabs.tab_rect(i) else {
            return;
        };
        let mut inv = Invalidations::default();
        self.tabs.begin_drag(i, r.x + 12, r.y + r.h / 2);
        self.tabs.on_event(
            &InputEvent::MouseMove {
                // 마지막 탭은 왼쪽으로(오른쪽은 띠 끝에 막혀 고스트가 제자리와 겹친다).
                x: if i + 1 == self.bufs.len() {
                    r.x + 12 - r.w / 3
                } else {
                    r.x + 12 + r.w / 3
                },
                y: r.y + r.h / 2,
            },
            &mut inv,
        );
    }

    /// 편집기 탭의 rect(탭 줄 · 보이지 않으면 None) — 이름 바꾸기 상자가 그 탭에 붙는다.
    pub(crate) fn tab_rect(&self, i: usize) -> Option<nexa_ctl::geom::Rect> {
        self.tabs.tab_rect(i)
    }

    /// 단계 기준(설정 `file.large_l1_mb`/`_lines` · `file.large_l2_mb`/`_lines` · 0 = 그 기준 끔).
    pub(crate) fn set_large_cfg(&mut self, cfg: [(usize, usize); 2]) {
        self.large_cfg = cfg;
    }

    /// 단계별 기능 제한 기준(설정 `file.large_ext_level` · `file.large_syntax_level`) — 바뀌면 전 탭에 다시 맞춘다.
    pub(crate) fn set_large_feature_levels(&mut self, ext: u8, syntax: u8) {
        if self.large_feature_levels == (ext, syntax) {
            return;
        }
        self.large_feature_levels = (ext, syntax);
        // 기준이 느슨해졌을 수 있다 → 큰 탭의 기능을 전역 설정대로 되돌린 뒤 새 기준으로 다시 줄인다.
        for i in 0..self.bufs.len() {
            if self.large.contains_key(&self.ids[i]) {
                self.restore_large_features(i);
            }
        }
        self.enforce_large();
    }

    /// 큰 파일 모드가 줄였던 **확장 효과 · 구문 강조**를 전역 설정대로 되돌린다(강제로 켜기 · 기준 변경 · 단계가 내려갔을 때).
    fn restore_large_features(&mut self, i: usize) {
        let syntax = self.syntax[i].clone();
        let opts = self.bracket_opts.clone();
        let b = &mut self.bufs[i];
        b.set_highlighter(Some(syntax));
        if let Some(o) = opts {
            b.set_bracket_opts(o);
        }
    }

    /// 본문 크기로 단계를 정한다(0 = 보통 · 1 = L1 · 2 = L2).
    fn level_for(&self, bytes: usize, lines: usize) -> u8 {
        let over = |(b, l): (usize, usize)| (b > 0 && bytes >= b) || (l > 0 && lines >= l);
        if over(self.large_cfg[1]) {
            2
        } else if over(self.large_cfg[0]) {
            1
        } else {
            0
        }
    }

    /// 탭의 단계를 지금 본문으로 다시 정하고 기능을 맞춘다(읽은 직후 · 저장 직후). 돌려주는 값 = 새 단계.
    pub(crate) fn reclassify(&mut self, i: usize) -> u8 {
        let Some(b) = self.bufs.get(i) else { return 0 };
        // 버퍼가 아는 값 그대로(UTF-8 바이트 · 줄 수) — 다시 세지 않는다.
        let level = self.level_for(b.buf().len_bytes(), b.buf().line_count());
        let id = self.tab_id(i);
        let before = self.large.get(&id).map_or(0, |x| x.0);
        let forced = self.large.get(&id).is_some_and(|x| x.1);
        if level == 0 {
            self.large.remove(&id);
        } else {
            self.large.insert(id, (level, forced));
        }
        // 저장 뒤 파일이 작아져 단계가 내려갔으면 줄였던 확장 효과·구문 강조를 되돌린다(종전에는 닫았다 열어야 돌아왔다).
        if level < before {
            self.restore_large_features(i);
        }
        self.enforce_large();
        level
    }

    /// 활성 탭의 (단계, 강제로 켬).
    pub(crate) fn active_large(&self) -> (u8, bool) {
        self.large
            .get(&self.active_id())
            .copied()
            .unwrap_or((0, false))
    }

    /// 탭이 큰 파일 모드로 **동작 중**인가(단계 ≥ 1 이고 강제로 켜지 않음) — 호스트가 자동 병합·저장본 비교를 건너뛸 때 본다.
    pub(crate) fn is_large(&self, i: usize) -> bool {
        self.large
            .get(&self.tab_id(i))
            .is_some_and(|&(l, forced)| l > 0 && !forced)
    }

    /// "기능 강제로 켜기" 토글(활성 탭) — 돌려주는 값 = 켠 뒤의 상태(단계 0이면 None).
    pub(crate) fn toggle_large_force(&mut self) -> Option<bool> {
        let id = self.active_id();
        let e = self.large.get_mut(&id)?;
        e.1 = !e.1;
        let on = e.1;
        // 강제로 켜면 전역 설정대로 되돌린다(상자를 다시 꾸미는 대신 설정값을 다시 넣는다).
        let i = self.active;
        let (mm, occ) = (self.minimap.0, self.occurrence_hl);
        let syntax = self.syntax[i].clone();
        if on {
            let b = &mut self.bufs[i];
            b.set_minimap(mm);
            b.set_occurrence_highlight(occ);
            b.set_highlighter(Some(syntax));
            if let Some(o) = self.bracket_opts.clone() {
                self.bufs[i].set_bracket_opts(o);
            }
            self.refresh_baseline(i);
        }
        self.enforce_large();
        Some(on)
    }

    /// 큰 파일 탭의 기능 축소를 적용한다 — 전역 설정이 바뀌어 전 탭에 다시 들어간 뒤에도 부른다.
    /// L1: 미니맵 · 선택어 강조 · 줄 변경 기준선 끔(+ 저장본 사본을 버린다 — 더러움은 저장 지점으로 O(1)).
    /// L2: + 구문 강조 끔(Plain). **확장 효과**(괄호 색·짝 표·짝 없음 표시 — Rainbow Pairs 등)와 **구문 강조**를 끄는 단계는 설정이
    /// 정한다(`file.large_ext_level` 기본 L1 · `file.large_syntax_level` 기본 L2 · 사용자 09-21).
    fn enforce_large(&mut self) {
        let (ext_at, syntax_at) = self.large_feature_levels;
        for i in 0..self.bufs.len() {
            let Some(&(level, forced)) = self.large.get(&self.ids[i]) else {
                continue;
            };
            if level == 0 || forced {
                continue;
            }
            let b = &mut self.bufs[i];
            b.set_minimap(false);
            b.set_occurrence_highlight(false);
            b.set_baseline(None);
            if feature_limited(level, syntax_at) {
                b.set_highlighter(None);
            }
            if feature_limited(level, ext_at) {
                let base = self.bracket_opts.clone().unwrap_or_default();
                self.bufs[i].set_bracket_opts(large_bracket_opts(&base));
            }
            // 저장본 사본(파일 크기만큼)을 놓는다 — `is_dirty`는 큰 탭에서 저장 지점만 본다.
            if !self.saved[i].is_empty() {
                self.saved[i] = String::new();
                self.dirty_cache.borrow_mut().remove(&self.ids[i]);
            }
        }
    }

    /// 읽기 전용 지정/해제.
    pub(crate) fn set_read_only(&mut self, i: usize, on: bool) {
        let Some(b) = self.bufs.get_mut(i) else {
            return;
        };
        b.set_read_only(on);
        let id = self.ids[i];
        if on {
            self.read_only.insert(id);
        } else {
            self.read_only.remove(&id);
        }
        self.sync_tabs();
    }

    pub(crate) fn active_read_only(&self) -> bool {
        self.read_only.contains(&self.active_id())
    }

    /// **뷰 탭 열기** — 같은 열쇠의 탭이 있으면 그 탭으로, 없으면 새 탭(본문은 빈 글 · 저장된 상태라 닫을 때 묻지 않는다).
    /// 돌려주는 값 = 새로 만들었는가.
    pub(crate) fn open_view_tab(&mut self, key: &str, title: &str) -> bool {
        let found = self
            .view_tabs
            .iter()
            .find(|(_, k)| k.as_str() == key)
            .and_then(|(id, _)| self.index_of_id(*id));
        match found {
            Some(i) => {
                self.switch(i);
                if self.titles[i] != title {
                    self.titles[i] = title.to_string();
                    self.sync_tabs();
                }
                false
            }
            None => {
                self.new_tab(Some(title.to_string()));
                let id = self.active_id();
                self.view_tabs.insert(id, key.to_string());
                true
            }
        }
    }

    /// 활성 탭이 뷰 탭이면 그 열쇠.
    pub(crate) fn active_view(&self) -> Option<&str> {
        self.view_tabs.get(&self.active_id()).map(String::as_str)
    }

    /// 닫힌 탭의 뷰 등록을 정리한다(호스트 틱).
    pub(crate) fn reap_views(&mut self) {
        let alive = &self.ids;
        self.view_tabs.retain(|id, _| alive.contains(id));
        self.large.retain(|id, _| alive.contains(id));
        self.read_only.retain(|id| alive.contains(id));
        self.loading.retain(|id| alive.contains(id));
    }

    // ───────────── 외부 파일 변경(docs/58 · T-140) ─────────────

    /// 확인 띠 자리(물리 px) — 바뀌었으면 다시 배치하고 true.
    pub(crate) fn set_top_inset(&mut self, px: i32) -> bool {
        if self.top_inset == px {
            return false;
        }
        self.top_inset = px;
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
        true
    }

    /// 확인 띠가 놓일 자리(탭 줄 바로 아래 · 본문 위).
    pub(crate) fn banner_rect(&self) -> Rect {
        let ed = self.editor_bounds();
        Rect::new(ed.x, ed.y - self.top_inset, ed.w, self.top_inset)
    }

    pub(crate) fn index_of_id(&self, id: u64) -> Option<usize> {
        self.ids.iter().position(|x| *x == id)
    }

    /// 파일에 묶인 탭들 — (탭 id, 경로).
    pub(crate) fn files(&self) -> Vec<(u64, PathBuf)> {
        (0..self.ids.len())
            .filter_map(|i| Some((self.ids[i], self.paths[i].clone()?)))
            .collect()
    }

    /// 탭의 (버퍼 본문, 기준 = 마지막으로 읽거나 저장한 본문, 인코딩, 줄끝).
    pub(crate) fn snapshot(&self, i: usize) -> Option<(String, &str, &str, Eol)> {
        Some((
            self.bufs.get(i)?.text(),
            self.saved.get(i)?.as_str(),
            self.encs.get(i)?.as_str(),
            *self.eol.get(i)?,
        ))
    }

    /// 외부 내용 반영: `buf` = 버퍼에 넣을 본문(`None` = 버퍼는 그대로 · 기준만) · `base` = 새 기준(디스크 본문).
    /// 버퍼 교체는 **되돌리기 한 단계**(달라진 가운데만 · 캐럿 유지). 줄끝·인코딩이 주어지면 저장 기준까지 맞춘다.
    pub(crate) fn apply_external(
        &mut self,
        i: usize,
        buf: Option<&str>,
        base: &str,
        eol: Option<Eol>,
        enc: Option<&str>,
    ) {
        if i >= self.bufs.len() {
            return;
        }
        if let Some(text) = buf {
            let mut inv = Invalidations::default();
            self.bufs[i].replace_all_undoable(text, &mut inv);
        }
        self.saved[i] = base.to_string();
        // 버퍼가 새 기준과 같아졌으면(다시 읽기 · 같은 수정) 그 상태가 저장 지점이다 — 병합은 더러운 채로 남는다.
        if self.bufs[i].buf().eq_str(base) {
            self.bufs[i].mark_saved();
        }
        if let Some(eol) = eol {
            self.eol[i] = eol;
            self.saved_eol[i] = eol;
        }
        if let Some(enc) = enc {
            self.encs[i] = enc.to_string();
        }
        self.dirty_cache.borrow_mut().remove(&self.tab_id(i));
        self.refresh_baseline(i);
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
        self.cur_mut().mark_saved();
        self.dirty_cache.borrow_mut().remove(&self.tab_id(i));
        self.saved_eol[i] = self.eol[i];
        // 단계를 먼저 정한다 — 큰 파일이면 저장본 사본(파일 크기)을 만들었다 버리지 않고 아예 안 만든다.
        if self.reclassify(i) == 0 || !self.is_large(i) {
            self.saved[i] = self.cur().text();
            self.refresh_baseline(i);
        }
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
        let base = if self.is_dirty(i) {
            format!("*{}", self.titles[i])
        } else {
            self.titles[i].clone()
        };
        // 미리보기 탭 = 제목 앞 ◦(Sublime의 기울임 대신 · 사용자 09-22).
        let base = if self.preview == Some(self.tab_id(i)) {
            format!("◦ {base}")
        } else {
            base
        };
        // 읽기 전용 탭 표식(큰 파일 보기 · 일부만 열기).
        let base = if self.loading.contains(&self.tab_id(i)) {
            format!("{base} …")
        } else if self.read_only.contains(&self.tab_id(i)) {
            format!("{base} [RO]")
        } else {
            base
        };
        // 실행 중 탭 = 제목 앞 ▶(T-108 · 사용자 09-17).
        let base = if self.running.contains(&self.tab_id(i)) {
            format!("▶ {base}")
        } else {
            match self.done_marks.get(&self.tab_id(i)) {
                Some(true) => format!("✓ {base}"),
                Some(false) => format!("✗ {base}"),
                None => base,
            }
        };
        // 미커밋 배지(수동 커밋 · n>0일 때만 · DR-30): count `●3` · dot `●` · 오래되면 `⚠`.
        match self.tx_badges.get(&self.tab_id(i)) {
            Some(&(n, stale)) if n > 0 && self.tx_badge_mode != "off" => {
                let mark = if stale { '⚠' } else { '●' };
                if self.tx_badge_mode == "dot" {
                    format!("{base} {mark}")
                } else {
                    format!("{base} {mark}{n}")
                }
            }
            _ => base,
        }
    }

    /// 미커밋 배지 갱신(호스트 · 탭 id → (문장 수, 오래됨)).
    pub(crate) fn set_tx_badges(&mut self, badges: HashMap<u64, (usize, bool)>, mode: &str) {
        self.tx_badges = badges;
        self.tx_badge_mode = mode.to_string();
    }

    /// 미커밋 탭 닫기 요청(탭 바 × · 1회성) — 호스트가 Commit/Rollback을 물은 뒤 닫는다.
    pub(crate) fn take_tx_close_request(&mut self) -> Option<usize> {
        self.tx_close_req.take()
    }

    /// 탭 닫기(확인 없이 · 미커밋 확인을 이미 거친 뒤).
    pub(crate) fn close_tab_confirmed(&mut self, i: usize) {
        self.tx_badges.remove(&self.tab_id(i));
        self.close_tab(i);
    }

    /// 저장하지 않은 탭을 닫으려 했다(탭 번호) — 호스트가 꺼내 묻는다.
    pub(crate) fn take_save_close_request(&mut self) -> Option<usize> {
        self.save_close_req.take()
    }

    /// 저장 여부를 **이미 물은 뒤** 닫는다(저장했거나 · 버리기로 했다).
    pub(crate) fn close_tab_forced(&mut self, i: usize) {
        self.close_forced = true;
        self.close_tab(i);
        self.close_forced = false;
    }

    /// 종전의 2단 닫기(설정 `editor.close_unsaved = twice`): 같은 탭을 3초 안에 다시 닫으면 버린다.
    pub(crate) fn close_tab_two_step(&mut self, i: usize) {
        let again =
            matches!(self.pending_close, Some((j, t)) if j == i && t.elapsed() <= CLOSE_CONFIRM);
        if again {
            self.close_tab_forced(i);
        } else {
            self.pending_close = Some((i, Instant::now()));
            self.notice = Some(Msg::StUnsavedCloseAgain);
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
        // 미커밋 문장이 있는 탭은 호스트에 넘긴다(잃는 순간만 묻는다 · DR-30).
        if self
            .tx_badges
            .get(&self.tab_id(i))
            .is_some_and(|(n, _)| *n > 0)
        {
            self.tx_close_req = Some(i);
            return;
        }
        // ★ 저장하지 않은 변경 = **호스트가 묻는다**(사용자 09-21 — 종전의 "3초 안에 한 번 더 닫기"는 상태줄 한 줄이라 놓치기 쉽고
        //   저장할 길을 주지 않았다). 호스트는 설정 `editor.close_unsaved`에 따라 메뉴로 묻거나(`ask`) 종전 2단 닫기(`twice`)를 한다.
        if self.is_dirty(i) && !self.close_forced {
            self.save_close_req = Some(i);
            return;
        }
        self.pending_close = None;
        if self.bufs.len() <= 1 {
            // 마지막 탭은 비우기만(제목 없는 새 스크립트로).
            if let Some(b) = self.bufs.get_mut(i) {
                b.set_text("");
            }
            self.paths[i] = None;
            self.saved[i] = String::new();
            self.dirty_cache.borrow_mut().remove(&self.tab_id(i));
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
        if self.preview == Some(self.tab_id(i)) {
            self.preview = None;
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
            // 동시 편집 밖의 탭으로 가면 단일 모드로(호스트의 전환·팔레트도 같은 길).
            if !self.split.is_empty() && !self.split.contains(&i) {
                self.end_split();
            }
            let focused = self.cur().is_focused();
            self.cur_mut().set_focused(false);
            self.active = i;
            self.cur_mut().set_focused(focused);
            // 이 탭을 봤다 → 뒤에서 끝난 실행 표시는 지운다.
            let id = self.tab_id(i);
            self.done_marks.remove(&id);
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
        self.sync_badges();
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
        // 탭·표식 메뉴도 배율을 받아야 한다 — 빠져 있어 맥 2x에서 행 높이가 1x로 계산돼 항목이 겹쳐 보였다(mac 09-21).
        self.menu.set_scale(scale);
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
        let top = th + self.top_inset;
        let ed = Rect::new(b.x, b.y + top, b.w, (b.h - top).max(0));
        self.body = ed;
        // 닫힘·이동으로 어긋난 동시 편집 집합은 버린다(2개 미만 = 단일).
        let len = self.bufs.len();
        self.split.retain(|&i| i < len);
        if self.split.len() < 2 {
            self.split.clear();
        }
        let split = &self.split;
        let n = split.len();
        let gap = (self.scale.round() as i32).max(1);
        for (i, tb) in self.bufs.iter_mut().enumerate() {
            let r = match split.iter().position(|&s| s == i) {
                Some(k) if n >= 2 => split_column(ed, k, n, gap),
                _ => ed,
            };
            tb.set_bounds(r, inv);
        }
    }

    /// 동시 편집 중인가(칸 2개 이상).
    pub(crate) fn is_split(&self) -> bool {
        self.split.len() >= 2
    }

    pub(crate) fn set_split_max(&mut self, n: i64) {
        self.split_max = n.clamp(1, 4) as usize;
        if self.split.len() > self.split_max {
            self.split.truncate(self.split_max);
            if !self.split.contains(&self.active) {
                self.end_split();
            } else {
                let mut inv = Invalidations::default();
                self.layout(&mut inv);
            }
        }
    }

    /// 동시 편집을 끝낸다(단일 모드 · 미니맵 복원 · 재배치). 이미 단일이면 아무 일도 없다.
    fn end_split(&mut self) {
        if self.split.is_empty() {
            return;
        }
        self.split.clear();
        let on = self.minimap.0;
        for tb in &mut self.bufs {
            tb.set_minimap(on);
        }
        self.sync_tabs();
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    /// 탭 본체 클릭(수식키 포함) — 뷰 탭은 늘 단일 · 수식키 없음 = 단일 전환 · Shift/Ctrl = [`split_plan`].
    fn tab_click(&mut self, i: usize, shift: bool, primary: bool) {
        let is_view = |this: &Self, k: usize| {
            k < this.bufs.len() && this.view_tabs.contains_key(&this.tab_id(k))
        };
        if (!shift && !primary) || is_view(self, i) || is_view(self, self.active) {
            self.end_split();
            self.switch(i);
            return;
        }
        let (set, act) = split_plan(
            self.active,
            &self.split,
            i,
            shift,
            primary,
            self.split_max,
            self.bufs.len(),
        );
        self.apply_split(set, act);
    }

    fn apply_split(&mut self, set: Vec<usize>, act: usize) {
        self.split = set;
        let on = self.minimap.0;
        let split = &self.split;
        for (k, tb) in self.bufs.iter_mut().enumerate() {
            tb.set_minimap(if split.contains(&k) { false } else { on });
        }
        self.switch(act);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    /// 본문 그리기 — 단일이면 활성 탭, 동시 편집이면 칸마다 그 탭 + 칸 사이 구분선.
    pub(crate) fn paint_bodies(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.is_split() {
            self.cur_mut().paint(dc, th);
            return;
        }
        let body = self.body;
        let gap = (self.scale.round() as i32).max(1);
        let idxs = self.split.clone();
        for (k, &i) in idxs.iter().enumerate() {
            if let Some(tb) = self.bufs.get_mut(i) {
                tb.paint(dc, th);
                if k + 1 < idxs.len() {
                    let r = tb.bounds();
                    dc.fill_rect(Rect::new(r.right(), body.y, gap, body.h), th.border);
                }
            }
        }
    }

    /// 커서 아래 칸의 편집기(동시 편집) · 아니면 활성 탭 — 휠·hover는 포커스가 아니라 커서 아래로 간다.
    pub(crate) fn box_at_or_cur_mut(&mut self, p: Point) -> &mut TextBox {
        if self.is_split() {
            if let Some(&i) = self
                .split
                .iter()
                .find(|&&i| self.bufs.get(i).is_some_and(|tb| tb.bounds().contains(p)))
            {
                return &mut self.bufs[i];
            }
        }
        self.cur_mut()
    }

    /// 동시 편집에서 다른 칸을 누르면 그 탭이 활성(키 입력·실행 대상)이 된다 — 집합은 그대로. 바뀌었으면 true.
    pub(crate) fn activate_pane_at(&mut self, p: Point) -> bool {
        if !self.is_split() {
            return false;
        }
        let hit = self
            .split
            .iter()
            .copied()
            .find(|&i| self.bufs.get(i).is_some_and(|tb| tb.bounds().contains(p)));
        match hit {
            Some(i) if i != self.active => {
                self.switch(i);
                true
            }
            _ => false,
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
    /// 탭 메뉴·표식 메뉴가 열려 있는가.
    /// 본문의 우클릭 편집 메뉴가 열려 있는가(어느 칸이든).
    pub(crate) fn edit_menu_open(&self) -> bool {
        self.bufs.iter().any(|tb| tb.popup_open())
    }

    /// 본문 편집 메뉴만 닫기.
    pub(crate) fn close_edit_menus(&mut self) {
        for tb in &mut self.bufs {
            tb.close_menu();
        }
    }

    pub(crate) fn tab_menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// 탭 메뉴·표식 메뉴를 닫는다(다른 영역에서 새 메뉴가 열릴 때 — **한 창에 열린 메뉴는 하나**).
    pub(crate) fn close_tab_menu(&mut self) {
        if self.menu.is_open() {
            self.menu.close();
            self.menu_tab = None;
            self.menu_is_badge = false;
        }
    }

    pub(crate) fn route_tabs(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> bool {
        // 열린 탭 메뉴 = 모달(바깥 좌/우클릭은 닫고 그 클릭을 그대로 진행 · 팝업 UX 규칙).
        if self.menu.is_open() {
            // ★ `on_event`는 바깥 클릭도 "소비"로 보고한다 → 먼저 바깥인지 재 둔다(사용자 09-22: 다시 눌러야 했다).
            let outside = self.menu.is_outside_click(ev);
            let consumed = self.menu.on_event(ev) && !outside;
            if let Some(id) = self.menu.take_picked() {
                let i = self.menu_tab.take().unwrap_or(self.active);
                if std::mem::take(&mut self.menu_is_badge) {
                    if i < self.ids.len() {
                        self.badge_pick = Some((self.ids[i], id));
                    }
                    inv.push(self.bounds);
                    return true;
                }
                self.tab_menu_req = match id.as_str() {
                    "rename" => Some(TabMenuReq::Rename(i)),
                    "close" => Some(TabMenuReq::Close(i)),
                    "close_left" => Some(TabMenuReq::CloseLeft(i)),
                    "close_right" => Some(TabMenuReq::CloseRight(i)),
                    "close_all" => Some(TabMenuReq::CloseAll),
                    "reveal" => Some(TabMenuReq::Reveal(i)),
                    _ => None,
                };
                inv.push(self.bounds);
                return true;
            }
            if consumed
                || !matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                )
            {
                inv.push(self.bounds);
                return true;
            }
        }
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
                TabAction::Switch(i) => {
                    let (shift, primary) = self.tabs.last_click_mods();
                    // 같은 탭을 곧바로 다시 클릭 = 더블클릭 → 미리보기 탭이면 승격(Sublime · 사용자 09-22).
                    let dbl = !shift && !primary && self.note_tab_click(i, Instant::now());
                    self.tab_click(i, shift, primary);
                    if dbl {
                        self.promote_tab(i);
                    }
                }
                TabAction::Close(i) => {
                    self.end_split();
                    self.close_tab(i);
                }
                TabAction::New => {
                    self.end_split();
                    self.new_tab(None);
                    self.new_tab_created = true;
                }
                TabAction::Move { from, to } => {
                    self.end_split();
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
                TabAction::Context(i) => {
                    let (x, y) = self.cursor;
                    self.open_tab_menu(i, Point { x, y });
                }
                // 세션 표식 — 좌클릭·우클릭 모두 그 탭의 세션 메뉴(호스트가 만든다 · docs/52 §7).
                TabAction::Badge(i) | TabAction::BadgeContext(i) => self.badge_req = Some(i),
            }
        }
        inv.push(self.bounds);
        true
    }

    /// 탭 우클릭 메뉴(사용자 09-17): 이름 바꾸기 · 닫기 · 왼쪽/오른쪽 닫기 · 모두 닫기 · 파일 위치 열기(파일이 디스크에 있을 때만).
    fn open_tab_menu(&mut self, i: usize, p: Point) {
        self.menu_tab = Some(i);
        self.menu_is_badge = false;
        let n = self.bufs.len();
        let has_file = self
            .paths
            .get(i)
            .and_then(|p| p.as_ref())
            .is_some_and(|p| p.exists());
        let items = vec![
            CtxItem::item("rename", t(Msg::MnTabRename)),
            CtxItem::Separator,
            CtxItem::item("close", t(Msg::MnCloseTab)),
            CtxItem::maybe("close_left", t(Msg::MnTabCloseLeft), i > 0),
            CtxItem::maybe("close_right", t(Msg::MnTabCloseRight), i + 1 < n),
            CtxItem::item("close_all", t(Msg::MnTabCloseAll)),
            CtxItem::Separator,
            CtxItem::maybe("reveal", t(Msg::MnTabReveal), has_file),
        ];
        let host = Rect::new(0, 0, i32::MAX / 2, i32::MAX / 2);
        let text_w = (200.0 * self.scale) as i32;
        self.menu.open_at(p.x, p.y, items, host, text_w);
    }

    /// 우클릭 메뉴(팝업 층 · 호스트가 맨 마지막에).
    /// 열린 메뉴 전부 닫기(탭 메뉴 · 본문 편집 메뉴) — 풀다운과 배타(사용자 09-22).
    pub(crate) fn close_menus(&mut self) {
        self.menu.close();
        for tb in &mut self.bufs {
            tb.close_menu();
        }
    }

    pub(crate) fn paint_popups(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        // 편집기 본문의 우클릭 편집 메뉴(동시 편집이면 칸마다) — 팝업 층 = 토스트·카드 위.
        if self.is_split() {
            for &i in &self.split {
                if let Some(tb) = self.bufs.get(i) {
                    tb.paint_popup(dc, th);
                }
            }
        } else {
            self.cur().paint_popup(dc, th);
        }
        self.menu.paint(dc, th);
    }

    /// 메뉴 선택(1회성).
    pub(crate) fn take_tab_menu_request(&mut self) -> Option<TabMenuReq> {
        self.tab_menu_req.take()
    }

    /// 탭 이름 바꾸기(빈 이름은 무시 · 저장/열기 때 파일 이름으로 다시 덮인다).
    pub(crate) fn rename_tab(&mut self, i: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() || i >= self.titles.len() {
            return;
        }
        self.titles[i] = name.to_string();
        self.sync_tabs();
    }

    pub(crate) fn tab_count(&self) -> usize {
        self.bufs.len()
    }

    pub(crate) fn title_of(&self, i: usize) -> String {
        self.titles.get(i).cloned().unwrap_or_default()
    }

    pub(crate) fn path_of(&self, i: usize) -> Option<PathBuf> {
        self.paths.get(i).and_then(|p| p.clone())
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
        // 접속: 전용 세션 탭은 자기 세션 설명(끊겼으면 빈 글) · 그 외는 공유 세션.
        let conn = match self.ids.get(i).and_then(|id| self.sess_info.get(id)) {
            Some((_, d)) => d.as_str(),
            None => self.conn_desc.as_str(),
        };
        if !conn.is_empty() {
            card.push('\n');
            card.push_str(t(Msg::TipConnection));
            card.push_str(": ");
            card.push_str(conn);
        }
        // ★ 가로 클램프는 창 왼쪽이 아니라 **편집기 영역의 x부터**(사용자 09-18 캡처): 첫 탭의 카드가 탭 가운데에 맞춰지며
        //   왼쪽으로 나가 탐색기 밑에 깔렸다(탐색기가 나중에 그려진다) — 결과 도구줄 툴팁(09-16)과 같은 처방.
        draw_tooltip_in(dc, th, r, (self.bounds.x, clamp_w), &card, self.scale);
    }
}

/// 경로 → 탭 제목(파일 이름 · 없으면 경로 전체).
pub(crate) fn file_title(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// 이 단계에서 그 기능을 줄이는가 — `at` = 줄이기 시작하는 단계(0 = 줄이지 않음).
fn feature_limited(level: u8, at: u8) -> bool {
    at > 0 && level >= at
}

/// 큰 파일 탭의 괄호 옵션 — **확장 효과**(깊이 색 · 현재 쌍 강조 · 짝 없음 표시 · 본문 전체 쌍 표 스캔)는 끄고, 편집 코어인
/// 자동 닫기(`editor.auto_close_pairs`)는 그대로 둔다. `max_chars = 0` = 쌍 표를 만들지 않는다(편집마다 본문 전체를 훑는 비용 0).
fn large_bracket_opts(base: &nexa_ctl::BracketOpts) -> nexa_ctl::BracketOpts {
    nexa_ctl::BracketOpts {
        rainbow: false,
        unmatched: false,
        match_mode: 0,
        max_chars: 0,
        ..base.clone()
    }
}

#[cfg(test)]
mod load_tab_tests {
    use super::*;

    /// 큰 파일 단계별 제한(사용자 09-21): 기준 단계 이상에서만 · 0 = 제한 없음 · 확장 효과만 끄고 자동 닫기는 남긴다 ·
    /// 구문 강조는 기본 L2부터 · 기준을 L1로 당기면 L1 탭도 꺼지고, 풀면(0) 돌아온다 · "강제로 켜기"는 되돌린다.
    #[test]
    fn preview_tab_is_reused_then_promoted_when_edited() {
        let mut ed = editors();
        let dir = std::env::temp_dir().join(format!("nsql-preview-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (a, b, c) = (dir.join("a.sql"), dir.join("b.sql"), dir.join("c.sql"));
        let base = ed.len();
        // 1번 클릭 = 미리보기 탭 생성.
        let i = ed.open_preview(&a, PreparedText::new("A".into()), Eol::Lf);
        assert_eq!(ed.len(), base + 1);
        assert!(ed.is_preview(i));
        let id_a = ed.tab_id(i);
        // 2번 클릭 = 같은 탭에 교체(탭 수 그대로 · id 그대로 · 경로 바뀜).
        let j = ed.open_preview(&b, PreparedText::new("B".into()), Eol::Lf);
        assert_eq!(ed.len(), base + 1);
        assert_eq!(ed.tab_id(j), id_a);
        assert_eq!(ed.path_of(j), Some(b.clone()));
        assert!(ed.shown_title(j).starts_with("◦ "));
        // 편집 → 승격(표식 사라짐).
        ed.cur_mut().set_text("B changed");
        assert!(ed.poll_preview());
        assert!(!ed.is_preview(j));
        assert!(ed.preview_id().is_none());
        // 3번 클릭 = 새 미리보기 탭(승격된 탭은 그대로).
        let k = ed.open_preview(&c, PreparedText::new("C".into()), Eol::Lf);
        assert_eq!(ed.len(), base + 2);
        assert!(ed.is_preview(k));
        assert_ne!(ed.tab_id(k), id_a);
        // 이미 연 파일을 다시 클릭 = 그 탭으로(새로 만들지 않는다).
        let j2 = ed.open_preview(&b, PreparedText::new("B".into()), Eol::Lf);
        assert_eq!(j2, j);
        assert_eq!(ed.len(), base + 2);
        // 더블클릭 = 승격.
        assert!(ed.promote_path(&c));
        assert!(!ed.is_preview(k));
        assert!(!ed.promote_path(&dir.join("none.sql")));
        // 탭 본체 더블클릭 = 승격(첫 클릭은 아님 · 두 번째가 시간 안이면).
        let p = ed.open_preview(&a, PreparedText::new("A".into()), Eol::Lf);
        let t0 = std::time::Instant::now();
        assert!(!ed.note_tab_click(p, t0));
        assert!(ed.is_preview(p));
        assert!(ed.note_tab_click(p, t0 + std::time::Duration::from_millis(100)));
        assert!(ed.promote_tab(p) && !ed.is_preview(p));
        assert!(!ed.promote_tab(p), "이미 정식");
        ed.close_tab_forced(p);
        // 닫으면 미리보기 없음.
        let m = ed.open_preview(&a, PreparedText::new("A".into()), Eol::Lf);
        ed.close_tab_forced(m);
        assert!(ed.preview_id().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn large_levels_limit_extensions_and_syntax() {
        assert!(!feature_limited(0, 1) && feature_limited(1, 1) && feature_limited(2, 1));
        assert!(!feature_limited(1, 2) && feature_limited(2, 2));
        assert!(!feature_limited(2, 0), "0 = 제한하지 않는다");
        let base = nexa_ctl::BracketOpts::default();
        let q = large_bracket_opts(&base);
        assert!(!q.rainbow && !q.unmatched && q.match_mode == 0 && q.max_chars == 0);
        assert_eq!(
            q.auto_close, base.auto_close,
            "자동 닫기 = 편집 코어 · 그대로"
        );

        let mut ed = editors();
        ed.set_large_cfg([(0, 1000), (0, 3000)]);
        let l1 = "select 1;".to_string() + &String::from(char::from(10));
        ed.open_file(
            Path::new("/tmp/nsql-test/lv1.sql"),
            l1.repeat(1500),
            Eol::Lf,
        );
        let i = ed.active();
        assert_eq!(ed.active_large().0, 1);
        assert!(
            ed.bufs[i].highlighter().is_some(),
            "L1 = 구문 강조 유지(기본 기준 L2)"
        );
        ed.set_large_feature_levels(1, 1);
        assert!(
            ed.bufs[i].highlighter().is_none(),
            "기준을 L1로 = L1 탭도 끈다"
        );
        ed.set_large_feature_levels(1, 0);
        assert!(
            ed.bufs[i].highlighter().is_some(),
            "0 = 제한 없음 → 되돌린다"
        );
        ed.set_large_feature_levels(1, 2);
        ed.open_file(
            Path::new("/tmp/nsql-test/lv2.sql"),
            l1.repeat(3500),
            Eol::Lf,
        );
        let j = ed.active();
        assert_eq!(ed.active_large().0, 2);
        assert!(ed.bufs[j].highlighter().is_none(), "L2 = 구문 강조 끔");
        assert_eq!(ed.toggle_large_force(), Some(true));
        assert!(ed.bufs[j].highlighter().is_some(), "강제로 켜기 = 되돌린다");
    }

    fn editors() -> Editors {
        Editors::new(true, true, false, Rc::new(SyntaxRegistry::load()))
    }

    /// 적재 자리 탭: 만들면 활성 · 비어 있고 편집되지 않으며 경로가 없다 · 그동안 **다른 탭은 그대로 편집된다** ·
    /// 채우면 활성 탭을 바꾸지 않고 본문·경로·줄끝이 들어가며 편집이 풀리고 더러움이 아니다.
    #[test]
    fn placeholder_tab_isolates_loading() {
        let mut ed = editors();
        let first = ed.active_id();
        let id = ed.begin_load_tab("big.sql");
        assert!(ed.active_loading() && ed.is_loading_id(id));
        assert!(
            ed.active_path().is_none(),
            "저장해도 원본을 덮지 않게 경로가 없다"
        );
        assert!(ed.cur().is_read_only());
        assert_eq!(ed.cur().text(), "");
        assert!(!ed.is_dirty(ed.active()));
        // 다른 탭으로 가서 편집한다 — 적재 중에도 된다.
        let i0 = ed.index_of_id(first).expect("first tab");
        ed.switch(i0);
        assert!(!ed.active_loading());
        ed.cur_mut().set_text("select 1;");
        // 뒤에서 적재가 끝난다: 활성 탭은 그대로.
        let p = Path::new("/tmp/nsql-test/big.sql");
        let at = ed.fill_loaded(
            id,
            Some(p),
            "big.sql",
            "big.sql",
            PreparedText::new("select 2;\nselect 3;\n".into()),
            Eol::Crlf,
        );
        let at = at.expect("tab alive");
        assert_eq!(
            ed.active_id(),
            first,
            "채워도 사용자가 있던 탭을 빼앗지 않는다"
        );
        assert_eq!(ed.cur().text(), "select 1;");
        assert!(!ed.is_loading_id(id));
        ed.switch(at);
        assert_eq!(ed.cur().text(), "select 2;\nselect 3;\n");
        assert_eq!(ed.active_path().as_deref(), Some(p));
        assert_eq!(ed.active_eol(), Eol::Crlf);
        assert!(!ed.cur().is_read_only() && !ed.is_dirty(at));
        assert_eq!(ed.path_tab(p), Some(at));
    }

    /// 적재 중에 자리 탭을 닫으면 채우기는 조용히 버려진다(호스트가 그 적재를 취소한다).
    #[test]
    fn fill_after_close_is_dropped() {
        let mut ed = editors();
        let id = ed.begin_load_tab("gone.sql");
        let i = ed.index_of_id(id).expect("tab");
        ed.close_tab(i);
        ed.reap_views();
        assert!(ed.index_of_id(id).is_none());
        let r = ed.fill_loaded(
            id,
            None,
            "gone.sql",
            "gone.sql",
            PreparedText::new("x".into()),
            Eol::Lf,
        );
        assert!(r.is_none() && !ed.is_loading_id(id));
    }

    /// 큰 본문은 채울 때 단계가 정해지고 저장본 사본을 만들지 않는다 · 제목 있는 탭은 Script_N 번호를 쓰지 않는다.
    #[test]
    fn large_fill_skips_saved_copy_and_counter_is_stable() {
        let mut ed = editors();
        ed.set_large_cfg([(0, 1000), (0, 0)]);
        let body = "select 1;\n".repeat(1500);
        ed.open_file(Path::new("/tmp/nsql-test/l1.sql"), body.clone(), Eol::Lf);
        let i = ed.active();
        assert_eq!(ed.active_large(), (1, false));
        assert!(ed.saved[i].is_empty(), "큰 파일 = 저장본 사본 없음");
        assert!(!ed.is_dirty(i));
        assert_eq!(ed.cur().text(), body);
        // 같은 경로를 다시 열면 새 탭이 아니라 그 탭.
        let n = ed.len();
        ed.open_file(Path::new("/tmp/nsql-test/l1.sql"), "other".into(), Eol::Lf);
        assert_eq!((ed.len(), ed.active()), (n, i));
        ed.new_tab(None);
        assert_eq!(
            ed.titles[ed.active()],
            "Script_2",
            "파일 탭은 번호를 먹지 않는다"
        );
    }
}

/// 동시 편집 칸 `k`(0..n)의 영역 — 폭을 n등분(마지막 칸이 나머지) · 칸 사이 `gap` px 구분선.
fn split_column(body: Rect, k: usize, n: usize, gap: i32) -> Rect {
    let n = n.max(1) as i32;
    let k = k as i32;
    let w = (body.w - gap * (n - 1)) / n;
    let x = body.x + k * (w + gap);
    let width = if k == n - 1 { body.right() - x } else { w };
    Rect::new(x, body.y, width.max(0), body.h)
}

/// ★ 탭 바 클릭 → 동시 편집 집합(순수 · MC/DC 테스트) — 사용자 09-22: Shift(연속) 또는 Ctrl(개별) + 좌클릭으로 최대 `max`개.
/// 돌려주는 값 = (집합 · 새 활성 탭). 집합이 2개 미만이면 빈 집합(단일 모드).
/// - 수식키 없음 = `([], i)` · Ctrl = 토글(상한이면 무시 · 활성을 빼면 남은 첫 칸이 활성) · Shift = 활성~i 연속(활성 쪽부터 `max`개).
pub(crate) fn split_plan(
    active: usize,
    split: &[usize],
    i: usize,
    shift: bool,
    primary: bool,
    max: usize,
    len: usize,
) -> (Vec<usize>, usize) {
    let max = max.max(1);
    if i >= len {
        return (split.to_vec(), active);
    }
    if !shift && !primary {
        return (Vec::new(), i);
    }
    let base: Vec<usize> = if split.is_empty() {
        vec![active]
    } else {
        split.to_vec()
    };
    let (mut set, act) = if primary {
        let mut set = base;
        if let Some(pos) = set.iter().position(|&s| s == i) {
            set.remove(pos);
            let act = if i == active {
                set.first().copied().unwrap_or(i)
            } else {
                active
            };
            (set, act)
        } else if set.len() >= max {
            return (if set.len() >= 2 { set } else { Vec::new() }, active);
        } else {
            set.push(i);
            set.sort_unstable();
            (set, i)
        }
    } else {
        // Shift = 활성에서 i 쪽으로 연속 · 활성 쪽부터 max개.
        let (lo, hi) = if i >= active {
            (active, i.min(active + max - 1))
        } else {
            (i.max((active + 1).saturating_sub(max)), active)
        };
        let set: Vec<usize> = (lo..=hi).collect();
        let act = if set.contains(&i) {
            i
        } else if i > active {
            hi
        } else {
            lo
        };
        (set, act)
    };
    if set.len() < 2 {
        set.clear();
    }
    (set, act)
}

#[cfg(test)]
mod split_tests {
    use super::*;

    /// 칸에 든 탭 = 탭 줄의 묶임 표식(상단 줄 · 사용자 09-22) · 단일 복귀 = 전부 해제.
    #[test]
    fn grouped_tabs_follow_split_set() {
        let mut ed = Editors::new(true, true, false, Rc::new(SyntaxRegistry::load()));
        let base = ed.len() - 1;
        ed.new_tab(None);
        ed.new_tab(None);
        ed.tab_click(base + 1, true, false);
        assert!(ed.tabs.is_grouped(base + 1) && ed.tabs.is_grouped(base + 2));
        assert!(!ed.tabs.is_grouped(base));
        ed.tab_click(base, false, false);
        assert!((0..ed.len()).all(|i| !ed.tabs.is_grouped(i)));
    }

    /// MC/DC — 수식키 없음 · Ctrl 추가/제거/상한/활성 제거 · Shift 앞·뒤·상한 · 범위 밖.
    #[test]
    fn split_plan_rules() {
        // 수식키 없음 = 단일 전환.
        assert_eq!(split_plan(0, &[0, 1], 2, false, false, 3, 5), (vec![], 2));
        // Ctrl: 단일에서 하나 더 = 둘 · 새 탭이 활성.
        assert_eq!(split_plan(0, &[], 2, false, true, 3, 5), (vec![0, 2], 2));
        // Ctrl: 이미 있는 것 = 제거 → 하나 남으면 단일.
        assert_eq!(split_plan(2, &[0, 2], 2, false, true, 3, 5), (vec![], 0));
        assert_eq!(
            split_plan(0, &[0, 2, 4], 2, false, true, 3, 5),
            (vec![0, 4], 0)
        );
        // Ctrl: 상한이면 무시(집합·활성 그대로).
        assert_eq!(
            split_plan(0, &[0, 1, 2], 3, false, true, 3, 5),
            (vec![0, 1, 2], 0)
        );
        // Shift: 앞으로 연속 · 상한만큼.
        assert_eq!(split_plan(0, &[], 2, true, false, 3, 5), (vec![0, 1, 2], 2));
        assert_eq!(split_plan(0, &[], 4, true, false, 3, 5), (vec![0, 1, 2], 2));
        // Shift: 뒤로 연속.
        assert_eq!(split_plan(4, &[], 1, true, false, 3, 5), (vec![2, 3, 4], 2));
        // Shift: 같은 탭 = 단일.
        assert_eq!(split_plan(1, &[], 1, true, false, 3, 5), (vec![], 1));
        // 범위 밖 = 불변.
        assert_eq!(
            split_plan(0, &[0, 1], 9, true, false, 3, 5),
            (vec![0, 1], 0)
        );
        // 칸 폭: 3칸 · 1px 구분선 · 마지막 칸이 나머지.
        let b = Rect::new(10, 0, 302, 100);
        assert_eq!(split_column(b, 0, 3, 1), Rect::new(10, 0, 100, 100));
        assert_eq!(split_column(b, 1, 3, 1), Rect::new(111, 0, 100, 100));
        assert_eq!(split_column(b, 2, 3, 1), Rect::new(212, 0, 100, 100));
    }
}
