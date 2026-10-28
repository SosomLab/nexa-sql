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

use super::{
    key_matches, Capabilities, Command, EditOp, EditorOps, Extension, ExtensionEffect, FileOp,
    Label, MenuContribution, MenuTarget, Prompt, PromptRow, SettingDef, SettingDefKind,
    TransformOut, TransformReq,
};
use nsql_settings::json::{self, Json};
use nsql_settings::Settings;
use std::cell::Cell;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wasmi::{Caller, Engine, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

/// 호출당 연료(인터프리터 명령 수) — 정책 층은 계산이 거의 없다(설정 파싱 · JSON).
const FUEL: u64 = 50_000_000;
/// 선형 메모리 상한.
const MEM_CAP: usize = 16 * 1024 * 1024;
/// 호출당 벽시계 상한(UI 스레드).
const CALL_TIMEOUT_MS: u64 = 200;
/// ★ 포맷 호출(ABI v1.1 `nx_ext_format`)은 문서 크기에 비례해 일한다 — 연료·시간 상한을 따로 둔다(1 MB 문서 기준).
const FORMAT_FUEL: u64 = 2_000_000_000;
const FORMAT_TIMEOUT_MS: u64 = 5_000;
/// ★ 프롬프트 변환 호출(ABI v1.2 `nx_ext_transform` · 148차)은 키 입력마다 UI 스레드에서 돈다 — 값 n개(다중 선택 상한)라 기본보다 넉넉히 · 0.5초.
const TRANSFORM_FUEL: u64 = 200_000_000;
const TRANSFORM_TIMEOUT_MS: u64 = 500;
/// 반환 버퍼 상한.
const OUT_CAP: usize = 1 << 20;
/// 모듈 크기 상한.
const MODULE_CAP: usize = 8 * 1024 * 1024;
/// 연속 실패 격리 횟수.
const BREAKER_LIMIT: u32 = 3;
/// ★ 길 A(154차) 상한: 확장 전용 상태 JSON · 호출당 편집 요청 수·바이트 · 본문 읽기 1회 상한.
const STORAGE_CAP: usize = 64 * 1024;
/// ★ 사건 배달(154차-b): 처리기 한 번의 연료·시간 — 키 입력 경로에 얹히므로 짧게.
const EVENT_FUEL: u64 = 20_000_000;
const EVENT_TIMEOUT_MS: u64 = 50;
const EDIT_OPS_CAP: usize = 256;
const EDIT_BYTES_CAP: usize = 1 << 20;
const DOC_READ_CAP: usize = 1 << 20;

/// 호스트 상태(호출 하나 동안) — 리미터 · 마감 · 게스트가 요청한 편집기 op · 로그.
struct HostCtx {
    limits: StoreLimits,
    deadline: Instant,
    ops: Vec<(i32, bool)>,
    logs: Vec<String>,
    /// ★ 호스트 상황(09-30 · docs/97): 활성 문서 경로 — `nx_host_get(DOC_PATH)`가 돌려준다.
    doc_path: String,
    /// ★ 파일 동작 요청(146차 · `nx_file_op`): (종류, 경로) · 호출당 16개까지.
    file_ops: Vec<(i32, String)>,
    /// ★ 이번 호출에서 읽을 수 있는 설정(전용 접두 + 메타 `settings_read` 선언 키 · 148차) — `nx_setting_get`이 여기서만 답한다.
    settings: Vec<(String, String)>,
    /// ★ 설정 덮어쓰기 요청(148차 · `nx_setting_set`): (키, 값) · 호출당 16개까지 · 호스트가 호출 뒤 선언·검증 후 overlay 층에.
    setting_sets: Vec<(String, String)>,
    /// ★ 길 A(154차): 본문 스냅샷(글자 열 · `editor.read` 선언 때만 채움) · 구간 · 읽기/쓰기 허용 · 편집 요청 큐 · 상태줄 · 전용 상태.
    doc: Arc<Vec<char>>,
    regions: Vec<(usize, usize)>,
    can_read: bool,
    can_write: bool,
    edit_ops: Vec<EditOp>,
    edit_bytes: usize,
    status: Option<String>,
    storage: Option<String>,
    storage_dirty: bool,
}

/// 게스트 버퍼(길이 접두)에 바이트를 써 준다 → 쓴 길이(-1 = cap 부족·메모리 없음).
fn write_buf(caller: &mut Caller<'_, HostCtx>, ptr: i32, cap: i32, bytes: &[u8]) -> i32 {
    if bytes.len() > cap.max(0) as usize {
        return -1;
    }
    let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
        return -1;
    };
    let mut buf = Vec::with_capacity(bytes.len() + 4);
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
    if mem.write(caller, ptr as usize, &buf).is_err() {
        return -1;
    }
    bytes.len() as i32
}

/// `[[a,b],…]` JSON → 구간 목록(모양이 다르면 None).
fn regions_from_json(text: &str) -> Option<Vec<(usize, usize)>> {
    let v = json::parse(text).ok()?;
    let Json::Arr(items) = v else {
        return None;
    };
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        let Json::Arr(ab) = it else {
            return None;
        };
        if ab.len() != 2 {
            return None;
        }
        let a = jnum(&ab[0])?;
        let b = jnum(&ab[1])?;
        if a < 0.0 || b < a {
            return None;
        }
        out.push((a as usize, b as usize));
    }
    Some(out)
}

