//! **확장 패널**(왼쪽 사이드 · 활동 막대 "확장" · 사용자 09-19 "VS Code처럼 검색 + 설치된 목록을 먼저").
//!
//! 위에서 아래로: 머리("확장" + ⟳) · 검색 상자 · **설치됨 (n)** · **설치 가능 (n)**. 행 = 이름·버전 / 한 줄 설명 + 오른쪽 버튼
//! (설치됨 = `끄기|켜기` · `삭제` · 설치 가능 = `설치`). 행을 누르면 호스트가 상세를 편집기 탭으로 연다.
//! 이 파일은 그리기·히트·검색 거르기만 — 저장소 읽기·설치·켜기/끄기는 호스트(`main.rs ext_*`)가 한다(네트워크는 패널을 열거나
//! ⟳를 누를 때만 · 26 §8). 확장 관리자가 꺼져 있으면 활동 막대에 아이콘이 없으므로 이 패널도 열리지 않는다.

use crate::filterbar::{FilterBar, FilterEvent, GAP_Y};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Invalidations, Key as CtlKey, TextBox};
use nsql_i18n::{t, tf, Msg};

/// 목록 한 줄(호스트가 만든다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExtRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub summary: String,
    /// 설치됨 구역인가(아니면 설치 가능).
    pub installed: bool,
    /// 설치됨일 때 켜져 있는가.
    pub enabled: bool,
    /// 설치 가능일 때 호스트 카탈로그 인덱스(`ext.install:<n>`).
    pub catalog: Option<usize>,
    /// 저장소 표시(설치 가능 행의 출처).
    pub source: String,
    /// ★ 설치본보다 **새 카탈로그 버전**(업데이트 대상 · 사용자 09-30) — 설치됨 + 카탈로그가 더 새 버전을 알 때만 Some.
    pub latest: Option<String>,
}

/// 패널이 호스트에 내는 동작.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExtPanelAction {
    /// ⟳ — 저장소를 다시 읽는다.
    Refresh,
    /// 설정 버튼 — 그 확장의 설정 분류로(사용자 09-30).
    Settings(String),
    /// 행 클릭 — 상세를 연다.
    Open(ExtRow),
    Install(usize),
    /// ★ 업데이트 = 카탈로그 `n`번을 설치본 위에(관리자가 옛 버전 폴더를 치운다).
    Update(usize),
    Remove(String),
    Enable(String),
    Disable(String),
}

/// 그려진 행 하나의 히트 영역: (rows 인덱스 · 행 rect · 버튼들).
type RowHit = (usize, Rect, Vec<(Btn, Rect)>);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Btn {
    /// ★ 그 확장의 설정 분류로(설치된 것만 활성 · 사용자 09-30).
    Settings,
    Toggle,
    Remove,
    Install,
    /// ★ 업데이트(설치본보다 새 카탈로그 버전이 있을 때만 · 강조 버튼).
    Update,
}

pub(crate) struct ExtPanel {
    visible: bool,
    focused: bool,
    bounds: Rect,
    /// ★ 고속 스크롤(이 패널은 오버레이 막대가 없다 → 가속기·HUD를 직접 · 사용자 09-30).
    accel: nexa_ctl::ScrollAccel,
    hud: nexa_ctl::SpeedHud,
    scale: f32,
    search: FilterBar,
    rows: Vec<ExtRow>,
    /// 저장소 읽기 결과 안내(오류·"읽는 중"이 아니라 마지막 상태 한 줄 · 비면 없음).
    note: String,
    scroll: i32,
    content_h: i32,
    hover: Option<(usize, Option<Btn>)>,
    /// 누르고 있는 행 버튼(MouseDown ~ MouseUp · 09-28) — 동작은 같은 버튼 위에서 뗄 때. 행 자체 클릭(열기)은 종전대로 누름에서.
    pressed: Option<(usize, Btn)>,
    /// 새로 고침 아이콘 hover · 누름.
    refresh_hover: bool,
    refresh_pressed: bool,
    selected: Option<String>,
    refresh_rect: Rect,
    /// 그릴 때 잡은 히트 영역: (필터된 행 인덱스 → rows 인덱스, 행 rect, 버튼들).
    hits: Vec<RowHit>,
    actions: Vec<ExtPanelAction>,
}

