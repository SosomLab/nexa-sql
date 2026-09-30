//! **WASM 확장 런타임**(ABI v1 · docs/75 · 사용자 09-23 "내려받아 동적 로딩") — `wasmi` 인터프리터(DR-16 · D-87 ①).
//!
//! 게스트 = `extensions/sdk/nexa-ext-sdk`로 만든 `.wasm`(export `memory`·`nx_alloc`·`nx_ext_meta`·`nx_ext_settings`·
//! `nx_ext_disabled`·`nx_ext_run` · import `env.nx_editor_op`·`env.nx_log`). 버퍼 = 4바이트 LE 길이 + UTF-8 · 메타/설정/효과 = JSON.
//!
//! 격리(nexa-dir2 ADR-0005 선례): 호출마다 **새 인스턴스**(상태 없음) · 연료 상한 · 선형 메모리 상한 · 벽시계 상한 ·
//! 연속 실패 3회 = 그 확장은 세션 동안 정지(서킷 브레이커). 편집기 조작은 게스트가 요청한 op를 **큐에 모아** 호출이 끝난 뒤
//! `EditorOps`에 적용한다(호스트 상태를 게스트 호출 안에 빌려주지 않는다 · unsafe 0).
//!
//! 같은 id의 WASM 확장이 로드되면 내장(builtin) 확장을 **대체**한다(내장 = 로드 실패 때 폴백).

