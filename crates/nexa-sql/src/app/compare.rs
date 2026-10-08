//! App — CREATE 문 ↔ 실제 객체 비교(T-283 1단계 · docs/19 §6 · 사용자 10-06).
//!
//! 편집기의 `CREATE … <종류> <이름>` 문장을 서버의 실제 DDL(탐색기 Generate SQL ▸ DDL과 같은 원천 = `nsql_catalog::generate` ·
//! 100 §5 보수적 생성)과 견준다. 1단계 = 새 컨트롤 0 — **비교 탭**(읽기 전용 뷰 탭): 본문 = 편집기 문장(정규화) ·
//! `TextBox::set_baseline(서버 DDL)` → 거터 띠(추가 `ok` · 변경 `warn` · 위에서 삭제 `danger` 쐐기) + 미니맵 · 머리 줄 한 줄 =
//! `일치` 또는 `N군데 다름`. 서버 DDL 본문은 팔레트 "서버 DDL 열기"(`obj.compare_server`)로 따로 탭에(동시 편집 칸으로 나란히).
//! 지연 로딩(61 §1-8) = 누를 때만 DDL을 청한다(메타 스레드 · 결과는 `ExplorerAction::Preview`를 가로챈다 = 삭제 백업과 같은 꼴).
//! 2단계(diff 뷰어 부품 · 맞은편 줄 내용)는 19 §6-2.

use crate::*;

/// 비교 대상(순수 판정 결과).
#[derive(Clone, Debug)]
pub(crate) struct CompareTarget {
    pub(crate) owner: nsql_catalog::ObjectInfo,
    /// 편집기 문장 원문(조각 그대로).
    pub(crate) stmt: String,
    /// 문장에 스키마가 적혀 있는가 — 생성 DDL의 한정(`GenOpts.qualified`)을 맞춘다(헛 차이 줄이기 · 19 §6-3).
    pub(crate) qualified: bool,
}

/// `nsql_core::DdlKind` → 카탈로그 객체 종류(스키마는 객체가 아님).
pub(crate) fn kind_of(k: nsql_core::DdlKind) -> Option<nsql_catalog::ObjectKind> {
    use nsql_catalog::ObjectKind as O;
    use nsql_core::DdlKind as D;
    Some(match k {
        D::Table => O::Table,
        D::View => O::View,
        D::MaterializedView => O::MaterializedView,
        D::Index => O::Index,
        D::Sequence => O::Sequence,
        D::Procedure => O::Procedure,
        D::Function => O::Function,
        D::Package => O::Package,
        D::PackageBody => O::PackageBody,
        D::Trigger => O::Trigger,
        D::Synonym => O::Synonym,
        D::Type => O::Type,
        D::Schema => return None,
    })
}

/// 문장이 `CREATE [OR REPLACE] <종류> <이름>`이면 비교 대상(순수 · 19 §6-4). 스키마가 없으면 세션 기본 스키마.
pub(crate) fn compare_target(
    stmt: &str,
    dialect: Dialect,
    default_schema: Option<&str>,
) -> Option<CompareTarget> {
    let t = nsql_core::ddl_target(stmt, dialect)?;
    if t.verb != nsql_core::DdlVerb::Create {
        return None;
    }
    let kind = kind_of(t.kind)?;
    let qualified = t.schema.is_some();
    let schema = t
        .schema
        .clone()
        .or_else(|| default_schema.map(str::to_string))
        .unwrap_or_default();
    Some(CompareTarget {
        owner: nsql_catalog::ObjectInfo {
            db: String::new(),
            schema,
            name: t.name,
            kind,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        },
        stmt: stmt.to_string(),
        qualified,
    })
}