impl ExtPanel {
    pub(crate) fn new() -> Self {
        ExtPanel {
            visible: false,
            focused: false,
            bounds: Rect::default(),
            accel: nexa_ctl::ScrollAccel::new(),
            hud: nexa_ctl::SpeedHud::default(),
            scale: 1.0,
            search: FilterBar::new(t(Msg::PhExtSearch), &[]),
            rows: Vec::new(),
            note: String::new(),
            scroll: 0,
            content_h: 0,
            hover: None,
            pressed: None,
            refresh_hover: false,
            refresh_pressed: false,
            selected: None,
            refresh_rect: Rect::default(),
            hits: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// 검색어 이력 잇기(전역 · `filter.extensions` · 사용자 09-23).
    pub(crate) fn set_history(&mut self, h: crate::search_history::SharedHistory) {
        self.search.set_history(h, "filter.extensions");
    }

    /// 팝업 층(검색 틀 편집 메뉴 · 이력 드롭다운 · 툴팁) — 창의 맨 마지막에.
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.visible {
            self.search.paint_popup(dc, th);
        }
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.hover = None;
            self.set_focused(false);
        }
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let pad = self.s(8.0);
        let head = self.s(30.0);
        self.search.set_bounds(
            Rect::new(b.x + pad, b.y + head, (b.w - pad * 2).max(0), self.s(28.0)),
            scale,
        );
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        self.search.set_focused(on);
    }

    /// 검색 상자에 포커스(패널을 열 때).
    pub(crate) fn focus_query(&mut self) {
        self.set_focused(true);
    }

