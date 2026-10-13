//! 드라이버 레지스트리 — UI·CLI가 아는 유일한 "드라이버" 진입점. 어댑터 크레이트는 여기 뒤에 숨는다(docs/01 §1).
//!
//! 접속 문자열:
//! - `sqlite::memory:` · `sqlite:path/to.db` · `sqlite:///abs/path.db`
//! - `oracle://user:pass@host:1521/service` · `user/pass@host:1521/svc`(방언 = 기본값 또는 `?dialect=`)
//! - `mssql://user:pass@host:1433/db`
//! - `postgres://user:pass@host:5432/db`(`postgresql://` · `pg://`)

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{DbError, Dialect, Session};
use nsql_script::ConnectSpec;

/// `sqlite:` 뒤 경로 — 권한부(`//`)는 **한 번만** 벗긴다. 종전 `trim_start_matches("//")`는 슬래시 쌍을 전부 먹어
/// `sqlite:////abs/x.db`(SQLAlchemy식 절대 경로)가 상대 경로 `abs/x.db`가 됐다(86차 mac 점검 · T-148).
/// 남은 선행 `//`는 POSIX에서 `/` 하나로 접는다(Windows는 UNC `//server/share`일 수 있어 그대로).
fn sqlite_path(rest: &str) -> String {
    let p = rest.strip_prefix("//").unwrap_or(rest);
    if !cfg!(windows) && p.starts_with("//") {
        return format!("/{}", p.trim_start_matches('/'));
    }
    p.to_string()
}

/// 접속 문자열 → 스펙(SQLite 축약형 처리 후 `ConnectSpec::parse`).
pub fn parse_target(s: &str, default_dialect: Dialect) -> Result<ConnectSpec, String> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("sqlite:") {
        let db = sqlite_path(rest);
        return Ok(ConnectSpec {
            dialect: Some(Dialect::Sqlite),
            database: Some(db),
            ..Default::default()
        });
    }
    let mut spec = ConnectSpec::parse(t)?;
    if spec.dialect.is_none() {
        spec.dialect = Some(default_dialect);
    }
    Ok(spec)
}

/// 스펙으로 세션을 연다.
pub fn open(spec: &ConnectSpec, default_dialect: Dialect) -> Result<Box<dyn Session>, DbError> {
    let dialect = spec.dialect.unwrap_or(default_dialect);
    match dialect {
        #[cfg(feature = "sqlite")]
        Dialect::Sqlite => {
            let s = nsql_driver_sqlite::SqliteSession::open(
                spec.database.as_deref().unwrap_or(":memory:"),
            )?;
            Ok(Box::new(s))
        }
        #[cfg(any(feature = "oracle", feature = "oracle-thin"))]
        Dialect::Oracle => open_oracle(spec),
        #[cfg(feature = "mssql")]
        Dialect::Mssql => Ok(Box::new(nsql_driver_mssql::MssqlSession::connect(spec)?)),
        #[cfg(feature = "pg")]
        Dialect::Postgres => Ok(Box::new(nsql_driver_pg::PgSession::connect(spec)?)),
        other => Err(DbError {
            code: None,
            message: format!("{other} 드라이버는 이 빌드에 없습니다(M4)"),
            position: None,
        }),
    }
}

/// ★ Oracle 드라이버 둘(docs/110 · 설정 `oracle.driver` · D-277/D-278): **OCI**(ODPI-C · Instant Client) / **Thin**(순수 Rust).
/// 선택은 프로세스 전역 하나 — GUI·CLI가 같은 설정을 넣고, 접속 입구 전부가 [`open`]의 Oracle 가지 한 자리에서 이 값을 읽는다
/// (한 자리 원칙 · 110 §3-2). 접속마다 읽으므로 **재시작 없이** 바뀐다(이미 열린 세션은 그대로 · 다음 접속부터).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OracleDriver {
    Oci,
    Thin,
}

