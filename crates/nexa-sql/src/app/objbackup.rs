//! ★ **객체 변경 전 백업**(docs/114 §7 · §7-2 · 사용자 10-15 "DROP뿐 아니라 변경되는 전체 동작에 각각 설정 · 기본 켬 · 서버별 일괄 ·
//! 파일명 = 스키마·객체·년월일시분초ms"): 편집기·객체 소스 탭(F5)·탐색기 삭제 창에서 **서버로 나가기 직전**에 문장을 훑어
//! `DROP` · `CREATE OR REPLACE`/`CREATE OR ALTER`/`ALTER` · `TRUNCATE`/`RENAME` 대상 객체의 **현재 DDL**(탐색기 Generate SQL ▸ DDL과
//! 같은 원천 = 보수적 생성 100 §5)을 파일로 받은 뒤 실행을 이어 간다. 설정은 **전역 하나 · 전 서버 동일**(서버 유형 무관 · 사용자 결정) ·
//! 동작별 켬/끔 `backup.object_drop`/`_replace`/`_truncate`(기본 켬) · 보관 `backup.object_days`(30 · D-302) + 객체별 최신 N
//! `backup.object_keep`(D-305) · 정리 = 기동 · 백업 직후 · `backup.prune_interval_min` 주기.
//!
//! 흐름(비동기 · 탐색기 메타 스레드): `run_text_in` 게이트 → [`App::objbk_plan`](대상 목록) → 대상마다 `gen_object_on(Ddl)` →
//! `ExplorerAction::Preview` 가로채기 [`App::objbk_take`](파일 쓰기) → 전부 끝 = [`App::objbk_finish`] → 같은 세션에서 원래 실행 재진입
//! (한 번짜리 건너뜀 깃발 `objbk_skip_once` + 운영 2단 확인 통과 깃발 `prod_confirmed_once`). 실패 = 운영 접속이면 **실행 막음** ·
//! 그 밖 = 경고 + 계속(114 §7). 없는 객체(새 객체 · DDL 없음)는 건너뜀.
use crate::*;
use nsql_catalog::ObjectInfo;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// 백업을 일으키는 동작(파일 머리 주석 `action:` · 설정 키 묶음).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BkAction {
    Drop,
    /// `CREATE OR REPLACE` · `CREATE OR ALTER`(SQL Server) · 객체 소스 탭 F5 재생성.
    Replace,
    Alter,
    Rename,
    Truncate,
}

impl BkAction {
    pub(crate) fn code(self) -> &'static str {
        match self {
            BkAction::Drop => "drop",
            BkAction::Replace => "replace",
            BkAction::Alter => "alter",
            BkAction::Rename => "rename",
            BkAction::Truncate => "truncate",
        }
    }

    /// 이 동작을 켜고 끄는 설정 키(사용자 10-15 선택 셋 = DROP · CREATE OR REPLACE/ALTER/F5 · TRUNCATE/RENAME).
    pub(crate) fn setting(self) -> &'static str {
        match self {
            BkAction::Drop => "backup.object_drop",
            BkAction::Replace | BkAction::Alter => "backup.object_replace",
            BkAction::Rename | BkAction::Truncate => "backup.object_truncate",
        }
    }
}

/// 백업 대상 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BkTarget {
    pub owner: ObjectInfo,
    pub action: BkAction,
}

/// 설정 스냅숏(순수 함수 입력).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BkFlags {
    pub drop: bool,
    pub replace: bool,
    pub truncate: bool,
}

impl BkFlags {
    /// 동작의 설정 키로 켬/끔을 본다(키 ↔ 묶음은 `BkAction::setting` 한 자리).
    fn allows(self, a: BkAction) -> bool {
        match a.setting() {
            "backup.object_drop" => self.drop,
            "backup.object_replace" => self.replace,
            _ => self.truncate,
        }
    }
}

fn unquote(s: &str) -> String {
    s.trim_matches(|c| matches!(c, '"' | '[' | ']' | '`'))
        .to_string()
}

