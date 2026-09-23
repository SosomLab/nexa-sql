//! **실행 상태 카드**(사용자 09-17 "쿼리를 실행하면 우측 토스트 스타일로 실행 상태 · 종료까지 유지 · 종료 시각 ·
//! 시작 시각 + 경과 · 단계/페치 진행 · 전송 속도 · 중지 버튼 · 끝난 뒤 지정 시간 지나면 사라짐").
//!
//! 일반 토스트([`crate::toast`])와 같은 자리(편집기 우하단)에 놓이고 다른 토스트는 그 위에 쌓인다.
//! - 1행: `■`(중지) + 문장 한 줄(`¶`·`→`로 줄바꿈·탭 · 넘치면 `…`) — hover 시 원문 툴팁(카드 위쪽).
//! - 2행: 시작 `YYYY-MM-DD HH:MM:SS.mmm` · 경과 `HH:MM:SS.mmm`(`run.toast_tick_ms`마다).
//! - 3행: 상태 = 실행 중(문장 n/m · 단계) / 가져오는 중(행 · 바이트 · **속도**) / 완료(행 · 소요 · 단계별 소요) / 오류 / 중지.
//!
//! ★ **누적**(사용자 09-22): 카드는 한 장이 아니라 **여러 장**이고 **최신이 최상단**이다 — 정렬 = 마지막 사건(시작 또는 종료) 시각의
//! 내림차순. 새 실행이 시작되거나 어떤 카드가 끝나면 그 카드가 맨 위로 온다(예: `3 실행 / 2 실행 / 1 실행`에서 2가 끝나면
//! `2 완료 / 3 실행 / 1 실행` → 2가 숨겨지면 `3 실행 / 1 실행`). 편집기 본문 **아래에 고정되어 위로 자라고**(오래된 것이 바닥) 넘치면 **휠로 픽셀 스크롤**.
//! `run.toast_follow`(기본 켬) = 새 카드가 오면 스크롤을 무시하고 맨 위로(끄면 사용자가 옮긴 자리 유지) · `run.toast_max` = 카드 상한
//! (넘치면 가장 오래된 끝난 카드부터 버림 · 향상 모드는 더 작게). 끝난 카드는 각자 `run.toast_hide_secs` 뒤 사라진다(0 = 클릭으로 닫기).
//!
//! 왼쪽 색 막대 = 상태(진행 accent · 완료 ok · 오류 danger · 중지 warn). 남은 시간 표시(사용자 09-22 · `ui.toast_progress`)는
//! 일반 토스트와 같은 규칙 = [`crate::toast::life_alpha`] · [`crate::toast::bar_remaining`] — 막대가 남은 시간만큼 진하고 지나간
//! 위쪽은 `ui.toast_bar_spent`(30 %)로 남으며 카드는 `ui.toast_fade_to`까지 투명해진 뒤 마지막 300 ms에 사라진다.
//! ★ 스택은 **앱에 하나**(사용자 09-22 "토스트는 탭에 제한되지 않고 전체 탭이 공유") — 세션(`Sess`)은 자기 카드의 **id**(`run_card`)만 들고
//! 진행 갱신을 그 id로 보낸다(다른 세션의 카드와 섞이지 않는다). 영역 = 편집기 본문(미니맵과 같은 높이 · 탭 무관) · 넘치는 카드는 **잘라서** 그린다.

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
    /// 1행 오른쪽 복사 버튼 — 그 실행의 SQL 원문(카드는 그대로 · 사용자 09-23).
    Copy(String),
}

struct Run {
    id: u64,
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
    /// 마지막으로 그린 경과 틱(`tick_ms` 단위 · 바뀔 때만 다시 그림 — 사용자 09-22 `00:00:00.000`).
    shown_tick: u64,
    /// 마지막으로 그린 자리(hit-test · 안 보이면 빈 사각형).
    rect: Rect,
    stop_rect: Rect,
    line_rect: Rect,
    /// 1행 오른쪽 복사 버튼 자리(사용자 09-23).
    copy_rect: Rect,
}

