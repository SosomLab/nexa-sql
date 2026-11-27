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
    let keys: Vec<nsql_ddl::Key> = d
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
            ref_table: k.ref_table.clone().unwrap_or_default(),
            ref_cols: k.ref_cols.clone(),
            ..nsql_ddl::Key::default()
        })
        .collect();
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
            TbPending::Load | TbPending::Reload { ok: true } => tab.view.load(model),
            TbPending::Reload { ok: false } => tab.view.rebase(model),
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
        let Some(key) = self.tbledit_apply_key.take() else {
            return;
        };
        let Some(tab) = self.tbledits.get(&key) else {
            return;
        };
        if !tab.view.can_apply() || self.tbledit_run.is_some() {
            return;
        }
        let stmts: Vec<String> = tab
            .view
            .plan()
            .stmts
            .iter()
            .map(|s| s.sql.clone())
            .collect();
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
        // 새 테이블이 만들어졌으면 이제부터 그 이름의 기존 테이블이다.
        if let Some(tab) = self.tbledits.get_mut(&run.key) {
            if tab.view.is_new() && applied > 0 {
                tab.view.created();
            }
            if !tab.view.is_new() {
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
        let redraw = self
            .tbledits
            .get_mut(&key)
            .is_some_and(|tab| tab.view.on_event(ev));
        self.tbledit_actions();
        if redraw {
            self.redraw();
        }
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

    /// 닫힌 탭의 상태를 버린다(탭 전환·닫기 뒤 · 닫기 = 즉시 전부 해제 D-323).
    pub(crate) fn tbledit_sync(&mut self) {
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
    /// `tbledit.set:<행>:<열>=<글>` · `tbledit.toggle:<행>:<열>` · `tbledit.remove:<행>` · `tbledit.move:<행>:<±1>` ·
    /// `tbledit.name:<글>` · `tbledit.comment:<글>` · `tbledit.revert` · `tbledit.refresh` · `tbledit.apply`(미리보기까지) ·
    /// `tbledit.apply_now`(미리보기 없이 실행 = 시험 전용 길) · `tbledit.dump:<파일>`.
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
            "apply" => {
                self.tbledit_apply_request(&key);
                return true;
            }
            "apply_now" => {
                self.tbledit_apply_key = Some(key);
                self.tbledit_apply_go();
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
            } else if let Some(s) = rest.strip_prefix("set:") {
                if let Some((pos, text)) = s.split_once('=') {
                    if let Some((r, c)) = nums(pos) {
                        v.set_cell(r, c, text);
                    }
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
    fn dialect_map() {
        assert_eq!(tb_dialect(Dialect::Oracle), Some(nsql_ddl::Dialect::Oracle));
        assert_eq!(tb_dialect(Dialect::Odbc), None);
        assert_eq!(tb_dialect(Dialect::Db2), None);
    }
}