/// `TRUNCATE [TABLE] [스키마.]이름`(순수 · 머리만) — nsql-core `ddl_target`은 TRUNCATE를 다루지 않는다(목록이 안 바뀌어 탐색기 갱신 대상이 아님).
fn truncate_target(stmt: &str) -> Option<(Option<String>, String)> {
    let mut it = stmt.split_whitespace();
    if !it.next()?.eq_ignore_ascii_case("TRUNCATE") {
        return None;
    }
    let mut tok = it.next()?;
    if tok.eq_ignore_ascii_case("TABLE") {
        tok = it.next()?;
    }
    let tok = tok.trim_end_matches(';');
    let parts: Vec<&str> = tok.split('.').collect();
    match parts.as_slice() {
        [n] => Some((None, unquote(n))),
        [s, n] => Some((Some(unquote(s)), unquote(n))),
        [_, s, n] => Some((Some(unquote(s)), unquote(n))),
        _ => None,
    }
}

/// ★ 식별자 접힘(10-16 · 사용자 "운영 모드 CREATE에 백업이 안 생김"): 따옴표 없는 이름은 Oracle = 대문자 · PostgreSQL = 소문자로 저장된다 —
/// 쓴 그대로(`nsqlt_x`) 찾으면 카탈로그(`NSQLT_X`)에 없어 "새 객체"로 오판해 백업을 건너뛰었다. 문장 안에서 `"이름"`(또는 `[이름]`·`` `이름` ``)으로
/// 인용됐으면 그대로 · 아니면 방언 규칙으로 접는다(SQL Server·MySQL·SQLite = 대소문자 무시 카탈로그 → 그대로).
fn fold_ident(stmt: &str, name: &str, dialect: Dialect) -> String {
    let quoted = stmt.contains(&format!("\"{name}\""))
        || stmt.contains(&format!("[{name}]"))
        || stmt.contains(&format!("`{name}`"));
    if quoted {
        return name.to_string();
    }
    match dialect {
        Dialect::Oracle => name.to_uppercase(),
        Dialect::Postgres => name.to_lowercase(),
        _ => name.to_string(),
    }
}

/// `CREATE OR REPLACE …` / `CREATE OR ALTER …`(SQL Server)인가 — 새 객체만 만드는 보통 `CREATE`는 백업할 원본이 없다.
fn is_create_replace(stmt: &str) -> bool {
    let head: String = stmt
        .split_whitespace()
        .take(4)
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>()
        .join(" ");
    head.starts_with("CREATE OR REPLACE") || head.starts_with("CREATE OR ALTER")
}

/// 순수: 실행할 문장들 → 백업 대상(설정이 끈 동작 제외 · 보통 `CREATE` 제외 · 같은 객체는 한 번 · 스키마 없으면 세션 기본 스키마).
pub(crate) fn backup_targets(
    items: &[String],
    dialect: Dialect,
    default_schema: Option<&str>,
    flags: BkFlags,
) -> Vec<BkTarget> {
    let mut out: Vec<BkTarget> = Vec::new();
    let mut push =
        |schema: Option<String>, name: String, kind: nsql_catalog::ObjectKind, action: BkAction| {
            if !flags.allows(action) || name.is_empty() {
                return;
            }
            let schema = schema
                .or_else(|| default_schema.map(str::to_string))
                .unwrap_or_default();
            let dup = out.iter().any(|t| {
                t.owner.kind == kind
                    && t.owner.name.eq_ignore_ascii_case(&name)
                    && t.owner.schema.eq_ignore_ascii_case(&schema)
            });
            if dup {
                return;
            }
            out.push(BkTarget {
                owner: ObjectInfo {
                    db: String::new(),
                    schema,
                    name,
                    kind,
                    status: String::new(),
                    modified: String::new(),
                    extra: String::new(),
                },
                action,
            });
        };
    for stmt in items {
        let s = stmt.trim();
        if let Some((schema, name)) = truncate_target(s) {
            let schema = schema.map(|x| fold_ident(s, &x, dialect));
            let name = fold_ident(s, &name, dialect);
            push(
                schema,
                name,
                nsql_catalog::ObjectKind::Table,
                BkAction::Truncate,
            );
            continue;
        }
        let Some(t) = nsql_core::ddl_target(s, dialect) else {
            continue;
        };
        let action = match t.verb {
            nsql_core::DdlVerb::Drop => BkAction::Drop,
            nsql_core::DdlVerb::Alter => BkAction::Alter,
            nsql_core::DdlVerb::Rename => BkAction::Rename,
            nsql_core::DdlVerb::Create if is_create_replace(s) => BkAction::Replace,
            nsql_core::DdlVerb::Create | nsql_core::DdlVerb::Comment => continue,
        };
        let Some(kind) = super::compare::kind_of(t.kind) else {
            continue;
        };
        let schema = t.schema.map(|x| fold_ident(s, &x, dialect));
        let name = fold_ident(s, &t.name, dialect);
        push(schema, name, kind, action);
    }
    out
}