/// `nx_host_get`의 종류 번호(SDK `query::*`와 같은 표).
const HOST_GET_DOC_PATH: i32 = 1;
/// `nx_file_op`의 종류 번호(SDK `file_op::*`와 같은 표 · 146차 · docs/97 §5 ②).
const FILE_OP_OPEN: i32 = 1;
const FILE_OP_REVEAL: i32 = 2;
const FILE_OP_COPY_PATH: i32 = 3;

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
    // ★ nx_file_op(kind, ptr) -> i32 : 파일 동작 요청(큐 · 146차) — 1 열기 · 2 탐색기에서 드러내기 · 3 경로 복사 · 호출이 끝난 뒤 호스트가 적용.
    l.func_wrap(
        "env",
        "nx_file_op",
        |mut caller: Caller<'_, HostCtx>, kind: i32, ptr: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 20_000)?;
            if !matches!(kind, FILE_OP_OPEN | FILE_OP_REVEAL | FILE_OP_COPY_PATH)
                || caller.data().file_ops.len() >= 16
            {
                return Ok(0);
            }
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            let path = read_buf(mem.data(&caller), ptr as u32).unwrap_or_default();
            if path.trim().is_empty() {
                return Ok(0);
            }
            caller.data_mut().file_ops.push((kind, path));
            Ok(1)
        },
    )?;
    // ★ nx_setting_get(key, ptr, cap) -> i32 : 설정 값 하나(전용 키 + 선언한 기본 키만 · 148차) → 게스트 버퍼 · 반환 = 길이(-1 = 없음·선언 밖).
    l.func_wrap(
        "env",
        "nx_setting_get",
        |mut caller: Caller<'_, HostCtx>,
         key: i32,
         ptr: i32,
         cap: i32|
         -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 20_000)?;
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(-1);
            };
            let k = read_buf(mem.data(&caller), key as u32).unwrap_or_default();
            let Some(value) = caller
                .data()
                .settings
                .iter()
                .find(|(sk, _)| *sk == k)
                .map(|(_, v)| v.clone())
            else {
                return Ok(-1);
            };
            let bytes = value.as_bytes();
            if bytes.len() > cap.max(0) as usize {
                return Ok(-1);
            }
            let mut buf = Vec::with_capacity(bytes.len() + 4);
            buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(bytes);
            if mem.write(&mut caller, ptr as usize, &buf).is_err() {
                return Ok(-1);
            }
            Ok(bytes.len() as i32)
        },
    )?;
    // ★ nx_setting_set(key, val) -> i32 : 설정 덮어쓰기 요청(큐 · 148차) — 호출이 끝난 뒤 호스트가 메타 `settings_write` 선언·레지스트리 검증을
    //   거쳐 **런타임 overlay 층**에 넣는다(파일에 저장 안 함 · 확장이 꺼지면 원복). 반환 1 = 접수.
    l.func_wrap(
        "env",
        "nx_setting_set",
        |mut caller: Caller<'_, HostCtx>, key: i32, val: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 20_000)?;
            if caller.data().setting_sets.len() >= 16 {
                return Ok(0);
            }
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            let k = read_buf(mem.data(&caller), key as u32).unwrap_or_default();
            let v = read_buf(mem.data(&caller), val as u32).unwrap_or_default();
            if k.trim().is_empty() {
                return Ok(0);
            }
            caller.data_mut().setting_sets.push((k, v));
            Ok(1)
        },
    )?;
    // ★ 길 A(ABI v1.3 · 154차) 편집기 읽기: nx_editor_len() · nx_editor_text(from,to,ptr,cap) · nx_editor_regions(ptr,cap) — `editor.read` 선언 때만 답한다(아니면 -1).
    l.func_wrap(
        "env",
        "nx_editor_len",
        |mut caller: Caller<'_, HostCtx>| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 1_000)?;
            if !caller.data().can_read {
                return Ok(-1);
            }
            Ok(caller.data().doc.len() as i32)
        },
    )?;
    l.func_wrap(
        "env",
        "nx_editor_text",
        |mut caller: Caller<'_, HostCtx>,
         from: i32,
         to: i32,
         ptr: i32,
         cap: i32|
         -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 20_000)?;
            if !caller.data().can_read || from < 0 || to < from {
                return Ok(-1);
            }
            let (a, b) = (from as usize, to as usize);
            if b > caller.data().doc.len() || b - a > DOC_READ_CAP {
                return Ok(-1);
            }
            let text: String = caller.data().doc[a..b].iter().collect();
            Ok(write_buf(&mut caller, ptr, cap, text.as_bytes()))
        },
    )?;
    l.func_wrap(
        "env",
        "nx_editor_regions",
        |mut caller: Caller<'_, HostCtx>, ptr: i32, cap: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 5_000)?;
            if !caller.data().can_read {
                return Ok(-1);
            }
            let text = format!(
                "[{}]",
                caller
                    .data()
                    .regions
                    .iter()
                    .map(|(a, b)| format!("[{a},{b}]"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            Ok(write_buf(&mut caller, ptr, cap, text.as_bytes()))
        },
    )?;
    // ★ 길 A 편집기 쓰기(큐 · `editor.write` 선언 때만 접수 · 호출당 256건 · 1 MB): nx_editor_replace(from,to,ptr) · nx_editor_set_regions(ptr).
    l.func_wrap(
        "env",
        "nx_editor_replace",
        |mut caller: Caller<'_, HostCtx>,
         from: i32,
         to: i32,
         ptr: i32|
         -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 20_000)?;
            if !caller.data().can_write
                || from < 0
                || to < from
                || caller.data().edit_ops.len() >= EDIT_OPS_CAP
            {
                return Ok(0);
            }
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            let Some(text) = read_buf(mem.data(&caller), ptr as u32) else {
                return Ok(0);
            };
            if caller.data().edit_bytes + text.len() > EDIT_BYTES_CAP {
                return Ok(0);
            }
            let d = caller.data_mut();
            d.edit_bytes += text.len();
            d.edit_ops
                .push(EditOp::Replace(from as usize, to as usize, text));
            Ok(1)
        },
    )?;
    l.func_wrap(
        "env",
        "nx_editor_set_regions",
        |mut caller: Caller<'_, HostCtx>, ptr: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 10_000)?;
            if !caller.data().can_write || caller.data().edit_ops.len() >= EDIT_OPS_CAP {
                return Ok(0);
            }
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            let text = read_buf(mem.data(&caller), ptr as u32).unwrap_or_default();
            let Some(r) = regions_from_json(&text) else {
                return Ok(0);
            };
            caller.data_mut().edit_ops.push(EditOp::SetRegions(r));
            Ok(1)
        },
    )?;
    // ★ nx_ui_status(ptr) : 상태줄 글(호출당 마지막 것).
    l.func_wrap(
        "env",
        "nx_ui_status",
        |mut caller: Caller<'_, HostCtx>, ptr: i32| -> Result<(), wasmi::Error> {
            host_guard(&mut caller, 5_000)?;
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(());
            };
            let text = read_buf(mem.data(&caller), ptr as u32).unwrap_or_default();
            caller.data_mut().status = Some(text.chars().take(256).collect());
            Ok(())
        },
    )?;
    // ★ 전용 상태(storage · `<root>/<id>/state.json` · 64 KB): nx_storage_get(ptr,cap) · nx_storage_set(ptr) — 호출 뒤 호스트가 파일에.
    l.func_wrap(
        "env",
        "nx_storage_get",
        |mut caller: Caller<'_, HostCtx>, ptr: i32, cap: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 5_000)?;
            let Some(v) = caller.data().storage.clone() else {
                return Ok(-1);
            };
            Ok(write_buf(&mut caller, ptr, cap, v.as_bytes()))
        },
    )?;
    l.func_wrap(
        "env",
        "nx_storage_set",
        |mut caller: Caller<'_, HostCtx>, ptr: i32| -> Result<i32, wasmi::Error> {
            host_guard(&mut caller, 10_000)?;
            let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                return Ok(0);
            };
            let Some(text) = read_buf(mem.data(&caller), ptr as u32) else {
                return Ok(0);
            };
            if text.len() > STORAGE_CAP {
                return Ok(0);
            }
            let d = caller.data_mut();
            d.storage = Some(text);
            d.storage_dirty = true;
            Ok(1)
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
    /// ★ 메타 `settings`(146차 · T-258): 호스트가 런타임 레지스트리에 등재.
    settings: Vec<SettingDef>,
    /// ★ 메타 `prompts`(ABI v1.2 · 148차): 라이브 프롬프트 선언 — 호스트가 명령으로 등록하고 `nx_ext_transform`으로 값을 받는다.
    prompts: Vec<Prompt>,
    /// ★ 메타 `settings_read` / `settings_write`(148차): 읽는·덮어쓰는 기본 설정 키 패턴.
    settings_read: Vec<String>,
    settings_write: Vec<String>,
    /// ★ 모듈이 import하는 호스트 함수 이름(T-448 · 능력 요약 = 파일 동작·설정 덮어쓰기 사용 여부).
    imports: Vec<String>,
    /// ★ 길 A(154차): 메타 `stateful`·`capabilities`·`events` · 보존 인스턴스(stateful) · 마지막 호출 시각(유휴 회수) · 호출 전 본문 스냅샷 ·
    ///   편집 요청·상태줄 큐 · 전용 상태(바깥 None = 아직 안 읽음).
    stateful: bool,
    caps_decl: Vec<String>,
    events: Vec<String>,
    live: std::cell::RefCell<Option<Live>>,
    last_call: Cell<Option<Instant>>,
    doc_snapshot: std::cell::RefCell<Snapshot>,
    edit_ops: std::cell::RefCell<Vec<EditOp>>,
    status: std::cell::RefCell<Option<String>>,
    storage: std::cell::RefCell<Option<Option<String>>>,
    /// 전용 상태 파일의 뿌리(호스트가 `set_storage_root`로 넣는다 · None = storage 없음 = 시험·개발 폴더 로드).
    storage_root: std::cell::RefCell<Option<std::path::PathBuf>>,
    /// 이번 호출에서 게스트가 읽을 수 있는 설정(전용 + 선언) — 호출 직전에 채운다.
    call_settings: std::cell::RefCell<Vec<(String, String)>>,
    /// 마지막 호출이 남긴 설정 덮어쓰기 요청.
    setting_sets: std::cell::RefCell<Vec<(String, String)>>,
    /// ★ 마지막 호출이 남긴 파일 동작 요청(146차).
    file_ops: std::cell::RefCell<Vec<FileOp>>,
    /// ★ 이번 `run_with` 호출의 호스트 상황(활성 문서 경로 · docs/97) — 호출 동안만.
    doc_path: std::cell::RefCell<String>,
    engine: Engine,
    module: Module,
    /// 연속 실패 수(브레이커).
    failures: Cell<u32>,
    /// 마지막 호출의 로그·오류(호스트가 거둬 로그 창에).
    pub(crate) notes: std::cell::RefCell<Vec<String>>,
}

