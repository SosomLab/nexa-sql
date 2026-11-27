//! 테이블 구조 편집기 **뷰 탭**(docs/119 §4 · DataGrip식 Modify Table을 탭으로 · DBeaver의 섹션 구성 참고) — 편집기 자리에 그린다.
//!
//! 위 = 테이블 이름·코멘트 · 가운데 = 섹션(컬럼 · 키/제약 · 인덱스) 그리드와 행 버튼 · 아래 = **DDL 미리보기**(읽기 전용 · 실시간).
//! 모든 편집은 편집 모델([`nsql_ddl::Table`])만 바꾼다 — 서버에는 [적용…]을 거쳐야 닿는다(호스트 `app/tbledit.rs`가 미리보기·문지기·
//! 백업을 맡는다). 이 부품은 `App`을 모른다: 사건을 받고 [`TbAction`]을 내놓는다. 문장 생성 = `nsql-ddl`(순수).
//! 방언이 못 하는 변경(재생성 필요)은 문장이 되지 않고 경고 줄로 보인다(D-318 재생성 금지).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key, TextBox, Widget};
use nsql_ddl::{Column, Dialect, Index, KeyKind, Plan, Reason, Table};
use nsql_i18n::{t, tf, Msg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Columns,
    Keys,
    Indexes,
    /// 이 탭에서 적용한 스크립트들(읽기 전용 · 119 F-15).
    History,
}

impl Section {
    pub(crate) fn from_id(s: &str) -> Option<Self> {
        match s {
            "columns" => Some(Self::Columns),
            "keys" => Some(Self::Keys),
            "indexes" => Some(Self::Indexes),
            "history" => Some(Self::History),
            _ => None,
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::Columns => "columns",
            Self::Keys => "keys",
            Self::Indexes => "indexes",
            Self::History => "history",
        }
    }
    fn label(self) -> Msg {
        match self {
            Self::Columns => Msg::TbeSecColumns,
            Self::Keys => Msg::TbeSecKeys,
            Self::Indexes => Msg::TbeSecIndexes,
            Self::History => Msg::TbeSecHistory,
        }
    }
}

/// 그리드 열 하나(제목 · 폭 비중 · 체크 칸인가).
struct ColSpec {
    title: Msg,
    weight: i32,
    check: bool,
}

const COLS_COLUMNS: &[ColSpec] = &[
    ColSpec {
        title: Msg::TbeColName,
        weight: 22,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColType,
        weight: 18,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColNotNull,
        weight: 9,
        check: true,
    },
    ColSpec {
        title: Msg::TbeColDefault,
        weight: 16,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColComment,
        weight: 35,
        check: false,
    },
];
const COLS_KEYS: &[ColSpec] = &[
    ColSpec {
        title: Msg::TbeColName,
        weight: 22,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColKind,
        weight: 14,
        check: false,
    },
    ColSpec {
        title: Msg::TbeSecColumns,
        weight: 24,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColDefinition,
        weight: 40,
        check: false,
    },
];
const COLS_HISTORY: &[ColSpec] = &[
    ColSpec {
        title: Msg::TbeColWhen,
        weight: 22,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColResult,
        weight: 18,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColFile,
        weight: 60,
        check: false,
    },
];
const COLS_INDEXES: &[ColSpec] = &[
    ColSpec {
        title: Msg::TbeColName,
        weight: 30,
        check: false,
    },
    ColSpec {
        title: Msg::TbeColUnique,
        weight: 10,
        check: true,
    },
    ColSpec {
        title: Msg::TbeSecColumns,
        weight: 60,
        check: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    TableName,
    TableComment,
    Cell { row: usize, col: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Btn {
    Sec(Section),
    Add,
    Remove,
    Up,
    Down,
    Refresh,
    Revert,
    Apply,
    NoticeOk,
    NoticeLicense,
}

/// 호스트가 처리할 일.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TbAction {
    /// [적용…] — 미리보기·확인을 거쳐 실행한다.
    Apply,
    /// [새로 고침] — 서버에서 다시 읽는다(미적용 변경은 버려진다 · 호스트가 묻는다).
    Refresh,
    /// 편집 모델이 바뀌었다(탭의 미저장 표식 갱신).
    Changed,
    /// 기록 섹션의 줄을 열었다(더블클릭 · Enter) — 그 스크립트 파일을 편집기 탭으로.
    OpenHistory(usize),
    /// 라이선스 안내의 [확인] — 이번 실행 동안 다시 묻지 않는다.
    NoticeAck,
    /// 라이선스 안내의 [라이선스…] — 라이선스 창을 연다(안내는 그대로).
    OpenLicense,
}

/// `A, B` → 이름 목록(빈 조각 제외 · 방언 접힘).
fn split_list(d: Dialect, s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| nsql_ddl::fold_new_name(d, p))
        .filter(|p| !p.is_empty())
        .collect()
}

/// 외래 키 정의 글 `대상(컬럼, …) [ON DELETE 규칙] [ON UPDATE 규칙]` → (대상, 컬럼들, 삭제 규칙, 갱신 규칙). 순수.
pub(crate) fn parse_fk(d: Dialect, s: &str) -> (String, Vec<String>, String, String) {
    let s = s.trim();
    // ASCII만 대문자로(바이트 자리가 원문과 같다) → 두 규칙 낱말의 자리를 찾는다(순서 무관).
    let upper = s.to_ascii_uppercase();
    let (del, upd) = (upper.find(" ON DELETE "), upper.find(" ON UPDATE "));
    let head_end = [del, upd].into_iter().flatten().min().unwrap_or(s.len());
    let rule_at = |at: Option<usize>, other: Option<usize>| -> String {
        let Some(i) = at else {
            return String::new();
        };
        let from = i + " ON DELETE ".len();
        let to = other.filter(|o| *o > i).unwrap_or(s.len());
        s[from..to].trim().to_ascii_uppercase()
    };
    let (on_delete, on_update) = (rule_at(del, upd), rule_at(upd, del));
    let head = &s[..head_end];
    let fold_name = |t: &str| {
        t.trim()
            .split('.')
            .map(|p| nsql_ddl::fold_new_name(d, p))
            .collect::<Vec<_>>()
            .join(".")
    };
    match head.split_once('(') {
        Some((tbl, rest)) => {
            let inside = rest.split(')').next().unwrap_or("");
            (fold_name(tbl), split_list(d, inside), on_delete, on_update)
        }
        None => (fold_name(head), Vec::new(), on_delete, on_update),
    }
}

fn kind_text(k: KeyKind) -> &'static str {
    match k {
        KeyKind::Primary => "PRIMARY KEY",
        KeyKind::Unique => "UNIQUE",
        KeyKind::Foreign => "FOREIGN KEY",
        KeyKind::Check => "CHECK",
    }
}

fn kind_suffix(k: KeyKind) -> &'static str {
    match k {
        KeyKind::Primary => "PK",
        KeyKind::Unique => "UK",
        KeyKind::Foreign => "FK",
        KeyKind::Check => "CK",
    }
}

/// ★ 라이선스 안내의 [확인]이 켜지기까지의 시간(초) — **코드 상수**(D-328: 설정 · 환경 변수 · 기동 명령 어디서도 바꿀 수 없다 ·
/// 자동 시험도 이만큼 기다린다).
pub(crate) const NOTICE_SECS: u64 = 3;

/// 되돌리기 단계 상한(모델 하나 = 수십 KB 이하 · 탭을 닫으면 함께 사라진다).
const UNDO_MAX: usize = 200;

/// 완성 목록에 한 번에 보이는 줄 수.
const SUGG_MAX: usize = 10;

/// 타입 완성 후보: `typed`(소문자)로 시작하는 방언 타입들 — `show_all`이면 거르지 않는다 · 친 글이 후보와 똑같으면 빈 목록
/// (더 고를 것이 없다). 순수.
pub(crate) fn type_suggestions(d: Dialect, typed: &str, show_all: bool) -> Vec<&'static str> {
    let all = nsql_ddl::type_names(d);
    if show_all {
        return all.to_vec();
    }
    if typed.is_empty() || all.iter().any(|n| n.to_lowercase() == typed) {
        return Vec::new();
    }
    all.iter()
        .copied()
        .filter(|n| n.to_lowercase().starts_with(typed))
        .collect()
}

/// 세로 가운데에 글 한 줄(`y`·`h` = 칸의 위·높이).
fn text_mid(
    dc: &mut dyn DrawCtx,
    x: i32,
    y: i32,
    h: i32,
    clip: Rect,
    text: &str,
    fg: nexa_ctl::Color,
) {
    let ty = dc.text_center_y(y, h);
    dc.text(x, ty, clip, text, fg);
}

pub(crate) struct TableEditView {
    dialect: Dialect,
    orig: Table,
    edit: Table,
    /// 새 테이블(원본 없음 · `CREATE TABLE`).
    is_new: bool,
    /// 서버에서 구조를 읽었는가(읽기 전에는 안내만 그린다).
    loaded: bool,
    /// 읽기 전용 사유(운영 금지 설정 · 지원하지 않는 방언) — 있으면 편집·적용 불가.
    read_only: Option<String>,
    section: Section,
    sel_row: usize,
    sel_col: usize,
    top: usize,
    editing: Option<Target>,
    cell: TextBox,
    ddl: TextBox,
    plan: Plan,
    bounds: Rect,
    scale: f32,
    focused: bool,
    hover: Option<Btn>,
    pressed: Option<Btn>,
    buttons: Vec<(Btn, Rect, bool)>,
    mouse: Point,
    // 마지막 그리기의 자리(사건 판정).
    grid_rect: Rect,
    row_h: i32,
    col_x: Vec<(i32, i32)>,
    name_rect: Rect,
    comment_rect: Rect,
    ddl_rect: Rect,
    /// 타입 완성 목록(타입 칸 편집 중 · 친 글자로 거른 방언 타입들) — 비면 닫힘.
    /// ★ 라이선스 안내(D-328 · 안내 게이트): 떠 있는 동안 편집기 대신 안내만 보인다 — 값 = 뜬 시각([확인]은 그로부터
    /// [`NOTICE_SECS`]초 뒤에 켜진다).
    notice: Option<std::time::Instant>,
    /// 서버에서 읽은 외래 키·체크 제약의 속성(삭제 규칙 · 식)을 **다 읽었는가** — 아니면 그 제약은 이름·삭제만(모르는 속성을
    /// 잃지 않게 · D-563).
    keys_complete: bool,
    /// 표현하지 못하는 속성이 있어 **잠긴** 제약들(서버에서의 이름) — 속성을 다 읽은 방언에서도 이름·삭제만.
    locked_keys: Vec<String>,
    /// 이 탭에서 적용한 스크립트(시각 · 결과 글 · 파일 경로) — 새 것이 위.
    history: Vec<[String; 3]>,
    /// ★ 되돌리기(Ctrl+Z · 다시 실행): 편집 모델의 지난 상태들 — 변경 하나(칸 확정 · 행 추가/삭제 · 체크 · 순서 · Revert)가 한 단계.
    /// 서버에서 다시 읽으면(열기 · 새로 고침 · 적용 뒤) 비운다(지난 상태의 `origin`이 더는 맞지 않는다).
    undo: Vec<Table>,
    redo: Vec<Table>,
    /// 마지막으로 기록한 편집 모델(이것과 달라졌을 때만 한 단계를 쌓는다).
    last: Table,
    /// 판정용(Debug 빌드만): 다음 그리기에서 일부러 패닉 — 오류 경계(D-329)가 탭만 걷는지 본다.
    #[cfg(debug_assertions)]
    test_panic: bool,
    sugg: Vec<&'static str>,
    sugg_sel: Option<usize>,
    /// 목록 줄의 자리(그릴 때 잡는다 · 클릭 판정).
    sugg_rects: Vec<Rect>,
    actions: Vec<TbAction>,
}