impl OracleDriver {
    /// 설정 값(`oci` | `thin`) → 드라이버. 모르는 값 = **Thin**(기본값 · D-278 · 사용자 10-13 "Thin을 기본으로").
    #[must_use]
    pub fn parse(s: &str) -> OracleDriver {
        if s.trim().eq_ignore_ascii_case("oci") {
            OracleDriver::Oci
        } else {
            OracleDriver::Thin
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            OracleDriver::Oci => "oci",
            OracleDriver::Thin => "thin",
        }
    }

    /// 이 빌드에 들어 있는가.
    #[must_use]
    pub const fn included(self) -> bool {
        match self {
            OracleDriver::Oci => cfg!(feature = "oracle"),
            OracleDriver::Thin => cfg!(feature = "oracle-thin"),
        }
    }
}

/// 레지스트리 초기값 = Thin(설정 기본값과 같다 · 호스트가 설정을 넣기 전의 접속도 같은 값).
static ORACLE_DRIVER_THIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// 설정 `oracle.driver` → 레지스트리(접속마다 읽는다 · 재시작 불필요).
pub fn set_oracle_driver(d: OracleDriver) {
    ORACLE_DRIVER_THIN.store(d == OracleDriver::Thin, std::sync::atomic::Ordering::Relaxed);
}

/// 사용자가 고른 드라이버(빌드에 없어도 그 값).
#[must_use]
pub fn oracle_driver() -> OracleDriver {
    if ORACLE_DRIVER_THIN.load(std::sync::atomic::Ordering::Relaxed) {
        OracleDriver::Thin
    } else {
        OracleDriver::Oci
    }
}

/// **실제로 쓰일** 드라이버 — 고른 것이 빌드에 있으면 그것 · 없으면 들어 있는 쪽(**선택지 자체가 없음** = 폴백이 아니다 · 110 §3-2 ·
/// 런타임 실패(Instant Client 없음)는 여기서 돌리지 않는다 = D-279). 둘 다 없으면 고른 값(접속 때 "이 빌드에 없음" 오류).
#[must_use]
pub fn oracle_driver_effective() -> OracleDriver {
    let want = oracle_driver();
    if want.included() {
        want
    } else if OracleDriver::Thin.included() {
        OracleDriver::Thin
    } else if OracleDriver::Oci.included() {
        OracleDriver::Oci
    } else {
        want
    }
}

#[cfg(any(feature = "oracle", feature = "oracle-thin"))]
fn open_oracle(spec: &ConnectSpec) -> Result<Box<dyn Session>, DbError> {
    match oracle_driver_effective() {
        #[cfg(feature = "oracle")]
        OracleDriver::Oci => Ok(Box::new(nsql_driver_oracle::OracleSession::connect(spec)?)),
        #[cfg(feature = "oracle-thin")]
        OracleDriver::Thin => Ok(Box::new(nsql_driver_oracle_thin::ThinSession::connect(
            spec,
        )?)),
        #[allow(unreachable_patterns)]
        other => Err(DbError {
            code: None,
            message: format!("Oracle driver '{}' is not in this build", other.as_str()),
            position: None,
        }),
    }
}

/// 이 빌드에 들어 있는 방언.
/// SQL Server 암호화 범위(설정 `mssql.encrypt` · T-108): `login_only`면 로그인만 암호화 → 실행 취소가 TDS Attention으로 세션을 유지한다.
pub fn set_mssql_encryption(login_only: bool) {
    #[cfg(feature = "mssql")]
    nsql_driver_mssql::set_encryption(login_only);
    #[cfg(not(feature = "mssql"))]
    let _ = login_only;
}

