//! ★ 탭 우클릭 메뉴 "객체 탐색기에서 보기"(사용자 10-08): 편집기 내용의 **주석을 뺀 첫 실행 문장**이 객체를 만들고·바꾸고·지우는
//! DDL이면(`nsql_core::ddl_target` = CREATE · ALTER · DROP · RENAME · COMMENT ON) 그 대상을 탭 메뉴에서 객체 탐색기로 찾아간다
//! (`ExplorerSet::reveal` · Ctrl 링크 "객체 탐색기에서 보기"와 같은 길).
//!
//! - 판정 시점 = 탭이 열릴 때 · 탭의 연결이 바뀔 때 · 본문이 바뀔 때 — 틱에서 탭별 열쇠 `(본문 세대, 세션 id, 연결 여부)`가 바뀐 탭만
//!   문장을 다시 가르고(머리 [`HEAD_CHARS`]글자만 · 큰 파일도 상수 비용), 메타 조회(해시 1~2번)는 틱마다 다시 한다(메타가 뒤늦게
//!   읽혀도 항목이 나타나게).
//! - ★ **항목은 메타에서 실제로 풀릴 때만**(사용자 10-08 2차 "목록을 확인할 대상이 아니면 표시하지 않도록"): 연결이 없거나 메타에 그 객체가
//!   없으면 항목 자체가 없다. 적힌 스키마에 없으면 **다른 스키마의 같은 이름**(종류 일치)을 찾아 그쪽으로(사용자 캡처 = `BISCM.SP_X`인데
//!   이 서버엔 `BISCM_SB`에만 있음 · 상태줄에 "BISCM에 없음 · BISCM_SB에 있음" 안내) · 스키마를 안 적었으면 그 탭 세션의 현재 스키마.
//! - SQL Server `DB.스키마`는 DB와 스키마로(objlink ⑭와 같은 규칙).

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

/// `스키마.이름` 또는 `이름`(시험·진단용 — 메뉴 라벨에는 이름을 넣지 않는다 · 사용자 10-08 2차).
#[cfg(test)]
pub(crate) fn qualified(t: &DdlTarget) -> String {
    match &t.schema {
        Some(s) if !s.is_empty() => format!("{s}.{}", t.name),
        _ => t.name.clone(),
    }
}

/// 같은 종류로 치는가(패키지 본문은 패키지 노드로 찾는다 · 그 밖은 같아야).
fn kind_matches(want: nsql_catalog::ObjectKind, have: nsql_catalog::ObjectKind) -> bool {
    use nsql_catalog::ObjectKind as K;
    want == have
        || matches!(
            (want, have),
            (K::PackageBody, K::Package) | (K::Package, K::PackageBody)
        )
}

fn head_text(buf: &nexa_ctl::edit::TextBuf, max_chars: usize) -> String {
    buf.slice_string(0, buf.len().min(max_chars))
}

/// 탭별 판정 캐시: 탭 id → (열쇠 `(본문 세대, 세션 id, 연결 여부)`, 가른 DDL 대상).
pub(crate) type TabObjCache = std::collections::HashMap<u64, ((u64, u64, bool), Option<DdlTarget>)>;

/// 탭 하나의 풀린 대상(메타에서 확인된 것) — 클릭 때 그대로 쓴다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TabObj {
    pub target: RevealTarget,
    /// 적힌 스키마와 다른 스키마에서 찾았으면 `Some((적힌 스키마, 찾은 스키마))` — 상태줄 안내.
    pub moved: Option<(String, String)>,
}

impl App {
    /// 그 탭 세션의 현재 스키마(서버가 답한 값 → `?schema=` → 사용자).
    fn tab_cur_schema(sess: &crate::sessions::Sess) -> Option<String> {
        sess.cur_schema
            .clone()
            .filter(|x| !x.is_empty())
            .or_else(|| {
                sess.spec
                    .as_ref()
                    .and_then(|p| p.schema.clone().or_else(|| p.user.clone()))
            })
            .filter(|x| !x.is_empty())
    }

