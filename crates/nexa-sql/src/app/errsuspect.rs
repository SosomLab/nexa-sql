//! ★ T-372 오류 위치 고급 분석 — 호스트 배선(132차 win · 사용자 10-20).
//!
//! 순수 분석은 nsql-script [`nsql_script::errsuspect`](의존 0 · 시험 포함)에 있고, 여기서는 ① 설정·부류 게이트 ② 메타 포트(Ctrl 링크의
//! `SnapResolver` = 같은 스냅숏·같은 판정) ③ 결과를 편집기 좌표로 바꾸는 일만 한다. 서버 왕복 0 · 구문 오류(`ErrorClass::Syntax`)에만.
//! 캐럿 = **첫 번째(가장 앞) 후보**(사용자 10-20) · 없으면 종전(서버 위치 → 문장 시작 줄). Output = 후보 목록 + 서버 위치 한 줄.

use crate::app::objlink::SnapResolver;
use crate::*;
use nsql_core::{Dialect, ErrorClass};
use nsql_script::errsuspect::{analyze, near_token, Input, Suspect};
use std::time::{Duration, Instant};

/// 분석을 돌릴 조건(순수 · MC/DC 시험): 설정 켬 · 구문 오류 부류 · 문장 시작을 편집기에 맞출 수 있음(`start`).
pub(crate) fn analyze_wanted(enabled: bool, class: ErrorClass, start: Option<usize>) -> bool {
    enabled && class == ErrorClass::Syntax && start.is_some()
}

impl App {
    /// 실패한 문장의 오타 후보(위치 순 · 글자 인덱스 = 문장 기준). 메타는 그 세션 칸의 스냅숏만.
    pub(crate) fn error_suspects(
        &self,
        dialect: Dialect,
        stmt: &str,
        server_pos: Option<usize>,
        message: &str,
    ) -> Vec<Suspect> {
        let budget =
            u64::try_from(self.settings.int("editor.error_analyze_budget_ms").max(1)).unwrap_or(20);
        let deadline = Instant::now() + Duration::from_millis(budget);
        let near = near_token(message);
        let spec = self.sess.spec.clone();
        let (names, snap) = self.explorer.meta_view(spec.as_ref());
        let res = SnapResolver::new(
            names,
            &snap,
            Some(dialect),
            self.objlink_cur_schema(),
            spec.as_ref().and_then(|s| s.user.clone()),
            self.explorer.meta_account(spec.as_ref()),
        );
        let input = Input {
            sql: stmt,
            dialect: Some(dialect),
            server_pos,
            // SQL Server = 줄 번호만(위치 = 줄 시작) → 구간은 그 줄(near 토큰까지).
            pos_is_line: dialect == Dialect::Mssql,
            near: near.as_deref(),
            deadline: Some(deadline),
        };
        analyze(&input, &res)
    }

    /// Output 한 줄 글 — 후보(최대 3 · `줄:열 '원문' → 기대?`) + 서버 위치(`줄:열` · 없으면 `-`). 좌표 = 활성 편집기(1부터).
    pub(crate) fn error_suspects_text(
        &self,
        suspects: &[Suspect],
        base: Option<usize>,
        start: Option<usize>,
        server_pos: Option<usize>,
        stmt_len: usize,
    ) -> Option<String> {
        if suspects.is_empty() {
            return None;
        }
        let buf = self.editors.cur().buf();
        let lc = |ch: usize| {
            let l = buf.line_of(ch);
            format!("{}:{}", l + 1, ch - buf.line_start(l) + 1)
        };
        let mut parts: Vec<String> = Vec::new();
        for s in suspects.iter().take(3) {
            let ch = app::run::error_caret(base, start, Some(s.start), stmt_len)?;
            parts.push(format!("{} '{}' → {}?", lc(ch), s.text, s.expect));
        }
        if suspects.len() > 3 {
            parts.push(format!("+{}", suspects.len() - 3));
        }
        let server = server_pos
            .and_then(|p| app::run::error_caret(base, start, Some(p), stmt_len))
            .map_or_else(|| "-".to_string(), lc);
        Some(tf(Msg::OutErrorSuspects, &[&parts.join(" · "), &server]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MC/DC: 설정 · 부류 · 문장 시작 각각이 결과를 독립적으로 바꾼다.
    #[test]
    fn analyze_wanted_mc_dc() {
        assert!(analyze_wanted(true, ErrorClass::Syntax, Some(0)));
        assert!(
            !analyze_wanted(false, ErrorClass::Syntax, Some(0)),
            "설정 끔(향상 모드 강제 포함)"
        );
        assert!(
            !analyze_wanted(true, ErrorClass::NoColumn, Some(0)),
            "식별자 오류 = 서버 위치가 정확 · 분석 안 함"
        );
        assert!(
            !analyze_wanted(true, ErrorClass::Syntax, None),
            "문장이 정규화돼 편집기에 못 맞춤"
        );
    }
}
