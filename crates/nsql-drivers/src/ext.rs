//! ★ 확장 드라이버 레지스트리(DR-29 · T-27 ③ · docs/22 §0): `drivers/<id>/<ver>/nsql_driver_<id>.{dll,so,dylib}` 레이아웃에서
//! **최신 버전**을 골라 첫 접속 때만 올린다(`nsql-driver-dyn` · 기동 때는 디스크를 읽지 않는다).
//!
//! - 드라이버 폴더 = `NSQL_DRIVER_DIR`(환경 변수 · 최우선) > 호스트가 넣은 값([`set_driver_dir`] · GUI/CLI = `<설정 폴더>/drivers`).
//! - id = 방언 이름([`nsql_core::Dialect`] Display · `odbc` · `mysql` …) — 내장 드라이버가 없는 방언([`crate::open`]의 폴백)이
//!   그 id의 확장을 찾는다(예: macOS/Linux ODBC = D-346 ⓐ · 빌드에서 뺀 드라이버의 cdylib).
//! - 버전 비교 = `.`으로 나눈 숫자 세그먼트 우선 · 아니면 문자열(semver 대략 · T-28에서 매니페스트로 보강).
//! - 올린 드라이버는 프로세스 수명 동안 캐시(64 §5: 내리지 않는다 — Rust cdylib FreeLibrary는 10-23 실측에서 접근 위반). 삭제는
//!   **올라가 있어도 된다**(사용자 10-23): 지울 수 없는 파일(Windows의 매핑된 DLL)은 `<root>/.trash/`로 옮기고(열린 DLL도 이름 바꾸기는 된다)
//!   `<root>/.delete-pending`에 적어 다음 시작([`sweep_pending`])에 지운다 · 목록·탐색은 즉시 그 판을 빼고 본다 · 올라간 판은 재시작까지 그대로 쓰인다.
pub mod install;
pub mod keys;
pub mod sha256;
pub mod zip;

use nsql_core::{DbError, Session};
use nsql_driver_dyn::{library_file_name, DynDriver};
use nsql_script::ConnectSpec;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

static DRIVER_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static LOADED: Mutex<Option<HashMap<String, Arc<DynDriver>>>> = Mutex::new(None);
/// 다음 시작 때 지울 폴더 목록(드라이버 폴더 기준 상대 경로 · 한 줄 하나).
pub const PENDING_FILE: &str = ".delete-pending";

/// 호스트가 드라이버 폴더를 넣는다(`<설정 폴더>/drivers`). `None` = 환경 변수만.
pub fn set_driver_dir(dir: Option<&Path>) {
    *DRIVER_DIR.lock().unwrap_or_else(|e| e.into_inner()) = dir.map(Path::to_path_buf);
}

/// 지금 드라이버 폴더 — `NSQL_DRIVER_DIR` > 호스트 값 > 없음.
#[must_use]
pub fn driver_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("NSQL_DRIVER_DIR") {
        if !d.is_empty() {
            return Some(PathBuf::from(d));
        }
    }
    DRIVER_DIR.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// 설치된 확장 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtInfo {
    pub id: String,
    pub version: String,
    /// 라이브러리 파일(매니페스트 `lib`가 있으면 그 이름 · 없으면 `nsql_driver_<id>.{dll,so,dylib}`).
    pub path: PathBuf,
    /// 매니페스트가 적은 방언(없으면 None · 실제 방언은 함수표가 말한다).
    pub dialect: Option<String>,
    /// 매니페스트 `protocol`(ABI 버전 · 없으면 None).
    pub abi: Option<u32>,
    /// `nexa-driver.json`이 있었는가.
    pub manifest: bool,
}

/// 매니페스트 파일 이름(docs/22 §2-3).
pub const MANIFEST: &str = "nexa-driver.json";

/// `nexa-driver.json`에서 쓰는 필드(cdylib 확장): `id` · `version` · `dialect` · `protocol`(ABI 버전) · `lib`(파일 이름 · 선택).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub dialect: String,
    pub protocol: Option<u32>,
    pub lib: Option<String>,
}

