//! ★ 확장 드라이버 레지스트리(DR-29 · T-27 ③ · docs/22 §0): `drivers/<id>/<ver>/nsql_driver_<id>.{dll,so,dylib}` 레이아웃에서
//! **최신 버전**을 골라 첫 접속 때만 올린다(`nsql-driver-dyn` · 기동 때는 디스크를 읽지 않는다).
//!
//! - 드라이버 폴더 = `NSQL_DRIVER_DIR`(환경 변수 · 최우선) > 호스트가 넣은 값([`set_driver_dir`] · GUI/CLI = `<설정 폴더>/drivers`).
//! - id = 방언 이름([`nsql_core::Dialect`] Display · `odbc` · `mysql` …) — 내장 드라이버가 없는 방언([`crate::open`]의 폴백)이
//!   그 id의 확장을 찾는다(예: macOS/Linux ODBC = D-346 ⓐ · 빌드에서 뺀 드라이버의 cdylib).
//! - 버전 비교 = `.`으로 나눈 숫자 세그먼트 우선 · 아니면 문자열(semver 대략 · T-28에서 매니페스트로 보강).
//! - 올린 드라이버는 프로세스 수명 동안 캐시(64 §5: 내리지 않는다).
use nsql_core::{DbError, Session};
use nsql_driver_dyn::{library_file_name, DynDriver};
use nsql_script::ConnectSpec;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

static DRIVER_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static LOADED: Mutex<Option<HashMap<String, Arc<DynDriver>>>> = Mutex::new(None);

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
    pub path: PathBuf,
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
    let lib = library_file_name(&format!("nsql-driver-{id}"));
    let dir = root.join(id);
    let mut best: Option<ExtInfo> = None;
    for e in std::fs::read_dir(&dir).ok()?.flatten() {
        let ver = e.file_name().to_string_lossy().into_owned();
        let path = e.path().join(&lib);
        if !path.is_file() {
            continue;
        }
        let better = best
            .as_ref()
            .is_none_or(|b| version_cmp(&ver, &b.version) == std::cmp::Ordering::Greater);
        if better {
            best = Some(ExtInfo {
                id: id.to_string(),
                version: ver,
                path,
            });
        }
    }
    best
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
        .filter(|e| e.path().is_dir())
        .filter_map(|e| find_extension_in(&root, &e.file_name().to_string_lossy()))
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
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
    let d = DynDriver::load(&info.path)?;
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
