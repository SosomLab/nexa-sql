//! App — 실시간 재조회(live) · Oracle 라이브 모니터(T-71 · docs/32 §2).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.
//! ★ 11-04 2차: 폴링 문장·줄 형식·중복 제거는 **nsql-run 부품 `nsql_run::live`**(CLI `nsql run --live`와 공용) · 여기는 설정 → 요청 조립 ·
//! 탐색기 메타 세션으로 보내기 · 응답을 로그 창 `LogKind::Live`(종류 메뉴 "live" 토글 = Live 세그먼트)로 흘리기만.

use crate::*;
use nsql_run::live::{LiveConfig, LiveLine};

impl App {
    /// 라이브 구성(설정 `oracle.live.*` · 매번 읽는다 = 설정 창에서 바꾸면 다음 실행부터) — 폴링할 것이 없으면 None.
    fn live_req(&self) -> Option<LiveReq> {
        // 라이브 모니터는 탐색기 메타 세션(= 접속 창으로 붙은 서버)으로 본다 → 그 세션의 실행만 대상(docs/52 §9).
        if self.sess.dialect != Dialect::Oracle
            || !self.explorer.has_server(self.sess.spec.as_ref())
        {
            return None;
        }
        let cfg = LiveConfig::from_lookup(&|k| {
            if k == "oracle.live.source" {
                self.settings.effective(k).map(str::to_string)
            } else {
                self.settings.get(k).map(str::to_string)
            }
        });
        if !cfg.active() {
            return None;
        }
        // 로그 표 소스인데 표 이름이 비었으면 그 소스는 성립하지 않는다(진행률만 켜져 있으면 진행률만).
        if cfg.source == Some(nsql_run::live::LiveSource::Table)
            && cfg.table.is_empty()
            && !cfg.longops
        {
            return None;
        }
        let sid = self.sess.live_sid.clone()?;
        Some(LiveReq {
            sid,
            cfg,
            since: self.sess.live.since.clone(),
        })
    }

    fn live_interval(&self) -> Duration {
        Duration::from_millis(
            self.settings
                .int("oracle.live.interval_ms")
                .clamp(250, 60_000) as u64,
        )
    }

    /// 실행 시작 — 상태(워터마크·직전 줄·오류) 초기화 · 첫 폴링 즉시(로그 표는 서버 현재 시각을 기준점으로 받는다).
    pub(crate) fn live_start(&mut self) {
        self.sess.live.reset();
        self.sess.live_final = true;
        if let Some(req) = self.live_req() {
            self.explorer.live_poll(self.sess.spec.as_ref(), req);
            self.sess.live_next = Instant::now() + self.live_interval();
        }
    }

    /// 주기 폴링(실행 중) · 실행이 끝나면 마지막 1회.
    pub(crate) fn live_tick(&mut self, now: Instant) -> Option<Instant> {
        if self.sess.busy {
            if now >= self.sess.live_next {
                if let Some(req) = self.live_req() {
                    self.explorer.live_poll(self.sess.spec.as_ref(), req);
                }
                self.sess.live_next = now + self.live_interval();
            }
            Some(self.sess.live_next)
        } else if self.sess.live_final {
            self.sess.live_final = false;
            if let Some(req) = self.live_req() {
                self.explorer.live_poll(self.sess.spec.as_ref(), req);
            }
            None
        } else {
            None
        }
    }

    /// 라이브 응답 → 로그 창(`LogKind::Live` · 직전 틱과 같은 줄·같은 오류는 `LiveState`가 거른다).
    pub(crate) fn live_drain(&mut self) -> bool {
        let mut changed = false;
        for r in self.explorer.take_live() {
            for line in self.sess.live.absorb(r) {
                match line {
                    LiveLine::Text(text) => self.log_win.push(LogEntry::new(LogKind::Live, text)),
                    LiveLine::Error(e) => self
                        .log_win
                        .push(LogEntry::new(LogKind::Error, format!("live: {e}"))),
                }
                changed = true;
            }
        }
        changed
    }
}
