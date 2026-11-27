//! 테이블 구조 편집기의 호스트 쪽(docs/119 · 화면 부품 = `tbledit_view.rs` · 문장 생성 = `nsql-ddl`).
//!
//! 흐름 = 탐색기 ▸ 구조 편집…/새 테이블… → 뷰 탭(`tbl:<서버>/<스키마>.<이름>`) → 백그라운드 메타 세션이 구조를 읽어 채움 →
//! 편집(모델만) → [적용…] = **서버 재확인 + 위험 검사 질의**(같은 요청 하나) → 미리보기 창(스크립트 + 경고 머리 주석) →
//! [적용] = 그 서버의 세션으로 **문장을 하나씩**(문지기 · 운영 확인 · 객체 백업 = 보통 실행과 같은 길) · 오류가 나면 거기서
//! 멈춘다(D-321 · 자동 되돌리기 없음) → 끝나면 구조를 다시 읽는다(성공 = 편집 모델 초기화 · 실패 = 남은 편집 유지).
//! 테이블 재생성은 하지 않는다(D-318) — 방언이 못 하는 변경은 문장이 되지 않고 화면의 경고 줄로만 남는다.

use crate::tbledit_view::{Section, TableEditView, TbAction};
use crate::App;
use nexa_ctl::geom::Point;
use nexa_ctl::{InputEvent, TextBox};
use nsql_catalog::TableEditData;
use nsql_core::Dialect;
use nsql_ddl::{CheckKind, KeyKind, Reason, Risk};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;

/// 뷰 탭 열쇠 접두.
pub(crate) const KEY_PREFIX: &str = "tbl:";

/// 서버 응답을 기다리는 까닭.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TbPending {
    None,
    /// 처음 열기 · [새로 고침].
    Load,
    /// [적용…] 직전의 서버 재확인 + 위험 검사.
    PreApply,
    /// 적용 뒤 다시 읽기(`ok` = 전부 성공).
    Reload {
        ok: bool,
    },
}

/// 편집기 탭 하나의 상태.
#[derive(Debug)]
pub(crate) struct TbTab {
    pub view: TableEditView,
    pub server: Option<ConnectSpec>,
    pub pending: TbPending,
    /// 위험 검사 질의의 뜻(응답의 행 수와 같은 순서).
    checks: Vec<(CheckKind, String)>,
}

/// 적용 중인 문장 열(한 번에 하나 · 세션 하나).
#[derive(Debug)]
pub(crate) struct TbRun {
    key: String,
    sid: u64,
    stmts: Vec<String>,
    /// 다음에 보낼 문장 자리(= 지금까지 보낸 수).
    next: usize,
    /// 사용자가 미리보기에서 고친 스크립트인가.
    edited: bool,
}

/// 코어 방언 → 구조 편집기 방언(그 밖 = 읽기 전용으로도 열지 않는다).
pub(crate) fn tb_dialect(d: Dialect) -> Option<nsql_ddl::Dialect> {
    match d {
        Dialect::Oracle => Some(nsql_ddl::Dialect::Oracle),
        Dialect::Postgres => Some(nsql_ddl::Dialect::Postgres),
        Dialect::Mssql => Some(nsql_ddl::Dialect::Mssql),
        Dialect::Mysql => Some(nsql_ddl::Dialect::Mysql),
        Dialect::Sqlite => Some(nsql_ddl::Dialect::Sqlite),
        _ => None,
    }
}

/// 카탈로그가 읽은 것 → 편집 모델. 제약에 딸린 인덱스(기본 키·유니크와 같은 이름 · MySQL `PRIMARY`)는 인덱스 목록에서 뺀다
/// (제약을 지우면 함께 사라지는 것을 따로 지우게 두지 않는다). 순수.
pub(crate) fn to_model(schema: &str, table: &str, d: &TableEditData) -> nsql_ddl::Table {
    let mut cols: Vec<&nsql_catalog::ColumnInfo> = d.cols.iter().collect();
    cols.sort_by_key(|c| c.position);
    let columns = cols
        .into_iter()
        .map(|c| nsql_ddl::Column {
            name: c.name.clone(),
            data_type: c.data_type.clone(),
            nullable: c.nullable,
            default: c.default.trim().to_string(),
            comment: d
                .detail
                .col_comments
                .iter()
                .find(|(n, _)| *n == c.name)
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
            origin: None,
        })
        .collect();
    let extra = |name: &str| {
        d.key_extra
            .as_ref()
            .and_then(|x| x.iter().find(|e| e.name == name))
    };
    let mut keys: Vec<nsql_ddl::Key> = d
        .detail
        .keys
        .iter()
        .map(|k| nsql_ddl::Key {
            name: k.name.clone(),
            kind: match k.kind {
                'P' => KeyKind::Primary,
                'U' => KeyKind::Unique,
                'R' => KeyKind::Foreign,
                _ => KeyKind::Check,
            },
            cols: k.cols.clone(),
            // 다른 스키마의 대상이면 덧붙은 속성이 준 `스키마.테이블`을 쓴다(제약 목록 질의는 이름만 준다 — 그대로 다시 만들면 엉뚱한
            // 스키마를 가리키거나 실패한다).
            ref_table: extra(&k.name)
                .map(|e| e.ref_table.clone())
                .filter(|r| !r.is_empty())
                .or_else(|| k.ref_table.clone())
                .unwrap_or_default(),
            ref_cols: k.ref_cols.clone(),
            on_delete: extra(&k.name)
                .map(|e| e.on_delete.clone())
                .unwrap_or_default(),
            on_update: extra(&k.name)
                .map(|e| e.on_update.clone())
                .unwrap_or_default(),
            check: extra(&k.name).map(|e| e.check.clone()).unwrap_or_default(),
            ..nsql_ddl::Key::default()
        })
        .collect();
    // 제약 목록 질의에 안 나온 체크 제약(방언에 따라 키·외래 키만 준다)도 덧붙은 속성에 있으면 보인다 — 지우거나 고칠 수 있게.
    if let Some(x) = &d.key_extra {
        for e in x.iter().filter(|e| e.kind == 'C') {
            if !keys.iter().any(|k| k.name == e.name) {
                keys.push(nsql_ddl::Key {
                    name: e.name.clone(),
                    kind: KeyKind::Check,
                    check: e.check.clone(),
                    ..nsql_ddl::Key::default()
                });
            }
        }
    }
    let indexes = d
        .detail
        .indexes
        .iter()
        .filter(|i| {
            i.name != "PRIMARY"
                && !keys.iter().any(|k| {
                    matches!(k.kind, KeyKind::Primary | KeyKind::Unique) && k.name == i.name
                })
        })
        .map(|i| nsql_ddl::Index {
            name: i.name.clone(),
            unique: i.unique,
            cols: i.cols.clone(),
            origin: None,
        })
        .collect();
    nsql_ddl::Table {
        schema: schema.to_string(),
        name: table.to_string(),
        comment: d.detail.comment.clone().unwrap_or_default(),
        columns,
        keys,
        indexes,
    }
}

