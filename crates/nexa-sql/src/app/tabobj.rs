//! ★ 탭 우클릭 메뉴 "객체 탐색기에서 보기"(사용자 10-08): 편집기 내용의 **주석을 뺀 첫 실행 문장**이 객체를 만들고·바꾸고·지우는
//! DDL이면(`nsql_core::ddl_target` = CREATE · ALTER · DROP · RENAME · COMMENT ON) 그 대상을 탭 메뉴에서 객체 탐색기로 찾아간다
//! (`ExplorerSet::reveal` · Ctrl 링크 "객체 탐색기에서 보기"와 같은 길).
//!
//! - 판정 시점 = 탭이 열릴 때 · 탭의 연결이 바뀔 때 · 본문이 바뀔 때 — 틱에서 탭별 열쇠 `(본문 세대, 세션 id, 연결 여부)`가 바뀐 탭만
//!   다시 판정한다(비용 = 탭 수만큼의 해시 비교 · 판정은 머리 [`HEAD_CHARS`]글자만 가른다 · 큰 파일도 상수 비용).
//! - 연결이 없거나 끊겼으면 항목은 보이되 흐리다(탭이 DDL 문서임은 알 수 있게 · 누르면 상태줄 안내).
//! - 스키마가 적혀 있지 않으면 그 탭 세션의 현재 스키마(서버가 답한 값 → `?schema=` → 사용자) · SQL Server `DB.스키마`는 DB와 스키마로.

use nsql_core::{DdlTarget, Dialect};
use nsql_i18n::{t, tf, Msg};

use crate::explorer::RevealTarget;
use crate::{App, Focus};

/// 판정에 읽는 머리 글자 수 — 머리 주석 블록 + 첫 문장이면 충분하다(이보다 긴 머리 주석 뒤의 첫 문장은 못 본다 · 상수 비용 우선).
pub(crate) const HEAD_CHARS: usize = 16 * 1024;

/// 앞머리의 공백·`-- 줄 주석`·`/* 블록 주석 */`을 걷는다(순수) — `ddl_target`은 첫 토큰이 동사여야 한다.
pub(crate) fn strip_leading_comments(mut s: &str) -> &str {
    loop {
        let trimmed = s.trim_start();
        if let Some(rest) = trimmed.strip_prefix("--") {
            s = rest.split_once('\n').map_or("", |(_, r)| r);
        } else if let Some(rest) = trimmed.strip_prefix("/*") {
            s = rest.split_once("*/").map_or("", |(_, r)| r);
        } else {
            return trimmed;
        }
    }
}

/// 첫 **실행 문장**(SQL*Plus식 명령 `CONNECT`·`VAR`…과 주석은 건너뜀)의 DDL 대상 — 순수. 첫 실행 문장이 DDL이 아니면 None.
pub(crate) fn first_ddl_target(text: &str, dialect: Option<Dialect>) -> Option<DdlTarget> {
    let items = nsql_script::split_script_in(text, dialect);
    let stmt = items
        .iter()
        .find(|it| matches!(it.kind, nsql_script::ItemKind::Sql(_)))?;
    let body = strip_leading_comments(&stmt.text);
    nsql_core::ddl_target(body, dialect.unwrap_or(Dialect::Oracle))
}

/// 메뉴 라벨에 넣을 이름(`스키마.이름` 또는 `이름`).
pub(crate) fn qualified(t: &DdlTarget) -> String {
    match &t.schema {
        Some(s) if !s.is_empty() => format!("{s}.{}", t.name),
        _ => t.name.clone(),
    }
}

fn head_text(buf: &nexa_ctl::edit::TextBuf, max_chars: usize) -> String {
    buf.slice_string(0, buf.len().min(max_chars))
}

impl App {
    /// 틱(북마크 `bm_tick` 옆): 열쇠 `(본문 세대, 세션 id, 연결 여부)`가 바뀐 탭만 다시 판정해 Editors에 라벨·활성을 넣는다.
    pub(crate) fn tab_obj_tick(&mut self) {
        let n = self.editors.tab_count();
        let mut live: Vec<u64> = Vec::with_capacity(n);
        for i in 0..n {
            let id = self.editors.tab_id(i);
            live.push(id);
            let sid = self.sess_id_for_tab(id);
            let (connected, dialect) = self.sess_by_id(sid).map_or((false, None), |s| {
                (s.connected && !s.broken, Some(s.dialect))
            });
            let (key, head) = {
                let Some(tb) = self.editors.tab_box(i) else {
                    continue;
                };
                let key = (tb.rev(), sid, connected);
                if self.tab_obj_cache.get(&id) == Some(&key) {
                    continue;
                }
                (key, head_text(tb.buf(), HEAD_CHARS))
            };
            let label = first_ddl_target(&head, dialect)
                .map(|t| tf(Msg::MnTabRevealObject, &[&qualified(&t)]));
            self.tab_obj_cache.insert(id, key);
            self.editors.set_tab_obj(id, label.map(|l| (l, connected)));
        }
        self.tab_obj_cache.retain(|id, _| live.contains(id));
    }