use super::{Command, EditorOps, Extension, ExtensionEffect, Label, MenuContribution};
use nsql_settings::json::{self, Json};
use nsql_settings::Settings;
use std::cell::Cell;
use std::path::Path;
use std::time::{Duration, Instant};
use wasmi::{Caller, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// 호출당 연료(인터프리터 명령 수) — 정책 층은 계산이 거의 없다(설정 파싱 · JSON).
const FUEL: u64 = 50_000_000;
/// 선형 메모리 상한.
const MEM_CAP: usize = 16 * 1024 * 1024;
/// 호출당 벽시계 상한(UI 스레드).
const CALL_TIMEOUT_MS: u64 = 200;
/// ★ 포맷 호출(ABI v1.1 `nx_ext_format`)은 문서 크기에 비례해 일한다 — 연료·시간 상한을 따로 둔다(1 MB 문서 기준).
const FORMAT_FUEL: u64 = 2_000_000_000;
const FORMAT_TIMEOUT_MS: u64 = 5_000;
/// 반환 버퍼 상한.
const OUT_CAP: usize = 1 << 20;
/// 모듈 크기 상한.
const MODULE_CAP: usize = 8 * 1024 * 1024;
/// 연속 실패 격리 횟수.
const BREAKER_LIMIT: u32 = 3;

/// 호스트 상태(호출 하나 동안) — 리미터 · 마감 · 게스트가 요청한 편집기 op · 로그.
struct HostCtx {
    limits: StoreLimits,
    deadline: Instant,
    ops: Vec<(i32, bool)>,
    logs: Vec<String>,
    /// ★ 호스트 상황(09-30 · docs/97): 활성 문서 경로 — `nx_host_get(DOC_PATH)`가 돌려준다.
    doc_path: String,
}

/// `nx_host_get`의 종류 번호(SDK `query::*`와 같은 표).
const HOST_GET_DOC_PATH: i32 = 1;

fn host_guard(caller: &mut Caller<'_, HostCtx>, cost: u64) -> Result<(), wasmi::Error> {
    if Instant::now() >= caller.data().deadline {
        return Err(wasmi::Error::new("call exceeded its time limit"));
    }
    let fuel = caller.get_fuel()?;
    if fuel < cost {
        return Err(wasmi::Error::new("out of fuel (host charge)"));
    }
    caller.set_fuel(fuel - cost)
}

fn read_buf(mem: &[u8], ptr: u32) -> Option<String> {
    let p = ptr as usize;
    let len = u32::from_le_bytes(mem.get(p..p + 4)?.try_into().ok()?) as usize;
    if len > OUT_CAP {
        return None;
    }
    Some(String::from_utf8_lossy(mem.get(p + 4..p + 4 + len)?).into_owned())
}

fn linker(engine: &Engine) -> Result<Linker<HostCtx>, wasmi::Error> {
    let mut l = Linker::new(engine);
    // nx_editor_op(op, flag) -> i32 : 편집기 이동 요청(큐) — 호출이 끝난 뒤 적용 · 게스트에는 1(요청 접수)을 준다.
    l.func_wrap(
        "env",
        "nx_editor_op",
        |mut caller: Caller<'_, HostCtx>, op: i32, flag: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 10_000)?;
            if caller.data().ops.len() >= 64 {
                return Ok(0);
            }
            caller.data_mut().ops.push((op, flag != 0));
            Ok(1)
        },
    )?;
    // ★ nx_host_get(kind, ptr, cap) -> i32 : 호스트 상황 값을 게스트 버퍼(길이 접두 · 게스트가 `nx_alloc`으로 마련 · `cap` =
    //   본문 최대)에 써 준다 · 반환 = 쓴 길이(모르는 종류·안 맞으면 -1). 종류 1 = 활성 문서 경로(docs/97 §2).
    l.func_wrap(
        "env",
        "nx_host_get",
        |mut caller: Caller<'_, HostCtx>,
         kind: i32,
         ptr: i32,
         cap: i32|
         -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 20_000)?;
            let value = match kind {
                HOST_GET_DOC_PATH => caller.data().doc_path.clone(),
                _ => return Ok(-1),
            };
            let bytes = value.as_bytes();
            if bytes.len() > cap.max(0) as usize {
                return Ok(-1);
            }
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let mut buf = Vec::with_capacity(bytes.len() + 4);
            buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(bytes);
            if mem.write(&mut caller, ptr as usize, &buf).is_err() {
                return Ok(-1);
            }
            Ok(bytes.len() as i32)
        },
    )?;
    // nx_log(ptr) : 로그 한 줄(호출당 32줄까지).
    l.func_wrap(
        "env",
        "nx_log",
        |mut caller: Caller<'_, HostCtx>, ptr: i32| -> Result<(), wasmi::Error> {
            host_guard(&mut caller, 20_000)?;
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(());
            };
            let text = read_buf(mem.data(&caller), ptr as u32).unwrap_or_default();
            if caller.data().logs.len() < 32 {
                caller
                    .data_mut()
                    .logs
                    .push(text.chars().take(512).collect());
            }
            Ok(())
        },
    )?;
    Ok(l)
}

/// 로드된 WASM 확장 — 모듈은 검증·컴파일 완료(호출마다 인스턴스만).
pub(crate) struct WasmExtension {
    id: String,
    name: String,
    prefix: String,
    commands: Vec<Command>,
    menus: Vec<MenuContribution>,
    /// 포맷터 선언(메타 `formatter` · ABI v1.1): (이름, 예시 SQL).
    formatter: Option<(Label, String)>,
    /// ★ 포맷터 미리보기 표식(메타 `formatter.marks` · 09-30): (설정 키, 조각) — 확장 것은 확장이 낸다.
    formatter_marks: Vec<(String, String)>,
    /// ★ 이번 `run_with` 호출의 호스트 상황(활성 문서 경로 · docs/97) — 호출 동안만.
    doc_path: std::cell::RefCell<String>,
    engine: Engine,
    module: Module,
    /// 연속 실패 수(브레이커).
    failures: Cell<u32>,
    /// 마지막 호출의 로그·오류(호스트가 거둬 로그 창에).
    pub(crate) notes: std::cell::RefCell<Vec<String>>,
}

impl std::fmt::Debug for WasmExtension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WasmExtension({} {})", self.id, self.name)
    }
}

pub(crate) fn jget<'a>(v: &'a Json, key: &str) -> Option<&'a Json> {
    match v {
        Json::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}