/// 고치면 안 되는 제약들(표현 못 하는 속성이 있는 것)의 이름. 순수.
pub(crate) fn locked_keys(d: &TableEditData) -> Vec<String> {
    let Some(x) = d.key_extra.as_ref() else {
        return Vec::new();
    };
    let mut out: Vec<String> = x
        .iter()
        .filter(|e| e.locked)
        .map(|e| e.name.clone())
        .collect();
    // 제약 목록에는 있는데 덧붙은 속성이 없는 외래 키·체크(질의가 못 본 것 — 예: 이름이 자동으로 붙은 제약)도 잠근다:
    // 식·규칙을 모르는 채로 다시 만들게 두지 않는다.
    for k in d.detail.keys.iter().filter(|k| matches!(k.kind, 'R' | 'C')) {
        if !x.iter().any(|e| e.name == k.name) && !out.contains(&k.name) {
            out.push(k.name.clone());
        }
    }
    out
}

/// 미리보기 머리 주석(영어 고정 = 스크립트의 일부 · 파일로 저장·보관된다): 위험 등급 · 0이 아닌 검사 결과 · 막힌 변경. 순수.
pub(crate) fn preview_header(
    table: &str,
    plan: &nsql_ddl::Plan,
    checks: &[(CheckKind, String)],
    counts: &[Option<u64>],
    prod: bool,
) -> String {
    let mut s = format!(
        "-- Nexa SQL table editor: {table} - {} statement(s)\n",
        plan.stmts.len()
    );
    if prod {
        s.push_str("-- PRODUCTION connection.\n");
    }
    match plan.risk() {
        Risk::Danger => {
            s.push_str("-- DANGER: this script removes data (dropped column or shortened type).\n")
        }
        Risk::Warn => {
            s.push_str("-- WARNING: some statements can fail or take long on existing data.\n")
        }
        Risk::Safe => {}
    }
    for ((kind, target), n) in checks.iter().zip(counts) {
        match (kind, n) {
            (_, None) => s.push_str(&format!(
                "-- check failed for {target} (could not count rows)\n"
            )),
            (_, Some(0)) => {}
            (CheckKind::NullRows, Some(n)) => {
                s.push_str(&format!(
                    "-- {target}: {n} row(s) are NULL - NOT NULL will fail\n"
                ));
            }
            (CheckKind::ValueRows, Some(n)) => {
                s.push_str(&format!(
                    "-- {target}: {n} row(s) have a value - DROP COLUMN deletes them\n"
                ));
            }
            (CheckKind::TableRows, Some(n)) => s.push_str(&format!(
                "-- {target}: table has {n} row(s) - adding NOT NULL without a default will fail\n"
            )),
            (CheckKind::DuplicateKeys, Some(n)) => {
                s.push_str(&format!(
                    "-- {target}: {n} duplicate key group(s) - the constraint will fail\n"
                ));
            }
        }
    }
    // 제약을 고치면 DROP 뒤 ADD로 나간다 — ADD가 기존 데이터 때문에 실패하면 제약이 **없는 채로** 멈춘다(11-28 실서버 3방언).
    if plan.stmts.iter().any(|st| {
        let u = st.sql.to_ascii_uppercase();
        u.contains(" DROP CONSTRAINT ")
            || u.contains(" DROP FOREIGN KEY ")
            || u.contains(" DROP CHECK ")
            || u.contains(" DROP PRIMARY KEY")
    }) {
        s.push_str("-- NOTE: a changed constraint is dropped and then added again. If the ADD fails on existing data, the script stops with the constraint dropped - fix the data or the definition and apply again (the object backup has the original).\n");
    }
    for b in &plan.blocked {
        s.push_str(&match b.reason {
            Reason::NeedsRebuild => format!("-- NOT APPLIED: {} needs table re-creation\n", b.what),
            Reason::NotSupported => format!(
                "-- NOT APPLIED: {} is not supported by this database\n",
                b.what
            ),
        });
    }
    s.push('\n');
    s
}

/// 보관할 "실행한 스크립트" 글(영어 고정 머리 주석 + 보낸 문장들). 순수.
pub(crate) fn applied_script(
    schema: &str,
    table: &str,
    when: &str,
    applied: usize,
    total: usize,
    ok: bool,
    script: &str,
) -> String {
    let mut s = format!(
        "-- Nexa SQL table editor: {schema}.{table} - applied {applied} of {total} statement(s) at {when}\n"
    );
    if !ok {
        s.push_str(
            "-- STOPPED: the last statement below failed; statements after it were not sent.\n",
        );
    }
    s.push('\n');
    s.push_str(script);
    s
}

