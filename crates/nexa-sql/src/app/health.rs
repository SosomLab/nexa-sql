//! ★ 서버 건강 전파(docs/107 · T-313 ①): 메타 세션(탐색기)이 확정한 끊김/복귀를 **같은 끝점의 실행 세션 전부**에 투영하고 토스트·로그로
//! 알린다 — 종전에는 실행 세션의 Broken은 메타로, 메타의 실패는 실행 세션으로 전해지지 않았다(같은 서버인데 상태가 둘). ②에서
//! `ServerHealth` 레지스트리(끝점 + 세션 두 층 · `health_merge`)로 키운다.

use crate::sessions::Sess;
use crate::toast::ToastKind;
use crate::App;
use nsql_i18n::{t, tf, Msg};
use nsql_log::{LogEntry, LogKind};
use nsql_script::ConnectSpec;

/// 스펙의 끝점 글 `host:port`(파일 방언 = None) — 메타 스레드의 `ep_of`와 같은 꼴.
pub(crate) fn ep_text(spec: Option<&ConnectSpec>) -> Option<String> {
    let s = spec?;
    Some(format!("{}:{}", s.host.as_deref()?, s.port?))
}

/// 이 세션이 그 끝점에 붙어 있는가(접속돼 있고 · 끝점이 같다).
pub(crate) fn sess_on(sess: &Sess, ep: &str) -> bool {
    sess.connected && ep_text(sess.spec.as_ref()).as_deref() == Some(ep)
}

impl App {
    /// 메타 세션의 건강 사건(docs/107 §5 안내 채널): 끊김 = 오류 토스트 1회 + 로그 + 같은 끝점 세션 전부 `broken`(탭 표식·플러그·세션 창) ·
    /// 복귀 = 정보 토스트 + 로그 + 그 끝점 세션의 `broken` 해제(실제로 죽은 세션은 다음 동작의 판정이 다시 가른다).
    pub(crate) fn health_changed(&mut self, ep: &str, alive: bool, reason: &str) {
        if alive {
            let title = tf(Msg::ExpHealthUpTitle, &[ep]);
            self.log_win
                .push(LogEntry::new(LogKind::Info, title.clone()));
            self.toasts.push(ToastKind::Info, title, String::new());
            let mut any = false;
            for s in std::iter::once(&mut self.sess).chain(self.parked.iter_mut()) {
                if s.broken && sess_on(s, ep) {
                    s.broken = false;
                    any = true;
                }
            }
            if any {
                self.sync_sess_ui();
            }
            return;
        }
        let body = tf(Msg::ExpHealthDownBody, &[ep, reason]);
        self.log_win.push(LogEntry::new(
            LogKind::Error,
            format!("{} - {body}", t(Msg::ExpHealthDownTitle)),
        ));
        self.toasts.push(
            ToastKind::Error,
            t(Msg::ExpHealthDownTitle).to_string(),
            body.clone(),
        );
        self.sess.status = body;
        // 같은 끝점의 실행 세션 전부에 투영(docs/107 P2 "서버 단위 한 상태").
        let mut lines = Vec::new();
        for s in std::iter::once(&mut self.sess).chain(self.parked.iter_mut()) {
            if !s.broken && sess_on(s, ep) {
                s.broken = true;
                lines.push(tf(Msg::StSessBroken, &[&s.desc]));
            }
        }
        for l in lines {
            self.log_win.push(LogEntry::new(LogKind::Error, l));
        }
        self.sync_sess_ui();
    }
}

#[cfg(test)]
mod tests {
    use super::ep_text;
    use nsql_script::ConnectSpec;

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
}