/// 본문 스냅샷(글자 열 · 구간) — `editor.read` 호출 전에 뜬다.
type Snapshot = (Arc<Vec<char>>, Vec<(usize, usize)>);

/// 보존 인스턴스(stateful · 길 A): Store(HostCtx는 호출마다 갈아 끼운다) + Instance.
struct Live {
    store: Store<HostCtx>,
    instance: Instance,
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
pub(crate) fn settings_json(s: &Settings, prefix: &str, reads: &[String]) -> String {
    let items: Vec<(String, Json)> = settings_pairs(s, prefix, reads)
        .into_iter()
        .map(|(k, v)| (k, Json::Str(v)))
        .collect();
    json::dump(&Json::Obj(items))
}

/// 게스트에 주는 설정 (키, 값): 전용 접두 키(정적 + 동적 메타 `settings` · 146차) + 메타 `settings_read` 패턴에 맞는 기본 키(148차).
pub(crate) fn settings_pairs(
    s: &Settings,
    prefix: &str,
    reads: &[String],
) -> Vec<(String, String)> {
    nsql_settings::entries_all()
        .into_iter()
        .filter(|e| {
            (!prefix.is_empty() && e.key.starts_with(prefix))
                || reads.iter().any(|pat| key_matches(pat, e.key))
        })
        .map(|e| {
            (
                e.key.to_string(),
                s.get(e.key).unwrap_or(e.default).to_string(),
            )
        })
        .collect()
}

/// ★ 변환 요청 JSON(ABI v1.2 · 148차): `{"id","text","n","regions":[{"line","text"}],"lang","doc_path","settings":{…}}`.
pub(crate) fn transform_request_json(req: &TransformReq, settings_json: &str) -> String {
    let regions: Vec<Json> = req
        .regions
        .iter()
        .map(|(l, t)| {
            Json::Obj(vec![
                ("line".into(), Json::Num(*l as f64)),
                ("text".into(), Json::Str(t.clone())),
            ])
        })
        .collect();
    let settings = json::parse(settings_json).unwrap_or(Json::Obj(Vec::new()));
    json::dump(&Json::Obj(vec![
        ("id".into(), Json::Str(req.id.clone())),
        ("text".into(), Json::Str(req.text.clone())),
        ("n".into(), Json::Num(req.n as f64)),
        ("regions".into(), Json::Arr(regions)),
        ("lang".into(), Json::Str(req.lang.clone())),
        ("doc_path".into(), Json::Str(req.doc_path.clone())),
        ("phase".into(), Json::Str(req.phase.clone())),
        ("selection".into(), Json::Str(req.selection.clone())),
        (
            "pick".into(),
            req.pick.clone().map_or(Json::Null, Json::Str),
        ),
        ("settings".into(), settings),
    ]))
}

/// 변환 응답 JSON → 값/오류/행(`{"values":[…]}` 또는 `{"error":"…"}` · 선택 `"rows":[{"text","warn"}]`).
pub(crate) fn transform_out_from_json(text: &str) -> Result<TransformOut, String> {
    let v = json::parse(text).map_err(|e| format!("transform json: {e}"))?;
    let error = jget(&v, "error").and_then(jstr).map(str::to_string);
    let values: Vec<String> = jget(&v, "values")
        .map(jarr)
        .unwrap_or(&[])
        .iter()
        .map(|x| jstr(x).unwrap_or("").to_string())
        .collect();
    let rows: Vec<PromptRow> = jget(&v, "rows")
        .map(jarr)
        .unwrap_or(&[])
        .iter()
        .filter_map(|r| {
            let text = jget(r, "text").and_then(jstr)?.to_string();
            Some(PromptRow {
                text,
                warn: jget(r, "warn").and_then(jbool).unwrap_or(false),
                id: jget(r, "id").and_then(jstr).map(str::to_string),
            })
        })
        .collect();
    let apply = jget(&v, "apply")
        .and_then(jstr)
        .unwrap_or("regions")
        .to_string();
    let text = jget(&v, "text").and_then(jstr).unwrap_or("").to_string();
    if error.is_none() && jget(&v, "values").is_none() && apply == "regions" {
        return Err("transform: neither values nor error".to_string());
    }
    Ok(TransformOut {
        values,
        error,
        rows,
        apply,
        text,
    })
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
        let imports: Vec<String> = module.imports().map(|i| i.name().to_string()).collect();
        let mut ext = WasmExtension {
            id: String::new(),
            name: String::new(),
            prefix: String::new(),
            commands: Vec::new(),
            menus: Vec::new(),
            formatter: None,
            formatter_marks: Vec::new(),
            settings: Vec::new(),
            prompts: Vec::new(),
            settings_read: Vec::new(),
            settings_write: Vec::new(),
            imports,
            stateful: false,
            caps_decl: Vec::new(),
            events: Vec::new(),
            live: std::cell::RefCell::new(None),
            last_call: Cell::new(None),
            doc_snapshot: std::cell::RefCell::new((Arc::new(Vec::new()), Vec::new())),
            edit_ops: std::cell::RefCell::new(Vec::new()),
            status: std::cell::RefCell::new(None),
            storage: std::cell::RefCell::new(None),
            storage_root: std::cell::RefCell::new(None),
            call_settings: std::cell::RefCell::new(Vec::new()),
            setting_sets: std::cell::RefCell::new(Vec::new()),
            file_ops: std::cell::RefCell::new(Vec::new()),
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
                let key = jget(c, "key").and_then(jstr).map(str::to_string);
                let key_mac = jget(c, "key_mac").and_then(jstr).map(str::to_string);
                Some(Command {
                    id,
                    label: label_of(jget(c, "label")),
                    key,
                    key_mac,
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
                let target = match jget(m, "target").and_then(jstr) {
                    Some("project_explorer") => MenuTarget::ProjectExplorer,
                    _ => MenuTarget::Editor,
                };
                Some(MenuContribution {
                    id,
                    label: label_of(jget(m, "label")),
                    items,
                    target,
                })
            })
            .collect();
        // ★ 설정 항목(146차 · T-258): `{"key","label","desc","kind","default"}` · kind = "bool" | "text" | {"int":{"min","max"}} | {"choice":[{"value","label"}]}.
        ext.settings = jget(&v, "settings")
            .map(jarr)
            .unwrap_or(&[])
            .iter()
            .filter_map(|d| {
                let key = jget(d, "key").and_then(jstr)?.to_string();
                if !key.starts_with(&ext.prefix) || ext.prefix.is_empty() {
                    return None;
                }
                let kind = match jget(d, "kind") {
                    Some(Json::Str(s)) if s == "bool" => SettingDefKind::Bool,
                    Some(Json::Str(s)) if s == "text" => SettingDefKind::Text,
                    Some(o @ Json::Obj(_)) => {
                        if let Some(i) = jget(o, "int") {
                            SettingDefKind::Int {
                                min: jget(i, "min").and_then(jnum).unwrap_or(i64::MIN as f64)
                                    as i64,
                                max: jget(i, "max").and_then(jnum).unwrap_or(i64::MAX as f64)
                                    as i64,
                            }
                        } else if let Some(c) = jget(o, "choice") {
                            SettingDefKind::Choice(
                                jarr(c)
                                    .iter()
                                    .filter_map(|it| {
                                        let v = jget(it, "value").and_then(jstr)?.to_string();
                                        Some((v, label_of(jget(it, "label"))))
                                    })
                                    .collect(),
                            )
                        } else {
                            SettingDefKind::Text
                        }
                    }
                    _ => SettingDefKind::Text,
                };
                Some(SettingDef {
                    key,
                    label: label_of(jget(d, "label")),
                    desc: label_of(jget(d, "desc")),
                    kind,
                    default: jget(d, "default").and_then(jstr).unwrap_or("").to_string(),
                })
            })
            .collect();
        // ★ 프롬프트(ABI v1.2 · 148차): `{"id","label","placeholder","placeholder_key","hint","key","help":[{"expr","label"}]}` ·
        //   `commands`에 없는 id는 같은 라벨·키로 명령에 병합(팔레트·키맵이 그대로 안다).
        ext.prompts = jget(&v, "prompts")
            .map(jarr)
            .unwrap_or(&[])
            .iter()
            .filter_map(|p| {
                let id = jget(p, "id").and_then(jstr)?.to_string();
                Some(Prompt {
                    id,
                    label: label_of(jget(p, "label")),
                    placeholder: jget(p, "placeholder")
                        .and_then(jstr)
                        .unwrap_or("")
                        .to_string(),
                    placeholder_key: jget(p, "placeholder_key")
                        .and_then(jstr)
                        .map(str::to_string),
                    hint: label_of(jget(p, "hint")),
                    key: jget(p, "key").and_then(jstr).map(str::to_string),
                    help: jget(p, "help")
                        .map(jarr)
                        .unwrap_or(&[])
                        .iter()
                        .filter_map(|h| {
                            let e = jget(h, "expr").and_then(jstr)?.to_string();
                            Some((e, label_of(jget(h, "label"))))
                        })
                        .collect(),
                })
            })
            .collect();
        let strs = |k: &str| -> Vec<String> {
            jget(&v, k)
                .map(jarr)
                .unwrap_or(&[])
                .iter()
                .filter_map(jstr)
                .map(str::to_string)
                .collect()
        };
        ext.settings_read = strs("settings_read");
        ext.settings_write = strs("settings_write");
        // ★ 길 A(ABI v1.3 · 154차): 인스턴스 유지 · 능력 선언 · 구독 사건.
        ext.stateful = jget(&v, "stateful").and_then(jbool).unwrap_or(false);
        ext.caps_decl = strs("capabilities");
        ext.events = strs("events");
        for p in &ext.prompts {
            if !ext.commands.iter().any(|c| c.id == p.id) {
                ext.commands.push(Command {
                    id: p.id.clone(),
                    label: p.label.clone(),
                    key: p.key.clone(),
                    key_mac: None,
                });
            }
        }
        Ok(ext)
    }

    /// 파일에서 로드.
    pub(crate) fn load_file(path: &Path) -> Result<WasmExtension, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::load(&bytes)
    }