/// 미리보기에서 고친 스크립트 → 실행할 문장들. SQL Server = `GO` 줄로 나눈 배치(배치 안의 `;`·변수 선언을 지킨다) · 그 밖 =
/// 스크립트 엔진의 문장 분리. 주석·빈 줄뿐인 조각은 버린다.
pub(crate) fn edited_statements(d: nsql_ddl::Dialect, text: &str) -> Vec<String> {
    let has_sql = |s: &str| {
        s.lines()
            .map(str::trim)
            .any(|l| !l.is_empty() && !l.starts_with("--"))
    };
    let parts: Vec<String> = if d == nsql_ddl::Dialect::Mssql {
        let mut out = vec![String::new()];
        for line in text.lines() {
            if line.trim().eq_ignore_ascii_case("GO") {
                out.push(String::new());
            } else if let Some(cur) = out.last_mut() {
                cur.push_str(line);
                cur.push('\n');
            }
        }
        out
    } else {
        let core = match d {
            nsql_ddl::Dialect::Oracle => Dialect::Oracle,
            nsql_ddl::Dialect::Postgres => Dialect::Postgres,
            nsql_ddl::Dialect::Mysql => Dialect::Mysql,
            nsql_ddl::Dialect::Sqlite | nsql_ddl::Dialect::Mssql => Dialect::Sqlite,
        };
        crate::split_items(text, core)
    };
    parts
        .into_iter()
        .map(|p| p.trim().trim_end_matches(';').trim().to_string())
        .filter(|p| has_sql(p))
        .collect()
}

impl App {
    /// 활성 탭이 구조 편집기면 그 열쇠.
    pub(crate) fn tbledit_key(&self) -> Option<String> {
        self.editors
            .active_view()
            .filter(|k| k.starts_with(KEY_PREFIX) && self.tbledits.contains_key(*k))
            .map(str::to_string)
    }

    pub(crate) fn tbledit_active(&self) -> bool {
        self.tbledit_key().is_some()
    }

    /// 그 서버의 방언(접속된 세션에서 · 없으면 접속 명세 · 그것도 없으면 활성 세션).
    fn tbledit_dialect(&self, server: Option<&ConnectSpec>) -> Option<Dialect> {
        match server {
            Some(spec) => self
                .all_sess()
                .filter(|s| !s.closing && s.connected)
                .find(|s| {
                    s.spec
                        .as_ref()
                        .is_some_and(|have| crate::worker::same_server(have, spec))
                })
                .map(|s| s.dialect)
                .or(spec.dialect),
            None => Some(self.sess.dialect),
        }
    }

    fn tbledit_prod(server: Option<&ConnectSpec>, sess_prod: bool) -> bool {
        match server {
            Some(s) => s.env == Some(nsql_script::ConnEnv::Prod),
            None => sess_prod,
        }
    }

    /// 탭을 열고(있으면 그 탭으로) 상태를 만든다. 지원하지 않는 방언 = 상태줄 안내만.
    fn tbledit_open_tab(
        &mut self,
        schema: &str,
        name: &str,
        server: Option<ConnectSpec>,
        is_new: bool,
    ) -> Option<String> {
        let Some(d) = self.tbledit_dialect(server.as_ref()).and_then(tb_dialect) else {
            self.sess.status = t(Msg::StTbeUnsupported).into();
            self.redraw();
            return None;
        };
        let host = server
            .as_ref()
            .and_then(|s| s.host.clone())
            .unwrap_or_default();
        let key = if is_new {
            self.tbledit_seq += 1;
            format!("{KEY_PREFIX}new:{}", self.tbledit_seq)
        } else {
            format!("{KEY_PREFIX}{host}/{schema}.{name}")
        };
        let title = if is_new {
            t(Msg::TbeTitleNew).to_string()
        } else {
            tf(Msg::TbeTitle, &[name])
        };
        if !self.editors.open_view_tab(&key, &title) && !self.tab_room() {
            return None;
        }
        if !self.tbledits.contains_key(&key) {
            let mut view = TableEditView::new(d, schema, name, is_new);
            let sess_prod = self
                .sess
                .spec
                .as_ref()
                .is_some_and(|s| s.env == Some(nsql_script::ConnEnv::Prod));
            let prod = Self::tbledit_prod(server.as_ref(), sess_prod);
            if prod && self.settings.get("tbledit.prod") == Some("deny") {
                view.set_read_only(Some(t(Msg::TbeProdDenied).to_string()));
            }
            // ★ 라이선스 안내 게이트(D-328): 라이선스가 없으면 **앱 실행마다 첫 열기 한 번** 안내를 보인다(막지 않는다) ·
            //   `license.gates` 설정과 무관(게이트를 꺼도 안내는 뜬다) → `entitled`가 아니라 검사 결과를 직접 본다.
            if !self.tbledit_notice_done
                && !self
                    .licensing
                    .check(nsql_license::Feature::ObjectEditor)
                    .is_allowed()
            {
                view.set_notice(true);
            }
            self.tbledits.insert(
                key.clone(),
                TbTab {
                    view,
                    server,
                    pending: TbPending::None,
                    checks: Vec::new(),
                },
            );
        }
        self.set_focus(crate::Focus::Editor);
        self.layout();
        self.redraw();
        Some(key)
    }

    /// 탐색기 ▸ 구조 편집…(팔레트 `obj.edit_structure`도 여기로).
    pub(crate) fn tbledit_open(&mut self, schema: &str, name: &str, server: Option<ConnectSpec>) {
        let Some(key) = self.tbledit_open_tab(schema, name, server, false) else {
            return;
        };
        let fresh = self
            .tbledits
            .get(&key)
            .is_some_and(|tab| !tab.view.loaded());
        if fresh {
            self.tbledit_request(&key, TbPending::Load, Vec::new());
        }
    }

    /// 탐색기 Tables 폴더 ▸ 새 테이블…
    pub(crate) fn tbledit_new(&mut self, schema: &str, server: Option<ConnectSpec>) {
        self.tbledit_open_tab(schema, "", server, true);
    }

    /// 서버에 구조(+ 검사 질의)를 청한다 — 답은 [`Self::tbledit_data`]로 온다.
    fn tbledit_request(&mut self, key: &str, why: TbPending, checks: Vec<nsql_ddl::RiskCheck>) {
        let Some(tab) = self.tbledits.get_mut(key) else {
            return;
        };
        tab.pending = why;
        tab.checks = checks.iter().map(|c| (c.kind, c.target.clone())).collect();
        let (schema, name) = (tab.view.orig().schema.clone(), tab.view.orig().name.clone());
        let server = tab.server.clone();
        self.explorer.request_table_edit(
            server.as_ref(),
            &schema,
            &name,
            checks.into_iter().map(|c| c.sql).collect(),
        );
    }