/// 매니페스트 본문 → 필드(순수 · 모르는 키 무시 · 숫자는 `protocol`만).
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let v = nsql_json::parse(text)?;
    let nsql_json::Json::Obj(items) = v else {
        return Err("manifest: not an object".to_string());
    };
    let mut m = Manifest::default();
    for (k, v) in items {
        match (k.as_str(), v) {
            ("id", nsql_json::Json::Str(s)) => m.id = s,
            ("version", nsql_json::Json::Str(s)) => m.version = s,
            ("dialect", nsql_json::Json::Str(s)) => m.dialect = s,
            ("lib", nsql_json::Json::Str(s)) => m.lib = Some(s),
            ("protocol", nsql_json::Json::Num(n)) => m.protocol = Some(n as u32),
            _ => {}
        }
    }
    Ok(m)
}

/// 버전 폴더 하나를 본다: 매니페스트(있으면 lib 이름 · protocol 검사) → 라이브러리 파일 존재.
/// 기본 라이브러리 파일 이름(`nsql_driver_<id>.{dll,so,dylib}`).
#[must_use]
pub fn library_file_name_for(id: &str) -> String {
    library_file_name(&format!("nsql-driver-{id}"))
}

fn inspect_version_dir(id: &str, dir: &Path) -> Option<ExtInfo> {
    let ver = dir.file_name()?.to_string_lossy().into_owned();
    let default_lib = library_file_name_for(id);
    let manifest = std::fs::read_to_string(dir.join(MANIFEST))
        .ok()
        .and_then(|t| parse_manifest(&t).ok());
    if let Some(m) = &manifest {
        // ABI가 다른 매니페스트 = 이 본체가 올릴 수 없는 판 → 목록에서 뺀다(설치 도구가 걸러야 할 몫 · T-29).
        if m.protocol.is_some_and(|p| p != nsql_abi::ABI_VERSION) {
            return None;
        }
    }
    let lib = manifest
        .as_ref()
        .and_then(|m| m.lib.clone())
        .unwrap_or(default_lib);
    let path = dir.join(lib);
    if !path.is_file() {
        return None;
    }
    Some(ExtInfo {
        id: id.to_string(),
        version: manifest
            .as_ref()
            .filter(|m| !m.version.is_empty())
            .map_or(ver, |m| m.version.clone()),
        path,
        dialect: manifest
            .as_ref()
            .filter(|m| !m.dialect.is_empty())
            .map(|m| m.dialect.clone()),
        abi: manifest.as_ref().and_then(|m| m.protocol),
        manifest: manifest.is_some(),
    })
}

/// 버전 문자열 비교(순수): 숫자 세그먼트는 값으로 · 그 밖은 문자열로 · 짧은 쪽이 작다.
#[must_use]
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let seg = |s: &str| -> Vec<Result<u64, String>> {
        s.trim_start_matches(['v', 'V'])
            .split(['.', '-'])
            .map(|p| p.parse::<u64>().map_err(|_| p.to_string()))
            .collect()
    };
    let (sa, sb) = (seg(a), seg(b));
    for (x, y) in sa.iter().zip(sb.iter()) {
        let o = match (x, y) {
            (Ok(x), Ok(y)) => x.cmp(y),
            (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
            (Err(_), Ok(_)) => std::cmp::Ordering::Less,
            (Err(x), Err(y)) => x.cmp(y),
        };
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    // 남은 세그먼트: 숫자 = 더 큰 버전(1.2.0 > 1.2) · 문자열 = 프리릴리스(1.0.0-beta < 1.0.0 · semver).
    let n = sa.len().min(sb.len());
    match (sa.get(n), sb.get(n)) {
        (None, None) => std::cmp::Ordering::Equal,
        (Some(Ok(_)), None) | (None, Some(Err(_))) => std::cmp::Ordering::Greater,
        (Some(Err(_)), None) | (None, Some(Ok(_))) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
}

/// `drivers/<id>/<ver>/<lib>`에서 최신 버전 하나(라이브러리 파일이 있는 버전만).
#[must_use]
pub fn find_extension_in(root: &Path, id: &str) -> Option<ExtInfo> {
    let dir = root.join(id);
    let pending = pending_in(root);
    let mut best: Option<ExtInfo> = None;
    for e in std::fs::read_dir(&dir).ok()?.flatten() {
        if is_pending(root, &e.path(), &pending) {
            continue;
        }
        let Some(info) = inspect_version_dir(id, &e.path()) else {
            continue;
        };
        let better = best
            .as_ref()
            .is_none_or(|b| version_cmp(&info.version, &b.version) == std::cmp::Ordering::Greater);
        if better {
            best = Some(info);
        }
    }
    best
}

/// 이 프로세스가 이미 올린 id인가(삭제해도 재시작까지 그 판이 쓰인다 — 호스트가 안내한다).
#[must_use]
pub fn is_loaded(id: &str) -> bool {
    LOADED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|m| m.contains_key(id))
}

/// ★ 올려 둔 판을 잊는다(접속 시 자동 갱신 뒤 · 사용자 10-30): 다음 [`open_ext`]가 새 판을 올린다. 이미 열린 세션은 `Arc`로 옛 판을
/// 붙들고 있으므로 안전(라이브러리는 내리지 않는다).
pub fn forget_loaded(id: &str) {
    if let Some(m) = LOADED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.remove(id);
    }
}