pub(crate) struct RunToast {
    runs: Vec<Run>,
    next_id: u64,
    enabled: bool,
    hide_after: Duration,
    /// 실행 중 경과 시간 갱신 주기(ms · 설정 `run.toast_tick_ms` · 향상 모드 강제 1000).
    tick_ms: u64,
    alpha: f32,
    /// 남은 시간 막대 + 진척 페이드(`ui.toast_progress` · `ui.toast_fade_to` · `ui.toast_bar_spent`).
    progress: bool,
    fade_to: f32,
    /// 지나간 부분의 막대 불투명도 비율(0..1).
    spent: f32,
    hover_line: Option<u64>,
    hover_stop: Option<u64>,
    hover_copy: Option<u64>,
    /// 누적이 영역을 넘을 때 아래로 내려 본 양(px · 0 = 맨 위 = 최신).
    scroll: i32,
    /// 새 카드가 오면 맨 위로(설정 `run.toast_follow`).
    follow: bool,
    /// 카드 상한(설정 `run.toast_max` · 넘치면 가장 오래된 끝난 카드부터).
    max_cards: usize,
    /// 마지막 그리기의 스택 영역(휠 대상)과 넘친 높이.
    stack_rect: Rect,
    overflow: i32,
}

const FADE_MS: u64 = 300;

/// `HH:MM:SS.mmm`(사용자 09-22).
fn hms(d: Duration) -> String {
    let s = d.as_secs();
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        s / 3600,
        (s / 60) % 60,
        s % 60,
        d.subsec_millis()
    )
}

fn speed_text(bps: f64) -> String {
    if bps <= 0.0 {
        return String::new();
    }
    format!("{}/s", nsql_core::fmt_bytes(bps as u64))
}

/// 누적 순서(위 → 아래): **마지막 사건(종료 시각 · 아니면 시작 시각)이 최근인 것부터**. 순수 함수(테스트).
fn stack_order(runs: &[Run]) -> Vec<usize> {
    let mut v: Vec<usize> = (0..runs.len()).collect();
    v.sort_by_key(|&i| std::cmp::Reverse(runs[i].ended.unwrap_or(runs[i].started)));
    v
}

impl RunToast {
    pub(crate) fn new() -> Self {
        RunToast {
            runs: Vec::new(),
            next_id: 1,
            enabled: true,
            hide_after: Duration::from_secs(5),
            tick_ms: 100,
            alpha: 0.9,
            progress: true,
            fade_to: 0.35,
            spent: 0.3,
            hover_line: None,
            hover_stop: None,
            hover_copy: None,
            scroll: 0,
            follow: true,
            max_cards: 30,
            stack_rect: Rect::default(),
            overflow: 0,
        }
    }

    /// 설정 `run.toast` · `run.toast_hide_secs`(0 = 수동 닫기) · `ui.toast_alpha`.
    pub(crate) fn configure(&mut self, enabled: bool, hide_secs: i64, alpha_pct: i64) {
        self.enabled = enabled;
        self.hide_after = Duration::from_secs(hide_secs.clamp(0, 600) as u64);
        self.alpha = (alpha_pct.clamp(30, 100) as f32) / 100.0;
        if !enabled {
            self.runs.clear();
        }
    }

    /// 경과 시간·카운트다운 갱신 주기(ms · 최소 30 · 설정 `run.toast_tick_ms` · 향상 모드 = 1000).
    pub(crate) fn configure_tick(&mut self, ms: i64) {
        self.tick_ms = ms.clamp(30, 5000) as u64;
    }

    /// 설정 `run.toast_follow`(새 카드 = 맨 위로) · `run.toast_max`(카드 상한 · 향상 모드는 작게).
    pub(crate) fn configure_stack(&mut self, follow: bool, max_cards: i64) {
        self.follow = follow;
        self.max_cards = max_cards.clamp(1, 500) as usize;
        self.enforce_max();
    }

    /// 카드 상한 — 넘치면 가장 오래 전에 끝난 카드부터 버린다(실행 중 카드는 남긴다).
    fn enforce_max(&mut self) {
        while self.runs.len() > self.max_cards {
            let victim = self
                .runs
                .iter()
                .enumerate()
                .filter(|(_, r)| r.ended.is_some())
                .min_by_key(|(_, r)| r.ended)
                .map(|(i, _)| i);
            match victim {
                Some(i) => {
                    self.runs.remove(i);
                }
                None => break,
            }
        }
    }