    /// 게스트가 읽을 수 있는 기본 설정 패턴 = `settings_read` ∪ `settings_write`(덮어쓸 키는 지금 값도 읽어야 토글할 수 있다 · 148차).
    fn readable(&self) -> Vec<String> {
        let mut v = self.settings_read.clone();
        v.extend(self.settings_write.iter().cloned());
        v
    }

    #[cfg(test)]
    fn take_wasm_notes_for_test(&self) -> Vec<String> {
        std::mem::take(&mut *self.notes.borrow_mut())
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

    fn has_cap(&self, cap: &str) -> bool {
        self.caps_decl.iter().any(|c| c == cap)
    }

    /// 전용 상태 파일(`<root>/<id>/state.json`).
    fn storage_path(&self) -> Option<std::path::PathBuf> {
        self.storage_root
            .borrow()
            .as_ref()
            .map(|r| r.join(&self.id).join("state.json"))
    }

    /// 전용 상태 파일의 뿌리(확장 관리자 설치 폴더 · 호스트가 로드 뒤 넣는다 · 단위 시험은 넣지 않아 실제 설정 폴더에 쓰지 않는다).
    pub(crate) fn set_storage_root(&self, root: Option<std::path::PathBuf>) {
        *self.storage_root.borrow_mut() = root;
        *self.storage.borrow_mut() = None;
    }

    /// 전용 상태(처음 한 번 파일에서 · 이후 캐시).
    fn storage_value(&self) -> Option<String> {
        let mut slot = self.storage.borrow_mut();
        if slot.is_none() {
            let v = self
                .storage_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .filter(|t| t.len() <= STORAGE_CAP);
            *slot = Some(v);
        }
        slot.clone().flatten()
    }

    /// ★ 유휴 회수(길 A · 39 §3): 마지막 호출에서 `idle`이 지난 보존 인스턴스를 버린다(상태는 storage에 둔 것만 남는다) → 로그 한 줄.
    pub(crate) fn reap_idle(&self, idle: Duration) -> Option<String> {
        if self.live.borrow().is_none() {
            return None;
        }
        let last = self.last_call.get()?;
        if last.elapsed() < idle {
            return None;
        }
        *self.live.borrow_mut() = None;
        Some(format!(
            "wasm extension {}: idle instance released",
            self.id
        ))
    }

    /// ★ 사건 배달(154차-b): `nx_ext_event(JSON)` → 처리 여부. 사건 처리기가 낸 편집·파일·설정 요청은 **버린다**(관찰 전용 · 로그 한 줄).
    pub(crate) fn dispatch_event(&self, json: &str) -> bool {
        let handled = self
            .call_limited(
                "nx_ext_event",
                Some(json),
                false,
                EVENT_FUEL,
                EVENT_TIMEOUT_MS,
            )
            .map(|(r, _)| r != "0")
            .unwrap_or(false);
        let dropped = self.edit_ops.borrow_mut().drain(..).count()
            + self.file_ops.borrow_mut().drain(..).count()
            + self.setting_sets.borrow_mut().drain(..).count();
        if dropped > 0 {
            self.notes.borrow_mut().push(format!(
                "[{}] {dropped} request(s) from an event handler ignored (events are observe-only)",
                self.id
            ));
        }
        handled
    }

    /// (재)인스턴스화 — stateful이면 `nx_ext_init`도 1회.
    fn instantiate(&self, ctx: HostCtx, fuel: u64) -> Result<Live, String> {
        let mut store = Store::new(&self.engine, ctx);
        store.limiter(|c| &mut c.limits);
        store.set_fuel(fuel).map_err(|e| e.to_string())?;
        let l = linker(&self.engine).map_err(|e| e.to_string())?;
        let instance = l
            .instantiate_and_start(&mut store, &self.module)
            .map_err(|e| e.to_string())?;
        if self.stateful {
            if let Ok(init) = instance.get_typed_func::<(), ()>(&store, "nx_ext_init") {
                init.call(&mut store, ())
                    .map_err(|e| format!("nx_ext_init: {e}"))?;
            }
        }
        Ok(Live { store, instance })
    }

    fn call_inner(
        &self,
        f: &str,
        input: Option<&str>,
        returns_buf: bool,
        fuel: u64,
        timeout_ms: u64,
    ) -> Result<(String, Vec<(i32, bool)>), String> {
        let (doc, regions) = self.doc_snapshot.borrow().clone();
        let ctx = HostCtx {
            limits: StoreLimitsBuilder::new().memory_size(MEM_CAP).build(),
            deadline: Instant::now() + Duration::from_millis(timeout_ms),
            ops: Vec::new(),
            logs: Vec::new(),
            doc_path: self.doc_path.borrow().clone(),
            file_ops: Vec::new(),
            settings: self.call_settings.borrow().clone(),
            setting_sets: Vec::new(),
            doc,
            regions,
            can_read: self.has_cap("editor.read"),
            can_write: self.has_cap("editor.write"),
            edit_ops: Vec::new(),
            edit_bytes: 0,
            status: None,
            storage: if self.stateful {
                self.storage_value()
            } else {
                None
            },
            storage_dirty: false,
        };
        // ★ 길 A(154차): stateful이면 보존 인스턴스를 다시 쓴다(HostCtx만 갈아 끼움 · 연료 다시 채움) · 아니면 호출마다 새로.
        let mut live = match self.live.borrow_mut().take() {
            Some(mut l) if self.stateful => {
                *l.store.data_mut() = ctx;
                l.store.set_fuel(fuel).map_err(|e| e.to_string())?;
                l
            }
            _ => self.instantiate(ctx, fuel)?,
        };
        self.last_call.set(Some(Instant::now()));
        let r = self.call_live(&mut live, f, input, returns_buf);
        // 호출이 남긴 것 수거(성공·실패 공통 · 트랩 때도 로그는 거둔다 · 146차).
        let data = live.store.data_mut();
        let logs = std::mem::take(&mut data.logs);
        let file_ops = std::mem::take(&mut data.file_ops);
        let setting_sets = std::mem::take(&mut data.setting_sets);
        let edit_ops = std::mem::take(&mut data.edit_ops);
        let status = data.status.take();
        let storage = if data.storage_dirty {
            data.storage.clone()
        } else {
            None
        };
        data.storage_dirty = false;
        let ops = std::mem::take(&mut data.ops);
        if !logs.is_empty() {
            let mut notes = self.notes.borrow_mut();
            for l in logs {
                notes.push(format!("[{}] {l}", self.id));
            }
        }
        match r {
            Err(e) => {
                // 트랩·시간 초과 = 인스턴스 버림(다음 호출 때 다시 만들고 init).
                Err(e)
            }
            Ok(out) => {
                if self.stateful {
                    *self.live.borrow_mut() = Some(live);
                }
                if !file_ops.is_empty() {
                    let mut q = self.file_ops.borrow_mut();
                    for (kind, path) in &file_ops {
                        q.push(match *kind {
                            FILE_OP_OPEN => FileOp::Open(path.clone()),
                            FILE_OP_REVEAL => FileOp::Reveal(path.clone()),
                            _ => FileOp::CopyPath(path.clone()),
                        });
                    }
                }
                if !setting_sets.is_empty() {
                    self.setting_sets.borrow_mut().extend(setting_sets);
                }
                if !edit_ops.is_empty() {
                    self.edit_ops.borrow_mut().extend(edit_ops);
                }
                if status.is_some() {
                    *self.status.borrow_mut() = status;
                }
                if let Some(v) = storage {
                    if let Some(p) = self.storage_path() {
                        if let Some(d) = p.parent() {
                            let _ = std::fs::create_dir_all(d);
                        }
                        if let Err(e) = std::fs::write(&p, v.as_bytes()) {
                            self.notes
                                .borrow_mut()
                                .push(format!("[{}] storage write failed: {e}", self.id));
                        }
                    }
                    *self.storage.borrow_mut() = Some(Some(v));
                }
                Ok((out, ops))
            }
        }
    }

    /// 인스턴스에 입력 버퍼를 넣고 `f`를 부른다 → 반환 버퍼(또는 정수 글).
    fn call_live(
        &self,
        live: &mut Live,
        f: &str,
        input: Option<&str>,
        returns_buf: bool,
    ) -> Result<String, String> {
        let store = &mut live.store;
        let instance = &live.instance;
        let mem = instance
            .get_memory(&*store, "memory")
            .ok_or("memory export missing")?;
        let arg = match input {
            Some(text) => {
                let alloc = instance
                    .get_typed_func::<i32, i32>(&*store, "nx_alloc")
                    .map_err(|_| "nx_alloc export missing")?;
                let ptr = alloc
                    .call(&mut *store, text.len() as i32)
                    .map_err(|e| e.to_string())? as usize;
                let mut buf = Vec::with_capacity(text.len() + 4);
                buf.extend_from_slice(&(text.len() as u32).to_le_bytes());
                buf.extend_from_slice(text.as_bytes());
                mem.write(&mut *store, ptr, &buf)
                    .map_err(|e| e.to_string())?;
                Some(ptr as i32)
            }
            None => None,
        };
        let ret = match arg {
            Some(p) => instance
                .get_typed_func::<i32, i32>(&*store, f)
                .map_err(|_| format!("{f} export missing"))?
                .call(&mut *store, p)
                .map_err(|e| e.to_string())?,
            None => instance
                .get_typed_func::<(), i32>(&*store, f)
                .map_err(|_| format!("{f} export missing"))?
                .call(&mut *store, ())
                .map_err(|e| e.to_string())?,
        };
        if returns_buf {
            read_buf(mem.data(&*store), ret as u32)
                .ok_or_else(|| "return buffer corrupt".to_string())
        } else {
            Ok(ret.to_string())
        }
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
        *self.call_settings.borrow_mut() = settings_pairs(settings, &self.prefix, &self.readable());
        let input = settings_json(settings, &self.prefix, &self.readable());
        self.call_buf("nx_ext_settings", Some(&input))
            .and_then(|t| effect_from_json(&t))
            .unwrap_or_default()
    }
    fn disabled_effect(&self) -> ExtensionEffect {
        self.call_buf("nx_ext_disabled", None)
            .and_then(|t| effect_from_json(&t))
            .unwrap_or_default()
    }
    fn run_with(&mut self, id: &str, ed: &mut dyn EditorOps, doc_path: &str, s: &Settings) -> bool {
        *self.doc_path.borrow_mut() = doc_path.to_string();
        *self.call_settings.borrow_mut() = settings_pairs(s, &self.prefix, &self.readable());
        // ★ 길 A(154차): `editor.read`를 선언한 확장에만 본문·구간 스냅샷(호출 한 번 동안).
        if self.has_cap("editor.read") {
            *self.doc_snapshot.borrow_mut() = ed.snapshot();
        }
        let r = self.run(id, ed);
        self.doc_path.borrow_mut().clear();
        *self.doc_snapshot.borrow_mut() = (Arc::new(Vec::new()), Vec::new());
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
        // ★ 반환 = 게스트가 **처리했는가**(146차): 편집기 op가 없는 명령(로그만 · 파일 op)도 처리됐으면 true — 종전 `handled && any`는
        //   그런 명령을 "미처리"로 돌려줬다(호스트는 다시 그리기 여부에만 쓴다).
        let _ = any;
        handled
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
    fn settings_defs(&self) -> Vec<SettingDef> {
        self.settings.clone()
    }
    fn take_file_ops(&self) -> Vec<FileOp> {
        std::mem::take(&mut self.file_ops.borrow_mut())
    }
    fn prompts(&self) -> Vec<Prompt> {
        self.prompts.clone()
    }
    fn settings_read(&self) -> Vec<String> {
        self.settings_read.clone()
    }
    fn settings_write(&self) -> Vec<String> {
        self.settings_write.clone()
    }
    fn take_setting_sets(&self) -> Vec<(String, String)> {
        std::mem::take(&mut self.setting_sets.borrow_mut())
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            commands: self.commands.iter().map(|c| c.id.clone()).collect(),
            prompts: self.prompts.iter().map(|p| p.id.clone()).collect(),
            settings_read: self.settings_read.clone(),
            settings_write: self.settings_write.clone(),
            file_ops: self.imports.iter().any(|n| n == "nx_file_op"),
            setting_set: self.imports.iter().any(|n| n == "nx_setting_set"),
            stateful: self.stateful,
            editor_read: self.has_cap("editor.read"),
            editor_write: self.has_cap("editor.write"),
            events: self.events.clone(),
        }
    }
    fn stateful(&self) -> bool {
        self.stateful
    }
    fn declared_caps(&self) -> Vec<String> {
        self.caps_decl.clone()
    }
    fn events(&self) -> Vec<String> {
        self.events.clone()
    }
    fn take_edit_ops(&self) -> Vec<EditOp> {
        std::mem::take(&mut self.edit_ops.borrow_mut())
    }
    fn take_status(&self) -> Option<String> {
        self.status.borrow_mut().take()
    }
    /// ★ 프롬프트 변환(ABI v1.2): 요청 JSON(+ 접두 설정) → `nx_ext_transform` → 값/오류/행.
    fn transform(&self, req: &TransformReq, s: &Settings) -> Result<TransformOut, String> {
        *self.call_settings.borrow_mut() = settings_pairs(s, &self.prefix, &self.readable());
        let input = transform_request_json(req, &settings_json(s, &self.prefix, &self.readable()));
        if input.len() > OUT_CAP {
            return Err(format!("request over {} KB", OUT_CAP >> 10));
        }
        *self.doc_path.borrow_mut() = req.doc_path.clone();
        let r = self.call_limited(
            "nx_ext_transform",
            Some(&input),
            true,
            TRANSFORM_FUEL,
            TRANSFORM_TIMEOUT_MS,
        );
        self.doc_path.borrow_mut().clear();
        transform_out_from_json(&r?.0)
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

    /// ★ 148차(ABI v1.2): 공식 패키지 `insert-sequence` wasm = 프롬프트 메타(id · 자리표시자 키 · 치트 시트 · 명령 병합 + 키) · 변환 = 값 n개 ·
    ///   오류(한국어 요청 = 한국어 문구) · 요청/응답 JSON 왕복. 파일이 없으면(아직 안 빌드) 건너뛴다.
    #[test]
    fn insert_sequence_wasm_prompts_and_transform() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../extensions/insert-sequence/insert_sequence.wasm");
        if !path.exists() {
            eprintln!("skip: {} not built", path.display());
            return;
        }
        let ext = WasmExtension::load_file(&path).expect("load");
        assert_eq!(ext.id(), "insert-sequence");
        let p = ext.prompts();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].id, "edit.insert_sequence");
        assert_eq!(
            p[0].placeholder_key.as_deref(),
            Some("ext.insert_sequence.default")
        );
        assert!(!p[0].help.is_empty());
        let c = ext.commands();
        assert!(c
            .iter()
            .any(|c| c.id == "edit.insert_sequence" && c.key.as_deref() == Some("ctrl+alt+n")));
        let s = Settings::from_text(std::path::PathBuf::from("x"), "");
        let req = TransformReq {
            id: "edit.insert_sequence".into(),
            text: "1+1~03".into(),
            n: 3,
            regions: vec![(1, String::new()), (2, "x".into())],
            lang: "ko".into(),
            doc_path: String::new(),
            phase: "live".into(),
            selection: String::new(),
            pick: None,
        };
        let out = ext.transform(&req, &s).expect("transform");
        assert_eq!(out.values, vec!["001", "002", "003"]);
        assert!(out.error.is_none());
        let bad = TransformReq {
            text: "[a]x1".into(),
            ..req
        };
        let out = ext.transform(&bad, &s).expect("transform");
        assert!(out.values.is_empty());
        assert!(
            out.error.as_deref().is_some_and(|e| e.contains("목록")),
            "{:?}",
            out.error
        );
        let j = transform_request_json(&bad, r#"{"ext.insert_sequence.default":"1+1"}"#);
        assert!(
            j.contains("\"n\":3")
                && j.contains("\"settings\":{\"ext.insert_sequence.default\":\"1+1\"}"),
            "{j}"
        );
        assert!(j.contains("\"phase\":\"live\"") && j.contains("\"pick\":null"));
        let t = transform_out_from_json(r#"{"apply":"text","text":"T"}"#).expect("text mode");
        assert_eq!((t.apply.as_str(), t.text.as_str()), ("text", "T"));
        assert_eq!(
            transform_out_from_json(r#"{"apply":"none","rows":[]}"#)
                .expect("none")
                .apply,
            "none"
        );
        let o = transform_out_from_json(r#"{"error":"e","rows":[{"text":"r","warn":true}]}"#)
            .expect("out");
        assert_eq!(
            (o.error.as_deref(), o.rows.len(), o.rows[0].warn),
            (Some("e"), 1, true)
        );
        let c = transform_out_from_json(r#"{"apply":"none","rows":[{"text":"Hi","id":"hi"}]}"#)
            .expect("choice");
        assert_eq!(c.rows[0].id.as_deref(), Some("hi"));
        assert!(transform_out_from_json("{}").is_err());
    }

    /// ★ 148차(설정 접근): hello-ext 0.2.0 = settings_read ui.lang · settings_write editor.auto_close_pairs · say → 로그에 ui.lang 값(선언 키만
    ///   읽힌다) · toggle → 덮어쓰기 요청 큐 (editor.auto_close_pairs, 반대 값) · 큐는 한 번 거두면 빈다.
    #[test]
    fn hello_wasm_reads_declared_setting_and_queues_overlay() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions/hello-ext/hello_ext.wasm");
        if !path.exists() {
            eprintln!("skip: {} not built", path.display());
            return;
        }
        let mut ext = WasmExtension::load_file(&path).expect("load");
        assert_eq!(ext.settings_read(), vec!["ui.lang".to_string()]);
        assert_eq!(
            ext.settings_write(),
            vec!["editor.auto_close_pairs".to_string()]
        );
        let caps = ext.capabilities();
        assert!(caps.setting_set && !caps.file_ops, "{caps:?}");
        assert_eq!(caps.commands.len(), 3, "명령 2 + 프롬프트 병합 1(0.3.0)");
        let s = Settings::from_text(
            std::path::PathBuf::from("x"),
            "ui.lang=ko
",
        );
        let mut tb = nexa_ctl::TextBox::new("");
        assert!(ext.run_with("ext.hello_ext.say", &mut tb, "", &s));
        let notes: Vec<String> = std::mem::take(&mut *ext.notes.borrow_mut());
        assert!(notes.iter().any(|n| n.contains("ui.lang=ko")), "{notes:?}");
        assert!(ext.run_with("ext.hello_ext.toggle_autoclose", &mut tb, "", &s));
        let def = s.get("editor.auto_close_pairs").expect("key").to_string();
        let expect = if def == "off" { "on" } else { "off" };
        assert_eq!(
            ext.take_setting_sets(),
            vec![("editor.auto_close_pairs".to_string(), expect.to_string())]
        );
        assert!(ext.take_setting_sets().is_empty());
        // ★ T-447(153차): hello 0.3.0 프롬프트 "Hello: Pick a greeting" = live → 고를 수 있는 행(id) · commit + pick → apply text.
        assert!(ext.prompts().iter().any(|p| p.id == "ext.hello_ext.pick"));
        let live = TransformReq {
            id: "ext.hello_ext.pick".into(),
            text: String::new(),
            n: 1,
            phase: "live".into(),
            ..TransformReq::default()
        };
        let out = ext.transform(&live, &s).expect("live");
        assert_eq!(out.apply, "none");
        assert!(
            out.rows.iter().any(|r| r.id.as_deref() == Some("ko")),
            "{:?}",
            out.rows
        );
        let commit = TransformReq {
            phase: "commit".into(),
            pick: Some("ko".into()),
            ..live
        };
        let out = ext.transform(&commit, &s).expect("commit");
        assert_eq!(
            (out.apply.as_str(), out.text.as_str()),
            ("text", "안녕하세요")
        );
    }

    /// ★ 길 A(154차 · ABI v1.3): uppercase-selection = stateful(세션 카운터가 호출 사이에 남음) · editor.read/write(스냅샷 → 교체 요청 큐 →
    ///   `apply_edits` 한 묶음) · 상태줄 · 유휴 회수 뒤 init 다시(카운터 1부터). 파일이 없으면 건너뛴다.
    #[test]
    fn uppercase_wasm_is_stateful_and_edits_through_queue() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../extensions/uppercase-selection/uppercase_selection.wasm");
        if !path.exists() {
            eprintln!("skip: {} not built", path.display());
            return;
        }
        let mut ext = WasmExtension::load_file(&path).expect("load");
        let caps = ext.capabilities();
        assert!(
            caps.stateful && caps.editor_read && caps.editor_write,
            "{caps:?}"
        );
        assert_eq!(caps.events.len(), 3, "caret · edit · doc");
        let s = Settings::from_text(std::path::PathBuf::from("x"), "");
        let mut tb = nexa_ctl::TextBox::new("");
        tb.set_text("abc def");
        tb.set_regions_pub(&[(0, 3)]);
        assert!(ext.run_with("ext.uppercase_selection.run", &mut tb, "", &s));
        let ops = ext.take_edit_ops();
        assert_eq!(ops, vec![EditOp::Replace(0, 3, "ABC".into())]);
        assert_eq!(EditorOps::apply_edits(&mut tb, &ops).expect("apply"), 1);
        assert!(tb.text().starts_with("ABC def"), "{}", tb.text());
        let st = ext.take_status().expect("status");
        assert!(st.contains("1 (session)"), "{st}");
        // 두 번째 호출 = 같은 인스턴스(세션 2).
        tb.set_regions_pub(&[(4, 7)]);
        assert!(ext.run_with("ext.uppercase_selection.run", &mut tb, "", &s));
        let st = ext.take_status().expect("status");
        assert!(st.contains("2 (session)"), "{st}");
        // 유휴 회수 → 다음 호출은 새 인스턴스(세션 1) · 겹치는 편집은 거절.
        assert!(ext.reap_idle(Duration::ZERO).is_some());
        assert!(ext.run_with("ext.uppercase_selection.run", &mut tb, "", &s));
        let st = ext.take_status().expect("status");
        assert!(st.contains("1 (session)"), "{st}");
        let _ = ext.take_edit_ops();
        let bad = vec![
            EditOp::Replace(0, 3, "x".into()),
            EditOp::Replace(2, 5, "y".into()),
        ];
        assert!(EditorOps::apply_edits(&mut tb, &bad).is_err());
        assert!(EditorOps::apply_edits(&mut tb, &[EditOp::Replace(0, 99, "x".into())]).is_err());
        // ★ 154차-b: 캐럿 사건 10번 → 게스트 로그 "caret events: 10" · 모르는 사건은 미처리.
        for _ in 0..10 {
            assert!(ext.dispatch_event(r#"{"kind":"caret","regions":[[1,1]]}"#));
        }
        assert!(!ext.dispatch_event(r#"{"kind":"nope"}"#));
        let notes = ext.take_wasm_notes_for_test();
        assert!(
            notes.iter().any(|n| n.contains("caret events: 10")),
            "{notes:?}"
        );
        let _ = ext.take_wasm_notes_for_test();
    }

    /// 소스 트리의 공식 패키지 `.wasm`(SDK 샘플로 빌드해 둔 것)을 로드해 메타·설정→효과·명령 큐를 확인한다.
    /// ★ 146차(협업 V1 결함 → SDK `host::get` 해제 용량 결함 수정): PEM 0.2.0 명령 4종 = 모르는 명령은 0 · 나머지 처리 · 파일 op 큐 · 트랩 0.
    #[test]
    fn pem_wasm_runs_show_path_and_file_ops() {
        let path = std::env::var_os("NSQL_PEM_WASM")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../extensions/project-explorer-menus/project_explorer_menus.wasm")
            });
        let mut ext = WasmExtension::load_file(&path).expect("load wasm");
        assert_eq!(ext.id(), "project-explorer-menus");
        let caps = ext.capabilities();
        assert!(caps.file_ops && !caps.setting_set, "{caps:?}");
        let mut tb = nexa_ctl::TextBox::new("");
        let s = Settings::from_text(std::path::PathBuf::from("x"), "");
        for (cmd, doc) in [
            ("nope", ""),
            ("ext.project_explorer_menus.show_path", ""),
            ("ext.project_explorer_menus.show_path", "C:/tmp/a.sql"),
            ("ext.project_explorer_menus.reveal", "C:/tmp/a.sql"),
            ("ext.project_explorer_menus.open", "C:/tmp/a.sql"),
        ] {
            let ok = ext.run_with(cmd, &mut tb, doc, &s);
            let notes: Vec<String> = std::mem::take(&mut *ext.notes.borrow_mut());
            let ops = ext.take_file_ops();
            eprintln!("{cmd} doc={doc:?} ok={ok} notes={notes:?} ops={ops:?}");
            assert!(
                !notes.iter().any(|n| n.contains("nx_ext_run:")),
                "{cmd} {doc}: {notes:?}"
            );
            if cmd == "nope" {
                assert!(!ok);
            } else {
                assert!(ok, "{cmd} {doc}: {notes:?}");
            }
            if cmd.ends_with("reveal") {
                assert_eq!(ops, vec![FileOp::Reveal("C:/tmp/a.sql".into())]);
            }
            if cmd.ends_with(".open") {
                assert_eq!(ops, vec![FileOp::Open("C:/tmp/a.sql".into())]);
            }
        }
    }

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
        let j = settings_json(&s, "ext.rainbow_pairs.", &[]);
        assert!(j.contains("\"ext.rainbow_pairs.enabled\"") && !j.contains("\"editor."));
    }
}
