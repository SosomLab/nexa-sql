//! Oracle 클라이언트(Instant Client) **찾기 · 지정**(사용자 09-21) — Oracle은 네이티브 라이브러리가 있어야 접속된다.
//!
//! 두 방식: **자동**(지금 환경에서 찾는다 · 찾은 결과는 읽기 전용으로 보여 준다) · **직접 지정**(폴더와 `TNS_ADMIN`을 사용자가 정한다 ·
//! 거기서 나오는 라이브러리 파일 · `tnsnames.ora` · `sqlnet.ora` 경로는 읽기 전용 파생 정보).
//!
//! 찾는 순서(자동) = ODPI-C가 찾는 순서에 **잘 알려진 설치 자리**를 더한 것: ① `NSQL_ORACLE_CLIENT_DIR` ② `ORACLE_HOME`
//! ③ OS의 라이브러리 검색 경로(Windows `PATH` · Linux `LD_LIBRARY_PATH` · macOS `DYLD_LIBRARY_PATH`) ④ 잘 알려진 자리(OS별).
//! 찾은 폴더는 ODPI-C 초기화에 **그대로 넘긴다** — 화면에 보인 것과 실제로 로드되는 것이 같다.
//!
//! ODPI-C는 프로세스에서 **한 번만** 초기화된다 — 첫 Oracle 접속 뒤에 바꾼 값은 다음 실행부터 적용된다([`loaded_version`]).
//! 파일 시스템만 읽는다(네트워크 0 · 라이브러리를 로드하지 않는다) → 설정 창을 열 때마다 불러도 된다.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// 어디서 찾았는가.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// 설정에서 직접 지정한 폴더.
    Setting,
    /// 환경 변수 `NSQL_ORACLE_CLIENT_DIR`.
    EnvVar,
    /// `ORACLE_HOME`.
    OracleHome,
    /// OS의 라이브러리 검색 경로.
    SearchPath,
    /// 잘 알려진 설치 자리.
    WellKnown,
    NotFound,
}

/// `TNS_ADMIN`을 어디서 정했는가.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TnsSource {
    Setting,
    /// 환경 변수 `TNS_ADMIN`.
    EnvVar,
    /// 클라이언트 폴더의 `network/admin`.
    ClientDir,
    /// `ORACLE_HOME/network/admin`.
    OracleHome,
    NotFound,
}

/// 사용자가 정한 값(직접 지정 방식일 때만 채운다).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientConfig {
    pub client_dir: Option<PathBuf>,
    pub tns_admin: Option<PathBuf>,
}

/// 찾은 결과 — 설정 창의 읽기 전용 칸들.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientReport {
    pub source: Source,
    pub dir: Option<PathBuf>,
    /// 실제 라이브러리 파일(`oci.dll` · `libclntsh.so[.N]` · `libclntsh.dylib`).
    pub library: Option<PathBuf>,
    pub tns_source: TnsSource,
    pub tns_admin: Option<PathBuf>,
    pub tnsnames: Option<PathBuf>,
    /// `tnsnames.ora`의 별칭 이름(나온 순서 · 중복 없이).
    pub aliases: Vec<String>,
    pub sqlnet: Option<PathBuf>,
}

/// 이 OS의 클라이언트 라이브러리 파일 이름(접두 — Linux는 `libclntsh.so.19.1`처럼 버전이 붙는다).
#[must_use]
pub const fn library_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "oci.dll"
    } else if cfg!(target_os = "macos") {
        "libclntsh.dylib"
    } else {
        "libclntsh.so"
    }
}

/// 폴더 안의 클라이언트 라이브러리 파일 — 정확한 이름이 먼저 · 없으면 그 이름으로 시작하는 파일(버전 접미).
fn library_in(dir: &Path) -> Option<PathBuf> {
    let name = library_name();
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|f| f.to_str())
                    .is_some_and(|f| f.to_ascii_lowercase().starts_with(name))
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

