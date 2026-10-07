//! App — 확장(관리자·패널·명령 · docs/50·75).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 설정 → 편집기 안내선(표시 · 색 · 투명도) + 동일 출현 외곽선(사용자 09-16).
    /// 끈 확장 id 목록(`extensions.disabled`).
    pub(crate) fn ext_disabled(&self) -> Vec<String> {
        // 확장 관리자가 꺼져 있으면 **모든 확장이 꺼진 것**(설치 기록·개별 켬/끔과 무관 · 사용자 09-19).
        if !self.settings.flag("extensions.enabled") {
            return self
                .extensions
                .ids()
                .into_iter()
                .map(|(id, _)| id)
                .collect();
        }
        self.ext_disabled_list()
    }

    /// 설정 `extensions.disabled` 그대로(관리자 상태와 무관 — 패널·목록의 켬/끔 표시용).
    fn ext_disabled_list(&self) -> Vec<String> {
        self.settings
            .get("extensions.disabled")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }

    /// 확장 효과 적용(시작 · 설정 변경 · 켜기/끄기): 레지스트리 → 편집기 전 탭(괄호 옵션 · 우클릭 서브메뉴).
    pub(crate) fn apply_extensions(&mut self, changed_key: Option<&str>) {
        // 끈 것 + **설치 기록 없는 내장 확장** = 효과 없음(설치해야 켜진다 · 사용자 09-17).
        let installed = extensions::manager::installed();
        // ★ 설치된 WASM 패키지 = 모듈을 로드해 레지스트리에(같은 id의 내장은 대체 · 실패면 내장 폴백 · docs/75).
        let want: Vec<(String, PathBuf)> = extensions::manager::root_dir()
            .map(|root| {
                installed
                    .iter()
                    .filter(|r| r.kind == extensions::manager::Kind::Wasm)
                    .filter_map(|r| {
                        extensions::wasm_module_path(&root, &r.id, &r.version)
                            .map(|p| (r.id.clone(), p))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for note in self.extensions.sync_wasm(&want) {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
        let mut disabled = self.ext_disabled();
        for (id, _) in self.extensions.ids() {
            if !installed.iter().any(|r| r.id == id) && !disabled.contains(&id) {
                disabled.push(id);
            }
        }
        // ★ 모든 확장 = 설정에 "확장 사용" 항목(사용자 10-07): 설치된 것마다 `ext.<id>.use`(자기 분류가 있으면 그 분류 맨 앞 · 없으면
        //   확장 이름 분류를 동적으로 = Project Explorer Menus처럼 설정이 0인 확장도 설정 창에 분류가 생긴다). 값 = `extensions.disabled`의
        //   거울(단일 원천은 목록 · 관리자 끔은 반영하지 않는다 = 개별 상태 보존).
        let mut extra_cats: Vec<Msg> = Vec::new();
        let mut dyn_entries = Vec::new();
        for r in &installed {
            let cat = match nsql_settings::extension_categories()
                .into_iter()
                .find(|(_, i)| *i == r.id)
            {
                Some((c, _)) => c,
                None => {
                    let c = nsql_i18n::dyn_msg(&r.name, &r.name);
                    nsql_settings::register_extension_category(c, &r.id);
                    c
                }
            };
            if !nsql_settings::CATEGORY_TREE
                .iter()
                .any(|(_, cats)| cats.contains(&cat))
            {
                extra_cats.push(cat);
            }
            dyn_entries.push(nsql_settings::Entry {
                key: nsql_settings::ext_use_key(&r.id),
                cat,
                label: Msg::LblExtUse,
                desc: Msg::DescExtUse,
                kind: nsql_settings::SettingKind::Bool,
                default: "on",
            });
        }
        nsql_settings::register_dynamic(dyn_entries);
        let list_off = self.ext_disabled_list();
        for r in &installed {
            let key = nsql_settings::ext_use_key(&r.id);
            let v = if list_off.contains(&r.id) {
                "off"
            } else {
                "on"
            };
            if self.settings.get(key) != Some(v) {
                let _ = self.settings.set(key, v);
            }
        }
        self.prefs_win.set_extra_categories(extra_cats);
        let effects = self
            .extensions
            .on_settings(&self.settings, changed_key, &disabled);
        for e in effects {
            if let Some(mut b) = e.bracket_opts {
                // 자동 닫기 = 편집 코어 설정(`editor.auto_close_pairs`) — Rainbow Pairs 기능이 아니다(확장 유무와 무관 · 사용자 09-19).
                b.auto_close = self.settings.flag("editor.auto_close_pairs");
                // 쌍 종류 · 문자열 안 · 현재 쌍 강조도 편집 코어 설정(`editor.pair_*` · 사용자 09-23 "Rainbow 확장이 아니라 기본 기능 설정으로").
                b.pairs = nexa_ctl::PairOpts {
                    kinds: nexa_ctl::PairOpts::kinds_from_spec(
                        self.settings.get("editor.pair_kinds").unwrap_or(""),
                    ),
                    in_strings: self.settings.flag("editor.pair_in_strings"),
                };
                b.match_mode = match self.settings.get("editor.pair_match").unwrap_or("near") {
                    "off" => 0,
                    "always" => 2,
                    _ => 1,
                };
                self.editors.set_bracket_opts(b);
            }
        }
        self.refresh_menu_extras();
        // 설정 창 `format.default` 콤보 = 지금 쓸 수 있는 포맷터(사용자 09-29).
        let choices = self.format_default_choices();
        self.prefs_win.set_dyn_choices("format.default", choices);
        // 설정 창: **미설치** 확장의 분류만 숨긴다(사용자 09-17 "설치되면 보이고 제거하면 사라진다") — 끈 확장의 분류는 이제 남는다
        //   ("확장 사용" 토글로 설정 창에서 다시 켤 수 있어야 하므로 · 사용자 10-07).
        let hidden: Vec<Msg> = nsql_settings::extension_categories()
            .into_iter()
            .filter(|(_, id)| !installed.iter().any(|r| r.id == *id))
            .map(|(c, _)| c)
            .collect();
        self.prefs_win.set_hidden_categories(hidden);
        self.prefs_sync();
        self.redraw();
    }

    /// 확장 명령(짝/형제/상위/하위 이동 등) — 소유 확장에 위임 · 켜져 있을 때만.
    pub(crate) fn run_extension_cmd(&mut self, id: &str) {
        let disabled = self.ext_disabled();
        // ★ 호스트 상황 = 활성 문서의 파일 경로(docs/97 · 확장이 `Editor::doc_path()`로 읽는다).
        let doc = self
            .editors
            .active_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if self
            .extensions
            .run_with(id, &disabled, self.editors.cur_mut(), &doc)
        {
            self.redraw();
        }
        // WASM 확장의 `nx_log` 줄·오류를 로그 창에(docs/75 §6).
        for note in self.extensions.take_wasm_notes() {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
    }

    /// Extension Manager 팔레트 명령(Sublime Package Control 방식 · docs/50 §10): 텍스트 목록을 만들어 팔레트에 띄우고,
    /// 고르면 `ext.<verb>:<key>`로 다시 들어온다.
    pub(crate) fn ext_command(&mut self, id: &str) {
        use extensions::manager as mgr;
        let disabled = self.ext_disabled();
        let builtin: Vec<(String, String)> = self.extensions.ids();
        let installed = mgr::installed();
        // builtin도 **설치 기록**이 있어야 설치된 것(설치 = 켜기 + 설정 분류 표시 · 삭제 = 끄기 + 숨김 · 사용자 09-17).
        let is_installed = |x: &str| installed.iter().any(|r| r.id == x);
        let _ = &builtin;
        let mut cmds: Vec<(String, String)> = Vec::new();
        // Package Control처럼 1회 활성화 — 켜기 전에는 목록/설치를 막는다(네트워크 사용을 알리는 지점).
        if id == "ext.enable_mgr" || id == "ext.disable_mgr" {
            let on = id == "ext.enable_mgr";
            let _ = self
                .settings
                .set("extensions.enabled", if on { "on" } else { "off" });
            let _ = self.settings.save();
            self.sess.status = if on {
                tf(
                    Msg::StExtManagerEnabled,
                    &[&mgr::default_source(&self.settings).display()],
                )
            } else {
                t(Msg::StExtManagerDisabled).into()
            };
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            // 끄면 확장 상세 뷰 탭(`ext:*`)도 전부 닫는다(사용자 09-28 — 관리자가 꺼진 채 상세 탭만 남아 있었다).
            if !on {
                self.editors.close_view_tabs("ext:");
                self.ext_details.clear();
            }
            // 켬/끔 = 확장 효과 전체 재적용(끄면 전부 정지) + 활동 막대 아이콘·패널(layout).
            self.apply_extensions(None);
            self.layout();
            self.redraw();
            return;
        }
        if !self.settings.flag("extensions.enabled") {
            self.sess.status = t(Msg::StExtManagerOff).into();
            self.redraw();
            return;
        }
        match id {
            "ext.install" => {
                self.ext_catalog.clear();
                for src in mgr::sources(&self.settings) {
                    let mut tr = mgr::Trace::default();
                    let r = mgr::fetch_index_traced(&src, &mut tr);
                    self.ext_trace(tr);
                    match r {
                        Ok(idx) => {
                            for p in idx.packages {
                                let n = self.ext_catalog.len();
                                if is_installed(&p.id) {
                                    self.ext_catalog.push((src.clone(), p));
                                    continue;
                                }
                                cmds.push((
                                    format!("ext.install:{n}"),
                                    format!(
                                        "{} {} - {} [{}] · {}",
                                        p.name,
                                        p.version,
                                        p.summary,
                                        p.kind.as_str(),
                                        src.display()
                                    ),
                                ));
                                self.ext_catalog.push((src.clone(), p));
                            }
                        }
                        Err(e) => {
                            self.sess.status = tf(Msg::StExtIndexFailed, &[&src.display(), &e]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                        }
                    }
                }
                // 비어도 **목록 창은 연다**(안내 한 줄 · 상태줄 글자만으로는 못 본다 · 사용자 09-19).
                if cmds.is_empty() {
                    cmds.push(("ext.noop".into(), t(Msg::StExtNoneAvailable).into()));
                }
                self.ext_panel_sync();
            }
            "ext.remove" | "ext.list" | "ext.enable" | "ext.disable" => {
                let verb = id.trim_start_matches("ext.");
                for r in &installed {
                    let off = disabled.iter().any(|d| d == &r.id);
                    let ok = match verb {
                        "enable" => off,
                        "disable" => !off,
                        _ => true,
                    };
                    if ok {
                        cmds.push((
                            format!("ext.{verb}:{}", r.id),
                            format!(
                                "{} {} · {} · {}",
                                r.name,
                                r.version,
                                r.kind.as_str(),
                                if off {
                                    t(Msg::ExtStDisabled)
                                } else {
                                    t(Msg::ExtStEnabled)
                                }
                            ),
                        ));
                    }
                }
                // 설치된 확장이 없어도 **목록 창을 열고 거기서** 알린다(사용자 09-19 · 종전 = 상태줄 "No extensions match").
                if cmds.is_empty() {
                    cmds.push((
                        "ext.noop".into(),
                        t(if installed.is_empty() {
                            Msg::ExtPanelNoneInstalled
                        } else {
                            Msg::StExtNoneInstalled
                        })
                        .into(),
                    ));
                }
            }
            "ext.repo_add" => {
                self.palette
                    .open_prompt("ext.repo_add", t(Msg::PhExtRepoUrl), "");
                self.redraw();
                return;
            }
            "ext.repo_list" | "ext.repo_remove" => {
                let user = mgr::user_sources(&self.settings);
                if id == "ext.repo_list" {
                    cmds.push((
                        "ext.repo:default".into(),
                        format!(
                            "{} {}",
                            mgr::default_source(&self.settings).display(),
                            t(Msg::StExtRepoDefault)
                        ),
                    ));
                }
                for (n, s) in user.iter().enumerate() {
                    cmds.push((
                        format!(
                            "{}:{n}",
                            if id == "ext.repo_list" {
                                "ext.repo"
                            } else {
                                "ext.repo_remove"
                            }
                        ),
                        s.display(),
                    ));
                }
                if cmds.is_empty() {
                    self.sess.status = t(Msg::StExtNoneInstalled).into();
                    self.redraw();
                    return;
                }
            }
            _ => return,
        }
        self.palette.set_commands(cmds);
        self.palette.open("");
        self.ime_refresh();
        self.redraw();
    }

    /// 팔레트에서 고른 확장 항목(`ext.<verb>:<key>`).
    pub(crate) fn ext_pick(&mut self, id: &str) {
        use extensions::manager as mgr;
        let (verb, key) = match id.trim_start_matches("ext.").split_once(':') {
            Some(p) => p,
            None => return,
        };
        if verb == "install" && !self.lic_gate(nsql_license::Feature::DriverExtensions) {
            return;
        }
        // 확장 관리자가 꺼져 있으면 고르기도 막는다(명령 쪽 `ext_command`와 같은 문).
        if !self.settings.flag("extensions.enabled") {
            self.sess.status = t(Msg::StExtManagerOff).into();
            self.redraw();
            return;
        }
        match verb {
            "install" => {
                let Some((src, sum)) = key
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| self.ext_catalog.get(n).cloned())
                else {
                    return;
                };
                let mut tr = mgr::Trace::default();
                let r = mgr::install(&src, &sum, &mut tr);
                self.ext_trace(tr);
                match r {
                    Ok(meta) => {
                        let note = if meta.message_install.is_empty() {
                            String::new()
                        } else {
                            format!(" - {}", meta.message_install)
                        };
                        self.sess.status =
                            tf(Msg::StExtInstalled, &[&meta.name, &meta.version, &note]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        self.apply_extensions(None);
                    }
                    Err(e) => {
                        self.sess.status = e;
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                    }
                }
            }
            "remove" => {
                let mut tr = mgr::Trace::default();
                let r = mgr::remove(key, &mut tr);
                self.ext_trace(tr);
                match r {
                    Ok(()) => {
                        self.sess.status = tf(Msg::StExtRemoved, &[key]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        self.apply_extensions(None);
                    }
                    Err(e) => self.sess.status = e,
                }
            }
            "enable" | "disable" => {
                let cur = self
                    .settings
                    .get("extensions.disabled")
                    .unwrap_or("")
                    .to_string();
                let next = mgr::list_toggle(&cur, key, verb == "disable");
                let _ = self.settings.set("extensions.disabled", &next);
                let _ = self.settings.save();
                self.sess.status = tf(
                    if verb == "disable" {
                        Msg::StExtDisabled
                    } else {
                        Msg::StExtEnabled
                    },
                    &[key],
                );
                self.apply_extensions(None);
            }
            // 목록에서 고르면 패널의 행 클릭과 같은 **상세 안내 탭**(종전 = 상태줄 한 줄).
            "list" => {
                let off = self.ext_disabled_list();
                if let Some(r) = mgr::installed().into_iter().find(|r| r.id == key) {
                    let cat = self.ext_catalog.iter().position(|(_, p)| p.id == r.id);
                    let latest = cat.and_then(|n| {
                        let v = &self.ext_catalog[n].1.version;
                        mgr::version_newer(v, &r.version).then(|| v.clone())
                    });
                    let row = ExtRow {
                        latest,
                        summary: cat
                            .map(|n| self.ext_catalog[n].1.summary.clone())
                            .unwrap_or_default(),
                        source: cat
                            .map(|n| self.ext_catalog[n].0.display())
                            .unwrap_or_default(),
                        enabled: !off.iter().any(|d| d == &r.id),
                        kind: r.kind.as_str().to_string(),
                        id: r.id,
                        name: r.name,
                        version: r.version,
                        installed: true,
                        catalog: cat,
                    };
                    self.ext_open_detail(&row);
                }
            }
            "repo" => {
                self.sess.status = if key == "default" {
                    mgr::default_source(&self.settings).display()
                } else {
                    key.parse::<usize>()
                        .ok()
                        .and_then(|n| {
                            mgr::user_sources(&self.settings)
                                .get(n)
                                .map(|s| s.display())
                        })
                        .unwrap_or_default()
                };
            }
            "repo_remove" => {
                let user = mgr::user_sources(&self.settings);
                if let Some(s) = key.parse::<usize>().ok().and_then(|n| user.get(n)) {
                    let cur = self
                        .settings
                        .get("extensions.repositories")
                        .unwrap_or("")
                        .to_string();
                    let next = mgr::list_toggle(&cur, &s.display(), false);
                    let _ = self.settings.set("extensions.repositories", &next);
                    let _ = self.settings.save();
                    self.sess.status = tf(Msg::StExtRepoRemoved, &[&s.display()]);
                    self.prefs_sync();
                }
            }
            _ => {}
        }
        if self.ext_panel.is_visible() {
            self.ext_panel_sync();
        }
        self.ext_details_sync();
        self.redraw();
    }

    /// 열려 있는 확장 상세 탭의 상태(설치됨·켜짐)를 지금 값으로 맞춘다 — 상세의 버튼을 누른 직후 버튼이 바뀌어야 한다.
    fn ext_details_sync(&mut self) {
        if self.ext_details.is_empty() {
            return;
        }
        let installed = extensions::manager::installed();
        let off = self.ext_disabled_list();
        for d in self.ext_details.values_mut() {
            let inst = installed.iter().any(|r| r.id == d.row.id);
            d.row.installed = inst;
            d.row.enabled = inst && !off.iter().any(|x| x == &d.row.id);
            d.row.catalog = self.ext_catalog.iter().position(|(_, p)| p.id == d.row.id);
            let state = t(if !inst {
                Msg::ExtStNotInstalled
            } else if d.row.enabled {
                Msg::ExtStEnabled
            } else {
                Msg::ExtStDisabled
            });
            if let Some(f) = d.fields.first_mut() {
                f.1 = state.to_string();
            }
        }
    }

    /// 확장 상세 뷰가 낸 동작 — 확장 패널과 같은 경로.
    pub(crate) fn ext_view_actions(&mut self) {
        for a in self.ext_view.take_actions() {
            match a {
                ext_view::ExtViewAction::Settings(id) => self.ext_open_settings(&id),
                ext_view::ExtViewAction::Install(n) | ext_view::ExtViewAction::Update(n) => {
                    self.ext_pick(&format!("ext.install:{n}"))
                }
                ext_view::ExtViewAction::Remove(id) => self.ext_pick(&format!("ext.remove:{id}")),
                ext_view::ExtViewAction::Enable(id) => self.ext_pick(&format!("ext.enable:{id}")),
                ext_view::ExtViewAction::Disable(id) => self.ext_pick(&format!("ext.disable:{id}")),
            }
        }
    }

    /// 확장 패널 목록 다시 만들기 — 설치 기록 + 마지막으로 읽은 카탈로그(네트워크 0).
    pub(crate) fn ext_panel_sync(&mut self) {
        use extensions::manager as mgr;
        let installed = mgr::installed();
        let off = self.ext_disabled_list();
        let mut rows: Vec<ExtRow> = installed
            .iter()
            .map(|r| {
                let cat = self.ext_catalog.iter().find(|(_, p)| p.id == r.id);
                // ★ 카탈로그가 더 새 버전을 알면 업데이트 대상(사용자 09-30).
                let latest = cat.and_then(|(_, p)| {
                    mgr::version_newer(&p.version, &r.version).then(|| p.version.clone())
                });
                ExtRow {
                    latest,
                    id: r.id.clone(),
                    name: r.name.clone(),
                    version: r.version.clone(),
                    kind: r.kind.as_str().to_string(),
                    summary: cat.map(|(_, p)| p.summary.clone()).unwrap_or_default(),
                    installed: true,
                    enabled: !off.iter().any(|d| d == &r.id),
                    catalog: self.ext_catalog.iter().position(|(_, p)| p.id == r.id),
                    source: cat.map(|(s, _)| s.display()).unwrap_or_default(),
                }
            })
            .collect();
        for (n, (src, p)) in self.ext_catalog.iter().enumerate() {
            if installed.iter().any(|r| r.id == p.id) || rows.iter().any(|r| r.id == p.id) {
                continue;
            }
            rows.push(ExtRow {
                id: p.id.clone(),
                name: p.name.clone(),
                version: p.version.clone(),
                kind: p.kind.as_str().to_string(),
                summary: p.summary.clone(),
                installed: false,
                enabled: false,
                catalog: Some(n),
                source: src.display(),
                latest: None,
            });
        }
        let note = if self.ext_fetch_rx.is_some() {
            t(Msg::ExtPanelLoading).to_string()
        } else {
            String::new()
        };
        self.ext_panel.set_rows(rows, note);
        self.redraw();
    }

    /// 저장소 `index.json` 읽기를 **스레드로** 시작(원격은 curl 최대 30초 — UI를 막지 않는다). 패널을 열 때와 ⟳에서만.
    pub(crate) fn ext_fetch_start(&mut self) {
        use extensions::manager as mgr;
        if self.ext_fetch_rx.is_some() {
            return;
        }
        let sources = mgr::sources(&self.settings);
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("ext-index".into())
            .spawn(move || {
                let out: ExtFetch = sources
                    .into_iter()
                    .map(|src| {
                        let mut tr = mgr::Trace::default();
                        let r = mgr::fetch_index_traced(&src, &mut tr);
                        (src, r, tr)
                    })
                    .collect();
                let _ = tx.send(out);
            });
        if spawned.is_ok() {
            self.ext_fetch_rx = Some(rx);
        }
        self.ext_panel_sync();
    }

    /// 읽기 결과 수거(틱) — 카탈로그 교체 · 추적 줄 · 실패는 로그.
    pub(crate) fn ext_fetch_poll(&mut self) {
        let Some(rx) = &self.ext_fetch_rx else {
            return;
        };
        let got = match rx.try_recv() {
            Ok(v) => v,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
        };
        self.ext_fetch_rx = None;
        self.ext_catalog.clear();
        for (src, r, tr) in got {
            self.ext_trace(tr);
            match r {
                Ok(idx) => {
                    for p in idx.packages {
                        self.ext_catalog.push((src.clone(), p));
                    }
                }
                Err(e) => {
                    let line = tf(Msg::StExtIndexFailed, &[&src.display(), &e]);
                    self.log_win.push(LogEntry::new(LogKind::Error, line));
                }
            }
        }
        self.ext_panel_sync();
    }

    /// 확장 패널이 낸 동작 처리.
    pub(crate) fn ext_panel_actions(&mut self) {
        for a in self.ext_panel.take_actions() {
            match a {
                ExtPanelAction::Refresh => self.ext_fetch_start(),
                ExtPanelAction::Settings(id) => self.ext_open_settings(&id),
                ExtPanelAction::Install(n) | ExtPanelAction::Update(n) => {
                    self.ext_pick(&format!("ext.install:{n}"))
                }
                ExtPanelAction::Remove(id) => self.ext_pick(&format!("ext.remove:{id}")),
                ExtPanelAction::Enable(id) => self.ext_pick(&format!("ext.enable:{id}")),
                ExtPanelAction::Disable(id) => self.ext_pick(&format!("ext.disable:{id}")),
                ExtPanelAction::Open(row) => self.ext_open_detail(&row),
            }
        }
    }

    /// ★ 설정 버튼(사용자 09-30): 그 확장의 설정 분류(`EXTENSION_CATEGORIES`)로 설정 창을 연다 · 분류가 없는 확장은 접두 검색.
    pub(crate) fn ext_open_settings(&mut self, id: &str) {
        let cat = nsql_settings::extension_categories()
            .into_iter()
            .find(|(_, i)| *i == id)
            .map(|(c, _)| c);
        match cat {
            Some(c) => self.prefs_category = Some(c),
            None => {
                use extensions::manager as mgr;
                let prefix = mgr::installed()
                    .iter()
                    .find(|r| r.id == id)
                    .and_then(|r| mgr::installed_meta(id, &r.version))
                    .map(|m| m.settings_prefix)
                    .unwrap_or_else(|| format!("ext.{}.", id.replace('-', "_")));
                self.prefs_query = Some(prefix);
            }
        }
        self.open_prefs = true;
    }

    /// ★ 설정 창에서 "확장 사용"(`ext.<id>.use`)을 바꿨다 → 확장 패널 켜기/끄기와 같은 길(`extensions.disabled` 갱신 → 재적용).
    pub(crate) fn ext_use_changed(&mut self, key: &str) {
        let Some(id) = extensions::manager::installed()
            .into_iter()
            .map(|r| r.id)
            .find(|id| nsql_settings::ext_use_key(id) == key)
        else {
            return;
        };
        let on = self.settings.flag(key);
        let already_off = self.ext_disabled_list().contains(&id);
        if on == !already_off {
            return;
        }
        let verb = if on { "enable" } else { "disable" };
        self.ext_pick(&format!("ext.{verb}:{id}"));
    }

    /// ★ 프로젝트 복원(사용자 09-30): `ext:<id>` 뷰 탭을 같은 뷰로 다시 연다 — 설치 목록에 있으면 true.
    pub(crate) fn ext_reopen_view(&mut self, id: &str) -> bool {
        if self.ext_panel.rows().is_empty() {
            self.ext_panel_sync();
        }
        let Some(row) = self
            .ext_panel
            .rows()
            .iter()
            .find(|r| r.id == id && r.installed)
            .cloned()
        else {
            return false;
        };
        self.ext_open_detail(&row);
        true
    }

    /// 확장 상세 = 읽기용 안내 탭(설치본의 메타 사본 → 없으면 저장소에서 · 없으면 목록 정보만).
    fn ext_open_detail(&mut self, row: &ExtRow) {
        use extensions::manager as mgr;
        let mut tr = mgr::Trace::default();
        let meta = mgr::installed_meta(&row.id, &row.version).or_else(|| {
            let (src, sum) = row.catalog.and_then(|n| self.ext_catalog.get(n))?;
            mgr::fetch_meta(src, &sum.dir, &mut tr).ok()
        });
        self.ext_trace(tr);
        let state = t(if !row.installed {
            Msg::ExtStNotInstalled
        } else if row.enabled {
            Msg::ExtStEnabled
        } else {
            Msg::ExtStDisabled
        });
        // 한 줄 설명 = 목록의 것 → 없으면(저장소를 읽기 전에 연 설치본) 메타의 것.
        let summary = if row.summary.is_empty() {
            meta.as_ref().map(|m| m.summary.clone()).unwrap_or_default()
        } else {
            row.summary.clone()
        };
        // ★ 편집기 탭이 아니라 **확장 탭**(전용 뷰 · VS Code식 · 사용자 09-19) — 글 본문은 비어 있고 호스트가 뷰를 그린다.
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut field = |label: Msg, v: &str| {
            if !v.is_empty() {
                fields.push((t(label).to_string(), v.to_string()));
            }
        };
        field(Msg::ExtDetState, state);
        // ★ 버전 칸 = 설치 버전 + (업데이트 가능: vN | 최신)(사용자 09-30).
        let ver = match (&row.latest, row.installed, row.catalog) {
            (Some(v), _, _) => format!("{} · {}", row.version, tf(Msg::ExtStUpdateAvail, &[v])),
            (None, true, Some(_)) => format!("{} · {}", row.version, t(Msg::ExtStUpToDate)),
            _ => row.version.clone(),
        };
        field(Msg::ExtDetVersion, &ver);
        field(Msg::ExtDetKind, &row.kind);
        field(Msg::ExtDetSource, &row.source);
        let mut description = String::new();
        if let Some(m) = &meta {
            field(Msg::ExtDetAuthor, &m.author);
            field(Msg::ExtDetLicense, &m.license);
            field(Msg::ExtDetHomepage, &m.homepage);
            field(Msg::ExtDetRequires, &m.requires.join(", "));
            field(Msg::ExtDetSettings, &m.settings_prefix);
            let files: Vec<String> = m.files.iter().map(|f| f.path.clone()).collect();
            field(Msg::ExtDetFiles, &files.join(", "));
            description = m.description.clone();
        }
        let mut shown = row.clone();
        shown.summary = summary;
        let key = format!("ext:{}", row.id);
        self.ext_details.insert(
            key.clone(),
            ext_view::ExtDetail {
                row: shown,
                fields,
                description,
            },
        );
        let created = self
            .editors
            .open_view_tab(&key, &format!("Extension: {}", row.name));
        self.set_focus(Focus::Editor);
        if created {
            // 읽을거리 탭 = DB 연결이 필요 없다(No connection).
            let tab = self.editors.active_id();
            self.make_unconnected(tab);
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.layout();
        self.redraw();
    }

    /// 매니저 추적 줄 → 로그 창 `ext` 층(개발자 모드 · `log.dev_layers`에 ext · 사용자 09-17 "다운로드 속도·설치 폴더까지").
    fn ext_trace(&mut self, tr: extensions::manager::Trace) {
        for line in tr.0 {
            dlog!(self, LogLayer::Ext, LogLevel::Timing, {
                LogEntry::new(LogKind::Info, line)
            });
        }
    }

    /// "Add Repository" 프롬프트 확정 — 루트에 index.json이 읽히면 설정에 더한다.
    pub(crate) fn ext_repo_add(&mut self, text: &str) {
        use extensions::manager as mgr;
        let src = mgr::Source::parse(text);
        if text.trim().is_empty() || mgr::fetch_index(&src).is_err() {
            self.sess.status = tf(Msg::StExtRepoBad, &[text.trim()]);
        } else {
            let cur = self
                .settings
                .get("extensions.repositories")
                .unwrap_or("")
                .to_string();
            let next = mgr::list_toggle(&cur, &src.display(), true);
            let _ = self.settings.set("extensions.repositories", &next);
            let _ = self.settings.save();
            self.sess.status = tf(Msg::StExtRepoAdded, &[&src.display()]);
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            self.prefs_sync();
        }
        self.redraw();
    }

    /// 활성 탭을 방금 읽었거나 저장했다 = 디스크와 기준이 맞다 → 서명을 기록하고 대기·유지·삭제 상태를 지운다(따라가기는 유지).
    pub(crate) fn ext_track_active(&mut self) {
        self.ext_track(self.editors.active_id());
    }

    /// 탭 하나의 파일 서명을 지금 디스크로 다시 잡는다(읽은 직후 · 저장 직후 — 활성 탭이 아닐 수도 있다).
    pub(crate) fn ext_track(&mut self, id: u64) {
        let path = self
            .editors
            .index_of_id(id)
            .and_then(|i| self.editors.path_of(i));
        let Some(path) = path else {
            self.ext_files.remove(&id);
            return;
        };
        let follow = self.ext_files.get(&id).is_some_and(|e| e.follow);
        self.ext_files.insert(
            id,
            extfile::ExtInfo {
                sig: nexa_fs::watch::file_sig(&path),
                follow,
                ..extfile::ExtInfo::default()
            },
        );
        self.ext_save_armed = None;
        self.ext_banner_sync();
    }

    /// 확인 요청(비동기) — `all` = 열린 파일 전부(창 활성화) · 아니면 활성 탭 하나(탭 전환·폴링). 실행 중인 탭은 건너뛴다.
    pub(crate) fn ext_check(&mut self, all: bool) {
        if self.settings.get("file.external_change") == Some("off") {
            return;
        }
        let active = self.editors.active_id();
        let reqs: Vec<nexa_fs::watch::WatchReq> = self
            .editors
            .files()
            .into_iter()
            .filter(|(id, _)| all || *id == active)
            .filter(|(id, _)| !self.editors.is_running(*id))
            .map(|(id, path)| nexa_fs::watch::WatchReq {
                key: id,
                path,
                known: self.ext_files.get(&id).and_then(extfile::ExtInfo::known),
            })
            .collect();
        self.ext_send(reqs);
    }

    fn ext_send(&mut self, reqs: Vec<nexa_fs::watch::WatchReq>) {
        if reqs.is_empty() {
            return;
        }
        if self.ext_watch.is_none() {
            let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
            let settle = self.settings.int("file.external_settle_ms").max(0) as u64;
            // 읽기 상한 = 병합 상한의 8배(그보다 큰 파일은 편집기가 열 대상이 아니다 — 읽지 않고 "못 읽음").
            let max = (self.settings.int("file.external_merge_max_kb").max(16) as u64) * 1024 * 8;
            self.ext_watch = nexa_fs::watch::StatWatch::spawn(
                Box::new(move || {
                    if let Ok(p) = proxy.lock() {
                        let _ = p.send_event(Wake);
                    }
                }),
                Duration::from_millis(settle),
                max,
            );
        }
        if let Some(w) = &self.ext_watch {
            w.check(reqs);
        }
    }

    /// 틱: 사건 수거 · 닫힌 탭 정리 · 폴링 예약(활성 창 = 보이는 탭 · 따라가기 탭 = 비활성에서도 간격 ×2).
    pub(crate) fn ext_tick(&mut self, now: Instant) -> Option<Instant> {
        let events = self
            .ext_watch
            .as_ref()
            .map(|w| w.poll())
            .unwrap_or_default();
        if !events.is_empty() {
            let mut changed = 0;
            for ev in events {
                changed += usize::from(self.ext_on_event(ev));
            }
            if changed > 1 {
                self.sess.status = tf(Msg::StExtMany, &[&changed.to_string()]);
            }
            self.ext_banner_sync();
            self.redraw();
        }
        let alive = self.editors.tab_ids();
        self.ext_files.retain(|id, _| alive.contains(id));
        self.editors.reap_views();
        // 저장 2단 확인은 3초 창 — 지나고도 남은 "저장 막힘" 띠는 10초 뒤 걷는다(다시 저장하면 다시 묻는다).
        if self
            .ext_save_armed
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(10))
        {
            self.ext_save_armed = None;
            self.ext_banner_sync();
        }
        let poll = self.settings.int("file.external_poll_ms").max(0) as u64;
        if poll == 0
            || self.settings.get("file.external_check") == Some("focus")
            || self.settings.get("file.external_change") == Some("off")
        {
            return None;
        }
        let following = self.ext_files.values().any(|e| e.follow);
        if !self.main_active && !following {
            self.ext_poll_next = None;
            return None;
        }
        let step = Duration::from_millis(if self.main_active { poll } else { poll * 2 });
        let next = *self.ext_poll_next.get_or_insert(now + step);
        if now < next {
            return Some(next);
        }
        if self.main_active {
            self.ext_check(false);
        }
        if following {
            let reqs: Vec<nexa_fs::watch::WatchReq> = self
                .editors
                .files()
                .into_iter()
                .filter(|(id, _)| self.ext_files.get(id).is_some_and(|e| e.follow))
                .map(|(id, path)| nexa_fs::watch::WatchReq {
                    key: id,
                    path,
                    known: self.ext_files.get(&id).and_then(extfile::ExtInfo::known),
                })
                .collect();
            self.ext_send(reqs);
        }
        self.ext_poll_next = Some(now + step);
        self.ext_poll_next
    }

    /// 사건 하나 처리 — 돌려주는 값 = 사용자가 알아야 할 변경이었는가.
    fn ext_on_event(&mut self, ev: nexa_fs::watch::WatchEvent) -> bool {
        use nexa_fs::watch::WatchEvent as E;
        match ev {
            E::Missing { key, path } => {
                let Some(_) = self.editors.index_of_id(key) else {
                    return false;
                };
                let info = self.ext_files.entry(key).or_default();
                if info.deleted {
                    return false;
                }
                info.deleted = true;
                info.pending = None;
                let name = nexa_fs::path::display(&path);
                self.sess.status = tf(Msg::StExtDeleted, &[&name]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                true
            }
            E::Unreadable { key, path, error } => {
                dlog!(self, LogLayer::Load, LogLevel::Timing, {
                    LogEntry::new(
                        LogKind::Info,
                        format!(
                            "external change: {} unreadable - {error} (tab {key})",
                            nexa_fs::path::display(&path)
                        ),
                    )
                });
                false
            }
            E::Changed {
                key,
                path,
                sig,
                bytes,
                ..
            } => self.ext_on_changed(key, &path, sig, &bytes),
        }
    }

    fn ext_on_changed(
        &mut self,
        id: u64,
        path: &Path,
        sig: nexa_fs::watch::FileSig,
        bytes: &[u8],
    ) -> bool {
        let Some(i) = self.editors.index_of_id(id) else {
            return false;
        };
        // 실행 중인 탭 = 줄 번호가 밀리면 실행 범위·오류 줄이 어긋난다 → 서명을 갱신하지 않고 둔다(다음 확인 때 다시 온다).
        if self.editors.is_running(id) {
            return false;
        }
        let Some((buf, base, enc, eol)) = self.editors.snapshot(i) else {
            return false;
        };
        let (mut base, enc) = (base.to_string(), enc.to_string());
        let (text, _, used) = Self::decode_bytes(bytes, &enc);
        let (disk_eol, disk) = eol::detect(&text);
        // 큰 파일 탭은 저장본 사본을 들고 있지 않다(docs/59) — 고치지 않은 탭이면 "기준 = 버퍼"로 보고 다시 읽기,
        //   고친 탭이면 병합 없이 묻는다(기준이 없으니 3-way가 성립하지 않는다).
        let large = self.editors.is_large(i);
        if large && !self.editors.is_dirty(i) {
            base = buf.clone();
        }
        let max = (self.settings.int("file.external_merge_max_kb").max(16) as usize) * 1024;
        let decision = extfile::decide(&extfile::DecideIn {
            base: &base,
            buf: &buf,
            disk: &disk,
            ask_always: self.settings.get("file.external_change") == Some("ask"),
            merge_on: self.settings.flag("file.external_merge") && !large,
            size_ok: bytes.len() <= max && buf.len() <= max,
            format_changed: disk_eol != eol || used != enc,
        });
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let now = Instant::now();
        let info = self.ext_files.entry(id).or_default();
        info.deleted = false;
        let suggest = info.note_change(now);
        let follow = info.follow;
        let mut notable = false;
        match decision {
            extfile::Decision::Ignore => {
                info.sig = Some(sig);
            }
            extfile::Decision::AdoptBase => {
                info.sig = Some(sig);
                info.pending = None;
                info.ignored = None;
                info.diverged = false;
                self.editors
                    .apply_external(i, None, &disk, Some(disk_eol), Some(used));
            }
            extfile::Decision::Reload => {
                info.sig = Some(sig);
                info.pending = None;
                info.ignored = None;
                info.diverged = false;
                self.editors
                    .apply_external(i, Some(&disk), &disk, Some(disk_eol), Some(used));
                self.sess.status = tf(Msg::StExtReloaded, &[&name]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                notable = true;
            }
            extfile::Decision::Merge(merged) => {
                info.sig = Some(sig);
                info.pending = None;
                info.ignored = None;
                info.diverged = false;
                self.ext_backup(&name, &buf);
                // 기준 = 디스크 본문 → 탭은 "디스크 대비 내 변경"만큼 dirty로 남는다.
                self.editors
                    .apply_external(i, Some(&merged.text), &disk, None, None);
                self.sess.status = tf(Msg::StExtMerged, &[&name, &merged.applied.to_string()]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                notable = true;
            }
            extfile::Decision::Ask(conflicts) => {
                if follow {
                    // 조용히 따라가기 = 겹치면 내 내용을 지킨다(띠 없음 · 저장은 2단 확인).
                    info.ignored = Some(sig);
                    info.diverged = true;
                } else {
                    info.pending = Some(extfile::Pending {
                        sig,
                        text: disk,
                        eol: disk_eol,
                        enc: used.to_string(),
                        conflicts,
                    });
                    info.ignored = None;
                    self.sess.status = tf(Msg::StExtConflict, &[&name]);
                    self.log_win
                        .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                    notable = true;
                }
            }
        }
        // 자주 바뀌는 파일 — 띠가 떠 있을 때 "조용히 따라가기"를 함께 내놓는다(`ext_banner_sync`가 본다).
        let _ = suggest;
        notable
    }

    /// 병합 전 본문 백업(`<설정 폴더>/backup/` · 최근 `file.external_backup_keep`개).
    fn ext_backup(&mut self, name: &str, text: &str) {
        let keep = self.settings.int("file.external_backup_keep").max(0) as usize;
        if keep == 0 {
            return;
        }
        let Some(dir) = nsql_settings::config_dir().map(|d| d.join("backup")) else {
            return;
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let stamp: String = nsql_log::now_local()
            .stamp()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let _ = std::fs::write(dir.join(format!("{stamp}-{name}")), text);
        // 오래된 것부터 지운다(이름 = 시각 접두라 사전순 = 시간순).
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            let extra = files.len().saturating_sub(keep);
            for old in files.into_iter().take(extra) {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    /// 확인 띠를 활성 탭의 상태로 맞춘다(띠 높이가 바뀌면 본문을 다시 배치).
    pub(crate) fn ext_banner_sync(&mut self) {
        let id = self.editors.active_id();
        let armed = self.ext_save_armed.is_some_and(|(t, _)| t == id);
        let view = self.ext_files.get(&id).and_then(|info| {
            let follow_btn = info.recent.len() >= 3 && !info.follow;
            if let Some(p) = &info.pending {
                let mut buttons = vec![
                    (extfile::BannerHit::Diff, t(Msg::BtnExtDiff).to_string()),
                    (extfile::BannerHit::Reload, t(Msg::BtnExtReload).to_string()),
                    (extfile::BannerHit::Keep, t(Msg::BtnExtKeep).to_string()),
                ];
                if follow_btn {
                    buttons.push((extfile::BannerHit::Follow, t(Msg::BtnExtFollow).to_string()));
                }
                Some(extfile::BannerView {
                    text: if p.conflicts > 0 {
                        tf(Msg::ExtBanConflict, &[&p.conflicts.to_string()])
                    } else {
                        t(Msg::ExtBanChanged).to_string()
                    },
                    buttons,
                    danger: false,
                })
            } else if info.deleted {
                Some(extfile::BannerView {
                    text: t(Msg::ExtBanDeleted).to_string(),
                    buttons: vec![(
                        extfile::BannerHit::Dismiss,
                        t(Msg::BtnExtDismiss).to_string(),
                    )],
                    danger: true,
                })
            } else if armed {
                Some(extfile::BannerView {
                    text: t(Msg::ExtBanSave).to_string(),
                    buttons: vec![
                        (extfile::BannerHit::Diff, t(Msg::BtnExtDiff).to_string()),
                        (
                            extfile::BannerHit::Overwrite,
                            t(Msg::BtnExtOverwrite).to_string(),
                        ),
                    ],
                    danger: true,
                })
            } else {
                None
            }
        });
        let changed = self.ext_banner.set(view);
        let inset = px(self.ext_banner.height(), self.scale);
        if self.editors.set_top_inset(inset) || changed {
            self.layout();
            self.redraw();
        }
    }

    pub(crate) fn ext_banner_pick(&mut self, hit: extfile::BannerHit) {
        let id = self.editors.active_id();
        let Some(i) = self.editors.index_of_id(id) else {
            return;
        };
        let name = self.editors.active_title();
        match hit {
            extfile::BannerHit::None | extfile::BannerHit::Body => {}
            extfile::BannerHit::Diff => {
                // 1차 = 디스크 내용을 읽기용 안내 탭으로(원래 파일의 구문 · No connection) — 좌우 비교 뷰는 docs/19가 들어오면.
                let text = self
                    .ext_files
                    .get(&id)
                    .and_then(|e| e.pending.as_ref().map(|p| p.text.clone()))
                    .or_else(|| {
                        let path = self.editors.active_path()?;
                        let bytes = std::fs::read(path).ok()?;
                        let (text, _, _) =
                            Self::decode_bytes(&bytes, &self.editors.active_encoding());
                        Some(eol::detect(&text).1)
                    });
                if let Some(text) = text {
                    let title = tf(Msg::ExtDiskTitle, &[&name]);
                    let created = self.editors.open_info_tab_like(&title, &text, Some(&name));
                    if created {
                        let tab = self.editors.active_id();
                        self.make_unconnected(tab);
                    }
                    self.set_focus(Focus::Editor);
                }
            }
            extfile::BannerHit::Reload => {
                let pending = self.ext_files.get_mut(&id).and_then(|e| e.pending.take());
                if let Some(p) = pending {
                    self.editors.apply_external(
                        i,
                        Some(&p.text),
                        &p.text,
                        Some(p.eol),
                        Some(&p.enc),
                    );
                    if let Some(e) = self.ext_files.get_mut(&id) {
                        e.sig = Some(p.sig);
                        e.ignored = None;
                        e.diverged = false;
                    }
                    self.sess.status = tf(Msg::StExtReloaded, &[&name]);
                }
            }
            extfile::BannerHit::Keep | extfile::BannerHit::Follow => {
                if let Some(e) = self.ext_files.get_mut(&id) {
                    if let Some(p) = e.pending.take() {
                        e.ignored = Some(p.sig);
                        e.diverged = true;
                    }
                    if hit == extfile::BannerHit::Follow {
                        e.follow = true;
                    }
                }
                self.sess.status = tf(
                    if hit == extfile::BannerHit::Follow {
                        Msg::StExtFollowOn
                    } else {
                        Msg::StExtKept
                    },
                    &[&name],
                );
            }
            extfile::BannerHit::Overwrite => {
                if let Some(path) = self.editors.active_path() {
                    self.ext_save_armed = Some((id, Instant::now()));
                    self.save_to(&path);
                }
            }
            extfile::BannerHit::Dismiss => {
                // 삭제 안내를 닫는다 — 상태(`deleted`)는 남겨 같은 삭제로 다시 띄우지 않는다.
                if let Some(e) = self.ext_files.get_mut(&id) {
                    e.deleted = false;
                    e.sig = None;
                }
            }
        }
        self.ext_banner_sync();
    }

    /// 팔레트 "외부 변경 조용히 따라가기(이 탭)".
    pub(crate) fn ext_toggle_follow(&mut self) {
        let id = self.editors.active_id();
        if self.editors.active_path().is_none() {
            return;
        }
        let name = self.editors.active_title();
        let e = self.ext_files.entry(id).or_default();
        e.follow = !e.follow;
        self.sess.status = tf(
            if e.follow {
                Msg::StExtFollowOn
            } else {
                Msg::StExtFollowOff
            },
            &[&name],
        );
        self.ext_poll_next = None;
        self.ext_banner_sync();
        self.redraw();
    }

    /// **저장 직전 확인**(동기 · stat 1 + 달라졌을 때만 읽기): true = 저장해도 된다. 디스크가 기준과 다르고 버퍼와도 다르면
    /// 첫 저장은 막고 띠 + 상태줄로 알린다 — 3초 안에 다시 저장하면(또는 띠의 [덮어쓰기]) 덮어쓴다(앱의 2단 확인 관례).
    pub(crate) fn ext_save_guard(&mut self, path: &Path) -> bool {
        let id = self.editors.active_id();
        // 다른 이름으로 저장 = 이 탭의 파일이 아니다(덮어쓰기 확인은 파일 대화상자의 몫).
        if self.editors.active_path().as_deref() != Some(path) {
            return true;
        }
        let armed = self
            .ext_save_armed
            .is_some_and(|(t, at)| t == id && at.elapsed() <= Duration::from_secs(3));
        if armed {
            return true;
        }
        let info = self.ext_files.get(&id).cloned().unwrap_or_default();
        let now_sig = nexa_fs::watch::file_sig(path);
        let mut differs = info.diverged || info.pending.is_some();
        if !differs && now_sig != info.sig && now_sig.is_some() {
            // 서명이 다르다 → 내용으로 확정(내용이 기준이나 버퍼와 같으면 통과 — VS Code와 같은 규칙).
            if let (Ok(bytes), Some(i)) = (std::fs::read(path), self.editors.index_of_id(id)) {
                if let Some((buf, base, enc, _)) = self.editors.snapshot(i) {
                    let (text, _, _) = Self::decode_bytes(&bytes, enc);
                    let disk = eol::detect(&text).1;
                    differs = disk != base && disk != buf;
                }
            }
        }
        if !differs {
            return true;
        }
        self.ext_save_armed = Some((id, Instant::now()));
        let name = self.editors.active_title();
        self.sess.status = tf(Msg::StExtSaveBlocked, &[&name]);
        self.ext_banner_sync();
        self.redraw();
        false
    }
}