pub(crate) fn jstr(v: &Json) -> Option<&str> {
    match v {
        Json::Str(s) => Some(s),
        _ => None,
    }
}
fn jbool(v: &Json) -> Option<bool> {
    match v {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}
fn jnum(v: &Json) -> Option<f64> {
    match v {
        Json::Num(n) => Some(*n),
        _ => None,
    }
}
fn jarr(v: &Json) -> &[Json] {
    match v {
        Json::Arr(a) => a,
        _ => &[],
    }
}

/// 라벨 JSON(`{"en":…,"ko":…}` 또는 문자열) → [`Label`].
fn label_of(v: Option<&Json>) -> Label {
    match v {
        Some(Json::Str(s)) => Label::Text(s.clone(), None),
        Some(o @ Json::Obj(_)) => Label::Text(
            jget(o, "en").and_then(jstr).unwrap_or("").to_string(),
            jget(o, "ko").and_then(jstr).map(str::to_string),
        ),
        _ => Label::Text(String::new(), None),
    }
}

/// 효과 JSON → [`ExtensionEffect`](색 층만 · 나머지는 앱 기본값 = 호스트가 덮어쓴다).
pub(crate) fn effect_from_json(text: &str) -> Result<ExtensionEffect, String> {
    let v = json::parse(text)?;
    let Some(b) = jget(&v, "bracket") else {
        return Ok(ExtensionEffect::default());
    };
    let colors: Vec<nexa_ctl::Color> = jget(b, "colors")
        .map(jarr)
        .unwrap_or(&[])
        .iter()
        .filter_map(jstr)
        .filter_map(|h| nexa_ctl::color_from_hex(h.trim_start_matches('#')))
        .collect();
    let max_chars = jget(b, "max_chars").and_then(jnum).unwrap_or(0.0);
    Ok(ExtensionEffect {
        bracket_opts: Some(nexa_ctl::BracketOpts {
            rainbow: jget(b, "rainbow").and_then(jbool).unwrap_or(true),
            unmatched: jget(b, "unmatched").and_then(jbool).unwrap_or(true),
            colors,
            max_chars: if max_chars <= 0.0 {
                usize::MAX
            } else {
                max_chars as usize
            },
            ..nexa_ctl::BracketOpts::default()
        }),
    })
}

/// 설정 → 게스트 입력 JSON(접두 키만 · 레지스트리가 아는 키 · 값은 원문).
pub(crate) fn settings_json(s: &Settings, prefix: &str) -> String {
    let items: Vec<(String, Json)> = nsql_settings::REGISTRY
        .iter()
        .filter(|e| e.key.starts_with(prefix))
        .map(|e| {
            (
                e.key.to_string(),
                Json::Str(s.get(e.key).unwrap_or(e.default).to_string()),
            )
        })
        .collect();
    json::dump(&Json::Obj(items))
}

impl WasmExtension {
    /// `.wasm` 바이트 → 검증·컴파일 + 메타 읽기.
    pub(crate) fn load(bytes: &[u8]) -> Result<WasmExtension, String> {
        if bytes.len() > MODULE_CAP {
            return Err(format!("module over {} MB", MODULE_CAP >> 20));
        }
        let mut cfg = wasmi::Config::default();
        cfg.consume_fuel(true);
        let engine = Engine::new(&cfg);
        let module = Module::new(&engine, bytes).map_err(|e| e.to_string())?;
        let mut ext = WasmExtension {
            id: String::new(),
            name: String::new(),
            prefix: String::new(),
            commands: Vec::new(),
            menus: Vec::new(),
            formatter: None,
            formatter_marks: Vec::new(),
            doc_path: std::cell::RefCell::new(String::new()),
            engine,
            module,
            failures: Cell::new(0),
            notes: std::cell::RefCell::new(Vec::new()),
        };
        let meta = ext.call_buf("nx_ext_meta", None)?;
        let v = json::parse(&meta).map_err(|e| format!("meta json: {e}"))?;
        let abi = jget(&v, "abi").and_then(jnum).unwrap_or(0.0) as i64;
        if abi != 1 {
            return Err(format!("unsupported abi {abi} (host = 1)"));
        }
        ext.id = jget(&v, "id")
            .and_then(jstr)
            .filter(|s| !s.is_empty())
            .ok_or("meta: id missing")?
            .to_string();
        ext.name = jget(&v, "name")
            .and_then(jstr)
            .unwrap_or(&ext.id)
            .to_string();
        ext.prefix = jget(&v, "settings_prefix")
            .and_then(jstr)
            .unwrap_or("")
            .to_string();
        ext.commands = jget(&v, "commands")
            .map(jarr)
            .unwrap_or(&[])
            .iter()
            .filter_map(|c| {
                let id = jget(c, "id").and_then(jstr)?.to_string();
                Some(Command {
                    id,
                    label: label_of(jget(c, "label")),
                })
            })
            .collect();
        ext.formatter = jget(&v, "formatter").map(|f| {
            (
                label_of(jget(f, "label")),
                jget(f, "sample").and_then(jstr).unwrap_or("").to_string(),
            )
        });
        ext.formatter_marks = jget(&v, "formatter")
            .and_then(|f| jget(f, "marks"))
            .map(jarr)
            .unwrap_or(&[])
            .iter()
            .filter_map(|m| {
                let key = jget(m, "key").and_then(jstr)?.to_string();
                let mark = jget(m, "mark").and_then(jstr)?.to_string();
                Some((key, mark))
            })
            .collect();
        ext.menus = jget(&v, "menus")
            .map(jarr)
            .unwrap_or(&[])
            .iter()
            .filter_map(|m| {
                let id = jget(m, "id").and_then(jstr)?.to_string();
                let items: Vec<Command> = jget(m, "items")
                    .map(jarr)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(jstr)
                    .filter_map(|cid| ext.commands.iter().find(|c| c.id == cid).cloned())
                    .collect();
                Some(MenuContribution {
                    id,
                    label: label_of(jget(m, "label")),
                    items,
                })
            })
            .collect();
        Ok(ext)
    }

    /// 파일에서 로드.
    pub(crate) fn load_file(path: &Path) -> Result<WasmExtension, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::load(&bytes)
    }

    fn tripped(&self) -> bool {
        self.failures.get() >= BREAKER_LIMIT
    }

    /// 인스턴스 생성 → (입력 버퍼가 있으면 `nx_alloc`에 쓰고) `f(ptr?) -> ptr` 호출 → 반환 버퍼. 로그는 `notes`에.
    fn call_buf(&self, f: &str, input: Option<&str>) -> Result<String, String> {
        let (out, _) = self.call(f, input, true)?;
        Ok(out)
    }

    fn call(
        &self,
        f: &str,
        input: Option<&str>,
        returns_buf: bool,
    ) -> Result<(String, Vec<(i32, bool)>), String> {
        self.call_limited(f, input, returns_buf, FUEL, CALL_TIMEOUT_MS)
    }

    fn call_limited(
        &self,
        f: &str,
        input: Option<&str>,
        returns_buf: bool,
        fuel: u64,
        timeout_ms: u64,
    ) -> Result<(String, Vec<(i32, bool)>), String> {
        if self.tripped() {
            return Err(format!(
                "{}: disabled after {} failures",
                self.id, BREAKER_LIMIT
            ));
        }
        let r = self.call_inner(f, input, returns_buf, fuel, timeout_ms);
        match &r {
            Ok(_) => self.failures.set(0),
            Err(e) => {
                self.failures.set(self.failures.get() + 1);
                self.notes.borrow_mut().push(format!("{f}: {e}"));
            }
        }
        r
    }

    fn call_inner(
        &self,
        f: &str,
        input: Option<&str>,
        returns_buf: bool,
        fuel: u64,
        timeout_ms: u64,
    ) -> Result<(String, Vec<(i32, bool)>), String> {
        let ctx = HostCtx {
            limits: StoreLimitsBuilder::new().memory_size(MEM_CAP).build(),
            deadline: Instant::now() + Duration::from_millis(timeout_ms),
            ops: Vec::new(),
            logs: Vec::new(),
            doc_path: self.doc_path.borrow().clone(),
        };
        let mut store = Store::new(&self.engine, ctx);
        store.limiter(|c| &mut c.limits);
        store.set_fuel(fuel).map_err(|e| e.to_string())?;
        let l = linker(&self.engine).map_err(|e| e.to_string())?;
        let instance = l
            .instantiate_and_start(&mut store, &self.module)
            .map_err(|e| e.to_string())?;
        let mem = instance
            .get_memory(&store, "memory")
            .ok_or("memory export missing")?;
        let arg = match input {
            Some(text) => {
                let alloc = instance
                    .get_typed_func::<i32, i32>(&store, "nx_alloc")
                    .map_err(|_| "nx_alloc export missing")?;
                let ptr = alloc
                    .call(&mut store, text.len() as i32)
                    .map_err(|e| e.to_string())? as usize;
                let mut buf = Vec::with_capacity(text.len() + 4);
                buf.extend_from_slice(&(text.len() as u32).to_le_bytes());
                buf.extend_from_slice(text.as_bytes());
                mem.write(&mut store, ptr, &buf)
                    .map_err(|e| e.to_string())?;
                Some(ptr as i32)
            }
            None => None,
        };
        let ret = match arg {
            Some(p) => instance
                .get_typed_func::<i32, i32>(&store, f)
                .map_err(|_| format!("{f} export missing"))?
                .call(&mut store, p)
                .map_err(|e| e.to_string())?,
            None => instance
                .get_typed_func::<(), i32>(&store, f)
                .map_err(|_| format!("{f} export missing"))?
                .call(&mut store, ())
                .map_err(|e| e.to_string())?,
        };
        let out = if returns_buf {
            read_buf(mem.data(&store), ret as u32).ok_or("return buffer corrupt")?
        } else {
            ret.to_string()
        };
        let data = store.into_data();
        if !data.logs.is_empty() {
            let mut notes = self.notes.borrow_mut();
            for l in data.logs {
                notes.push(format!("[{}] {l}", self.id));
            }
        }
        Ok((out, data.ops))
    }
}

