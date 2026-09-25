//! ★ 객체 상세 패널(docs/86 · T-223 · 사용자 09-25): 객체 탐색기 **아래 독립 영역**(탐색기 스크롤과 겹치지 않음) — View ▸ Object Details.
//! 머리 줄(UI 글꼴) = 종류 · 이름 · 설명(흐리게) · ▾/▴(축소·확장) · 복사 버튼 · 본문(고정폭 · 읽기 전용 텍스트박스 = 선택·복사·상하/좌우 스크롤) =
//! 유형별 섹션(속성 · 컬럼 · 제약/인덱스/트리거/인자 … · 소스/DDL · `nsql_catalog::object_details`). 축소 = 머리 줄 하나(설명 + 복사).
//! 복사 = 설명 · **Shift+클릭** = `종류 - 이름 - 설명` · 복사됨 효과 = `CopyBtn`(1 s).

use crate::copybtn::{CopyBtn, Look};
use crate::explorer::{sub_msg, DetailTarget};
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, TextBox, Widget};
use nsql_catalog::{DetailSection, HeaderId, SectionId};
use nsql_i18n::{t, Msg};
use std::time::Instant;

/// 머리 줄 높이(논리 px).
const HEAD_H: f32 = 26.0;

/// 테이블 단위 코멘트 캐시: `스키마.테이블` → (테이블 코멘트, [(컬럼, 코멘트 그대로 · NULL = None)]).
type CommentCache =
    std::collections::HashMap<String, (Option<String>, Vec<(String, Option<String>)>)>;

pub(crate) enum DetailAction {
    /// 클립보드로.
    Copy(String),
    /// 축소 상태가 바뀌었다(호스트 = 설정 저장 + 배치).
    Collapsed(bool),
    /// 머리 줄 더블클릭 = 소스/DDL을 편집기 새 탭으로(제목, 글).
    OpenInEditor(String, String),
}

pub(crate) struct DetailPanel {
    bounds: Rect,
    scale: f32,
    visible: bool,
    collapsed: bool,
    focused: bool,
    tb: TextBox,
    copy: CopyBtn,
    toggle: Rect,
    target: Option<DetailTarget>,
    sections: Vec<DetailSection>,
    loading: bool,
    error: Option<String>,
    actions: Vec<DetailAction>,
    hover_toggle: bool,
    /// 스키마 대상의 종류별 객체 수(호스트가 메타에서 채움 · 서버 왕복 0).
    schema_counts: Vec<(nsql_catalog::ObjectKind, usize)>,
    /// ★ 테이블 단위 코멘트 캐시(`스키마.테이블` → (테이블 코멘트, 컬럼 코멘트 **그대로** · NULL = None)) — 한 번 읽으면 그 테이블의 모든 컬럼은 즉시(사용자 09-25).
    comments: CommentCache,
    last_head_click: Option<Instant>,
}

impl DetailPanel {
    pub(crate) fn new() -> Self {
        let mut tb = TextBox::new("").with_multiline();
        Self::tune_box(&mut tb);
        DetailPanel {
            bounds: Rect::default(),
            scale: 1.0,
            visible: false,
            collapsed: false,
            focused: false,
            tb,
            copy: CopyBtn::new(),
            toggle: Rect::default(),
            target: None,
            sections: Vec::new(),
            loading: false,
            error: None,
            actions: Vec::new(),
            hover_toggle: false,
            schema_counts: Vec::new(),
            comments: CommentCache::new(),
            last_head_click: None,
        }
    }

    fn owner_key(o: &nsql_catalog::ObjectInfo) -> String {
        format!("{}.{}", o.schema, o.name)
    }

    /// 이 테이블의 코멘트를 이미 아는가(캐시).
    pub(crate) fn knows_comments(&self, owner: &nsql_catalog::ObjectInfo) -> bool {
        self.comments.contains_key(&Self::owner_key(owner))
    }

    /// 코멘트 도착(NULL·공백도 그대로 기억) — 다시 그린다.
    pub(crate) fn set_comments(
        &mut self,
        owner: &nsql_catalog::ObjectInfo,
        table: Option<String>,
        cols: Vec<(String, Option<String>)>,
    ) {
        self.comments.insert(Self::owner_key(owner), (table, cols));
        self.render();
    }

