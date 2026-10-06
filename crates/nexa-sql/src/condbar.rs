//! ★ 결과 그리드 **인라인 조건 입력란**(DBeaver 조건 바 · 사용자 10-06 · T-181 후속) — 필터 줄(칩) 자리를 조건 바로 쓴다.
//! 한 줄 입력(`TextBox`)에 `WHERE` 뒤에 올 식을 치면 **Enter = 실행**: 호스트가 출처 문장을 `SELECT * FROM ( <출처> ) q WHERE 1=1
//! AND (<조건>)`으로 감싸 **같은 결과 탭**에서 다시 돈다(오류는 실행 오류 그대로 = 토스트). 자동 완성 = 결과 열 이름 + 연산 낱말
//! (`AND` `OR` `NOT` `LIKE` `IN` `IS NULL` …) + 방언 내장 함수 — 캐럿 앞 낱말로 거른 팝업(`ContextMenu`) · Tab/Enter/클릭 = 넣기 · Esc = 닫기.
//! 맨 오른쪽 = **조회 SQL 복사** 버튼(`copybtn` 부품 · 눌림 애니메이션). 설정 `grid.condition_bar`(기본 보임).
//! **검증(사용자 10-06)**: Enter 때 연결된 탭의 **방언** 기준으로 식을 본다 — 괄호·따옴표 짝 · `;`·문장 시작 낱말 금지 · 식별자는
//! 결과 열(`q.열`도) · 연산 낱말 · 방언 내장 함수(nsql-script `builtins` · 뒤에 `(`)만 · PostgreSQL 전용 `::`은 다른 방언에서 오류.
//! 틀리면 실행하지 않고 상자 테두리 빨강 + 상태줄 원인. 완성 후보도 같은 기준(열 → 낱말 → 방언 함수).
use crate::copybtn::CopyBtn;
use nexa_ctl::controls::ctxmenu::{ContextMenu as CtxMenu, CtxItem};
use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Key, TextBox, Widget};
use nsql_core::Dialect;
use nsql_i18n::{t, tf, Msg};
use std::time::Instant;

/// 자동 완성에 넣는 연산 낱말(열 이름 뒤에 · 접두가 맞을 때만).
const KEYWORDS: &[&str] = &[
    "AND",
    "OR",
    "NOT",
    "LIKE",
    "IN",
    "IS NULL",
    "IS NOT NULL",
    "BETWEEN",
    "EXISTS",
];

/// 식 안에서 허용하는 낱말(검증 · 대소문자 무시) — 연산·논리·CASE·리터럴.
const ALLOWED_WORDS: &[&str] = &[
    "AND",
    "OR",
    "NOT",
    "LIKE",
    "ILIKE",
    "IN",
    "IS",
    "NULL",
    "BETWEEN",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "ESCAPE",
    "TRUE",
    "FALSE",
    "ANY",
    "ALL",
    "SOME",
    "DISTINCT",
    "AS",
    "Q",
    "REGEXP",
    "RLIKE",
    "GLOB",
    "DATE",
    "TIMESTAMP",
    "INTERVAL",
    "COLLATE",
    "SIMILAR",
    "TO",
];

/// 조건 자리에 올 수 없는 문장 낱말(다른 문장 끼워 넣기 방지).
const STATEMENT_WORDS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE", "GRANT",
    "REVOKE", "MERGE", "UNION", "WITH", "EXEC", "EXECUTE", "CALL", "BEGIN", "COMMIT", "ROLLBACK",
];

pub(crate) struct CondBar {
    tb: TextBox,
    copy: CopyBtn,
    menu: CtxMenu,
    rect: Rect,
    scale: f32,
    /// 결과 열 이름(완성 후보 · 검증).
    cols: Vec<String>,
    /// 완성 중인 낱말의 글자 범위 `[start, caret)`.
    comp: Option<(usize, usize)>,
    /// Enter로 확정된 조건 글(호스트가 가져가 감싸서 실행).
    run: Option<String>,
    /// 복사 버튼 눌림(호스트가 감싼 SQL을 클립보드로).
    copy_req: bool,
    /// 연결된 탭의 방언(검증 · 함수 후보).
    dialect: Dialect,
    /// 마지막 검증 오류(상자 테두리 빨강 · 글이 바뀌면 지움) · 호스트에 한 번 알렸는가(상태줄 1회).
    err: Option<String>,
    err_reported: bool,
    /// 멀티라인 모드(▾ 버튼 · 사용자 10-06) · 그 높이(px · 0 = 기본 3줄) · 스플리터 드래그(누른 y · 그때 높이).
    multi: bool,
    multi_h: i32,
    drag_h: Option<(i32, i32)>,
    /// 모드 버튼(▾/▴) 자리 · 완성 팝업이 들어갈 호스트(창).
    btn_mode: Rect,
    host: Rect,
}

