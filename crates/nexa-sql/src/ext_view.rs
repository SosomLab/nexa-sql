//! 확장 상세 **뷰 탭**(VS Code식 · 09-19 · 사용자 09-30 "읽기 전용이되 선택·복사·단축키·스크롤·우클릭 메뉴") — 편집기 자리에
//! 그리는 읽을거리 화면. 머리(이름 · id·버전·종류·상태 · 한 줄 설명 · 동작 버튼)는 직접 그리고, **본문(세부 정보 + 설명)은
//! 읽기 전용 `TextBox`** 하나다 — 드래그 선택 · 복사(⌘/Ctrl+C · 우클릭 메뉴 · 선택 없으면 전체) · 전체 선택 · 휠/키 스크롤 ·
//! 우클릭 편집 메뉴(복사·전체 선택만 활성 · 쓰기 항목은 nexa-ctl `EditMenuCaps.read_only`로 비활성)가 상자에서 그대로 온다.
//! 글 = UI 글꼴(호스트가 `RasterCtx`로 준다) · 줄 바꿈 = 상자 폭(`set_wrap`).
//!
//! ★ 버전·업데이트(사용자 09-30 "버전 확인이 어렵고 업데이트 대상인지 표시가 안 돼"): 부제 `id · v설치 · 종류 · 상태`에
//! 카탈로그가 더 새 버전을 알면 강조색 **"업데이트 가능: vN"** 줄 + 첫 버튼 **"업데이트 → vN"**(강조) · 같은 버전이면 "최신".

use crate::ext_panel::ExtRow;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, TextBox, Widget};
use nsql_i18n::{t, tf, Msg};

/// 상세 한 벌(호스트가 만든다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExtDetail {
    pub row: ExtRow,
    /// (라벨, 값) — 값이 빈 줄은 호스트가 뺀다.
    pub fields: Vec<(String, String)>,
    pub description: String,
}

