//! App — 변수(내장·DEFINE·변수 창 적용 · docs/63).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 변수 창의 줄 — 지금 편집기 탭의 표(탭 층) + 이 세션의 공유 층. 비밀 값은 가린다 · 긴 값은 앞부분만.
    /// 다음 실행에 넘길 치환 변수(활성 탭 · 이름 · 원문).
    /// 내장 변수 스냅숏(`${workspaceFolder}` · `${file}` · `${config:키}` … · [nsql_script::intrinsic] · 사용자 09-23) — 실행마다
    /// 그 순간의 프로젝트·활성 탭·접속·설정으로 만든다(수십 항목 + 설정 키 · 복사 0 = `Arc`). 설정 `vars.intrinsic` 끔 = 빈 표.
    pub(crate) fn run_intrinsic(
        &self,
    ) -> std::sync::Arc<std::collections::BTreeMap<String, String>> {
        if !self.settings.flag("vars.intrinsic") {
            return std::sync::Arc::default();
        }
        std::sync::Arc::new(nsql_script::intrinsic::build(&self.intrinsic_context()))
    }

    fn intrinsic_context(&self) -> nsql_script::intrinsic::Context {
        let tb = self.editors.cur();
        let caret = tb.caret();
        let line = tb.buf().line_of(caret);
        let col = caret.saturating_sub(tb.buf().line_start(line));
        let leaf = |p: &Path| {
            p.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        nsql_script::intrinsic::Context {
            project_file: self.project.path.clone(),
            workspace_dir: self.arg_folder.clone(),
            folders: self
                .project
                .folders
                .iter()
                .map(|f| (leaf(f), f.clone()))
                .collect(),
            file: self.editors.active_path(),
            // 미저장 탭 = 현재 언어의 기본값 글(사용자 10-07).
            no_file_text: Some(t(Msg::VarUnsavedTab).to_string()),
            line: Some(line + 1),
            column: Some(col + 1),
            user_home: std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from),
            app_home: nsql_settings::config_dir(),
            exec: std::env::current_exe().ok(),
            cwd: std::env::current_dir().ok(),
            profile: Some(self.sess.profile.clone()).filter(|p| !p.is_empty()),
            dialect: self
                .sess
                .spec
                .as_ref()
                .and_then(|s| s.dialect)
                .map(|d| d.to_string()),
            config: nsql_settings::REGISTRY
                .iter()
                .filter_map(|e| {
                    self.settings
                        .get(e.key)
                        .map(|v| (e.key.to_string(), v.to_string()))
                })
                .collect(),
        }
    }

    pub(crate) fn run_defines(&self) -> Option<Vec<(String, String)>> {
        self.tab_defines.get(&self.editors.active_id()).map(|v| {
            v.iter()
                .map(|(n, raw, _)| (n.clone(), raw.clone()))
                .collect()
        })
    }

    /// 치환 변수 글로벌 층 → 러너에 보낼 `(이름, 원문)` 쌍(`Cmd::GlobalMacros` · T-153 후속 10-26).
    pub(crate) fn global_macro_pairs(&self) -> Vec<(String, String)> {
        self.global_macros
            .iter()
            .map(|(n, raw, _)| (n.clone(), raw.clone()))
            .collect()
    }

    /// ★ T-153: 접속 프로필의 `var.<이름>=값` → 프로필 층(읽기 전용 · `Layer::Fixed`) — 세션 사본 + 워커 러너(`Cmd::FixedVars`).
    pub(crate) fn load_profile_vars(&mut self) {
        let pairs: Vec<(String, String)> = if self.sess.profile.is_empty() {
            Vec::new()
        } else {
            Vault::open_default()
                .and_then(|v| v.vars(&self.sess.profile))
                .unwrap_or_default()
        };
        self.sess.fixed_vars = nsql_run::profile_var_states(pairs.clone());
        self.sess.control(worker::Cmd::FixedVars(pairs));
    }

    /// 이름이 **프로필 층에만** 있는 변수인가(탭·공유·글로벌에 같은 이름이 없음) — 창에서 고치기·지우기·층 옮기기를 거절한다.
    fn is_profile_only_var(&self, name: &str) -> bool {
        let tab = self.editors.active_id();
        let is = |v: &nsql_script::VarState| v.name.eq_ignore_ascii_case(name);
        self.sess.fixed_vars.iter().any(is)
            && !self.sess.shared_vars.iter().any(is)
            && !self.global_vars.iter().any(is)
            && !self.tab_vars.get(&tab).is_some_and(|l| l.iter().any(is))
    }

    pub(crate) fn vars_rows(&self) -> Vec<vars_win::VarRow> {
        const MAX: usize = 200;
        let tab = self.editors.active_id();
        let changed = (self.vars_changed.0 == tab).then_some(&self.vars_changed.1);
        let local = self.tab_vars.get(&tab).map(Vec::as_slice).unwrap_or(&[]);
        // ★ 가림 판정(사용자 10-07): 공유 줄은 탭에 같은 이름이 있으면 · 글로벌 줄은 탭/공유에 있으면 가려진 줄(흐리게).
        let up = |s: &str| s.to_ascii_uppercase();
        let local_names: std::collections::BTreeSet<String> =
            local.iter().map(|v| up(&v.name)).collect();
        let near_names: std::collections::BTreeSet<String> = local_names
            .iter()
            .cloned()
            .chain(self.sess.shared_vars.iter().map(|v| up(&v.name)))
            .collect();
        local
            .iter()
            .map(|v| (v, nsql_script::Layer::Local))
            .chain(
                self.sess
                    .shared_vars
                    .iter()
                    .map(|v| (v, nsql_script::Layer::Shared)),
            )
            .chain(
                self.global_vars
                    .iter()
                    .map(|v| (v, nsql_script::Layer::Global)),
            )
            .chain(
                self.sess
                    .fixed_vars
                    .iter()
                    .map(|v| (v, nsql_script::Layer::Fixed)),
            )
            .map(|(v, layer)| {
                let full = match &v.value {
                    nsql_core::Value::Null => String::new(),
                    other => other.display(),
                };
                let shown = if v.secret {
                    "******".to_string()
                } else if matches!(v.value, nsql_core::Value::Null) {
                    "NULL".to_string()
                } else if full.chars().count() > MAX {
                    format!("{}…", full.chars().take(MAX).collect::<String>())
                } else {
                    full.clone()
                };
                // ★ D-355: stale = 값 뒤 `⟳`(재계산 전임을 보인다 · 왕복 0) · 수식 자체는 Formula 열(사용자 10-25).
                let shown = if v.stale {
                    format!("{shown} \u{21bb}")
                } else {
                    shown
                };
                let shadowed = match layer {
                    nsql_script::Layer::Shared => local_names.contains(&up(&v.name)),
                    nsql_script::Layer::Global => near_names.contains(&up(&v.name)),
                    nsql_script::Layer::Fixed => {
                        near_names.contains(&up(&v.name))
                            || self.global_vars.iter().any(|g| up(&g.name) == up(&v.name))
                    }
                    nsql_script::Layer::Local => false,
                };
                vars_win::VarRow {
                    name: v.name.clone(),
                    ty: var_type_text(&v.ty),
                    value: shown,
                    edit: if v.secret { String::new() } else { full },
                    layer,
                    changed: changed.is_some_and(|c| c.contains(&v.name.to_ascii_uppercase())),
                    shadowed,
                    stale: v.stale,
                    formula: v
                        .formula
                        .as_deref()
                        .map(|f| format!(":= {f}"))
                        .unwrap_or_default(),
                }
            })
            // 치환 변수(`&이름` · DEFINE) — 표시값 = 사용 시 모드면 `원문 → 현재 값`(63 §9) · 편집 = 원문.
            .chain(
                self.tab_defines
                    .get(&tab)
                    .into_iter()
                    .flatten()
                    .map(|d| macro_row(d, nsql_script::Layer::Local)),
            )
            // ★ 공유·글로벌 층 치환 변수(`DEFINE x SHARE|GLOBAL` · T-153 후속 10-26) — 창에서 값을 고치면 탭 층에 그림자가 생긴다(종전 편집 경로) ·
            //   층 이동은 스크립트 명령으로.
            .chain(
                self.sess
                    .shared_macros
                    .iter()
                    .map(|d| macro_row(d, nsql_script::Layer::Shared)),
            )
            .chain(
                self.global_macros
                    .iter()
                    .map(|d| macro_row(d, nsql_script::Layer::Global)),
            )
            .collect()
    }

    /// ★ D-355(10-24): 변수 창 ⟳ — 사용 시 모드의 stale 수식을 지금 한 묶음으로 재계산(빈 스크립트 실행 + `refresh_vars` · 왕복 = 수식 묶음 1회).
    ///   DB로 가는 진입점이므로 문지기(`gate_pass`)를 지난다 · 막혔으면 상태줄 안내만.
    pub(crate) fn vars_refresh_now(&mut self) {
        let Some(pass) = self.gate_pass() else {
            return;
        };
        self.sess.submit(
            pass,
            worker::Cmd::Run {
                src: String::new(),
                preflight: None,
                max_rows: 0,
                vars: self.run_vars(),
                defines: self.run_defines(),
                intrinsic: Some(self.run_intrinsic()),
                whole: false,
                hold: None,
                refresh_vars: true,
            },
        );
        self.redraw();
    }

    /// 변수 창의 동작을 표에 반영한다 — 탭 층은 App이 주인(바로 고친다 · 보존 파일도) · 공유 층은 워커에 통째로 알린다.
    pub(crate) fn vars_apply(&mut self, action: vars_win::VarsWinAction) {
        use vars_win::VarsWinAction as A;
        let tab = self.editors.active_id();
        let is = |v: &nsql_script::VarState, n: &str| v.name.eq_ignore_ascii_case(n);
        let mut shared_dirty = false;
        let mut global_dirty = false;
        match action {
            A::None | A::Paint => return,
            A::Refresh => {
                self.vars_refresh_now();
                return;
            }
            A::Script => {
                self.menu_action("vars.script");
                return;
            }
            // ★ T-420: auto 변수의 타입을 지금 값에서 고정(선언 변수로 · 다음 대입에서 값을 따라 바뀌지 않는다).
            A::FixType(name) if !name.starts_with('&') => {
                let is = |v: &nsql_script::VarState| v.name.eq_ignore_ascii_case(&name);
                let fix = |v: &mut nsql_script::VarState| -> String {
                    if v.ty == nsql_core::VarType::Auto {
                        v.ty = nsql_core::VarType::infer(&v.value);
                    }
                    v.declared = true;
                    v.ty.sql_name()
                };
                let fixed = if let Some(v) = self.sess.shared_vars.iter_mut().find(|v| is(v)) {
                    shared_dirty = true;
                    Some(fix(v))
                } else if let Some(v) = self.global_vars.iter_mut().find(|v| is(v)) {
                    global_dirty = true;
                    Some(fix(v))
                } else {
                    self.tab_vars
                        .get_mut(&tab)
                        .and_then(|l| l.iter_mut().find(|v| is(v)))
                        .map(fix)
                };
                if let Some(ty) = fixed {
                    self.sess.status = tf(Msg::StVarTypeFixed, &[&name, &ty]);
                    self.vars_changed = (tab, std::iter::once(name.to_ascii_uppercase()).collect());
                }
            }
            A::FixType(_) => return,
            A::Set(name, _) | A::SetNull(name) | A::Delete(name) | A::Layer(name, _)
                if !name.starts_with('&') && self.is_profile_only_var(&name) =>
            {
                // 프로필 층은 읽기 전용(docs/63 §11) — 바꾸려면 프로필(`nsql conn var`).
                self.sess.status = t(Msg::StVarsProfileReadOnly).into();
                self.redraw();
                return;
            }
            A::Set(name, text) if name.starts_with('&') => {
                // 치환 변수 편집 = 원문을 바꾼다(다음 실행 때 엔진에 전달 · 표시값도 원문으로).
                let key = name[1..].to_ascii_uppercase();
                let list = self.tab_defines.entry(tab).or_default();
                match list.iter_mut().find(|(n, _, _)| *n == key) {
                    Some(slot) => {
                        slot.1 = text.clone();
                        slot.2 = text;
                    }
                    None => list.push((key.clone(), text.clone(), text)),
                }
                self.vars_changed = (tab, std::iter::once(name.to_ascii_uppercase()).collect());
            }
            A::SetNull(name) if name.starts_with('&') => {
                let key = name[1..].to_ascii_uppercase();
                if let Some(list) = self.tab_defines.get_mut(&tab) {
                    list.retain(|(n, _, _)| *n != key);
                }
            }
            A::Set(name, text) => {
                let value = nsql_run::input_value(&text);
                let set = |v: &mut nsql_script::VarState| {
                    if v.ty == nsql_core::VarType::Auto || !v.declared {
                        v.ty = nsql_core::VarType::infer(&value);
                    }
                    v.value = value.clone();
                };
                if let Some(v) = self.sess.shared_vars.iter_mut().find(|v| is(v, &name)) {
                    set(v);
                    shared_dirty = true;
                } else if let Some(v) = self.global_vars.iter_mut().find(|v| is(v, &name)) {
                    set(v);
                    global_dirty = true;
                } else {
                    let list = self.tab_vars.entry(tab).or_default();
                    match list.iter_mut().find(|v| is(v, &name)) {
                        Some(v) => set(v),
                        None => list.push(nsql_script::VarState {
                            secret: nsql_script::looks_secret(&name),
                            name: name.clone(),
                            ty: nsql_core::VarType::infer(&value),
                            value: value.clone(),
                            declared: false,
                            layer: nsql_script::Layer::Local,
                            formula: None,
                            stale: false,
                        }),
                    }
                }
                self.vars_changed = (tab, std::iter::once(name.to_ascii_uppercase()).collect());
            }
            A::SetNull(name) => {
                if let Some(v) = self.sess.shared_vars.iter_mut().find(|v| is(v, &name)) {
                    v.value = nsql_core::Value::Null;
                    shared_dirty = true;
                } else if let Some(v) = self.global_vars.iter_mut().find(|v| is(v, &name)) {
                    v.value = nsql_core::Value::Null;
                    global_dirty = true;
                } else if let Some(v) = self
                    .tab_vars
                    .get_mut(&tab)
                    .and_then(|l| l.iter_mut().find(|v| is(v, &name)))
                {
                    v.value = nsql_core::Value::Null;
                }
            }
            A::Delete(name) => {
                let before = self.sess.shared_vars.len();
                self.sess.shared_vars.retain(|v| !is(v, &name));
                shared_dirty = self.sess.shared_vars.len() != before;
                let gb = self.global_vars.len();
                self.global_vars.retain(|v| !is(v, &name));
                global_dirty = self.global_vars.len() != gb;
                if let Some(l) = self.tab_vars.get_mut(&tab) {
                    l.retain(|v| !is(v, &name));
                }
            }
            A::Layer(name, to) => {
                // 세 층(탭 · 공유 · 글로벌) 사이 이동 — 단일 원천에서 빼서 목적 층에 넣는다(docs/63 §11).
                let mut taken: Option<nsql_script::VarState> = None;
                if let Some(l) = self.tab_vars.get_mut(&tab) {
                    if let Some(i) = l.iter().position(|v| is(v, &name)) {
                        taken = Some(l.remove(i));
                    }
                }
                if taken.is_none() {
                    if let Some(i) = self.sess.shared_vars.iter().position(|v| is(v, &name)) {
                        taken = Some(self.sess.shared_vars.remove(i));
                        shared_dirty = true;
                    }
                }
                if taken.is_none() {
                    if let Some(i) = self.global_vars.iter().position(|v| is(v, &name)) {
                        taken = Some(self.global_vars.remove(i));
                        global_dirty = true;
                    }
                }
                if let Some(mut v) = taken {
                    v.layer = to;
                    match to {
                        nsql_script::Layer::Shared => {
                            self.sess.shared_vars.push(v);
                            shared_dirty = true;
                        }
                        nsql_script::Layer::Global => {
                            self.global_vars.push(v);
                            global_dirty = true;
                        }
                        _ => self.tab_vars.entry(tab).or_default().push(v),
                    }
                }
            }
        }
        if shared_dirty {
            self.sess
                .control(worker::Cmd::SharedVars(self.sess.shared_vars.clone()));
        }
        if global_dirty {
            self.global_vars_changed();
        }
        let local = self.tab_vars.get(&tab).cloned().unwrap_or_default();
        self.vars_persist_save(tab, &local);
        self.vars_win.redraw();
    }

    /// 글로벌 층이 바뀌었다(변수 창 · 스크립트 `VAR x GLOBAL`) — 파일에 남기고(`vars.global_persist`) 모든 세션에 전파.
    pub(crate) fn global_vars_changed(&mut self) {
        if self.settings.flag("vars.global_persist") {
            if let Some(dir) = varsfile::dir() {
                varsfile::store_global(&dir, &self.global_vars);
            }
        }
        let v = self.global_vars.clone();
        for s in self.all_sess() {
            s.control(worker::Cmd::GlobalVars(v.clone()));
        }
    }

    /// 지금 실행하려는 탭의 변수 표(탭 층) — 없으면 빈 표. 실행 명령에 실어 보낸다(D-135).
    pub(crate) fn run_vars(&self) -> Option<Vec<nsql_script::VarState>> {
        Some(
            self.tab_vars
                .get(&self.editors.active_id())
                .cloned()
                .unwrap_or_default(),
        )
    }
}

/// 치환 변수 한 줄(변수 창) — 표시값 = 사용 시 모드면 `원문 → 현재 값`(63 §9) · 편집 = 원문 · 층 = 탭/공유/글로벌(T-153 후속).
fn macro_row(
    (n, raw, shown): &(String, String, String),
    layer: nsql_script::Layer,
) -> vars_win::VarRow {
    vars_win::VarRow {
        name: format!("&{n}"),
        ty: "DEFINE".into(),
        value: shown.clone(),
        edit: raw.clone(),
        layer,
        changed: false,
        shadowed: false,
        stale: false,
        formula: String::new(),
    }
}