/// `ORACLE_HOME` 아래에서 라이브러리가 있는 폴더(Windows = `bin` · 그 밖 = `lib`) — Instant Client를 `ORACLE_HOME`으로 잡은 경우는 그 폴더 자체.
fn oracle_home_dirs(home: &Path) -> Vec<PathBuf> {
    let sub = if cfg!(target_os = "windows") {
        "bin"
    } else {
        "lib"
    };
    vec![home.join(sub), home.to_path_buf()]
}

/// OS의 라이브러리 검색 경로 환경 변수 이름.
const fn search_path_var() -> &'static str {
    if cfg!(target_os = "windows") {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

/// 잘 알려진 설치 자리(OS별) — `instantclient*` 폴더는 이름 접두로 찾는다(버전이 붙는다).
fn well_known_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut fixed: Vec<PathBuf> = Vec::new();
    let mut parents: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "windows") {
        for p in ["C:\\oracle", "C:\\", "C:\\Program Files\\Oracle"] {
            parents.push(PathBuf::from(p));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(h) = home {
            fixed.push(h.join("lib"));
            parents.push(h.join("Downloads"));
            parents.push(h.to_path_buf());
        }
        fixed.push(PathBuf::from("/usr/local/lib"));
        fixed.push(PathBuf::from("/opt/homebrew/lib"));
        parents.push(PathBuf::from("/opt/oracle"));
    } else {
        parents.push(PathBuf::from("/opt/oracle"));
        parents.push(PathBuf::from("/usr/lib/oracle"));
        fixed.push(PathBuf::from("/usr/lib"));
        fixed.push(PathBuf::from("/usr/local/lib"));
    }
    let mut out = fixed;
    for parent in parents {
        let Ok(rd) = std::fs::read_dir(&parent) else {
            continue;
        };
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|f| f.to_str())
                        .is_some_and(|f| f.to_ascii_lowercase().starts_with("instantclient"))
            })
            .collect();
        // 가장 높은 버전(이름이 큰 것)이 먼저.
        subs.sort();
        subs.reverse();
        out.extend(subs);
        // RPM 설치: /usr/lib/oracle/<버전>/client64/lib
        if parent.ends_with("usr/lib/oracle") {
            if let Ok(vers) = std::fs::read_dir(&parent) {
                let mut v: Vec<PathBuf> = vers.flatten().map(|e| e.path()).collect();
                v.sort();
                v.reverse();
                for ver in v {
                    out.push(ver.join("client64").join("lib"));
                    out.push(ver.join("client").join("lib"));
                }
            }
        }
    }
    out
}

/// `tnsnames.ora`의 별칭 이름 — 첫 칸에서 시작하고 `=`가 있는 줄의 왼쪽(쉼표로 여러 이름) · 주석(`#`)·괄호 줄은 건너뛴다.
#[must_use]
pub fn tns_aliases(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut depth = 0i32;
    for line in text.lines() {
        let at_top = depth == 0;
        let t = line.trim_end();
        let starts_plain = t
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace() && c != '#' && c != '(' && c != ')');
        if at_top && starts_plain {
            if let Some((left, _)) = t.split_once('=') {
                for name in left.split(',') {
                    let n = name.trim();
                    if !n.is_empty()
                        && n.chars()
                            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
                        && !out.iter().any(|o| o.eq_ignore_ascii_case(n))
                    {
                        out.push(n.to_string());
                    }
                }
            }
        }
        if !t.trim_start().starts_with('#') {
            for c in t.chars() {
                match c {
                    '(' => depth += 1,
                    ')' => depth = (depth - 1).max(0),
                    _ => {}
                }
            }
        }
    }
    out
}