impl ExtDetail {
    /// 본문 상자에 넣을 글(세부 정보 표 + 설명 문단) — 순수 함수(시험).
    pub(crate) fn body_text(&self) -> String {
        let mut s = String::new();
        if !self.fields.is_empty() {
            s.push_str(t(Msg::ExtViewDetails));
            s.push('\n');
            let w = self
                .fields
                .iter()
                .map(|(l, _)| l.chars().count())
                .max()
                .unwrap_or(0);
            for (label, value) in &self.fields {
                let pad = w.saturating_sub(label.chars().count());
                s.push_str("  ");
                s.push_str(label);
                s.push_str(&" ".repeat(pad));
                s.push_str("   ");
                s.push_str(value);
                s.push('\n');
            }
        }
        if !self.description.is_empty() {
            if !s.is_empty() {
                s.push('\n');
            }
            s.push_str(t(Msg::ExtViewDescription));
            s.push('\n');
            // 문단 사이 빈 줄(사용자 09-30 "적절하게 띄어쓰기") — 패키지 설명의 `\n`이 문단 경계.
            for (i, para) in self.description.split('\n').enumerate() {
                if i > 0 {
                    s.push('\n');
                }
                s.push_str(para.trim_end());
                s.push('\n');
            }
        }
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExtViewAction {
    /// 설정 버튼 — 그 확장의 설정 분류로(사용자 09-30 · 설치된 것만).
    Settings(String),
    Install(usize),
    /// ★ 업데이트 = 카탈로그 `n`번을 설치본 위에 다시 설치(옛 버전 폴더는 관리자가 치운다).
    Update(usize),
    Remove(String),
    Enable(String),
    Disable(String),
}

pub(crate) struct ExtView {
    bounds: Rect,
    scale: f32,
    hover: Option<usize>,
    /// 누르고 있는 버튼(MouseDown ~ MouseUp · 사용자 09-28 "클릭 효과 없이 동작") — 동작은 **같은 버튼 위에서 뗄 때**(nexa-ctl `Button`과 같은 규칙).
    pressed: Option<usize>,
    /// 그릴 때 잡은 버튼 영역(동작 · 자리).
    buttons: Vec<(ExtViewAction, Rect)>,
    actions: Vec<ExtViewAction>,
    /// 본문 = 읽기 전용 상자(선택·복사·스크롤·우클릭 메뉴).
    body: TextBox,
    /// 본문 글의 출처 표식(확장 id · 버전 · 상태 · 글 길이) — 바뀌면 `set_text`(스크롤·선택은 상자가 유지).
    body_key: String,
    /// 마지막 머리 높이(본문 상자의 y) — 사건 라우팅.
    head_bottom: i32,
}

impl Default for ExtView {
    fn default() -> Self {
        let mut body = TextBox::new("").with_multiline();
        body.set_read_only(true);
        body.set_wrap(true);
        body.set_line_numbers(false);
        body.set_gutter_marks(false);
        body.set_minimap(false);
        body.set_focus_ring(false);
        // 우클릭 메뉴는 팝업 층에서(`paint_popup` · 호스트가 맨 마지막에 부른다).
        body.set_popup_deferred(true);
        Self {
            bounds: Rect::default(),
            scale: 1.0,
            hover: None,
            pressed: None,
            buttons: Vec::new(),
            actions: Vec::new(),
            body,
            body_key: String::new(),
            head_bottom: 0,
        }
    }
}

impl ExtView {
    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        if (scale - self.scale).abs() > f32::EPSILON {
            self.body.set_scale(scale);
        }
        self.scale = scale;
    }

    /// 다른 상세 탭으로 바뀌었다 — 본문·hover를 처음으로.
    pub(crate) fn reset(&mut self) {
        self.hover = None;
        self.pressed = None;
        self.buttons.clear();
        self.body_key.clear();
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ExtViewAction> {
        std::mem::take(&mut self.actions)
    }

    /// 본문 상자(⌘/Ctrl+C 등 호스트의 클립보드 동작 대상 · `focused_textbox`).
    pub(crate) fn textbox_mut(&mut self) -> &mut TextBox {
        &mut self.body
    }

    /// 우클릭 편집 메뉴가 열려 있는가(호스트 팝업 게이트).
    pub(crate) fn popup_open(&self) -> bool {
        self.body.popup_open()
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.body.set_focused(on);
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// 사건 처리 — 다시 그릴 일이 있으면 true. 마우스는 뷰 영역 안일 때 호스트가 넘기고, 키·문자·전체 선택은 뷰 탭이
    /// 활성이고 편집기 포커스일 때 넘긴다. 머리(버튼) 밖의 포인터 사건과 키는 모두 본문 상자로.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        let mut inv = Invalidations::default();
        // 메뉴가 열려 있으면 상자가 먼저(항목 선택·바깥 클릭 닫기).
        if self.body.popup_open() {
            self.body.on_event(ev, &mut inv);
            self.after_body();
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                let h = self.buttons.iter().position(|(_, r)| r.contains(p));
                let changed = h != self.hover;
                self.hover = h;
                // 본문 드래그 선택은 상자가 안다(머리 위로 올라가도 이어진다).
                self.body.on_event(ev, &mut inv);
                changed || !inv.is_empty()
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                self.pressed = self.buttons.iter().position(|(_, r)| r.contains(p));
                if self.pressed.is_none() && y >= self.head_bottom {
                    self.body.set_focused(true);
                    self.body.on_event(ev, &mut inv);
                }
                true
            }
            InputEvent::RightDown { x, y } => {
                if y >= self.head_bottom {
                    self.body.set_focused(true);
                    self.body.on_event(ev, &mut inv);
                }
                let _ = x;
                true
            }
            InputEvent::MouseUp { x, y } => {
                let p = Point { x, y };
                if let Some(i) = self.pressed.take() {
                    if let Some((a, r)) = self.buttons.get(i) {
                        if r.contains(p) {
                            self.actions.push(a.clone());
                        }
                    }
                    return true;
                }
                self.body.on_event(ev, &mut inv);
                self.after_body();
                true
            }
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => {
                self.body.on_event(ev, &mut inv);
                true
            }
            // 키·전체 선택 = 상자(읽기 전용이라 글은 안 바뀐다 · 캐럿/선택/스크롤만).
            InputEvent::Key { .. } | InputEvent::SelectAll => {
                // 뷰 탭이 활성 + 편집기 포커스 = 본문 상자가 키의 대상(호스트가 그때만 넘긴다).
                self.body.set_focused(true);
                self.body.on_event(ev, &mut inv);
                self.after_body();
                true
            }
            _ => false,
        }
    }

    /// 상자가 남긴 편집 메뉴 요청 — 읽기 전용이라 복사만 뜻이 있다(선택 없으면 전체 · SQL Preview와 같은 규칙).
    fn after_body(&mut self) {
        if let Some(a) = self.body.take_edit_ctx() {
            if matches!(a, nexa_ctl::EditCtxAction::Copy) {
                let text = self
                    .body
                    .copy_selection()
                    .unwrap_or_else(|| self.body.text());
                let _ = crate::clipboard::write_text(&text);
            }
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme, d: &ExtDetail) {
        let b = self.bounds;
        if b.w <= 0 || b.h <= 0 {
            return;
        }
        dc.fill_rect(b, th.field_bg);
        let pad = self.s(24.0);
        let x = b.x + pad;
        let w = (b.w - pad * 2).max(self.s(80.0));
        let clip = b;
        let mut y = b.y + pad;
        // ── 머리: 이름(크게) · id·버전·종류·상태 · (업데이트 가능) · 한 줄 설명
        // 제목 = `Message` 자리(호스트가 UI 글꼴의 1.7배로 넣어 준다).
        dc.select_font(FontSlot::Message, true);
        let th_title = dc.text_height();
        dc.text(x, y, clip, &d.row.name, th.text);
        y += th_title + self.s(4.0);
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let state = t(if !d.row.installed {
            Msg::ExtStNotInstalled
        } else if d.row.enabled {
            Msg::ExtStEnabled
        } else {
            Msg::ExtStDisabled
        });
        let mut sub = format!(
            "{}  ·  v{}  ·  {}  ·  {state}",
            d.row.id, d.row.version, d.row.kind
        );
        if d.row.installed && d.row.latest.is_none() && d.row.catalog.is_some() {
            sub.push_str("  ·  ");
            sub.push_str(t(Msg::ExtStUpToDate));
        }
        dc.text(x, y, clip, &sub, th.text_dim);
        y += lh + self.s(4.0);
        if let Some(v) = &d.row.latest {
            dc.select_font(FontSlot::Base, true);
            dc.text(x, y, clip, &tf(Msg::ExtStUpdateAvail, &[v]), th.accent);
            dc.select_font(FontSlot::Base, false);
            y += lh + self.s(4.0);
        }
        y += self.s(4.0);
        if !d.row.summary.is_empty() {
            for line in wrap(dc, &d.row.summary, w) {
                dc.text(x, y, clip, &line, th.text);
                y += lh + self.s(2.0);
            }
            y += self.s(8.0);
        }
        // ── 동작 버튼
        let mut labels: Vec<(ExtViewAction, String, bool)> = Vec::new();
        if d.row.installed {
            if let (Some(v), Some(n)) = (&d.row.latest, d.row.catalog) {
                labels.push((
                    ExtViewAction::Update(n),
                    tf(Msg::ExtBtnUpdateTo, &[v]),
                    true,
                ));
            }
            labels.push((
                ExtViewAction::Settings(d.row.id.clone()),
                t(Msg::ExtBtnSettings).to_string(),
                false,
            ));
            labels.push((
                if d.row.enabled {
                    ExtViewAction::Disable(d.row.id.clone())
                } else {
                    ExtViewAction::Enable(d.row.id.clone())
                },
                t(if d.row.enabled {
                    Msg::ExtBtnDisable
                } else {
                    Msg::ExtBtnEnable
                })
                .to_string(),
                false,
            ));
            labels.push((
                ExtViewAction::Remove(d.row.id.clone()),
                t(Msg::ExtBtnRemove).to_string(),
                false,
            ));
        } else if let Some(n) = d.row.catalog {
            labels.push((
                ExtViewAction::Install(n),
                t(Msg::ExtBtnInstall).to_string(),
                true,
            ));
        }
        self.buttons.clear();
        let btn_h = lh + self.s(12.0);
        let mut bx = x;
        let has_btns = !labels.is_empty();
        for (i, (action, label, primary)) in labels.into_iter().enumerate() {
            let bw = dc.text_width(&label) + self.s(28.0);
            let r = Rect::new(bx, y, bw, btn_h);
            let hot = self.hover == Some(i);
            let down = self.pressed == Some(i);
            // 상태 레이어(hover 8 % · 눌림 12 % · 버튼 부품과 같은 토큰) + 눌림 = 글자 1px 아래(안으로 들어가는 느낌).
            let st = nexa_ctl::tokens::State::of(false, hot, down, true);
            let ty = dc.text_center_y(r.y, r.h) + if down { self.s(1.0) } else { 0 };
            if primary {
                dc.fill_round_rect(r, self.s(5.0), th.accent);
                dc.state_layer(r, th.text, st);
                dc.text(
                    r.x + self.s(14.0),
                    ty,
                    clip,
                    &label,
                    nexa_ctl::Color(0x00FF_FFFF),
                );
            } else {
                dc.fill_round_rect(r, self.s(5.0), th.panel_bg_alt);
                dc.state_layer(r, th.text, st);
                dc.stroke_round_rect(
                    r,
                    self.s(5.0),
                    if hot || down { th.accent } else { th.border },
                    1.0,
                );
                dc.text(r.x + self.s(14.0), ty, clip, &label, th.text);
            }
            self.buttons.push((action, r));
            bx += bw + self.s(8.0);
        }
        if has_btns {
            y += btn_h + self.s(18.0);
        }
        dc.fill_rect(Rect::new(x, y, w, 1).intersection(&b), th.border);
        y += self.s(10.0);
        self.head_bottom = y;
        // ── 본문 = 읽기 전용 상자(세부 정보 + 설명 · 선택·복사·스크롤·우클릭 메뉴).
        let key = format!(
            "{}|{}|{}|{}|{}",
            d.row.id,
            d.row.version,
            d.row.enabled,
            d.row.latest.as_deref().unwrap_or(""),
            d.fields.len() + d.description.len()
        );
        if key != self.body_key {
            self.body_key = key;
            self.body.set_text(&d.body_text());
        }
        let body_rect = Rect::new(
            x - self.s(4.0),
            y,
            w + self.s(4.0),
            (b.bottom() - y - self.s(8.0)).max(lh),
        );
        let mut inv = Invalidations::default();
        self.body.set_bounds(body_rect, &mut inv);
        self.body.paint(dc, th);
    }

    /// 우클릭 편집 메뉴(팝업 층 · 호스트가 맨 마지막에 부른다).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.body.popup_open() {
            self.body.paint_popup(dc, th);
        }
    }
}