/// 비교 전 정규화(19 §6-3 · 순수): 줄 끝 공백 · 탭 → 공백 4 · 빈 줄 연속 → 하나 · 끝의 `;`/`/`·공백 제거 ·
/// `ignore_ws`면 줄 안 공백 연속도 하나로(들여쓰기 차이 무시) · `canon`이면 [`canon_line`](편집 가능 키워드 · 따옴표 식별자).
/// 앱은 표시용 원문도 필요해 [`normalize_pairs`]를 쓴다 — 이 꼴은 시험의 비교용 요약.
#[cfg(test)]
pub(crate) fn normalize_ddl(text: &str, ignore_ws: bool, canon: bool) -> Vec<String> {
    normalize_pairs(text, ignore_ws, canon)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// 서버 DDL의 꾸밈을 걷는다(순수 · 설정 `compare.canon` · 협업 V1 bin111 (a)): Oracle `DBMS_METADATA`가 붙이는 `EDITIONABLE`/
/// `NONEDITIONABLE` 낱말을 빼고, 따옴표 식별자 `"BISCM"."SP_TEST1"`은 안이 **대소문자 섞이지 않은 보통 식별자**일 때만 따옴표를
/// 벗긴다(`"MyTab"`처럼 섞인 것은 따옴표가 뜻을 가지므로 그대로). 양쪽(편집기·서버)에 같이 적용하므로 "같은 뜻 = 같은 줄"만 맞춘다.
pub(crate) fn canon_line(l: &str) -> String {
    let mut out = String::with_capacity(l.len());
    let cs: Vec<char> = l.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        // 문자열 리터럴 `'…'`은 그대로(안의 낱말·따옴표는 데이터 · `''` 이스케이프는 두 리터럴로 읽혀도 결과는 같다).
        if c == '\'' {
            let end = cs[i + 1..]
                .iter()
                .position(|&x| x == '\'')
                .map_or(cs.len(), |j| i + 1 + j + 1);
            out.extend(&cs[i..end]);
            i = end;
            continue;
        }
        if c == '"' {
            if let Some(j) = cs[i + 1..].iter().position(|&x| x == '"') {
                let inner: String = cs[i + 1..i + 1 + j].iter().collect();
                if plain_ident(&inner) {
                    out.push_str(&inner);
                    i += j + 2;
                    continue;
                }
            }
            out.push(c);
            i += 1;
            continue;
        }
        if c.is_alphabetic() {
            let st = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            let w: String = cs[st..i].iter().collect();
            let up = w.to_ascii_uppercase();
            if up == "EDITIONABLE" || up == "NONEDITIONABLE" {
                // 낱말 + 뒤따르는 공백 하나를 함께 뺀다(`CREATE OR REPLACE NONEDITIONABLE PROCEDURE` → `CREATE OR REPLACE PROCEDURE`).
                if i < cs.len() && cs[i] == ' ' {
                    i += 1;
                }
                continue;
            }
            out.push_str(&w);
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// 따옴표를 벗겨도 뜻이 같은 식별자인가 — 글자/`_` 시작 · 영숫자·`_`·`$`·`#` · 대소문자가 섞이지 않음.
fn plain_ident(s: &str) -> bool {
    let mut it = s.chars();
    let Some(f) = it.next() else { return false };
    if !(f.is_ascii_alphabetic() || f == '_') {
        return false;
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '#'))
    {
        return false;
    }
    let has_up = s.chars().any(|c| c.is_ascii_uppercase());
    let has_lo = s.chars().any(|c| c.is_ascii_lowercase());
    !(has_up && has_lo)
}

/// [`normalize_ddl`] + 표시용 원문 쌍 — `(비교용, 표시용)` · 표시용 = 그 줄의 원문(줄 끝 공백 제거 · 탭 → 공백 4 · 들여쓰기 유지).
/// diff 뷰어는 비교는 앞 것으로 · 본문은 뒤 것으로 보인다(협업 V1 bin111 (c) = 공백을 걷은 글이 보였다).
pub(crate) fn normalize_pairs(text: &str, ignore_ws: bool, canon: bool) -> Vec<(String, String)> {
    let t = text
        .trim()
        .trim_end_matches(|c: char| c == ';' || c == '/' || c.is_whitespace());
    let mut out: Vec<(String, String)> = Vec::new();
    let mut prev_blank = false;
    // 머리의 `-- 주석`·빈 줄은 뺀다(생성 DDL은 `-- t2 definition` 머리 줄을 단다 · 협업 V1 H1 = 그 줄이 "삭제"로 잡혔다).
    let body_at = t
        .lines()
        .position(|l| {
            let l = l.trim();
            !(l.is_empty() || l.starts_with("--"))
        })
        .unwrap_or(0);
    for raw in t.lines().skip(body_at) {
        let show = raw.trim_end().replace('\t', "    ");
        let mut l = show.clone();
        if ignore_ws {
            l = l.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        if canon {
            l = canon_line(&l);
        }
        let blank = l.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        prev_blank = blank;
        out.push((l, show));
    }
    while out.last().is_some_and(|(l, _)| l.trim().is_empty()) {
        out.pop();
    }
    out
}

/// 표시용 이름 — 스키마가 없으면(SQLite) 이름만(`.t2`가 아니라 `t2` · 협업 V1 H).
fn object_label(o: &nsql_catalog::ObjectInfo) -> String {
    if o.schema.is_empty() {
        o.name.clone()
    } else {
        format!("{}.{}", o.schema, o.name)
    }
}

/// 차이 수(순수) = 기준선(서버) 대비 편집기 줄의 표시 수(추가·변경·위에서 삭제 · nexa-ctl `diff_lines`).
pub(crate) fn diff_count(base: &[String], cur: &[String]) -> usize {
    let cur_ref: Vec<&str> = cur.iter().map(String::as_str).collect();
    nexa_ctl::controls::textbox::diff_lines(base, &cur_ref).len()
}

impl App {
    /// 캐럿이 든 문장(선택 무시 · 원문 조각 · 현재 문 실행과 같은 길).
    fn compare_stmt_at_caret(&mut self) -> Option<String> {
        let caret = self.ed_mut().caret();
        self.compare_stmt_at_char(caret)
    }

    /// 글자 인덱스가 든 문장(Ctrl 링크 자리 → 그 문장).
    pub(crate) fn compare_stmt_at_char(&mut self, ch: usize) -> Option<String> {
        let full = self.ed_mut().text();
        let byte_pos = full.char_indices().nth(ch).map_or(full.len(), |(b, _)| b);
        nsql_script::statement_at_in(&full, byte_pos, Some(self.sess.dialect))
            .map(|it| full[it.span].to_string())
    }

    /// 명령 `obj.compare`(팔레트 · hover 카드 버튼 · 기동 명령): 캐럿 문장이 CREATE면 서버 DDL을 청한다(결과 = `compare_take`).
    pub(crate) fn obj_compare(&mut self) {
        let Some(stmt) = self.compare_stmt_at_caret() else {
            self.sess.status = t(Msg::StCompareNoTarget).into();
            self.redraw();
            return;
        };
        self.obj_compare_stmt(stmt);
    }

    /// 문장으로 비교 시작 — 대상이 아니거나 접속이 없으면 상태줄만.
    pub(crate) fn obj_compare_stmt(&mut self, stmt: String) {
        let ds = self.meta_default_schema();
        let Some(target) = compare_target(&stmt, self.sess.dialect, ds.as_deref()) else {
            self.sess.status = t(Msg::StCompareNoTarget).into();
            self.redraw();
            return;
        };
        let Some(server) = self.sess.spec.clone() else {
            self.sess.status = t(Msg::StCompareNoServer).into();
            self.redraw();
            return;
        };
        let mut opts = gen_opts_from(&self.settings);
        opts.full_ddl = true;
        opts.qualified = target.qualified;
        let owner = target.owner.clone();
        let label = object_label(&owner);
        self.compare_wait = Some(target);
        if self
            .explorer
            .gen_object_on(&server, owner, nsql_catalog::GenWhat::Ddl, opts)
        {
            self.sess.status = tf(Msg::StCompareWait, &[&label]);
        } else {
            self.compare_wait = None;
            self.sess.status = t(Msg::StCompareNoServer).into();
        }
        self.redraw();
    }

    /// DDL 생성 결과가 비교용이면 가로챈다(`ExplorerAction::Preview` · 삭제 백업과 같은 꼴) — true = 가로챘다.
    pub(crate) fn compare_take(
        &mut self,
        spec: &nsql_catalog::GenSpec,
        r: &Result<String, String>,
    ) -> bool {
        let Some(w) = self.compare_wait.as_ref() else {
            return false;
        };
        if spec.what != nsql_catalog::GenWhat::Ddl
            || spec.owner.schema != w.owner.schema
            || spec.owner.name != w.owner.name
        {
            return false;
        }
        let Some(w) = self.compare_wait.take() else {
            return false;
        };
        match r {
            Err(e) => {
                self.sess.status = tf(Msg::StGenFailed, &[e]);
                self.redraw();
            }
            Ok(ddl) => self.compare_open(w, ddl),
        }
        true
    }

    /// 지금 활성 탭이 2-pane diff 비교 탭인가(`compare:*` 뷰 탭 · `compare.view = diff`로 열렸을 때).
    pub(crate) fn compare_is_diff_tab(&self) -> bool {
        self.editors
            .active_view()
            .is_some_and(|k| k.starts_with("compare:") && k == self.compare_diff_key)
    }

    /// 명령 `compare.next`/`compare.prev`: diff 뷰어의 다음/이전 덩어리.
    pub(crate) fn compare_step(&mut self, forward: bool) {
        if !self.compare_is_diff_tab() || !self.diff_view.step(forward) {
            self.sess.status = t(Msg::StCompareNoHunk).into();
        }
        self.redraw();
    }

    /// 비교 탭 열기 — 본문 = 편집기 문장(정규화) · 기준선 = 서버 DDL(정규화) · 머리 줄 = 일치/N군데 다름(둘 다 같은 머리 줄).
    /// `compare.view = diff`(기본 · T-283 2단계) = 2-pane diff 뷰어(맞은편 줄 내용 · 줄 배경 · 스크롤 동기 · 덩어리 이동) ·
    /// `inline` = 1단계(거터 표식만).
    fn compare_open(&mut self, w: CompareTarget, ddl: &str) {
        let ignore_ws = self.settings.flag("compare.ignore_ws");
        let canon = self.settings.flag("compare.canon");
        let (cur, cur_show): (Vec<String>, Vec<String>) =
            normalize_pairs(&w.stmt, ignore_ws, canon)
                .into_iter()
                .unzip();
        let (base, base_show): (Vec<String>, Vec<String>) =
            normalize_pairs(ddl, ignore_ws, canon).into_iter().unzip();
        let n = diff_count(&base, &cur);
        let obj = object_label(&w.owner);
        if self.settings.get("compare.view").unwrap_or("diff") == "diff" {
            let title = format!("⇄ {}", w.owner.name);
            let key = format!("compare:{obj}");
            if !self.editors.open_view_tab(&key, &title) && !self.tab_room() {
                return;
            }
            self.diff_view.set(
                &cur,
                &base,
                &cur_show,
                &base_show,
                &obj,
                (
                    t(Msg::CompareLabelEditor).to_string(),
                    t(Msg::CompareLabelServer).to_string(),
                ),
            );
            // 자리 탭의 숨은 상자도 읽기 전용(어느 길로든 그 상자의 메뉴·입력이 닿으면 §214 규칙대로 쓰기 항목 비활성).
            let i = self.editors.active();
            self.editors.set_read_only(i, true);
            self.compare_diff_key = key;
            let hunks = self.diff_view.hunk_count();
            self.compare_last = Some((w.owner.name, ddl.to_string(), hunks));
            self.sess.status = if hunks == 0 {
                t(Msg::StCompareSame).into()
            } else {
                tf(Msg::StCompareDiff2, &[&hunks.to_string()])
            };
            self.set_focus(Focus::Editor);
            self.diff_view.set_focused(true);
            self.redraw();
            return;
        }
        let head = if n == 0 {
            format!("-- {}", tf(Msg::CompareHeadSame, &[&obj]))
        } else {
            format!("-- {}", tf(Msg::CompareHeadDiff, &[&obj, &n.to_string()]))
        };
        let head = format!("{head} · {}", t(Msg::CompareHeadHint));
        let text = std::iter::once(head.clone())
            .chain(cur)
            .collect::<Vec<_>>()
            .join("\n");
        let base_text = std::iter::once(head)
            .chain(base)
            .collect::<Vec<_>>()
            .join("\n");
        let title = format!("⇄ {}", w.owner.name);
        let key = format!("compare:{obj}");
        if !self.editors.open_view_tab(&key, &title) && !self.tab_room() {
            return;
        }
        let i = self.editors.active();
        if let Some(tb) = self.editors.tab_box_mut(i) {
            tb.set_read_only(false);
            tb.set_text(&text);
            tb.goto_line(1);
            tb.mark_saved();
            // 뷰 탭은 경로가 없어 `refresh_baseline`이 기준선을 주지 않는다 → 직접(큰 파일 모드·`editor.diff_marks` 토글은 지운다).
            tb.set_baseline(Some(&base_text));
        }
        self.editors.set_read_only(i, true);
        self.compare_last = Some((w.owner.name, ddl.to_string(), n));
        self.sess.status = if n == 0 {
            t(Msg::StCompareSame).into()
        } else {
            tf(Msg::StCompareDiff, &[&n.to_string()])
        };
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// 명령 `obj.compare_server`: 마지막 비교의 서버 DDL 본문을 읽기 전용 탭으로(동시 편집 칸으로 나란히 두고 본다).
    pub(crate) fn open_compare_server(&mut self) {
        let Some((name, ddl, _)) = self.compare_last.clone() else {
            self.sess.status = t(Msg::StCompareNone).into();
            self.redraw();
            return;
        };
        let key = format!("compare-server:{name}");
        let title = format!("⇄ {name} (server)");
        if !self.editors.open_view_tab(&key, &title) && !self.tab_room() {
            return;
        }
        let i = self.editors.active();
        if let Some(tb) = self.editors.tab_box_mut(i) {
            tb.set_read_only(false);
            tb.set_text(&ddl);
            tb.goto_line(1);
            tb.mark_saved();
        }
        self.editors.set_read_only(i, true);
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// 자체 시험 덤프(`compare.dump:<파일>`): `same=<bool> hunks=<n> name=<이름>` 또는 `none`.
    pub(crate) fn compare_dump(&self, path: &str) {
        let mut text = match &self.compare_last {
            Some((name, _, n)) => format!("same={} hunks={n} name={name}\n", *n == 0),
            None => "none\n".to_string(),
        };
        if self.compare_is_diff_tab() {
            text.push_str(&self.diff_view.dump());
            text.push('\n');
        }
        let _ = std::fs::write(path, text);
    }

    /// Ctrl 링크 `k`가 든 문장이 그 이름의 CREATE면 그 문장(hover 카드 "실제 객체와 비교" 버튼의 활성·동작).
    pub(crate) fn objlink_compare_stmt(&mut self, k: usize) -> Option<String> {
        let (start, name) = {
            let l = self.objlinks.links.get(k)?;
            (l.range.0, l.name.clone())
        };
        let stmt = self.compare_stmt_at_char(start)?;
        let ds = self.meta_default_schema();
        let t = compare_target(&stmt, self.sess.dialect, ds.as_deref())?;
        t.owner.name.eq_ignore_ascii_case(&name).then_some(stmt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 대상 판정 MC/DC(19 §6-5): CREATE만 · 종류 사상 · 스키마 없으면 기본 스키마 · 적혀 있으면 `qualified`.
    #[test]
    fn target_rules() {
        let t = compare_target("CREATE TABLE emp (id INT)", Dialect::Oracle, Some("HR"))
            .expect("create");
        assert_eq!(
            (
                t.owner.schema.as_str(),
                t.owner.name.as_str(),
                t.owner.kind,
                t.qualified
            ),
            ("HR", "emp", nsql_catalog::ObjectKind::Table, false)
        );
        let t = compare_target(
            "CREATE OR REPLACE VIEW scott.v1 AS SELECT 1 FROM dual",
            Dialect::Oracle,
            Some("HR"),
        )
        .expect("view");
        assert_eq!(
            (t.owner.schema.as_str(), t.owner.kind, t.qualified),
            ("scott", nsql_catalog::ObjectKind::View, true)
        );
        assert!(
            compare_target("DROP TABLE emp", Dialect::Oracle, None).is_none(),
            "CREATE만"
        );
        assert!(compare_target("SELECT 1", Dialect::Oracle, None).is_none());
        assert!(
            compare_target("CREATE SCHEMA s", Dialect::Postgres, None).is_none(),
            "스키마는 객체가 아님"
        );
        let t = compare_target("CREATE TABLE t (a INT)", Dialect::Mssql, None).expect("no schema");
        assert_eq!(
            t.owner.schema, "",
            "기본 스키마 없음 = 빈 값(탐색기가 dbo로)"
        );
    }

    /// 정규화: 끝 `;`/`/` · 줄 끝 공백 · 탭 · 빈 줄 연속 · `ignore_ws`면 줄 안 공백 연속도 하나.
    #[test]
    fn normalize_rules() {
        let a = normalize_ddl(
            "CREATE TABLE t (\n\tid  INT,  \n\n\n  name VARCHAR2(10)\n)\n;\n",
            false,
            false,
        );
        assert_eq!(
            a,
            vec![
                "CREATE TABLE t (",
                "    id  INT,",
                "",
                "  name VARCHAR2(10)",
                ")"
            ]
        );
        let b = normalize_ddl(
            "CREATE TABLE t (\n  id INT,\n  name VARCHAR2(10)\n)\n/",
            true,
            false,
        );
        assert_eq!(
            b,
            vec!["CREATE TABLE t (", "id INT,", "name VARCHAR2(10)", ")"]
        );
        // 같은 뜻 다른 들여쓰기 = ignore_ws면 차이 0.
        let c = normalize_ddl(
            "CREATE TABLE t (\n\tid INT,\n\tname VARCHAR2(10)\n);",
            true,
            false,
        );
        assert_eq!(diff_count(&b, &c), 0);
        // 컬럼 하나 다름 = 1군데.
        let d = normalize_ddl(
            "CREATE TABLE t (\n  id INT,\n  name VARCHAR2(20)\n)",
            true,
            false,
        );
        assert_eq!(diff_count(&b, &d), 1);
        // 생성 DDL 머리 주석 + 빈 줄 + 끝 `;` = 차이 아님(협업 V1 bin27 H1 · SQLite `-- t2 definition`).
        let s = normalize_ddl(
            "-- t2 definition\n\nCREATE TABLE t2 (id INTEGER, name TEXT);",
            true,
            false,
        );
        let e = normalize_ddl("CREATE TABLE t2 (id INTEGER, name TEXT)", true, false);
        assert_eq!(s, e);
        assert_eq!(diff_count(&s, &e), 0);
    }

    /// `compare.canon`(협업 V1 bin111 (a)): `NONEDITIONABLE`/`EDITIONABLE` 무시 · 따옴표 식별자는 대소문자 안 섞인 것만 벗김 ·
    /// 표시용 원문은 들여쓰기 그대로.
    #[test]
    fn canon_rules() {
        assert_eq!(
            canon_line("CREATE OR REPLACE NONEDITIONABLE PROCEDURE \"BISCM\".\"SP_TEST1\" ("),
            "CREATE OR REPLACE PROCEDURE BISCM.SP_TEST1 ("
        );
        assert_eq!(
            canon_line("create editionable function \"f1\""),
            "create function f1"
        );
        // 섞인 대소문자·문자열 리터럴·낱말 일부는 손대지 않는다.
        assert_eq!(
            canon_line("\"MyTab\" 'NONEDITIONABLE x'"),
            "\"MyTab\" 'NONEDITIONABLE x'"
        );
        assert_eq!(canon_line("EDITIONABLEX \"A B\""), "EDITIONABLEX \"A B\"");
        let srv = normalize_ddl(
            "CREATE OR REPLACE NONEDITIONABLE PROCEDURE \"BISCM\".\"SP_TEST1\" AS\nBEGIN\n  NULL;\nEND;\n/",
            true,
            true,
        );
        let ed = normalize_ddl(
            "CREATE OR REPLACE PROCEDURE BISCM.SP_TEST1 AS\nBEGIN\n    NULL;\nEND;",
            true,
            true,
        );
        assert_eq!(srv, ed);
        // 끄면 차이로 센다.
        assert_ne!(
            normalize_ddl("CREATE NONEDITIONABLE PROCEDURE \"A\".\"B\"", true, false),
            normalize_ddl("CREATE PROCEDURE A.B", true, false)
        );
        // 표시용 = 원문 들여쓰기(탭 → 4칸) · 비교용 = 접힘.
        let pairs = normalize_pairs("CREATE TABLE t (\n\tid INT\n)", true, true);
        assert_eq!(pairs[1].0, "id INT");
        assert_eq!(pairs[1].1, "    id INT");
    }
}