    /// DDL 대상을 그 탭 세션의 메타로 푼다 — 적힌 스키마(없으면 현재 스키마) → 없으면 다른 스키마의 같은 이름(종류 일치 · 첫 것).
    fn resolve_tab_obj(&self, sid: u64, t: &DdlTarget) -> Option<TabObj> {
        let s = self.sess_by_id(sid)?;
        if !s.connected || s.broken {
            return None;
        }
        let kind = super::compare::kind_of(t.kind)?;
        let cur = Self::tab_cur_schema(s);
        let dialect = s.dialect;
        let (names, snap) = self.explorer.meta_view(s.spec.as_ref());
        let stated = t.schema.as_deref().or(cur.as_deref());
        let direct =
            snap.lookup_resolvable_from(names, t.schema.as_deref(), cur.as_deref(), &t.name);
        let (id, moved) = match direct {
            Some(id) => (id, None),
            None => {
                let id = snap
                    .lookup_any_schema(names, &t.name)
                    .into_iter()
                    .find(|&id| snap.object(id).is_some_and(|o| kind_matches(kind, o.kind)))?;
                let found = names.get(snap.object(id)?.schema).to_string();
                (id, Some((stated.unwrap_or_default().to_string(), found)))
            }
        };
        let o = snap.object(id)?;
        let schema = names.get(o.schema).to_string();
        let name = names.get(o.name).to_string();
        let (db, schema) = match (dialect, schema.split_once('.')) {
            (Dialect::Mssql, Some((d, sc))) if !sc.is_empty() => {
                (Some(d.to_string()), sc.to_string())
            }
            _ => (None, schema),
        };
        Some(TabObj {
            target: RevealTarget {
                db,
                schema,
                kind: o.kind,
                name,
                member: None,
            },
            moved,
        })
    }

    /// 틱(북마크 `bm_tick` 옆): 열쇠 `(본문 세대, 세션 id, 연결 여부)`가 바뀐 탭만 문장을 다시 가르고, 메타 조회는 매 틱(가볍다) →
    /// Editors에 항목 유무를 넣는다.
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
            let (key, parsed) = {
                let Some(tb) = self.editors.tab_box(i) else {
                    continue;
                };
                let key = (tb.rev(), sid, connected);
                match self.tab_obj_cache.get(&id) {
                    Some((k, parsed)) if *k == key => (key, parsed.clone()),
                    _ => (
                        key,
                        first_ddl_target(&head_text(tb.buf(), HEAD_CHARS), dialect),
                    ),
                }
            };
            self.tab_obj_cache.insert(id, (key, parsed.clone()));
            let resolved = parsed.as_ref().and_then(|t| self.resolve_tab_obj(sid, t));
            let changed = self.tab_obj_target.get(&id) != resolved.as_ref();
            if changed {
                match resolved {
                    Some(o) => {
                        self.tab_obj_target.insert(id, o);
                        self.editors
                            .set_tab_obj(id, Some((t(Msg::MnTabRevealObject).to_string(), true)));
                    }
                    None => {
                        self.tab_obj_target.remove(&id);
                        self.editors.set_tab_obj(id, None);
                    }
                }
            }
        }
        self.tab_obj_cache.retain(|id, _| live.contains(id));
        self.tab_obj_target.retain(|id, _| live.contains(id));
    }

    /// 탭 메뉴 "객체 탐색기에서 보기": 틱이 풀어 둔 대상을 그 탭의 세션 칸에서 찾아 선택(탐색기 자동 표시 · 포커스).
    pub(crate) fn tab_reveal_object(&mut self, i: usize) {
        let id = self.editors.tab_id(i);
        let Some(obj) = self.tab_obj_target.get(&id).cloned() else {
            return;
        };
        let sid = self.sess_id_for_tab(id);
        let spec = self.sess_by_id(sid).and_then(|s| s.spec.clone());
        if !self.explorer.is_visible() {
            self.menu_action("view.explorer");
        }
        let ok = self.explorer.reveal(spec.as_ref(), obj.target.clone());
        if ok {
            self.set_focus(Focus::Explorer);
            self.explorer_actions();
            if let Some((stated, found)) = obj.moved {
                self.sess.status = tf(Msg::ObjLinkElsewhere, &[&stated, &found]);
            }
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

    #[test]
    fn package_body_matches_package_node() {
        use nsql_catalog::ObjectKind as K;
        assert!(kind_matches(K::PackageBody, K::Package));
        assert!(kind_matches(K::Procedure, K::Procedure));
        assert!(!kind_matches(K::Procedure, K::Table));
    }
}
