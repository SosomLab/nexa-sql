//! ★ 객체 찾기 창 호스트 쪽(docs/113 · 10-15): 열기 · 동작(검색 요청 → 탐색기 칸 방송 · 소스 탭 · 탐색기에서 보기 · 복사 · SELECT) ·
//! 설정 반영. 검색 자체는 `explorer.rs` 메타 스레드(`Req::Find`) · 응답 합치기는 창(`push_hits`).
use crate::findobj_win::FoAction;
use crate::*;
use nsql_i18n::{t, Msg};
use nsql_log::{LogEntry, LogKind};
use winit::event_loop::ActiveEventLoop;

impl App {
    /// 창 열기(이미 열려 있으면 앞으로 · `preset` = 미리 넣을 검색어).
    pub(crate) fn open_findobj_window(&mut self, el: &ActiveEventLoop, preset: Option<&str>) {
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.findobj_apply_settings();
        self.findobj_win.set_servers(self.explorer.find_servers());
        self.findobj_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
            preset,
        );
    }

    /// 메뉴·팔레트·키(이벤트 루프 밖) = 깃발 → 다음 `about_to_wait`에서 연다. 열려 있으면 앞으로(검색어 갱신).
    pub(crate) fn request_findobj(&mut self, preset: Option<String>) {
        self.open_findobj = Some(preset);
        self.redraw();
    }

    /// 설정 `findobj.*` → 창.
    pub(crate) fn findobj_apply_settings(&mut self) {
        let max = self.settings.int("findobj.max_hits").max(0) as usize;
        let per = self.settings.int("findobj.lines_per_object").max(0) as usize;
        self.findobj_win.set_limits(max, per);
        let tg = self
            .settings
            .get("findobj.targets")
            .unwrap_or("")
            .to_string();
        let kd = self.settings.get("findobj.kinds").unwrap_or("").to_string();
        self.findobj_win.set_remembered(&tg, &kd);
    }

    /// 창의 체크 상태를 설정에 남긴다(검색 때).
    fn findobj_remember(&mut self) {
        let (tg, kd) = self.findobj_win.remembered();
        let mut changed = false;
        for (k, v) in [("findobj.targets", tg), ("findobj.kinds", kd)] {
            if self.settings.get(k) != Some(v.as_str()) {
                let _ = self.settings.set(k, &v);
                changed = true;
            }
        }
        if changed {
            self.persist_settings();
        }
    }

    /// 창 동작 처리.
    pub(crate) fn findobj_action(&mut self, act: FoAction) {
        match act {
            FoAction::None | FoAction::Paint => {}
            FoAction::Close => {
                if self.findobj_win.is_running() {
                    let rev = self.findobj_win.rev();
                    self.explorer.find_stop(rev);
                }
                self.findobj_win.close();
                self.persist_window_sizes(false);
                self.redraw();
            }
            FoAction::Search(q, only) => {
                self.findobj_remember();
                let rev = self.findobj_win.rev();
                let n = self.explorer.find_objects(rev, &q, only.as_ref());
                self.findobj_win.started(n);
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    format!("[findobj] #{rev} \"{}\" servers={n}", q.pat.text),
                ));
            }
            FoAction::Stop(rev) => self.explorer.find_stop(rev),
            FoAction::OpenSource { server, obj, line } => {
                if line > 0 {
                    self.findobj_goto = Some((obj.name.clone(), line));
                }
                if !self.explorer.open_source_for(server.as_ref(), &obj) {
                    self.sess.status = t(Msg::ExpNotConnected).to_string();
                }
                if let Some(w) = &self.window {
                    crate::winfocus::focus(w);
                }
            }
            FoAction::Reveal { server, obj } => {
                let db = (!obj.db.is_empty()).then(|| obj.db.clone());
                let t = crate::explorer::RevealTarget {
                    db,
                    schema: obj.schema,
                    kind: obj.kind,
                    name: obj.name,
                    member: None,
                };
                if self.explorer.reveal(server.as_ref(), t) {
                    self.set_focus(Focus::Explorer);
                    if let Some(w) = &self.window {
                        crate::winfocus::focus(w);
                    }
                }
                self.redraw();
            }
            FoAction::Copy(s) => {
                if !clipboard::write_text(&s) {
                    self.sess.status = t(Msg::ErrClipboard).into();
                }
            }
            FoAction::GenSelect { server, obj } => {
                if !self.explorer.gen_select_for(server.as_ref(), &obj) {
                    self.sess.status = t(Msg::ExpNotConnected).to_string();
                }
            }
        }
    }
}