/// 휴지통 폴더 이름(드라이버 폴더 안 · 다음 시작 때 비운다).
pub const TRASH_DIR: &str = ".trash";

/// 확장 폴더 삭제(T-28): `ver` 없으면 id 전체. **올라가 있어도 된다**(사용자 10-23 "로딩된 대상 삭제"): 파일을 하나씩 지우고, 못 지우는 것
/// (Windows에 매핑된 DLL · 동봉 런타임)은 `.trash/<id>-<ver>-<pid>-<n>/`로 옮긴 뒤 표식에 적는다(다음 시작 [`sweep_pending`]). 돌려주는 값 =
/// 지운/옮긴 최상위 폴더. 올라간 판은 재시작까지 그대로 쓰인다([`is_loaded`]로 안내).
pub fn remove_extension(id: &str, ver: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let root = driver_dir().ok_or_else(|| "driver folder unknown".to_string())?;
    let target = match ver {
        Some(v) => root.join(id).join(v),
        None => root.join(id),
    };
    if !target.is_dir() {
        return Ok(Vec::new());
    }
    if std::fs::remove_dir_all(&target).is_ok() {
        return Ok(vec![target]);
    }
    // 통째로는 안 됨(열린 파일) → 파일 단위: 지우거나 휴지통으로.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let trash_rel = PathBuf::from(TRASH_DIR).join(format!(
        "{id}-{}-{}-{stamp}",
        ver.unwrap_or("all"),
        std::process::id()
    ));
    let trash = root.join(&trash_rel);
    let mut moved = false;
    move_or_delete_tree(&target, &target, &trash, &mut moved)?;
    let _ = std::fs::remove_dir_all(&target);
    if target.exists() {
        mark_pending_in(&root, target.strip_prefix(&root).unwrap_or(&target))?;
    }
    if moved {
        mark_pending_in(&root, &trash_rel)?;
    }
    Ok(vec![target])
}

/// `dir` 아래를 재귀로: 파일은 지우고, 못 지우면 `trash/<상대>`로 옮긴다(같은 볼륨 rename · 열린 DLL도 된다).
fn move_or_delete_tree(
    base: &Path,
    dir: &Path,
    trash: &Path,
    moved: &mut bool,
) -> Result<(), String> {
    for e in std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
    {
        let p = e.path();
        if p.is_dir() {
            move_or_delete_tree(base, &p, trash, moved)?;
            let _ = std::fs::remove_dir(&p);
            continue;
        }
        if std::fs::remove_file(&p).is_ok() {
            continue;
        }
        let rel = p.strip_prefix(base).unwrap_or(&p);
        let dest = trash.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::rename(&p, &dest).map_err(|e| format!("{}: {e}", p.display()))?;
        *moved = true;
    }
    Ok(())
}

/// `.delete-pending`에 상대 경로 한 줄 추가(중복 없이).
pub fn mark_pending_in(root: &Path, rel: &Path) -> Result<(), String> {
    let mut set = pending_in(root);
    if set.iter().any(|p| p == rel) {
        return Ok(());
    }
    set.push(rel.to_path_buf());
    let text: String = set
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/") + "\n")
        .collect();
    std::fs::write(root.join(PENDING_FILE), text)
        .map_err(|e| format!("{}: {e}", root.join(PENDING_FILE).display()))
}