impl std::fmt::Debug for TableEditView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableEditView")
            .field("table", &self.edit.name)
            .field("section", &self.section)
            .field("stmts", &self.plan.stmts.len())
            .finish()
    }
}

impl TableEditView {
    /// `schema`·`name` = 대상(새 테이블이면 `name`이 비어 있을 수 있다). 구조는 [`Self::load`]로 채운다.
    pub(crate) fn new(dialect: Dialect, schema: &str, name: &str, is_new: bool) -> Self {
        let mut cell = TextBox::new("");
        cell.set_focus_ring(false);
        let mut ddl = TextBox::new("").with_multiline();
        ddl.set_read_only(true);
        ddl.set_line_numbers(false);
        ddl.set_gutter_marks(false);
        ddl.set_minimap(false);
        ddl.set_focus_ring(false);
        ddl.set_popup_deferred(true);
        let orig = Table {
            schema: schema.to_string(),
            name: name.to_string(),
            ..Table::default()
        };
        let mut v = Self {
            dialect,
            edit: orig.editable(),
            orig,
            is_new,
            loaded: is_new,
            read_only: None,
            section: Section::Columns,
            sel_row: 0,
            sel_col: 0,
            top: 0,
            editing: None,
            cell,
            ddl,
            plan: Plan::default(),
            bounds: Rect::default(),
            scale: 1.0,
            focused: false,
            hover: None,
            pressed: None,
            buttons: Vec::new(),
            mouse: Point { x: 0, y: 0 },
            grid_rect: Rect::default(),
            row_h: 1,
            col_x: Vec::new(),
            name_rect: Rect::default(),
            comment_rect: Rect::default(),
            ddl_rect: Rect::default(),
            notice: None,
            keys_complete: false,
            locked_keys: Vec::new(),
            history: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            last: Table::default(),
            #[cfg(debug_assertions)]
            test_panic: false,
            sugg: Vec::new(),
            sugg_sel: None,
            sugg_rects: Vec::new(),
            actions: Vec::new(),
        };
        v.recalc();
        v
    }

    /// 서버에서 읽은 구조로 채운다(처음 · 새로 고침 · 적용 뒤) — 편집 모델도 처음으로.
    pub(crate) fn load(&mut self, orig: Table) {
        self.edit = orig.editable();
        self.orig = orig;
        self.is_new = false;
        self.loaded = true;
        self.editing = None;
        self.sel_row = 0;
        self.sel_col = 0;
        self.top = 0;
        self.forget_history();
        self.recalc();
    }

    /// 적용이 중간에 멈춘 뒤: 원본만 방금 읽은 것으로 바꾸고 **편집은 남긴다** — 이미 적용된 변경은 차이에서 사라지고 남은
    /// 변경만 다시 문장이 된다. 항목의 `origin`을 새 원본에 맞춘다(이름 바꾸기·추가가 이미 적용됐으면 지금 이름이 원본이다).
    pub(crate) fn rebase(&mut self, orig: Table) {
        fn fix(origin: &mut Option<String>, name: &str, has: impl Fn(&str) -> bool) {
            let keep = origin.as_deref().is_some_and(&has);
            if !keep && has(name) {
                *origin = Some(name.to_string());
            }
        }
        for c in &mut self.edit.columns {
            fix(&mut c.origin, &c.name, |n| {
                orig.columns.iter().any(|o| o.name == n)
            });
        }
        for k in &mut self.edit.keys {
            fix(&mut k.origin, &k.name, |n| {
                orig.keys.iter().any(|o| o.name == n)
            });
        }
        for i in &mut self.edit.indexes {
            fix(&mut i.origin, &i.name, |n| {
                orig.indexes.iter().any(|o| o.name == n)
            });
        }
        // ★ 서버에 **새로 생긴** 항목(지난 원본에 없고 편집 모델도 모르는 것 — 고친 스크립트로 더했거나 다른 세션이 더한 것)은
        //   편집 모델에 그대로 들인다. 들이지 않으면 다음 차이가 그것을 "지울 것"으로 잡는다(데이터 보호: 사용자가 지우라고 한 적이 없다).
        for c in &orig.columns {
            let known = self.orig.columns.iter().any(|o| o.name == c.name)
                || self
                    .edit
                    .columns
                    .iter()
                    .any(|e| e.name == c.name || e.origin.as_deref() == Some(c.name.as_str()));
            if !known {
                let mut c = c.clone();
                c.origin = Some(c.name.clone());
                self.edit.columns.push(c);
            }
        }
        for k in &orig.keys {
            let known = self.orig.keys.iter().any(|o| o.name == k.name)
                || self
                    .edit
                    .keys
                    .iter()
                    .any(|e| e.name == k.name || e.origin.as_deref() == Some(k.name.as_str()));
            if !known {
                let mut k = k.clone();
                k.origin = Some(k.name.clone());
                self.edit.keys.push(k);
            }
        }
        for x in &orig.indexes {
            let known = self.orig.indexes.iter().any(|o| o.name == x.name)
                || self
                    .edit
                    .indexes
                    .iter()
                    .any(|e| e.name == x.name || e.origin.as_deref() == Some(x.name.as_str()));
            if !known {
                let mut x = x.clone();
                x.origin = Some(x.name.clone());
                self.edit.indexes.push(x);
            }
        }
        self.orig = orig;
        self.is_new = false;
        self.loaded = true;
        self.editing = None;
        self.forget_history();
        self.recalc();
    }

    /// 새 테이블의 `CREATE`가 실행됐다 — 이제 그 이름의 기존 테이블이다(구조는 호스트가 곧 다시 읽어 준다).
    pub(crate) fn created(&mut self) {
        self.is_new = false;
        self.orig = Table {
            schema: self.orig.schema.clone(),
            name: self.edit.name.clone(),
            ..Table::default()
        };
    }

    /// 편집할 수 없는 상태(읽기 전용 · 아직 안 읽힘 · 라이선스 안내가 떠 있음).
    fn locked(&self) -> bool {
        self.read_only.is_some() || !self.loaded || self.notice.is_some()
    }

    /// 라이선스 안내를 띄우거나(이미 떠 있으면 시각 유지) 걷는다.
    pub(crate) fn set_notice(&mut self, on: bool) {
        if !on {
            self.notice = None;
        } else if self.notice.is_none() {
            self.notice = Some(std::time::Instant::now());
        }
    }

    /// [확인]이 켜지기까지 남은 초(0 = 켜짐 · 안내가 없으면 None).
    pub(crate) fn notice_left(&self) -> Option<u64> {
        self.notice
            .map(|t0| NOTICE_SECS.saturating_sub(t0.elapsed().as_secs()))
    }

    /// [확인](판정용 기동 명령도 이 길) — 시간이 지났을 때만 받아들인다.
    pub(crate) fn notice_ok(&mut self) -> bool {
        if self.notice_left() == Some(0) {
            self.notice = None;
            self.actions.push(TbAction::NoticeAck);
            true
        } else {
            false
        }
    }

    /// `complete` = 제약의 덧붙은 속성(규칙 · 식)을 읽었다 · `locked` = 그래도 고치면 안 되는 제약들(지연 검사 · 비활성 등).
    pub(crate) fn set_key_lock(&mut self, complete: bool, locked: Vec<String>) {
        self.keys_complete = complete;
        self.locked_keys = locked;
    }

    /// 적용 하나를 기록 섹션 맨 위에(시각 · 결과 글 · 보관한 스크립트 파일).
    pub(crate) fn push_history(&mut self, when: String, result: String, file: String) {
        self.history.insert(0, [when, result, file]);
    }

    /// 기록 `i`번째의 스크립트 파일 경로.
    pub(crate) fn history_file(&self, i: usize) -> Option<&str> {
        self.history
            .get(i)
            .map(|h| h[2].as_str())
            .filter(|f| !f.is_empty())
    }

    /// 판정용(Debug 빌드만 · 기동 명령 `tbledit.test_panic`).
    #[cfg(debug_assertions)]
    pub(crate) fn arm_test_panic(&mut self) {
        self.test_panic = true;
    }

    pub(crate) fn set_read_only(&mut self, reason: Option<String>) {
        self.read_only = reason;
    }

    pub(crate) fn dialect(&self) -> Dialect {
        self.dialect
    }

    pub(crate) fn is_new(&self) -> bool {
        self.is_new
    }

    pub(crate) fn loaded(&self) -> bool {
        self.loaded
    }

    /// 적용 전 테이블(스키마 · 이름).
    pub(crate) fn orig(&self) -> &Table {
        &self.orig
    }

    pub(crate) fn edited(&self) -> &Table {
        &self.edit
    }

    pub(crate) fn plan(&self) -> &Plan {
        &self.plan
    }