impl Default for CondBar {
    fn default() -> Self {
        Self::new()
    }
}

impl CondBar {
    pub(crate) fn new() -> Self {
        let mut tb = TextBox::new(t(Msg::CondPlaceholder)).with_clearable();
        tb.set_focus_ring(false);
        CondBar {
            tb,
            copy: CopyBtn::new(),
            menu: CtxMenu::new(),
            rect: Rect::default(),
            scale: 1.0,
            cols: Vec::new(),
            comp: None,
            run: None,
            copy_req: false,
            dialect: Dialect::Sqlite,
            err: None,
            err_reported: false,
            multi: false,
            multi_h: 0,
            drag_h: None,
            btn_mode: Rect::default(),
            host: Rect::default(),
        }
    }

    /// 완성 팝업이 들어갈 영역(창 전체 · 호스트가 그리기 전에).
    pub(crate) fn set_host(&mut self, host: Rect) {
        self.host = host;
    }

    /// 조건 바가 원하는 높이 — 한 줄 = 26 · 멀티라인 = 기본 3줄(+ 스플리터) 또는 끌어 둔 높이.
    pub(crate) fn wanted_height(&self, scale: f32) -> i32 {
        let s = |v: f32| (v * scale).round() as i32;
        if !self.multi {
            return s(26.0);
        }
        if self.multi_h > 0 {
            self.multi_h
        } else {
            s(20.0) * 3 + s(8.0) + s(6.0)
        }
    }

    /// 한 줄 ↔ 멀티라인(글·포커스 유지 · 상자는 다시 만든다 — 모드는 생성 때만 정해진다).
    pub(crate) fn toggle_multi(&mut self) {
        let text = self.tb.text();
        let focused = self.tb.is_focused();
        self.multi = !self.multi;
        let mut tb = TextBox::new(t(Msg::CondPlaceholder));
        if self.multi {
            tb = tb.with_multiline();
        } else {
            tb = tb.with_clearable();
        }
        tb.set_focus_ring(false);
        tb.set_text(&text);
        tb.set_focused(focused);
        self.tb = tb;
        self.menu.close();
        self.comp = None;
        self.drag_h = None;
    }

    #[cfg(test)]
    pub(crate) fn is_multi(&self) -> bool {
        self.multi
    }

    /// 멀티라인 아래 가장자리(스플리터 · 6 px).
    fn splitter_rect(&self) -> Rect {
        let h = (6.0 * self.scale).round() as i32;
        Rect::new(self.rect.x, self.rect.bottom() - h, self.rect.w, h)
    }

    /// 결과가 바뀌면 열 이름을 새로(완성 후보 · 검증).
    pub(crate) fn set_columns(&mut self, cols: Vec<String>) {
        self.cols = cols;
    }

    /// 연결된 탭의 방언(검증·함수 후보의 기준).
    pub(crate) fn set_dialect(&mut self, d: Dialect) {
        self.dialect = d;
    }

    /// 자리(필터 줄 사각형) — 오른쪽 끝은 복사 버튼.
    pub(crate) fn set_rect(&mut self, r: Rect, scale: f32) {
        self.rect = r;
        self.scale = scale;
        let s = |v: f32| (v * scale).round() as i32;
        let pad = s(4.0);
        let split = if self.multi { s(6.0) } else { 0 };
        let bh = s(18.0);
        // 오른쪽 = [▾ 모드] [복사] (멀티라인이면 위쪽에 맞춘다).
        let btn = Rect::new(r.right() - pad - bh, r.y + pad, bh, bh);
        self.copy.set_rect(btn);
        self.btn_mode = Rect::new(btn.x - s(4.0) - bh, r.y + pad, bh, bh);
        let mut inv = Invalidations::default();
        self.tb.set_scale(scale);
        self.tb.set_bounds(
            Rect::new(
                r.x + pad,
                r.y + pad,
                (self.btn_mode.x - s(6.0) - (r.x + pad)).max(s(40.0)),
                (r.h - pad * 2 - split).max(s(16.0)),
            ),
            &mut inv,
        );
    }

    pub(crate) fn is_focused(&self) -> bool {
        self.tb.is_focused()
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.tb.set_focused(on);
        if !on {
            self.menu.close();
            self.comp = None;
        }
    }

