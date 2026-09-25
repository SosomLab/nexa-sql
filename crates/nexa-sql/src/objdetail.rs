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

pub(crate) enum DetailAction {
    /// 클립보드로.
    Copy(String),
    /// 축소 상태가 바뀌었다(호스트 = 설정 저장 + 배치).
    Collapsed(bool),
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
}

impl DetailPanel {
    pub(crate) fn new() -> Self {
        let mut tb = TextBox::new("").with_multiline();
        tb.set_read_only(true);
        tb.set_minimap(false);
        tb.set_gutter_marks(false);
        tb.set_line_numbers(false);
        tb.set_wrap(false);
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
        }
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

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let mut inv = Invalidations::default();
        let hh = Self::head_h(scale);
        let body = Rect::new(b.x, b.y + hh, b.w, (b.h - hh).max(0));
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

    /// 선택 대상이 바뀌었다 — 객체는 호스트가 상세를 청하고(로딩 표시) · 컬럼·잎·스키마는 바로 속성만.
    pub(crate) fn set_target(&mut self, t: Option<DetailTarget>) {
        self.target = t;
        self.sections.clear();
        self.error = None;
        self.loading = matches!(self.target, Some(DetailTarget::Object(_)));
        self.render();
    }

    /// 스레드가 준 섹션(같은 객체일 때만 받는다).
    pub(crate) fn set_sections(
        &mut self,
        owner: &nsql_catalog::ObjectInfo,
        r: Result<Vec<DetailSection>, String>,
    ) {
        let same = matches!(&self.target, Some(DetailTarget::Object(o)) if o.schema == owner.schema && o.name == owner.name && o.kind == owner.kind);
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

    /// 설명(Description) = 객체 = 부가(없으면 상태) · 컬럼 = 타입 + NULL 여부 · 잎 = 부가 · 없으면 빈 글.
    fn description(&self) -> String {
        match &self.target {
            Some(DetailTarget::Object(o)) => {
                if !o.extra.is_empty() {
                    o.extra.clone()
                } else {
                    o.status.clone()
                }
            }
            Some(DetailTarget::Column { col, .. }) => format!(
                "{} {}",
                col.data_type,
                if col.nullable { "NULL" } else { "NOT NULL" }
            ),
            Some(DetailTarget::Item { item, .. }) => item.detail.clone(),
            _ => String::new(),
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
        })
        .to_string()
    }

    fn section_label(id: SectionId) -> String {
        match id {
            SectionId::Properties => t(Msg::DetSecProperties).to_string(),
            SectionId::Columns => t(Msg::DetSecColumns).to_string(),
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
            Some(DetailTarget::Object(_)) => {
                if self.loading {
                    out.push_str(t(Msg::DetLoading));
                } else if let Some(e) = &self.error {
                    out.push_str(e);
                }
                for sec in &self.sections {
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
            Some(DetailTarget::Column { owner, col }) => {
                let heads = vec![
                    Self::header_label(HeaderId::Property),
                    Self::header_label(HeaderId::Value),
                ];
                let rows = vec![
                    vec!["table".into(), format!("{}.{}", owner.schema, owner.name)],
                    vec!["name".into(), col.name.clone()],
                    vec!["type".into(), col.data_type.clone()],
                    vec![
                        "nullable".into(),
                        if col.nullable {
                            "NULL".into()
                        } else {
                            "NOT NULL".into()
                        },
                    ],
                    vec!["default".into(), col.default.clone()],
                    vec!["position".into(), col.position.to_string()],
                ];
                out.push_str(&format!("== {} ==\n", t(Msg::DetSecProperties)));
                out.push_str(&nsql_catalog::render_table(&heads, &rows));
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
                out.push_str(&format!("== {} ==\n{}\n", t(Msg::DetSecProperties), s));
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
        match ev {
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
            let name = self.name_label();
            let nw = dc.text_width(&name);
            let clip = Rect::new(x, head.y, (right - x).max(0), hh);
            dc.text(x, ty, clip, &name, th.text);
            x += nw + px(10.0);
            if x < right {
                let desc = self.description();
                let d = if self.loading && desc.is_empty() {
                    t(Msg::DetLoading).to_string()
                } else {
                    desc
                };
                dc.text(
                    x,
                    ty,
                    Rect::new(x, head.y, (right - x).max(0), hh),
                    &d,
                    th.text_dim,
                );
            }
        }
        // ▾(펼침 상태 = 축소) / ▴(축소 상태 = 확장).
        let glyph = if self.collapsed { "▴" } else { "▾" };
        if self.hover_toggle {
            dc.fill_round_rect(self.toggle, px(3.0), th.panel_bg_alt);
        }
        let gw = dc.text_width(glyph);
        let gy = dc.text_center_y(self.toggle.y, self.toggle.h);
        dc.text(
            self.toggle.x + (self.toggle.w - gw) / 2,
            gy,
            self.toggle,
            glyph,
            th.text,
        );
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