/// "그 객체가 없다"로 읽히는 DDL 생성 오류(새 객체 · 이미 없는 객체 = 백업할 원본 없음 → 건너뜀).
pub(crate) fn looks_missing(e: &str) -> bool {
    let l = e.to_lowercase();
    [
        "not found",
        "not available",
        "does not exist",
        "doesn't exist",
        "no such",
        "invalid object name",
        "ora-31603",
        "ora-00942",
        "ora-04043",
        "ora-01418",
    ]
    .iter()
    .any(|n| l.contains(n))
}

/// 진행 중인 백업 + 미뤄 둔 실행.
pub(crate) struct ObjBackupRun {
    src: String,
    line_base: usize,
    all: bool,
    whole: bool,
    sid: u64,
    server: ConnectSpec,
    profile: String,
    prod: bool,
    dialect: Dialect,
    queue: VecDeque<BkTarget>,
    cur: Option<BkTarget>,
    files: Vec<PathBuf>,
    skipped: usize,
    warned: usize,
    started: Instant,
}

/// 응답을 기다리는 상한(메타 호출 상한보다 넉넉히) — 지나면 실패 경로.
const WAIT_MAX: Duration = Duration::from_secs(90);

fn label(o: &ObjectInfo) -> String {
    format!(
        "{}.{} ({})",
        o.schema,
        o.name,
        o.kind.code().to_ascii_uppercase()
    )
}

impl App {
    pub(crate) fn objbk_flags(&self) -> BkFlags {
        BkFlags {
            drop: self.settings.flag("backup.object_drop"),
            replace: self.settings.flag("backup.object_replace"),
            truncate: self.settings.flag("backup.object_truncate"),
        }
    }

    /// `run_text_in` 게이트 — 대상이 있으면 백업을 시작하고 true(호출자는 돌아간다 · 끝나면 같은 인자로 재진입).
    pub(crate) fn objbk_plan(
        &mut self,
        src: &str,
        line_base: usize,
        all: bool,
        whole: bool,
    ) -> bool {
        let flags = self.objbk_flags();
        if !(flags.drop || flags.replace || flags.truncate) {
            return false;
        }
        let items = split_items(src, self.sess.dialect);
        // 스키마 없는 문장 = 세션 `?schema=` → 없으면 탐색기가 아는 그 서버의 현재 스키마(SQLite `main` · Oracle 로그인 스키마).
        let ds = self
            .meta_default_schema()
            .or_else(|| self.explorer.current_schema(self.sess.spec.as_ref()));
        let targets = backup_targets(&items, self.sess.dialect, ds.as_deref(), flags);
        if targets.is_empty() {
            return false;
        }
        let Some(server) = self.sess.spec.clone() else {
            return false;
        };
        let prod = server.env == Some(nsql_script::ConnEnv::Prod);
        self.objbk = Some(ObjBackupRun {
            src: src.to_string(),
            line_base,
            all,
            whole,
            sid: self.sess.id,
            profile: self.sess.profile.clone(),
            prod,
            dialect: self.sess.dialect,
            server,
            queue: targets.into(),
            cur: None,
            files: Vec::new(),
            skipped: 0,
            warned: 0,
            started: Instant::now(),
        });
        self.objbk_next();
        true
    }