    pub(crate) fn textbox_mut(&mut self) -> &mut TextBox {
        &mut self.tb
    }

    pub(crate) fn text(&self) -> String {
        self.tb.text()
    }

    pub(crate) fn set_text(&mut self, s: &str) {
        self.tb.set_text(s);
        self.menu.close();
        self.comp = None;
        self.err = None;
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// Enter로 확정된 조건(한 번).
    pub(crate) fn take_run(&mut self) -> Option<String> {
        self.run.take()
    }

    /// 지금 글로 실행 요청(Enter · 기동 명령 `grid.cond.run`) — 검증을 거친다 · 틀리면 오류만 남긴다.
    pub(crate) fn request_run(&mut self) {
        let text = self.tb.text();
        match validate(&text, &self.cols, self.dialect) {
            Ok(()) => {
                self.err = None;
                self.run = Some(text);
            }
            Err(e) => {
                self.err = Some(e);
                self.err_reported = false;
            }
        }
    }

    pub(crate) fn take_copy_req(&mut self) -> bool {
        std::mem::take(&mut self.copy_req)
    }

    /// 새 검증 오류를 **한 번** 알린다(호스트 상태줄) — 오류 자체는 남아 테두리·덤프에 보인다(글이 바뀌면 지움 · 협업 V1 c5/c6).
    pub(crate) fn take_error(&mut self) -> Option<String> {
        if self.err_reported {
            return None;
        }
        self.err_reported = true;
        self.err.clone()
    }

    /// 애니메이션(복사 눌림 · 캐럿) 중인가 — 호스트가 다음 프레임을 예약.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        let a = self.tb.tick(now_ms);
        let b = self.copy.next_tick(Instant::now()).is_some();
        a || b
    }

    /// 상자 글이 상자 밖 길(IME 확정·붙여넣기)로 바뀐 뒤 — 완성 후보를 다시.
    pub(crate) fn query_changed(&mut self) {
        let _ = self.tb.take_changed();
        self.refresh_completion();
    }

    /// 캐럿 앞 낱말(식별자 글자 · `.` 포함) 범위.
    fn word_at_caret(&self) -> (usize, usize) {
        let text: Vec<char> = self.tb.text().chars().collect();
        let caret = self.tb.caret().min(text.len());
        let mut start = caret;
        while start > 0
            && (text[start - 1].is_alphanumeric()
                || matches!(text[start - 1], '_' | '$' | '#' | '.'))
        {
            start -= 1;
        }
        (start, caret)
    }

    /// 완성 후보 = 접두로 시작하는 열(원래 순서) → 접두를 품은 열 → 연산 낱말 → 방언 함수(접두 2자부터 · `NAME(`). 접두가 비면 없음.
    fn candidates(&self, prefix: &str) -> Vec<String> {
        if prefix.is_empty() {
            return Vec::new();
        }
        let p = prefix.to_lowercase();
        let mut out: Vec<String> = Vec::new();
        for c in &self.cols {
            if c.to_lowercase().starts_with(&p) {
                out.push(c.clone());
            }
        }
        for c in &self.cols {
            if !out.contains(c) && c.to_lowercase().contains(&p) {
                out.push(c.clone());
            }
        }
        for k in KEYWORDS {
            if k.to_lowercase().starts_with(&p) {
                out.push((*k).to_string());
            }
        }
        if p.chars().count() >= 2 {
            for f in nsql_script::builtins::functions(Some(self.dialect)) {
                if f.name.to_lowercase().starts_with(&p) {
                    out.push(format!("{}(", f.name));
                }
            }
        }
        out.truncate(12);
        out
    }

    /// 완성 팝업 갱신(글이 바뀐 뒤) — 후보가 없으면 닫는다.
    fn refresh_completion(&mut self) {
        let (start, caret) = self.word_at_caret();
        let prefix: String = self
            .tb
            .text()
            .chars()
            .skip(start)
            .take(caret - start)
            .collect();
        let items = self.candidates(&prefix);
        if items.is_empty() || !self.tb.is_focused() {
            self.menu.close();
            self.comp = None;
            return;
        }
        self.comp = Some((start, caret));
        // ★ 팝업은 입력 글을 가리지 않는다(사용자 10-06): 조건 바 **아래**(그리드 쪽) · 아래에 자리가 없으면 조건 바 **위**.
        //   가로 = 캐럿 x(접두 시작 근처) · 호스트 = 창(그리드가 준다 · 모르면 조건 바 아래 400).
        let x = self.tb.caret_point().map_or(self.tb.bounds().x, |p| {
            p.x - (prefix.chars().count() as i32 * 7)
        });
        let host = if self.host.w > 0 && self.host.h > 0 {
            self.host
        } else {
            Rect::new(
                self.rect.x,
                self.rect.y,
                self.rect.w,
                (400.0 * self.scale) as i32,
            )
        };
        let est_h = items.len() as i32 * (24.0 * self.scale) as i32 + (8.0 * self.scale) as i32;
        let y = if self.rect.bottom() + est_h <= host.bottom() {
            self.rect.bottom()
        } else {
            (self.rect.y - est_h).max(host.y)
        };
        let entries: Vec<CtxItem> = items
            .iter()
            .map(|c| CtxItem::item(c.clone(), c.clone()))
            .collect();
        self.menu.set_scale(self.scale);
        self.menu.open_at(
            x.max(self.rect.x),
            y,
            entries,
            host,
            (160.0 * self.scale) as i32,
        );
        self.menu.select(0);
    }

