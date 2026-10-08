//! ★ 서버 건강 전파(docs/107 · T-313 ①~③): 끝점(host:port) 단위 **레지스트리** `ServerHealth` + 순수 판정 `health_merge`(MC/DC) + 전파 한 자리
//! `App::health_event` — 실행 세션 워커의 `ConnOutcome::Broken/Alive`와 메타 세션의 `Resp::Health`가 **둘 다 여기로** 들어와 토스트·로그 ·
//! 같은 끝점 세션 전부(`Sess.broken` 투영) · 탐색기 헤더("끊김 hh:mm") · 복귀를 한 곳에서 맞춘다. 종전에는 실행 세션의 Broken은 메타로,
//! 메타의 실패는 실행 세션으로 전해지지 않았다(같은 서버인데 상태가 둘).
//!
//! 두 층(107 §9): **끝점 층** = 여기(네트워크·서버 다운 · 그 끝점의 모든 세션) · **세션 층** = `Sess.broken`(DBA kill · 유휴 끊김처럼 그 접속
//! 하나만 · 워커가 SYN 없이 접속성 오류만 봤을 때) — 투영 = `Sess.broken || 끝점 Broken`. 유휴 닫기(`idle_closed`)는 건강 사건이 아니다.

use crate::sessions::Sess;
use crate::toast::ToastKind;
use crate::App;
use nsql_i18n::{t, tf, Msg};
use nsql_log::{LogEntry, LogKind};
use nsql_script::ConnectSpec;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 끝점의 건강 상태(107 §3-1).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum HealthState {
    #[default]
    Alive,
    /// 의심(OS 네트워크 변경 뒤 · 다음 동작이 판정) — 알림 없음.
    Suspect,
    Broken,
}

/// 건강 사건(어디서든 올 수 있다 · 107 §3-1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum HealthEvent {
    /// 끊김 확정(SYN 실패 · 접속성 오류 · 호출 상한) — 사유 글.
    Down(String),
    /// 네트워크 경로가 바뀌었다(L0 · ⑤) — 판정은 다음 동작이.
    NetChanged,
    /// 다시 닿았다(SYN 성공 · 접속 성공 · 질의 성공).
    Up,
}

/// 전파할 알림(순수 판정의 결과).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Notify {
    None,
    /// 처음 끊김(Alive/Suspect → Broken) — 토스트·로그·투영.
    Down,
    /// 끊겼다가 복귀(Broken → Alive) — 토스트·로그·해제.
    Up,
}

/// ★ 순수 판정(107 §3-1 · MC/DC): Down → Broken(처음이면 알림) · NetChanged → Alive만 Suspect(알림 없음) · Up → Alive(끊겼던 것이면 복귀 알림).
pub(crate) fn health_merge(prev: HealthState, ev: &HealthEvent) -> (HealthState, Notify) {
    match ev {
        HealthEvent::Down(_) => (
            HealthState::Broken,
            if prev == HealthState::Broken {
                Notify::None
            } else {
                Notify::Down
            },
        ),
        HealthEvent::NetChanged => (
            if prev == HealthState::Alive {
                HealthState::Suspect
            } else {
                prev
            },
            Notify::None,
        ),
        HealthEvent::Up => (
            HealthState::Alive,
            if prev == HealthState::Broken {
                Notify::Up
            } else {
                Notify::None
            },
        ),
    }
}

/// 끝점 하나의 기록.
#[derive(Clone, Debug)]
pub(crate) struct EpHealth {
    pub state: HealthState,
    /// 지금 상태가 된 시각(헤더 "끊김 hh:mm" = 벽시계는 따로 `since_wall`).
    pub since: Instant,
    pub since_wall: std::time::SystemTime,
    pub reason: String,
    /// 마지막 끊김 토스트 시각(억제 창 `net.notify_quiet_secs` · D-266).
    pub last_toast: Option<Instant>,
}

/// 끝점 키 → 기록(App 소유 · 수명 = 그 끝점에 세션이 하나라도 있는 동안 · 107 §9-5).
#[derive(Default)]
pub(crate) struct ServerHealth {
    map: HashMap<String, EpHealth>,
}

impl ServerHealth {
    pub(crate) fn state(&self, ep: &str) -> HealthState {
        self.map.get(ep).map_or(HealthState::Alive, |h| h.state)
    }

    pub(crate) fn is_broken(&self, ep: &str) -> bool {
        self.state(ep) == HealthState::Broken
    }

    pub(crate) fn get(&self, ep: &str) -> Option<&EpHealth> {
        self.map.get(ep)
    }