    /// 메타 세션의 답(탐색기 동작 `TableEditData`).
    pub(crate) fn tbledit_data(
        &mut self,
        schema: &str,
        table: &str,
        r: Result<TableEditData, String>,
        server: Option<&ConnectSpec>,
    ) {
        // 같은 테이블을 기다리는 탭(서버가 같거나 · 서버를 모르는 탭).
        let key = self.tbledits.iter().find_map(|(k, tab)| {
            let o = tab.view.orig();
            let same_server = match (&tab.server, server) {
                (Some(a), Some(b)) => crate::worker::same_server(a, b),
                _ => true,
            };
            (tab.pending != TbPending::None && o.schema == schema && o.name == table && same_server)
                .then(|| k.clone())
        });
        let Some(key) = key else {
            return;
        };
        let Some(tab) = self.tbledits.get_mut(&key) else {
            return;
        };
        let why = std::mem::replace(&mut tab.pending, TbPending::None);
        let data = match r {
            Ok(d) => d,
            Err(e) => {
                self.sess.status = tf(Msg::StTbeReadFailed, &[table, &e]);
                self.redraw();
                return;
            }
        };
        let model = to_model(schema, table, &data);
        match why {
            TbPending::None => {}
            TbPending::Load | TbPending::Reload { ok: true } => {
                tab.view
                    .set_key_lock(data.key_extra.is_some(), locked_keys(&data));
                tab.view.load(model);
            }
            TbPending::Reload { ok: false } => {
                tab.view
                    .set_key_lock(data.key_extra.is_some(), locked_keys(&data));
                tab.view.rebase(model);
            }
            TbPending::PreApply => {
                // F-13: 열어 본 뒤 다른 세션이 구조를 바꿨으면 적용하지 않는다(사용자가 본 차이가 더는 맞지 않는다).
                if model != *tab.view.orig() {
                    self.sess.status = tf(Msg::StTbeChangedOnServer, &[table]);
                    self.toasts.push(
                        crate::toast::ToastKind::Error,
                        t(Msg::TbeApplyTitle).to_string(),
                        tf(Msg::StTbeChangedOnServer, &[table]),
                    );
                } else {
                    let prod = Self::tbledit_prod(tab.server.as_ref(), false);
                    let text = format!(
                        "{}{}",
                        preview_header(table, tab.view.plan(), &tab.checks, &data.counts, prod),
                        tab.view.script()
                    );
                    self.tbledit_apply_key = Some(key.clone());
                    self.tbledit_apply_shown = Some(text.clone());
                    self.sess.status = tf(Msg::TbeApplyTitleFor, &[table]);
                    self.sqlprev_apply = Some((tf(Msg::TbeApplyTitleFor, &[table]), text));
                }
            }
        }
        self.tbledit_title_sync(&key);
        self.redraw();
    }

    /// 탭 제목·미저장 표식을 편집 상태에 맞춘다.
    fn tbledit_title_sync(&mut self, key: &str) {
        let Some(tab) = self.tbledits.get(key) else {
            return;
        };
        let name = &tab.view.edited().name;
        let title = if tab.view.is_new() && name.is_empty() {
            t(Msg::TbeTitleNew).to_string()
        } else {
            tf(Msg::TbeTitle, &[name])
        };
        self.editors.set_view_title(key, &title);
        self.editors.set_view_dirty(key, tab.view.is_dirty());
    }

    /// 화면 부품이 낸 동작을 거둔다(입력 처리 뒤).
    pub(crate) fn tbledit_actions(&mut self) {
        let Some(key) = self.tbledit_key() else {
            return;
        };
        let acts = match self.tbledits.get_mut(&key) {
            Some(tab) => tab.view.take_actions(),
            None => return,
        };
        for a in acts {
            match a {
                TbAction::Changed => self.tbledit_title_sync(&key),
                TbAction::OpenHistory(i) => {
                    let file = self
                        .tbledits
                        .get(&key)
                        .and_then(|tab| tab.view.history_file(i))
                        .map(std::path::PathBuf::from);
                    if let Some(f) = file {
                        self.open_file(&f);
                    }
                }
                TbAction::NoticeAck => {
                    self.tbledit_notice_done = true;
                    for tab in self.tbledits.values_mut() {
                        tab.view.set_notice(false);
                    }
                }
                TbAction::OpenLicense => self.open_license = true,
                TbAction::Refresh => self.tbledit_request(&key, TbPending::Load, Vec::new()),
                TbAction::Apply => self.tbledit_apply_request(&key),
            }
        }
    }

    /// [적용…] — 새 테이블은 바로 미리보기 · 기존 테이블은 서버 재확인 + 위험 검사 뒤.
    fn tbledit_apply_request(&mut self, key: &str) {
        // 앞선 적용이 아직 도는가: 그 세션이 실행 중이거나 객체 백업이 진행 중일 때만 "적용 중"이다 — 백업 취소·문지기 거절로
        // 문장이 끝내 나가지 않은 낡은 상태는 여기서 걷는다(다음 적용을 영영 막지 않게).
        if let Some(run) = &self.tbledit_run {
            let sid = run.sid;
            if self.all_sess().any(|s| s.id == sid && s.busy) || self.objbk.is_some() {
                self.sess.status = t(Msg::StTbeBusy).into();
                return;
            }
            self.tbledit_run = None;
        }
        let Some(tab) = self.tbledits.get(key) else {
            return;
        };
        if !tab.view.can_apply() {
            return;
        }
        if tab.view.is_new() {
            let name = tab.view.edited().name.clone();
            let prod = Self::tbledit_prod(tab.server.as_ref(), false);
            let text = format!(
                "{}{}",
                preview_header(&name, tab.view.plan(), &[], &[], prod),
                tab.view.script()
            );
            self.tbledit_apply_key = Some(key.to_string());
            self.tbledit_apply_shown = Some(text.clone());
            self.sqlprev_apply = Some((tf(Msg::TbeApplyTitleFor, &[&name]), text));
            return;
        }
        let d = tab.view.dialect();
        let orig = tab.view.orig().clone();
        let checks = nsql_ddl::risk_checks(d, &orig, &nsql_ddl::diff(&orig, tab.view.edited()));
        self.sess.status = t(Msg::StTbeChecking).into();
        self.tbledit_request(key, TbPending::PreApply, checks);
        self.redraw();
    }