/// 단어 단위 줄 접기(한 단어가 폭을 넘으면 그대로 둔다 — 잘려 보인다).
fn wrap(dc: &mut dyn DrawCtx, text: &str, max_w: i32) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let cand = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && dc.text_width(&cand) > max_w {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = cand;
        }
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(latest: Option<&str>) -> ExtDetail {
        ExtDetail {
            row: ExtRow {
                id: "x".into(),
                name: "X".into(),
                version: "1.2.0".into(),
                kind: "wasm".into(),
                summary: "sum".into(),
                installed: true,
                enabled: true,
                catalog: Some(0),
                source: String::new(),
                latest: latest.map(str::to_string),
            },
            fields: vec![
                ("상태".into(), "켜짐".into()),
                ("버전".into(), "1.2.0".into()),
            ],
            description: "first para\nsecond para".into(),
        }
    }

    /// 버튼 히트 → 동작 · 밖에서 떼면 동작 없음 · 다른 탭으로 바뀌면 처음으로.
    #[test]
    fn click_and_reset() {
        let mut v = ExtView::default();
        v.set_bounds(Rect::new(100, 50, 600, 300), 1.0);
        v.head_bottom = 200;
        v.buttons = vec![
            (
                ExtViewAction::Disable("x".into()),
                Rect::new(124, 150, 80, 28),
            ),
            (
                ExtViewAction::Remove("x".into()),
                Rect::new(212, 150, 80, 28),
            ),
        ];
        let down = |x, y| InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        };
        // 버튼 = 누름 + 같은 자리에서 뗌(09-28 클릭 효과) · 밖에서 떼면 동작 없음.
        for (x, y) in [(130, 160), (220, 160), (500, 250)] {
            v.on_event(&down(x, y));
            v.on_event(&InputEvent::MouseUp { x, y });
        }
        v.on_event(&down(130, 160));
        v.on_event(&InputEvent::MouseUp { x: 5, y: 5 });
        assert_eq!(
            v.take_actions(),
            vec![
                ExtViewAction::Disable("x".into()),
                ExtViewAction::Remove("x".into())
            ]
        );
        v.body_key = "k".into();
        v.reset();
        assert!(v.body_key.is_empty() && v.buttons.is_empty());
    }

    /// 본문 글 = 세부 정보 표(라벨 정렬) + 설명 문단 사이 빈 줄 · 읽기 전용 상자 · 전체 선택은 받는다.
    #[test]
    fn body_text_and_read_only_box() {
        let d = detail(Some("1.3.0"));
        let text = d.body_text();
        assert!(text.contains("  상태   켜짐\n  버전   1.2.0\n"), "{text}");
        assert!(text.contains("first para\n\nsecond para\n"), "{text}");
        let mut v = ExtView::default();
        assert!(v.body.is_read_only() && v.body.wrap());
        v.body.set_text(&text);
        v.on_event(&InputEvent::SelectAll);
        assert_eq!(v.body.copy_selection().as_deref(), Some(text.as_str()));
        // 문자 입력은 읽기 전용이라 바뀌지 않는다.
        v.on_event(&InputEvent::Char { c: 'z', now_ms: 0 });
        assert_eq!(v.body.text(), text);
    }
}