/// 다음 시작 때 지울 상대 경로들(파일이 없으면 빈 목록 · 파일 하나만 읽는다 = 기동 비용 stat 1).
#[must_use]
pub fn pending_in(root: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(root.join(PENDING_FILE))
        .map(|t| {
            t.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// 표식에 적힌 폴더(휴지통 포함)를 지운다(기동 직후 · 이 프로세스는 아직 아무 드라이버도 올리지 않았다). 못 지운 것은 표식에 남긴다.
pub fn sweep_pending_in(root: &Path) -> Vec<PathBuf> {
    let list = pending_in(root);
    if list.is_empty() {
        return Vec::new();
    }
    let mut removed = Vec::new();
    let mut left = Vec::new();
    for rel in list {
        // 안전: 드라이버 폴더 밖으로 나가는 경로는 무시한다.
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            continue;
        }
        let target = root.join(&rel);
        if !target.exists() || std::fs::remove_dir_all(&target).is_ok() {
            removed.push(target);
        } else {
            left.push(rel);
        }
    }
    let f = root.join(PENDING_FILE);
    if left.is_empty() {
        let _ = std::fs::remove_file(&f);
    } else {
        let text: String = left
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/") + "\n")
            .collect();
        let _ = std::fs::write(&f, text);
    }
    removed
}

/// 기동 직후 한 번(GUI·CLI): 지난번에 못 지운 폴더를 지운다 — 표식 파일이 없으면 stat 하나로 끝(docs/61 §1-8 지연 로딩).
pub fn sweep_pending() -> Vec<PathBuf> {
    driver_dir()
        .map(|r| sweep_pending_in(&r))
        .unwrap_or_default()
}

/// 삭제 예정(표식)인 경로인가 — 목록·탐색이 그 판을 빼고 본다.
fn is_pending(root: &Path, path: &Path, pending: &[PathBuf]) -> bool {
    pending.iter().any(|rel| path.starts_with(root.join(rel)))
}

/// 실제로 올려 본 확장 정보(`nsql driver check`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckInfo {
    pub driver_id: String,
    pub version: String,
    pub dialect: String,
    pub abi: u32,
    pub path: PathBuf,
}

pub fn check_extension(id: &str) -> Result<CheckInfo, DbError> {
    let info = find_extension(id).ok_or_else(|| DbError {
        code: None,
        message: format!("extension '{id}' not installed"),
        position: None,
    })?;
    let d = driver(id)?.ok_or_else(|| DbError {
        code: None,
        message: format!("extension '{id}' not loadable"),
        position: None,
    })?;
    Ok(CheckInfo {
        driver_id: d.id().to_string(),
        version: info.version,
        dialect: d.dialect().to_string(),
        abi: nsql_abi::ABI_VERSION,
        path: info.path,
    })
}

#[must_use]
pub fn find_extension(id: &str) -> Option<ExtInfo> {
    find_extension_in(&driver_dir()?, id)
}

/// 설치된 확장 전부(id별 최신 하나 · id 순).
#[must_use]
pub fn list_extensions() -> Vec<ExtInfo> {
    let Some(root) = driver_dir() else {
        return Vec::new();
    };
    let mut v: Vec<ExtInfo> = std::fs::read_dir(&root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| find_extension_in(&root, &e.file_name().to_string_lossy()))
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

/// 폴더 안 **모든** (id, 버전) 조합(`nsql driver list` 상세 · 시험) — 매니페스트가 걸러낸 판은 빠진다.
#[must_use]
pub fn list_extensions_in(root: &Path) -> Vec<ExtInfo> {
    let mut v = Vec::new();
    let pending = pending_in(root);
    for id_entry in std::fs::read_dir(root).ok().into_iter().flatten().flatten() {
        if !id_entry.path().is_dir() || id_entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let id = id_entry.file_name().to_string_lossy().into_owned();
        for ver_entry in std::fs::read_dir(id_entry.path())
            .ok()
            .into_iter()
            .flatten()
            .flatten()
        {
            if is_pending(root, &ver_entry.path(), &pending) {
                continue;
            }
            if let Some(info) = inspect_version_dir(&id, &ver_entry.path()) {
                v.push(info);
            }
        }
    }
    v.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| version_cmp(&b.version, &a.version))
    });
    v
}