    /// 설정 `ui.toast_progress` · `ui.toast_fade_to`(%) · `ui.toast_bar_spent`(%).
    pub(crate) fn configure_progress(&mut self, on: bool, fade_to_pct: i64, spent_pct: i64) {
        self.progress = on;
        self.fade_to = (fade_to_pct.clamp(0, 100) as f32) / 100.0;
        self.spent = (spent_pct.clamp(0, 100) as f32) / 100.0;
    }

    /// 끝난 카드의 숨김 진척(0 = 방금 끝남 · 1 = 숨김) — 막대·페이드의 입력. 진행 중이거나 수동 닫기면 `None`.
    fn hide_progress_of(&self, r: &Run, now: Instant) -> Option<f32> {
        let ended = r.ended?;
        if self.hide_after.is_zero() {
            return None;
        }
        let p = now.duration_since(ended).as_secs_f64() / self.hide_after.as_secs_f64();
        Some(p.clamp(0.0, 1.0) as f32)
    }

    /// 세션이 든 카드(id) — 없으면(아직 시작 전 · 이미 닫힘) None.
    fn by_id(&mut self, card: Option<u64>) -> Option<&mut Run> {
        let id = card?;
        self.runs.iter_mut().find(|r| r.id == id)
    }

    /// 새 사건(시작·종료) — 포커스 이동 옵션이면 맨 위로.
    fn on_new_event(&mut self) {
        if self.follow {
            self.scroll = 0;
        }
    }

    #[cfg(test)]
    fn is_visible(&self) -> bool {
        !self.runs.is_empty()
    }

    #[cfg(test)]
    fn is_running(&self) -> bool {
        self.runs.iter().any(|r| r.ended.is_none())
    }