    /// 다음 대상의 DDL을 청한다 · 남은 것이 없으면 마무리.
    fn objbk_next(&mut self) {
        loop {
            let Some(run) = self.objbk.as_mut() else {
                return;
            };
            let Some(tg) = run.queue.pop_front() else {
                self.objbk_finish();
                return;
            };
            let server = run.server.clone();
            let owner = tg.owner.clone();
            run.cur = Some(tg);
            run.started = Instant::now();
            let mut opts = gen_opts_from(&self.settings);
            opts.full_ddl = true;
            opts.qualified = true;
            if self
                .explorer
                .gen_object_on(&server, owner.clone(), nsql_catalog::GenWhat::Ddl, opts)
            {
                self.sess.status = tf(Msg::StObjBackupWait, &[&label(&owner)]);
                self.redraw();
                return;
            }
            // 그 서버의 탐색기 칸이 없다(접속만 있고 탐색기에 안 붙음) = 원본을 읽을 길이 없음.
            if self.objbk_failed(t(Msg::ObjBackupNoExplorer).to_string()) {
                return;
            }
        }
    }

    /// DDL 생성 결과가 백업용이면 가로챈다(`ExplorerAction::Preview` · 삭제 백업·비교와 같은 꼴) — true = 가로챘다.
    pub(crate) fn objbk_take(
        &mut self,
        spec: &nsql_catalog::GenSpec,
        r: &Result<String, String>,
    ) -> bool {
        let Some(run) = self.objbk.as_mut() else {
            return false;
        };
        let Some(cur) = run.cur.as_ref() else {
            return false;
        };
        if spec.what != nsql_catalog::GenWhat::Ddl
            || spec.owner.schema != cur.owner.schema
            || spec.owner.name != cur.owner.name
        {
            return false;
        }
        let cur = run.cur.take().expect("checked");
        let ed = self.editors.active_id();
        match r {
            Ok(ddl) => {
                let (profile, dialect) = (run.profile.clone(), run.dialect.to_string());
                match backups::write_object(&profile, &cur.owner, &dialect, cur.action.code(), ddl)
                {
                    Ok(path) => {
                        if let Some(run) = self.objbk.as_mut() {
                            run.files.push(path.clone());
                        }
                        let line = tf(
                            Msg::OutObjBackupOk,
                            &[&label(&cur.owner), &path.display().to_string()],
                        );
                        self.output_push(ed, crate::output::OutKind::Info, &line);
                        self.log_win
                            .push(nsql_log::LogEntry::new(nsql_log::LogKind::Info, line));
                        self.objbk_prune(false);
                    }
                    Err(e) => {
                        if self.objbk_failed(e) {
                            return true;
                        }
                    }
                }
            }
            Err(e) if looks_missing(e) => {
                if let Some(run) = self.objbk.as_mut() {
                    run.skipped += 1;
                }
                let line = tf(Msg::OutObjBackupSkipped, &[&label(&cur.owner)]);
                self.output_push(ed, crate::output::OutKind::Info, &line);
                self.log_win
                    .push(nsql_log::LogEntry::new(nsql_log::LogKind::Info, line));
            }
            Err(e) => {
                if self.objbk_failed(e.clone()) {
                    return true;
                }
            }
        }
        self.objbk_next();
        true
    }

    /// 실패 처리 — 운영 = 실행 막고 끝(true) · 그 밖 = 경고만 남기고 계속(false).
    fn objbk_failed(&mut self, e: String) -> bool {
        let Some(run) = self.objbk.as_mut() else {
            return true;
        };
        let ed = self.editors.active_id();
        if run.prod {
            let msg = tf(Msg::OutObjBackupBlocked, &[&e]);
            self.objbk = None;
            self.output_push(ed, crate::output::OutKind::Error, &msg);
            self.toasts
                .push(toast::ToastKind::Error, t(Msg::ObjBackupTitle), msg.clone());
            self.sess.status = msg;
            self.redraw();
            return true;
        }
        run.warned += 1;
        self.output_push(
            ed,
            crate::output::OutKind::Warn,
            &tf(Msg::OutObjBackupFailed, &[&e]),
        );
        false
    }