impl Extension for WasmExtension {
    fn id(&self) -> &str {
        &self.id
    }
    fn settings_prefix(&self) -> &str {
        &self.prefix
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn commands(&self) -> Vec<Command> {
        self.commands.clone()
    }
    fn menus(&self) -> Vec<MenuContribution> {
        self.menus.clone()
    }
    fn on_settings(&mut self, settings: &Settings) -> ExtensionEffect {
        let input = settings_json(settings, &self.prefix);
        self.call_buf("nx_ext_settings", Some(&input))
            .and_then(|t| effect_from_json(&t))
            .unwrap_or_default()
    }
    fn disabled_effect(&self) -> ExtensionEffect {
        self.call_buf("nx_ext_disabled", None)
            .and_then(|t| effect_from_json(&t))
            .unwrap_or_default()
    }
    fn run_with(&mut self, id: &str, ed: &mut dyn EditorOps, doc_path: &str) -> bool {
        *self.doc_path.borrow_mut() = doc_path.to_string();
        let r = self.run(id, ed);
        self.doc_path.borrow_mut().clear();
        r
    }
    fn run(&mut self, id: &str, ed: &mut dyn EditorOps) -> bool {
        let Ok((ret, ops)) = self.call("nx_ext_run", Some(id), false) else {
            return false;
        };
        let handled = ret != "0";
        // 게스트가 요청한 편집기 op를 이제 적용한다(SDK `op::*`와 같은 번호표).
        let mut any = false;
        for (op, flag) in ops {
            any |= match op {
                1 => ed.goto_bracket(flag),
                2 => ed.expand_to_brackets(),
                3 => ed.goto_bracket_sibling(false, flag),
                4 => ed.goto_bracket_sibling(true, flag),
                5 => ed.goto_bracket_parent(flag),
                6 => ed.goto_bracket_child(flag),
                _ => false,
            };
        }
        handled && any
    }
    fn is_wasm(&self) -> bool {
        true
    }
    fn as_wasm(&self) -> Option<&WasmExtension> {
        Some(self)
    }
    fn formatter(&self) -> Option<(Label, String)> {
        self.formatter.clone()
    }
    fn formatter_marks(&self) -> Vec<(String, String)> {
        self.formatter_marks.clone()
    }
    fn format(&self, input_json: &str) -> Result<String, String> {
        if input_json.len() > OUT_CAP {
            return Err(format!("text over {} KB", OUT_CAP >> 10));
        }
        let (out, _) = self.call_limited(
            "nx_ext_format",
            Some(input_json),
            true,
            FORMAT_FUEL,
            FORMAT_TIMEOUT_MS,
        )?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 소스 트리의 공식 패키지 `.wasm`(SDK 샘플로 빌드해 둔 것)을 로드해 메타·설정→효과·명령 큐를 확인한다.
    #[test]
    fn loads_rainbow_pairs_wasm_and_round_trips() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../extensions/rainbow-pairs/rainbow_pairs.wasm");
        let ext = WasmExtension::load_file(&path).expect("load wasm");
        assert_eq!(ext.id(), "rainbow-pairs");
        assert_eq!(ext.settings_prefix(), "ext.rainbow_pairs.");
        assert_eq!(ext.commands().len(), 6);
        assert_eq!(ext.menus()[0].items.len(), 6);
        assert!(
            matches!(&ext.commands()[0].label, Label::Text(en, Some(ko)) if !en.is_empty() && !ko.is_empty())
        );
        // 설정 → 효과(색 층만).
        let input = r##"{"ext.rainbow_pairs.enabled":"on","ext.rainbow_pairs.unmatched":"off","ext.rainbow_pairs.colors":"#FF0000,#00FF00","ext.rainbow_pairs.max_kb":"0"}"##;
        let out = ext
            .call_buf("nx_ext_settings", Some(input))
            .expect("settings call");
        let e = effect_from_json(&out)
            .expect("effect")
            .bracket_opts
            .expect("bracket");
        assert!(e.rainbow && !e.unmatched && e.max_chars == usize::MAX);
        assert_eq!(e.colors.len(), 2);
        // 꺼짐 = 색 없음.
        let off = ext.disabled_effect().bracket_opts.expect("off");
        assert!(!off.rainbow && !off.unmatched);
        // 명령 = op 큐.
        let (ret, ops) = ext
            .call("nx_ext_run", Some("edit.bracket_next"), false)
            .expect("run");
        assert_eq!((ret.as_str(), ops), ("1", vec![(4, false)]));
        let (ret, ops) = ext.call("nx_ext_run", Some("nope"), false).expect("run");
        assert_eq!((ret.as_str(), ops.len()), ("0", 0));
        assert!(ext.notes.borrow().is_empty(), "오류 0");
    }

    #[test]
    fn effect_json_defaults_and_settings_json_prefix() {
        let e = effect_from_json("{}").expect("empty");
        assert!(e.bracket_opts.is_none());
        let e = effect_from_json(r##"{"bracket":{"colors":["#123456","zz"],"max_chars":100}}"##)
            .expect("partial")
            .bracket_opts
            .expect("b");
        assert!(e.rainbow && e.unmatched && e.max_chars == 100 && e.colors.len() == 1);
        let s = Settings::from_text(std::path::PathBuf::from("x"), "");
        let j = settings_json(&s, "ext.rainbow_pairs.");
        assert!(j.contains("\"ext.rainbow_pairs.enabled\"") && !j.contains("\"editor."));
    }
}