    /// 미리보기 창의 [적용](또는 시험 명령) — 그 서버의 세션으로 첫 문장을 보낸다.
    pub(crate) fn tbledit_apply_go(&mut self) {
        self.tbledit_apply_text(None);
    }

    /// 미리보기 창의 [적용] — `shown` = 창에 남아 있던 글. 사용자가 미리보기를 고쳤으면(생성한 글과 다르면) **고친 글 그대로**
    /// 문장으로 나눠 실행한다(D-317 "본 스크립트가 곧 실행되는 스크립트") · 그대로면 생성한 문장 목록.
    pub(crate) fn tbledit_apply_text(&mut self, shown: Option<String>) {
        let Some(key) = self.tbledit_apply_key.take() else {
            return;
        };
        let Some(tab) = self.tbledits.get(&key) else {
            return;
        };
        if !tab.view.can_apply() || self.tbledit_run.is_some() {
            return;
        }
        let generated: Vec<String> = tab
            .view
            .plan()
            .stmts
            .iter()
            .map(|s| s.sql.clone())
            .collect();
        let d = tab.view.dialect();
        let offered = self.tbledit_apply_shown.take();
        let (stmts, edited) = match (shown, offered) {
            (Some(now), Some(was)) if now.trim() != was.trim() => {
                (edited_statements(d, &now), true)
            }
            _ => (generated, false),
        };
        if stmts.is_empty() {
            self.sess.status = t(Msg::TbeNoChanges).into();
            self.redraw();
            return;
        }
        let sid = match tab.server.as_ref() {
            Some(spec) => self
                .all_sess()
                .filter(|s| !s.closing && s.connected)
                .find(|s| {
                    s.spec
                        .as_ref()
                        .is_some_and(|have| crate::worker::same_server(have, spec))
                })
                .map(|s| s.id),
            None => Some(self.sess.id),
        };
        let Some(sid) = sid else {
            self.sess.status = t(Msg::DropNoSession).into();
            self.redraw();
            return;
        };
        self.tbledit_run = Some(TbRun {
            key,
            sid,
            stmts,
            next: 0,
            edited,
        });
        self.tbledit_send_next(true);
    }

    /// 다음 문장을 보낸다. `first` = 미리보기 창의 확인이 곧 운영 2단 확인이다(그 뒤 문장은 같은 확인 아래) · 객체 백업은 첫
    /// 문장에서만(같은 테이블을 문장마다 다시 백업하지 않는다).
    fn tbledit_send_next(&mut self, first: bool) {
        let Some(run) = self.tbledit_run.as_mut() else {
            return;
        };
        let Some(sql) = run.stmts.get(run.next).cloned() else {
            return;
        };
        run.next += 1;
        let sid = run.sid;
        let sent = self
            .with_sess(sid, |a| {
                if !a.gate_open() {
                    return false;
                }
                a.prod_confirmed_once = true;
                a.hold_skip_once = true;
                if !first {
                    a.objbk_skip_once = true;
                }
                a.run_text_whole(sql);
                a.prod_confirmed_once = false;
                a.hold_skip_once = false;
                a.objbk_skip_once = false;
                true
            })
            .unwrap_or(false);
        if !sent {
            // 문지기가 막았다(그 세션이 다른 일을 하는 중) — 아무것도 보내지 않았다.
            self.tbledit_run = None;
        }
        self.redraw();
    }

    /// 그 세션의 실행 하나가 끝났다(`ok` = 오류 없음) — 다음 문장 또는 마무리.
    pub(crate) fn tbledit_run_done(&mut self, sid: u64, ok: bool) {
        let Some(run) = self.tbledit_run.as_ref() else {
            return;
        };
        if run.sid != sid {
            return;
        }
        let (total, sent) = (run.stmts.len(), run.next);
        if ok && sent < total {
            self.tbledit_send_next(false);
            return;
        }
        let Some(run) = self.tbledit_run.take() else {
            return;
        };
        let applied = if ok { sent } else { sent.saturating_sub(1) };
        let (n, m) = (applied.to_string(), total.to_string());
        if ok {
            self.tbledit_note = Some(tf(Msg::StTbeApplied, &[&n, &m]));
        } else {
            let msg = tf(Msg::StTbeApplyStopped, &[&n, &m]);
            self.tbledit_note = Some(msg.clone());
            self.toasts.push(
                crate::toast::ToastKind::Error,
                t(Msg::TbeApplyTitle).to_string(),
                msg,
            );
        }
        // ★ 실행한 스크립트 보관(119 F-9 ④ · F-15): 객체 백업 폴더에 `…_applied.sql` + 탭의 기록 섹션 한 줄(보낸 문장 전부 ·
        //   머리 주석에 적용 수 — 멈춘 실행이면 마지막 문장이 실패한 것이다).
        let now = nsql_log::now_local();
        let profile = self
            .all_sess()
            .find(|s| s.id == run.sid)
            .map(|s| s.profile.clone())
            .unwrap_or_default();
        if let Some(tab) = self.tbledits.get_mut(&run.key) {
            let d = tab.view.dialect();
            let (schema, table) = (
                tab.view.orig().schema.clone(),
                tab.view.edited().name.clone(),
            );
            let sent_stmts: Vec<nsql_ddl::Stmt> = run
                .stmts
                .iter()
                .take(sent)
                .map(|sql| nsql_ddl::Stmt {
                    sql: sql.clone(),
                    risk: Risk::Safe,
                })
                .collect();
            let body = applied_script(
                &schema,
                &table,
                &now.stamp(),
                applied,
                total,
                ok,
                &nsql_ddl::script(d, &sent_stmts),
            );
            let file = crate::backups::objects_dir()
                .ok_or_else(|| "no config dir".to_string())
                .and_then(|dir| {
                    crate::backups::write_applied_in(
                        &dir,
                        &profile,
                        &schema,
                        &table,
                        &crate::backups::stamp17(&now),
                        &body,
                    )
                });
            let file = match file {
                Ok(f) => f.display().to_string(),
                Err(e) => {
                    eprintln!("[tbledit] applied script not saved: {e}");
                    String::new()
                }
            };
            tab.view.push_history(
                now.stamp(),
                if ok {
                    tf(Msg::StTbeApplied, &[&n, &m])
                } else {
                    tf(Msg::TbeHistStopped, &[&n, &m])
                },
                file,
            );
        }
        // 새 테이블이 만들어졌으면 이제부터 그 이름의 기존 테이블이다.
        if let Some(tab) = self.tbledits.get_mut(&run.key) {
            if tab.view.is_new() && applied > 0 {
                tab.view.created();
            }
            if !tab.view.is_new() {
                // 고친 스크립트를 실행했으면 화면의 편집 가운데 일부만 적용됐을 수 있다 → 성공이어도 남은 편집을 지키는 쪽으로.
                let ok = ok && !run.edited;
                self.tbledit_request(&run.key, TbPending::Reload { ok }, Vec::new());
            }
        }
        self.redraw();
    }