    /// 후보 넣기 — 완성 중 낱말을 바꾼다.
    fn accept(&mut self, pick: &str, inv: &mut Invalidations) {
        if let Some((a, b)) = self.comp.take() {
            self.tb.replace_range(a, b, pick, inv);
        }
        self.menu.close();
        let _ = self.tb.take_changed();
    }

    /// 사건 — 돌려주는 값 = 먹었는가. 상자 밖 MouseDown은 포커스를 거두고 **false**(그리드로 간다).
    pub(crate) fn on_event(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> bool {
        if self.rect.h <= 0 {
            return false;
        }
        // 완성 팝업이 떠 있으면 이동·확정·닫기·마우스만 팝업이(글자·Backspace는 상자로 가서 다시 거른다) · 바깥 클릭은 닫고 통과.
        if self.menu.is_open() {
            if let InputEvent::Key {
                key: Key::Escape, ..
            } = ev
            {
                self.menu.close();
                self.comp = None;
                return true;
            }
            // Tab(글자 '\t'로 온다 · input.rs 번역) = 선택 후보 넣기.
            if matches!(ev, InputEvent::Char { c: '\t', .. }) {
                let pick = self
                    .menu
                    .hovered()
                    .and_then(|i| self.menu.item_ids().get(i).map(|s| (*s).to_string()));
                if let Some(p) = pick {
                    self.accept(&p, inv);
                }
                return true;
            }
            let nav = matches!(
                ev,
                InputEvent::Key {
                    key: Key::Up | Key::Down | Key::Enter | Key::PageUp | Key::PageDown,
                    ..
                } | InputEvent::MouseMove { .. }
                    | InputEvent::MouseDown { .. }
                    | InputEvent::MouseUp { .. }
                    | InputEvent::Wheel { .. }
            );
            if self.menu.is_outside_click(ev) {
                self.menu.close();
                self.comp = None;
            } else if nav && self.menu.on_event(ev) {
                if let Some(p) = self.menu.take_picked() {
                    self.accept(&p, inv);
                }
                return true;
            }
        }
        // 상자의 우클릭 편집 메뉴가 떠 있으면(상자 밖 아래에 그려진다) 마우스 사건은 전부 상자로(항목이 안 눌리던 결함 · 사용자 10-06).
        if self.tb.popup_open()
            && matches!(
                ev,
                InputEvent::MouseMove { .. }
                    | InputEvent::MouseDown { .. }
                    | InputEvent::MouseUp { .. }
                    | InputEvent::RightDown { .. }
            )
        {
            self.tb.on_event(ev, inv);
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                if let Some((y0, h0)) = self.drag_h {
                    // 스플리터 드래그 = 높이(2줄 ~ 12줄).
                    let s = |v: f32| (v * self.scale).round() as i32;
                    let (lo, hi) = (s(20.0) * 2 + s(14.0), s(20.0) * 12 + s(14.0));
                    self.multi_h = (h0 + (y - y0)).clamp(lo, hi);
                    return true;
                }
                self.copy.set_hover(self.copy.hit(p));
                if self.rect.contains(p) || self.tb.is_dragging() {
                    self.tb.on_event(ev, inv);
                }
                self.rect.contains(p)
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if std::env::var_os("NSQL_TRACE_COND").is_some() {
                    eprintln!(
                        "[cond] down p={},{} rect={:?} mode={:?} copy={:?} multi={}",
                        x, y, self.rect, self.btn_mode, self.copy.rect, self.multi
                    );
                }
                if self.copy.hit(p) {
                    self.copy.press(Instant::now());
                    self.copy_req = true;
                    return true;
                }
                if self.btn_mode.contains(p) {
                    self.toggle_multi();
                    return true;
                }
                if self.multi && self.splitter_rect().contains(p) {
                    self.drag_h = Some((y, self.wanted_height(self.scale)));
                    return true;
                }
                if self.rect.contains(p) {
                    self.tb.on_event(ev, inv);
                    return true;
                }
                if self.tb.is_focused() {
                    self.set_focused(false);
                }
                false
            }
            InputEvent::MouseUp { x, y } => {
                if self.drag_h.take().is_some() {
                    return true;
                }
                let p = Point { x, y };
                if self.rect.contains(p) || self.tb.is_dragging() {
                    self.tb.on_event(ev, inv);
                    return true;
                }
                false
            }
            InputEvent::RightDown { x, y } => {
                let p = Point { x, y };
                if self.rect.contains(p) {
                    self.tb.on_event(ev, inv);
                    return true;
                }
                false
            }
            InputEvent::Key { .. }
            | InputEvent::Char { .. }
            | InputEvent::SelectAll
            | InputEvent::Undo
            | InputEvent::Redo
                if self.tb.is_focused() =>
            {
                if let InputEvent::Key {
                    key: Key::Escape, ..
                } = ev
                {
                    self.set_focused(false);
                    return true;
                }
                // 멀티라인 = Enter는 줄 바꿈 · **Ctrl/⌘+Enter** = 실행.
                if self.multi {
                    if let InputEvent::Key {
                        key: Key::Enter,
                        primary: true,
                        ..
                    } = ev
                    {
                        self.menu.close();
                        self.comp = None;
                        self.request_run();
                        return true;
                    }
                }
                self.tb.on_event(ev, inv);
                if !self.multi && self.tb.take_committed().is_some() {
                    // Enter = 검증 → 실행(완성 팝업이 없을 때) · 틀리면 실행하지 않고 오류만.
                    self.menu.close();
                    self.comp = None;
                    self.request_run();
                    return true;
                }
                if self.tb.take_changed().is_some() {
                    self.err = None;
                    self.refresh_completion();
                }
                true
            }
            _ => false,
        }
    }

