//! ★ 객체 찾기 창 호스트 쪽(docs/113 · 10-15): 열기 · 동작(검색 → **전용 엔진** `findobj_engine` · 소스 탭 · 탐색기에서 보기 · 복사 · SELECT) ·
//! 설정 반영 · 엔진 메시지 펌프(`findobj_pump`). 검색은 창 전용 스레드들(서버마다 계획 + 워커 N · D-295 실시간 직접 조회) · 합치기는 창(`push_msg`).
use crate::findobj_win::FoAction;
use crate::*;
use nsql_i18n::{t, Msg};
use nsql_log::{LogEntry, LogKind};
use nsql_script::ConnectSpec;
use std::time::Instant;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;

/// ★ 미리보기 반응 계측의 시작점(10-16): 객체 찾기 창에 OS 사건(왼쪽 누름 · 키 누름)이 닿은 순간 + 그 메시지가 OS 큐에서 기다린 시간.
pub(crate) struct FoTrace {
    pub(crate) t_evt: Instant,
    pub(crate) src: &'static str,
    pub(crate) os_wait_ms: Option<u32>,
}

impl App {
    /// ★ 객체 찾기 창의 사건 한 건 — OS가 보낸 것이든 자체 시험이 합성한 것이든 **이 한 길**(사용자 10-16 "다른 컨트롤이 먼저 처리하는지"는
    ///   창 `handle`이 `consumed`로 남기고, 여기는 계측 시작점을 찍는다).
    pub(crate) fn findobj_window_event(&mut self, event: &WindowEvent) {
        let kind = match event {
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => Some("click"),
            WindowEvent::KeyboardInput {
                event: k,
                is_synthetic: false,
                ..
            } if k.state == ElementState::Pressed => Some("key"),
            _ => None,
        };
        if let Some(src) = kind {
            // 합성 사건(자체 시험)은 OS 큐를 지나지 않았다 → `GetMessageTime`은 마지막 **실제** 메시지 시각이라 뜻 없는 값(협업 16차 E2E
            //   16·31·250·2296 ms) → "-".
            let os_wait_ms = if self.fo_synth {
                None
            } else {
                nexa_sys::last_message_age_ms()
            };
            self.fo_trace = Some(FoTrace {
                t_evt: Instant::now(),
                src,
                os_wait_ms,
            });
        }
        match self.findobj_win.handle(event) {
            FoAction::Paint => {
                let ui_px = self.settings.font_px("ui.font_size");
                self.findobj_win.paint(&self.ui_font, &self.theme, ui_px);
            }
            other => self.findobj_action(other),
        }
    }

    /// 자체 시험(`findobj.click:<x>/<y>` · `findobj.clickpre:<k>`): 창 좌표에 **왼쪽 클릭을 합성**해 OS 사건과 같은 길로 흘린다
    ///   (CursorMoved → Pressed → Released · 창 안의 모든 분기를 지난다 — 컨트롤 함수를 직접 부르면 보이지 않는 "누가 먼저 먹나"를 함께 잰다).
    pub(crate) fn findobj_synth_click(&mut self, x: i32, y: i32) {
        use winit::dpi::PhysicalPosition;
        use winit::event::DeviceId;
        let dev = DeviceId::dummy();
        let evs = [
            WindowEvent::CursorMoved {
                device_id: dev,
                position: PhysicalPosition::new(f64::from(x), f64::from(y)),
            },
            WindowEvent::MouseInput {
                device_id: dev,
                state: ElementState::Pressed,
                button: MouseButton::Left,
            },
            WindowEvent::MouseInput {
                device_id: dev,
                state: ElementState::Released,
                button: MouseButton::Left,
            },
        ];
        self.fo_synth = true;
        for ev in &evs {
            self.findobj_window_event(ev);
        }
        self.fo_synth = false;
    }

    /// 자체 시험 `findobj.key:<down|up>`: 키 사건은 밖에서 만들 수 없어 창 분기 함수를 직접 부르되, **계측 시작점은 같은 자리**에 찍는다
    ///   (종전 = 시작점 없음 → 키 경로의 `event→text`·`painted`가 비었다 · 협업 16차 E2E A-9).
    pub(crate) fn findobj_synth_key(&mut self, down: bool) -> Option<u32> {
        self.fo_trace = Some(FoTrace {
            t_evt: Instant::now(),
            src: "key",
            os_wait_ms: None,
        });
        self.findobj_win.test_preview_key(down)
    }