    /// 적용할 것이 있는가(탭의 미저장 표식).
    pub(crate) fn is_dirty(&self) -> bool {
        if self.is_new {
            !self.edit.name.is_empty() || !self.edit.columns.is_empty()
        } else {
            !self.plan.stmts.is_empty() || !self.plan.blocked.is_empty()
        }
    }

    /// 적용할 수 있는가: 문장이 있고 · 읽기 전용이 아니고 · 이름·타입이 빈 컬럼이 없다(새 테이블은 이름과 컬럼 하나 이상).
    pub(crate) fn can_apply(&self) -> bool {
        self.read_only.is_none()
            && self.notice.is_none()
            && self.loaded
            && !self.plan.stmts.is_empty()
            && !self.edit.name.trim().is_empty()
            && !self.edit.columns.is_empty()
            && self
                .edit
                .columns
                .iter()
                .all(|c| !c.name.trim().is_empty() && !c.data_type.trim().is_empty())
    }

    /// 지금의 스크립트(미리보기 · 적용 창).
    pub(crate) fn script(&self) -> String {
        nsql_ddl::script(self.dialect, &self.plan.stmts)
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        if (scale - self.scale).abs() > f32::EPSILON {
            self.cell.set_scale(scale);
            self.ddl.set_scale(scale);
        }
        self.scale = scale;
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if !on {
            self.ddl.set_focused(false);
        }
    }

    pub(crate) fn take_actions(&mut self) -> Vec<TbAction> {
        std::mem::take(&mut self.actions)
    }

    /// 클립보드·IME의 대상 상자: 셀 편집 중이면 그 상자 · 아니면 DDL 미리보기.
    pub(crate) fn textbox_mut(&mut self) -> &mut TextBox {
        if self.editing.is_some() {
            &mut self.cell
        } else {
            &mut self.ddl
        }
    }

    pub(crate) fn popup_open(&self) -> bool {
        self.ddl.popup_open() || !self.sugg.is_empty()
    }

    /// 지금 편집 중인 칸이 컬럼 섹션의 타입 칸인가.
    fn editing_type(&self) -> bool {
        self.section == Section::Columns
            && matches!(self.editing, Some(Target::Cell { col: 1, .. }))
    }

    /// 타입 완성 목록을 지금 친 글로 다시 거른다(앞머리 일치 · 대소문자 무시 · 이미 목록의 글과 같으면 닫는다).
    fn refresh_sugg(&mut self, show_all: bool) {
        self.sugg.clear();
        self.sugg_sel = None;
        if !self.editing_type() {
            return;
        }
        let typed = self.cell.text().trim().to_lowercase();
        self.sugg = type_suggestions(self.dialect, &typed, show_all);
    }

    /// 완성 목록의 `i`번째를 타입 칸에 넣고 확정한다.
    fn accept_sugg(&mut self, i: usize) {
        if let Some(choice) = self.sugg.get(i).copied() {
            self.cell.set_text(choice);
            self.commit_edit();
        }
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn specs(&self) -> &'static [ColSpec] {
        match self.section {
            Section::Columns => COLS_COLUMNS,
            Section::Keys => COLS_KEYS,
            Section::Indexes => COLS_INDEXES,
            Section::History => COLS_HISTORY,
        }
    }

    fn rows(&self) -> usize {
        match self.section {
            Section::Columns => self.edit.columns.len(),
            Section::Keys => self.edit.keys.len(),
            Section::Indexes => self.edit.indexes.len(),
            Section::History => self.history.len(),
        }
    }

    fn cell_text(&self, row: usize, col: usize) -> String {
        match self.section {
            Section::Columns => {
                self.edit
                    .columns
                    .get(row)
                    .map_or_else(String::new, |c| match col {
                        0 => c.name.clone(),
                        1 => c.data_type.clone(),
                        2 => String::new(),
                        3 => c.default.clone(),
                        _ => c.comment.clone(),
                    })
            }
            Section::Keys => self
                .edit
                .keys
                .get(row)
                .map_or_else(String::new, |k| match col {
                    0 => k.name.clone(),
                    1 => kind_text(k.kind).to_string(),
                    2 => k.cols.join(", "),
                    _ => match k.kind {
                        KeyKind::Foreign => {
                            let mut s = k.ref_table.clone();
                            if !k.ref_cols.is_empty() {
                                s.push_str(&format!("({})", k.ref_cols.join(", ")));
                            }
                            if !k.on_delete.is_empty() {
                                s.push_str(&format!(" ON DELETE {}", k.on_delete));
                            }
                            if !k.on_update.is_empty() {
                                s.push_str(&format!(" ON UPDATE {}", k.on_update));
                            }
                            s
                        }
                        KeyKind::Check => k.check.clone(),
                        _ => String::new(),
                    },
                }),
            Section::History => self
                .history
                .get(row)
                .and_then(|h| h.get(col))
                .cloned()
                .unwrap_or_default(),
            Section::Indexes => {
                self.edit
                    .indexes
                    .get(row)
                    .map_or_else(String::new, |i| match col {
                        0 => i.name.clone(),
                        1 => String::new(),
                        _ => i.cols.join(", "),
                    })
            }
        }
    }

    fn cell_checked(&self, row: usize, col: usize) -> bool {
        match (self.section, col) {
            (Section::Columns, 2) => self.edit.columns.get(row).is_some_and(|c| !c.nullable),
            (Section::Indexes, 1) => self.edit.indexes.get(row).is_some_and(|i| i.unique),
            _ => false,
        }
    }

    /// 그 칸을 고칠 수 있는가. 서버에서 읽은 외래 키·체크 제약은 이름만(식·삭제 규칙을 읽지 않았으므로 고치면 잃는다 → 지우고
    /// 새로 만드는 길만) · 종류는 새 제약만 바꾼다 · 기본 키/유니크의 정의 칸은 없다.
    fn cell_editable(&self, row: usize, col: usize) -> bool {
        if self.locked() {
            return false;
        }
        match self.section {
            Section::Columns | Section::Indexes => true,
            Section::History => false,
            Section::Keys => {
                let Some(k) = self.edit.keys.get(row) else {
                    return false;
                };
                let locked = k
                    .origin
                    .as_deref()
                    .is_some_and(|o| self.locked_keys.iter().any(|l| l == o));
                let fixed = k.origin.is_some()
                    && matches!(k.kind, KeyKind::Foreign | KeyKind::Check)
                    && (!self.keys_complete || locked);
                match col {
                    0 => true,
                    1 => k.origin.is_none(),
                    2 => !fixed && k.kind != KeyKind::Check,
                    _ => !fixed && matches!(k.kind, KeyKind::Foreign | KeyKind::Check),
                }
            }
        }
    }

    /// 칸에 글을 넣는다(편집 확정 · 시험 명령 공용). 바뀌었으면 true.
    pub(crate) fn set_cell(&mut self, row: usize, col: usize, text: &str) -> bool {
        if !self.cell_editable(row, col) || self.cell_text(row, col) == text {
            return false;
        }
        let d = self.dialect;
        match self.section {
            Section::Columns => {
                let Some(c) = self.edit.columns.get_mut(row) else {
                    return false;
                };
                match col {
                    0 => {
                        // 이름이 바뀌면 그 이름을 쓰는 제약·인덱스의 컬럼 목록도 따라간다(이름만 바뀐 것 = 제약 변경 아님).
                        let old = std::mem::replace(&mut c.name, nsql_ddl::fold_new_name(d, text));
                        let new = c.name.clone();
                        if !old.is_empty() {
                            for k in &mut self.edit.keys {
                                for kc in k.cols.iter_mut().filter(|kc| **kc == old) {
                                    kc.clone_from(&new);
                                }
                            }
                            for i in &mut self.edit.indexes {
                                for ic in i.cols.iter_mut().filter(|ic| **ic == old) {
                                    ic.clone_from(&new);
                                }
                            }
                        }
                    }
                    1 => c.data_type = text.trim().to_string(),
                    3 => c.default = text.trim().to_string(),
                    4 => c.comment = text.to_string(),
                    _ => return false,
                }
            }
            Section::Keys => {
                let Some(k) = self.edit.keys.get_mut(row) else {
                    return false;
                };
                match col {
                    0 => k.name = nsql_ddl::fold_new_name(d, text),
                    2 => k.cols = split_list(d, text),
                    3 if k.kind == KeyKind::Foreign => {
                        let (tbl, cols, rule, upd) = parse_fk(d, text);
                        k.ref_table = tbl;
                        k.ref_cols = cols;
                        k.on_delete = rule;
                        k.on_update = upd;
                    }
                    3 if k.kind == KeyKind::Check => k.check = text.trim().to_string(),
                    _ => return false,
                }
            }
            Section::History => return false,
            Section::Indexes => {
                let Some(i) = self.edit.indexes.get_mut(row) else {
                    return false;
                };
                match col {
                    0 => i.name = nsql_ddl::fold_new_name(d, text),
                    2 => i.cols = split_list(d, text),
                    _ => return false,
                }
            }
        }
        self.recalc();
        true
    }

    /// 체크 칸 뒤집기 · 새 제약의 종류 돌리기.
    pub(crate) fn toggle(&mut self, row: usize, col: usize) -> bool {
        if self.locked() {
            return false;
        }
        match (self.section, col) {
            (Section::Columns, 2) => match self.edit.columns.get_mut(row) {
                Some(c) => c.nullable = !c.nullable,
                None => return false,
            },
            (Section::Indexes, 1) => match self.edit.indexes.get_mut(row) {
                Some(i) => i.unique = !i.unique,
                None => return false,
            },
            (Section::Keys, 1) => {
                let table = self.edit.name.clone();
                let Some(k) = self.edit.keys.get_mut(row) else {
                    return false;
                };
                if k.origin.is_some() {
                    return false;
                }
                let next = match k.kind {
                    KeyKind::Primary => KeyKind::Unique,
                    KeyKind::Unique => KeyKind::Foreign,
                    KeyKind::Foreign => KeyKind::Check,
                    KeyKind::Check => KeyKind::Primary,
                };
                // 자동으로 붙인 이름이면 종류 꼬리도 따라 바꾼다.
                let auto = format!("{table}_{}", kind_suffix(k.kind));
                if k.name.starts_with(&auto) {
                    k.name = format!("{table}_{}{}", kind_suffix(next), &k.name[auto.len()..]);
                }
                k.kind = next;
            }
            _ => return false,
        }
        self.recalc();
        true
    }