    /// 조건 바 그리기(그리드 위 한 줄) — 바탕 + 상자(오류 = 빨간 테두리) + 복사 버튼.
    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme, now: Instant) {
        if self.rect.h <= 0 {
            return;
        }
        dc.fill_rect(self.rect, th.chrome_bg);
        dc.fill_rect(
            Rect::new(self.rect.x, self.rect.bottom() - 1, self.rect.w, 1),
            th.border,
        );
        self.tb.paint(dc, th);
        if self.err.is_some() {
            dc.stroke_round_rect(self.tb.bounds(), (4.0 * self.scale) as i32, th.danger, 1.5);
        }
        self.copy.paint(dc, th, 1.0, self.scale, now);
        // 모드 버튼 = ▾(한 줄 → 멀티라인) / ▴(돌아가기) · 복사 버튼과 같은 상자 꼴.
        let b = self.btn_mode;
        dc.fill_round_rect_alpha(b, (3.0 * self.scale) as i32, th.text_dim, 0.12);
        let g = (b.w / 2).max(4);
        let (cx, cy) = (b.x + b.w / 2, b.y + b.h / 2);
        if self.multi {
            dc.fill_triangle(
                (cx - g / 2, cy + g / 4),
                (cx + g / 2, cy + g / 4),
                (cx, cy - g / 4),
                th.text,
            );
        } else {
            dc.fill_triangle(
                (cx - g / 2, cy - g / 4),
                (cx + g / 2, cy - g / 4),
                (cx, cy + g / 4),
                th.text,
            );
        }
        if self.multi {
            // 스플리터 = 아래 가장자리 가운데 짧은 손잡이.
            let sp = self.splitter_rect();
            let w = (40.0 * self.scale) as i32;
            dc.fill_round_rect_alpha(
                Rect::new(sp.x + (sp.w - w) / 2, sp.y + sp.h / 2 - 1, w, 2),
                1,
                th.text_dim,
                0.6,
            );
        }
    }

    /// 팝업 층(상자 편집 메뉴 · 완성 팝업).
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.tb.paint_popup(dc, th);
        self.menu.paint(dc, th);
    }

    /// 자체 시험 덤프: `text=… focused=… menu=[a|b|…] err=…`.
    pub(crate) fn dump(&self) -> String {
        let items: Vec<String> = if self.menu.is_open() {
            self.menu
                .item_ids()
                .into_iter()
                .map(str::to_string)
                .collect()
        } else {
            Vec::new()
        };
        let m = self.btn_mode;
        let c = self.copy.rect;
        format!(
            "text={} focused={} menu=[{}] err={} multi={} h={} mode={},{},{},{} copy={},{},{},{}",
            self.tb.text(),
            self.tb.is_focused(),
            items.join("|"),
            self.err.as_deref().unwrap_or(""),
            self.multi,
            self.wanted_height(self.scale),
            m.x,
            m.y,
            m.w,
            m.h,
            c.x,
            c.y,
            c.w,
            c.h
        )
    }
}