    /// 활성 편집기 탭에 사건을 넘기고 동작을 거둔다.
    pub(crate) fn tbledit_event(&mut self, ev: &InputEvent) {
        let Some(key) = self.tbledit_key() else {
            return;
        };
        // ★ 오류 경계(D-329): 편집기 안의 패닉이 메인 편집기까지 끌고 가지 않게 — 잡히면 그 탭의 상태를 통째로 버린다.
        //   (Release가 `panic = "abort"`인 동안은 Debug에서만 효과가 있다 · 전역 전환은 크기 실측 뒤 결정.)
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.tbledits
                .get_mut(&key)
                .is_some_and(|tab| tab.view.on_event(ev))
        }));
        let Ok(redraw) = r else {
            self.tbledit_crashed(&key);
            return;
        };
        self.tbledit_actions();
        if redraw {
            self.redraw();
        }
    }

    /// 편집기 탭 하나가 패닉으로 죽었다 — 상태 폐기 · 탭 닫기 · 안내(다른 탭·세션·결과는 그대로).
    pub(crate) fn tbledit_crashed(&mut self, key: &str) {
        eprintln!("[tbledit] panic in table editor tab {key} - tab state dropped");
        self.tbledits.remove(key);
        if self.tbledit_run.as_ref().is_some_and(|r| r.key == key) {
            self.tbledit_run = None;
        }
        self.editors.close_view_tabs(key);
        self.toasts.push(
            crate::toast::ToastKind::Error,
            t(Msg::TbeApplyTitle).to_string(),
            t(Msg::TbeCrashed).to_string(),
        );
        self.sync_grid_tab();
        self.redraw();
    }

    /// 입력 책임 연쇄의 한 고리(비교 탭과 같은 규칙): 포인터 = 뷰 영역 안이거나 팝업이 열려 있을 때 · 키 = 편집기 포커스일 때.
    pub(crate) fn route_tbledit(&mut self, ev: InputEvent, is_mouse: bool) -> bool {
        let Some(key) = self.tbledit_key() else {
            return false;
        };
        let cur = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        let in_view = self.editors.editor_bounds().contains(cur);
        let pointer =
            is_mouse || matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
        let popup = self
            .tbledits
            .get(&key)
            .is_some_and(|tab| tab.view.popup_open());
        if pointer && (in_view || popup) {
            if matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) {
                self.set_focus(crate::Focus::Editor);
            }
            self.tbledit_event(&ev);
            return true;
        }
        if !pointer && self.focus == crate::Focus::Editor {
            self.tbledit_event(&ev);
            return true;
        }
        false
    }

    /// 클립보드·IME 대상 상자(셀 편집 상자 또는 DDL 미리보기).
    pub(crate) fn tbledit_textbox(&mut self) -> Option<&mut TextBox> {
        let key = self.tbledit_key()?;
        self.tbledits
            .get_mut(&key)
            .map(|tab| tab.view.textbox_mut())
    }

    /// 라이선스 안내의 남은 초 표시를 움직인다(안내가 떠 있는 동안만 0.25초마다 다시 그린다 · 그 밖에는 깨우지 않는다).
    pub(crate) fn tbledit_tick(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        let key = self.tbledit_key()?;
        let left = self.tbledits.get(&key)?.view.notice_left()?;
        self.redraw();
        (left > 0).then(|| now + std::time::Duration::from_millis(250))
    }

    /// 닫힌 탭의 상태를 버린다(탭 전환·닫기 뒤 · 닫기 = 즉시 전부 해제 D-323).
    pub(crate) fn tbledit_sync(&mut self) {
        // 그리기 중 패닉(경계가 잡아 표식만 남겼다) → 여기서 탭을 걷는다.
        if let Some(key) = self.tbledit_panicked.take() {
            self.tbledit_crashed(&key);
        }
        if self.tbledits.is_empty() {
            return;
        }
        let dead: Vec<String> = self
            .tbledits
            .keys()
            .filter(|k| !self.editors.has_view_key(k))
            .cloned()
            .collect();
        for k in dead {
            self.tbledits.remove(&k);
        }
        let focused = self.focus == crate::Focus::Editor;
        if let Some(key) = self.tbledit_key() {
            if let Some(tab) = self.tbledits.get_mut(&key) {
                tab.view.set_focused(focused);
            }
        }
    }

    /// 기동 명령(헤드리스 판정 · 자체 시험) — 맞는 명령이면 true.
    ///
    /// `tbledit.open:<스키마.테이블>` · `tbledit.new:<스키마>` · `tbledit.section:<columns|keys|indexes>` · `tbledit.add` ·
    /// `tbledit.set:<행>:<열>=<글>` · `tbledit.toggle:<행>:<열>` · `tbledit.edit:<행>:<열>`(편집 열기) · `tbledit.remove:<행>` · `tbledit.move:<행>:<±1>` ·
    /// `tbledit.name:<글>` · `tbledit.comment:<글>` · `tbledit.revert` · `tbledit.undo` · `tbledit.redo` · `tbledit.refresh` · `tbledit.apply`(미리보기까지) ·
    /// `tbledit.apply_now`(미리보기 없이 실행 = 시험 전용 길) · `tbledit.apply_script:<파일>`(고친 미리보기 글로 적용) · `tbledit.notice_ok`(라이선스 안내 확인 · 3초 뒤에만) · `tbledit.dump:<파일>`.
    pub(crate) fn tbledit_cmd(&mut self, id: &str) -> bool {
        let Some(rest) = id.strip_prefix("tbledit.") else {
            return false;
        };
        if let Some(target) = rest.strip_prefix("open:") {
            let (schema, name) = target.split_once('.').unwrap_or(("", target));
            self.tbledit_open(schema, name, None);
            return true;
        }
        if let Some(schema) = rest.strip_prefix("new:") {
            self.tbledit_new(schema, None);
            return true;
        }
        let Some(key) = self.tbledit_key() else {
            return true;
        };
        if let Some(path) = rest.strip_prefix("dump:") {
            let text = self
                .tbledits
                .get(&key)
                .map(|tab| tab.view.dump())
                .unwrap_or_default();
            let _ = std::fs::write(path, text);
            return true;
        }
        match rest {
            // 판정용(Debug 빌드만): 다음 그리기에서 편집기가 패닉 → 오류 경계가 그 탭만 걷는지.
            #[cfg(debug_assertions)]
            "test_panic" => {
                if let Some(tab) = self.tbledits.get_mut(&key) {
                    tab.view.arm_test_panic();
                }
                self.redraw();
                return true;
            }
            // 라이선스 안내의 [확인] — 화면 버튼과 같은 규칙(3초가 지나야 받아들인다).
            "notice_ok" => {
                if let Some(tab) = self.tbledits.get_mut(&key) {
                    tab.view.notice_ok();
                }
                self.tbledit_actions();
                self.redraw();
                return true;
            }
            "apply" => {
                self.tbledit_apply_request(&key);
                return true;
            }
            "apply_now" => {
                self.tbledit_apply_key = Some(key);
                self.tbledit_apply_go();
                return true;
            }
            _ if rest.starts_with("apply_script:") => {
                // 판정용: 미리보기 창에서 스크립트를 고친 뒤 [적용]을 누른 것과 같은 길(파일의 글 = 고친 글).
                let edited =
                    std::fs::read_to_string(&rest["apply_script:".len()..]).unwrap_or_default();
                let shown = self.tbledits.get(&key).map(|tab| tab.view.script());
                self.tbledit_apply_key = Some(key);
                self.tbledit_apply_shown = shown;
                self.tbledit_apply_text(Some(edited));
                return true;
            }
            "refresh" => {
                self.tbledit_request(&key, TbPending::Load, Vec::new());
                return true;
            }
            _ => {}
        }
        let nums = |s: &str| -> Option<(usize, usize)> {
            let (a, b) = s.split_once(':')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        };
        if let Some(tab) = self.tbledits.get_mut(&key) {
            let v = &mut tab.view;
            if let Some(s) = rest.strip_prefix("section:") {
                if let Some(sec) = Section::from_id(s) {
                    v.set_section(sec);
                }
            } else if rest == "add" {
                v.add_row();
            } else if rest == "revert" {
                v.revert();
            } else if rest == "undo" {
                v.undo();
            } else if rest == "redo" {
                v.redo();
            } else if let Some(s) = rest.strip_prefix("set:") {
                if let Some((pos, text)) = s.split_once('=') {
                    if let Some((r, c)) = nums(pos) {
                        v.set_cell(r, c, text);
                    }
                }
            } else if let Some(s) = rest.strip_prefix("edit:") {
                if let Some((r, c)) = nums(s) {
                    v.edit_cell(r, c);
                }
            } else if let Some(s) = rest.strip_prefix("toggle:") {
                if let Some((r, c)) = nums(s) {
                    v.toggle(r, c);
                }
            } else if let Some(s) = rest.strip_prefix("remove:") {
                if let Ok(r) = s.parse() {
                    v.remove_row(r);
                }
            } else if let Some(s) = rest.strip_prefix("move:") {
                if let Some((r, d)) = s.split_once(':') {
                    if let (Ok(r), Ok(d)) = (r.parse(), d.parse()) {
                        v.move_row(r, d);
                    }
                }
            } else if let Some(s) = rest.strip_prefix("name:") {
                v.set_table_name(s);
            } else if let Some(s) = rest.strip_prefix("comment:") {
                v.set_table_comment(s);
            }
        }
        self.tbledit_actions();
        self.redraw();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_catalog::{ColumnInfo, IndexDef, KeyDef, TableDetail};

    fn data() -> TableEditData {
        TableEditData {
            cols: vec![
                ColumnInfo {
                    name: "NAME".into(),
                    data_type: "VARCHAR2(50)".into(),
                    nullable: true,
                    position: 2,
                    default: " 'x' ".into(),
                },
                ColumnInfo {
                    name: "ID".into(),
                    data_type: "NUMBER".into(),
                    nullable: false,
                    position: 1,
                    default: String::new(),
                },
            ],
            detail: TableDetail {
                keys: vec![KeyDef {
                    name: "T_PK".into(),
                    kind: 'P',
                    cols: vec!["ID".into()],
                    ref_table: None,
                    ref_cols: vec![],
                }],
                indexes: vec![
                    IndexDef {
                        name: "T_PK".into(),
                        unique: true,
                        cols: vec!["ID".into()],
                    },
                    IndexDef {
                        name: "T_IX1".into(),
                        unique: false,
                        cols: vec!["NAME".into()],
                    },
                ],
                comment: Some("tbl".into()),
                col_comments: vec![("NAME".into(), "nm".into())],
            },
            counts: vec![],
            key_extra: None,
        }
    }

    #[test]
    fn model_orders_columns_and_hides_key_indexes() {
        let m = to_model("APP", "T", &data());
        assert_eq!(
            m.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["ID", "NAME"]
        );
        assert_eq!(m.columns[1].default, "'x'");
        assert_eq!(m.columns[1].comment, "nm");
        assert_eq!(m.comment, "tbl");
        assert_eq!(m.indexes.len(), 1);
        assert_eq!(m.indexes[0].name, "T_IX1");
        // 같은 것을 두 번 읽으면 같다(적용 직전 재확인의 전제).
        assert_eq!(m, to_model("APP", "T", &data()));
    }

    #[test]
    fn header_reports_only_nonzero_checks() {
        let plan = nsql_ddl::Plan {
            stmts: vec![nsql_ddl::Stmt {
                sql: "ALTER TABLE T DROP COLUMN X".into(),
                risk: Risk::Danger,
            }],
            blocked: vec![nsql_ddl::Blocked {
                what: "Y".into(),
                reason: Reason::NeedsRebuild,
            }],
        };
        let checks = vec![
            (CheckKind::ValueRows, "X".to_string()),
            (CheckKind::NullRows, "Z".to_string()),
        ];
        let h = preview_header("T", &plan, &checks, &[Some(7), Some(0)], true);
        assert!(h.contains("1 statement(s)"));
        assert!(h.contains("PRODUCTION"));
        assert!(h.contains("DANGER"));
        assert!(h.contains("X: 7 row(s) have a value"));
        assert!(!h.contains("Z:"));
        assert!(h.contains("NOT APPLIED: Y needs table re-creation"));
    }

    #[test]
    fn edited_script_splits_per_dialect_and_drops_comment_only_parts() {
        let ora =
            "-- header\n\nALTER TABLE T ADD (A NUMBER);\n-- note\nCOMMENT ON TABLE T IS 'x;y';\n";
        let s = edited_statements(nsql_ddl::Dialect::Oracle, ora);
        assert_eq!(s.len(), 2, "{s:?}");
        assert!(s[0].ends_with("ALTER TABLE T ADD (A NUMBER)"));
        assert!(s[1].ends_with("COMMENT ON TABLE T IS 'x;y'"));
        let ms = "-- header\nGO\nDECLARE @a int = 1; IF @a = 1 EXEC(N'x')\nGO\nALTER TABLE t ADD c int NULL\ngo\n";
        let s = edited_statements(nsql_ddl::Dialect::Mssql, ms);
        assert_eq!(
            s,
            [
                "DECLARE @a int = 1; IF @a = 1 EXEC(N'x')",
                "ALTER TABLE t ADD c int NULL"
            ]
        );
        assert!(edited_statements(nsql_ddl::Dialect::Postgres, "-- only a comment\n\n").is_empty());
    }

    #[test]
    fn applied_script_header_says_what_ran() {
        let ok = applied_script(
            "APP",
            "T",
            "2026-11-27 23:00:00.000",
            2,
            2,
            true,
            "A;\nB;\n",
        );
        assert!(ok.starts_with(
            "-- Nexa SQL table editor: APP.T - applied 2 of 2 statement(s) at 2026-11-27"
        ));
        assert!(!ok.contains("STOPPED"));
        assert!(ok.ends_with("\nA;\nB;\n"));
        let bad = applied_script("APP", "T", "x", 1, 3, false, "A;\nB;\n");
        assert!(bad.contains("applied 1 of 3"));
        assert!(bad.contains("-- STOPPED: the last statement below failed"));
    }

    #[test]
    fn model_takes_fk_rule_and_check_expression_from_extras() {
        let mut d = data();
        d.detail.keys.push(KeyDef {
            name: "T_FK".into(),
            kind: 'R',
            cols: vec!["ID".into()],
            ref_table: Some("P".into()),
            ref_cols: vec!["ID".into()],
        });
        d.key_extra = Some(vec![
            nsql_catalog::KeyExtra {
                name: "T_FK".into(),
                kind: 'R',
                on_delete: "CASCADE".into(),
                on_update: "SET NULL".into(),
                ref_table: "OTHER.P".into(),
                ..nsql_catalog::KeyExtra::default()
            },
            nsql_catalog::KeyExtra {
                name: "T_CK".into(),
                kind: 'C',
                check: "ID > 0".into(),
                locked: true,
                ..nsql_catalog::KeyExtra::default()
            },
        ]);
        let m = to_model("APP", "T", &d);
        let fk = m.keys.iter().find(|k| k.name == "T_FK").expect("fk");
        assert_eq!(fk.on_delete, "CASCADE");
        assert_eq!(fk.on_update, "SET NULL");
        // 다른 스키마의 대상 = 덧붙은 속성의 `스키마.테이블`이 이긴다.
        assert_eq!(fk.ref_table, "OTHER.P");
        // 표현 못 하는 속성이 있는 제약만 잠긴다.
        assert_eq!(locked_keys(&d), ["T_CK"]);
        // 속성 항목이 없는 기존 외래 키·체크도 잠긴다(모르는 채로 다시 만들지 않는다).
        d.detail.keys.push(KeyDef {
            name: "SYS_C001".into(),
            kind: 'C',
            cols: vec!["ID".into()],
            ref_table: None,
            ref_cols: vec![],
        });
        assert_eq!(locked_keys(&d), ["T_CK", "SYS_C001"]);
        // 제약 목록에 없던 체크 제약도 들어온다.
        let ck = m.keys.iter().find(|k| k.name == "T_CK").expect("ck");
        assert_eq!((ck.kind, ck.check.as_str()), (KeyKind::Check, "ID > 0"));
    }

    #[test]
    fn dialect_map() {
        assert_eq!(tb_dialect(Dialect::Oracle), Some(nsql_ddl::Dialect::Oracle));
        assert_eq!(tb_dialect(Dialect::Odbc), None);
        assert_eq!(tb_dialect(Dialect::Db2), None);
    }
}
