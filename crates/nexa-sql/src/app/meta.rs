//! App — 메타·객체 상세·탐색기 동작(docs/85·86).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 문장에 스키마가 없을 때 쓸 그 세션의 기본 스키마(`?schema=` · SQL Server는 그 값이 DB라 제외 → 탐색기가 `dbo`로).
    pub(crate) fn meta_default_schema(&self) -> Option<String> {
        if self.sess.dialect == Dialect::Mssql {
            return None;
        }
        self.sess.spec.as_ref().and_then(|s| s.schema.clone())
    }

    /// 성공한 문장이 DDL이면 대상을 모은다(docs/57 T1) — 반영은 실행이 끝난 뒤(또는 커밋 때) 한 번에.
    pub(crate) fn meta_on_done(&mut self, stmt: &str) {
        if !self.settings.flag("meta.refresh_on_ddl") {
            return;
        }
        let Some(t) = nsql_core::ddl_target(stmt, self.sess.dialect) else {
            return;
        };
        let wait = sessions::ddl_waits_for_commit(
            self.sess.dialect.ddl_transactional(),
            self.settings.flag("session.autocommit"),
            self.settings.flag("meta.refresh_on_commit"),
        );
        if wait {
            self.sess.ddl_wait.push(t);
        } else {
            self.sess.ddl_now.push(t);
        }
    }

    /// 모아 둔 DDL 대상을 탐색기에 반영 — 같은 (동작 종류·객체 종류·스키마)는 폴더가 같으므로 탐색기가 겹친 요청을 하나로 접는다.
    pub(crate) fn meta_flush(&mut self, targets: Vec<nsql_core::DdlTarget>) {
        if targets.is_empty() {
            return;
        }
        let schema = self.meta_default_schema();
        let mut sent = 0;
        for t in &targets {
            sent += self
                .explorer
                .apply_ddl(self.sess.spec.as_ref(), t, schema.as_deref());
        }
        dlog!(self, LogLayer::Load, LogLevel::Timing, {
            LogEntry::new(
                LogKind::Info,
                format!(
                    "explorer refresh after DDL: {} target(s) → {sent} folder request(s)",
                    targets.len()
                ),
            )
        });
        if sent > 0 {
            self.redraw();
        }
    }

    /// 유휴 워터마크(docs/57 T2): `meta.refresh_secs`마다 · 입력이 `meta.refresh_idle_ms` 동안 없고 실행 중인 세션이 없을 때만.
    pub(crate) fn meta_refresh_tick(&mut self, now: Instant) -> Option<Instant> {
        let secs = self.settings.int("meta.refresh_secs").max(0) as u64;
        if secs == 0 || !self.explorer.is_visible() {
            return None;
        }
        let next = *self
            .meta_refresh_next
            .get_or_insert(now + Duration::from_secs(secs));
        if now < next {
            return Some(next);
        }
        let idle = Duration::from_millis(self.settings.int("meta.refresh_idle_ms").max(0) as u64);
        let busy = self.all_sess().any(|s| s.busy);
        if busy || now.duration_since(self.blink_origin) < idle {
            // 바쁘면 조금 뒤에 다시 본다(주기를 통째로 미루지 않는다).
            let retry = now + idle.max(Duration::from_secs(5));
            self.meta_refresh_next = Some(retry);
            return Some(retry);
        }
        let all = self.settings.get("meta.refresh_scope") == Some("all");
        self.explorer.watermark_poll(all);
        let next = now + Duration::from_secs(secs);
        self.meta_refresh_next = Some(next);
        Some(next)
    }

    /// 탐색기가 부탁한 동작(우클릭 메뉴 · 더블클릭 — SQL 열기 · 이름 복사 · 서버 연결/해제 · 새 탭)을 **바로** 처리한다.
    /// ★ 종전에는 워커 응답을 걷는 `drain_events` 안에서만 걷어서, 메뉴로 고른 연결 해제·이름 복사가 **다음 워커 응답이 올 때까지**
    ///   실행되지 않았다(응답이 없으면 끝내 안 됨 · 사용자 09-21). 입력을 탐색기에 준 직후에도 부른다.
    /// ★ 객체 상세 패널(docs/86): 탐색기 선택이 바뀌었으면 대상을 바꾸고 객체면 상세를 청한다(`force` = 켜는 순간).
    pub(crate) fn sync_detail_target(&mut self, force: bool) {
        if !self.objdetail.is_visible() {
            return;
        }
        let t = self.explorer.selected_target();
        let key = t.as_ref().map(explorer::DetailTarget::key);
        if !force && key == self.detail_key {
            return;
        }
        self.detail_key = key;
        // ★ 상세 캐시(86 §5 · 사용자 09-26): 이미 본 대상은 깜빡임 없이 즉시 · 새로 고침 범위/TTL 지난 것만 다시 읽어 교체.
        let cached = match &t {
            Some(explorer::DetailTarget::Object(_)) => t
                .as_ref()
                .map(explorer::DetailTarget::key)
                .and_then(|k| self.explorer.cached_details(&k)),
            _ => None,
        };
        match &t {
            Some(explorer::DetailTarget::Object(o)) => {
                // ★ 지연 로딩(사용자 09-28 · 61 §1-8): 접힌 한 줄 모드는 **설명(코멘트)만** 필요하다 → 상세는 청하지 않는다.
                //   펼치는 순간 `detail_actions`가 `sync_detail_target(true)`로 다시 와서 캐시가 있으면 즉시 배정 · 없으면 그때 읽는다.
                if !self.objdetail.is_collapsed() && cached.as_ref().is_none_or(|(_, stale)| *stale)
                {
                    self.explorer.request_details(o.clone(), None);
                }
                // 테이블 코멘트 = 메타(스키마 단위 워머)에 있으면 즉시 · 없으면 테이블 단위로 한 번(그 컬럼들도 즉시 · 86 §4~5).
                if o.kind.is_relation() {
                    self.feed_comments(o);
                }
            }
            // 컬럼 = 탐색기 값은 즉시 · 코멘트는 메타/캐시 · 모르면 한 번 읽는다(상세 왕복 없음).
            Some(explorer::DetailTarget::Column { owner, .. }) => {
                let o = owner.clone();
                self.feed_comments(&o);
            }
            _ => {}
        }
        let schema = match &t {
            Some(explorer::DetailTarget::Schema(s)) => Some(s.clone()),
            _ => None,
        };
        self.objdetail.set_target(t.clone());
        if let (Some(explorer::DetailTarget::Object(o)), Some((secs, _))) = (&t, cached) {
            self.objdetail.set_sections(o, None, Ok(secs));
        }
        if let Some(sc) = schema {
            let counts = self.explorer.schema_kind_counts(&sc);
            self.objdetail.set_schema_counts(counts);
        }
        self.redraw();
    }

    /// 코멘트를 패널에: 메타(스키마 워머)에 있으면 즉시 · 패널 캐시에 없으면 테이블 단위로 요청(86 §4~5).
    fn feed_comments(&mut self, o: &nsql_catalog::ObjectInfo) {
        if self.objdetail.knows_comments(o) {
            return;
        }
        if let Some((tc, cols)) = self.explorer.comments_of(o) {
            self.objdetail.set_comments(o, tc, cols);
        } else {
            self.explorer.request_comments(o.clone());
        }
    }

    /// 객체 상세 패널의 동작(복사 · 축소/확장).
    pub(crate) fn detail_actions(&mut self) {
        for a in self.objdetail.take_actions() {
            match a {
                objdetail::DetailAction::Copy(s) => {
                    if !clipboard::write_text(&s) {
                        self.sess.status = t(Msg::ErrClipboard).into();
                    }
                    self.redraw();
                }
                objdetail::DetailAction::OpenInEditor(title, text) if self.tab_room() => {
                    self.editors.new_tab(Some(title));
                    self.editors.cur_mut().set_text(&text);
                    self.editors.cur_mut().goto_line(1);
                    self.set_focus(Focus::Editor);
                    self.redraw();
                }
                objdetail::DetailAction::OpenInEditor(..) => {}
                objdetail::DetailAction::Collapsed(on) => {
                    let _ = self
                        .settings
                        .set("explorer.details_collapsed", if on { "on" } else { "off" });
                    self.persist_settings();
                    self.layout();
                    // ★ 펼치는 순간에 상세를 채운다(지연 로딩 · 캐시가 있으면 즉시 배정 · 없으면 그때 조회 · 사용자 09-28).
                    if !on {
                        self.sync_detail_target(true);
                    }
                    self.redraw();
                }
            }
        }
    }

    pub(crate) fn explorer_actions(&mut self) -> bool {
        // 새로 고침 범위 → 상세 패널 코멘트 캐시도 같은 범위로 버리고 보이는 대상은 다시 채운다(T-227 후속 · 사용자 09-26 "변경 요청 누락 없이").
        let inv = self.explorer.take_detail_invalidations();
        if !inv.is_empty() {
            let mut n = 0;
            for (sc, nm) in &inv {
                n += self.objdetail.forget_comments(sc.as_deref(), nm.as_deref());
            }
            if n > 0 {
                self.sync_detail_target(true);
            }
        }
        self.sync_detail_target(false);
        let mut changed = false;
        for a in self.explorer.take_actions() {
            changed = true;
            match a {
                ExplorerAction::OpenSql {
                    title,
                    text,
                    origin,
                } if self.tab_room() => {
                    self.editors.new_tab(Some(title));
                    self.editors.cur_mut().set_text(&text);
                    // 캐럿 = 문서 처음(BOF · 사용자 09-26 "Open source 뒤 EOF가 아니라 BOF").
                    self.editors.cur_mut().goto_line(1);
                    self.set_focus(Focus::Editor);
                    // ★ 객체 소스 탭(09-30): 출처를 기억하고(F5 = 한 단위 실행) 그 객체의 서버 세션에 묶는다 —
                    //   종전에는 활성 공유 세션(다른 서버일 수 있음)에 붙어 방언이 달라지면 분할이 어긋났다.
                    if let Some(o) = origin {
                        let tab = self.editors.active_id();
                        if let Some(spec) = o.server.clone() {
                            self.bind_tab_to_server(tab, &spec);
                        }
                        self.object_tabs.insert(tab, o);
                        self.sync_sess();
                        self.sync_sess_ui();
                    }
                }
                ExplorerAction::OpenSql { .. } => {}
                ExplorerAction::Status(s) => self.sess.status = s,
                // 상태줄은 놓치기 쉽다 → 경고 토스트도(예: 연결이 해제된 서버에서 새로 고침).
                ExplorerAction::Notice(s) => {
                    self.toasts.push(
                        toast::ToastKind::Warn,
                        t(Msg::ExpNotConnected).to_string(),
                        s.clone(),
                    );
                    self.sess.status = s;
                }
                // (서버 제거는 `ExplorerSet::take_actions`가 안에서 처리한다.)
                ExplorerAction::RemoveServer => {}
                ExplorerAction::DisconnectServer(spec) => {
                    if let Some(spec) = spec {
                        self.disconnect_server(&spec);
                    }
                }
                ExplorerAction::ConnectServer(spec) => {
                    if let Some(spec) = spec {
                        self.connect_server(spec);
                    }
                }
                ExplorerAction::NewTabHere(spec) => {
                    if let Some(spec) = spec {
                        self.new_tab_on(&spec);
                    }
                }
                ExplorerAction::Copy(s) => {
                    if !clipboard::write_text(&s) {
                        self.sess.status = t(Msg::ErrClipboard).into();
                    }
                }
                ExplorerAction::Details { owner, col, r } => {
                    self.objdetail.set_sections(&owner, col.as_ref(), r);
                }
                ExplorerAction::Comments { owner, table, cols } => {
                    self.objdetail.set_comments(&owner, table, cols);
                }
                // 스키마 코멘트가 메타에 들어왔다 → 지금 대상이 그 스키마면 메타에서 채운다(패널 캐시 없을 때).
                ExplorerAction::CommentsLoaded { schema } => {
                    let owner = match self.explorer.selected_target() {
                        Some(explorer::DetailTarget::Object(o)) if o.schema == schema => Some(o),
                        Some(explorer::DetailTarget::Column { owner, .. })
                            if owner.schema == schema =>
                        {
                            Some(owner)
                        }
                        _ => None,
                    };
                    if let Some(o) = owner {
                        self.feed_comments(&o);
                    }
                }
                // Generate SQL 결과(83 §3) — 창은 `el`이 있는 자리에서 연다(이미 열려 있으면 바로 본문 교체).
                ExplorerAction::DropObject { owner, server } => {
                    self.drop_request(owner, server);
                    changed = true;
                }
                // DDL 결과가 삭제 백업용이면 미리보기 창 대신 백업 흐름으로(10-01).
                ExplorerAction::Preview { spec, r, .. } if self.drop_take_backup(&spec, &r) => {
                    changed = true;
                }
                ExplorerAction::Preview { spec, r, server } => {
                    if let Err(e) = &r {
                        self.sess.status = tf(Msg::StGenFailed, &[e]);
                    } else {
                        self.sess.status = spec.title();
                    }
                    if self.sqlprev_win.is_open() {
                        self.sqlprev_win.spec = Some(spec);
                        self.sqlprev_win.server = server;
                        self.sqlprev_win.set_result(r);
                    } else {
                        self.sqlprev_pending = Some((spec, r, server));
                    }
                    changed = true;
                }
                // ★ Import Data…(89 §3-3): 표 표기를 정하고 파일 창(열기) → 고르면 Import 창.
                ExplorerAction::Import { owner, server } => {
                    if !self.lic_gate(nsql_license::Feature::ImportGui) {
                        changed = true;
                        continue;
                    }
                    let dialect = server
                        .as_ref()
                        .and_then(|s| s.dialect)
                        .unwrap_or(self.sess.dialect);
                    let table = nsql_catalog::qualified(dialect, &owner.schema, &owner.name);
                    self.import_ctx = Some((table, PathBuf::new(), server));
                    self.file_purpose = FilePurpose::Import;
                    self.open_file_dlg = Some(PickerMode::Open);
                    changed = true;
                }
            }
        }
        changed
    }
}