/// 출처 문장을 조건으로 감싼다(순수 · 조건이 비면 출처 그대로) — `SELECT * FROM ( <출처> ) q WHERE 1=1 AND (<조건>)`.
pub(crate) fn wrap_condition(source: &str, cond: &str) -> String {
    let src = source.trim().trim_end_matches(';').trim();
    let c = cond.trim().trim_end_matches(';').trim();
    if c.is_empty() {
        return src.to_string();
    }
    format!("SELECT *\nFROM (\n{src}\n) q\nWHERE 1=1\nAND ({c})")
}

/// 식 토큰(검증용 · 순수).
#[derive(Debug, PartialEq, Eq)]
enum Tok {
    /// 식별자(따옴표 없음 · `.` 포함) · 뒤에 `(`가 오는지.
    Ident(String, bool),
    /// `"…"` `[…]` `` `…` `` 인용 식별자(안의 글).
    QIdent(String),
    Other,
}

/// 식을 토큰으로(문자열 리터럴·숫자·연산자는 `Other`) — 괄호·따옴표 짝과 `;`·`::`도 여기서 판정해 오류로.
fn tokenize(cond: &str, dialect: Dialect) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = cond.chars().collect();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut out = Vec::new();
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                // 문자열: `''` = 따옴표 하나.
                let mut j = i + 1;
                loop {
                    if j >= cs.len() {
                        return Err(t(Msg::CondErrQuote).to_string());
                    }
                    if cs[j] == '\'' {
                        if j + 1 < cs.len() && cs[j + 1] == '\'' {
                            j += 2;
                            continue;
                        }
                        break;
                    }
                    j += 1;
                }
                out.push(Tok::Other);
                i = j + 1;
            }
            '"' | '[' | '`' => {
                let close = match c {
                    '[' => ']',
                    other => other,
                };
                let mut j = i + 1;
                while j < cs.len() && cs[j] != close {
                    j += 1;
                }
                if j >= cs.len() {
                    return Err(t(Msg::CondErrQuote).to_string());
                }
                out.push(Tok::QIdent(cs[i + 1..j].iter().collect()));
                i = j + 1;
            }
            '(' => {
                depth += 1;
                out.push(Tok::Other);
                i += 1;
            }
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(t(Msg::CondErrParen).to_string());
                }
                out.push(Tok::Other);
                i += 1;
            }
            ';' => return Err(t(Msg::CondErrStatement).to_string()),
            ':' if i + 1 < cs.len() && cs[i + 1] == ':' => {
                if dialect != Dialect::Postgres {
                    return Err(t(Msg::CondErrPgCast).to_string());
                }
                out.push(Tok::Other);
                i += 2;
                // 뒤의 형 이름(`::text` `::numeric(10,2)`)은 열이 아니다 — 식별자 글자를 통째로 건너뛴다.
                while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                    i += 1;
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut j = i;
                while j < cs.len()
                    && (cs[j].is_alphanumeric() || matches!(cs[j], '_' | '$' | '#' | '.'))
                {
                    j += 1;
                }
                let word: String = cs[i..j].iter().collect();
                let mut k = j;
                while k < cs.len() && cs[k].is_whitespace() {
                    k += 1;
                }
                let call = k < cs.len() && cs[k] == '(';
                out.push(Tok::Ident(word, call));
                i = j;
            }
            _ => {
                out.push(Tok::Other);
                i += 1;
            }
        }
    }
    if depth != 0 {
        return Err(t(Msg::CondErrParen).to_string());
    }
    Ok(out)
}

