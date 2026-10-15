//! ★ 객체 찾기 창 호스트 쪽(docs/113 · 10-15): 열기 · 동작(검색 → **전용 엔진** `findobj_engine` · 소스 탭 · 탐색기에서 보기 · 복사 · SELECT) ·
//! 설정 반영 · 엔진 메시지 펌프(`findobj_pump`). 검색은 창 전용 스레드들(서버마다 계획 + 워커 N · D-295 실시간 직접 조회) · 합치기는 창(`push_msg`).
use crate::findobj_win::FoAction;
use crate::*;
use nsql_i18n::{t, Msg};
use nsql_log::{LogEntry, LogKind};
use nsql_script::ConnectSpec;
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
        self.findobj_win
            .set_all_dbs(self.settings.flag("findobj.all_dbs"));
    }

    /// 창의 체크 상태를 설정에 남긴다(검색 때).
    fn findobj_remember(&mut self) {
        let (tg, kd) = self.findobj_win.remembered();
        let adb = if self.findobj_win.all_dbs() {
            "on"
        } else {
            "off"
        }
        .to_string();
        let mut changed = false;
        for (k, v) in [
            ("findobj.targets", tg),
            ("findobj.kinds", kd),
            ("findobj.all_dbs", adb),
        ] {
            if self.settings.get(k) != Some(v.as_str()) {
                let _ = self.settings.set(k, &v);
                changed = true;
            }
        }
        if changed {
            self.persist_settings();
        }
    }

    /// 엔진 메시지를 창에 넘긴다(매 `about_to_wait` · 창이 닫혔으면 버린다).
    pub(crate) fn findobj_pump(&mut self) {
        let open = self.findobj_win.is_open();
        while let Some(m) = self.findobj_engine.try_recv() {
            if open {
                self.findobj_win.push_msg(m);
            }
        }
        // ★ 열린 창의 서버 목록은 접속이 바뀌면 따라간다(협업 V1 ⑤ · 10-15) — 칸 몇 개 비교라 비용 0.
        if open {
            let now = self.explorer.find_servers();
            if now.as_slice() != self.findobj_win.servers() {
                self.findobj_win.set_servers(now);
                self.findobj_win.redraw();
            }
        }
    }

    /// 창 동작 처리.
    pub(crate) fn findobj_action(&mut self, act: FoAction) {
        match act {
            FoAction::None | FoAction::Paint => {}
            FoAction::Close => {
                if self.findobj_win.is_running() {
                    self.findobj_engine.cancel();
                }
                self.findobj_win.close();
                self.persist_window_sizes(false);
                self.redraw();
            }
            FoAction::Search(q, only) => {
                self.findobj_remember();
                let rev = self.findobj_win.rev();
                // 대상 = 연결된 서버 칸(한 서버만이면 그것만) · 엔진 `server` 번호 = 이 목록 차례 → 창에 같은 차례의 키를 알린다.
                let targets: Vec<crate::findobj_engine::ServerTarget> = self
                    .findobj_win
                    .servers()
                    .iter()
                    .filter_map(|(k, label)| {
                        let spec = k.clone()?;
                        if only.as_ref().is_some_and(|o| o != &spec) {
                            return None;
                        }
                        Some(crate::findobj_engine::ServerTarget {
                            db: self.explorer.current_db(Some(&spec)),
                            spec,
                            label: label.clone(),
                        })
                    })
                    .collect();
                let specs: Vec<ConnectSpec> = targets.iter().map(|t| t.spec.clone()).collect();
                let workers = self.settings.int("findobj.workers").clamp(1, 8) as usize;
                let n = self.findobj_engine.start(rev, *q.clone(), targets, workers);
                self.findobj_win.started(specs);
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    format!(
                        "[findobj] #{rev} \"{}\" servers={n} workers={workers}",
                        q.pat.text
                    ),
                ));
            }
            FoAction::Stop => self.findobj_engine.cancel(),
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
                // 패널 전환까지 한 길(`explorer_reveal_show` · 10-16) + 모달리스 창에서 메인 창으로 포커스.
                if self.explorer_reveal_show(server.as_ref(), t) {
                    if let Some(w) = &self.window {
                        crate::winfocus::focus(w);
                    }
                }
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
