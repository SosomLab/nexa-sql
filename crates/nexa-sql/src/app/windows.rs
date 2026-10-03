//! App — 보조 창 열기·모달·창 목록(로그·트랜잭션 로그·변수·세션).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 지금 열린 모달 창(접속 · 파일 · 비밀번호 입력) — 사건 가드·`sync_modal`이 같은 판정을 쓴다.
    pub(crate) fn modal_window(&self) -> Option<&Window> {
        if self.input_win.is_modal() {
            return self.input_win.window();
        }
        self.file_win
            .window()
            .or_else(|| self.conn_win.window())
            .or_else(|| self.sqlprev_win.window())
            .or_else(|| self.import_win.window())
            // 라이선스·About 창 = 모달(열려 있는 동안 메인 잠금 · 사용자 09-27).
            .or_else(|| self.license_win.window())
            .or_else(|| self.about_win.window())
            .or_else(|| self.drop_win.window())
    }

    pub(crate) fn modal_open(&self) -> bool {
        self.conn_win.is_open()
            || self.file_win.is_open()
            || self.input_win.is_modal()
            || self.sqlprev_win.is_open()
            || self.import_win.is_open()
            || self.license_win.is_open()
            || self.about_win.is_open()
            || self.drop_win.is_open()
    }

    /// 모달 창(접속 · 파일 · 비밀번호 입력) 열림/닫힘 전환 → 메인 창 활성 상태 동기화(닫히면 메인으로 포커스).
    pub(crate) fn sync_modal(&mut self) {
        let open = self.modal_open();
        if open == self.conn_modal && !open {
            return;
        }
        self.conn_modal = open;
        if !open {
            // 닫힌 모달 창의 WindowId는 z-order 목록에서 걷어낸다(열 때마다 새 id → 남겨 두면 한 칸씩 자란다 · 09-15 누수 점검).
            let live: Vec<WindowId> = self.all_windows().into_iter().map(Window::id).collect();
            self.z_order.retain(|id| live.contains(id));
        }
        // 메인 창 + 그 일부로 보는 보조 창 전부(로그 · 색 · 단축키 · 설정).
        let others: Vec<&Window> = [
            self.window.as_deref(),
            self.log_win.window(),
            self.colors_win.window(),
            self.keys_win.window(),
            self.prefs_win.window(),
        ]
        .into_iter()
        .flatten()
        .collect();
        for w in &others {
            winfocus::set_enabled(w, !open);
        }
        if !open {
            // 파일 창을 띄운 보조 창(라이선스 창 · 설정 창)이 살아 있으면 거기로 · 아니면 메인으로(09-27).
            let back = self.picker_return.take().and_then(|id| self.aux_window(id));
            if let Some(w) = back.or(self.window.as_deref()) {
                w.focus_window();
            }
        }
    }

    /// 창 기하 기억(사용자 09-17 규칙): 닫힌 보조 창의 마지막 (위치, 크기)를 설정에 · `main`이면 메인 창도(종료 직전).
    pub(crate) fn persist_window_sizes(&mut self, main: bool) {
        let mut changed = false;
        let mut put = |settings: &mut Settings, name: &str, g: Option<((i32, i32), (f64, f64))>| {
            if let Some(((x, y), (w, h))) = g {
                for (key, v) in [
                    (format!("window.{name}_pos"), wingeom::format_pos(x, y)),
                    (format!("window.{name}_size"), wingeom::format_size(w, h)),
                ] {
                    if settings.get(&key) != Some(v.as_str()) {
                        let _ = settings.set(&key, &v);
                        changed = true;
                    }
                }
            }
        };
        let g1 = self.conn_win.take_last();
        let g2 = self.log_win.take_last();
        let g3 = self.txlog_win.take_last();
        let g4 = self.prefs_win.take_last();
        let g5 = self.sessions_win.take_last();
        let g6 = self.mem_win.take_last();
        put(&mut self.settings, "login", g1);
        put(&mut self.settings, "log", g2);
        put(&mut self.settings, "txlog", g3);
        put(&mut self.settings, "prefs", g4);
        put(&mut self.settings, "sessions", g5);
        put(&mut self.settings, "mem", g6);
        if main {
            let g = self
                .window
                .as_ref()
                .and_then(|w| wingeom::outer_pos(w).map(|p| (p, wingeom::logical_size(w))));
            put(&mut self.settings, "main", g);
        }
        if changed {
            let _ = self.settings.save();
            self.apply_window_sizes();
        }
    }

    /// 설정의 기억된 기하를 보조 창의 메모로(열 때 같은 모니터면 그대로 · 아니면 기본 규칙).
    pub(crate) fn apply_window_sizes(&mut self) {
        let memo = |s: &Settings, name: &str| wingeom::Memo {
            pos: s
                .get(&format!("window.{name}_pos"))
                .and_then(wingeom::parse_pos),
            size: s
                .get(&format!("window.{name}_size"))
                .and_then(wingeom::parse_size),
        };
        let (a, b, c, d) = (
            memo(&self.settings, "login"),
            memo(&self.settings, "log"),
            memo(&self.settings, "txlog"),
            memo(&self.settings, "prefs"),
        );
        self.conn_win.set_memo(a);
        self.log_win.set_memo(b);
        self.txlog_win.set_memo(c);
        self.prefs_win.set_memo(d);
        let e = memo(&self.settings, "sessions");
        self.sessions_win.set_memo(e);
        let f = memo(&self.settings, "mem");
        self.mem_win.set_memo(f);
    }

    /// 메인 창 항상 위(설정 `window.always_on_top`). 로그 창은 메인 창의 소유 창이라 둘 다 켜도 로그가 위(Windows/mac 소유 규칙 · 사용자 09-16).
    pub(crate) fn apply_on_top(&self) {
        if let Some(w) = &self.window {
            w.set_window_level(if self.settings.flag("window.always_on_top") {
                winit::window::WindowLevel::AlwaysOnTop
            } else {
                winit::window::WindowLevel::Normal
            });
        }
    }

    /// 앱의 모든 창(메인 + 보조 + 모달 · 열린 것만) — z-order 정리 · id → 창 조회의 단일 원천(09-27).
    fn all_windows(&self) -> Vec<&Window> {
        [
            self.window.as_deref(),
            self.log_win.window(),
            self.txlog_win.window(),
            self.sessions_win.window(),
            self.license_win.window(),
            self.about_win.window(),
            self.drop_win.window(),
            self.vars_win.window(),
            self.mem_win.window(),
            self.order_win.window(),
            self.colors_win.window(),
            self.keys_win.window(),
            self.prefs_win.window(),
            self.conn_win.window(),
            self.file_win.window(),
            self.input_win.window(),
            self.sqlprev_win.window(),
            self.import_win.window(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// 창 id → 창(닫혔으면 `None`).
    fn aux_window(&self, id: WindowId) -> Option<&Window> {
        self.all_windows().into_iter().find(|w| w.id() == id)
    }

    /// 창이 포커스를 받았다 — z-order 갱신 · `window.focus = group`이면 나머지 창을 활성화 없이 같이 올린다.
    pub(crate) fn on_window_focused(&mut self, id: WindowId) {
        self.z_order.retain(|w| *w != id);
        self.z_order.push(id);
        if self.settings.get("window.focus") == Some("single") {
            return;
        }
        let wins: Vec<&Window> = self
            .z_order
            .iter()
            .filter_map(|wid| self.aux_window(*wid))
            .collect();
        // ★ 보조 창(메모리 창 등)을 골라도 메인·다른 창이 함께 앞으로(사용자 09-24) — 맥은 `orderFront:` · 고른 창이 맨 위.
        winfocus::raise_group(&wins);
    }

    pub(crate) fn toggle_log_window(&mut self, el: &ActiveEventLoop) {
        if self.log_win.is_open() {
            self.log_win.close();
        } else {
            let near = self.window.as_ref().and_then(|w| {
                w.outer_position()
                    .ok()
                    .map(|p| (p.x, p.y, w.outer_size().width))
            });
            let owner = self.window.clone();
            self.log_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                near,
                owner.as_deref(),
            );
        }
    }

    /// 트랜잭션 로그 창(모덜리스 · docs/44 §2 · T-107) — 이미 열려 있으면 앞으로.
    /// 트랜잭션 로그 창 — **토글**(사용자 09-19: 열려 있으면 닫는다 · 로그 창·세션 창과 같은 규칙).
    pub(crate) fn open_txlog_window(&mut self, el: &ActiveEventLoop) {
        if self.txlog_win.is_open() {
            self.txlog_win.close();
            self.persist_window_sizes(false);
            return;
        }
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        let ctx = self.conn_win.active_name().to_string();
        self.txlog_win.set_active_editor(self.editors.active_id());
        self.txlog_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
            &ctx,
        );
    }

    /// 변수 창 열기/닫기(View ▸ Variables).
    pub(crate) fn open_vars_window(&mut self, el: &ActiveEventLoop) {
        if self.vars_win.is_open() {
            self.vars_win.close();
            return;
        }
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.vars_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
        self.vars_win_context();
    }

    /// 변수 창의 제목 = 지금 편집기 탭.
    pub(crate) fn vars_win_context(&mut self) {
        if !self.vars_win.is_open() {
            return;
        }
        let id = self.editors.active_id();
        let title = self
            .editors
            .tab_list()
            .into_iter()
            .find(|(t, _, _)| *t == id)
            .map(|(_, title, _)| title)
            .unwrap_or_default();
        self.vars_win.set_context(&title);
    }

    pub(crate) fn open_sessions_window(&mut self, el: &ActiveEventLoop) {
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.sessions_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
    }
}