/// 올린 드라이버(캐시) — 없으면 찾아서 올린다.
fn driver(id: &str) -> Result<Option<Arc<DynDriver>>, DbError> {
    let mut g = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(d) = map.get(id) {
        return Ok(Some(Arc::clone(d)));
    }
    let Some(info) = find_extension(id) else {
        return Ok(None);
    };
    let d = DynDriver::load(&info.path, id)?;
    map.insert(id.to_string(), Arc::clone(&d));
    Ok(Some(d))
}

/// 확장 드라이버로 세션을 연다 — 설치돼 있지 않으면 `Ok(None)`.
pub fn open_ext(id: &str, spec: &ConnectSpec) -> Result<Option<Box<dyn Session>>, DbError> {
    match driver(id)? {
        None => Ok(None),
        Some(d) => Ok(Some(Box::new(d.open(spec)?))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 삭제 예정 표식(사용자 10-23 "올라간 드라이버 삭제"): 표식에 적힌 판은 목록·탐색에서 빠지고, sweep이 지우며 표식을 정리한다.
    #[test]
    fn pending_marker_hides_then_sweep_removes() {
        let root = std::env::temp_dir().join(format!("nsql-ext-pending-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (id, ver) in [("zz", "1.0.0"), ("zz", "2.0.0"), ("yy", "1.0.0")] {
            let d = root.join(id).join(ver);
            std::fs::create_dir_all(&d).expect("mkdir");
            std::fs::write(d.join(library_file_name_for(id)), b"").expect("lib");
        }
        assert_eq!(
            find_extension_in(&root, "zz").map(|e| e.version),
            Some("2.0.0".into())
        );
        mark_pending_in(&root, Path::new("zz/2.0.0")).expect("mark");
        mark_pending_in(&root, Path::new("zz/2.0.0")).expect("mark twice = 한 줄");
        assert_eq!(pending_in(&root).len(), 1);
        assert_eq!(
            find_extension_in(&root, "zz").map(|e| e.version),
            Some("1.0.0".into()),
            "표식 판 제외"
        );
        assert_eq!(list_extensions_in(&root).len(), 2);
        mark_pending_in(&root, Path::new("yy")).expect("mark id");
        assert!(find_extension_in(&root, "yy").is_none(), "id 전체 표식");
        let removed = sweep_pending_in(&root);
        assert_eq!(removed.len(), 2);
        assert!(!root.join("zz").join("2.0.0").exists() && !root.join("yy").exists());
        assert!(root.join("zz").join("1.0.0").is_dir());
        assert!(
            !root.join(PENDING_FILE).exists(),
            "다 지우면 표식 파일도 없앤다"
        );
        assert!(sweep_pending_in(&root).is_empty());
        // 폴더 밖 경로는 무시.
        mark_pending_in(&root, Path::new("../outside")).expect("mark");
        assert!(sweep_pending_in(&root).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn version_order_is_numeric_per_segment() {
        use std::cmp::Ordering::*;
        assert_eq!(version_cmp("1.10.0", "1.2.0"), Greater);
        assert_eq!(version_cmp("1.2", "1.2.0"), Less);
        assert_eq!(version_cmp("v2.0.0", "2.0.0"), Equal);
        assert_eq!(
            version_cmp("1.0.0-beta", "1.0.0"),
            Less,
            "접미 세그먼트 = 문자열 < 숫자"
        );
        assert_eq!(version_cmp("abc", "abd"), Less);
    }

    #[test]
    fn manifest_fields_and_unknown_keys() {
        let m = parse_manifest(
            r#"{"id":"duckdb","version":"1.2.0","dialect":"odbc","protocol":1,"lib":"libduck.so","x":{"y":1}}"#,
        )
        .expect("manifest");
        assert_eq!(m.id, "duckdb");
        assert_eq!(m.protocol, Some(1));
        assert_eq!(m.lib.as_deref(), Some("libduck.so"));
        assert!(parse_manifest("[1]").is_err());
        assert!(parse_manifest("{").is_err());
    }

    #[test]
    fn driver_dir_prefers_env_then_host_value() {
        // 환경 변수는 건드리지 않는다(병렬 시험) — 호스트 값만 본다.
        if std::env::var_os("NSQL_DRIVER_DIR").is_none() {
            set_driver_dir(Some(Path::new("X:/drivers")));
            assert_eq!(driver_dir(), Some(PathBuf::from("X:/drivers")));
            set_driver_dir(None);
            assert_eq!(driver_dir(), None);
        }
    }
}