    /// 탭 메뉴 "객체 탐색기에서 보기": 그 탭의 세션 기준으로 대상을 풀어 탐색기를 보이게 하고 그 서버 칸에서 찾아 선택한다.
    pub(crate) fn tab_reveal_object(&mut self, i: usize) {
        let id = self.editors.tab_id(i);
        let Some(head) = self
            .editors
            .tab_box(i)
            .map(|tb| head_text(tb.buf(), HEAD_CHARS))
        else {
            return;
        };
        let sid = self.sess_id_for_tab(id);
        let Some((connected, dialect, spec, cur)) = self.sess_by_id(sid).map(|s| {
            let cur = s
                .cur_schema
                .clone()
                .filter(|x| !x.is_empty())
                .or_else(|| {
                    s.spec
                        .as_ref()
                        .and_then(|p| p.schema.clone().or_else(|| p.user.clone()))
                })
                .filter(|x| !x.is_empty());
            (s.connected && !s.broken, s.dialect, s.spec.clone(), cur)
        }) else {
            return;
        };
        if !connected {
            self.sess.status = t(Msg::ExpNotConnected).to_string();
            self.redraw();
            return;
        }
        let Some(target) = first_ddl_target(&head, Some(dialect)) else {
            return;
        };
        // 스키마·사용자 DDL은 탐색기 노드가 없다(`kind_of` = None) — 조용히 끝(메뉴는 라벨만 보였던 셈).
        let Some(kind) = super::compare::kind_of(target.kind) else {
            return;
        };
        let schema = target.schema.or(cur).unwrap_or_default();
        // SQL Server 다른 DB 객체 = `DB.스키마`(objlink와 같은 규칙 ⑭).
        let (db, schema) = match (dialect, schema.split_once('.')) {
            (Dialect::Mssql, Some((d, s))) if !s.is_empty() => (Some(d.to_string()), s.to_string()),
            _ => (None, schema),
        };
        if !self.explorer.is_visible() {
            self.menu_action("view.explorer");
        }
        let ok = self.explorer.reveal(
            spec.as_ref(),
            RevealTarget {
                db,
                schema,
                kind,
                name: target.name,
                member: None,
            },
        );
        if ok {
            self.set_focus(Focus::Explorer);
            self.explorer_actions();
        }
        self.redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{DdlKind, DdlVerb};

    #[test]
    fn leading_comments_are_skipped() {
        let s = "  -- 머리\n/* 블록\n주석 */\n  -- 둘\nCREATE TABLE t (a INT)";
        assert_eq!(strip_leading_comments(s), "CREATE TABLE t (a INT)");
        assert_eq!(strip_leading_comments("-- only"), "");
    }

    #[test]
    fn first_executable_statement_decides() {
        // 머리 주석 + CREATE OR REPLACE PROCEDURE = 프로시저(사용자 캡처 모양).
        let sql = "-- 생성\nCREATE OR REPLACE PROCEDURE BISCM.SP_X (p IN NUMBER) AS\nBEGIN NULL; END;\n/\nSELECT 1 FROM dual;";
        let t = first_ddl_target(sql, Some(Dialect::Oracle)).expect("ddl");
        assert_eq!(t.verb, DdlVerb::Create);
        assert_eq!(t.kind, DdlKind::Procedure);
        assert_eq!(t.schema.as_deref(), Some("BISCM"));
        assert_eq!(t.name, "SP_X");
        assert_eq!(qualified(&t), "BISCM.SP_X");
        // 첫 실행 문장이 SELECT면 뒤에 DDL이 있어도 대상 아님.
        assert!(
            first_ddl_target("SELECT 1 FROM dual;\nDROP TABLE t;", Some(Dialect::Oracle)).is_none()
        );
        // 명령(CONNECT)은 건너뛰고 첫 SQL을 본다 · DROP도 대상.
        let t =
            first_ddl_target("CONNECT dev\nDROP TABLE s.t;", Some(Dialect::Oracle)).expect("drop");
        assert_eq!(t.verb, DdlVerb::Drop);
        assert_eq!(qualified(&t), "s.t");
        // 빈 글·주석만 = None.
        assert!(first_ddl_target("-- 아무것도\n", None).is_none());
    }
}