/// 찾는다 — `env` = 환경 변수 읽기(테스트는 가짜를 넣는다) · `home` = 사용자 홈(macOS `~/lib`).
#[must_use]
pub fn detect_with(
    cfg: &ClientConfig,
    env: &dyn Fn(&str) -> Option<OsString>,
    home: Option<&Path>,
    well_known: bool,
) -> ClientReport {
    let mut found: Option<(Source, PathBuf, PathBuf)> = None;
    let try_dir = |source: Source, dir: PathBuf, found: &mut Option<(Source, PathBuf, PathBuf)>| {
        if found.is_none() {
            if let Some(lib) = library_in(&dir) {
                *found = Some((source, dir, lib));
            }
        }
    };
    if let Some(d) = &cfg.client_dir {
        // 직접 지정 = 그 폴더만 본다(없으면 "못 찾음" — 다른 곳에서 찾은 것을 조용히 쓰지 않는다).
        try_dir(Source::Setting, d.clone(), &mut found);
    } else {
        if let Some(d) = env(super::CLIENT_DIR_ENV) {
            try_dir(Source::EnvVar, PathBuf::from(d), &mut found);
        }
        if let Some(h) = env("ORACLE_HOME") {
            for d in oracle_home_dirs(Path::new(&h)) {
                try_dir(Source::OracleHome, d, &mut found);
            }
        }
        if let Some(p) = env(search_path_var()) {
            for d in std::env::split_paths(&p) {
                if !d.as_os_str().is_empty() {
                    try_dir(Source::SearchPath, d, &mut found);
                }
            }
        }
        if well_known {
            for d in well_known_dirs(home) {
                try_dir(Source::WellKnown, d, &mut found);
            }
        }
    }
    let (source, dir, library) = match found {
        Some((s, d, l)) => (s, Some(d), Some(l)),
        None => (Source::NotFound, cfg.client_dir.clone(), None),
    };
    // TNS_ADMIN: 설정 → 환경 변수 → 클라이언트 폴더/network/admin → ORACLE_HOME/network/admin.
    let mut tns: Option<(TnsSource, PathBuf)> = None;
    if let Some(t) = &cfg.tns_admin {
        tns = Some((TnsSource::Setting, t.clone()));
    }
    if tns.is_none() {
        if let Some(t) = env("TNS_ADMIN") {
            tns = Some((TnsSource::EnvVar, PathBuf::from(t)));
        }
    }
    if tns.is_none() {
        if let Some(d) = &dir {
            let cand = d.join("network").join("admin");
            if cand.is_dir() {
                tns = Some((TnsSource::ClientDir, cand));
            }
        }
    }
    if tns.is_none() {
        if let Some(h) = env("ORACLE_HOME") {
            let cand = Path::new(&h).join("network").join("admin");
            if cand.is_dir() {
                tns = Some((TnsSource::OracleHome, cand));
            }
        }
    }
    let (tns_source, tns_admin) = match tns {
        Some((s, p)) => (s, Some(p)),
        None => (TnsSource::NotFound, None),
    };
    let file_in = |name: &str| -> Option<PathBuf> {
        let p = tns_admin.as_ref()?.join(name);
        p.is_file().then_some(p)
    };
    let tnsnames = file_in("tnsnames.ora");
    let sqlnet = file_in("sqlnet.ora");
    let aliases = tnsnames
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| tns_aliases(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    ClientReport {
        source,
        dir,
        library,
        tns_source,
        tns_admin,
        tnsnames,
        aliases,
        sqlnet,
    }
}

/// 지금 환경에서 찾는다.
#[must_use]
pub fn detect(cfg: &ClientConfig) -> ClientReport {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    detect_with(cfg, &|k| std::env::var_os(k), home.as_deref(), true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nsql-ora-client-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_client(dir: &Path, with_tns: bool) {
        std::fs::create_dir_all(dir).unwrap();
        let lib = if cfg!(target_os = "linux") {
            "libclntsh.so.19.1".to_string()
        } else {
            library_name().to_string()
        };
        std::fs::write(dir.join(lib), b"x").unwrap();
        if with_tns {
            let adm = dir.join("network").join("admin");
            std::fs::create_dir_all(&adm).unwrap();
            std::fs::write(
                adm.join("tnsnames.ora"),
                "# comment\nDEV, dev2 =\n  (DESCRIPTION =\n    (ADDRESS = (PROTOCOL = TCP)(HOST = h)(PORT = 1521))\n    (CONNECT_DATA = (SERVICE_NAME = x))\n  )\n\nPROD.WORLD=(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=p)(PORT=1521)))\n",
            )
            .unwrap();
            std::fs::write(adm.join("sqlnet.ora"), "NAMES.DIRECTORY_PATH=(TNSNAMES)\n").unwrap();
        }
    }

    /// 찾는 순서 · 직접 지정은 그 폴더만 · TNS_ADMIN의 출처 · 별칭 읽기 · 못 찾음 — 가짜 환경 변수와 임시 폴더로(실제 환경을 읽지 않는다).
    #[test]
    fn detection_order_manual_mode_and_tns_files() {
        let root = tmp("order");
        let (a, b, c) = (root.join("env"), root.join("home"), root.join("path"));
        fake_client(&a, false);
        fake_client(
            &b.join(if cfg!(target_os = "windows") {
                "bin"
            } else {
                "lib"
            }),
            false,
        );
        fake_client(&c, true);
        let sp = search_path_var();
        let env_all = |k: &str| -> Option<OsString> {
            match k {
                "NSQL_ORACLE_CLIENT_DIR" => Some(a.clone().into()),
                "ORACLE_HOME" => Some(b.clone().into()),
                k if k == sp => Some(c.clone().into()),
                _ => None,
            }
        };
        let auto = ClientConfig::default();
        let r = detect_with(&auto, &env_all, None, false);
        assert_eq!(
            (r.source, r.dir.as_deref()),
            (Source::EnvVar, Some(a.as_path()))
        );
        assert!(r.library.is_some());
        let env_home = |k: &str| (k == "ORACLE_HOME").then(|| b.clone().into());
        assert_eq!(
            detect_with(&auto, &env_home, None, false).source,
            Source::OracleHome
        );
        let env_path = |k: &str| (k == sp).then(|| c.clone().into());
        let r = detect_with(&auto, &env_path, None, false);
        assert_eq!(r.source, Source::SearchPath);
        assert_eq!(r.tns_source, TnsSource::ClientDir);
        assert_eq!(r.aliases, vec!["DEV", "dev2", "PROD.WORLD"]);
        assert!(r.tnsnames.is_some() && r.sqlnet.is_some());
        // 직접 지정: 그 폴더만 — 다른 곳에 있어도 쓰지 않는다.
        let manual_bad = ClientConfig {
            client_dir: Some(root.join("nope")),
            tns_admin: None,
        };
        let r = detect_with(&manual_bad, &env_all, None, false);
        assert_eq!((r.source, r.library.is_some()), (Source::NotFound, false));
        assert_eq!(
            r.dir.as_deref(),
            Some(root.join("nope").as_path()),
            "지정한 폴더는 그대로 보여 준다"
        );
        let manual = ClientConfig {
            client_dir: Some(c.clone()),
            tns_admin: Some(root.join("mytns")),
        };
        let r = detect_with(&manual, &env_all, None, false);
        assert_eq!(
            (r.source, r.tns_source),
            (Source::Setting, TnsSource::Setting)
        );
        assert!(
            r.tnsnames.is_none(),
            "지정한 TNS_ADMIN에 파일이 없으면 없다고 보여 준다"
        );
        // 환경 변수 TNS_ADMIN.
        let tns_env = |k: &str| match k {
            k if k == sp => Some(a.clone().into()),
            "TNS_ADMIN" => Some(c.join("network").join("admin").into()),
            _ => None,
        };
        let r = detect_with(&auto, &tns_env, None, false);
        assert_eq!((r.tns_source, r.aliases.len()), (TnsSource::EnvVar, 3));
        assert_eq!(
            detect_with(&auto, &|_| None, None, false).source,
            Source::NotFound
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tns_alias_parsing() {
        assert_eq!(
            tns_aliases("A=(DESCRIPTION=(X=1))\n  B = ignored(indented)\n#C = no\nD.E , f =\n (DESCRIPTION=\n  (G=1))\nH=(X)"),
            vec!["A", "D.E", "f", "H"]
        );
        assert!(tns_aliases("").is_empty());
    }
}