/// 네트워크 생존 옵션(docs/53): TCP keepalive 유휴(초 · PG·SQL Server · 0 = 끔) · Oracle 호출 상한(초 · 0 = 없음). 다음 접속부터.
pub fn set_net_options(keepalive_secs: u64, call_timeout_secs: u64) {
    #[cfg(feature = "pg")]
    nsql_driver_pg::set_keepalive_secs(keepalive_secs);
    #[cfg(feature = "mssql")]
    nsql_driver_mssql::set_keepalive_secs(keepalive_secs);
    // Oracle 호출 상한 = **두 드라이버 전역에 다**(static · 선택과 무관 · 110 §3-2).
    #[cfg(feature = "oracle")]
    nsql_driver_oracle::set_call_timeout_secs(call_timeout_secs);
    #[cfg(feature = "oracle-thin")]
    nsql_driver_oracle_thin::set_call_timeout_secs(call_timeout_secs);
    #[cfg(not(all(
        feature = "pg",
        feature = "mssql",
        feature = "oracle",
        feature = "oracle-thin"
    )))]
    let _ = (keepalive_secs, call_timeout_secs);
}

/// Oracle 클라이언트 탐지 결과(설정 창의 읽기 전용 칸 · 사용자 09-21) — **값만** 돌려준다(문구는 호스트가 `Msg`로 만든다).
/// 파일 시스템만 읽는다(네트워크 0 · 라이브러리를 로드하지 않는다). 출처 코드: `setting` · `env` · `oracle_home` · `search_path` ·
/// `well_known` · `not_found` / TNS: `setting` · `env` · `client_dir` · `oracle_home` · `not_found`.
#[derive(Clone, Debug, Default)]
pub struct OracleClientInfo {
    /// 이 빌드에 Oracle **OCI** 드라이버(Instant Client 경로)가 들어 있는가.
    pub included: bool,
    /// 이 빌드에 Oracle **Thin** 드라이버가 들어 있는가(docs/110).
    pub thin_included: bool,
    /// 지금 접속에 쓰일 드라이버(`oci` | `thin` · [`oracle_driver_effective`]) · 사용자가 고른 값(`oracle_driver()`).
    pub driver: &'static str,
    pub chosen: &'static str,
    pub source: &'static str,
    pub tns_source: &'static str,
    pub dir: Option<std::path::PathBuf>,
    pub library: Option<std::path::PathBuf>,
    /// 이미 로드된 클라이언트의 버전(아직 접속한 적이 없으면 `None`).
    pub version: Option<String>,
    pub tns_admin: Option<std::path::PathBuf>,
    pub tnsnames: Option<std::path::PathBuf>,
    pub aliases: Vec<String>,
    pub sqlnet: Option<std::path::PathBuf>,
    /// 이 OS에서 찾는 클라이언트 라이브러리 파일 이름(`oci.dll` · `libclntsh.so` · `libclntsh.dylib`) — "없음" 안내에 쓴다.
    pub library_name: &'static str,
}

/// 지금 설정([`set_oracle_client`])과 환경으로 찾은 Oracle 클라이언트.
#[must_use]
pub fn oracle_client_info() -> OracleClientInfo {
    #[cfg(feature = "oracle")]
    {
        use nsql_driver_oracle::client::{detect, Source, TnsSource};
        let r = detect(&nsql_driver_oracle::client_config());
        OracleClientInfo {
            included: true,
            thin_included: cfg!(feature = "oracle-thin"),
            driver: oracle_driver_effective().as_str(),
            chosen: oracle_driver().as_str(),
            source: match r.source {
                Source::Setting => "setting",
                Source::EnvVar => "env",
                Source::OracleHome => "oracle_home",
                Source::SearchPath => "search_path",
                Source::WellKnown => "well_known",
                Source::NotFound => "not_found",
            },
            tns_source: match r.tns_source {
                TnsSource::Setting => "setting",
                TnsSource::EnvVar => "env",
                TnsSource::ClientDir => "client_dir",
                TnsSource::OracleHome => "oracle_home",
                TnsSource::NotFound => "not_found",
            },
            dir: r.dir,
            library: r.library,
            version: nsql_driver_oracle::loaded_version(),
            tns_admin: r.tns_admin,
            tnsnames: r.tnsnames,
            aliases: r.aliases,
            sqlnet: r.sqlnet,
            library_name: nsql_driver_oracle::client::library_name(),
        }
    }
    #[cfg(not(feature = "oracle"))]
    OracleClientInfo {
        thin_included: cfg!(feature = "oracle-thin"),
        driver: oracle_driver_effective().as_str(),
        chosen: oracle_driver().as_str(),
        source: "not_found",
        tns_source: "not_found",
        ..OracleClientInfo::default()
    }
}