    pub(crate) fn set_section(&mut self, s: Section) {
        if self.section != s {
            self.commit_edit();
            self.section = s;
            self.sel_row = 0;
            self.sel_col = 0;
            self.top = 0;
        }
    }

    /// 새 행을 맨 끝에 더한다. 새 행으로 선택이 옮겨 간다.
    pub(crate) fn add_row(&mut self) -> bool {
        if self.locked() {
            return false;
        }
        self.commit_edit();
        // 맨 끝에(DataGrip과 같다): 가운데에 넣는 자리 지정은 MySQL만 되고, 그때는 [위로]/[아래로]로 옮긴다.
        let at = self.rows();
        let table = self.edit.name.clone();
        match self.section {
            Section::Columns => self.edit.columns.insert(
                at,
                Column {
                    nullable: true,
                    ..Column::default()
                },
            ),
            Section::Keys => {
                let kind = if self.edit.keys.iter().any(|k| k.kind == KeyKind::Primary) {
                    KeyKind::Unique
                } else {
                    KeyKind::Primary
                };
                let n = self.edit.keys.iter().filter(|k| k.kind == kind).count();
                let name = if kind == KeyKind::Primary {
                    format!("{table}_PK")
                } else {
                    format!("{table}_{}{}", kind_suffix(kind), n + 1)
                };
                self.edit.keys.insert(
                    at,
                    nsql_ddl::Key {
                        name: nsql_ddl::fold_new_name(self.dialect, &name),
                        kind,
                        ..nsql_ddl::Key::default()
                    },
                );
            }
            Section::History => return false,
            Section::Indexes => {
                let name = format!("{table}_IX{}", self.edit.indexes.len() + 1);
                self.edit.indexes.insert(
                    at,
                    Index {
                        name: nsql_ddl::fold_new_name(self.dialect, &name),
                        ..Index::default()
                    },
                );
            }
        }
        self.sel_row = at;
        self.sel_col = 0;
        self.recalc();
        true
    }

    pub(crate) fn remove_row(&mut self, row: usize) -> bool {
        if self.locked() || row >= self.rows() {
            return false;
        }
        self.editing = None;
        match self.section {
            Section::Columns => {
                self.edit.columns.remove(row);
            }
            Section::Keys => {
                self.edit.keys.remove(row);
            }
            Section::History => return false,
            Section::Indexes => {
                self.edit.indexes.remove(row);
            }
        }
        self.sel_row = self.sel_row.min(self.rows().saturating_sub(1));
        self.recalc();
        true
    }

    /// 컬럼 순서(위 = −1 · 아래 = +1) — 컬럼 섹션만. 방언이 못 옮기면 문장이 아니라 경고 줄이 된다.
    pub(crate) fn move_row(&mut self, row: usize, delta: i32) -> bool {
        if self.locked() || self.section != Section::Columns {
            return false;
        }
        let Some(to) = row.checked_add_signed(delta as isize) else {
            return false;
        };
        if row >= self.edit.columns.len() || to >= self.edit.columns.len() {
            return false;
        }
        self.commit_edit();
        self.edit.columns.swap(row, to);
        self.sel_row = to;
        self.recalc();
        true
    }

    pub(crate) fn set_table_name(&mut self, text: &str) -> bool {
        let n = nsql_ddl::fold_new_name(self.dialect, text);
        if self.locked() || n == self.edit.name {
            return false;
        }
        self.edit.name = n;
        self.recalc();
        true
    }

    pub(crate) fn set_table_comment(&mut self, text: &str) -> bool {
        if self.locked() || text == self.edit.comment {
            return false;
        }
        self.edit.comment = text.to_string();
        self.recalc();
        true
    }

    /// 모든 변경을 버린다(원본으로).
    pub(crate) fn revert(&mut self) {
        self.editing = None;
        self.edit = self.orig.editable();
        self.sel_row = 0;
        self.top = 0;
        self.recalc();
    }

