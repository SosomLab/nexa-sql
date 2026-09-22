//! **실행 상태 카드**(사용자 09-17 "쿼리를 실행하면 우측 토스트 스타일로 실행 상태 · 종료까지 유지 · 문장 한 줄+툴팁 ·
//! 시작 시각 + 경과 · 단계/페치 진행 · 전송 속도 · 중지 버튼 · 끝난 뒤 지정 시간 지나면 사라짐").
//!
//! 일반 토스트([`crate::toast`])와 같은 자리(편집기 우하단)에 **맨 아래** 한 장으로 놓이고 다른 토스트는 그 위에 쌓인다.
//! - 1행: `■`(중지) + 문장 한 줄(`¶`·`→`로 줄바꿈·탭 · 넘치면 `…`) — hover 시 원문 툴팁(카드 위쪽).
//! - 2행: 시작 `YYYY-MM-DD HH:MM:SS.mmm` · 경과 `HH:MM:SS`(1초마다).
//! - 3행: 상태 = 실행 중(문장 n/m · 단계) / 가져오는 중(행 · 바이트 · **속도**) / 완료(행 · 소요 · 단계별) / 오류 / 중지됨.
//!
//! 왼쪽 색 막대 = 상태(진행 accent · 완료 ok · 오류 danger · 중지 warn). 끝난 뒤 `run.toast_hide_secs` 지나면 사라진다
//! (0 = 클릭으로 닫을 때까지 유지). 데이터는 호스트가 받는 이벤트에서 그대로 넣는다(별도 계측 없음).
//!
//! ★ 남은 시간 표시(사용자 09-22 "이미 있는 왼쪽 세로선을 그대로 활용 · 투명해지면서 줄어들고 위에서 아래로 · 지나간 부분은 70% 투명한
//! 같은 색" · `ui.toast_progress` · 일반 토스트와 같은 규칙 = [`crate::toast::life_alpha`] · [`crate::toast::bar_remaining`]): 끝난 카드의
//! **왼쪽 상태 색 막대**가 숨김까지 남은 시간만큼 진하고, 지나간 위쪽은 같은 색을 `ui.toast_bar_spent`(30%)로 — 카드와 함께 진척에 따라
//! `ui.toast_fade_to`까지 투명해진 뒤 마지막 300ms에 사라진다. 숨김 0초(수동 닫기)·진행 중이면 막대는 가득. 향상 모드(`perf.boost`)는 끈다.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nsql_i18n::{tf, Msg};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Phase {
    /// 문장 index(0부터) · 전체 수.
    Running {
        index: usize,
        total: usize,
    },
    Fetching,
    Done {
        rows: Option<u64>,
        secs: f64,
        stages: String,
    },
    Error(String),
    Stopped {
        rows: u64,
    },
}

pub(crate) enum RunToastHit {
    None,
    /// 중지 버튼.
    Stop,
    /// 카드 본문(끝난 카드 = 닫기).
    Card,
}

struct Run {
    sql: String,
    line: String,
    started_stamp: String,
    started: Instant,
    phase: Phase,
    rows: u64,
    bytes: u64,
    /// 속도 계산 — (시각, 누적 바이트) 마지막 표본 · 지수 이동 평균 B/s.
    last_sample: (Instant, u64),
    speed: f64,
    /// 끝난 시각(있으면 숨김 카운트다운).
    ended: Option<Instant>,
    /// 마지막으로 그린 경과 초(초가 바뀔 때만 다시 그림).
    shown_secs: u64,
}

pub(crate) struct RunToast {
    run: Option<Run>,
    enabled: bool,
    hide_after: Duration,
    alpha: f32,
    /// 남은 시간 막대 + 진척 페이드(`ui.toast_progress` · `ui.toast_fade_to` · `ui.toast_bar_spent`).
    progress: bool,
    fade_to: f32,
    /// 지나간 부분의 막대 불투명도 비율(0..1).
    spent: f32,
    rect: Rect,
    stop_rect: Rect,
    line_rect: Rect,
    hover_line: bool,
    hover_stop: bool,
}

const FADE_MS: u64 = 300;

