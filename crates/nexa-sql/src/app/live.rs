//! App — 실시간 재조회(live).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 라이브 로그 설정(끔이면 None) — 매번 읽는다(설정 창에서 바꾸면 다음 실행부터).
    fn live_req(&self) -> Option<LiveReq> {
        // 라이브 모니터는 탐색기 메타 세션(= 접속 창으로 붙은 서버)으로 본다 → 그 세션의 실행만 대상(docs/52 §9).
        if self.sess.dialect != Dialect::Oracle
            || !self.explorer.has_server(self.sess.spec.as_ref())
        {
            return None;
        }
        let source = self
            .settings
            .effective("oracle.live.source")
            .unwrap_or("off");
        if source == "off" {
            return None;
        }
        let sid = self.sess.live_sid.clone()?;
        let table = self
            .settings
            .get("oracle.live.table")
            .unwrap_or("")
            .trim()
            .to_string();
        if source == "table" && table.is_empty() {
            return None;
        }
        Some(LiveReq {
            sid,
            source: source.to_string(),
            table,
            ts_col: self
                .settings
                .get("oracle.live.ts_col")
                .unwrap_or("LOG_TIME")
                .trim()
                .to_string(),
            text_col: self
                .settings
                .get("oracle.live.text_col")
                .unwrap_or("LOG_TEXT")
                .trim()
                .to_string(),
            since: self.sess.live_since.clone(),
        })
    }

    fn live_interval(&self) -> Duration {
        Duration::from_millis(
            self.settings
                .int("oracle.live.interval_ms")
                .clamp(250, 60_000) as u64,
        )
    }

    /// 실행 시작 — 기준 시각 초기화 · 첫 폴링 즉시(로그 테이블은 서버 현재 시각을 기준점으로 받는다).
    pub(crate) fn live_start(&mut self) {
        self.sess.live_since = None;
        self.sess.live_last.clear();
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

    /// 라이브 응답 → 로그 창(세션 소스는 바뀐 줄만).
    pub(crate) fn live_drain(&mut self) -> bool {
        let mut changed = false;
        for r in self.explorer.take_live() {
            match r {
                Ok((lines, last_ts)) => {
                    if last_ts.is_some() {
                        self.sess.live_since = last_ts;
                    }
                    for line in lines {
                        if line == self.sess.live_last {
                            continue;
                        }
                        self.sess.live_last = line.clone();
                        let text = format!("[live] {line}");
                        self.log_win.push(LogEntry::new(LogKind::Output, text));
                        changed = true;
                    }
                }
                Err(e) => {
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("[live] {e}")));
                    changed = true;
                }
            }
        }
        changed
    }
}