    /// 되돌리기 기록을 비우고 지금 모델을 기준으로 삼는다.
    fn forget_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last = self.edit.clone();
    }

    /// 한 단계 되돌리기 — 되돌렸으면 true.
    pub(crate) fn undo(&mut self) -> bool {
        self.cancel_edit();
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.edit, prev));
        self.last = self.edit.clone();
        self.after_history();
        true
    }

    /// 되돌린 것을 다시 — 했으면 true.
    pub(crate) fn redo(&mut self) -> bool {
        self.cancel_edit();
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.edit, next));
        self.last = self.edit.clone();
        self.after_history();
        true
    }

    fn after_history(&mut self) {
        self.sel_row = self.sel_row.min(self.rows().saturating_sub(1));
        self.recalc();
    }

    fn recalc(&mut self) {
        // 모델이 달라졌으면 지난 상태를 한 단계 쌓는다(되돌리기·다시 실행 자신은 `last`를 먼저 맞춰 두어 쌓이지 않는다).
        if self.edit != self.last {
            self.undo
                .push(std::mem::replace(&mut self.last, self.edit.clone()));
            if self.undo.len() > UNDO_MAX {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.plan = if self.is_new {
            Plan {
                stmts: if self.edit.columns.is_empty() {
                    Vec::new()
                } else {
                    nsql_ddl::create_table(self.dialect, &self.edit)
                },
                blocked: Vec::new(),
            }
        } else {
            nsql_ddl::plan(
                self.dialect,
                &self.orig,
                &nsql_ddl::diff(&self.orig, &self.edit),
            )
        };
        self.ddl.set_text(&self.script());
        self.actions.push(TbAction::Changed);
    }

    /// 행의 상태 표식: `+` 새 것 · `*` 바뀐 것 · 빈 글 = 그대로.
    fn row_mark(&self, row: usize) -> &'static str {
        match self.section {
            Section::Columns => match self.edit.columns.get(row) {
                Some(c) => match c
                    .origin
                    .as_deref()
                    .and_then(|o| self.orig.columns.iter().find(|x| x.name == o))
                {
                    None => "+",
                    Some(o) => {
                        // 타입은 뜻으로 견준다(`varchar` = `character varying` · 문장 생성과 같은 잣대 — 글자만 다르면 바뀜이 아니다).
                        let same = o.name == c.name
                            && nsql_ddl::norm_type(&o.data_type)
                                == nsql_ddl::norm_type(&c.data_type)
                            && o.nullable == c.nullable
                            && o.default.trim() == c.default.trim()
                            && o.comment == c.comment;
                        if same {
                            ""
                        } else {
                            "*"
                        }
                    }
                },
                None => "",
            },
            Section::Keys => match self.edit.keys.get(row) {
                Some(k) => match k
                    .origin
                    .as_deref()
                    .and_then(|o| self.orig.keys.iter().find(|x| x.name == o))
                {
                    None => "+",
                    Some(o) => {
                        let mut same = k.clone();
                        same.origin = None;
                        if *o == same {
                            ""
                        } else {
                            "*"
                        }
                    }
                },
                None => "",
            },
            Section::History => "",
            Section::Indexes => match self.edit.indexes.get(row) {
                Some(i) => match i
                    .origin
                    .as_deref()
                    .and_then(|o| self.orig.indexes.iter().find(|x| x.name == o))
                {
                    None => "+",
                    Some(o) => {
                        let mut same = i.clone();
                        same.origin = None;
                        if *o == same {
                            ""
                        } else {
                            "*"
                        }
                    }
                },
                None => "",
            },
        }
    }

    // ───────────────────────────── 편집

    fn target_rect(&self, tg: Target) -> Rect {
        match tg {
            Target::TableName => self.name_rect,
            Target::TableComment => self.comment_rect,
            Target::Cell { row, col } => {
                let (x, w) = self
                    .col_x
                    .get(col)
                    .copied()
                    .unwrap_or((self.grid_rect.x, 0));
                let y = self.grid_rect.y + self.row_h * (1 + row.saturating_sub(self.top) as i32);
                Rect::new(x, y, w, self.row_h)
            }
        }
    }

    fn begin_edit(&mut self, tg: Target, replace: Option<char>) {
        let text = match tg {
            Target::TableName => {
                if self.read_only.is_some() {
                    return;
                }
                self.edit.name.clone()
            }
            Target::TableComment => {
                if self.read_only.is_some() {
                    return;
                }
                self.edit.comment.clone()
            }
            Target::Cell { row, col } => {
                if !self.cell_editable(row, col) || self.specs().get(col).is_none_or(|s| s.check) {
                    return;
                }
                if self.section == Section::Keys && col == 1 {
                    return;
                }
                self.cell_text(row, col)
            }
        };
        self.editing = Some(tg);
        let mut inv = Invalidations::default();
        match replace {
            Some(c) => {
                self.cell.set_text("");
                self.cell.set_focused(true);
                self.cell
                    .on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
            }
            None => {
                self.cell.set_text(&text);
                self.cell.set_focused(true);
                self.cell.on_event(&InputEvent::SelectAll, &mut inv);
            }
        }
        self.ddl.set_focused(false);
        // 타입 칸 = 완성 목록(전체 선택으로 열었으면 목록 전부 · 글자로 열었으면 그 글자로 거른 것).
        self.refresh_sugg(replace.is_none());
    }

    /// 그 칸의 편집을 연다(판정용 기동 명령 `tbledit.edit:` — 마우스·키 없이 편집 상태와 완성 목록을 본다).
    pub(crate) fn edit_cell(&mut self, row: usize, col: usize) {
        if row < self.rows() {
            self.sel_row = row;
            self.sel_col = col;
            self.begin_edit(Target::Cell { row, col }, None);
        }
    }

    fn commit_edit(&mut self) {
        let Some(tg) = self.editing.take() else {
            return;
        };
        self.sugg.clear();
        self.sugg_sel = None;
        let text = self.cell.text();
        self.cell.set_focused(false);
        match tg {
            Target::TableName => {
                self.set_table_name(&text);
            }
            Target::TableComment => {
                self.set_table_comment(&text);
            }
            Target::Cell { row, col } => {
                self.set_cell(row, col, &text);
            }
        }
    }

    fn cancel_edit(&mut self) {
        self.sugg.clear();
        self.sugg_sel = None;
        self.editing = None;
        self.cell.set_focused(false);
    }

    fn visible_rows(&self) -> usize {
        ((self.grid_rect.h / self.row_h.max(1)) - 1).max(1) as usize
    }

    fn ensure_visible(&mut self) {
        let vis = self.visible_rows();
        if self.sel_row < self.top {
            self.top = self.sel_row;
        } else if self.sel_row >= self.top + vis {
            self.top = self.sel_row + 1 - vis;
        }
    }

    fn cell_at(&self, p: Point) -> Option<(usize, usize)> {
        if !self.grid_rect.contains(p) || p.y < self.grid_rect.y + self.row_h {
            return None;
        }
        let row = self.top + ((p.y - self.grid_rect.y) / self.row_h.max(1) - 1) as usize;
        let col = self
            .col_x
            .iter()
            .position(|&(x, w)| p.x >= x && p.x < x + w)?;
        (row < self.rows()).then_some((row, col))
    }

    fn press(&mut self, b: Btn) {
        match b {
            Btn::Sec(s) => self.set_section(s),
            Btn::Add => {
                if self.add_row() {
                    self.ensure_visible();
                    self.begin_edit(
                        Target::Cell {
                            row: self.sel_row,
                            col: 0,
                        },
                        None,
                    );
                }
            }
            Btn::Remove => {
                self.remove_row(self.sel_row);
            }
            Btn::Up => {
                self.move_row(self.sel_row, -1);
            }
            Btn::Down => {
                self.move_row(self.sel_row, 1);
            }
            Btn::Refresh => {
                self.commit_edit();
                self.actions.push(TbAction::Refresh);
            }
            Btn::Revert => self.revert(),
            Btn::Apply => {
                self.commit_edit();
                if self.can_apply() {
                    self.actions.push(TbAction::Apply);
                }
            }
            Btn::NoticeOk => {
                self.notice_ok();
            }
            Btn::NoticeLicense => self.actions.push(TbAction::OpenLicense),
        }
        self.ensure_visible();
    }

    /// 사건 처리 — 다시 그릴 일이 있으면 true(호스트 규칙 = 확장 상세 뷰와 같다).
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        // ★ 라이선스 안내가 떠 있는 동안 = 안내의 두 버튼만(키·휠·다른 클릭은 삼킨다 · Esc/Enter로 닫히지 않는다).
        if self.notice.is_some() {
            match *ev {
                InputEvent::MouseMove { x, y } => {
                    let p = Point { x, y };
                    self.hover = self
                        .buttons
                        .iter()
                        .find(|(_, r, on)| *on && r.contains(p))
                        .map(|(b, ..)| *b);
                }
                InputEvent::MouseDown { x, y, .. } => {
                    let p = Point { x, y };
                    self.pressed = self
                        .buttons
                        .iter()
                        .find(|(_, r, on)| *on && r.contains(p))
                        .map(|(b, ..)| *b);
                }
                InputEvent::MouseUp { x, y } => {
                    let p = Point { x, y };
                    if let Some(b) = self.pressed.take() {
                        if self
                            .buttons
                            .iter()
                            .any(|(bb, r, on)| *bb == b && *on && r.contains(p))
                        {
                            self.press(b);
                        }
                    }
                }
                _ => {}
            }
            return true;
        }
        let mut inv = Invalidations::default();
        if self.ddl.popup_open() {
            self.ddl.on_event(ev, &mut inv);
            self.after_ddl();
            return true;
        }
        // ── 셀 편집 중: Enter/Tab = 확정 · Esc = 취소 · 상자 밖 클릭 = 확정 뒤 그 클릭을 이어서 처리.
        if let Some(tg) = self.editing {
            // 타입 완성 목록이 열려 있으면: ↑↓ = 고르기 · Enter = 고른 것 넣기 · Esc = 목록만 닫기 · 줄 클릭 = 넣기.
            if !self.sugg.is_empty() {
                match *ev {
                    InputEvent::Key { key: Key::Down, .. } => {
                        let n = self.sugg.len();
                        self.sugg_sel = Some(self.sugg_sel.map_or(0, |i| (i + 1) % n));
                        return true;
                    }
                    InputEvent::Key { key: Key::Up, .. } => {
                        let n = self.sugg.len();
                        self.sugg_sel = Some(self.sugg_sel.map_or(n - 1, |i| (i + n - 1) % n));
                        return true;
                    }
                    InputEvent::Key {
                        key: Key::Enter, ..
                    } if self.sugg_sel.is_some() => {
                        self.accept_sugg(self.sugg_sel.unwrap_or(0));
                        return true;
                    }
                    InputEvent::Key {
                        key: Key::Escape, ..
                    } => {
                        self.sugg.clear();
                        self.sugg_sel = None;
                        return true;
                    }
                    InputEvent::MouseDown { x, y, .. } => {
                        let p = Point { x, y };
                        if let Some(i) = self.sugg_rects.iter().position(|r| r.contains(p)) {
                            self.accept_sugg(i);
                            return true;
                        }
                    }
                    InputEvent::MouseMove { x, y } => {
                        let p = Point { x, y };
                        if let Some(i) = self.sugg_rects.iter().position(|r| r.contains(p)) {
                            self.sugg_sel = Some(i);
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            match *ev {
                InputEvent::Key {
                    key: Key::Enter, ..
                } => {
                    self.commit_edit();
                    return true;
                }
                InputEvent::Key {
                    key: Key::Escape, ..
                } => {
                    self.cancel_edit();
                    return true;
                }
                InputEvent::Key {
                    key: Key::Tab,
                    shift,
                    ..
                } => {
                    self.commit_edit();
                    if let Target::Cell { row, col } = tg {
                        self.sel_row = row.min(self.rows().saturating_sub(1));
                        self.step_col(col, !shift);
                        self.begin_edit(
                            Target::Cell {
                                row: self.sel_row,
                                col: self.sel_col,
                            },
                            None,
                        );
                    }
                    return true;
                }
                InputEvent::MouseDown { x, y, .. } | InputEvent::DoubleClick { x, y, .. }
                    if !self.target_rect(tg).contains(Point { x, y }) =>
                {
                    self.commit_edit();
                }
                InputEvent::RightDown { .. } => {
                    self.commit_edit();
                }
                InputEvent::Wheel { .. } => {
                    self.commit_edit();
                }
                _ => {
                    self.cell.on_event(ev, &mut inv);
                    // 글이 바뀌었으면 완성 목록을 다시 거른다(키·문자 입력 뒤).
                    if matches!(ev, InputEvent::Char { .. } | InputEvent::Key { .. }) {
                        self.refresh_sugg(false);
                    }
                    return true;
                }
            }
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                self.mouse = p;
                let h = self
                    .buttons
                    .iter()
                    .find(|(_, r, on)| *on && r.contains(p))
                    .map(|(b, ..)| *b);
                let changed = h != self.hover;
                self.hover = h;
                self.ddl.on_event(ev, &mut inv);
                changed || !inv.is_empty()
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                self.pressed = self
                    .buttons
                    .iter()
                    .find(|(_, r, on)| *on && r.contains(p))
                    .map(|(b, ..)| *b);
                if self.pressed.is_some() {
                    return true;
                }
                if self.name_rect.contains(p) {
                    self.begin_edit(Target::TableName, None);
                } else if self.comment_rect.contains(p) {
                    self.begin_edit(Target::TableComment, None);
                } else if let Some((row, col)) = self.cell_at(p) {
                    let again = self.sel_row == row && self.sel_col == col;
                    self.sel_row = row;
                    self.sel_col = col;
                    self.ddl.set_focused(false);
                    let check = self.specs().get(col).is_some_and(|s| s.check);
                    if check || (self.section == Section::Keys && col == 1) {
                        self.toggle(row, col);
                    } else if again {
                        // 이미 선택된 칸을 다시 누르면 편집(DataGrip · 엑셀식 두 번째 클릭).
                        self.begin_edit(Target::Cell { row, col }, None);
                    }
                } else if self.ddl_rect.contains(p) {
                    self.ddl.set_focused(true);
                    self.ddl.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::DoubleClick { x, y, .. } => {
                let p = Point { x, y };
                if let Some((row, col)) = self.cell_at(p) {
                    self.sel_row = row;
                    self.sel_col = col;
                    if self.section == Section::History {
                        self.actions.push(TbAction::OpenHistory(row));
                    } else {
                        self.begin_edit(Target::Cell { row, col }, None);
                    }
                } else if self.ddl_rect.contains(p) {
                    self.ddl.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::MouseUp { x, y } => {
                if let Some(b) = self.pressed.take() {
                    let p = Point { x, y };
                    if self
                        .buttons
                        .iter()
                        .any(|(bb, r, on)| *bb == b && *on && r.contains(p))
                    {
                        self.press(b);
                    }
                    return true;
                }
                self.ddl.on_event(ev, &mut inv);
                self.after_ddl();
                true
            }
            InputEvent::RightDown { x, y } => {
                if self.ddl_rect.contains(Point { x, y }) {
                    self.ddl.set_focused(true);
                    self.ddl.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::Wheel { delta } => {
                if self.ddl_rect.contains(self.mouse) {
                    self.ddl.on_event(ev, &mut inv);
                } else {
                    let max_top = self.rows().saturating_sub(self.visible_rows());
                    let step = 3usize;
                    self.top = if delta > 0 {
                        self.top.saturating_sub(step)
                    } else {
                        (self.top + step).min(max_top)
                    };
                }
                true
            }
            InputEvent::HWheel { .. } => {
                self.ddl.on_event(ev, &mut inv);
                true
            }
            InputEvent::Key { .. } | InputEvent::SelectAll if self.ddl_focused() => {
                self.ddl.on_event(ev, &mut inv);
                self.after_ddl();
                true
            }
            InputEvent::Key { key, shift, .. } => {
                let rows = self.rows();
                match key {
                    Key::Up => self.sel_row = self.sel_row.saturating_sub(1),
                    Key::Down => self.sel_row = (self.sel_row + 1).min(rows.saturating_sub(1)),
                    Key::PageUp => self.sel_row = self.sel_row.saturating_sub(self.visible_rows()),
                    Key::PageDown => {
                        self.sel_row =
                            (self.sel_row + self.visible_rows()).min(rows.saturating_sub(1));
                    }
                    Key::Home => self.sel_row = 0,
                    Key::End => self.sel_row = rows.saturating_sub(1),
                    Key::Left => self.step_col(self.sel_col, false),
                    Key::Right => self.step_col(self.sel_col, true),
                    Key::Tab => self.step_col(self.sel_col, !shift),
                    Key::Enter if self.section == Section::History => {
                        if self.sel_row < self.history.len() {
                            self.actions.push(TbAction::OpenHistory(self.sel_row));
                        }
                    }
                    Key::Enter => self.begin_edit(
                        Target::Cell {
                            row: self.sel_row,
                            col: self.sel_col,
                        },
                        None,
                    ),
                    Key::Space => {
                        self.toggle(self.sel_row, self.sel_col);
                    }
                    Key::Delete => {
                        self.remove_row(self.sel_row);
                    }
                    _ => return false,
                }
                self.ensure_visible();
                true
            }
            InputEvent::Undo => {
                self.undo();
                true
            }
            InputEvent::Redo => {
                self.redo();
                true
            }
            // 글자를 치면 그 글자로 편집 시작(엑셀식).
            InputEvent::Char { c, .. } if !c.is_control() && !self.ddl_focused() => {
                self.begin_edit(
                    Target::Cell {
                        row: self.sel_row,
                        col: self.sel_col,
                    },
                    Some(c),
                );
                true
            }
            _ => false,
        }
    }

    fn ddl_focused(&self) -> bool {
        self.ddl.is_focused()
    }

    fn step_col(&mut self, from: usize, forward: bool) {
        let n = self.specs().len();
        self.sel_col = if forward {
            (from + 1).min(n - 1)
        } else {
            from.saturating_sub(1)
        };
    }

    /// DDL 상자의 우클릭 메뉴 — 읽기 전용이라 복사만(선택 없으면 전체).
    fn after_ddl(&mut self) {
        if let Some(a) = self.ddl.take_edit_ctx() {
            if matches!(a, nexa_ctl::EditCtxAction::Copy) {
                let text = self.ddl.copy_selection().unwrap_or_else(|| self.ddl.text());
                let _ = crate::clipboard::write_text(&text);
            }
        }
    }

    // ───────────────────────────── 그리기

    #[allow(clippy::too_many_lines)]
    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        #[cfg(debug_assertions)]
        assert!(
            !self.test_panic,
            "tbledit.test_panic (injected by a startup command)"
        );
        let b = self.bounds;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        dc.fill_rect(b, th.field_bg);
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let pad = self.s(12.0);
        let x0 = b.x + pad;
        let w = (b.w - pad * 2).max(self.s(120.0));
        let field_h = lh + self.s(8.0);
        let mut y = b.y + pad;
        self.buttons.clear();
        if !self.loaded {
            dc.text(x0, y, b, t(Msg::TbeLoading), th.text_dim);
            self.grid_rect = Rect::default();
            self.ddl_rect = Rect::default();
            return;
        }
        // ★ 라이선스 안내(D-328): 편집기 대신 안내 판 + [확인(남은 초)] [라이선스…].
        if self.notice.is_some() {
            self.grid_rect = Rect::default();
            self.ddl_rect = Rect::default();
            self.name_rect = Rect::default();
            self.comment_rect = Rect::default();
            dc.select_font(FontSlot::Base, true);
            dc.text(x0, y, b, t(Msg::TbeLicTitle), th.text);
            dc.select_font(FontSlot::Base, false);
            y += lh + self.s(10.0);
            dc.text(x0, y, b, t(Msg::TbeLicBody), th.text);
            y += lh + self.s(4.0);
            dc.text(x0, y, b, t(Msg::TbeLicBody2), th.text_dim);
            y += lh + self.s(16.0);
            let left = self.notice_left().unwrap_or(0);
            let ok = if left == 0 {
                t(Msg::TbeLicOk).to_string()
            } else {
                tf(Msg::TbeLicOkIn, &[&left.to_string()])
            };
            let btn_h = lh + self.s(12.0);
            let bw = dc.text_width(&ok) + self.s(36.0);
            self.button(
                dc,
                th,
                Btn::NoticeOk,
                Rect::new(x0, y, bw, btn_h),
                &ok,
                left == 0,
                false,
                true,
            );
            let lic = t(Msg::TbeLicBtn);
            let lw = dc.text_width(lic) + self.s(28.0);
            self.button(
                dc,
                th,
                Btn::NoticeLicense,
                Rect::new(x0 + bw + self.s(8.0), y, lw, btn_h),
                lic,
                true,
                false,
                false,
            );
            return;
        }
        // ── 머리: 테이블 이름 · 코멘트(누르면 편집).
        let lab = t(Msg::TbeTableName);
        let lab_w = dc.text_width(lab) + self.s(8.0);
        text_mid(dc, x0, y, field_h, b, lab, th.text_dim);
        let name_w = (w * 30 / 100).max(self.s(120.0));
        self.name_rect = Rect::new(x0 + lab_w, y, name_w, field_h);
        let lab2 = t(Msg::TbeColComment);
        let lab2_x = self.name_rect.right() + self.s(16.0);
        let lab2_w = dc.text_width(lab2) + self.s(8.0);
        text_mid(dc, lab2_x, y, field_h, b, lab2, th.text_dim);
        self.comment_rect = Rect::new(
            lab2_x + lab2_w,
            y,
            (x0 + w - lab2_x - lab2_w).max(self.s(60.0)),
            field_h,
        );
        let schema_name = if self.orig.schema.is_empty() {
            self.edit.name.clone()
        } else {
            format!("{}.{}", self.orig.schema, self.edit.name)
        };
        for (r, text, tg) in [
            (self.name_rect, schema_name, Target::TableName),
            (
                self.comment_rect,
                self.edit.comment.clone(),
                Target::TableComment,
            ),
        ] {
            dc.fill_round_rect(r, self.s(3.0), th.panel_bg_alt);
            dc.stroke_round_rect(r, self.s(3.0), th.border, 1.0);
            if self.editing != Some(tg) {
                text_mid(dc, r.x + self.s(6.0), r.y, r.h, r, &text, th.text);
            }
        }
        y += field_h + self.s(10.0);
        // ── 섹션 탭 · 행 버튼 · 오른쪽 동작 버튼.
        let btn_h = lh + self.s(10.0);
        let mut bx = x0;
        let ro = self.read_only.is_some();
        let mut left: Vec<(Btn, String, bool, bool)> = [
            Section::Columns,
            Section::Keys,
            Section::Indexes,
            Section::History,
        ]
        .into_iter()
        .map(|s| {
            let n = match s {
                Section::Columns => self.edit.columns.len(),
                Section::Keys => self.edit.keys.len(),
                Section::Indexes => self.edit.indexes.len(),
                Section::History => self.history.len(),
            };
            (
                Btn::Sec(s),
                format!("{} ({n})", t(s.label())),
                true,
                self.section == s,
            )
        })
        .collect();
        let has_row = self.sel_row < self.rows();
        let cols_sec = self.section == Section::Columns;
        // 기록 섹션은 읽기 전용(행 추가·삭제 없음).
        let ro = ro || self.section == Section::History;
        left.push((Btn::Add, t(Msg::TbeBtnAdd).to_string(), !ro, false));
        left.push((
            Btn::Remove,
            t(Msg::TbeBtnRemove).to_string(),
            !ro && has_row,
            false,
        ));
        left.push((
            Btn::Up,
            t(Msg::TbeBtnUp).to_string(),
            !ro && cols_sec && has_row && self.sel_row > 0,
            false,
        ));
        left.push((
            Btn::Down,
            t(Msg::TbeBtnDown).to_string(),
            !ro && cols_sec && self.sel_row + 1 < self.rows(),
            false,
        ));
        for (i, (btn, label, on, active)) in left.into_iter().enumerate() {
            if i == 3 {
                bx += self.s(14.0);
            }
            let bw = dc.text_width(&label) + self.s(20.0);
            self.button(
                dc,
                th,
                btn,
                Rect::new(bx, y, bw, btn_h),
                &label,
                on,
                active,
                false,
            );
            bx += bw + self.s(6.0);
        }
        let right: [(Btn, String, bool, bool); 3] = [
            (
                Btn::Apply,
                t(Msg::TbeBtnApply).to_string(),
                self.can_apply(),
                true,
            ),
            (
                Btn::Revert,
                t(Msg::TbeBtnRevert).to_string(),
                !ro && self.is_dirty(),
                false,
            ),
            (
                Btn::Refresh,
                t(Msg::TbeBtnRefresh).to_string(),
                !self.is_new,
                false,
            ),
        ];
        let mut rx = x0 + w;
        for (btn, label, on, primary) in right {
            let bw = dc.text_width(&label) + self.s(24.0);
            rx -= bw;
            self.button(
                dc,
                th,
                btn,
                Rect::new(rx, y, bw, btn_h),
                &label,
                on,
                false,
                primary,
            );
            rx -= self.s(6.0);
        }
        let status = if self.plan.stmts.is_empty() {
            t(Msg::TbeNoChanges).to_string()
        } else {
            tf(Msg::TbeChanges, &[&self.plan.stmts.len().to_string()])
        };
        let sw = dc.text_width(&status);
        if rx - sw - self.s(8.0) > bx {
            text_mid(dc, rx - sw - self.s(8.0), y, btn_h, b, &status, th.text_dim);
        }
        y += btn_h + self.s(8.0);
        // ── 아래 영역 높이: 경고 줄 + DDL 미리보기(전체의 1/3 · 최소 4줄).
        let mut notes: Vec<(String, bool)> = Vec::new();
        if let Some(r) = &self.read_only {
            notes.push((r.clone(), true));
        }
        for bl in &self.plan.blocked {
            let m = match bl.reason {
                Reason::NeedsRebuild => Msg::TbeBlockedRebuild,
                Reason::NotSupported => Msg::TbeBlockedUnsupported,
            };
            notes.push((tf(m, &[&bl.what]), true));
        }
        notes.truncate(4);
        let notes_h = notes.len() as i32 * (lh + self.s(2.0));
        let ddl_h = ((b.bottom() - y) / 3).max(lh * 4);
        let ddl_label_h = lh + self.s(6.0);
        let grid_h = (b.bottom() - y - notes_h - ddl_h - ddl_label_h - pad).max(lh * 3);
        // ── 그리드.
        self.row_h = lh + self.s(8.0);
        self.grid_rect = Rect::new(x0, y, w, grid_h);
        let g = self.grid_rect;
        dc.stroke_round_rect(g, 0, th.border, 1.0);
        let specs = self.specs();
        let mark_w = self.s(18.0);
        let total: i32 = specs.iter().map(|c| c.weight).sum();
        self.col_x.clear();
        let mut cx = g.x + mark_w;
        for (i, c) in specs.iter().enumerate() {
            let cw = if i + 1 == specs.len() {
                g.x + g.w - cx
            } else {
                (g.w - mark_w) * c.weight / total.max(1)
            };
            self.col_x.push((cx, cw));
            cx += cw;
        }
        let head = Rect::new(g.x, g.y, g.w, self.row_h);
        dc.fill_rect(head, th.panel_bg_alt);
        for (c, &(x, cw)) in specs.iter().zip(&self.col_x) {
            let cell = Rect::new(x, g.y, cw, self.row_h);
            text_mid(
                dc,
                x + self.s(6.0),
                g.y,
                self.row_h,
                cell,
                t(c.title),
                th.text_dim,
            );
        }
        let vis = self.visible_rows();
        self.top = self.top.min(self.rows().saturating_sub(1));
        for vi in 0..vis {
            let row = self.top + vi;
            if row >= self.rows() {
                break;
            }
            let ry = g.y + self.row_h * (1 + vi as i32);
            let rr = Rect::new(g.x + 1, ry, g.w - 2, self.row_h);
            if row == self.sel_row {
                dc.fill_rect(
                    rr,
                    if self.focused {
                        th.sel_bg
                    } else {
                        th.sel_bg_inactive
                    },
                );
            }
            let mark = self.row_mark(row);
            if !mark.is_empty() {
                text_mid(
                    dc,
                    g.x + self.s(5.0),
                    ry,
                    self.row_h,
                    rr,
                    mark,
                    if mark == "+" { th.ok } else { th.warn },
                );
            }
            for (ci, (c, &(x, cw))) in specs.iter().zip(&self.col_x).enumerate() {
                let cell = Rect::new(x, ry, cw, self.row_h);
                if self.editing == Some(Target::Cell { row, col: ci }) {
                    continue;
                }
                if c.check {
                    let sz = self.s(13.0);
                    let cb = Rect::new(x + self.s(8.0), ry + (self.row_h - sz) / 2, sz, sz);
                    dc.stroke_round_rect(cb, self.s(2.0), th.border, 1.0);
                    if self.cell_checked(row, ci) {
                        let inner = Rect::new(
                            cb.x + self.s(3.0),
                            cb.y + self.s(3.0),
                            sz - self.s(6.0),
                            sz - self.s(6.0),
                        );
                        dc.fill_round_rect(inner, self.s(1.0), th.accent);
                    }
                } else {
                    let text = self.cell_text(row, ci);
                    let fg = if self.cell_editable(row, ci) {
                        th.text
                    } else {
                        th.text_dim
                    };
                    text_mid(dc, x + self.s(6.0), ry, self.row_h, cell, &text, fg);
                }
                if row == self.sel_row && ci == self.sel_col && self.focused {
                    dc.stroke_round_rect(cell, 0, th.accent, 1.0);
                }
            }
        }
        y += grid_h + self.s(4.0);
        // ── 경고 줄.
        for (line, warn) in &notes {
            dc.text(x0, y, b, line, if *warn { th.warn } else { th.text_dim });
            y += lh + self.s(2.0);
        }
        // ── DDL 미리보기.
        dc.text(x0, y + self.s(2.0), b, t(Msg::TbeDdlPreview), th.text_dim);
        y += ddl_label_h;
        self.ddl_rect = Rect::new(x0, y, w, (b.bottom() - y - self.s(6.0)).max(lh));
        dc.stroke_round_rect(self.ddl_rect, 0, th.border, 1.0);
        let mut inv = Invalidations::default();
        self.ddl.set_bounds(self.ddl_rect, &mut inv);
        self.ddl.paint(dc, th);
        // ── 셀 편집 상자(맨 위).
        if let Some(tg) = self.editing {
            let r = self.target_rect(tg);
            dc.fill_rect(r, th.field_bg);
            self.cell.set_bounds(r, &mut inv);
            self.cell.paint(dc, th);
            dc.stroke_round_rect(r, 0, th.accent, 1.0);
        }
        // ── 타입 완성 목록의 자리(그리기는 팝업 층 `paint_popup`): 칸 아래 · 모자라면 위 · 뷰 영역 밖으로 나가지 않는다.
        self.sugg_rects.clear();
        if let (false, Some(tg)) = (self.sugg.is_empty(), self.editing) {
            let cell = self.target_rect(tg);
            let line_h = lh + self.s(6.0);
            let n = self.sugg.len().min(SUGG_MAX) as i32;
            let mut w0 = cell.w.max(self.s(180.0));
            for name in &self.sugg {
                w0 = w0.max(dc.text_width(name) + self.s(20.0));
            }
            let w0 = w0.min(b.w - pad);
            let x = cell
                .x
                .min(b.right() - w0 - self.s(4.0))
                .max(b.x + self.s(4.0));
            let below = cell.bottom() + n * line_h <= b.bottom() - self.s(4.0);
            let y0 = if below {
                cell.bottom()
            } else {
                (cell.y - n * line_h).max(b.y)
            };
            for i in 0..n {
                self.sugg_rects
                    .push(Rect::new(x, y0 + i * line_h, w0, line_h));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn button(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        btn: Btn,
        r: Rect,
        label: &str,
        on: bool,
        active: bool,
        primary: bool,
    ) {
        let hot = on && self.hover == Some(btn);
        let down = on && self.pressed == Some(btn);
        let st = nexa_ctl::tokens::State::of(false, hot, down, true);
        let ty = dc.text_center_y(r.y, r.h) + if down { self.s(1.0) } else { 0 };
        let tx = r.x + (r.w - dc.text_width(label)) / 2;
        if primary && on {
            dc.fill_round_rect(r, self.s(4.0), th.accent);
            dc.state_layer(r, th.text, st);
            dc.text(tx, ty, r, label, nexa_ctl::Color(0x00FF_FFFF));
        } else {
            dc.fill_round_rect(
                r,
                self.s(4.0),
                if active {
                    th.sel_bg_inactive
                } else {
                    th.panel_bg_alt
                },
            );
            if on {
                dc.state_layer(r, th.text, st);
            }
            dc.stroke_round_rect(
                r,
                self.s(4.0),
                if active || hot { th.accent } else { th.border },
                1.0,
            );
            dc.text(tx, ty, r, label, if on { th.text } else { th.text_dim });
        }
        self.buttons.push((btn, r, on));
    }

    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.ddl.popup_open() {
            self.ddl.paint_popup(dc, th);
        }
        if let (Some(first), Some(last)) = (self.sugg_rects.first(), self.sugg_rects.last()) {
            dc.select_font(FontSlot::Base, false);
            let frame = Rect::new(first.x, first.y, first.w, last.bottom() - first.y);
            dc.fill_rect(frame, th.panel_bg);
            dc.stroke_round_rect(frame, 0, th.border, 1.0);
            for (i, (r, name)) in self.sugg_rects.iter().zip(&self.sugg).enumerate() {
                if self.sugg_sel == Some(i) {
                    dc.fill_rect(Rect::new(r.x + 1, r.y, r.w - 2, r.h), th.sel_bg);
                }
                text_mid(dc, r.x + self.s(8.0), r.y, r.h, *r, name, th.text);
            }
        }
    }

    /// 판정용 덤프(기동 명령 `tbledit.dump:` · 시험): 섹션 · 행 · 문장 · 막힌 변경.
    pub(crate) fn dump(&self) -> String {
        let mut s = format!(
            "table={}.{} new={} loaded={} dirty={} can_apply={} section={} rows={}{}\n",
            self.orig.schema,
            self.edit.name,
            self.is_new,
            self.loaded,
            self.is_dirty(),
            self.can_apply(),
            self.section.id(),
            self.rows(),
            if self.notice.is_some() {
                " notice=on"
            } else {
                ""
            }
        );
        for r in 0..self.rows() {
            let cells: Vec<String> = (0..self.specs().len())
                .map(|c| {
                    if self.specs()[c].check {
                        if self.cell_checked(r, c) { "Y" } else { "N" }.to_string()
                    } else {
                        self.cell_text(r, c)
                    }
                })
                .collect();
            s.push_str(&format!(
                "row|{}|{}|{}\n",
                r,
                self.row_mark(r),
                cells.join("|")
            ));
        }
        if !self.undo.is_empty() || !self.redo.is_empty() {
            s.push_str(&format!("hist|{}|{}\n", self.undo.len(), self.redo.len()));
        }
        if !self.sugg.is_empty() {
            s.push_str(&format!(
                "sugg|{}|{}\n",
                self.sugg.len(),
                self.sugg.join("|")
            ));
        }
        for st in &self.plan.stmts {
            s.push_str(&format!(
                "stmt|{:?}|{}\n",
                st.risk,
                st.sql.replace('\n', " ")
            ));
        }
        for bl in &self.plan.blocked {
            s.push_str(&format!("blocked|{:?}|{}\n", bl.reason, bl.what));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TableEditView {
        let mut v = TableEditView::new(Dialect::Oracle, "APP", "ITEM", false);
        v.load(Table {
            schema: "APP".into(),
            name: "ITEM".into(),
            comment: String::new(),
            columns: vec![
                Column {
                    name: "ID".into(),
                    data_type: "NUMBER(10)".into(),
                    nullable: false,
                    ..Column::default()
                },
                Column {
                    name: "NAME".into(),
                    data_type: "VARCHAR2(50)".into(),
                    nullable: true,
                    ..Column::default()
                },
            ],
            keys: vec![nsql_ddl::Key {
                name: "ITEM_FK".into(),
                kind: KeyKind::Foreign,
                cols: vec!["NAME".into()],
                ref_table: "UNIT".into(),
                ..nsql_ddl::Key::default()
            }],
            indexes: vec![Index {
                name: "ITEM_IX1".into(),
                unique: false,
                cols: vec!["NAME".into()],
                origin: None,
            }],
        });
        v
    }

    #[test]
    fn edits_build_statements_and_revert_clears() {
        let mut v = sample();
        assert!(!v.is_dirty());
        assert!(!v.can_apply());
        assert!(v.add_row());
        assert!(v.set_cell(2, 0, "flag"));
        assert!(v.set_cell(2, 1, "CHAR(1)"));
        assert!(v.toggle(2, 2));
        assert!(v.is_dirty());
        assert!(v.can_apply());
        // 새 이름은 방언 접힘(Oracle 대문자) · 새 행은 맨 끝 = 자리 경고 없음.
        assert_eq!(
            v.plan().stmts[0].sql,
            "ALTER TABLE APP.ITEM ADD (FLAG CHAR(1) NOT NULL)"
        );
        assert!(v.plan().blocked.is_empty());
        v.revert();
        assert!(!v.is_dirty());
        assert_eq!(v.edited().columns.len(), 2);
    }

    #[test]
    fn column_rename_follows_into_keys_and_indexes() {
        let mut v = sample();
        assert!(v.set_cell(1, 0, "title"));
        assert_eq!(v.edited().keys[0].cols, ["TITLE"]);
        assert_eq!(v.edited().indexes[0].cols, ["TITLE"]);
        let sqls: Vec<&str> = v.plan().stmts.iter().map(|s| s.sql.as_str()).collect();
        assert_eq!(sqls, ["ALTER TABLE APP.ITEM RENAME COLUMN NAME TO TITLE"]);
    }

    #[test]
    fn server_read_fk_is_name_only_but_new_key_is_free() {
        let mut v = sample();
        v.set_section(Section::Keys);
        assert!(v.cell_editable(0, 0));
        assert!(!v.cell_editable(0, 2));
        assert!(!v.cell_editable(0, 3));
        assert!(!v.toggle(0, 1));
        assert!(v.add_row());
        // 기본 키가 없으니 새 제약 = PRIMARY KEY · 종류 돌리기 = 이름 꼬리도 따라간다.
        assert_eq!(v.edited().keys[1].name, "ITEM_PK");
        assert!(v.toggle(1, 1));
        assert_eq!(v.edited().keys[1].kind, KeyKind::Unique);
        assert_eq!(v.edited().keys[1].name, "ITEM_UK");
        assert!(v.toggle(1, 1));
        assert!(v.set_cell(1, 2, "id"));
        assert!(v.set_cell(1, 3, "app.unit(id) on delete cascade"));
        let k = &v.edited().keys[1];
        assert_eq!(
            (k.ref_table.as_str(), k.on_delete.as_str()),
            ("APP.UNIT", "CASCADE")
        );
        assert_eq!(k.ref_cols, ["ID"]);
        assert_eq!(
            v.plan().stmts[0].sql,
            "ALTER TABLE APP.ITEM ADD CONSTRAINT ITEM_FK FOREIGN KEY (ID) REFERENCES APP.UNIT (ID) ON DELETE CASCADE"
        );
    }

    #[test]
    fn read_only_blocks_every_edit() {
        let mut v = sample();
        v.set_read_only(Some("prod".into()));
        assert!(!v.add_row());
        assert!(!v.set_cell(0, 0, "X"));
        assert!(!v.toggle(0, 2));
        assert!(!v.remove_row(0));
        assert!(!v.set_table_name("X"));
        assert!(!v.can_apply());
    }

    #[test]
    fn new_table_needs_name_and_typed_columns() {
        let mut v = TableEditView::new(Dialect::Postgres, "public", "", true);
        assert!(!v.can_apply());
        assert!(v.set_table_name("Orders"));
        assert!(v.add_row());
        assert!(v.set_cell(0, 0, "Id"));
        assert!(!v.can_apply()); // 타입 없음
        assert!(v.set_cell(0, 1, "integer"));
        assert!(v.can_apply());
        assert_eq!(
            v.plan().stmts[0].sql,
            "CREATE TABLE public.orders (\n    id integer\n)"
        );
    }

    #[test]
    fn type_suggestions_filter_by_prefix() {
        let all = type_suggestions(Dialect::Oracle, "", true);
        assert!(all.contains(&"VARCHAR2(100)"));
        assert_eq!(
            type_suggestions(Dialect::Oracle, "var", false),
            ["VARCHAR2(100)"]
        );
        assert!(type_suggestions(Dialect::Oracle, "num", false).len() >= 3);
        assert!(type_suggestions(Dialect::Oracle, "", false).is_empty());
        assert!(type_suggestions(Dialect::Oracle, "date", false).is_empty()); // 똑같은 글 = 닫힘
        assert!(type_suggestions(Dialect::Postgres, "zzz", false).is_empty());
    }

    #[test]
    fn undo_redo_walk_edit_steps_and_reload_forgets() {
        let mut v = sample();
        assert!(!v.undo());
        assert!(v.set_cell(1, 0, "title"));
        assert!(v.add_row());
        assert!(v.toggle(0, 2));
        assert_eq!(v.plan().stmts.len(), 3 - 1 + 1); // 이름 · NULL 허용 · (빈 새 행은 문장이 되지만 적용 불가)
        assert!(v.undo()); // 체크 되돌림
        assert!(!v.edited().columns[0].nullable);
        assert!(v.undo()); // 행 추가 되돌림
        assert_eq!(v.edited().columns.len(), 2);
        assert!(v.undo()); // 이름 되돌림 — 딸린 제약·인덱스의 컬럼 이름도 함께 돌아온다
        assert_eq!(v.edited().columns[1].name, "NAME");
        assert_eq!(v.edited().indexes[0].cols, ["NAME"]);
        assert!(!v.is_dirty());
        assert!(!v.undo());
        assert!(v.redo());
        assert_eq!(v.edited().columns[1].name, "TITLE");
        // 새 편집은 다시 실행 기록을 버린다.
        assert!(v.set_cell(0, 4, "key"));
        assert!(!v.redo());
        // Revert도 한 단계 = 되돌릴 수 있다.
        v.revert();
        assert!(!v.is_dirty());
        assert!(v.undo());
        assert!(v.is_dirty());
        // 서버에서 다시 읽으면 기록을 비운다.
        let orig = v.orig().clone();
        v.load(orig);
        assert!(!v.undo());
        assert!(!v.redo());
    }

    #[test]
    fn rebase_adopts_server_side_additions_and_keeps_pending_edits() {
        let mut v = sample();
        assert!(v.set_cell(1, 0, "title")); // 아직 적용 안 한 편집
        let mut server = v.orig().clone();
        server.columns.push(Column {
            name: "EXTRA".into(),
            data_type: "VARCHAR2(10)".into(),
            nullable: true,
            ..Column::default()
        });
        server.indexes.push(Index {
            name: "ITEM_IX9".into(),
            unique: false,
            cols: vec!["EXTRA".into()],
            origin: None,
        });
        v.rebase(server);
        // 서버에 새로 생긴 컬럼·인덱스를 "지울 것"으로 잡지 않는다 · 남은 편집은 그대로.
        let sqls: Vec<&str> = v.plan().stmts.iter().map(|s| s.sql.as_str()).collect();
        assert_eq!(sqls, ["ALTER TABLE APP.ITEM RENAME COLUMN NAME TO TITLE"]);
        assert_eq!(v.edited().columns.len(), 3);
        assert_eq!(v.edited().indexes.len(), 2);
        // 사용자가 지운 것은 여전히 지울 것이다(들여오지 않는다).
        let mut v = sample();
        assert!(v.remove_row(1));
        let server = v.orig().clone();
        v.rebase(server);
        assert!(v
            .plan()
            .stmts
            .iter()
            .any(|s| s.sql == "ALTER TABLE APP.ITEM DROP COLUMN NAME"));
    }

    #[test]
    fn server_read_fk_is_editable_once_its_attributes_were_read() {
        let mut v = sample();
        v.set_key_lock(true, Vec::new());
        v.set_section(Section::Keys);
        assert!(v.cell_editable(0, 2));
        assert!(v.cell_editable(0, 3));
        assert!(v.set_cell(0, 3, "UNIT(ID) ON DELETE CASCADE"));
        // 바뀐 제약 = 지우고 다시 만든다(외래 키 삭제가 먼저 · 추가가 마지막).
        let sqls: Vec<&str> = v.plan().stmts.iter().map(|s| s.sql.as_str()).collect();
        assert_eq!(
            sqls,
            [
                "ALTER TABLE APP.ITEM DROP CONSTRAINT ITEM_FK",
                "ALTER TABLE APP.ITEM ADD CONSTRAINT ITEM_FK FOREIGN KEY (NAME) REFERENCES UNIT (ID) ON DELETE CASCADE"
            ]
        );
    }

    #[test]
    fn parse_fk_forms() {
        assert_eq!(
            parse_fk(Dialect::Oracle, "unit"),
            ("UNIT".to_string(), vec![], String::new(), String::new())
        );
        assert_eq!(
            parse_fk(Dialect::Mssql, "dbo.Unit(Id, Code) on delete set null"),
            (
                "dbo.Unit".to_string(),
                vec!["Id".to_string(), "Code".to_string()],
                "SET NULL".to_string(),
                String::new()
            )
        );
        // 두 규칙 · 순서 무관.
        let a = parse_fk(
            Dialect::Postgres,
            "unit(id) ON DELETE CASCADE ON UPDATE SET NULL",
        );
        let b = parse_fk(
            Dialect::Postgres,
            "unit(id) on update set null on delete cascade",
        );
        assert_eq!(a, b);
        assert_eq!((a.2.as_str(), a.3.as_str()), ("CASCADE", "SET NULL"));
    }

    #[test]
    fn locked_server_constraint_stays_name_only_even_when_attributes_were_read() {
        let mut v = sample();
        v.set_key_lock(true, vec!["ITEM_FK".to_string()]);
        v.set_section(Section::Keys);
        assert!(v.cell_editable(0, 0));
        assert!(!v.cell_editable(0, 2));
        assert!(!v.cell_editable(0, 3));
        assert!(!v.set_cell(0, 3, "UNIT(ID) ON DELETE CASCADE"));
        assert!(v.plan().stmts.is_empty());
        // 새로 더한 제약은 잠금과 무관.
        assert!(v.add_row());
        assert!(v.cell_editable(1, 2));
    }
}
