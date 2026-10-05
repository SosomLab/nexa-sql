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

    pub(crate) fn vars_rows(&self) -> Vec<vars_win::VarRow> {
        const MAX: usize = 200;
        let tab = self.editors.active_id();
        let changed = (self.vars_changed.0 == tab).then_some(&self.vars_changed.1);
        let local = self.tab_vars.get(&tab).map(Vec::as_slice).unwrap_or(&[]);
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
                vars_win::VarRow {
                    name: v.name.clone(),
                    ty: var_type_text(&v.ty),
                    value: shown,
                    edit: if v.secret { String::new() } else { full },
                    layer,
                    changed: changed.is_some_and(|c| c.contains(&v.name.to_ascii_uppercase())),
                }
            })
            // 치환 변수(`&이름` · DEFINE) — 표시값 = 사용 시 모드면 `원문 → 현재 값`(63 §9) · 편집 = 원문.
            .chain(
                self.tab_defines
                    .get(&tab)
                    .into_iter()
                    .flatten()
                    .map(|(n, raw, shown)| vars_win::VarRow {
                        name: format!("&{n}"),
                        ty: "DEFINE".into(),
                        value: shown.clone(),
                        edit: raw.clone(),
                        layer: nsql_script::Layer::Local,
                        changed: false,
                    }),
            )
            .collect()
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
            A::Script => {
                self.menu_action("vars.script");
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