    /// 사건 적용 → 알림. 상태가 바뀌면 시각·사유를 새로.
    pub(crate) fn apply(&mut self, ep: &str, ev: &HealthEvent, now: Instant) -> Notify {
        let prev = self.state(ep);
        let (next, notify) = health_merge(prev, ev);
        let e = self.map.entry(ep.to_string()).or_insert_with(|| EpHealth {
            state: HealthState::Alive,
            since: now,
            since_wall: std::time::SystemTime::now(),
            reason: String::new(),
            last_toast: None,
        });
        if next != prev {
            e.since = now;
            e.since_wall = std::time::SystemTime::now();
        }
        e.state = next;
        if let HealthEvent::Down(r) = ev {
            e.reason.clone_from(r);
        }
        if next == HealthState::Alive {
            // 멀쩡한 끝점은 기록을 들고 있을 이유가 없다 — 복귀 뒤 다시 끊기면 새 사건(토스트) · 덤프에 `Alive` 줄이 남지 않는다(협업 V1 bin101 ②).
            self.map.remove(ep);
        }
        notify
    }

    /// 끊김 토스트를 지금 띄워도 되는가(억제 창) — 띄우면 시각을 기록한다.
    pub(crate) fn toast_allowed(&mut self, ep: &str, now: Instant, quiet: Duration) -> bool {
        let Some(e) = self.map.get_mut(ep) else {
            return true;
        };
        let ok = e
            .last_toast
            .is_none_or(|t| now.saturating_duration_since(t) >= quiet);
        if ok {
            e.last_toast = Some(now);
        }
        ok
    }

    /// 수명(107 §9-5): 세션이 하나도 안 붙은 끝점의 기록은 지운다 — 나중에 같은 서버에 다시 붙을 때 옛 Broken이 남지 않게.
    pub(crate) fn retain_eps(&mut self, live: impl Fn(&str) -> bool) {
        self.map.retain(|ep, _| live(ep));
    }