/// 조건식 검증(순수 · 방언 기준) — 비면 OK(= 조건 없음). 오류 = 사용자에게 보일 한 줄.
pub(crate) fn validate(cond: &str, cols: &[String], dialect: Dialect) -> Result<(), String> {
    let c = cond.trim();
    if c.is_empty() {
        return Ok(());
    }
    let toks = tokenize(c, dialect)?;
    let is_col = |name: &str| -> bool {
        // `q.열`(감싼 서브쿼리 별칭)도 허용 · 그 밖의 한정자(`A.열`)는 바깥 WHERE에서 안 보인다.
        let bare = name
            .strip_prefix("q.")
            .or_else(|| name.strip_prefix("Q."))
            .unwrap_or(name);
        !bare.contains('.') && cols.iter().any(|col| col.eq_ignore_ascii_case(bare))
    };
    let funcs = nsql_script::builtins::functions(Some(dialect));
    let is_func = |name: &str| funcs.iter().any(|f| f.name.eq_ignore_ascii_case(name));
    for tk in &toks {
        match tk {
            Tok::Ident(w, call) => {
                let up = w.to_ascii_uppercase();
                if STATEMENT_WORDS.contains(&up.as_str()) {
                    return Err(t(Msg::CondErrStatement).to_string());
                }
                if ALLOWED_WORDS.contains(&up.as_str()) || is_col(w) {
                    continue;
                }
                if *call {
                    if is_func(w) {
                        continue;
                    }
                    return Err(tf(Msg::CondErrFunc, &[w]));
                }
                return Err(tf(Msg::CondErrIdent, &[w]));
            }
            Tok::QIdent(w) => {
                if !is_col(w) {
                    return Err(tf(Msg::CondErrIdent, &[w]));
                }
            }
            Tok::Other => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_rules() {
        assert_eq!(wrap_condition("SELECT * FROM t;", ""), "SELECT * FROM t");
        assert_eq!(
            wrap_condition("SELECT * FROM t;", "a = 1 OR b LIKE 'x%';"),
            "SELECT *\nFROM (\nSELECT * FROM t\n) q\nWHERE 1=1\nAND (a = 1 OR b LIKE 'x%')"
        );
    }

    /// 완성 후보 = 접두 시작 열 → 품은 열 → 낱말 → 방언 함수(접두 2자부터 · `NAME(`) · 접두 없으면 없음 · 상한 12.
    #[test]
    fn candidates_order() {
        let mut c = CondBar::new();
        c.set_columns(vec![
            "ITEM_CD".into(),
            "ITEM_NM".into(),
            "PROJECT_CD".into(),
        ]);
        assert_eq!(c.candidates("it"), vec!["ITEM_CD", "ITEM_NM"]);
        assert_eq!(c.candidates("_cd"), vec!["ITEM_CD", "PROJECT_CD"]);
        assert_eq!(
            c.candidates("i"),
            vec!["ITEM_CD", "ITEM_NM", "IN", "IS NULL", "IS NOT NULL"]
        );
        assert!(c.candidates("").is_empty());
        c.set_dialect(Dialect::Oracle);
        assert!(
            c.candidates("nv").iter().any(|s| s == "NVL("),
            "{:?}",
            c.candidates("nv")
        );
    }

    /// 검증(방언 기준): 열·낱말·함수·`q.열`은 통과 · 모르는 열/함수 · 괄호·따옴표 짝 · `;` · 문장 낱말 · PG 밖 `::`는 오류.
    #[test]
    fn validate_rules() {
        let cols: Vec<String> = vec!["ITEM_CD".into(), "ITEM_NM".into()];
        let ok = |s: &str, d: Dialect| validate(s, &cols, d).is_ok();
        assert!(ok("", Dialect::Oracle));
        assert!(ok(
            "ITEM_CD LIKE 'MH%' OR item_nm = 'a''b'",
            Dialect::Oracle
        ));
        assert!(ok(
            "q.ITEM_CD IN ('A', 'B') AND ITEM_NM IS NOT NULL",
            Dialect::Postgres
        ));
        assert!(ok(
            "UPPER(ITEM_NM) = 'X' AND NVL(ITEM_CD, 'z') <> 'z'",
            Dialect::Oracle
        ));
        assert!(ok("ITEM_CD::text = '1'", Dialect::Postgres));
        assert!(ok("\"ITEM_CD\" = 1", Dialect::Postgres));
        assert!(!ok("ITEM_CD::text = '1'", Dialect::Oracle), "PG 전용 ::");
        assert!(!ok("NO_SUCH = 1", Dialect::Oracle));
        assert!(!ok("A.ITEM_CD = 1", Dialect::Oracle), "다른 한정자");
        assert!(!ok("FOO(ITEM_CD) = 1", Dialect::Oracle), "모르는 함수");
        assert!(!ok("ITEM_CD = 'x", Dialect::Oracle), "따옴표 짝");
        assert!(!ok("(ITEM_CD = 1", Dialect::Oracle), "괄호 짝");
        assert!(!ok("ITEM_CD = 1; DROP TABLE t", Dialect::Oracle), ";");
        assert!(!ok("SELECT 1", Dialect::Oracle), "문장");
    }

    /// 멀티라인: ▾ = 3줄 높이(+스플리터) · 글·포커스 유지 · Enter는 줄 바꿈이고 Ctrl+Enter = 실행 · ▴ = 한 줄로.
    #[test]
    fn multiline_toggle_and_ctrl_enter() {
        let mut c = CondBar::new();
        c.set_columns(vec!["ITEM_CD".into()]);
        c.set_rect(Rect::new(0, 0, 400, 26), 1.0);
        c.set_focused(true);
        c.set_text("ITEM_CD = 1");
        assert_eq!(c.wanted_height(1.0), 26);
        c.toggle_multi();
        assert!(c.is_multi() && c.is_focused());
        assert_eq!(c.text(), "ITEM_CD = 1");
        assert_eq!(c.wanted_height(1.0), 20 * 3 + 8 + 6);
        c.set_rect(Rect::new(0, 0, 400, c.wanted_height(1.0)), 1.0);
        let mut inv = Invalidations::default();
        let plain = InputEvent::Key {
            key: Key::Enter,
            shift: false,
            primary: false,
        };
        c.on_event(&plain, &mut inv);
        assert!(c.take_run().is_none(), "멀티라인 Enter = 줄 바꿈");
        let run = InputEvent::Key {
            key: Key::Enter,
            shift: false,
            primary: true,
        };
        c.on_event(&run, &mut inv);
        assert!(c.take_run().is_some(), "Ctrl+Enter = 실행");
        c.toggle_multi();
        assert!(!c.is_multi());
        assert_eq!(c.wanted_height(1.0), 26);
    }

    /// ▾ 버튼 클릭(MouseDown) = 멀티라인 토글 · 상자 안 클릭 = 포커스 · 버튼 자리는 복사 버튼 왼쪽.
    #[test]
    fn mode_button_click_toggles() {
        let mut c = CondBar::new();
        c.set_rect(Rect::new(0, 0, 400, 26), 1.0);
        let m = c.btn_mode;
        assert!(
            m.w > 0 && m.right() < c.copy.rect.x,
            "{m:?} {:?}",
            c.copy.rect
        );
        let mut inv = Invalidations::default();
        let hit = c.on_event(
            &InputEvent::MouseDown {
                x: m.x + m.w / 2,
                y: m.y + m.h / 2,
                shift: false,
                primary: false,
            },
            &mut inv,
        );
        assert!(hit && c.is_multi(), "{}", c.dump());
        let d = c.dump();
        assert!(d.contains("mode=") && d.contains("copy="));
    }

    /// 글자를 치면 완성 팝업(글자는 상자로 계속) · Tab = 넣기 · Enter = 검증 뒤 실행 요청 · 틀린 식 = 오류만 · Esc = 포커스 거둠.
    #[test]
    fn typing_completion_and_enter_runs() {
        let mut c = CondBar::new();
        c.set_columns(vec!["ITEM_CD".into(), "PROJECT_CD".into()]);
        c.set_dialect(Dialect::Oracle);
        c.set_rect(Rect::new(0, 0, 400, 26), 1.0);
        c.set_focused(true);
        let mut inv = Invalidations::default();
        for ch in "ite".chars() {
            assert!(c.on_event(&InputEvent::Char { c: ch, now_ms: 0 }, &mut inv));
        }
        assert_eq!(c.text(), "ite");
        assert!(c.menu_open(), "{}", c.dump());
        assert!(c.on_event(&InputEvent::Char { c: '\t', now_ms: 0 }, &mut inv));
        assert_eq!(c.text(), "ITEM_CD");
        assert!(!c.menu_open());
        for ch in " = 1".chars() {
            c.on_event(&InputEvent::Char { c: ch, now_ms: 0 }, &mut inv);
        }
        let enter = InputEvent::Key {
            key: Key::Enter,
            shift: false,
            primary: false,
        };
        c.on_event(&enter, &mut inv);
        assert_eq!(c.take_run().as_deref(), Some("ITEM_CD = 1"));
        assert!(c.take_error().is_none());
        c.set_text("NO_SUCH = 1");
        c.set_focused(true);
        c.on_event(&enter, &mut inv);
        assert!(c.take_run().is_none(), "틀린 식은 실행 안 함");
        assert!(c.take_error().is_some());
        c.on_event(
            &InputEvent::Key {
                key: Key::Escape,
                shift: false,
                primary: false,
            },
            &mut inv,
        );
        assert!(!c.is_focused());
    }
}