    /// IME·편집 컨텍스트 라우팅용.
    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.focused {
            Some(self.search.tb_mut())
        } else {
            None
        }
    }

    /// 목록 교체(설치됨 먼저 · 그다음 설치 가능) + 안내 한 줄.
    pub(crate) fn set_rows(&mut self, rows: Vec<ExtRow>, note: String) {
        self.rows = rows;
        self.note = note;
        self.clamp_scroll();
    }

    /// 검색어가 있는가(Esc = 있으면 지우고 · 없으면 호스트가 편집기로 포커스를 돌린다).
    pub(crate) fn has_query(&self) -> bool {
        !self.search.text().is_empty()
    }

    /// 지금 목록의 행(호스트가 id로 찾을 때 · 확장 뷰 탭 복원).
    pub(crate) fn rows(&self) -> &[ExtRow] {
        &self.rows
    }

    pub(crate) fn take_actions(&mut self) -> Vec<ExtPanelAction> {
        std::mem::take(&mut self.actions)
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn list_rect(&self) -> Rect {
        let b = self.bounds;
        let top = b.y + self.s(30.0) + self.s(28.0) + self.s(GAP_Y);
        Rect::new(b.x, top, b.w, (b.bottom() - top).max(0))
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h - self.list_rect().h).max(0);
        self.scroll = self.scroll.clamp(0, max);
    }

    /// 검색어로 거른 rows 인덱스(이름·id·설명 부분 일치 · 대소문자 무시 · 조합 중 글자 포함).
    fn filtered(&self) -> Vec<usize> {
        // 옵션(Aa·ab·(.*))은 부품이 본다 — 이름·id·설명 중 하나라도.
        (0..self.rows.len())
            .filter(|&i| {
                let r = &self.rows[i];
                self.search.is_empty()
                    || self.search.matches(&r.name)
                    || self.search.matches(&r.id)
                    || self.search.matches(&r.summary)
            })
            .collect()
    }

    /// 사건 처리 — 다시 그릴 일이 있으면 true.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        // ★ 검색 틀 팝업(이력 드롭다운) 열림 중 마우스는 틀이 먼저(사용자 10-07 · 공용 규칙 `popup_takes/keeps`).
        if self.search.popup_takes(ev) {
            if self.search.on_event(ev, &mut inv) == FilterEvent::Changed {
                self.scroll = 0;
            }
            if self.search.popup_keeps(ev) {
                return true;
            }
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                // 검색 틀 토글 hover.
                let _ = self.search.on_event(ev, &mut inv);
                let p = Point { x, y };
                let h = self
                    .hits
                    .iter()
                    .find(|(_, r, _)| r.contains(p))
                    .map(|(i, _, btns)| {
                        (
                            *i,
                            btns.iter().find(|(_, r)| r.contains(p)).map(|(b, _)| *b),
                        )
                    });
                let rh = self.refresh_rect.contains(p);
                let changed = h != self.hover || rh != self.refresh_hover;
                self.hover = h;
                self.refresh_hover = rh;
                changed
            }
            InputEvent::Wheel { delta } => {
                if !self.bounds.contains(Point {
                    x: self.bounds.x + 1,
                    y: self.bounds.y + 1,
                }) {
                    return false;
                }
                let cfg = nexa_ctl::fast_scroll();
                let k = self.accel.factor_cfg(-delta, &cfg);
                self.hud.note(k, &cfg);
                self.scroll -= delta / 120 * self.s(48.0) * k;
                self.clamp_scroll();
                true
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if self.refresh_rect.contains(p) {
                    self.refresh_pressed = true;
                    return true;
                }
                if self.search.bounds().contains(p) {
                    self.set_focused(true);
                    if self.search.on_event(ev, &mut inv) == FilterEvent::Changed {
                        self.scroll = 0;
                    }
                    return true;
                }
                let hit = self
                    .hits
                    .iter()
                    .find(|(_, r, _)| r.contains(p))
                    .map(|(i, _, btns)| {
                        (
                            *i,
                            btns.iter().find(|(_, r)| r.contains(p)).map(|(b, _)| *b),
                        )
                    });
                if let Some((i, btn)) = hit {
                    let Some(row) = self.rows.get(i).cloned() else {
                        return false;
                    };
                    self.selected = Some(row.id.clone());
                    match btn {
                        // 버튼 = 누름 표시만 · 동작은 같은 버튼 위에서 뗄 때(사용자 09-28 "클릭 효과").
                        Some(b) => self.pressed = Some((i, b)),
                        None => self.actions.push(ExtPanelAction::Open(row)),
                    }
                    return true;
                }
                false
            }
            InputEvent::MouseUp { x, y } => {
                let p = Point { x, y };
                if self.refresh_pressed {
                    self.refresh_pressed = false;
                    if self.refresh_rect.contains(p) {
                        self.actions.push(ExtPanelAction::Refresh);
                    }
                    return true;
                }
                if let Some((i, b)) = self.pressed.take() {
                    let same = self
                        .hits
                        .iter()
                        .find(|(k, _, _)| *k == i)
                        .and_then(|(_, _, btns)| {
                            btns.iter().find(|(kind, r)| *kind == b && r.contains(p))
                        })
                        .is_some();
                    if same {
                        if let Some(row) = self.rows.get(i).cloned() {
                            self.actions.push(match b {
                                Btn::Install => match row.catalog {
                                    Some(n) => ExtPanelAction::Install(n),
                                    None => ExtPanelAction::Open(row),
                                },
                                Btn::Update => match row.catalog {
                                    Some(n) => ExtPanelAction::Update(n),
                                    None => ExtPanelAction::Open(row),
                                },
                                Btn::Remove => ExtPanelAction::Remove(row.id),
                                Btn::Toggle if row.enabled => ExtPanelAction::Disable(row.id),
                                Btn::Toggle => ExtPanelAction::Enable(row.id),
                                Btn::Settings => ExtPanelAction::Settings(row.id),
                            });
                        }
                    }
                    return true;
                }
                // 토글 hover·클릭(뗌에서 확정) — 옵션이 바뀌면 다시 거른다.
                if self.search.on_event(ev, &mut inv) == FilterEvent::Changed {
                    self.scroll = 0;
                    return true;
                }
                false
            }
            InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } if self.focused => {
                if !self.search.text().is_empty() {
                    self.search.set_text("");
                    self.scroll = 0;
                }
                true
            }
            InputEvent::Key { .. }
            | InputEvent::Char { .. }
            | InputEvent::Undo
            | InputEvent::Redo
                if self.focused =>
            {
                if self.search.on_event(ev, &mut inv) == FilterEvent::Changed {
                    self.scroll = 0;
                }
                true
            }
            _ => false,
        }
    }

    /// IME 조합·확정 뒤 호스트가 부른다 — 목록을 바로 거른다(조합 중 글자 포함).
    pub(crate) fn query_changed(&mut self) {
        self.search.refresh();
        self.scroll = 0;
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.visible
            && (self.search.tick(now_ms)
                | self
                    .hud
                    .tick(std::time::Instant::now(), &nexa_ctl::fast_scroll()))
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible || self.bounds.w <= 0 {
            return;
        }
        let b = self.bounds;
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        let pad = self.s(8.0);
        // 머리.
        dc.select_font(FontSlot::Base, true);
        let lh = dc.text_height();
        let head_h = self.s(30.0);
        dc.text(
            b.x + pad,
            b.y + (head_h - lh) / 2,
            b,
            t(Msg::ExtPanelTitle),
            th.text_dim,
        );
        dc.select_font(FontSlot::Base, false);
        let rw = dc.text_width("⟳") + self.s(12.0);
        self.refresh_rect = Rect::new(
            b.right() - pad - rw,
            b.y + self.s(4.0),
            rw,
            head_h - self.s(8.0),
        );
        // 새로 고침 아이콘 = hover/눌림 상태 레이어(09-28).
        dc.state_layer(
            self.refresh_rect,
            th.text,
            nexa_ctl::tokens::State::of(false, self.refresh_hover, self.refresh_pressed, true),
        );
        dc.text(
            self.refresh_rect.x + self.s(6.0),
            b.y + (head_h - lh) / 2 + if self.refresh_pressed { self.s(1.0) } else { 0 },
            self.refresh_rect,
            "⟳",
            th.text,
        );
        self.search.paint(dc, th, true);
        // 목록.
        let list = self.list_rect();
        let idx = self.filtered();
        let querying = !self.search.display_text().trim().is_empty();
        let (inst, avail): (Vec<usize>, Vec<usize>) =
            idx.iter().copied().partition(|&i| self.rows[i].installed);
        let row_h = lh * 2 + self.s(14.0);
        let sec_h = lh + self.s(10.0);
        let mut y = list.y - self.scroll;
        self.hits.clear();
        let updates = inst
            .iter()
            .filter(|&&i| self.rows[i].latest.is_some())
            .count();
        let sections: [(Msg, &Vec<usize>, Msg); 2] = [
            (Msg::ExtPanelInstalled, &inst, Msg::ExtPanelNoneInstalled),
            (Msg::ExtPanelAvailable, &avail, Msg::ExtPanelNoneAvailable),
        ];
        for (title, items, empty) in sections {
            // ★ 설치됨 제목 뒤 " · 업데이트 M"(대상이 있을 때만 · 사용자 09-30).
            let extra = if title == Msg::ExtPanelInstalled && updates > 0 {
                tf(Msg::ExtPanelUpdates, &[&updates.to_string()])
            } else {
                String::new()
            };
            let sec = Rect::new(list.x, y, list.w, sec_h);
            if let Some(c) = clip(sec, list) {
                dc.fill_rect(c, th.panel_bg_alt);
                dc.select_font(FontSlot::Base, true);
                dc.text(
                    sec.x + pad,
                    sec.y + (sec_h - lh) / 2,
                    c,
                    &format!("{}{extra}", tf(title, &[&items.len().to_string()])),
                    th.text,
                );
                dc.select_font(FontSlot::Base, false);
            }
            y += sec_h;
            if items.is_empty() {
                let r = Rect::new(list.x, y, list.w, sec_h);
                if let Some(c) = clip(r, list) {
                    let msg = if querying {
                        Msg::ExtPanelNoMatch
                    } else {
                        empty
                    };
                    dc.text(r.x + pad, r.y + (sec_h - lh) / 2, c, t(msg), th.text_dim);
                }
                y += sec_h;
                continue;
            }
            for &i in items {
                let row = &self.rows[i];
                let r = Rect::new(list.x, y, list.w, row_h);
                y += row_h;
                let Some(c) = clip(r, list) else { continue };
                let hot = self.hover.is_some_and(|(h, _)| h == i);
                if self.selected.as_deref() == Some(row.id.as_str()) {
                    dc.fill_rect(c, th.sel_bg);
                } else if hot {
                    dc.fill_rect_alpha(c, th.text, 0.06);
                }
                // 1줄: 이름(굵게) + 버전(오른쪽 · 흐림).
                dc.select_font(FontSlot::Base, true);
                let name_color = if row.installed && !row.enabled {
                    th.text_dim
                } else {
                    th.text
                };
                dc.text(r.x + pad, r.y + self.s(5.0), c, &row.name, name_color);
                let nw = dc.text_width(&row.name);
                dc.select_font(FontSlot::Base, false);
                // ★ 버전은 이름 바로 옆 `v1.2.0`(사용자 09-30 "버전 확인이 어렵다") · 업데이트 대상이면 `→ v1.3.0` 강조색 ·
                //   오른쪽 끝 = 종류(흐림).
                let mut vx = r.x + pad + nw + self.s(8.0);
                let ver = format!("v{}", row.version);
                dc.text(vx, r.y + self.s(5.0), c, &ver, th.text_dim);
                vx += dc.text_width(&ver) + self.s(6.0);
                if let Some(nv) = &row.latest {
                    let up = format!("→ v{nv}");
                    dc.text(vx, r.y + self.s(5.0), c, &up, th.accent);
                }
                let kw = dc.text_width(&row.kind);
                dc.text(
                    r.right() - pad - kw,
                    r.y + self.s(5.0),
                    c,
                    &row.kind,
                    th.text_dim,
                );
                // 2줄: 설명(왼쪽) + 버튼(오른쪽).
                let y2 = r.y + self.s(5.0) + lh + self.s(2.0);
                // 설정 버튼은 늘 맨 앞(끄기/설치 앞) · 설치된 것만 활성(사용자 09-30).
                let settings = (
                    Btn::Settings,
                    t(Msg::ExtBtnSettings).to_string(),
                    row.installed,
                );
                let labels: Vec<(Btn, String, bool)> = if row.installed {
                    let mut v = Vec::new();
                    if row.latest.is_some() && row.catalog.is_some() {
                        v.push((Btn::Update, t(Msg::ExtBtnUpdate).to_string(), true));
                    }
                    v.extend([
                        settings,
                        (
                            Btn::Toggle,
                            t(if row.enabled {
                                Msg::ExtBtnDisable
                            } else {
                                Msg::ExtBtnEnable
                            })
                            .to_string(),
                            true,
                        ),
                        (Btn::Remove, t(Msg::ExtBtnRemove).to_string(), true),
                    ]);
                    v
                } else {
                    vec![
                        settings,
                        (Btn::Install, t(Msg::ExtBtnInstall).to_string(), true),
                    ]
                };
                let mut bx = r.right() - pad;
                let mut btns = Vec::new();
                // ★ 버튼 = 보조(Status) 글꼴 · 폭 = 그 언어의 글 폭 + 여백(사용자 09-30 "글자 작게 · i18n에 맞춰").
                dc.select_font(FontSlot::Status, false);
                let blh = dc.text_height();
                for (kind, label, enabled) in labels.iter().rev() {
                    let bw = dc.text_width(label) + self.s(10.0);
                    bx -= bw;
                    let br = Rect::new(bx, y2, bw, blh + self.s(3.0));
                    let bhot = *enabled && self.hover == Some((i, Some(*kind)));
                    let bdown = *enabled && self.pressed == Some((i, *kind));
                    let st = nexa_ctl::tokens::State::of(false, bhot, bdown, true);
                    let ly = y2 + self.s(1.0) + if bdown { self.s(1.0) } else { 0 };
                    if !*enabled {
                        // 비활성 = 흐린 테두리·글자 · 히트 없음.
                        dc.stroke_round_rect(br, self.s(4.0), th.border, 1.0);
                        dc.text(br.x + self.s(5.0), ly, br, label, th.text_dim);
                        bx -= self.s(5.0);
                        continue;
                    }
                    if matches!(kind, Btn::Install | Btn::Update) {
                        dc.fill_round_rect(br, self.s(4.0), th.accent);
                        dc.state_layer(br, th.text, st);
                        dc.text(br.x + self.s(5.0), ly, br, label, th.panel_bg);
                    } else {
                        dc.fill_round_rect(br, self.s(4.0), th.field_bg);
                        dc.state_layer(br, th.text, st);
                        dc.stroke_round_rect(
                            br,
                            self.s(4.0),
                            if bhot || bdown { th.accent } else { th.border },
                            1.0,
                        );
                        dc.text(br.x + self.s(5.0), ly, br, label, th.text);
                    }
                    btns.push((*kind, br));
                    bx -= self.s(5.0);
                }
                dc.select_font(FontSlot::Base, false);
                let sum_clip = Rect::new(r.x + pad, y2, (bx - r.x - pad).max(0), lh + self.s(4.0));
                if let Some(sc) = clip(sum_clip, list) {
                    dc.text(r.x + pad, y2 + self.s(1.0), sc, &row.summary, th.text_dim);
                }
                self.hits.push((i, c, btns));
            }
        }
        if !self.note.is_empty() {
            let r = Rect::new(list.x, y + self.s(4.0), list.w, sec_h);
            if let Some(c) = clip(r, list) {
                dc.text(
                    r.x + pad,
                    r.y + (sec_h - lh) / 2,
                    c,
                    &self.note,
                    th.text_dim,
                );
            }
            y += sec_h + self.s(4.0);
        }
        self.content_h = y + self.scroll - list.y;
        // 속도 HUD(목록 영역 · 설정 위치).
        self.hud
            .paint(dc, th, list, self.scale, &nexa_ctl::fast_scroll());
    }
}