    /// 계측 사슬 ④ = 본문이 편집기에 실렸다(같은 탭 이동 = 동기 · 새 객체 = 소스 도착). 시작점이 있으면 로그 + 그리기 꼬리 예약.
    pub(crate) fn findobj_trace_done(&mut self, stage: &str) {
        if let Some((t0, src)) = self.fo_open_trace.take() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!(
                    "[preview] {src} · {stage} · event→text {} ms",
                    t0.elapsed().as_millis()
                ),
            ));
            self.fo_paint = Some((t0, src));
        }
    }

    /// 계측 사슬 ⑤ = 메인 창 첫 그리기(RedrawRequested 뒤).
    pub(crate) fn findobj_paint_trace(&mut self) {
        if let Some((t0, src)) = self.fo_paint.take() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!(
                    "[preview] {src} · painted · event→frame {} ms",
                    t0.elapsed().as_millis()
                ),
            ));
        }
    }

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
        self.findobj_win
            .set_passthrough(self.settings.flag("ui.ctxmenu_passthrough"));
        self.findobj_win
            .set_preview_click(self.settings.flag("findobj.preview_click"));
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

    /// 살아 있는 미리보기 탭이 **이 객체**(같은 서버 · 스키마 · 이름)를 보여 주고 있으면 그 탭 인덱스.
    fn findobj_same_preview_tab(
        &mut self,
        server: Option<&ConnectSpec>,
        obj: &nsql_catalog::ObjectInfo,
    ) -> Option<usize> {
        let id = self.editors.preview_id()?;
        let o = self.object_tabs.get(&id)?;
        let same = o.name.eq_ignore_ascii_case(&obj.name)
            && o.schema.eq_ignore_ascii_case(&obj.schema)
            && o.server.as_ref() == server;
        if !same {
            return None;
        }
        self.editors.index_of_id(id)
    }

    /// 창 동작 처리.
    pub(crate) fn findobj_action(&mut self, act: FoAction) {
        match act {
            FoAction::None | FoAction::Paint => {}
            FoAction::Close => {
                if self.findobj_win.is_running() {
                    self.findobj_engine.cancel();
                }
                // 창이 결과를 놓는다(`close` 안) → 1초 뒤 힙 정리 예약(큰 것을 놓았을 때와 같은 길 · `mem.trim_on_release`).
                self.findobj_win.close();
                self.mem_released();
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
                        if !only.is_empty() && !only.contains(&spec) {
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
            FoAction::OpenSource {
                server,
                obj,
                line,
                preview,
            } => {
                // ★ 같은 객체가 이미 미리보기 탭에 떠 있으면 **재조회 없이** 그 탭에서 줄만 이동(사용자 10-16 "동일 객체에서 줄만 바뀌는데
                //   느릴 이유 없음" · ↓ 연타 때 같은 객체 요청이 두 번 나가 뒤 것이 평범한 열기로 취급돼 1행 정식 탭이 열리던 결함도 함께).
                if let Some(i) = self.findobj_same_preview_tab(server.as_ref(), &obj) {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        format!("[preview] same tab · {} → line {line}", obj.name),
                    ));
                    self.findobj_goto = None;
                    self.editors.switch(i);
                    if !preview {
                        // 정식 열기 = 승격(새 탭을 또 열지 않는다).
                        self.editors.set_preview_id(None);
                    }
                    if line > 0 {
                        self.editors.cur_mut().goto_line(line as usize);
                    }
                    self.findobj_trace_done("same tab");
                    self.redraw();
                    if !preview {
                        if let Some(w) = &self.window {
                            crate::winfocus::focus(w);
                        }
                    }
                    return;
                }
                self.findobj_goto = Some((obj.name.clone(), line, preview));
                if !self.explorer.open_source_for(server.as_ref(), &obj) {
                    self.sess.status = t(Msg::ExpNotConnected).to_string();
                    self.findobj_goto = None;
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        format!("[source] dropped: no explorer pane for {}", obj.name),
                    ));
                }
                // 정식 열기만 메인 창으로 포커스 — 미리보기(줄 클릭)는 객체 찾기 창의 미리보기 목록에 포커스를 둔다
                //   (사용자 10-16 "포커스는 클릭한 목록에 유지" → ↑/↓로 계속 훑기).
                if !preview {
                    if let Some(w) = &self.window {
                        crate::winfocus::focus(w);
                    }
                }
            }
            FoAction::Reveal {
                server,
                obj,
                member,
            } => {
                let db = (!obj.db.is_empty()).then(|| obj.db.clone());
                let t = crate::explorer::RevealTarget {
                    db,
                    schema: obj.schema,
                    kind: obj.kind,
                    name: obj.name,
                    member,
                };
                // 패널 전환까지 한 길(`explorer_reveal_show` · 10-16) + 모달리스 창에서 메인 창으로 포커스.
                if self.explorer_reveal_show(server.as_ref(), t) {
                    if let Some(w) = &self.window {
                        crate::winfocus::focus(w);
                    }
                }
            }
            // "미리보기" 스위치 = 설정 `findobj.preview_click`과 같은 값(창에서 바꾸면 즉시 저장 · 설정 창에서 바꾸면 창에 반영).
            FoAction::PreviewClick(on) => {
                let v = if on { "on" } else { "off" };
                if self.settings.get("findobj.preview_click") != Some(v) {
                    let _ = self.settings.set("findobj.preview_click", v);
                    let _ = self.settings.save();
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