/// 설정 `oracle.client_mode`/`oracle.client_dir`/`oracle.tns_admin` → Oracle 드라이버. `manual`이 아니면 자동 탐지(빈 값).
/// 첫 Oracle 접속 **전**에 부른 값만 이번 실행에 쓰인다(ODPI-C는 한 번만 초기화된다).
pub fn set_oracle_client(manual: bool, client_dir: &str, tns_admin: &str) {
    #[cfg(feature = "oracle")]
    {
        let opt = |s: &str| {
            let t = s.trim().trim_matches('"');
            (manual && !t.is_empty()).then(|| std::path::PathBuf::from(t))
        };
        nsql_driver_oracle::set_client_config(nsql_driver_oracle::client::ClientConfig {
            client_dir: opt(client_dir),
            tns_admin: opt(tns_admin),
        });
    }
    // Thin도 같은 TNS_ADMIN(직접 지정일 때만 · 아니면 드라이버 기본 = 환경 변수 `TNS_ADMIN`).
    #[cfg(feature = "oracle-thin")]
    nsql_driver_oracle_thin::set_tns_admin(
        (manual && !tns_admin.trim().is_empty())
            .then(|| tns_admin.trim().trim_matches('"').to_string()),
    );
    #[cfg(not(feature = "oracle"))]
    let _ = client_dir;
    #[cfg(not(any(feature = "oracle", feature = "oracle-thin")))]
    let _ = (manual, tns_admin);
}

/// SQL Server 취소 방식(설정 `mssql.cancel`): `socket` = 소켓 종료(항상) · 아니면 Attention(로그인만 암호화일 때 · 아니면 소켓 종료로 대체).
pub fn set_mssql_cancel_socket(socket: bool) {
    #[cfg(feature = "mssql")]
    nsql_driver_mssql::set_cancel_socket(socket);
    #[cfg(not(feature = "mssql"))]
    let _ = socket;
}

pub fn available() -> Vec<Dialect> {
    let mut v = Vec::new();
    if cfg!(feature = "sqlite") {
        v.push(Dialect::Sqlite);
    }
    if cfg!(any(feature = "oracle", feature = "oracle-thin")) {
        v.push(Dialect::Oracle);
    }
    if cfg!(feature = "mssql") {
        v.push(Dialect::Mssql);
    }
    if cfg!(feature = "pg") {
        v.push(Dialect::Postgres);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_shorthands() {
        let s = parse_target("sqlite::memory:", Dialect::Oracle).unwrap();
        assert_eq!(s.dialect, Some(Dialect::Sqlite));
        assert_eq!(s.database.as_deref(), Some(":memory:"));
        let s = parse_target("sqlite:///tmp/a.db", Dialect::Oracle).unwrap();
        assert_eq!(s.database.as_deref(), Some("/tmp/a.db"));
        let s = parse_target("u/p@h:1521/svc", Dialect::Oracle).unwrap();
        assert_eq!(s.dialect, Some(Dialect::Oracle));
    }

    /// 슬래시 4개(SQLAlchemy식 절대 경로)가 상대 경로로 바뀌지 않는다(T-148 · 86차).
    #[test]
    fn sqlite_four_slashes_stay_absolute() {
        let s = parse_target("sqlite:////tmp/a.db", Dialect::Oracle).unwrap();
        #[cfg(not(windows))]
        assert_eq!(s.database.as_deref(), Some("/tmp/a.db"));
        #[cfg(windows)]
        assert_eq!(s.database.as_deref(), Some("//tmp/a.db"));
        let s = parse_target("sqlite:rel/a.db", Dialect::Oracle).unwrap();
        assert_eq!(s.database.as_deref(), Some("rel/a.db"));
    }
}