    /// 전부 끝 — 요약 한 줄 · 미뤄 둔 실행을 **같은 세션**에서 재진입(백업 게이트·운영 2단 확인은 한 번 건너뜀).
    fn objbk_finish(&mut self) {
        let Some(run) = self.objbk.take() else {
            return;
        };
        let ed = self.editors.active_id();
        self.output_push(
            ed,
            crate::output::OutKind::Info,
            &tf(
                Msg::OutObjBackupDone,
                &[
                    &run.files.len().to_string(),
                    &run.skipped.to_string(),
                    &run.warned.to_string(),
                ],
            ),
        );
        let (src, lb, all, whole) = (run.src, run.line_base, run.all, run.whole);
        self.with_sess(run.sid, |a| {
            a.objbk_skip_once = true;
            a.prod_confirmed_once = true;
            a.run_text_in(src, lb, all, whole);
            a.objbk_skip_once = false;
            a.prod_confirmed_once = false;
        });
        self.redraw();
    }

    /// 틱 — 응답 대기 상한 + 주기 정리(`backup.prune_interval_min` · 0 = 끔). 돌려주는 값 = 다음 깨울 시각.
    pub(crate) fn objbk_tick(&mut self, now: Instant) -> Option<Instant> {
        let mut next: Option<Instant> = None;
        if let Some(run) = self.objbk.as_ref() {
            if run.cur.is_some() {
                let due = run.started + WAIT_MAX;
                if now >= due {
                    if !self.objbk_failed(t(Msg::ObjBackupTimeout).to_string()) {
                        self.objbk_next();
                    }
                } else {
                    next = Some(due);
                }
            }
        }
        let every = self.settings.int("backup.prune_interval_min").max(0) as u64;
        if every > 0 {
            let period = Duration::from_secs(every * 60);
            let due = *self.objbk_prune_next.get_or_insert(now + period);
            if now >= due {
                self.objbk_prune(true);
                self.objbk_prune_next = Some(now + period);
                next = Some(next.map_or(now + period, |n| n.min(now + period)));
            } else {
                next = Some(next.map_or(due, |n| n.min(due)));
            }
        }
        next
    }

    /// 보관 규칙대로 정리(일수 + 객체별 최신 N) — `log`면 지운 개수를 로그에.
    pub(crate) fn objbk_prune(&mut self, log: bool) {
        let days = self.settings.int("backup.object_days").max(1) as u64;
        let keep = self.settings.int("backup.object_keep").max(0) as usize;
        let n = backups::prune_objects(days, keep);
        if log && n > 0 {
            self.log_win.push(nsql_log::LogEntry::new(
                nsql_log::LogKind::Info,
                tf(Msg::LogObjBackupPruned, &[&n.to_string()]),
            ));
        }
    }

    /// 설정 창 덧말 — 파일 스냅숏 보관(`project.backup_days`)의 **현재 값**을 객체 백업 보관 설정 아래에(사용자 10-15).
    pub(crate) fn objbk_prefs_note(&mut self) {
        let days = self.settings.int("project.backup_days").to_string();
        self.prefs_win.set_note(
            "backup.object_days",
            Some(tf(Msg::NoteBackupFileDays, &[&days])),
        );
    }