fn hms(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn speed_text(bps: f64) -> String {
    if bps <= 0.0 {
        return String::new();
    }
    format!("{}/s", nsql_core::fmt_bytes(bps as u64))
}

impl RunToast {
    pub(crate) fn new() -> Self {
        RunToast {
            run: None,
            enabled: true,
            hide_after: Duration::from_secs(5),
            alpha: 0.9,
            progress: true,
            fade_to: 0.35,
            spent: 0.3,
            rect: Rect::default(),
            stop_rect: Rect::default(),
            line_rect: Rect::default(),
            hover_line: false,
            hover_stop: false,
        }
    }

    /// 설정 `run.toast` · `run.toast_hide_secs`(0 = 수동 닫기) · `ui.toast_alpha`.
    pub(crate) fn configure(&mut self, enabled: bool, hide_secs: i64, alpha_pct: i64) {
        self.enabled = enabled;
        self.hide_after = Duration::from_secs(hide_secs.clamp(0, 600) as u64);
        self.alpha = (alpha_pct.clamp(30, 100) as f32) / 100.0;
        if !enabled {
            self.run = None;
        }
    }

    /// 설정 `ui.toast_progress` · `ui.toast_fade_to`(%) · `ui.toast_bar_spent`(%).
    pub(crate) fn configure_progress(&mut self, on: bool, fade_to_pct: i64, spent_pct: i64) {
        self.progress = on;
        self.fade_to = (fade_to_pct.clamp(0, 100) as f32) / 100.0;
        self.spent = (spent_pct.clamp(0, 100) as f32) / 100.0;
    }

    /// 끝난 카드의 숨김 진척(0 = 방금 끝남 · 1 = 숨김) — 막대·페이드의 입력. 진행 중이거나 수동 닫기면 `None`.
    fn hide_progress(&self, now: Instant) -> Option<f32> {
        let ended = self.run.as_ref()?.ended?;
        if self.hide_after.is_zero() {
            return None;
        }
        let p = now.duration_since(ended).as_secs_f64() / self.hide_after.as_secs_f64();
        Some(p.clamp(0.0, 1.0) as f32)
    }

    #[cfg(test)]
    fn is_visible(&self) -> bool {
        self.run.is_some()
    }

    #[cfg(test)]
    fn is_running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.ended.is_none())
    }

    /// 실행 시작(문장 전체 · 시작 시각 스탬프).
    pub(crate) fn start(&mut self, sql: &str, stamp: String, total: usize) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        self.run = Some(Run {
            sql: sql.trim().to_string(),
            line: nsql_run::txlog::one_line(sql, 400),
            started_stamp: stamp,
            started: now,
            phase: Phase::Running { index: 0, total },
            rows: 0,
            bytes: 0,
            last_sample: (now, 0),
            speed: 0.0,
            ended: None,
            shown_secs: 0,
        });
    }

    pub(crate) fn set_phase(&mut self, phase: Phase) {
        if let Some(r) = &mut self.run {
            r.phase = phase;
        }
    }

    /// 조회 결과 첫 세그먼트(행 · 바이트 · 소요) — 속도 = 바이트/소요.
    pub(crate) fn first_page(&mut self, rows: u64, bytes: u64, elapsed: Duration) {
        if let Some(r) = &mut self.run {
            r.rows = rows;
            r.bytes = bytes;
            let secs = elapsed.as_secs_f64();
            if secs > 0.0 && bytes > 0 {
                r.speed = bytes as f64 / secs;
            }
            r.last_sample = (Instant::now(), bytes);
        }
    }

    /// 전체 조회 진행(누적 행 · 누적 바이트) — 표본 간 차이로 속도(지수 이동 평균).
    pub(crate) fn progress(&mut self, rows: u64, bytes: u64) {
        let now = Instant::now();
        if self.run.as_ref().is_none_or(|r| r.ended.is_some()) {
            // 끝난 카드 위에서 전체 조회가 시작됨 → 다시 진행 상태로.
            if let Some(r) = &mut self.run {
                r.ended = None;
                r.last_sample = (now, r.bytes);
            } else {
                return;
            }
        }
        if let Some(r) = &mut self.run {
            r.phase = Phase::Fetching;
            let (t0, b0) = r.last_sample;
            let dt = now.duration_since(t0).as_secs_f64();
            if dt >= 0.2 && bytes >= b0 {
                let inst = (bytes - b0) as f64 / dt;
                r.speed = if r.speed <= 0.0 {
                    inst
                } else {
                    r.speed * 0.6 + inst * 0.4
                };
                r.last_sample = (now, bytes);
            }
            r.rows = rows;
            r.bytes = bytes;
        }
    }

    /// 단계별 소요가 뒤따라오면(Timing) 완료 문구에 반영.
    pub(crate) fn timing(&mut self, secs: f64, stages: String) {
        if let Some(r) = &mut self.run {
            let rows = match &r.phase {
                Phase::Done { rows, .. } => *rows,
                _ => Some(r.rows),
            };
            r.phase = Phase::Done { rows, secs, stages };
        }
    }

    /// 러너가 끝났다 — 지금 상태 그대로 마감(결과 없이 실패했으면 오류 문구).
    pub(crate) fn finish_keep(&mut self, failed: Option<String>) {
        if let Some(r) = &mut self.run {
            if r.ended.is_some() {
                return;
            }
            if let Some(m) = failed {
                if !matches!(r.phase, Phase::Error(_)) {
                    r.phase = Phase::Error(m);
                }
            } else if matches!(r.phase, Phase::Running { .. } | Phase::Fetching) {
                r.phase = Phase::Done {
                    rows: Some(r.rows),
                    secs: r.started.elapsed().as_secs_f64(),
                    stages: String::new(),
                };
            }
            r.ended = Some(Instant::now());
        }
    }

    /// 끝(완료·오류·중지) — 숨김 카운트다운 시작.
    pub(crate) fn finish(&mut self, phase: Phase) {
        if let Some(r) = &mut self.run {
            if let Phase::Done { rows: Some(n), .. } = &phase {
                r.rows = *n;
            }
            if let Phase::Stopped { rows } = &phase {
                r.rows = *rows;
            }
            r.phase = phase;
            r.ended = Some(Instant::now());
        }
    }

    pub(crate) fn close(&mut self) {
        self.run = None;
        self.hover_line = false;
        self.hover_stop = false;
    }

    /// 틱 — 다시 그려야 하면 true · 다음 깨울 시각(경과 1초 · 숨김 만료 · 페이드).
    pub(crate) fn tick(&mut self, now: Instant) -> (bool, Option<Instant>) {
        let Some(r) = &mut self.run else {
            return (false, None);
        };
        let mut redraw = false;
        if let Some(ended) = r.ended {
            if self.hide_after.is_zero() {
                return (false, None);
            }
            let age = now.duration_since(ended);
            if age >= self.hide_after {
                self.run = None;
                return (true, None);
            }
            let left = self.hide_after - age;
            // 남은 시간 막대가 켜져 있으면 카운트다운 내내 30ms 틱(막대·투명도가 매 틱 바뀐다) · 꺼져 있으면 종전 = 마지막 300ms만.
            let animating = self.progress || left.as_millis() as u64 <= FADE_MS;
            let next = if animating {
                now + Duration::from_millis(30)
            } else {
                now + (left - Duration::from_millis(FADE_MS))
            };
            return (animating, Some(next));
        }
        let secs = now.duration_since(r.started).as_secs();
        if secs != r.shown_secs {
            r.shown_secs = secs;
            redraw = true;
        }
        let next = r.started + Duration::from_secs(secs + 1);
        (redraw, Some(next))
    }

    /// 마우스 이동 — hover 상태가 바뀌면 true.
    pub(crate) fn hover(&mut self, p: Point) -> bool {
        if self.run.is_none() {
            return false;
        }
        let line = self.line_rect.contains(p);
        let stop = self.stop_rect.contains(p);
        let changed = line != self.hover_line || stop != self.hover_stop;
        self.hover_line = line;
        self.hover_stop = stop;
        changed
    }

    pub(crate) fn click(&mut self, p: Point) -> RunToastHit {
        let Some(r) = &self.run else {
            return RunToastHit::None;
        };
        if self.stop_rect.contains(p) && r.ended.is_none() {
            return RunToastHit::Stop;
        }
        if self.rect.contains(p) {
            if r.ended.is_some() {
                self.close();
            }
            return RunToastHit::Card;
        }
        RunToastHit::None
    }

    fn status_line(r: &Run) -> String {
        match &r.phase {
            Phase::Running { index, total } => tf(
                Msg::RtRunning,
                &[&(index + 1).to_string(), &total.max(&1).to_string()],
            ),
            Phase::Fetching => {
                let sp = speed_text(r.speed);
                tf(
                    Msg::RtFetching,
                    &[&r.rows.to_string(), &nsql_core::fmt_bytes(r.bytes), &sp],
                )
            }
            Phase::Done { rows, secs, stages } => {
                let n = rows.map(|n| n.to_string()).unwrap_or_default();
                let sp = speed_text(r.speed);
                tf(
                    Msg::RtDone,
                    &[
                        &n,
                        &format!("{secs:.3}"),
                        &nsql_core::fmt_bytes(r.bytes),
                        &sp,
                        stages,
                    ],
                )
            }
            Phase::Error(m) => tf(Msg::RtError, &[m]),
            Phase::Stopped { rows } => tf(Msg::RtStopped, &[&rows.to_string()]),
        }
    }

    /// 그리기 — 편집기 우하단 기준(`right`·`bottom`) · 반환 = 카드 위 y(일반 토스트가 그 위에 쌓인다).
    pub(crate) fn paint(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        right: i32,
        bottom: i32,
        s: f32,
    ) -> i32 {
        let Some(r) = &self.run else {
            return bottom;
        };
        let px = |v: f32| (v * s).round() as i32;
        let (w, pad, gap, bar) = (px(440.0), px(10.0), px(6.0), px(4.0));
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let now = Instant::now();
        let fade = match r.ended {
            Some(e) if !self.hide_after.is_zero() => {
                let left = self.hide_after.saturating_sub(now.duration_since(e));
                let ms = left.as_millis() as u64;
                if ms < FADE_MS {
                    ms as f32 / FADE_MS as f32
                } else {
                    1.0
                }
            }
            _ => 1.0,
        };
        let life = self.hide_progress(now).filter(|_| self.progress);
        let a = self.alpha
            * life.map_or(1.0, |p| crate::toast::life_alpha(p, self.fade_to, true))
            * fade;

        let h = pad * 2 + lh * 3 + px(4.0);
        let card = Rect::new(right - gap - w, bottom - gap - h, w, h);
        self.rect = card;
        dc.fill_round_rect_alpha(card, px(6.0), th.panel_bg, a);
        dc.stroke_round_rect_alpha(card, px(6.0), th.border, 1.0, a);
        let color = match &r.phase {
            Phase::Running { .. } | Phase::Fetching => th.accent,
            Phase::Done { .. } => th.ok,
            Phase::Error(_) => th.danger,
            Phase::Stopped { .. } => th.warn,
        };
        // 왼쪽 상태 막대 = 남은 시간(끝난 카드): 아래를 고정하고 위에서부터 옅어진다 — 지나간 부분은 같은 색을
        // `ui.toast_bar_spent`(기본 30% = 70% 투명)로 남겨 "사라진다"기보다 "투명해지는" 느낌(사용자 09-22). 진행 중·수동 닫기·꺼짐 = 가득.
        let full = h - pad * 2;
        let bar_h = crate::toast::bar_remaining(full, life);
        let bar_x = card.x + px(6.0);
        if bar_h < full {
            dc.fill_round_rect_alpha(
                Rect::new(bar_x, card.y + pad, bar, full),
                px(2.0),
                color,
                a * self.spent,
            );
        }
        if bar_h > 0 {
            dc.fill_round_rect_alpha(
                Rect::new(bar_x, card.y + pad + (full - bar_h), bar, bar_h),
                px(2.0),
                color,
                a,
            );
        }
        let tx = card.x + px(6.0) + bar + pad;
        let clip = Rect::new(card.x, card.y, card.w - pad, card.h);
        // 1행: ■ 중지 + 문장 한 줄
        let stop_sz = lh;
        let stop = Rect::new(tx, card.y + pad, stop_sz, stop_sz);
        self.stop_rect = if r.ended.is_none() {
            stop
        } else {
            Rect::default()
        };
        let stop_color = if r.ended.is_none() {
            if self.hover_stop {
                th.danger
            } else {
                th.text
            }
        } else {
            th.text_dim
        };
        let sq = px(3.0);
        dc.fill_round_rect_alpha(
            Rect::new(stop.x + sq, stop.y + sq, stop.w - sq * 2, stop.h - sq * 2),
            px(2.0),
            stop_color,
            a,
        );
        let lx = stop.right() + px(8.0);
        let line_clip = Rect::new(lx, card.y + pad, card.right() - pad - lx, lh);
        self.line_rect = line_clip;
        let mut line = r.line.clone();
        if dc.text_width(&line) > line_clip.w {
            while dc.text_width(&format!("{line}…")) > line_clip.w && line.pop().is_some() {}
            line.push('…');
        }
        dc.text(lx, card.y + pad, line_clip, &line, th.text);
        // 2행: 시작 시각 · 경과
        let y2 = card.y + pad + lh + px(2.0);
        dc.text(tx, y2, clip, &r.started_stamp, th.text_dim);
        let el = hms(now.duration_since(r.started).min(
            r.ended
                .map(|e| e.duration_since(r.started))
                .unwrap_or(Duration::MAX),
        ));
        let el_txt = format!("⏱ {el}");
        let ew = dc.text_width(&el_txt);
        dc.text(card.right() - pad - ew, y2, clip, &el_txt, th.text);
        // 3행: 상태
        let y3 = y2 + lh + px(2.0);
        let st = Self::status_line(r);
        dc.text(tx, y3, clip, &st, color);
        // 툴팁(원문 · 카드 위쪽 · 최대 24줄)
        if self.hover_line {
            dc.select_font(FontSlot::Status, false);
            let lines: Vec<&str> = r.sql.lines().take(24).collect();
            let tw = lines.iter().map(|l| dc.text_width(l)).max().unwrap_or(0);
            let tlh = dc.text_height();
            let tw = tw.min(px(720.0)) + px(12.0);
            let thh = tlh * lines.len().max(1) as i32 + px(8.0);
            let tr = Rect::new(
                (card.right() - tw).max(px(4.0)),
                card.y - px(6.0) - thh,
                tw,
                thh,
            );
            dc.fill_round_rect_alpha(tr, px(4.0), th.text, 0.92);
            for (i, l) in lines.iter().enumerate() {
                dc.text(
                    tr.x + px(6.0),
                    tr.y + px(4.0) + tlh * i as i32,
                    tr,
                    l,
                    th.panel_bg,
                );
            }
            dc.select_font(FontSlot::Base, false);
        }
        card.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시작 → 진행(속도) → 완료 → 숨김 시간 뒤 사라짐 · 0초면 유지.
    #[test]
    fn lifecycle_speed_and_hide() {
        let mut rt = RunToast::new();
        rt.configure(true, 2, 90);
        rt.start("SELECT *\n\tFROM t", "2026-09-17 10:00:00.000".into(), 1);
        assert!(rt.is_running());
        assert_eq!(rt.run.as_ref().expect("run").line, "SELECT *¶ FROM t");
        rt.first_page(200, 4_000_000, Duration::from_secs(2));
        assert!((rt.run.as_ref().expect("run").speed - 2_000_000.0).abs() < 1.0);
        rt.progress(400, 8_000_000);
        assert_eq!(rt.run.as_ref().expect("run").phase, Phase::Fetching);
        rt.finish(Phase::Done {
            rows: Some(400),
            secs: 2.5,
            stages: "fetch 2.5s".into(),
        });
        assert!(!rt.is_running());
        let ended = rt.run.as_ref().expect("run").ended.expect("ended");
        let (_, next) = rt.tick(ended);
        assert!(next.is_some());
        let (redraw, _) = rt.tick(ended + Duration::from_secs(3));
        assert!(redraw && !rt.is_visible(), "숨김 시간 뒤 사라짐");
        let mut keep = RunToast::new();
        keep.configure(true, 0, 90);
        keep.start("x", "s".into(), 1);
        keep.finish(Phase::Error("boom".into()));
        let e = keep.run.as_ref().expect("run").ended.expect("ended");
        assert!(
            !keep.tick(e + Duration::from_secs(100)).0 && keep.is_visible(),
            "0 = 유지"
        );
        assert_eq!(hms(Duration::from_secs(3661)), "01:01:01");
    }

    /// 남은 시간 진척: 진행 중·수동 닫기(0초)는 없음 · 끝난 뒤 0 → 1 · 켜져 있으면 카운트다운 내내 30ms 틱, 꺼져 있으면 마지막 300ms만.
    #[test]
    fn hide_progress_and_tick_cadence() {
        let mut rt = RunToast::new();
        rt.configure(true, 4, 90);
        rt.start("x", "s".into(), 1);
        assert_eq!(rt.hide_progress(Instant::now()), None, "진행 중");
        rt.finish(Phase::Error("e".into()));
        let e = rt.run.as_ref().expect("run").ended.expect("ended");
        assert_eq!(rt.hide_progress(e), Some(0.0));
        assert!((rt.hide_progress(e + Duration::from_secs(2)).expect("p") - 0.5).abs() < 1e-6);
        assert_eq!(rt.hide_progress(e + Duration::from_secs(9)), Some(1.0));
        // 켜짐 = 1초 뒤에도 30ms 뒤 다시(막대가 움직인다).
        let (redraw, next) = rt.tick(e + Duration::from_secs(1));
        assert!(redraw);
        assert_eq!(
            next,
            Some(e + Duration::from_secs(1) + Duration::from_millis(30))
        );
        // 꺼짐 = 종전: 마지막 300ms 전까지는 한 번에 건너뛴다.
        rt.configure_progress(false, 35, 30);
        let (redraw, next) = rt.tick(e + Duration::from_secs(1));
        assert!(!redraw);
        assert_eq!(
            next,
            Some(e + Duration::from_secs(4) - Duration::from_millis(FADE_MS))
        );
        // 수동 닫기(0초) = 진척 없음.
        let mut keep = RunToast::new();
        keep.configure(true, 0, 90);
        keep.start("x", "s".into(), 1);
        keep.finish(Phase::Error("e".into()));
        assert_eq!(keep.hide_progress(Instant::now()), None);
    }
}