fn clip(r: Rect, within: Rect) -> Option<Rect> {
    let c = r.intersection(&within);
    (c.w > 0 && c.h > 0).then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, installed: bool, enabled: bool, catalog: Option<usize>) -> ExtRow {
        ExtRow {
            id: id.into(),
            name: id.to_uppercase(),
            version: "1.0.0".into(),
            kind: "builtin".into(),
            summary: format!("{id} summary"),
            installed,
            enabled,
            catalog,
            source: String::new(),
            latest: None,
        }
    }

    /// 검색 거르기(이름·id·설명 · 대소문자 무시) + 클릭 → 동작(행 = 상세 · 버튼 = 켜기/끄기/삭제/설치) + 숨기면 안 받음.
    #[test]
    fn filter_and_click_actions() {
        let mut p = ExtPanel::new();
        p.set_visible(true);
        p.set_bounds(Rect::new(48, 40, 300, 600), 1.0);
        p.set_rows(
            vec![
                row("rainbow-pairs", true, true, Some(0)),
                row("sql-snippets", true, false, None),
                row("csv-tools", false, false, Some(1)),
            ],
            String::new(),
        );
        assert_eq!(p.filtered(), vec![0, 1, 2]);
        p.search.set_text("SNIP");
        assert_eq!(p.filtered(), vec![1]);
        p.search.set_text("summary");
        assert_eq!(p.filtered().len(), 3);
        p.search.set_text("");
        // 그린 뒤의 히트 영역을 흉내(행 0 = 끄기·삭제 · 행 2 = 설치).
        p.hits = vec![
            (
                0,
                Rect::new(48, 120, 300, 40),
                vec![
                    (Btn::Toggle, Rect::new(200, 140, 50, 18)),
                    (Btn::Remove, Rect::new(260, 140, 50, 18)),
                ],
            ),
            (
                1,
                Rect::new(48, 160, 300, 40),
                vec![(Btn::Toggle, Rect::new(200, 180, 50, 18))],
            ),
            (
                2,
                Rect::new(48, 240, 300, 40),
                vec![(Btn::Install, Rect::new(260, 260, 50, 18))],
            ),
        ];
        let down = |x, y| InputEvent::MouseDown {
            x,
            y,
            shift: false,
            primary: false,
        };
        // 버튼은 누름 + 같은 자리에서 뗌(09-28 클릭 효과) · 행 클릭(열기)은 누름에서.
        for (x, y) in [(60, 125), (210, 145), (270, 145), (210, 185), (270, 265)] {
            p.on_event(&down(x, y));
            p.on_event(&InputEvent::MouseUp { x, y });
        }
        // 눌렀다가 다른 자리에서 떼면 동작 없음.
        p.on_event(&down(210, 145));
        p.on_event(&InputEvent::MouseUp { x: 5, y: 5 });
        let got = p.take_actions();
        assert_eq!(got.len(), 5, "버튼 밖에서 뗀 것은 동작 없음");
        assert!(matches!(&got[0], ExtPanelAction::Open(r) if r.id == "rainbow-pairs"));
        assert_eq!(got[1], ExtPanelAction::Disable("rainbow-pairs".into()));
        assert_eq!(got[2], ExtPanelAction::Remove("rainbow-pairs".into()));
        assert_eq!(got[3], ExtPanelAction::Enable("sql-snippets".into()));
        assert_eq!(got[4], ExtPanelAction::Install(1));
        assert!(p.take_actions().is_empty());
        p.set_visible(false);
        assert!(!p.on_event(&down(60, 125)) && p.take_actions().is_empty());
    }
}