    /// 상태바 저장 메뉴 ▸ 객체 백업 위치 열기(없으면 만들고 · D-303).
    pub(crate) fn objbk_open_dir(&mut self) {
        let Some(dir) = backups::objects_dir() else {
            self.sess.status = t(Msg::StAutosaveNone).into();
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        self.sess.status = match crate::open_external(&dir) {
            Ok(()) => tf(Msg::StAutosaveOpened, &[&dir.display().to_string()]),
            Err(e) => e,
        };
    }

    /// 자체 시험 덤프(`objbk.dump:`) — 진행 상태 한 줄.
    pub(crate) fn objbk_dump(&self) -> String {
        match self.objbk.as_ref() {
            None => "idle".to_string(),
            Some(r) => format!(
                "waiting={}|queue={}|files={}|skipped={}|warned={}|prod={}",
                r.cur.as_ref().map(|t| label(&t.owner)).unwrap_or_default(),
                r.queue.len(),
                r.files.len(),
                r.skipped,
                r.warned,
                r.prod
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_catalog::ObjectKind as K;

    fn all() -> BkFlags {
        BkFlags {
            drop: true,
            replace: true,
            truncate: true,
        }
    }

    fn names(v: &[BkTarget]) -> Vec<(String, String, K, BkAction)> {
        v.iter()
            .map(|t| {
                (
                    t.owner.schema.clone(),
                    t.owner.name.clone(),
                    t.owner.kind,
                    t.action,
                )
            })
            .collect()
    }

    #[test]
    fn targets_cover_drop_replace_alter_rename_truncate_and_skip_plain_create() {
        let items: Vec<String> = [
            "DROP TABLE hr.emp",
            "CREATE OR REPLACE VIEW v_sales AS SELECT 1 FROM dual",
            "CREATE TABLE new_t (a int)",
            "CREATE OR ALTER PROCEDURE dbo.p_x AS SELECT 1",
            "ALTER TABLE emp ADD (c2 NUMBER)",
            "TRUNCATE TABLE \"HR\".\"ORDERS\"",
            "TRUNCATE t_plain",
            "RENAME old_t TO new_t",
            "COMMENT ON TABLE emp IS 'x'",
            "SELECT 1 FROM dual",
            "DROP TABLE HR.EMP",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let got = names(&backup_targets(&items, Dialect::Oracle, Some("hr"), all()));
        assert_eq!(
            got,
            vec![
                // Oracle = 따옴표 없는 이름은 대문자로 접힘(fold_ident) · 기본 스키마는 세션 값 그대로.
                ("HR".into(), "EMP".into(), K::Table, BkAction::Drop),
                ("hr".into(), "V_SALES".into(), K::View, BkAction::Replace),
                ("DBO".into(), "P_X".into(), K::Procedure, BkAction::Replace),
                ("HR".into(), "ORDERS".into(), K::Table, BkAction::Truncate),
                ("hr".into(), "T_PLAIN".into(), K::Table, BkAction::Truncate),
                ("hr".into(), "OLD_T".into(), K::Table, BkAction::Rename),
            ]
        );
    }

    #[test]
    fn identifiers_fold_per_dialect_unless_quoted() {
        let items: Vec<String> = [
            "create or replace procedure hr.nsqlt_x as begin null; end;",
            "ALTER TABLE \"MixedCase\" ADD (c int)",
            "truncate table lower_t",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let ora = names(&backup_targets(&items, Dialect::Oracle, Some("HR"), all()));
        assert_eq!(ora[0].0, "HR");
        assert_eq!(ora[0].1, "NSQLT_X");
        assert_eq!(ora[1].1, "MixedCase", "따옴표 식별자는 그대로");
        assert_eq!(ora[2].1, "LOWER_T");
        let pg = names(&backup_targets(
            &["ALTER TABLE Public.Big_T ADD c int".to_string()],
            Dialect::Postgres,
            None,
            all(),
        ));
        assert_eq!((pg[0].0.as_str(), pg[0].1.as_str()), ("public", "big_t"));
        let ms = names(&backup_targets(
            &["ALTER TABLE dbo.MyTab ADD c int".to_string()],
            Dialect::Mssql,
            None,
            all(),
        ));
        assert_eq!(ms[0].1, "MyTab", "대소문자 무시 카탈로그는 그대로");
    }

    #[test]
    fn flags_turn_actions_off() {
        let items: Vec<String> = [
            "DROP TABLE a",
            "ALTER TABLE b ADD (c int)",
            "TRUNCATE TABLE c",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let f = BkFlags {
            drop: false,
            replace: true,
            truncate: false,
        };
        let got = names(&backup_targets(&items, Dialect::Postgres, None, f));
        assert_eq!(
            got,
            vec![("".into(), "b".into(), K::Table, BkAction::Alter)]
        );
        assert!(backup_targets(
            &items,
            Dialect::Postgres,
            None,
            BkFlags {
                drop: false,
                replace: false,
                truncate: false
            }
        )
        .is_empty());
    }

    #[test]
    fn missing_object_errors_are_recognised() {
        assert!(looks_missing("HR.X: DDL not found (TABLE)"));
        assert!(looks_missing(
            "ORA-31603: object \"X\" of type TABLE not found in schema"
        ));
        assert!(looks_missing("Invalid object name 'dbo.x'"));
        assert!(looks_missing("main.no_such_t: DDL not available for Table"));
        assert!(!looks_missing("ORA-12170: TNS:Connect timeout occurred"));
        assert!(!looks_missing("connection reset"));
    }
}
