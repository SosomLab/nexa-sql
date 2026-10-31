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
            // ★ 기본 켜진 내장(D-368 · Insert Sequence · 148차)은 관리자가 꺼져 있어도 산다(핵심 편집 기능 · 끄려면 `extensions.disabled`).
            let default_on = self.extensions.default_on_ids();
            let mut off: Vec<String> = self
                .extensions
                .ids()
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| !default_on.iter().any(|(d, _)| d == id))
                .collect();
            for id in self.ext_disabled_list() {
                if !off.contains(&id) {
                    off.push(id);
                }
            }
            return off;
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
        let mut want: Vec<(String, PathBuf)> = extensions::manager::root_dir()
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
        // ★ 개발 폴더(146차 · `extensions.dev_dir`): `*.wasm`을 설치 없이 로드 — 같은 id의 설치본은 개발 판이 대체(목록에서 뒤가 이긴다).
        let dev_ids = self.ext_dev_modules();
        for (id, p) in &dev_ids {
            want.retain(|(i, _)| i != id);
            want.push((id.clone(), p.clone()));
        }
        for note in self.extensions.sync_wasm(&want) {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
        // ★ 길 A(154차): 상태 있는 확장의 전용 상태 파일 뿌리 = 확장 설치 폴더(`<root>/<id>/state.json`).
        self.extensions
            .set_storage_root(extensions::manager::root_dir());
        let mut disabled = self.ext_disabled();
        // ★ 기본 켜진 내장(D-368 · Insert Sequence · 148차)은 설치 기록 없이도 산다 — 끄기만 가능.
        let default_on = self.extensions.default_on_ids();
        for (id, _) in self.extensions.ids() {
            let is_dev = dev_ids.iter().any(|(d, _)| *d == id);
            let is_default_on = default_on.iter().any(|(d, _)| *d == id);
            if !installed.iter().any(|r| r.id == id)
                && !is_dev
                && !is_default_on
                && !disabled.contains(&id)
            {
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
        // ★ 확장 메타 `settings` → 런타임 레지스트리(146차 · T-258): 분류 = 그 확장의 분류(없으면 이름 분류) · 라벨·설명 = 동적 글.
        for (id, name, defs) in self.extensions.settings_defs_all() {
            let cat = match nsql_settings::extension_categories()
                .into_iter()
                .find(|(_, i)| *i == id)
            {
                Some((c, _)) => c,
                None => {
                    let c = nsql_i18n::dyn_msg(&name, &name);
                    nsql_settings::register_extension_category(c, &id);
                    if !extra_cats.contains(&c) {
                        extra_cats.push(c);
                    }
                    c
                }
            };
            if !nsql_settings::CATEGORY_TREE
                .iter()
                .any(|(_, cats)| cats.contains(&cat))
                && !extra_cats.contains(&cat)
            {
                extra_cats.push(cat);
            }
            for d in defs {
                let (en, ko) = label_pair(&d.label);
                let (den, dko) = label_pair(&d.desc);
                let kind = match &d.kind {
                    extensions::SettingDefKind::Bool => nsql_settings::SettingKind::Bool,
                    extensions::SettingDefKind::Text => nsql_settings::SettingKind::Text,
                    extensions::SettingDefKind::Int { min, max } => {
                        nsql_settings::SettingKind::Int {
                            min: *min,
                            max: *max,
                        }
                    }
                    extensions::SettingDefKind::Choice(items) => {
                        let v: Vec<(&'static str, Msg)> = items
                            .iter()
                            .map(|(val, l)| {
                                let (e, k) = label_pair(l);
                                (nsql_settings::intern_key(val), nsql_i18n::dyn_msg(&e, &k))
                            })
                            .collect();
                        nsql_settings::SettingKind::Choice(Box::leak(v.into_boxed_slice()))
                    }
                };
                dyn_entries.push(nsql_settings::Entry {
                    key: nsql_settings::intern_key(&d.key),
                    cat,
                    label: nsql_i18n::dyn_msg(&en, &ko),
                    desc: nsql_i18n::dyn_msg(&den, &dko),
                    kind,
                    default: nsql_settings::intern_key(&d.default),
                });
            }
        }
        nsql_settings::register_dynamic(dyn_entries);
        // ★ 결함 A(148차 V1 · 협업 발견): 파일에서 읽을 때 미등록이라 `unknown`에 머물던 동적 키 값을 등재 뒤 승격(재기동 때 값 유실 방지).
        for k in self.settings.adopt_unknown() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!("settings: adopted {k} from file (dynamic key registered by an extension)"),
            ));
        }
        // ★ 확장 명령의 기본 단축키(146차 · docs/75 §9 ②) → 키맵 동적 표 + 재조립 · ★ 149차(사용자 10-28 "단축키도 확장 영역의 설정으로"):
        //   명령마다 `key.<명령 id>` 설정을 그 확장 분류에 동적 등재 → 설정 창(Preferences ▸ Extensions ▸ <확장>)에서 바꿀 수 있고
        //   `Keymap::from_settings`가 등록된 키만 읽으므로 사용자 지정이 먹는다(종전 "동적 명령은 기본 코드만").
        {
            let off = self.ext_disabled();
            let cmds: Vec<(String, String, extensions::Command)> = self
                .extensions
                .commands_all(&off)
                .into_iter()
                .filter(|(_, _, c)| c.key.is_some() || c.key_mac.is_some())
                .collect();
            let dynamic: Vec<(&'static str, String, String, String)> = cmds
                .iter()
                .map(|(_, _, c)| {
                    let win = c.key.clone().unwrap_or_default();
                    let mac = c
                        .key_mac
                        .clone()
                        .unwrap_or_else(|| win.replace("ctrl+", "cmd+"));
                    (nsql_settings::intern_key(&c.id), win.clone(), mac, win)
                })
                .collect();
            let mut key_entries = Vec::new();
            for (id, name, c) in &cmds {
                let cat = ext_category(id, name, &mut extra_cats);
                let (en, ko) = label_pair(&c.label);
                let code = c.key.clone().unwrap_or_default();
                key_entries.push(nsql_settings::Entry {
                    key: nsql_settings::intern_key(&keymap::setting_key(&c.id)),
                    cat,
                    label: nsql_i18n::dyn_msg(&format!("Shortcut: {en}"), &format!("단축키: {ko}")),
                    desc: nsql_i18n::dyn_msg(
                        &format!("Key chord for '{en}' (extension default {code}) - e.g. ctrl+alt+n · empty = default · none = unbound"),
                        &format!("'{ko}' 단축키(확장 기본 {code}) - 예 ctrl+alt+n · 비우면 기본 · none = 없음"),
                    ),
                    kind: nsql_settings::SettingKind::Text,
                    default: "",
                });
            }
            nsql_settings::register_dynamic(key_entries);
            for k in self.settings.adopt_unknown() {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    format!("settings: adopted {k} from file (extension shortcut key)"),
                ));
            }
            self.prefs_win.set_extra_categories(extra_cats.clone());
            keymap::set_dynamic(dynamic);
            self.keymap = Keymap::from_settings(&self.settings);
            // 메뉴 표시 단축키도 다시(결함 C · 동적 명령의 단축키 글이 메뉴에 없던 것).
            self.sync_menu_shortcuts();
        }
        let list_off = self.ext_disabled_list();
        let mut mirror_dirty = false;
        for r in &installed {
            let key = nsql_settings::ext_use_key(&r.id);
            let v = if list_off.contains(&r.id) {
                "off"
            } else {
                "on"
            };
            if self.settings.get(key) != Some(v) {
                let _ = self.settings.set(key, v);
                mirror_dirty = true;
            }
        }
        // ★ 거울 키가 바뀌었으면 파일에도(148차 V1-b 관찰 · 협업): 종전엔 메모리만 고쳐 `ext.<id>.use=off` 줄이 파일에 남았고, 재기동 때
        //   `adopt_unknown`이 그 줄을 올려 `extensions.disabled`와 어긋난 값이 보였다(설정 창 "사용" 칸).
        if mirror_dirty {
            let _ = self.settings.save();
        }
        self.prefs_win.set_extra_categories(extra_cats.clone());
        // ★ 꺼졌거나 내려간 확장의 설정 덮어쓰기는 통째로 걷는다(148차 · 자동 원복) — 걷힌 키는 즉시 재적용.
        {
            let live: Vec<String> = self
                .extensions
                .ids()
                .into_iter()
                .map(|(id, _)| id)
                .filter(|id| !disabled.contains(id))
                .collect();
            for owner in self.settings.overlay_owners() {
                if live.contains(&owner) {
                    continue;
                }
                let keys = self.settings.clear_overlay(&owner);
                if !keys.is_empty() {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        format!("[{owner}] setting overlay cleared: {}", keys.join(", ")),
                    ));
                    for k in keys {
                        self.apply_setting(&k);
                    }
                }
            }
        }
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
        self.ext_apply_setting_sets();
        self.refresh_menu_extras();
        // ★ 프로젝트 탐색기 행 우클릭 메뉴의 확장 항목(146차 · target = project_explorer).
        {
            let off = self.ext_disabled();
            let km = &self.keymap;
            let items = self.extensions.menu_extras(
                &off,
                &|id| km.display_of(id),
                extensions::MenuTarget::ProjectExplorer,
            );
            self.project_panel.set_menu_extras(items);
        }
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
        // ★ 호스트 상황 = 활성 문서의 파일 경로(docs/97 · 확장이 `Editor::doc_path()`로 읽는다).
        let doc = self
            .editors
            .active_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.run_extension_cmd_for(id, &doc);
    }

    /// ★ 확장 명령 실행 + 상황 경로 지정(프로젝트 탐색기 행 · 146차) + 파일 동작 요청 적용.
    pub(crate) fn run_extension_cmd_for(&mut self, id: &str, doc: &str) {
        // ★ 길 A(154차): 유휴 보존 인스턴스 회수(`extensions.stateful_idle_secs` · 0 = 안 함).
        let idle = self.settings.int("extensions.stateful_idle_secs");
        if idle > 0 {
            for note in self
                .extensions
                .reap_idle(std::time::Duration::from_secs(idle as u64))
            {
                self.log_win.push(LogEntry::new(LogKind::Info, note));
            }
        }
        // ★ 관찰 ①(154차 V1-k · 협업): editor.read/write를 선언한 확장의 명령은 **편집기 탭에서만** — 상세 탭 같은 뷰 탭이면 안내하고 끝
        //   (종전엔 조용히 실행돼 편집 요청만 버려지고 카운터는 올랐다).
        if self.editors.is_view_tab_active() {
            let needs_editor = self
                .extensions
                .owner_id_of(id)
                .and_then(|o| self.extensions.capabilities_of(&o))
                .is_some_and(|c| c.editor_read || c.editor_write);
            if needs_editor {
                self.sess.status = tf(Msg::StExtNeedsEditor, &[id]);
                self.redraw();
                return;
            }
        }
        let disabled = self.ext_disabled();
        if self
            .extensions
            .run_with(id, &disabled, self.editors.cur_mut(), doc, &self.settings)
        {
            self.redraw();
        }
        // WASM 확장의 `nx_log` 줄·오류를 로그 창에(docs/75 §6).
        for note in self.extensions.take_wasm_notes() {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
        let mut ask: Vec<String> = Vec::new();
        for (ext, op) in self.extensions.drain_file_ops() {
            if !self.ext_use_approved(&ext, "file_ops") {
                self.ext_held
                    .push((ext.clone(), extensions::HeldOp::File(op)));
                if !ask.contains(&ext) {
                    ask.push(ext);
                }
                continue;
            }
            self.ext_file_op(op);
        }
        for id in ask {
            self.ext_request_use(&id);
        }
        self.ext_apply_setting_sets();
        self.ext_apply_edit_ops();
    }

    /// ★ 사건 배달(154차-b · 길 A ⑤): 매 틱 활성 탭·본문 세대·구간·탭 목록을 비교해 구독한 wasm에 `edit`(디바운스 `extensions.event_debounce_ms`) ·
    ///   `caret` · `doc`(open/close/activate)를 보낸다. 마스터 스위치 `extensions.events` · 구독 확장이 없으면 비교만(비용 ≈ 0).
    pub(crate) fn ext_events_tick(&mut self, now: std::time::Instant) {
        if !self.settings.flag("extensions.events") {
            return;
        }
        let disabled = self.ext_disabled();
        let want_edit = self.extensions.wants_event("edit", &disabled);
        let want_caret = self.extensions.wants_event("caret", &disabled);
        let want_doc = self.extensions.wants_event("doc", &disabled);
        if !(want_edit || want_caret || want_doc) {
            return;
        }
        let n = self.editors.len();
        let tabs: Vec<(u64, Option<std::path::PathBuf>)> = (0..n)
            .map(|i| (self.editors.tab_id(i), self.editors.path_of(i)))
            .collect();
        let active = self.editors.tab_id(self.editors.active());
        if want_doc {
            let opened: Vec<(u64, Option<std::path::PathBuf>)> = tabs
                .iter()
                .filter(|(id, _)| !self.ext_ev.tabs.iter().any(|(x, _)| x == id))
                .cloned()
                .collect();
            let closed: Vec<(u64, Option<std::path::PathBuf>)> = self
                .ext_ev
                .tabs
                .iter()
                .filter(|(id, _)| !tabs.iter().any(|(x, _)| x == id))
                .cloned()
                .collect();
            for (id, p) in opened {
                self.ext_dispatch_event("doc", &doc_event_json("open", id, p.as_deref()));
            }
            for (id, p) in closed {
                self.ext_dispatch_event("doc", &doc_event_json("close", id, p.as_deref()));
            }
            if self.ext_ev.active != Some(active) {
                let p = self.editors.path_of(self.editors.active());
                self.ext_dispatch_event("doc", &doc_event_json("activate", active, p.as_deref()));
            }
        }
        let switched = self.ext_ev.active != Some(active);
        self.ext_ev.tabs = tabs;
        self.ext_ev.active = Some(active);
        let rev = self.editors.cur().rev();
        let regions = self.editors.cur().regions();
        if switched {
            // 탭 전환 = 기준만 바꾼다(편집·캐럿 사건 아님).
            self.ext_ev.rev = rev;
            self.ext_ev.regions = regions;
            self.ext_ev.edit_due = None;
            self.ext_ev.pending_rev = None;
            return;
        }
        if want_edit {
            if rev != self.ext_ev.rev {
                self.ext_ev.rev = rev;
                self.ext_ev.pending_rev = Some(rev);
                let ms = self
                    .settings
                    .int("extensions.event_debounce_ms")
                    .clamp(0, 5_000) as u64;
                self.ext_ev.edit_due = Some(now + std::time::Duration::from_millis(ms));
            }
            if let (Some(due), Some(r)) = (self.ext_ev.edit_due, self.ext_ev.pending_rev) {
                if now >= due {
                    self.ext_ev.edit_due = None;
                    self.ext_ev.pending_rev = None;
                    self.ext_dispatch_event(
                        "edit",
                        &format!("{{\"kind\":\"edit\",\"doc\":\"{active}\",\"version\":{r}}}"),
                    );
                }
            }
        } else {
            self.ext_ev.rev = rev;
        }
        if want_caret {
            if regions != self.ext_ev.regions {
                let json = format!(
                    "{{\"kind\":\"caret\",\"doc\":\"{active}\",\"regions\":[{}]}}",
                    regions
                        .iter()
                        .map(|(a, b)| format!("[{a},{b}]"))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                self.ext_ev.regions = regions;
                self.ext_dispatch_event("caret", &json);
            }
        } else {
            self.ext_ev.regions = regions;
        }
    }

    /// 사건 하나를 구독 확장들에 보내고 로그를 거둔다.
    fn ext_dispatch_event(&mut self, kind: &str, json: &str) {
        let disabled = self.ext_disabled();
        if self.extensions.dispatch_event(kind, json, &disabled) > 0 {
            for note in self.extensions.take_wasm_notes() {
                self.log_win.push(LogEntry::new(LogKind::Info, note));
            }
        }
    }

    /// ★ 길 A(154차): 확장의 편집 요청 적용 — `editor.write` 첫 사용은 D-90 확인(요청은 버리고 허용 뒤 명령 재실행) · 검증·한 되돌리기 묶음은
    ///   `EditorOps::apply_edits`(활성 탭) · 상태줄 요청은 그대로 상태줄에.
    pub(crate) fn ext_apply_edit_ops(&mut self) {
        let mut ask: Vec<String> = Vec::new();
        for (ext, ops) in self.extensions.drain_edit_ops() {
            if !self.ext_use_approved(&ext, "editor_write") {
                self.ext_held
                    .push((ext.clone(), extensions::HeldOp::Edit(ops.len())));
                if !ask.contains(&ext) {
                    ask.push(ext);
                }
                continue;
            }
            match extensions::EditorOps::apply_edits(self.editors.cur_mut(), &ops) {
                Ok(n) => self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    format!("[{ext}] applied {n} edit(s) as one undo step"),
                )),
                Err(e) => self.log_win.push(LogEntry::new(
                    LogKind::Error,
                    format!("[{ext}] edits refused: {e}"),
                )),
            }
            self.redraw();
        }
        for id in ask {
            self.ext_request_use(&id);
        }
        for (ext, text) in self.extensions.drain_status() {
            self.sess.status = format!("[{ext}] {text}");
            self.redraw();
        }
    }

    /// ★ 확장의 설정 덮어쓰기 요청 적용(148차 · 사용자 10-27 "기본 설정을 바꿀 수 있되 본 프로그램에 영향 X"): 메타 `settings_write` 선언 + 레지스트리
    ///   검증을 지난 것만 **overlay 층**(파일 저장 없음 · 확장이 꺼지면 원복)에 넣고 그 키를 즉시 재적용 · 거절·적용은 로그 한 줄 · 재귀 상한 3.
    pub(crate) fn ext_apply_setting_sets(&mut self) {
        let sets = self.extensions.drain_setting_sets();
        if sets.is_empty() {
            return;
        }
        if self.ext_overlay_depth >= 3 {
            self.log_win.push(LogEntry::new(
                LogKind::Error,
                "extension setting overlay: loop stopped (depth 3)".to_string(),
            ));
            return;
        }
        self.ext_overlay_depth += 1;
        let mut ask: Vec<String> = Vec::new();
        for (ext, key, value) in sets {
            // ★ D-90 ③(152차): 설정 덮어쓰기는 첫 사용 때 사용자 허용이 있어야 — 없으면 보류하고 상세 탭에서 묻는다.
            if !self.ext_use_approved(&ext, "settings_write") {
                self.ext_held
                    .push((ext.clone(), extensions::HeldOp::Set(key, value)));
                if !ask.contains(&ext) {
                    ask.push(ext);
                }
                continue;
            }
            self.ext_apply_one_set(&ext, &key, &value);
        }
        self.ext_overlay_depth -= 1;
        for id in ask {
            self.ext_request_use(&id);
        }
    }

    /// 확장이 요청한 파일 동작(146차 · docs/97 §5 ②): 열기 = 새 탭 · 드러내기 = 프로젝트 탐색기 · 경로 복사 = 클립보드(명시적 명령에서만 온다).
    fn ext_file_op(&mut self, op: extensions::FileOp) {
        match op {
            extensions::FileOp::Open(p) => self.open_file(Path::new(&p)),
            extensions::FileOp::Reveal(p) => self.reveal_in_project(Path::new(&p)),
            extensions::FileOp::CopyPath(p) => self.copy_path_text(Path::new(&p), true),
        }
        self.redraw();
    }

    /// ★ 개발 폴더의 `*.wasm` → `(id, 경로)`(146차 · `extensions.dev_dir`): id는 모듈 메타에서(수정 시각 캐시 · 같은 파일은 다시 읽지 않는다).
    fn ext_dev_modules(&mut self) -> Vec<(String, PathBuf)> {
        let dir = self
            .settings
            .get("extensions.dev_dir")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let Some(dir) = dir else {
            self.ext_dev_ids.clear();
            return Vec::new();
        };
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut paths: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("wasm"))
            })
            .collect();
        paths.sort();
        for p in paths {
            let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
            let cached = self
                .ext_dev_ids
                .get(&p)
                .filter(|(t, _)| *t == mtime)
                .map(|(_, id)| id.clone());
            let id = match cached {
                Some(id) => id,
                None => match extensions::wasm::WasmExtension::load_file(&p) {
                    Ok(e) => {
                        let id = extensions::Extension::id(&e).to_string();
                        self.ext_dev_ids.insert(p.clone(), (mtime, id.clone()));
                        id
                    }
                    Err(e) => {
                        self.log_win.push(LogEntry::new(
                            LogKind::Error,
                            format!("dev extension {}: {e}", nexa_fs::path::display(&p)),
                        ));
                        continue;
                    }
                },
            };
            seen.push(p.clone());
            out.push((id, p));
        }
        self.ext_dev_ids.retain(|p, _| seen.contains(p));
        if !out.is_empty() {
            let names: Vec<String> = out.iter().map(|(id, _)| id.clone()).collect();
            self.sess.status = tf(Msg::StExtDevLoaded, &[&names.join(", ")]);
        }
        out
    }

    /// ★ `requires` 자동 설치(146차): 메타가 요구하는 id 중 미설치 = 마지막으로 읽은 카탈로그에서 찾아 설치(한 단계 · 로그 한 줄) · 없으면 안내.
    fn ext_install_requires(&mut self, meta: &extensions::manager::Meta) {
        use extensions::manager as mgr;
        for req in &meta.requires {
            if mgr::installed().iter().any(|r| &r.id == req) {
                continue;
            }
            let Some(n) = self.ext_catalog.iter().position(|(_, p)| &p.id == req) else {
                self.log_win.push(LogEntry::new(
                    LogKind::Error,
                    tf(Msg::StExtRequiresMissing, &[&meta.name, req]),
                ));
                continue;
            };
            let (src, sum) = self.ext_catalog[n].clone();
            let mut tr = mgr::Trace::default();
            let r = mgr::install(&src, &sum, &mut tr);
            self.ext_trace(tr);
            match r {
                Ok(m) => self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(
                        Msg::StExtRequiresInstalled,
                        &[&format!("{} {}", m.name, m.version), &meta.name],
                    ),
                )),
                Err(e) => self.log_win.push(LogEntry::new(LogKind::Error, e)),
            }
        }
    }

    /// 카탈로그 `n`번의 행(패널 행과 같은 규칙 · 설치본이 있으면 그 버전·종류).
    fn ext_row_for_catalog(&self, n: usize) -> Option<ExtRow> {
        use extensions::manager as mgr;
        let (src, p) = self.ext_catalog.get(n)?;
        let installed = self.ext_installed_plus_builtin();
        let rec = installed.iter().find(|r| r.id == p.id);
        let off = self.ext_disabled_list();
        Some(ExtRow {
            id: p.id.clone(),
            name: p.name.clone(),
            version: rec.map_or_else(|| p.version.clone(), |r| r.version.clone()),
            kind: rec.map_or_else(
                || p.kind.as_str().to_string(),
                |r| r.kind.as_str().to_string(),
            ),
            summary: p.summary.clone(),
            installed: rec.is_some(),
            enabled: rec.is_some() && !off.iter().any(|d| d == &p.id),
            catalog: Some(n),
            source: src.display(),
            latest: rec.and_then(|r| {
                mgr::version_newer(&p.version, &r.version).then(|| p.version.clone())
            }),
        })
    }

    /// 설치본(또는 기본 켜진 내장) id의 행.
    fn ext_row_for_installed(&self, id: &str) -> Option<ExtRow> {
        use extensions::manager as mgr;
        let r = self
            .ext_installed_plus_builtin()
            .into_iter()
            .find(|r| r.id == id)?;
        let cat = self.ext_catalog.iter().position(|(_, p)| p.id == r.id);
        let off = self.ext_disabled_list();
        Some(ExtRow {
            latest: cat.and_then(|n| {
                let v = &self.ext_catalog[n].1.version;
                mgr::version_newer(v, &r.version).then(|| v.clone())
            }),
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
        })
    }

    /// 능력 요약 → 상세 탭 칸(T-448 · D-90 승인 창이 같은 칸을 쓴다).
    fn ext_cap_fields(c: &extensions::Capabilities) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut field = |label: Msg, v: String| {
            if !v.is_empty() {
                out.push((t(label).to_string(), v));
            }
        };
        field(Msg::ExtDetCommands, c.commands.join(", "));
        field(Msg::ExtDetPrompts, c.prompts.join(", "));
        field(Msg::ExtDetReads, c.settings_read.join(", "));
        let writes = if c.settings_write.is_empty() && c.setting_set {
            t(Msg::ExtDetWritesUndeclared).to_string()
        } else {
            c.settings_write.join(", ")
        };
        field(Msg::ExtDetWrites, writes);
        if c.file_ops {
            field(Msg::ExtDetFileOps, t(Msg::ExtDetFileOpsList).to_string());
        }
        // ★ 길 A(154차): 인스턴스 유지 · 편집기 접근 · 구독 사건.
        if c.stateful {
            field(Msg::ExtDetStateful, t(Msg::ExtDetYes).to_string());
        }
        let editor = match (c.editor_read, c.editor_write) {
            (true, true) => "read · write",
            (true, false) => "read",
            (false, true) => "write",
            _ => "",
        };
        field(Msg::ExtDetEditor, editor.to_string());
        field(Msg::ExtDetEvents, c.events.join(", "));
        out
    }

    /// 위험 능력(`settings_write` · `file_ops`)의 첫 사용이 허용됐는가 — 내장(설치 기록 없음)은 늘 허용.
    fn ext_use_approved(&self, id: &str, cap: &str) -> bool {
        match extensions::manager::installed()
            .into_iter()
            .find(|r| r.id == id)
        {
            None => true,
            Some(r) => r.approved.iter().any(|c| c == cap),
        }
    }

    /// ★ 첫 사용 확인 요청(D-90 ③ · 152차): 보류된 요청을 상세 탭에 보여 주고 [허용][거부]를 기다린다(이미 떠 있으면 줄만 갱신).
    fn ext_request_use(&mut self, id: &str) {
        let Some(row) = self.ext_row_for_installed(id) else {
            return;
        };
        // ★ 관찰 ②(154차 V1-k): 요청이 나온 편집기 탭을 기억해 허용/거부 뒤 그 탭으로 돌아간다(상세 탭에서 다시 실행하면 ①에 걸린다).
        if !self.editors.is_view_tab_active() {
            self.ext_return_tab = Some(self.editors.tab_id(self.editors.active()));
        }
        let key = format!("ext:{id}");
        let already = self
            .ext_details
            .get(&key)
            .is_some_and(|d| matches!(&d.pending, Some(p) if matches!(&p.kind, ext_view::PendingKind::FirstUse(x) if x == id)));
        if !already {
            self.ext_open_detail(&row);
        }
        let held: Vec<String> = self
            .ext_held
            .iter()
            .filter(|(e, _)| e == id)
            .map(|(_, op)| match op {
                extensions::HeldOp::Set(k, v) => format!("{} = {v}", k),
                extensions::HeldOp::File(extensions::FileOp::Open(p)) => format!("open {p}"),
                extensions::HeldOp::File(extensions::FileOp::Reveal(p)) => format!("reveal {p}"),
                extensions::HeldOp::File(extensions::FileOp::CopyPath(p)) => {
                    format!("copy path {p}")
                }
                extensions::HeldOp::Edit(n) => format!(
                    "edit the document ({n} change(s)) - run the command again after allowing"
                ),
            })
            .collect();
        if let Some(d) = self.ext_details.get_mut(&key) {
            d.pending = Some(ext_view::Pending {
                kind: ext_view::PendingKind::FirstUse(id.to_string()),
                title: t(Msg::ExtApproveTitleUse).to_string(),
            });
            d.fields.retain(|(l, _)| l != t(Msg::ExtDetHeldOps));
            d.fields
                .insert(0, (t(Msg::ExtDetHeldOps).to_string(), held.join(" · ")));
        }
        self.sess.status = tf(Msg::StExtApprovalPending, &[&row.name, &row.version]);
        self.redraw();
    }

    /// 첫 사용 요청이 나온 편집기 탭으로 돌아간다(관찰 ② · 154차) — 기억한 탭이 아직 있으면.
    fn ext_return_to_editor(&mut self) {
        if let Some(t) = self.ext_return_tab.take() {
            if self.editors.index_of_id(t).is_some() {
                self.editors.switch_to_id(t);
                self.set_focus(Focus::Editor);
                self.redraw();
            }
        }
    }

    /// 설정 덮어쓰기 요청 하나 적용(선언 · 검증 · overlay · 로그 · 재적용).
    fn ext_apply_one_set(&mut self, ext: &str, key: &str, value: &str) {
        if !self.extensions.writable(ext, key) {
            self.log_win.push(LogEntry::new(
                LogKind::Error,
                format!("[{ext}] setting overlay refused: {key} is not declared in settings_write"),
            ));
            return;
        }
        match self.settings.set_overlay(key, value, ext) {
            Ok(changed) => {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    format!("[{ext}] setting overlay: {key} = {value} (runtime only · restored when the extension is off)"),
                ));
                if changed {
                    self.apply_setting(key);
                }
            }
            Err(e) => self.log_win.push(LogEntry::new(
                LogKind::Error,
                format!("[{ext}] setting overlay refused: {e}"),
            )),
        }
    }

    /// ★ 설치 기록 + 기본 켜진 내장(D-368 · 설치 기록 없음 · 148차 V1 ③): 팔레트 목록(list·remove·enable·disable·update)에도 내장이 보이게.
    pub(crate) fn ext_installed_plus_builtin(&self) -> Vec<extensions::manager::Installed> {
        let mut installed = extensions::manager::installed();
        for (id, name) in self.extensions.default_on_ids() {
            if !installed.iter().any(|r| r.id == id) {
                installed.push(extensions::manager::Installed {
                    id,
                    name,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    kind: extensions::manager::Kind::Builtin,
                    placed: Vec::new(),
                    approved: Vec::new(),
                });
            }
        }
        installed
    }

    /// Extension Manager 팔레트 명령(Sublime Package Control 방식 · docs/50 §10): 텍스트 목록을 만들어 팔레트에 띄우고,
    /// 고르면 `ext.<verb>:<key>`로 다시 들어온다.
    pub(crate) fn ext_command(&mut self, id: &str) {
        use extensions::manager as mgr;
        let disabled = self.ext_disabled();
        let builtin: Vec<(String, String)> = self.extensions.ids();
        let installed = self.ext_installed_plus_builtin();
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
            // ★ 업데이트(146차 · T-118 잔여): 설치본보다 새 버전이 저장소에 있는 것만 · "모두 업데이트"는 차례로 설치.
            "ext.update" => {
                self.ext_catalog.clear();
                for src in mgr::sources(&self.settings) {
                    let mut tr = mgr::Trace::default();
                    let r = mgr::fetch_index_traced(&src, &mut tr);
                    self.ext_trace(tr);
                    match r {
                        Ok(idx) => {
                            for p in idx.packages {
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
                let mut count = 0;
                for (n, (src, p)) in self.ext_catalog.iter().enumerate() {
                    if let Some(r) = installed.iter().find(|r| r.id == p.id) {
                        if mgr::version_newer(&p.version, &r.version) {
                            count += 1;
                            cmds.push((
                                format!("ext.install:{n}"),
                                format!(
                                    "{} {} → {} - {} · {}",
                                    p.name,
                                    r.version,
                                    p.version,
                                    p.summary,
                                    src.display()
                                ),
                            ));
                        }
                    }
                }
                // ★ 기본 켜진 내장 → 저장소의 wasm 판(148차 V1 ②): 버전 비교가 아니라 "내장을 wasm으로" 교체 대상.
                for (id, name) in self.extensions.default_on_ids() {
                    if mgr::installed().iter().any(|r| r.id == id) {
                        continue;
                    }
                    if let Some((n, (src, p))) = self
                        .ext_catalog
                        .iter()
                        .enumerate()
                        .find(|(_, (_, p))| p.id == id)
                    {
                        count += 1;
                        cmds.push((
                            format!("ext.install:{n}"),
                            format!(
                                "{} builtin → {} (wasm) - {} · {}",
                                name,
                                p.version,
                                p.summary,
                                src.display()
                            ),
                        ));
                    }
                }
                if count > 1 {
                    cmds.insert(
                        0,
                        (
                            "ext.update_all:".into(),
                            tf(Msg::ExtUpdateAll, &[&count.to_string()]),
                        ),
                    );
                }
                if cmds.is_empty() {
                    cmds.push(("ext.noop".into(), t(Msg::StExtUpToDate).into()));
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
                let n: usize = key.parse().unwrap_or(usize::MAX);
                let mut tr = mgr::Trace::default();
                let r = mgr::stage(&src, &sum, &mut tr);
                self.ext_trace(tr);
                match r {
                    Ok(staged) => {
                        // ★ D-90 ③(152차): 내려받기·검증까지 하고 **배치 전에** 능력을 보여 승인받는다(상세 탭 승인 상태 · [설치][취소]).
                        let mut lines: Vec<(String, String)> = Vec::new();
                        if let Some(bytes) = staged.wasm_bytes() {
                            match extensions::wasm::WasmExtension::load(bytes) {
                                Ok(ext) => {
                                    lines = Self::ext_cap_fields(
                                        &extensions::Extension::capabilities(&ext),
                                    )
                                }
                                Err(e) => lines
                                    .push((t(Msg::ExtDetState).to_string(), format!("wasm: {e}"))),
                            }
                        }
                        let placed = staged.placed_files();
                        if !placed.is_empty() {
                            lines.push((t(Msg::ExtDetFiles).to_string(), placed.join(", ")));
                        }
                        self.ext_staged.retain(|(i, _)| *i != n);
                        self.ext_staged.push((n, staged));
                        let Some(row) = self.ext_row_for_catalog(n) else {
                            return;
                        };
                        self.ext_open_detail(&row);
                        if let Some(d) = self.ext_details.get_mut(&format!("ext:{}", row.id)) {
                            d.pending = Some(ext_view::Pending {
                                kind: ext_view::PendingKind::Install(n),
                                title: t(Msg::ExtApproveTitleInstall).to_string(),
                            });
                            let mut fields = lines;
                            fields.append(&mut d.fields);
                            d.fields = fields;
                        }
                        self.sess.status =
                            tf(Msg::StExtApprovalPending, &[&row.name, &row.version]);
                        self.redraw();
                    }
                    Err(e) => {
                        self.sess.status = e;
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                    }
                }
            }
            // ★ 승인(D-90 ③ · 152차): 스테이지된 패키지를 배치·기록 — 종전 설치 성공 경로.
            "approve" => {
                let n: usize = key.parse().unwrap_or(usize::MAX);
                let Some(pos) = self.ext_staged.iter().position(|(i, _)| *i == n) else {
                    return;
                };
                let (_, staged) = self.ext_staged.remove(pos);
                let mut tr = mgr::Trace::default();
                let r = mgr::commit_staged(staged, &mut tr);
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
                        // ★ `requires` 자동 설치(146차 · T-118 잔여): 카탈로그에 있는 것만 · 한 단계 · 이미 설치됐으면 건너뜀.
                        self.ext_install_requires(&meta);
                        self.apply_extensions(None);
                        if let Some(row) = self.ext_row_for_installed(&meta.id) {
                            self.ext_details.remove(&format!("ext:{}", row.id));
                            self.ext_open_detail(&row);
                        }
                    }
                    Err(e) => {
                        self.sess.status = e;
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                    }
                }
            }
            "reject" => {
                let n: usize = key.parse().unwrap_or(usize::MAX);
                self.ext_staged.retain(|(i, _)| *i != n);
                if let Some(row) = self.ext_row_for_catalog(n) {
                    self.sess.status = tf(Msg::StExtRejected, &[&row.name]);
                    self.ext_details.remove(&format!("ext:{}", row.id));
                    self.ext_open_detail(&row);
                }
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            }
            // ★ 첫 사용 허용/거부(D-90 ③): 보류된 요청을 적용하거나 버린다 · 허용은 설치 기록에 남는다(새 버전 = 다시 확인).
            "allow_use" => {
                let held: Vec<extensions::HeldOp> = {
                    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ext_held)
                        .into_iter()
                        .partition(|(e, _)| e == key);
                    self.ext_held = rest;
                    mine.into_iter().map(|(_, op)| op).collect()
                };
                if held
                    .iter()
                    .any(|h| matches!(h, extensions::HeldOp::Set(..)))
                {
                    if let Err(e) = mgr::approve_use(key, "settings_write") {
                        self.log_win.push(LogEntry::new(LogKind::Error, e));
                    }
                }
                if held
                    .iter()
                    .any(|h| matches!(h, extensions::HeldOp::File(_)))
                {
                    if let Err(e) = mgr::approve_use(key, "file_ops") {
                        self.log_win.push(LogEntry::new(LogKind::Error, e));
                    }
                }
                let rerun = held
                    .iter()
                    .any(|h| matches!(h, extensions::HeldOp::Edit(_)));
                if rerun {
                    if let Err(e) = mgr::approve_use(key, "editor_write") {
                        self.log_win.push(LogEntry::new(LogKind::Error, e));
                    }
                }
                let id = key.to_string();
                for h in held {
                    match h {
                        extensions::HeldOp::Set(k, v) => self.ext_apply_one_set(&id, &k, &v),
                        extensions::HeldOp::File(op) => self.ext_file_op(op),
                        extensions::HeldOp::Edit(_) => {}
                    }
                }
                self.sess.status = tf(
                    if rerun {
                        Msg::StExtUseAllowedRerun
                    } else {
                        Msg::StExtUseAllowed
                    },
                    &[&id],
                );
                if let Some(row) = self.ext_row_for_installed(&id) {
                    self.ext_details.remove(&format!("ext:{}", row.id));
                    self.ext_open_detail(&row);
                }
                self.ext_return_to_editor();
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            }
            "deny_use" => {
                self.ext_held.retain(|(e, _)| e != key);
                self.sess.status = tf(Msg::StExtUseDenied, &[key]);
                if let Some(row) = self.ext_row_for_installed(key) {
                    self.ext_details.remove(&format!("ext:{}", row.id));
                    self.ext_open_detail(&row);
                }
                self.ext_return_to_editor();
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            }
            // ★ 모두 업데이트(146차): 설치본보다 새 버전이 있는 카탈로그 항목을 차례로 설치(각각 `install:n`과 같은 길).
            "update_all" => {
                let targets: Vec<usize> = {
                    let installed = mgr::installed();
                    let default_on = self.extensions.default_on_ids();
                    self.ext_catalog
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, p))| {
                            installed
                                .iter()
                                .any(|r| r.id == p.id && mgr::version_newer(&p.version, &r.version))
                                || (!installed.iter().any(|r| r.id == p.id)
                                    && default_on.iter().any(|(d, _)| *d == p.id))
                        })
                        .map(|(n, _)| n)
                        .collect()
                };
                for n in targets {
                    self.ext_pick(&format!("ext.install:{n}"));
                }
                return;
            }
            "remove" => {
                // ★ 기본 켜진 내장(설치 기록 없음 · D-368)의 삭제 = 끄기(09-17 규칙 "삭제 = 끄기" · 지울 파일이 없다).
                if !mgr::installed().iter().any(|r| r.id == key)
                    && self
                        .extensions
                        .default_on_ids()
                        .iter()
                        .any(|(d, _)| d == key)
                {
                    self.ext_pick(&format!("ext.disable:{key}"));
                    return;
                }
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
                if let Some(r) = self
                    .ext_installed_plus_builtin()
                    .into_iter()
                    .find(|r| r.id == key)
                {
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
        let default_on = self.extensions.default_on_ids();
        for d in self.ext_details.values_mut() {
            let rec = installed.iter().find(|r| r.id == d.row.id);
            // 기본 켜진 내장(설치 기록 없음 · D-368) = 설치됨 · 카탈로그의 wasm 판은 늘 "업데이트" 대상.
            let def_on = rec.is_none() && default_on.iter().any(|(x, _)| *x == d.row.id);
            let inst = rec.is_some() || def_on;
            d.row.installed = inst;
            d.row.enabled = inst && !off.iter().any(|x| x == &d.row.id);
            d.row.catalog = self.ext_catalog.iter().position(|(_, p)| p.id == d.row.id);
            // ★ 버전·업데이트 가능도 지금 값으로(147차 · 사용자 10-27 "업데이트 뒤에도 열린 상세 탭에 업데이트 버튼이 남는다"):
            //   설치본 버전 = 기록 · latest = 카탈로그가 더 새 버전을 알 때만.
            if let Some(r) = rec {
                d.row.version = r.version.clone();
                d.row.kind = r.kind.as_str().to_string();
            } else if def_on {
                d.row.version = env!("CARGO_PKG_VERSION").to_string();
                d.row.kind = "builtin".into();
            }
            d.row.latest = d.row.catalog.and_then(|n| {
                let v = &self.ext_catalog[n].1.version;
                (def_on || (inst && extensions::manager::version_newer(v, &d.row.version))).then(
                    || {
                        if def_on {
                            format!("{v} (wasm)")
                        } else {
                            v.clone()
                        }
                    },
                )
            });
            let state = t(if !inst {
                Msg::ExtStNotInstalled
            } else if d.row.enabled {
                Msg::ExtStEnabled
            } else {
                Msg::ExtStDisabled
            });
            // 상태 칸은 라벨로 찾아 쓴다(결함 E · 153차 V1-j: 승인 대기 땐 능력 줄이 앞에 붙어 첫 칸이 "명령"이었다).
            let state_label = t(Msg::ExtDetState);
            if let Some(f) = d.fields.iter_mut().find(|(l, _)| l == state_label) {
                f.1 = state.to_string();
            }
            let ver = match (&d.row.latest, inst, d.row.catalog) {
                (Some(v), _, _) => {
                    format!("{} · {}", d.row.version, tf(Msg::ExtStUpdateAvail, &[v]))
                }
                (None, true, Some(_)) => format!("{} · {}", d.row.version, t(Msg::ExtStUpToDate)),
                _ => d.row.version.clone(),
            };
            let ver_label = t(Msg::ExtDetVersion);
            if let Some(f) = d.fields.iter_mut().find(|(l, _)| l == ver_label) {
                f.1 = ver;
            }
            // 종류 칸도(148차 V1 ① · wasm 설치 뒤 builtin으로 남던 것).
            let kind_label = t(Msg::ExtDetKind);
            if let Some(f) = d.fields.iter_mut().find(|(l, _)| l == kind_label) {
                f.1 = d.row.kind.clone();
            }
        }
    }

    /// 확장 상세 뷰가 낸 동작 — 확장 패널과 같은 경로.
    /// 활성 탭이 **확장 뷰로 그려지는** 뷰 탭인가(`ext_details`에 상세가 있는 키 = paint의 `Some(detail)` 가지와 같은 판정).
    /// 상세 없는 뷰 탭(비교 inline `compare:*` · 서버 DDL `compare-server:*` · 정보 탭)은 읽기 전용 **편집기**로 그려지므로 입력도
    /// 편집기로 — 종전엔 모든 뷰 탭의 마우스·키를 `ext_view`로 보내 본문 우클릭 메뉴(복사·전체 선택)가 열리지 않았다(협업 V1 bin112 ④).
    pub(crate) fn ext_view_active(&self) -> bool {
        self.editors
            .active_view()
            .is_some_and(|k| self.ext_details.contains_key(k))
    }

    pub(crate) fn ext_view_actions(&mut self) {
        for a in self.ext_view.take_actions() {
            match a {
                ext_view::ExtViewAction::Settings(id) => self.ext_open_settings(&id),
                ext_view::ExtViewAction::Install(n) | ext_view::ExtViewAction::Update(n) => {
                    self.ext_pick(&format!("ext.install:{n}"))
                }
                ext_view::ExtViewAction::Approve(n) => self.ext_pick(&format!("ext.approve:{n}")),
                ext_view::ExtViewAction::Reject(n) => self.ext_pick(&format!("ext.reject:{n}")),
                ext_view::ExtViewAction::AllowUse(id) => {
                    self.ext_pick(&format!("ext.allow_use:{id}"))
                }
                ext_view::ExtViewAction::DenyUse(id) => {
                    self.ext_pick(&format!("ext.deny_use:{id}"))
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
        // ★ 설치 기록 없이 기본 켜진 내장 확장(D-368 · 148차) = 설치됨 행(builtin · 앱 버전) · 카탈로그에 같은 id의 wasm 패키지가 있으면
        //   그 요약 + "업데이트 → wasm 판"(설치 = 내장 대체 · Rainbow Pairs 방식).
        for (id, name) in self.extensions.default_on_ids() {
            if rows.iter().any(|r| r.id == id) {
                continue;
            }
            let cat = self.ext_catalog.iter().find(|(_, p)| p.id == id);
            rows.push(ExtRow {
                latest: cat.map(|(_, p)| format!("{} (wasm)", p.version)),
                id: id.clone(),
                name,
                version: env!("CARGO_PKG_VERSION").to_string(),
                kind: "builtin".into(),
                summary: cat.map(|(_, p)| p.summary.clone()).unwrap_or_default(),
                installed: true,
                enabled: !off.iter().any(|d| d == &id),
                catalog: self.ext_catalog.iter().position(|(_, p)| p.id == id),
                source: cat.map(|(s, _)| s.display()).unwrap_or_default(),
            });
        }
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
        // ⟳ = 저장소를 다시 읽는다 → 설명 메타 캐시도 비운다.
        self.ext_meta_cache.clear();
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
    /// ★ T-374(10-21): 저장소 메타의 **첫 조회는 스레드**(종전 = UI 스레드 동기 curl ≤30 s → 느린 망·오프라인에서 창이 멈춤) — 지금은
    /// 목록 정보 + "읽는 중" 한 줄로 바로 열고, 답이 오면 그 행이 아직 열려 있을 때만 다시 그린다(`ext_meta_poll` · 최신 의도만 · 99).
    fn ext_open_detail(&mut self, row: &ExtRow) {
        use extensions::manager as mgr;
        let mut loading = false;
        let meta = match mgr::installed_meta(&row.id, &row.version) {
            Some(m) => Some(m),
            None => match row.catalog.and_then(|n| self.ext_catalog.get(n)) {
                None => None,
                Some((src, sum)) => {
                    let key = (src.display(), sum.dir.clone());
                    match self.ext_meta_cache.get(&key) {
                        Some(hit) => hit.clone(),
                        None => {
                            let (src, dir) = (src.clone(), sum.dir.clone());
                            loading = true;
                            self.ext_meta_start(key, src, dir, row.clone());
                            None
                        }
                    }
                }
            },
        };
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
            // 내장(builtin)인데 카탈로그 메타(wasm 판)에서 온 파일 목록은 보이지 않는다(151차 V1-h 관찰).
            if row.kind != "builtin" {
                let files: Vec<String> = m.files.iter().map(|f| f.path.clone()).collect();
                field(Msg::ExtDetFiles, &files.join(", "));
            }
            description = m.description.clone();
        }
        // ★ 능력 요약(T-448 · 151차 · 사용자 10-28): 로드된 확장만(카탈로그만 있는 것은 설치 때 승인 창 · D-90).
        if let Some(c) = self.extensions.capabilities_of(&row.id) {
            fields.extend(Self::ext_cap_fields(&c));
        }
        if loading {
            description = t(Msg::ExtDetLoading).to_string();
        }
        let mut shown = row.clone();
        shown.summary = summary;
        let key = format!("ext:{}", row.id);
        // ★ 결함 D(152차 V1-i · 협업): 저장소 메타를 스레드로 읽은 뒤 `ext_meta_poll`이 여기를 다시 불러 항목을 통째로 바꾸며 **승인 상태와
        //   능력 줄이 사라졌다** → 기존 항목에 `pending`이 있으면 그것과 앞에 붙인 줄(새 칸에 없는 라벨)을 옮겨 담는다. 승인·거부가 끝난
        //   길은 항목을 먼저 지우고 다시 연다(`ext_details.remove`).
        let (pending, fields) = match self.ext_details.get(&key) {
            Some(prev) if prev.pending.is_some() => {
                let mut merged: Vec<(String, String)> = prev
                    .fields
                    .iter()
                    .filter(|(l, _)| !fields.iter().any(|(nl, _)| nl == l))
                    .cloned()
                    .collect();
                merged.extend(fields);
                (prev.pending.clone(), merged)
            }
            _ => (None, fields),
        };
        self.ext_details.insert(
            key.clone(),
            ext_view::ExtDetail {
                row: shown,
                fields,
                description,
                pending,
            },
        );
        let created = self
            .editors
            .open_view_tab(&key, &format!("Extension: {}", row.name));
        // ★ 포커스는 옮기지 않는다(사용자 10-20 "직접 클릭하지 않으면 내용으로 이동하지 않게 · 확장 간 키보드 이동") —
        //   지금 포커스를 새 활성 탭에 다시 적용만(뷰 탭 본문 캐럿이 켜지지 않게). 본문은 그 탭을 클릭할 때 포커스.
        self.set_focus(self.focus);
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

    /// ★ T-374: 상세 메타 첫 조회를 스레드로 시작 — 같은 열쇠가 이미 읽는 중이면 기다리는 행만 바꾼다(최신 의도 · 요청은 사용자 동작당
    /// 1회 · 26 §8). 스레드 생성 실패 = 조용히(목록 정보만 보인다).
    fn ext_meta_start(
        &mut self,
        key: (String, String),
        src: extensions::manager::Source,
        dir: String,
        row: ExtRow,
    ) {
        use extensions::manager as mgr;
        let already = self.ext_meta_inflight.contains(&key);
        self.ext_meta_wait = Some((key.clone(), row));
        if already {
            return;
        }
        let tx = self.ext_meta_ch.0.clone();
        let k2 = key.clone();
        let spawned = std::thread::Builder::new()
            .name("ext-meta".into())
            .spawn(move || {
                let mut tr = mgr::Trace::default();
                let got = mgr::fetch_meta(&src, &dir, &mut tr).ok();
                let _ = tx.send((k2, got, tr));
            });
        if spawned.is_ok() {
            self.ext_meta_inflight.push(key);
        } else {
            self.ext_meta_wait = None;
        }
    }

    /// ★ T-374: 답 수거(틱 · 읽는 중일 때만 = 닫혀 있으면 비용 0): 캐시에 넣고(실패도 None = 반복 조회 0 · ⟳가 비운다) · 기다리던 행이
    /// 아직 뷰 탭으로 열려 있으면 다시 그린다(이제 캐시 적중 · 탭을 닫았으면 다시 열지 않는다).
    pub(crate) fn ext_meta_poll(&mut self) {
        if self.ext_meta_inflight.is_empty() {
            return;
        }
        while let Ok((key, got, tr)) = self.ext_meta_ch.1.try_recv() {
            self.ext_meta_inflight.retain(|k| *k != key);
            self.ext_meta_cache.insert(key.clone(), got);
            self.ext_trace(tr);
            let Some((k, row)) = self.ext_meta_wait.take() else {
                continue;
            };
            if k != key {
                self.ext_meta_wait = Some((k, row));
                continue;
            }
            let view_key = format!("ext:{}", row.id);
            let still_open = (0..self.editors.len())
                .any(|i| self.editors.view_key_of(i).as_deref() == Some(view_key.as_str()));
            if still_open {
                self.ext_open_detail(&row);
            }
        }
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
                let Some(i) = self.editors.index_of_id(key) else {
                    return false;
                };
                let info = self.ext_files.entry(key).or_default();
                if info.deleted {
                    return false;
                }
                info.deleted = true;
                info.pending = None;
                // 탭 = 미저장(●) · 진빨강 · 미리보기면 편집기 탭으로 승격(사용자 10-07).
                self.editors.set_deleted(i, true);
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

    pub(crate) fn ext_on_changed(
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
        // 파일이 (다시) 있다 = 삭제 표시 해제(사용자 10-07).
        self.editors.set_deleted(i, false);
        let info = self.ext_files.entry(id).or_default();
        info.deleted = false;
        let suggest = info.note_change(now);
        let follow = info.follow;
        let mut notable = false;
        // ★ T-300(사용자 10-07): 본문에 반영된 경우의 알림 글 — 끝에서 토스트로(설정 `file.external_notify`).
        let mut applied_note: Option<String> = None;
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
                // 수정 없는 탭이 조용히 디스크를 따라간 경우 — 종전엔 아무 표시가 없어 "변경이 자동 반영되는데 메시지가 없다"(사용자 10-07).
                let note = tf(Msg::ToastExtAdopted, &[&name]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, note.clone()));
                // 기법도 함께(사용자 10-07 "단순 로딩·변경 병합 등 사용된 기법").
                applied_note = Some(format!("{note} · {}", t(Msg::ExtMethodAdopt)));
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
                applied_note = Some(format!(
                    "{} · {}",
                    self.sess.status,
                    t(Msg::ExtMethodReload)
                ));
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
                applied_note = Some(format!(
                    "{} · {}",
                    self.sess.status,
                    tf(Msg::ExtMethodMerge, &[&merged.applied.to_string()])
                ));
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
        // ★ T-300: 본문에 반영됐으면 토스트(설정 `file.external_notify` · 상태줄·로그는 위에서 늘).
        if let Some(note) = applied_note.take() {
            if self.settings.flag("file.external_notify") {
                self.toasts.push(
                    toast::ToastKind::Info,
                    t(Msg::ToastExtChanged).to_string(),
                    note,
                );
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
        // ★ T-167 B6 이동 제안 띠(D-163 · 69 §4-1): 외부 변경 띠가 없을 때만 · 이 탭의 제안 하나.
        let view = view.or_else(|| {
            let s = self.bm_suggest.as_ref().filter(|s| s.tab == id)?;
            // 옛 경로는 마지막 두 조각만(`…/폴더/파일.sql` · 협업 27차 관찰 = 긴 경로가 버튼 앞에서 잘림).
            let parts: Vec<&str> = s
                .old_path
                .split(['/', '\\'])
                .filter(|p| !p.is_empty())
                .collect();
            let shown = if parts.len() > 2 {
                format!("…/{}", parts[parts.len() - 2..].join("/"))
            } else {
                s.old_path.clone()
            };
            Some(extfile::BannerView {
                text: tf(Msg::BmBanMove, &[&s.ids.len().to_string(), &shown]),
                buttons: vec![
                    (
                        extfile::BannerHit::BmAttach,
                        t(Msg::BtnBmAttach).to_string(),
                    ),
                    (extfile::BannerHit::BmDismiss, t(Msg::BtnBmNo).to_string()),
                ],
                danger: false,
            })
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
                    // 디스크 사본은 읽을거리 — 읽기 전용(객체 정보 탭과 같음 · 협업 bin61 관찰).
                    let di = self.editors.active();
                    self.editors.set_read_only(di, true);
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
            extfile::BannerHit::BmAttach => {
                if let Some(s) = self.bm_suggest.take() {
                    let n = self.bookmarks.attach_moved(&s.ids, &s.new_path);
                    self.bookmarks.relocate_doc(&self.editors, i);
                    self.bm_sync_ui();
                    self.sess.status = tf(Msg::StBookmarkAttached, &[&n.to_string(), &name]);
                }
            }
            extfile::BannerHit::BmDismiss => {
                self.bm_suggest = None;
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

/// 확장 라벨 → (영어, 한국어) 글(동적 설정 항목의 라벨·설명 · 146차).
/// 문서 사건 JSON(`doc` · open/close/activate · 경로는 JSON 글자 이스케이프).
fn doc_event_json(what: &str, id: u64, path: Option<&std::path::Path>) -> String {
    let p = path
        .map(|p| {
            p.to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        })
        .unwrap_or_default();
    format!("{{\"kind\":\"doc\",\"what\":\"{what}\",\"doc\":\"{id}\",\"path\":\"{p}\"}}")
}

/// 확장의 설정 분류(있으면 그것 · 없으면 확장 이름으로 동적 분류 등재 + `extra_cats`에) — 149차 `key.<id>` 등재가 쓴다.
fn ext_category(id: &str, name: &str, extra_cats: &mut Vec<Msg>) -> Msg {
    let cat = match nsql_settings::extension_categories()
        .into_iter()
        .find(|(_, i)| *i == id)
    {
        Some((c, _)) => c,
        None => {
            let c = nsql_i18n::dyn_msg(name, name);
            nsql_settings::register_extension_category(c, id);
            c
        }
    };
    if !nsql_settings::CATEGORY_TREE
        .iter()
        .any(|(_, cats)| cats.contains(&cat))
        && !extra_cats.contains(&cat)
    {
        extra_cats.push(cat);
    }
    cat
}

fn label_pair(l: &extensions::Label) -> (String, String) {
    match l {
        extensions::Label::Msg(m) => (
            nsql_i18n::tr(nsql_i18n::Lang::En, *m).to_string(),
            nsql_i18n::tr(nsql_i18n::Lang::Ko, *m).to_string(),
        ),
        extensions::Label::Text(en, ko) => (
            en.clone(),
            ko.clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| en.clone()),
        ),
    }
}
