//! 결과 그리드 — 최소 가상화(보이는 행만 그린다) · **픽셀 단위 스크롤**(사용자 09-14) · 오버레이 스크롤바(필요할 때만 ·
//! 호버 두껍게 · 자동 숨김 — nexa-ctl `ScrollBars` 공용) · 컬럼 폭은 앞 200행 실측 · 메시지 모드.
//! `nexa-grid` 크레이트(U-3 · nexa-ui 21)가 오면 교체한다. 고정폭 층에서 그려진다.

use crate::toolicons;
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::{InputEvent, Key, ScrollBars};
use nsql_core::{fmt_bytes, fmt_dur, Dialect, ResultSet, Value};
use nsql_i18n::{t, Msg};

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
use nsql_io::{generate, guess_table, KeySpec};

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
            gutter_w: 0,
            col_order: Vec::new(),
            row_order: Vec::new(),
            sort_keys: Vec::new(),
            hdr_drag: None,
            hdr_resize: None,
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
            sc_copy: self.sc_copy.clone(),
            sc_all: self.sc_all.clone(),
            dialect: self.dialect,
            ..Grid::default()
        }
    }

    pub(crate) fn set_row_numbers(&mut self, on: bool) {
        self.row_numbers = on;
    }

    /// 복사할 때 null을 `NULL` 글자로(설정 `grid.copy_null` · 기본 끔 = 빈 칸 · 사용자 09-16).
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
        self.load = t.elapsed();
    }

    /// 실행 직전 호스트가 알려 주는 원본 문장 — SQL 복사의 테이블 이름 근거.
    pub(crate) fn set_source_sql(&mut self, sql: &str) {
        self.source_table = guess_table(sql);
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
        (self.bounds.h - self.header_h - self.row_h).max(0)
    }

    /// 콘텐츠 크기(스크롤 범위) — 헤더 + 전 행 · 컬럼 폭 합.
    fn content_size(&self) -> (i32, i32) {
        let w: i32 = self.col_w.iter().sum();
        let h = self.header_h + self.row_h * self.rows() as i32;
        (w, h)
    }

    fn max_scroll(&self) -> (i32, i32) {
        let (cw, ch) = self.content_size();
        (
            (cw - (self.bounds.w - self.gutter_w)).max(0),
            (ch - (self.bounds.h - self.row_h)).max(0),
        )
    }

    fn clamp(&mut self) {
        let (mx, my) = self.max_scroll();
        self.scroll_x = self.scroll_x.clamp(0, mx);
        self.scroll_y = self.scroll_y.clamp(0, my);
        if self.row_snap && self.row_h > 0 && self.scroll_y < my {
            self.scroll_y -= self.scroll_y % self.row_h;
        }
    }

    /// 페이드 타이머(스크롤바 · 호버 행) — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let a = self.bars.tick(now_ms);
        let b = self.hover.tick(now_ms);
        a || b
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
        let di = ((y - body.y + self.scroll_y) / self.row_h) as usize;
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
                b.h - self.header_h - self.row_h,
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
                        let w0 = self.col_w.get(ci).copied().unwrap_or(80);
                        self.hdr_resize = Some((ci, x, w0));
                    } else if let Some(pos) = self.header_pos_at(x) {
                        self.hdr_drag = Some((pos, x, x, false, shift));
                    }
                    return;
                }
                InputEvent::MouseMove { x, .. } if self.hdr_resize.is_some() => {
                    if let Some((ci, x0, w0)) = self.hdr_resize {
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
        self.row_h = dc.text_height() + pad;
        // 메시지 모드(결과 없음 · 오류 · PRINT)
        let Some(rs) = self.rs.as_ref() else {
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
                if y > b.y + b.h {
                    break;
                }
                dc.text(b.x + pad, y, b, m, th.text);
                y += self.row_h;
            }
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
                    w.min((420.0 * s) as i32) + pad * 2
                })
                .collect();
        }
        let header = Rect::new(b.x, b.y + 1, b.w, self.row_h);
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
        let footer = Rect::new(b.x, b.bottom() - self.row_h, b.w, self.row_h);
        let body = Rect::new(
            b.x,
            header.bottom(),
            b.w,
            (footer.y - header.bottom()).max(0),
        );
        let first = (self.scroll_y / self.row_h.max(1)) as usize;
        let sub = self.scroll_y % self.row_h.max(1);
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
        // 위치 표시(우상단 헤더 줄)
        let info = format!(
            "{}–{} / {} · load {} · render {} · ~{}",
            if rs.rows.is_empty() { 0 } else { first + 1 },
            last,
            rs.rows.len(),
            fmt_dur(self.load),
            fmt_dur(self.render),
            fmt_bytes(self.approx_bytes)
        );
        // 푸터 글자 = 상태줄 크기(사용자 09-16) · 세로 가운데.
        dc.select_font(FontSlot::Status, false);
        let iw = dc.text_width(&info);
        dc.fill_rect(footer, th.chrome_bg);
        dc.fill_rect(Rect::new(b.x, footer.y, b.w, 1), th.border);
        let iy = dc.text_center_y(footer.y, footer.h);
        dc.text(b.x + b.w - iw - pad, iy, footer, &info, th.text_dim);
        dc.select_font(FontSlot::Base, false);
        // 오버레이 스크롤바(필요할 때만 · 스크롤/호버 시 · 반투명) — 헤더 아래부터 푸터 위까지(데이터 영역만).
        let (cw, ch) = self.content_size();
        let ch = ch - self.header_h;
        let vp = Rect::new(
            b.x + self.gutter_w,
            b.y + self.header_h,
            b.w - self.gutter_w,
            b.h - self.header_h - self.row_h,
        );
        self.bars.paint(
            dc,
            th,
            vp,
            cw.max(vp.w),
            ch.max(vp.h),
            self.scroll_x,
            self.scroll_y,
            s,
        );
    }

    /// 우클릭 메뉴 — 호스트가 **UI 글꼴 최상위 패스**에서 그린다(그리드 패스의 고정폭 글꼴로 그리면 다른 메뉴와 글꼴이 달랐다 · 09-16).
    pub(crate) fn paint_menu(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.menu.paint(dc, th);
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