    /// 실행 시작(문장 전체 · 시작 시각 스탬프) — 새 카드를 **더한다**(끝난 카드는 각자의 카운트다운대로 남는다). 돌려주는 값 = 카드 id
    /// (세션이 `run_card`로 들고 진행 갱신에 넘긴다 · 꺼져 있으면 None).
    pub(crate) fn start(&mut self, sql: &str, stamp: String, total: usize) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        let now = Instant::now();
        let id = self.next_id;
        self.next_id += 1;
        self.runs.push(Run {
            id,
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
            shown_tick: 0,
            rect: Rect::default(),
            stop_rect: Rect::default(),
            line_rect: Rect::default(),
            copy_rect: Rect::default(),
        });
        self.enforce_max();
        self.on_new_event();
        Some(id)
    }

    pub(crate) fn set_phase(&mut self, card: Option<u64>, phase: Phase) {
        if let Some(r) = self.by_id(card) {
            r.phase = phase;
        }
    }

    /// 조회 결과 첫 세그먼트(행 · 바이트 · 소요) — 속도 = 바이트/소요.
    pub(crate) fn first_page(
        &mut self,
        card: Option<u64>,
        rows: u64,
        bytes: u64,
        elapsed: Duration,
    ) {
        if let Some(r) = self.by_id(card) {
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
    pub(crate) fn progress(&mut self, card: Option<u64>, rows: u64, bytes: u64) {
        let now = Instant::now();
        if let Some(r) = self.by_id(card) {
            if r.ended.is_some() {
                // 끝난 카드 위에서 전체 조회가 이어짐 → 다시 진행 상태로.
                r.ended = None;
                r.last_sample = (now, r.bytes);
            }
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
    pub(crate) fn timing(&mut self, card: Option<u64>, secs: f64, stages: String) {
        if let Some(r) = self.by_id(card) {
            let rows = match &r.phase {
                Phase::Done { rows, .. } => *rows,
                _ => Some(r.rows),
            };
            r.phase = Phase::Done { rows, secs, stages };
        }
    }

    /// 러너가 끝났다 — 지금 상태 그대로 마감(결과 없이 실패했으면 오류 문구).
    pub(crate) fn finish_keep(&mut self, card: Option<u64>, failed: Option<String>) {
        if let Some(r) = self.by_id(card).filter(|r| r.ended.is_none()) {
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
            self.on_new_event();
        }
    }

    /// 끝(완료·오류·중지) — 그 카드의 숨김 카운트다운 시작 · 맨 위로.
    pub(crate) fn finish(&mut self, card: Option<u64>, phase: Phase) {
        if let Some(r) = self.by_id(card).filter(|r| r.ended.is_none()) {
            if let Phase::Done { rows: Some(n), .. } = &phase {
                r.rows = *n;
            }
            if let Phase::Stopped { rows } = &phase {
                r.rows = *rows;
            }
            r.phase = phase;
            r.ended = Some(Instant::now());
            self.on_new_event();
        }
    }

    /// 전부 닫기.
    #[cfg(test)]
    pub(crate) fn close(&mut self) {
        self.runs.clear();
        self.hover_line = None;
        self.hover_stop = None;
        self.hover_copy = None;
        self.scroll = 0;
    }

    /// 틱 — 다시 그려야 하면 true · 다음 깨울 시각(경과 갱신 · 숨김 만료 · 페이드) = 카드들 중 가장 이른 것.
    pub(crate) fn tick(&mut self, now: Instant) -> (bool, Option<Instant>) {
        let mut redraw = false;
        let mut next: Option<Instant> = None;
        let bump = |t: Instant, next: &mut Option<Instant>| {
            *next = Some(next.map_or(t, |n| n.min(t)));
        };
        let hide_after = self.hide_after;
        let tick = self.tick_ms.max(30);
        let progress = self.progress;
        let before = self.runs.len();
        // 만료된 카드 제거.
        self.runs.retain(|r| match r.ended {
            Some(e) if !hide_after.is_zero() => now.duration_since(e) < hide_after,
            _ => true,
        });
        if self.runs.len() != before {
            redraw = true;
        }
        for r in &mut self.runs {
            match r.ended {
                Some(ended) => {
                    if hide_after.is_zero() {
                        continue;
                    }
                    let left = hide_after - now.duration_since(ended);
                    // 남은 시간 막대가 켜져 있으면 카운트다운 내내 틱(막대·투명도가 매 틱 바뀐다) · 꺼져 있으면 마지막 300ms만.
                    let animating = progress || left.as_millis() as u64 <= FADE_MS;
                    if animating {
                        redraw = true;
                        bump(now + Duration::from_millis(tick), &mut next);
                    } else {
                        bump(now + (left - Duration::from_millis(FADE_MS)), &mut next);
                    }
                }
                None => {
                    let bucket = now.duration_since(r.started).as_millis() as u64 / tick;
                    if bucket != r.shown_tick {
                        r.shown_tick = bucket;
                        redraw = true;
                    }
                    bump(
                        r.started + Duration::from_millis((bucket + 1) * tick),
                        &mut next,
                    );
                }
            }
        }
        (redraw, next)
    }

    fn run_at(&self, p: Point) -> Option<&Run> {
        self.runs.iter().find(|r| r.rect.contains(p))
    }

    /// 마우스 이동 — hover 상태가 바뀌면 true.
    pub(crate) fn hover(&mut self, p: Point) -> bool {
        let (line, stop, copy) = match self.run_at(p) {
            Some(r) => (
                r.line_rect.contains(p).then_some(r.id),
                (r.ended.is_none() && r.stop_rect.contains(p)).then_some(r.id),
                r.copy_rect.contains(p).then_some(r.id),
            ),
            None => (None, None, None),
        };
        let changed = line != self.hover_line || stop != self.hover_stop || copy != self.hover_copy;
        self.hover_line = line;
        self.hover_stop = stop;
        self.hover_copy = copy;
        changed
    }

    pub(crate) fn click(&mut self, p: Point) -> RunToastHit {
        let Some(r) = self.run_at(p) else {
            return RunToastHit::None;
        };
        if r.ended.is_none() && r.stop_rect.contains(p) {
            return RunToastHit::Stop;
        }
        // 복사 버튼 = 카드를 닫지 않고 SQL만 넘긴다(끝난 카드도).
        if r.copy_rect.contains(p) {
            return RunToastHit::Copy(r.sql.clone());
        }
        if r.ended.is_some() {
            let id = r.id;
            self.runs.retain(|x| x.id != id);
            self.hover_line = None;
            self.hover_stop = None;
        }
        RunToastHit::Card
    }

    /// 휠 — 누적이 영역을 넘어 스크롤이 있을 때 카드 위에서만. 먹었으면 true.
    pub(crate) fn wheel(&mut self, p: Point, delta: i32) -> bool {
        if self.overflow <= 0 || !self.stack_rect.contains(p) {
            return false;
        }
        // 픽셀 단위(한 눈금 120 = 40px · 터치패드의 잔 값도 그대로 · 사용자 09-22). 위로 굴리면(delta > 0) 최신 쪽으로.
        let step = delta * 40 / 120;
        self.scroll = (self.scroll - step).clamp(0, self.overflow);
        true
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

    /// 그리기 — 편집기 본문(`top`..`bottom` · 오른쪽 `right`) **우상단부터 아래로**(최신이 위) · 넘치는 카드는 영역에서 잘라 그린다 ·
    /// 반환 = 일반 토스트가 쌓기 시작할 y(카드가 영역을 덮지 않은 아래쪽 = 마지막 카드 아래 · 없으면 `bottom`).
    pub(crate) fn paint(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        top: i32,
        right: i32,
        bottom: i32,
        s: f32,
    ) -> i32 {
        if self.runs.is_empty() {
            self.stack_rect = Rect::default();
            self.overflow = 0;
            return bottom;
        }
        let px = |v: f32| (v * s).round() as i32;
        let (w, pad, gap, bar) = (px(440.0), px(10.0), px(6.0), px(4.0));
        dc.select_font(FontSlot::Base, false);
        let lh = dc.text_height();
        let now = Instant::now();
        let h = pad * 2 + lh * 3 + px(4.0);
        let order = stack_order(&self.runs);
        let n = order.len() as i32;
        let total = n * h + (n - 1).max(0) * gap;
        let avail = (bottom - top - gap * 2).max(h);
        self.overflow = (total - avail).max(0);
        self.scroll = self.scroll.clamp(0, self.overflow);
        let x = right - gap - w;
        // 그릴 수 있는 영역 = 편집기 본문(미니맵과 같은 높이) — 넘치는 카드는 잘라서 그린다.
        let area = Rect::new(x, top + gap, w, (bottom - top - gap * 2).max(0));
        // ★ 아래 고정 · 위로 자란다(사용자 09-22 "생성 순서는 아래에서 위로 grow up"): 가장 오래된 카드가 영역 바닥, 최신이 그 위.
        //   넘치면 최신(맨 위)이 보이도록 위에 맞추고, 스크롤만큼 전체를 올린다(아래 카드가 올라와 보인다).
        let stack_top = (area.bottom() - total).max(area.y);
        let mut y_top = stack_top - self.scroll;
        let mut tooltip: Option<(Rect, String)> = None;
        self.stack_rect = Rect::new(x, stack_top, w, total.min(area.h));
        for &i in &order {
            let card = Rect::new(x, y_top, w, h);
            y_top = card.bottom() + gap;
            let vis = card.intersection(&area);
            if vis.is_empty() {
                let r = &mut self.runs[i];
                r.rect = Rect::default();
                r.stop_rect = Rect::default();
                r.line_rect = Rect::default();
                r.copy_rect = Rect::default();
                continue;
            }
            let tip = self.paint_card(dc, th, i, card, area, now, pad, bar, lh, px(6.0), s);
            if tip.is_some() {
                tooltip = tip;
            }
        }
        // 툴팁(원문 · 카드 위쪽 · 최대 24줄) — 카드들 위에.
        if let Some((card, sql)) = tooltip {
            dc.select_font(FontSlot::Status, false);
            let lines: Vec<&str> = sql.lines().take(24).collect();
            let tw = lines.iter().map(|l| dc.text_width(l)).max().unwrap_or(0);
            let tlh = dc.text_height();
            // ★ 카드 **왼쪽 옆**(사용자 09-22 "맨 위 카드에서 잘림 · 위치가 다름"): 창 안(그리기 인자 right/bottom = 확정 영역)에서
            //   카드 상단에 맞추고, 아래로 넘치면 위로 끌어올린다 · 폭은 카드 왼쪽 여백까지.
            let host = Rect::new(0, 0, right, bottom);
            let tw = tw.min(px(720.0)).min((card.x - px(16.0)).max(px(120.0))) + px(12.0);
            let thh = tlh * lines.len().max(1) as i32 + px(8.0);
            let mut tr = Rect::new(card.x - px(8.0) - tw, card.y, tw, thh);
            if tr.bottom() > host.bottom() - px(4.0) {
                tr.y = (host.bottom() - px(4.0) - thh).max(host.y + px(4.0));
            }
            let tr = nexa_ctl::geom::nudge_into(tr, host);
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
        // 일반 토스트는 종전대로 아래에서 위로 쌓인다(카드 스택은 위쪽 · 서로 다른 끝).
        bottom
    }

    /// 카드 한 장 — 자리(hit-test)를 기록하고, hover면 툴팁 정보를 돌려준다.
    #[allow(clippy::too_many_arguments)]
    fn paint_card(
        &mut self,
        dc: &mut dyn DrawCtx,
        th: &Theme,
        i: usize,
        card: Rect,
        area: Rect,
        now: Instant,
        pad: i32,
        bar: i32,
        lh: i32,
        radius: i32,
        s: f32,
    ) -> Option<(Rect, String)> {
        let px = |v: f32| (v * s).round() as i32;
        let life = self
            .hide_progress_of(&self.runs[i], now)
            .filter(|_| self.progress);
        let (alpha, fade_to, spent, hide_after) =
            (self.alpha, self.fade_to, self.spent, self.hide_after);
        let (hover_line, hover_stop, hover_copy) =
            (self.hover_line, self.hover_stop, self.hover_copy);
        let r = &mut self.runs[i];
        let fade = match r.ended {
            Some(e) if !hide_after.is_zero() => {
                let left = hide_after.saturating_sub(now.duration_since(e));
                let ms = left.as_millis() as u64;
                if ms < FADE_MS {
                    ms as f32 / FADE_MS as f32
                } else {
                    1.0
                }
            }
            _ => 1.0,
        };
        let a = alpha * life.map_or(1.0, |p| crate::toast::life_alpha(p, fade_to, true)) * fade;
        let vis = card.intersection(&area);
        r.rect = vis;
        dc.fill_round_rect_alpha(vis, radius, th.panel_bg, a);
        dc.stroke_round_rect_alpha(vis, radius, th.border, 1.0, a);
        let color = match &r.phase {
            Phase::Running { .. } | Phase::Fetching => th.accent,
            Phase::Done { .. } => th.ok,
            Phase::Error(_) => th.danger,
            Phase::Stopped { .. } => th.warn,
        };
        // 왼쪽 상태 막대 = 남은 시간(끝난 카드): 아래를 고정하고 위에서부터 옅어진다 — 지나간 부분은
        // `ui.toast_bar_spent`(기본 30% = 70% 투명)로 남겨 "사라진다"기보다 "투명해지는" 느낌(사용자 09-22). 진행 중 = 가득.
        let full = card.h - pad * 2;
        let bar_h = crate::toast::bar_remaining(full, life);
        let bar_x = card.x + px(6.0);
        if bar_h < full {
            dc.fill_round_rect_alpha(
                Rect::new(bar_x, card.y + pad, bar, full).intersection(&area),
                px(2.0),
                color,
                a * spent,
            );
        }
        if bar_h > 0 {
            dc.fill_round_rect_alpha(
                Rect::new(bar_x, card.y + pad + (full - bar_h), bar, bar_h).intersection(&area),
                px(2.0),
                color,
                a,
            );
        }
        let tx = card.x + px(6.0) + bar + pad;
        let clip = Rect::new(card.x, card.y, card.w - pad, card.h).intersection(&area);
        // 1행: ■ 중지 + 문장 한 줄
        let stop_sz = lh;
        let stop = Rect::new(tx, card.y + pad, stop_sz, stop_sz);
        r.stop_rect = if r.ended.is_none() {
            stop.intersection(&area)
        } else {
            Rect::default()
        };
        // 중지 버튼(사용자 09-22): **누를 수 있으면 빨강**(실행 중) · 못 누르면 회색 · hover = 빨간 링을 더해 눌린다는 것을 알린다.
        let stop_color = if r.ended.is_none() {
            th.danger
        } else {
            th.text_dim
        };
        if r.ended.is_none() && hover_stop == Some(r.id) {
            dc.stroke_round_rect_alpha(stop.intersection(&area), px(3.0), th.danger, 1.0, a);
        }
        let sq = px(3.0);
        dc.fill_round_rect_alpha(
            Rect::new(stop.x + sq, stop.y + sq, stop.w - sq * 2, stop.h - sq * 2)
                .intersection(&area),
            px(2.0),
            stop_color,
            a,
        );
        let lx = stop.right() + px(8.0);
        // 1행 오른쪽 끝 = 복사 버튼(사용자 09-23 "툴팁으로 보이는 쿼리를 클릭 한 번에 복사") — 문장 줄은 그 앞에서 끝난다.
        let copy_sz = lh;
        let copy = Rect::new(card.right() - pad - copy_sz, card.y + pad, copy_sz, copy_sz);
        r.copy_rect = copy.intersection(&area);
        {
            let hovered = hover_copy == Some(r.id);
            if hovered {
                dc.fill_round_rect_alpha(copy.intersection(&area), px(3.0), th.text, 0.12 * a);
            }
            let (cr, cg, cb) = (if hovered { th.text } else { th.text_dim }).rgb();
            let ic = crate::toolicons::mi_copy();
            let img =
                nexa_ctl::theme::IconImage::from_alpha_tinted(ic.w, ic.h, &ic.alpha, (cr, cg, cb));
            let sz = px(14.0).min(copy_sz);
            dc.image_scaled(
                Rect::new(
                    copy.x + (copy.w - sz) / 2,
                    copy.y + (copy.h - sz) / 2,
                    sz,
                    sz,
                ),
                &img,
                copy.intersection(&area),
            );
        }
        let line_clip = Rect::new(lx, card.y + pad, copy.x - px(6.0) - lx, lh).intersection(&area);
        r.line_rect = line_clip;
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
        (hover_line == Some(r.id)).then(|| (card, r.sql.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(rt: &RunToast) -> Vec<u64> {
        stack_order(&rt.runs)
            .into_iter()
            .map(|i| rt.runs[i].id)
            .collect()
    }

    /// 복사 버튼(사용자 09-23): 클릭 = 그 실행의 SQL 원문 · 카드는 남는다(끝난 카드도) · hover 상태가 바뀌면 다시 그린다.
    #[test]
    fn copy_button_returns_sql_and_keeps_card() {
        let mut rt = RunToast::new();
        rt.configure(true, 2, 90);
        let c = rt
            .start("SELECT 1;\nSELECT 2;", "t".into(), 2)
            .expect("card");
        rt.finish(
            Some(c),
            Phase::Done {
                rows: Some(2),
                secs: 0.1,
                stages: String::new(),
            },
        );
        rt.runs[0].rect = Rect::new(0, 0, 200, 60);
        rt.runs[0].copy_rect = Rect::new(180, 4, 16, 16);
        assert!(rt.hover(Point { x: 185, y: 10 }));
        assert_eq!(rt.hover_copy, Some(c));
        assert!(
            matches!(rt.click(Point { x: 185, y: 10 }), RunToastHit::Copy(s) if s == "SELECT 1;\nSELECT 2;")
        );
        assert_eq!(rt.runs.len(), 1, "copy must not dismiss the card");
        // 버튼 밖 카드 본문 클릭 = 끝난 카드 닫기(종전 규칙 그대로).
        assert!(matches!(
            rt.click(Point { x: 50, y: 30 }),
            RunToastHit::Card
        ));
        assert!(rt.runs.is_empty());
    }

    /// 시작 → 진행(속도) → 완료 → 숨김 시간 뒤 사라짐 · 0초면 유지.
    #[test]
    fn lifecycle_speed_and_hide() {
        let mut rt = RunToast::new();
        rt.configure(true, 2, 90);
        let c = rt.start("SELECT *\n\tFROM t", "2026-09-17 10:00:00.000".into(), 1);
        assert_eq!(c, Some(1));
        assert!(rt.is_running());
        assert_eq!(rt.runs[0].line, "SELECT *¶ FROM t");
        rt.first_page(c, 200, 4_000_000, Duration::from_secs(2));
        assert!((rt.runs[0].speed - 2_000_000.0).abs() < 1.0);
        rt.progress(c, 400, 8_000_000);
        assert_eq!(rt.runs[0].phase, Phase::Fetching);
        rt.finish(
            c,
            Phase::Done {
                rows: Some(400),
                secs: 2.5,
                stages: "fetch 2.5s".into(),
            },
        );
        assert!(!rt.is_running());
        let ended = rt.runs[0].ended.expect("ended");
        let (_, next) = rt.tick(ended);
        assert!(next.is_some());
        let (redraw, _) = rt.tick(ended + Duration::from_secs(3));
        assert!(redraw && !rt.is_visible(), "숨김 시간 뒤 사라짐");
        let mut keep = RunToast::new();
        keep.configure(true, 0, 90);
        let k = keep.start("x", "s".into(), 1);
        keep.finish(k, Phase::Error("boom".into()));
        let e = keep.runs[0].ended.expect("ended");
        assert!(
            !keep.tick(e + Duration::from_secs(100)).0 && keep.is_visible(),
            "0 = 유지"
        );
        assert_eq!(hms(Duration::from_millis(3_661_042)), "01:01:01.042");
        keep.close();
        assert!(!keep.is_visible());
    }

    /// ★ 누적 순서(사용자 09-22): **최신이 최상단** — 시작·종료 사건이 최근인 순 · 숨겨지면 빠진다 · 포커스 이동 옵션 · 상한.
    #[test]
    fn stack_order_rules() {
        let mut rt = RunToast::new();
        rt.configure(true, 3, 90);
        let c1 = rt.start("q1", "s".into(), 1); // id 1
        let c2 = rt.start("q2", "s".into(), 1); // id 2
        let c3 = rt.start("q3", "s".into(), 1); // id 3
                                                // 위→아래: 3 · 2 · 1(최신 시작이 위).
        assert_eq!(ids(&rt), vec![3, 2, 1]);
        // 2가 끝나면 2가 맨 위로: 2 · 3 · 1.
        rt.finish(
            c2,
            Phase::Done {
                rows: Some(1),
                secs: 0.1,
                stages: String::new(),
            },
        );
        assert_eq!(ids(&rt), vec![2, 3, 1]);
        // 2가 숨겨지면 3 · 1.
        let e2 = rt.runs[1].ended.expect("ended");
        rt.tick(e2 + Duration::from_secs(3));
        assert_eq!(ids(&rt), vec![3, 1]);
        assert!(rt.is_running());
        // 포커스 이동: 스크롤을 옮겨 두고 새 사건이 오면 0(켬) · 끄면 유지.
        rt.overflow = 100;
        rt.scroll = 60;
        rt.finish(c3, Phase::Error("e".into()));
        assert_eq!(rt.scroll, 0, "follow 켬 = 맨 위로");
        rt.configure_stack(false, 30);
        rt.scroll = 60;
        rt.finish(c1, Phase::Error("e".into()));
        assert_eq!(rt.scroll, 60, "follow 끔 = 유지");
        // 상한: 2로 줄이면 가장 오래 전에 끝난 카드부터 버린다.
        rt.configure_stack(true, 2);
        assert_eq!(rt.runs.len(), 2);
        assert_eq!(ids(&rt), vec![1, 3]);
    }
    /// 남은 시간 진척: 진행 중·수동 닫기(0초)는 없음 · 끝난 뒤 0 → 1 · 켜져 있으면 카운트다운 내내 틱.
    #[test]
    fn hide_progress_and_tick_cadence() {
        let mut rt = RunToast::new();
        rt.configure(true, 4, 90);
        rt.configure_tick(30);
        let c = rt.start("x", "s".into(), 1);
        assert_eq!(
            rt.hide_progress_of(&rt.runs[0], Instant::now()),
            None,
            "진행 중"
        );
        rt.finish(c, Phase::Error("e".into()));
        let e = rt.runs[0].ended.expect("ended");
        assert_eq!(rt.hide_progress_of(&rt.runs[0], e), Some(0.0));
        assert!(
            (rt.hide_progress_of(&rt.runs[0], e + Duration::from_secs(2))
                .expect("p")
                - 0.5)
                .abs()
                < 1e-6
        );
        assert_eq!(
            rt.hide_progress_of(&rt.runs[0], e + Duration::from_secs(9)),
            Some(1.0)
        );
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
        // 수동 닫기(0초) = 진척 없음 · 휠은 넘침이 없으면 먹지 않는다.
        let mut keep = RunToast::new();
        keep.configure(true, 0, 90);
        let k = keep.start("x", "s".into(), 1);
        keep.finish(k, Phase::Error("e".into()));
        assert_eq!(keep.hide_progress_of(&keep.runs[0], Instant::now()), None);
        assert!(!keep.wheel(Point { x: 0, y: 0 }, 120));
    }
}