    /// 진단 한 줄씩(기동 명령 `health.dump`).
    pub(crate) fn dump(&self) -> String {
        let mut v: Vec<_> = self.map.iter().collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        v.iter()
            .map(|(ep, h)| {
                format!(
                    "{ep}|{:?}|{}s|{}",
                    h.state,
                    h.since.elapsed().as_secs(),
                    h.reason
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 스펙의 끝점 글 `host:port`(파일 방언 = None) — 메타 스레드의 `ep_of`와 같은 꼴.
pub(crate) fn ep_text(spec: Option<&ConnectSpec>) -> Option<String> {
    let s = spec?;
    Some(format!("{}:{}", s.host.as_deref()?, s.port?))
}

/// 이 세션이 그 끝점에 붙어 있는가(접속돼 있고 · 끝점이 같다).
pub(crate) fn sess_on(sess: &Sess, ep: &str) -> bool {
    sess.connected && ep_text(sess.spec.as_ref()).as_deref() == Some(ep)
}

/// 벽시계 `hh:mm`(헤더 "끊김 hh:mm").
pub(crate) fn hhmm(t: std::time::SystemTime) -> String {
    let l = nsql_log::local_at(t);
    format!("{:02}:{:02}", l.hour, l.min)
}

impl App {
    /// 메타 세션의 건강 사건(종전 입구 · `ExplorerAction::Health`) — 레지스트리 길로.
    pub(crate) fn health_changed(&mut self, ep: &str, alive: bool, reason: &str) {
        let ev = if alive {
            HealthEvent::Up
        } else {
            HealthEvent::Down(reason.to_string())
        };
        self.health_event(ep, ev);
    }

    /// ★ 전파 한 자리(107 §3 · §5): 사건 → 순수 판정 → (처음 끊김) 오류 토스트(억제 창 · 클릭 = 다시 연결) + 로그 + 같은 끝점 세션 전부
    /// `broken` + 탐색기 헤더 · (복귀) 정보 토스트 + 로그 + 해제 · 세션 UI 동기.
    pub(crate) fn health_event(&mut self, ep: &str, ev: HealthEvent) {
        let now = Instant::now();
        let notify = self.health.apply(ep, &ev, now);
        match notify {
            Notify::None => {
                // 이미 끊긴 끝점에 새로 붙은 세션이 있을 수 있다 — 투영만 맞춘다(알림 없음).
                if self.health.is_broken(ep) {
                    let changed = self.project_broken(ep, true);
                    if changed {
                        self.sync_sess_ui();
                    }
                }
            }
            Notify::Down => {
                let reason = match &ev {
                    HealthEvent::Down(r) => r.clone(),
                    _ => String::new(),
                };
                let body = tf(Msg::ExpHealthDownBody, &[ep, &reason]);
                self.log_win.push(LogEntry::new(
                    LogKind::Error,
                    format!("{} - {body}", t(Msg::ExpHealthDownTitle)),
                ));
                let quiet =
                    Duration::from_secs(self.settings.int("net.notify_quiet_secs").max(0) as u64);
                if self.health.toast_allowed(ep, now, quiet) {
                    // 클릭 = 그 끝점 다시 연결(③ · `menu_action("net.reconnect:<ep>")`).
                    self.toasts.push_action(
                        ToastKind::Error,
                        t(Msg::ExpHealthDownTitle).to_string(),
                        format!("{body}\n{}", t(Msg::ExpHealthReconnectHint)),
                        &format!("net.reconnect:{ep}"),
                    );
                }
                self.sess.status = body;
                self.project_broken(ep, true);
                let wall = self.health.get(ep).map(|h| h.since_wall);
                self.explorer.set_ep_broken(ep, wall);
                self.sync_sess_ui();
            }
            Notify::Up => {
                let title = tf(Msg::ExpHealthUpTitle, &[ep]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, title.clone()));
                self.toasts.push(ToastKind::Info, title, String::new());
                self.project_broken(ep, false);
                self.explorer.set_ep_broken(ep, None);
                // 복귀한 끝점의 메타 세션은 다음 요청 때 스스로 재개한다(`resume`) — 트리가 비어 있으면 지금 청한다.
                self.explorer.kick_meta_for_ep(ep);
                self.sync_sess_ui();
            }
        }
        self.health_gc();
        self.redraw();
    }

    /// 같은 끝점의 실행 세션 전부에 투영(107 P2 "서버 단위 한 상태") — 바뀐 세션이 있으면 true · 로그 한 줄씩.
    fn project_broken(&mut self, ep: &str, broken: bool) -> bool {
        let mut lines = Vec::new();
        for s in std::iter::once(&mut self.sess).chain(self.parked.iter_mut()) {
            if s.broken != broken && sess_on(s, ep) {
                s.broken = broken;
                if broken {
                    lines.push(tf(Msg::StSessBroken, &[&s.desc]));
                }
            }
        }
        let changed = !lines.is_empty() || !broken;
        for l in lines {
            self.log_win.push(LogEntry::new(LogKind::Error, l));
        }
        changed
    }

    /// 수명(107 §9-5): 세션이 하나도 없는 끝점의 기록을 지운다(`sync_sess_ui`·건강 사건 뒤).
    pub(crate) fn health_gc(&mut self) {
        let eps: Vec<String> = self
            .all_sess()
            .filter_map(|s| ep_text(s.spec.as_ref()))
            .collect();
        self.health.retain_eps(|ep| eps.iter().any(|e| e == ep));
    }

    /// ★ 토스트 [다시 연결](③ · D-271): 그 끝점의 실행 세션 전부 = 조용한 재접속(비밀번호 없는 일회성 세션은 워커가 `PasswordNeeded`로
    /// 기존 입력 창을 띄운다) · 메타 세션 = 재개 요청.
    pub(crate) fn reconnect_endpoint(&mut self, ep: &str) {
        let ids: Vec<u64> = self
            .all_sess()
            .filter(|s| {
                !s.closing && s.spec.is_some() && ep_text(s.spec.as_ref()).as_deref() == Some(ep)
            })
            .map(|s| s.id)
            .collect();
        // 끊겼던 끝점이므로 "같은 서버면 유지"(`connect.reconnect_same` 끔)를 적용하지 않고 **진짜 재접속**(협업 V1 bin101 ④ = 유지 길로
        //   가서 `접속 중…`에 머물렀다).
        let again = true;
        for id in ids {
            self.with_sess(id, |a| {
                if a.sess.busy {
                    return;
                }
                if let Some(spec) = a.sess.spec.clone() {
                    a.connect_quietly(spec, again);
                }
            });
        }
        self.explorer.kick_meta_for_ep(ep);
        self.sync_sess_ui();
        self.redraw();
    }

    /// ★ L0 네트워크 신호(⑤ · 107 §3-2): 1초 안 신호는 합친다 · 세션이 있는 끝점 전부 `NetChanged`(Alive → Suspect · 알림 없음) ·
    /// 메타 스레드는 `Req::NetChanged`(다음 카탈로그 요청 = 판정부터) · 워커는 다음 실행의 `preflight`(run.rs = 끝점이 Alive가 아니면).
    pub(crate) fn net_changed(&mut self) {
        let now = Instant::now();
        if self
            .net_last
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(1))
        {
            return;
        }
        self.net_last = Some(now);
        let mut eps: Vec<String> = self
            .all_sess()
            .filter(|s| s.connected)
            .filter_map(|s| ep_text(s.spec.as_ref()))
            .collect();
        eps.sort();
        eps.dedup();
        if eps.is_empty() {
            return;
        }
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            tf(Msg::LogNetChanged, &[&eps.len().to_string()]),
        ));
        for ep in &eps {
            self.health_event(ep, HealthEvent::NetChanged);
        }
        self.explorer.net_changed_all();
    }

    /// 진단(기동 명령 `health.dump:<경로>`): 레지스트리 · 세션별 끝점/broken · 탐색기 칸 끊김.
    pub(crate) fn health_dump_text(&self) -> String {
        let mut out = self.health.dump();
        for s in self.all_sess() {
            out.push_str(&format!(
                "\nsess {}|{}|connected={}|broken={}|private={}",
                s.id,
                ep_text(s.spec.as_ref()).unwrap_or_default(),
                s.connected,
                s.broken,
                s.is_private()
            ));
        }
        out.push('\n');
        out.push_str(&self.explorer.health_dump());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_text_follows_meta_thread_shape() {
        let mut s = ConnectSpec::default();
        assert_eq!(ep_text(Some(&s)), None, "파일 방언 = 끝점 없음");
        s.host = Some("db.local".into());
        assert_eq!(ep_text(Some(&s)), None, "포트 없으면 없음");
        s.port = Some(1521);
        assert_eq!(ep_text(Some(&s)).as_deref(), Some("db.local:1521"));
        assert_eq!(ep_text(None), None);
    }

    /// 107 §3-1 판정 표(MC/DC — 사건 × 이전 상태).
    #[test]
    fn merge_table() {
        use HealthState::*;
        let down = HealthEvent::Down("x".into());
        assert_eq!(
            health_merge(Alive, &down),
            (Broken, Notify::Down),
            "처음 끊김 = 알림"
        );
        assert_eq!(
            health_merge(Suspect, &down),
            (Broken, Notify::Down),
            "의심 뒤 확정 = 알림"
        );
        assert_eq!(
            health_merge(Broken, &down),
            (Broken, Notify::None),
            "이미 끊김 = 조용히"
        );
        assert_eq!(
            health_merge(Alive, &HealthEvent::NetChanged),
            (Suspect, Notify::None)
        );
        assert_eq!(
            health_merge(Suspect, &HealthEvent::NetChanged),
            (Suspect, Notify::None)
        );
        assert_eq!(
            health_merge(Broken, &HealthEvent::NetChanged),
            (Broken, Notify::None),
            "끊김은 그대로"
        );
        assert_eq!(
            health_merge(Broken, &HealthEvent::Up),
            (Alive, Notify::Up),
            "복귀 = 알림"
        );
        assert_eq!(
            health_merge(Suspect, &HealthEvent::Up),
            (Alive, Notify::None),
            "의심 해소 = 조용히"
        );
        assert_eq!(health_merge(Alive, &HealthEvent::Up), (Alive, Notify::None));
    }

    /// 레지스트리: 사건 적용 · 억제 창 · 수명.
    #[test]
    fn registry_apply_quiet_window_and_gc() {
        let mut r = ServerHealth::default();
        let t0 = Instant::now();
        assert_eq!(
            r.apply("a:1", &HealthEvent::Down("vpn".into()), t0),
            Notify::Down
        );
        assert!(r.is_broken("a:1"));
        assert_eq!(r.get("a:1").map(|h| h.reason.as_str()), Some("vpn"));
        assert_eq!(
            r.apply("a:1", &HealthEvent::Down("again".into()), t0),
            Notify::None
        );
        assert_eq!(
            r.get("a:1").map(|h| h.reason.as_str()),
            Some("again"),
            "사유는 최신"
        );
        // 억제 창: 처음 허용 · 창 안 거부 · 창 지나면 허용.
        let q = Duration::from_secs(60);
        assert!(r.toast_allowed("a:1", t0, q));
        assert!(!r.toast_allowed("a:1", t0 + Duration::from_secs(10), q));
        assert!(r.toast_allowed("a:1", t0 + Duration::from_secs(61), q));
        // 복귀 = 기록 제거(협업 V1 bin101 ②).
        assert_eq!(r.apply("a:1", &HealthEvent::Up, t0), Notify::Up);
        assert!(!r.is_broken("a:1"));
        assert!(r.get("a:1").is_none(), "복귀 뒤 줄 없음");
        // 멀쩡한 끝점의 NetChanged = 의심(기록 생김 · 알림 없음) · Up = 조용히 Alive.
        assert_eq!(r.apply("b:2", &HealthEvent::NetChanged, t0), Notify::None);
        assert_eq!(r.state("b:2"), HealthState::Suspect);
        assert_eq!(r.apply("b:2", &HealthEvent::Up, t0), Notify::None);
        // 수명: 세션이 없는 끝점은 지워진다.
        r.apply("c:3", &HealthEvent::Down("x".into()), t0);
        r.retain_eps(|ep| ep == "a:1");
        assert!(!r.is_broken("c:3"));
        assert!(r.get("c:3").is_none());
    }

    #[test]
    fn hhmm_shape() {
        let s = hhmm(std::time::SystemTime::now());
        assert_eq!(s.len(), 5);
        assert_eq!(&s[2..3], ":");
    }
}
