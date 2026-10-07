//! App — 메모리 회수·메모리 창(docs/80).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// "방금 큰 것을 놓았다" — 1초 뒤(해제가 실제로 끝나고 · 연달아 닫아도 한 번만) 힙을 OS에 돌려준다.
    pub(crate) fn mem_released(&mut self) {
        if self.settings.flag("mem.trim_on_release") {
            self.mem_trim_due = Some(Instant::now() + Duration::from_secs(1));
        }
    }

    /// 틱: ① 탭이 줄었으면 회수 예약 ② 예약된 1회 회수 ③ 유휴 주기 회수(`mem.trim_secs` · 입력 없음 5초 + 실행 중 세션 없음).
    pub(crate) fn mem_tick(&mut self, now: Instant) -> Option<Instant> {
        let tabs = self.editors.tab_count();
        if tabs < self.mem_last_tabs {
            self.mem_released();
        }
        self.mem_last_tabs = tabs;
        let mut next: Option<Instant> = None;
        if let Some(due) = self.mem_trim_due {
            if now >= due {
                self.mem_trim_due = None;
                self.mem_trim("release");
            } else {
                next = Some(due);
            }
        }
        let secs = self.settings.int("mem.trim_secs").max(0) as u64;
        if secs > 0 {
            let at = *self
                .mem_trim_next
                .get_or_insert(now + Duration::from_secs(secs));
            if now >= at {
                let idle = now.duration_since(self.blink_origin) >= Duration::from_secs(5)
                    && !self.all_sess().any(|s| s.busy);
                if idle {
                    self.mem_trim("periodic");
                    self.mem_trim_next = Some(now + Duration::from_secs(secs));
                } else {
                    // 바쁘면 10초 뒤에 다시 본다.
                    self.mem_trim_next = Some(now + Duration::from_secs(10));
                }
            }
            next = Some(next.map_or(self.mem_trim_next.unwrap_or(at), |n| {
                n.min(self.mem_trim_next.unwrap_or(at))
            }));
        }
        next
    }

    /// 회수 한 번: ① 보이지 않는 탭의 그리기 캐시를 놓는다(다시 보면 다시 만든다) ② 힙을 정리해 OS에 돌려준다.
    /// 개발자 모드(`load` 층)에는 전후 Private·걸린 시간을 남긴다.
    pub(crate) fn mem_trim(&mut self, why: &str) {
        let (before, _) = memtrim::usage();
        let released = self.editors.release_inactive_caches();
        let us = memtrim::trim();
        let (after, ws) = memtrim::usage();
        dlog!(self, LogLayer::Load, LogLevel::Timing, {
            LogEntry::new(
                LogKind::Info,
                format!(
                    "memory trim ({why}): private {} → {} · working set {} · {released} tab cache(s) released · {:.1} ms",
                    nsql_core::fmt_bytes(before),
                    nsql_core::fmt_bytes(after),
                    nsql_core::fmt_bytes(ws),
                    us as f64 / 1000.0
                ),
            )
        });
        if std::env::var_os("NSQL_TRACE_MEM").is_some() {
            eprintln!(
                "[mem] trim ({why}) private {before} -> {after} · {released} caches · {us} us"
            );
        }
    }

    /// [힙 정리] 본체(창 버튼 · 기동 명령 `mem.trim` 공용 · 80 §7): 할당자 빈 조각 → OS · 곧바로 표본을 다시 떠서 전후를 보이고
    /// 바닥 줄 "힙 정리 완료 - N 반환(ms)" + 줄별 ▲/▼(표본이 알아서 · 사용자 10-07) + 상태줄.
    pub(crate) fn mem_trim_now(&mut self) {
        let before = memstat::sys_total();
        let us = memtrim::trim();
        self.mem_heap = None; // 정리 전후가 바로 보이게 힙 통계를 이번 표본에서 새로 잰다.
        let s = self.mem_sample();
        self.mem_status = (s.sys.footprint, Some(Instant::now()));
        let every = self.mem_every();
        self.mem_win.set_sample(s, every.as_millis() as u64);
        self.mem_win.set_trim_result(before, s.sys.footprint, us);
        self.sess.status = nsql_i18n::tf(
            nsql_i18n::Msg::StMemTrimResult,
            &[
                &memstat::fmt(before),
                &memstat::fmt(s.sys.footprint),
                &(us / 1000).to_string(),
            ],
        );
        self.redraw();
    }

    /// 메모리 맵 창 열기(docs/80 · 모델리스 · 최상위는 설정 `mem.always_on_top`) — 첫 표본은 다음 유휴 틱에.
    pub(crate) fn open_mem_window(&mut self, el: &ActiveEventLoop) {
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.mem_win
            .set_on_top(self.settings.flag("mem.always_on_top"));
        self.mem_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
        self.mem_next = Instant::now();
        self.mem_heap = None;
    }

    /// 상태줄 세그먼트·메뉴에서 토글(열기는 깃발 → `about_to_wait`가 이벤트 루프로 연다).
    pub(crate) fn toggle_mem_window(&mut self) {
        if self.mem_win.is_open() {
            self.mem_win.close();
            self.persist_window_sizes(false);
        } else {
            self.open_mem = true;
        }
    }

    pub(crate) fn mem_every(&self) -> Duration {
        Duration::from_millis(self.settings.int("mem.refresh_ms").clamp(250, 10_000) as u64)
    }

    /// 힙 통계 재측정 간격(`mem.heap_refresh_ms` · 1~60 s · T-310 = 힙 걷기는 표본마다가 아니라 이 간격으로).
    pub(crate) fn mem_heap_every(&self) -> Duration {
        Duration::from_millis(self.settings.int("mem.heap_refresh_ms").clamp(1000, 60_000) as u64)
    }

    /// 전체 표본(창이 열려 있을 때만 불린다) — 부품 보고(`MemSource`) + 결과 탭 + 창 표면 + 로그.
    /// 힙 통계(사용·여유)는 `mem_heap_every`가 지났을 때만 새로 재고 그 사이는 지난 값을 쓴다(T-310 · `memstat::sample`).
    pub(crate) fn mem_sample(&mut self) -> memstat::Sample {
        use memstat::Cat;
        let now = Instant::now();
        let reuse = match self.mem_heap {
            Some((h, at)) if now < at + self.mem_heap_every() => Some(h),
            _ => None,
        };
        let grids: Vec<(u64, u64)> = self.all_grids().map(|g| g.mem_parts()).collect();
        let main_surface = self.window.as_ref().map_or(0, |w| {
            let s = w.inner_size();
            u64::from(s.width) * u64::from(s.height) * 4
        });
        let surfaces = main_surface + self.mem_win.surface_bytes();
        let logs = self.log_win.approx_bytes() + self.txlog.len() as u64 * 256;
        let s = memstat::sample(
            &[&self.editors, &self.explorer, &self.intel],
            |acc| {
                for (d, tx) in grids {
                    acc.add(Cat::ResultData, d);
                    acc.add(Cat::ResultText, tx);
                }
                acc.add(Cat::Surfaces, surfaces);
                acc.add(Cat::Logs, logs);
            },
            reuse,
        );
        if reuse.is_none() {
            self.mem_heap = Some(((s.sys.heap_used, s.sys.heap_held), now));
        }
        s
    }

    /// 상태줄 총량 글(`mem.statusbar` 꺼짐 = None) — 마지막 조회가 `mem.status_refresh_ms`보다 오래됐을 때만 OS 한 번(그릴 때만 · 깨우지 않음).
    pub(crate) fn mem_status_text(&mut self) -> Option<String> {
        if !self.settings.flag("mem.statusbar") {
            return None;
        }
        let now = Instant::now();
        let every = Duration::from_millis(
            self.settings
                .int("mem.status_refresh_ms")
                .clamp(1000, 60_000) as u64,
        );
        if self
            .mem_status
            .1
            .is_none_or(|t| now.duration_since(t) >= every)
        {
            self.mem_status = (memstat::sys_total(), Some(now));
        }
        Some(memstat::fmt(self.mem_status.0))
    }
}
