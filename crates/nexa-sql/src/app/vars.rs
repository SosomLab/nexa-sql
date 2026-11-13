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
            // ★ 비밀(표시값 `******` = HIDE 또는 이름 규칙 · T-132 ①)은 디스크·다른 세션으로 내보내지 않는다.
            .filter(|(n, _, shown)| !macro_is_secret(n, shown))
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
        // ★ 👁 보기(11-11): 켜져 있으면 비밀 행에 원문을 실어 보낸다(창이 누르고 있는 동안만 그린다 · 로그·저장은 안 본다).
        let reveal_on = self.settings.flag("vars.reveal_secret");
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
                let reveal = (v.secret && reveal_on).then(|| vars_win::Hidden(full.clone()));
                vars_win::VarRow {
                    name: v.name.clone(),
                    ty: var_type_text(&v.ty),
                    value: shown,
                    secret: v.secret,
                    reveal,
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
                    .map(|d| macro_row(d, nsql_script::Layer::Local, reveal_on)),
            )
            // ★ 공유·글로벌 층 치환 변수(`DEFINE x SHARE|GLOBAL` · T-153 후속 10-26) — 창에서 값을 고치면 탭 층에 그림자가 생긴다(종전 편집 경로) ·
            //   층 이동은 스크립트 명령으로.
            .chain(
                self.sess
                    .shared_macros
                    .iter()
                    .map(|d| macro_row(d, nsql_script::Layer::Shared, reveal_on)),
            )
            .chain(
                self.global_macros
                    .iter()
                    .map(|d| macro_row(d, nsql_script::Layer::Global, reveal_on)),
            )
            .collect()
    }

    /// ★ D-355(10-24): 변수 창 ⟳ — 사용 시 모드의 stale 수식을 지금 한 묶음으로 재계산(빈 스크립트 실행 + `refresh_vars` · 왕복 = 수식 묶음 1회).
    ///   DB로 가는 진입점이므로 문지기(`gate_pass`)를 지난다 · 막혔으면 상태줄 안내만.
    /// ★ 변수를 수식으로 지정(변수 창 · 사용자 11-12): `EXEC :이름 := 수식`을 보통 실행 경로로 보낸다(결과·오류는 실행 카드와
    /// Output에). 수식 안에서 `:` 없이 쓴 **이미 있는 변수 이름**은 `:이름`으로 바꿔 준다(`VAR2 + 5` → `:VAR2 + 5`).
    /// 식 하나의 평가라 운영 2단 무장·보류 커밋은 건너뛴다(읽기 전용 검사·문지기는 그대로).
    pub(crate) fn vars_set_formula(&mut self, name: &str, expr: &str) {
        let name = name.trim().trim_start_matches(':');
        let expr = expr.trim().trim_end_matches(';').trim();
        if name.is_empty() || expr.is_empty() || expr.contains(';') {
            self.sess.status = t(Msg::StVarFormulaBad).into();
            self.redraw();
            return;
        }
        if !self.sess.connected || !self.gate_open() {
            self.sess.status = t(Msg::StVarFormulaNeedConn).into();
            self.redraw();
            return;
        }
        let known: Vec<String> = self
            .vars_rows()
            .iter()
            .map(|r| r.name.to_ascii_uppercase())
            .collect();
        let expr = bind_bare_names(expr, &known);
        // ★ 수식으로 **기억**되는 것은 "사용 시" 계산 모드(`vars.expand_at = use`)일 때뿐이다 — 대입 시 모드면 값만 한 번 계산된다
        //   (사용자 11-12 "값은 들어갔는데 수식으로 동작하지 않는다"). 모드를 멋대로 바꾸지 않고(DEFINE 치환 방식도 함께 바뀐다)
        //   알림의 [켜고 수식으로]로 묻는다 — 누르면 모드를 켜고 같은 수식을 다시 지정한다.
        if self.settings.get("vars.expand_at") != Some("use") {
            self.toasts.push_button(
                toast::ToastKind::Info,
                t(Msg::VarFormulaModeTitle),
                t(Msg::VarFormulaModeBody),
                t(Msg::VarFormulaModeOn),
                &format!("vars.formula_on:{name}\t{expr}"),
            );
        }
        // ★ 건너뛰는 것은 **계산만 하는 식**일 때뿐(11-13 데이터 보호): 식 안에 사용자 루틴 호출이 있으면 그 루틴이 DML을 할 수
        //   있으므로 보통 실행과 똑같이 운영 2단 확인·보류 커밋을 거친다(같은 수식을 제한 시간 안에 다시 적용하면 진행).
        let plain = nsql_script::bind::expr_is_plain(&expr);
        self.prod_confirmed_once = plain;
        self.hold_skip_once = plain;
        self.run_text(format!("EXEC :{name} := {expr};"), 0, true);
        self.prod_confirmed_once = false;
        self.hold_skip_once = false;
        self.project_touch();
    }

    pub(crate) fn vars_refresh_now(&mut self) {
        let Some(pass) = self.gate_pass() else {
            return;
        };
        // ★ 결과(`RunEvent::Vars`)는 `sess.run_editor` 탭의 표에 반영된다 — 이 실행이 보내는 표(`run_vars` = 활성 탭)와 같은 탭으로
        //   맞춘다(협업 V1 11-13 "재기동 뒤 ⟳가 재계산하지 않는다": 그 탭에서 한 번도 실행하지 않았으면 `run_editor`가 다른 탭이라
        //   계산된 값이 엉뚱한 탭 표로 들어갔다 · 같은 실행 안에서는 앞선 실행이 맞춰 놓아 드러나지 않았다).
        self.sess.run_editor = self.editors.active_id();
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
            // ★ 수식 지정(변수 창 `이름 := 수식` · 고른 줄에 `= 수식` · 11-12): 엔진으로 `EXEC :이름 := 수식`을 실행한다.
            A::Set(name, text) if !name.starts_with('&') && text.trim_start().starts_with('=') => {
                let expr = text.trim_start()[1..].trim().to_string();
                self.vars_set_formula(&name, &expr);
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
                            // 표기 = 대문자 하나(사용자 11-13 "변수는 모두 대문자로 관리").
                            name: name.to_ascii_uppercase(),
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
        // 변수 창(· AI 도구 `set_var`)에서 고친 것도 프로젝트 자동 저장 대상(무제 탭의 변수 = 프로젝트 파일 · 11-12).
        self.project_touch();
        self.vars_win.redraw();
        // ★ 값을 고쳤으면 그 값을 쓰는 수식 변수를 바로 다시 계산한다(사용자 11-13 "요청이 즉시 반영되면 좋겠다" · 사용 시 모드 ·
        //   수식이 하나라도 있을 때만 · 접속 중이고 세션이 한가할 때만 — 아니면 다음 실행·⟳에서 계산된다).
        let has_formula = local.iter().any(|v| v.formula.is_some())
            || self.sess.shared_vars.iter().any(|v| v.formula.is_some())
            || self.global_vars.iter().any(|v| v.formula.is_some());
        if has_formula
            && self.settings.get("vars.expand_at") == Some("use")
            && self.sess.connected
            && !self.sess.busy
            && self.gate_open()
        {
            self.vars_refresh_now();
        }
    }

    /// 글로벌 층이 바뀌었다(변수 창 · 스크립트 `VAR x GLOBAL`) — 파일에 남기고(`vars.global_persist`) 모든 세션에 전파.
    pub(crate) fn global_vars_changed(&mut self) {
        self.persist_global_layers();
        let v = self.global_vars.clone();
        for s in self.all_sess() {
            s.control(worker::Cmd::GlobalVars(v.clone()));
        }
    }

    /// ★ 치환 변수 글로벌 층이 바뀌었다(스크립트 `DEFINE x GLOBAL` · T-153 후속 10-27) — 같은 파일에 `DEFINE` 줄로 남기고 모든 세션 러너에 전파.
    pub(crate) fn global_macros_changed(&mut self) {
        self.persist_global_layers();
        let pairs = self.global_macro_pairs();
        for s in self.all_sess() {
            s.control(worker::Cmd::GlobalMacros(pairs.clone()));
        }
    }

    /// 글로벌 층 두 가지(바인드 + 치환)를 `vars/global.sql` 한 파일에(`vars.global_persist` · 비밀·커서 값 제외).
    fn persist_global_layers(&self) {
        if self.settings.flag("vars.global_persist") {
            if let Some(dir) = varsfile::dir() {
                varsfile::store_global(&dir, &self.global_vars, &self.global_macro_pairs());
            }
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

/// 치환 변수가 비밀인가(T-132 ①) — 러너가 표시값을 `******`로 보낸 것(`ACCEPT HIDE`) 또는 이름 규칙(D-140). 원문(`raw`)은
/// 다음 실행에 되돌려 주려고 메모리에만 든다.
fn macro_is_secret(name: &str, shown: &str) -> bool {
    shown == nsql_script::SECRET_MASK || nsql_script::looks_secret(name)
}

/// 치환 변수 한 줄(변수 창) — 표시값 = 사용 시 모드면 `원문 → 현재 값`(63 §9) · 편집 = 원문 · 층 = 탭/공유/글로벌(T-153 후속).
fn macro_row(
    (n, raw, shown): &(String, String, String),
    layer: nsql_script::Layer,
    reveal_on: bool,
) -> vars_win::VarRow {
    // ★ 비밀 치환 변수(T-132 ①)는 바인드 비밀과 같이 값 `******` · 편집란 비움 · 👁 보기(11-11)는 설정이 켜져 있을 때만 원문 실음.
    let secret = macro_is_secret(n, shown);
    vars_win::VarRow {
        name: format!("&{n}"),
        ty: "DEFINE".into(),
        value: if secret {
            nsql_script::SECRET_MASK.into()
        } else {
            shown.clone()
        },
        edit: if secret { String::new() } else { raw.clone() },
        layer,
        changed: false,
        shadowed: false,
        stale: false,
        formula: String::new(),
        secret,
        reveal: (secret && reveal_on).then(|| vars_win::Hidden(raw.clone())),
    }
}

#[cfg(test)]
mod reveal_tests {
    use super::macro_row;

    /// ★ 비밀 행 = 값 `******` · 편집란 빈 글 · 👁 원문은 설정이 켜졌을 때만 · `{:?}`에는 안 찍힌다(11-11).
    #[test]
    fn secret_macro_row_masks_and_reveals_only_when_enabled() {
        let d = (
            "DB_PASS".to_string(),
            "hunter2".to_string(),
            "hunter2".to_string(),
        );
        let off = macro_row(&d, nsql_script::Layer::Local, false);
        assert_eq!(off.value, nsql_script::SECRET_MASK);
        assert!(off.edit.is_empty() && off.secret && off.reveal.is_none());
        let on = macro_row(&d, nsql_script::Layer::Local, true);
        assert_eq!(on.reveal.as_ref().map(|h| h.0.as_str()), Some("hunter2"));
        assert!(
            !format!("{on:?}").contains("hunter2"),
            "Debug에 원문이 새지 않는다"
        );
        let plain = macro_row(
            &(
                "OWNER".to_string(),
                "scott".to_string(),
                "scott".to_string(),
            ),
            nsql_script::Layer::Local,
            true,
        );
        assert!(!plain.secret && plain.reveal.is_none() && plain.edit == "scott");
    }
}

/// 수식 안에서 `:` 없이 쓴 낱말이 알려진 변수 이름(대문자 목록)이면 `:이름`으로(문자열 리터럴 안 · 이미 `:`가 붙은 것 ·
/// 뒤에 `(`가 오는 함수 호출 · `.` 뒤의 낱말은 그대로). 순수 함수(시험).
pub(crate) fn bind_bare_names(expr: &str, known: &[String]) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let mut out = String::with_capacity(expr.len() + 8);
    let mut i = 0;
    let mut in_str = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            in_str = !in_str;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_str && (c.is_alphabetic() || c == '_') {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$' | '#'))
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let prev = start.checked_sub(1).map(|p| chars[p]);
            let next = chars[i..].iter().find(|c| !c.is_whitespace()).copied();
            let bare = !matches!(prev, Some(':' | '.' | '&')) && next != Some('(');
            if bare && known.iter().any(|k| k.eq_ignore_ascii_case(&word)) {
                out.push(':');
            }
            out.push_str(&word);
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod formula_tests {
    use super::bind_bare_names;

    #[test]
    fn bare_variable_names_get_a_colon() {
        let known = vec!["VAR2".to_string(), "V1".to_string()];
        assert_eq!(bind_bare_names("VAR2 + 5", &known), ":VAR2 + 5");
        assert_eq!(bind_bare_names(":VAR2 + v1", &known), ":VAR2 + :v1");
        // 문자열 안 · 함수 호출 · 한정 이름 · 모르는 낱말은 그대로.
        assert_eq!(
            bind_bare_names("'VAR2' || t.VAR2", &known),
            "'VAR2' || t.VAR2"
        );
        assert_eq!(
            bind_bare_names("NVL(VAR2, 0) + other", &known),
            "NVL(:VAR2, 0) + other"
        );
        assert_eq!(bind_bare_names("V1(3)", &known), "V1(3)");
    }
}