    /// 대상 컬럼의 코멘트: None = 아직 모름 · Some(None) = NULL(알고 있음) · Some(Some(값)) = 조회된 값 그대로(공백 포함).
    fn column_comment(&self) -> Option<Option<String>> {
        let Some(DetailTarget::Column { owner, col }) = &self.target else {
            return None;
        };
        let (_, cols) = self.comments.get(&Self::owner_key(owner))?;
        Some(
            cols.iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(&col.name))
                .and_then(|(_, c)| c.clone()),
        )
    }

    /// 대상 객체의 코멘트 상태: None = 아직 모름 · Some(None) = NULL/코멘트 없는 종류 · Some(Some(값)) = 그대로.
    fn table_comment(&self) -> Option<Option<String>> {
        let Some(DetailTarget::Object(o)) = &self.target else {
            return None;
        };
        if let Some((tc, _)) = self.comments.get(&Self::owner_key(o)) {
            return Some(tc.clone());
        }
        if let Some(c) = self.comment_row() {
            return Some(Some(c));
        }
        // 관계가 아닌 종류(프로시저·시퀀스 …)는 코멘트 자체가 없다 = NULL.
        (!o.kind.is_relation()).then_some(None)
    }

    /// 스키마 대상 = 종류별 객체 수(86 §3).
    pub(crate) fn set_schema_counts(&mut self, counts: Vec<(nsql_catalog::ObjectKind, usize)>) {
        self.schema_counts = counts;
        self.render();
    }

    /// 소스/DDL 섹션 글(편집기로 열기용).
    fn code_text(&self) -> Option<(String, String)> {
        let sec = self
            .sections
            .iter()
            .find(|s| matches!(s.id, SectionId::Source | SectionId::Ddl))?;
        let text = sec.text.clone()?;
        let title = match &self.target {
            Some(DetailTarget::Object(o)) => format!("{}.{}", o.schema, o.name),
            _ => "detail".into(),
        };
        Some((title, text))
    }

    fn tune_box(tb: &mut TextBox) {
        tb.set_read_only(true);
        // 우클릭 편집 메뉴는 팝업 층(`paint_popup`)에서만 — 본문(고정폭 패스)이 아니라 UI 글꼴로 · 다른 층이 덮지 않게(09-26).
        tb.set_popup_deferred(true);
        tb.set_minimap(false);
        tb.set_gutter_marks(false);
        tb.set_line_numbers(false);
        tb.set_wrap(false);
    }

    /// 편집기 탭과 같은 설정의 상자로 바꾼다(`Editors::preview_box` · 글꼴 지표·줄 간격이 편집기 글꼴과 맞아야 겹치지 않는다 · 09-25 캡처).
    pub(crate) fn set_box(&mut self, mut tb: TextBox) {
        Self::tune_box(&mut tb);
        self.tb = tb;
        let (b, s) = (self.bounds, self.scale);
        if b.w > 0 {
            self.set_bounds(b, s);
        }
        self.render();
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
    }
    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }
    pub(crate) fn set_collapsed(&mut self, on: bool) {
        self.collapsed = on;
    }
    pub(crate) fn is_collapsed(&self) -> bool {
        self.collapsed
    }
    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }
    /// 호스트가 배치에 쓰는 높이(논리 px): 축소 = 머리 줄만.
    pub(crate) fn head_h(scale: f32) -> i32 {
        (HEAD_H * scale).round() as i32
    }
    /// 확장 상태의 **최소 높이**(물리 px) = 머리 줄 + 본문 4줄 + 상자 여백(사용자 09-26 "헤더 및 하위 4줄").
    pub(crate) fn min_h(&self) -> i32 {
        Self::head_h(self.scale) + self.tb.line_h() * 4 + (8.0 * self.scale).round() as i32
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let mut inv = Invalidations::default();
        let hh = Self::head_h(scale);
        let body = Rect::new(b.x, b.y + hh, b.w, (b.h - hh).max(0));
        // ★ 배율(09-25 캡처 "폰트 겹침"): 텍스트박스 줄 높이 = `s(20)` — 배율을 안 넘기면 레티나에서 절반이라 글자가 겹친다(편집기 탭은 `make_box`가 넘김).
        self.tb.set_scale(scale);
        self.tb.set_bounds(body, &mut inv);
        let px = |v: f32| (v * scale).round() as i32;
        let bh = hh - px(6.0);
        // 오른쪽 끝 = 복사 · 그 왼쪽 = ▾/▴.
        let cx = b.right() - px(6.0) - bh;
        self.copy.set_rect(Rect::new(cx, b.y + px(3.0), bh, bh));
        self.toggle = Rect::new(cx - px(4.0) - bh, b.y + px(3.0), bh, bh);
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        self.tb.set_focused(on && !self.collapsed);
    }

    pub(crate) fn take_actions(&mut self) -> Vec<DetailAction> {
        std::mem::take(&mut self.actions)
    }

    /// 우클릭 편집 메뉴(복사·전체 선택 · 읽기 전용이라 잘라내기·붙여넣기는 비활성 = nexa-ctl `EditMenuCaps.read_only`)가 열려 있는가.
    pub(crate) fn menu_open(&self) -> bool {
        self.tb.popup_open()
    }
    pub(crate) fn menu_bounds(&self) -> Rect {
        self.tb.popup_bounds()
    }
    pub(crate) fn close_menu(&mut self) {
        self.tb.close_menu();
    }
    /// 팝업 층(호스트가 모든 층을 그린 뒤 · UI 글꼴).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.visible && !self.collapsed {
            self.tb.paint_popup(dc, th);
        }
    }

    /// 메뉴가 남긴 요청 — 복사만(선택 없으면 전체 본문).
    fn apply_edit_ctx(&mut self) {
        if let Some(a) = self.tb.take_edit_ctx() {
            if matches!(a, nexa_ctl::EditCtxAction::Copy) {
                let text = self.tb.copy_selection().unwrap_or_else(|| self.tb.text());
                self.actions.push(DetailAction::Copy(text));
            }
        }
    }

    /// 선택 대상이 바뀌었다 — 객체는 호스트가 상세를 청하고(로딩 표시) · 컬럼·잎·스키마는 바로 속성만.
    pub(crate) fn set_target(&mut self, t: Option<DetailTarget>) {
        self.target = t;
        self.sections.clear();
        self.error = None;
        // 서버 몫이 오기 전에도 있는 것(이름·상태·컬럼 정보)은 바로 보인다 — "읽는 중" 문구 없음(사용자 09-25).
        self.loading = matches!(
            self.target,
            Some(DetailTarget::Object(_) | DetailTarget::Column { .. })
        );
        self.render();
    }

    /// 스레드가 준 섹션(같은 객체일 때만 받는다).
    pub(crate) fn set_sections(
        &mut self,
        owner: &nsql_catalog::ObjectInfo,
        col: Option<&nsql_catalog::ColumnInfo>,
        r: Result<Vec<DetailSection>, String>,
    ) {
        let same = match (&self.target, col) {
            (Some(DetailTarget::Object(o)), None) => {
                o.schema == owner.schema && o.name == owner.name && o.kind == owner.kind
            }
            (Some(DetailTarget::Column { owner: ow, col: c }), Some(cc)) => {
                ow.schema == owner.schema && ow.name == owner.name && c.name == cc.name
            }
            _ => false,
        };
        if !same {
            return;
        }
        self.loading = false;
        match r {
            Ok(s) => self.sections = s,
            Err(e) => self.error = Some(e),
        }
        self.render();
    }

    /// 종류 라벨(폴더 이름 단수형 대신 종류 코드 대문자 · 짧게).
    fn kind_label(&self) -> String {
        match &self.target {
            Some(DetailTarget::Object(o)) => o.kind.code().to_uppercase(),
            Some(DetailTarget::Column { .. }) => "COLUMN".into(),
            Some(DetailTarget::Item { sub, .. }) => {
                t(sub_msg(*sub)).trim_end_matches('s').to_uppercase()
            }
            Some(DetailTarget::Schema(_)) => "SCHEMA".into(),
            None => String::new(),
        }
    }

    fn name_label(&self) -> String {
        match &self.target {
            Some(DetailTarget::Object(o)) => format!("{}.{}", o.schema, o.name),
            Some(DetailTarget::Column { owner, col }) => {
                format!("{}.{}.{}", owner.schema, owner.name, col.name)
            }
            Some(DetailTarget::Item { owner, item, .. }) => {
                format!("{}.{}.{}", owner.schema, owner.name, item.name)
            }
            Some(DetailTarget::Schema(s)) => s.clone(),
            None => String::new(),
        }
    }

    /// 속성 표의 `comment` 값(상세가 왔을 때).
    fn comment_row(&self) -> Option<String> {
        self.sections
            .iter()
            .find(|s| s.id == SectionId::Properties)
            .and_then(|s| {
                s.rows
                    .iter()
                    .find(|r| r.first().is_some_and(|k| k == "comment"))
                    .and_then(|r| r.get(1).cloned())
            })
            .filter(|c| !c.trim().is_empty())
    }

    /// 설명(Description · 사용자 09-25) = 테이블·객체 = 코멘트, 없으면 이름 · 스키마 = 이름 · 컬럼 = 코멘트, 없으면 컬럼 이름 · 잎 = 부가, 없으면 이름.
    fn description(&self) -> String {
        match &self.target {
            // 객체 = 코멘트 그대로(공백 포함) · NULL·아직 모름 = 빈 글(머리 줄은 NULL을 흐리게 · 사용자 09-26 "없으면 NULL 혹은 공백").
            Some(DetailTarget::Object(_)) => self.table_comment().flatten().unwrap_or_default(),
            // 컬럼 = 조회된 값 그대로(공백 포함) · NULL·아직 모름 = 빈 글(머리 줄은 NULL을 흐리게).
            Some(DetailTarget::Column { .. }) => {
                self.column_comment().flatten().unwrap_or_default()
            }
            Some(DetailTarget::Item { item, .. }) => {
                if item.detail.is_empty() {
                    item.name.clone()
                } else {
                    item.detail.clone()
                }
            }
            Some(DetailTarget::Schema(s)) => s.clone(),
            None => String::new(),
        }
    }

    /// 복사 글: 기본 = 설명(없으면 이름) · `full` = `종류 - 이름 - 설명`.
    pub(crate) fn copy_text(&self, full: bool) -> String {
        let desc = self.description();
        if full {
            format!("{} - {} - {}", self.kind_label(), self.name_label(), desc)
        } else if desc.is_empty() {
            self.name_label()
        } else {
            desc
        }
    }

    /// 본문 선택 글(호스트 `edit.copy`).
    pub(crate) fn copy_selection(&self) -> Option<String> {
        if self.collapsed {
            return None;
        }
        self.tb.copy_selection()
    }

    /// 본문 전체 글(자체 시험 `details.dump`).
    pub(crate) fn text(&self) -> String {
        self.tb.text()
    }

    fn header_label(h: HeaderId) -> String {
        t(match h {
            HeaderId::Property => Msg::DetHdrProperty,
            HeaderId::Value => Msg::DetHdrValue,
            HeaderId::Num => Msg::DetHdrNum,
            HeaderId::Name => Msg::DetHdrName,
            HeaderId::Type => Msg::DetHdrType,
            HeaderId::Nullable => Msg::DetHdrNullable,
            HeaderId::Default => Msg::DetHdrDefault,
            HeaderId::Detail => Msg::DetHdrDetail,
            HeaderId::Status => Msg::DetHdrStatus,
            HeaderId::Comment => Msg::DetHdrComment,
        })
        .to_string()
    }

    fn section_label(id: SectionId) -> String {
        match id {
            SectionId::Properties => t(Msg::DetSecProperties).to_string(),
            SectionId::Columns => t(Msg::DetSecColumns).to_string(),
            SectionId::PrimaryKey => t(Msg::DetSecPrimaryKey).to_string(),
            SectionId::Indexes => t(sub_msg(nsql_catalog::SubKind::Indexes)).to_string(),
            SectionId::Sub(sub) => t(sub_msg(sub)).to_string(),
            SectionId::Source => t(Msg::DetSecSource).to_string(),
            SectionId::Ddl => t(Msg::DetSecDdl).to_string(),
        }
    }

    /// 섹션 → 본문 글(고정폭 표 · 글).
    fn render(&mut self) {
        let mut out = String::new();
        match &self.target {
            None => out.push_str(t(Msg::DetNoSelection)),
            Some(DetailTarget::Object(o)) | Some(DetailTarget::Column { owner: o, .. }) => {
                if let Some(e) = &self.error {
                    out.push_str(e);
                    out.push('\n');
                }
                let secs: Vec<DetailSection> = if self.sections.is_empty() {
                    // 서버 몫 전 = 탐색기가 이미 아는 것(왕복 0 · 사용자 09-25 "로딩 메시지를 볼 일은 거의 없다").
                    let rows: Vec<Vec<String>> = match &self.target {
                        Some(DetailTarget::Column { col, .. }) => {
                            let mut r = vec![vec!["name".into(), col.name.clone()]];
                            match self.column_comment() {
                                Some(Some(c)) => r.push(vec!["comment".into(), c]),
                                Some(None) => r.push(vec!["comment".into(), "NULL".into()]),
                                None => {}
                            }
                            r.push(vec!["type".into(), col.data_type.clone()]);
                            r.push(vec![
                                "nullable".into(),
                                if col.nullable {
                                    "NULL".into()
                                } else {
                                    "NOT NULL".into()
                                },
                            ]);
                            r.push(vec!["default".into(), col.default.clone()]);
                            r.push(vec!["table".into(), format!("{}.{}", o.schema, o.name)]);
                            r
                        }
                        _ => {
                            let mut r =
                                vec![vec!["name".into(), format!("{}.{}", o.schema, o.name)]];
                            if !o.status.is_empty() {
                                r.push(vec!["status".into(), o.status.clone()]);
                            }
                            if !o.modified.is_empty() {
                                r.push(vec!["modified".into(), o.modified.clone()]);
                            }
                            if !o.extra.is_empty() {
                                r.push(vec!["detail".into(), o.extra.clone()]);
                            }
                            r
                        }
                    };
                    vec![DetailSection {
                        id: SectionId::Properties,
                        headers: vec![HeaderId::Property, HeaderId::Value],
                        rows,
                        text: None,
                    }]
                } else {
                    self.sections.clone()
                };
                for sec in &secs {
                    let n = if sec.text.is_some() {
                        String::new()
                    } else {
                        format!(" ({})", sec.rows.len())
                    };
                    out.push_str(&format!("== {}{n} ==\n", Self::section_label(sec.id)));
                    match &sec.text {
                        Some(tx) => {
                            out.push_str(tx);
                            if !tx.ends_with('\n') {
                                out.push('\n');
                            }
                        }
                        None => {
                            let heads: Vec<String> =
                                sec.headers.iter().map(|h| Self::header_label(*h)).collect();
                            out.push_str(&nsql_catalog::render_table(&heads, &sec.rows));
                        }
                    }
                    out.push('\n');
                }
            }
            Some(DetailTarget::Item { owner, sub, item }) => {
                let heads = vec![
                    Self::header_label(HeaderId::Property),
                    Self::header_label(HeaderId::Value),
                ];
                let rows = vec![
                    vec!["owner".into(), format!("{}.{}", owner.schema, owner.name)],
                    vec!["kind".into(), t(sub_msg(*sub)).to_string()],
                    vec!["name".into(), item.name.clone()],
                    vec!["detail".into(), item.detail.clone()],
                    vec!["status".into(), item.status.clone()],
                ];
                out.push_str(&format!("== {} ==\n", t(Msg::DetSecProperties)));
                out.push_str(&nsql_catalog::render_table(&heads, &rows));
            }
            Some(DetailTarget::Schema(s)) => {
                out.push_str(&format!("== {} ==\n{}\n\n", t(Msg::DetSecProperties), s));
                if !self.schema_counts.is_empty() {
                    let heads = vec![
                        Self::header_label(HeaderId::Type),
                        t(Msg::DetHdrCount).to_string(),
                    ];
                    let rows: Vec<Vec<String>> = self
                        .schema_counts
                        .iter()
                        .map(|(k, n)| vec![k.folder().to_string(), n.to_string()])
                        .collect();
                    let total: usize = self.schema_counts.iter().map(|(_, n)| n).sum();
                    out.push_str(&format!("== {} ({total}) ==\n", t(Msg::DetHdrCount)));
                    out.push_str(&nsql_catalog::render_table(&heads, &rows));
                }
            }
        }
        self.tb.set_text(&out);
    }

    /// 사건 — 안에서 처리했으면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent, now: Instant) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        // ★ 편집 메뉴가 열려 있으면 마우스는 전부 메뉴(텍스트박스)로 — 바깥 클릭은 호스트가 닫고 흘려 보낸다(09-26).
        if self.tb.popup_open()
            && matches!(
                ev,
                InputEvent::MouseMove { .. }
                    | InputEvent::MouseDown { .. }
                    | InputEvent::MouseUp { .. }
                    | InputEvent::RightDown { .. }
                    | InputEvent::Wheel { .. }
                    | InputEvent::HWheel { .. }
                    | InputEvent::Key { .. }
            )
        {
            self.tb.on_event(ev, &mut inv);
            self.apply_edit_ctx();
            return true;
        }
        match ev {
            InputEvent::RightDown { x, y } => {
                let p = Point { x: *x, y: *y };
                if self.collapsed || !self.tb.bounds().contains(p) {
                    return false;
                }
                self.tb.set_focused(true);
                self.tb.on_event(ev, &mut inv);
                true
            }
            InputEvent::MouseMove { x, y } => {
                let p = Point { x: *x, y: *y };
                let mut changed = self.copy.set_hover(self.copy.hit(p));
                let ht = self.toggle.contains(p);
                if ht != self.hover_toggle {
                    self.hover_toggle = ht;
                    changed = true;
                }
                if !self.collapsed && self.tb.bounds().contains(p) {
                    self.tb.on_event(ev, &mut inv);
                }
                changed
            }
            InputEvent::MouseDown { x, y, shift, .. } => {
                let p = Point { x: *x, y: *y };
                if !self.bounds.contains(p) {
                    return false;
                }
                if self.copy.hit(p) {
                    self.copy.press(now);
                    self.actions
                        .push(DetailAction::Copy(self.copy_text(*shift)));
                    return true;
                }
                if self.toggle.contains(p) {
                    self.collapsed = !self.collapsed;
                    self.tb.set_focused(self.focused && !self.collapsed);
                    self.actions.push(DetailAction::Collapsed(self.collapsed));
                    return true;
                }
                let hh = Self::head_h(self.scale);
                if p.y < self.bounds.y + hh {
                    // 머리 줄 더블클릭(400 ms) = 소스/DDL을 편집기 새 탭으로(86 §8).
                    let dbl = self
                        .last_head_click
                        .is_some_and(|t0| now.duration_since(t0).as_millis() < 400);
                    self.last_head_click = Some(now);
                    if dbl {
                        if let Some((title, text)) = self.code_text() {
                            self.actions.push(DetailAction::OpenInEditor(title, text));
                        }
                    }
                    return true;
                }
                if !self.collapsed && self.tb.bounds().contains(p) {
                    self.tb.set_focused(true);
                    self.tb.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::MouseUp { .. } | InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => {
                if !self.collapsed {
                    self.tb.on_event(ev, &mut inv);
                }
                true
            }
            // 키 = 읽기 전용 텍스트박스(이동·선택 · 편집 키는 무시) · ⌘/Ctrl+C는 호스트의 `edit.copy`가 `copy_selection`으로.
            InputEvent::Key { .. } if self.focused && !self.collapsed => {
                self.tb.on_event(ev, &mut inv);
                true
            }
            InputEvent::Char { .. } | InputEvent::SelectAll if self.focused && !self.collapsed => {
                self.tb.on_event(ev, &mut inv);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn is_animating(&self, now: Instant) -> bool {
        self.copy.next_tick(now).is_some()
    }

    /// 머리 줄(UI 글꼴 층).
    pub(crate) fn paint_header(&self, dc: &mut dyn DrawCtx, th: &Theme, now: Instant) {
        if !self.visible || self.bounds.w <= 0 {
            return;
        }
        let b = self.bounds;
        let s = self.scale;
        let px = |v: f32| (v * s).round() as i32;
        let hh = Self::head_h(s);
        let head = Rect::new(b.x, b.y, b.w, hh);
        dc.fill_rect(head, th.panel_bg);
        dc.fill_rect(Rect::new(b.x, b.y, b.w, 1), th.border);
        let ty = dc.text_center_y(head.y, head.h);
        let mut x = b.x + px(8.0);
        let right = self.toggle.x - px(6.0);
        if self.target.is_none() {
            dc.text(
                x,
                ty,
                Rect::new(x, head.y, (right - x).max(0), hh),
                t(Msg::DetNoSelection),
                th.text_dim,
            );
        } else {
            // 종류 칩.
            let kind = self.kind_label();
            if !kind.is_empty() {
                let cw = dc.text_width(&kind) + px(10.0);
                let chip = Rect::new(x, head.y + px(5.0), cw, hh - px(10.0));
                dc.fill_round_rect(chip, px(3.0), th.accent);
                let cy = dc.text_center_y(chip.y, chip.h);
                dc.text(chip.x + px(5.0), cy, chip, &kind, th.window_bg);
                x += cw + px(8.0);
            }
            // 종류 칩 옆 = **설명만**(사용자 09-25) · 컬럼 코멘트가 NULL임을 알면 흐린 회색 NULL · 공백은 공백 그대로.
            let desc = self.description();
            let clip = Rect::new(x, head.y, (right - x).max(0), hh);
            let known_null = matches!(self.column_comment(), Some(None))
                || matches!(self.table_comment(), Some(None));
            if known_null {
                dc.text(x, ty, clip, "NULL", th.text_dim);
            } else {
                dc.text(x, ty, clip, &desc, th.text);
            }
        }
        // ▾(펼침 상태 = 축소) / ▴(축소 상태 = 확장) — 글꼴 글리프 대신 도형(3-OS 동일).
        if self.hover_toggle {
            dc.fill_round_rect(self.toggle, px(3.0), th.panel_bg_alt);
        }
        // 채운 삼각형(▼ = 펼침 상태에서 축소 · ▲ = 축소 상태에서 확장 · 사용자 09-25): 1px 가로 띠(DrawCtx에 삼각형 채움이 없다).
        let tw = px(9.0).max(3);
        let thh = px(5.0).max(2);
        let cx = self.toggle.x + self.toggle.w / 2;
        let cy = self.toggle.y + self.toggle.h / 2;
        let top = cy - thh / 2;
        for row in 0..thh {
            let k = row as f32 / (thh - 1).max(1) as f32; // 0 = 위 · 1 = 아래
            let frac = if self.collapsed { k } else { 1.0 - k };
            let w = ((tw as f32) * frac).round().max(1.0) as i32;
            dc.fill_rect(Rect::new(cx - w / 2, top + row, w, 1), th.text);
        }
        let alpha = match self.copy.look(now) {
            Look::Idle => 0.8,
            _ => 1.0,
        };
        self.copy.paint(dc, th, alpha, s, now);
    }

    /// 본문(고정폭 층) — 축소면 그리지 않는다.
    pub(crate) fn paint_body(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible || self.collapsed || self.bounds.w <= 0 {
            return;
        }
        let hh = Self::head_h(self.scale);
        let body = Rect::new(
            self.bounds.x,
            self.bounds.y + hh,
            self.bounds.w,
            (self.bounds.h - hh).max(0),
        );
        if body.h <= 0 {
            return;
        }
        dc.fill_rect(body, th.field_bg);
        self.tb.paint(dc, th);
    }
}
