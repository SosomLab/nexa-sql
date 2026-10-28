//! 드라이버 레지스트리 — UI·CLI가 아는 유일한 "드라이버" 진입점. 어댑터 크레이트는 여기 뒤에 숨는다(docs/01 §1).
//!
//! 접속 문자열:
//! - `sqlite::memory:` · `sqlite:path/to.db` · `sqlite:///abs/path.db`
//! - `oracle://user:pass@host:1521/service` · `user/pass@host:1521/svc`(방언 = 기본값 또는 `?dialect=`)
//! - `mssql://user:pass@host:1433/db`
//! - `postgres://user:pass@host:5432/db`(`postgresql://` · `pg://`)
//! - `mysql://user:pass@host:3306/db`(`mariadb://` 별칭 · 같은 드라이버 · 135차)

//! - `odbc:DSN=…` · `odbc:Driver={…};…`(Windows 내장 · D-345) — 내장이 없는 빌드에서는 [`ext`] 확장(`drivers/odbc/<ver>/`)으로 폴백.
#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{DbError, Dialect, Session};
use nsql_script::ConnectSpec;

pub mod ext;

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
    // ★ T-439(150차): `-d duckdb D:\x.bin`처럼 기본 방언이 파일형이고 대상이 드라이브·UNC 경로면 그 스킴으로(확장자 추정보다 명시가 먼저).
    let file_scheme = match default_dialect {
        Dialect::Sqlite => Some("sqlite"),
        Dialect::Duckdb => Some("duckdb"),
        Dialect::Access => Some("access"),
        _ => None,
    };
    let mut spec = match file_scheme {
        Some(scheme) if nsql_script::connect::is_drive_or_unc_path(t) => {
            ConnectSpec::parse(&format!("{scheme}:{t}"))?
        }
        _ => ConnectSpec::parse(t)?,
    };
    if spec.dialect.is_none() {
        spec.dialect = Some(default_dialect);
    }
    Ok(spec)
}

/// 스펙으로 세션을 연다.
/// ★ PostgreSQL TLS(T-70 · 155차 · 설정 `pg.tls` + `pg.tls_ca`): 접속 문자열 `?tls=`/`sslrootcert=`가 있으면 그쪽이 이긴다 · 다음 접속부터 ·
///   허브가 [`open`]에서 스펙에 넣어 주므로 내장(rlib)·확장(cdylib · ABI kv `tls`/`tls_ca`) 어느 경로든 같다.
static PG_TLS: std::sync::Mutex<(String, String)> =
    std::sync::Mutex::new((String::new(), String::new()));

pub fn set_pg_tls(mode: &str, ca_path: &str) {
    *PG_TLS.lock().unwrap_or_else(|e| e.into_inner()) =
        (mode.trim().to_string(), ca_path.trim().to_string());
}

/// 스펙에 PG TLS 기본값을 채운다(명시된 값은 그대로).
fn with_pg_tls(spec: &ConnectSpec) -> ConnectSpec {
    let (mode, ca) = PG_TLS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut s = spec.clone();
    if s.tls.is_none() && !mode.is_empty() {
        s.tls = Some(mode);
    }
    if s.tls_ca.is_none() && !ca.is_empty() {
        s.tls_ca = Some(ca);
    }
    s
}

pub fn open(spec: &ConnectSpec, default_dialect: Dialect) -> Result<Box<dyn Session>, DbError> {
    let dialect = spec.dialect.unwrap_or(default_dialect);
    // ★ T-70: PostgreSQL(와이어 호환 포함)은 설정의 TLS 모드·CA를 스펙에 넣어 어느 드라이버 경로든 같은 값을 쓴다.
    let filled;
    let spec = if dialect == Dialect::Postgres {
        filled = with_pg_tls(spec);
        &filled
    } else {
        spec
    };
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
        #[cfg(feature = "mysql")]
        Dialect::Mysql => Ok(Box::new(nsql_driver_mysql::MysqlSession::connect(spec)?)),
        // ★ Db2(D-361 · 10-25): 주 경로 = 네이티브 DRDA 확장 `db2`(아래 확장 분기) · 내장 ODBC 어댑터는 **`?driver=odbc`로 명시할 때만**
        //   (IBM clidriver가 있는 PC · 모든 OS 같은 규칙 · 협업 V1 (c) "안내 없이 clidriver 경로로 빠짐" 교정).
        #[cfg(feature = "odbc")]
        Dialect::Db2 if spec.driver.as_deref() == Some("odbc") => {
            Ok(Box::new(nsql_driver_odbc::OdbcSession::connect(spec)?))
        }
        #[cfg(feature = "odbc")]
        Dialect::Odbc | Dialect::Access => {
            Ok(Box::new(nsql_driver_odbc::OdbcSession::connect(spec)?))
        }
        // ★ D-350: 내장이 없는 방언 = 확장 드라이버(`drivers/<id>/<ver>/` cdylib · 첫 접속 때 지연 로드) · 없으면
        //   [`ERR_DRIVER_MISSING`](호출자가 자동 설치로 잇는다 · C단계). 모든 feature가 켜진 빌드에서는 닿지 않는다.
        #[allow(unreachable_patterns)]
        other => open_extension(other, spec),
    }
}

/// ★ 확장 드라이버 패키지 id(D-350 · 색인·`drivers/<id>/` 폴더 이름) — 방언마다 후보(Oracle = Thin·OCI 둘 · 설정 `oracle.driver` 순).
pub const DRIVER_IDS: &[(Dialect, &[&str])] = &[
    (Dialect::Sqlite, &["sqlite"]),
    (Dialect::Postgres, &["pg"]),
    (Dialect::Mysql, &["mysql"]),
    (Dialect::Mssql, &["mssql"]),
    (Dialect::Oracle, &["oracle-thin", "oracle-oci"]),
    (Dialect::Odbc, &["odbc"]),
    // DuckDB = 확장 전용(내장 feature 없음 · libduckdb ≈ 50 MB).
    (Dialect::Duckdb, &["duckdb"]),
    // ClickHouse = 확장 전용(D-354 · HTTP 인터페이스 · 외부 crate 0 · 설치본 정책상 확장).
    (Dialect::Clickhouse, &["clickhouse"]),
    // ★ Db2 = 네이티브 DRDA 확장 `db2`(D-361 · 외부 런타임 0) · ODBC 어댑터(D-359 · clidriver)는 `?driver=odbc` 명시일 때만(내장 분기).
    (Dialect::Db2, &["db2"]),
    (Dialect::Access, &["odbc"]),
];

/// 방언의 확장 후보 id(우선순위 순 · Oracle은 설정이 고른 드라이버가 먼저).
#[must_use]
pub fn ext_ids(dialect: Dialect) -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = DRIVER_IDS
        .iter()
        .find(|(d, _)| *d == dialect)
        .map_or(Vec::new(), |(_, ids)| ids.to_vec());
    if dialect == Dialect::Oracle && oracle_driver() == OracleDriver::Oci {
        ids.reverse();
    }
    ids
}

/// ★ `?driver=`가 가리키는 후보를 맨 앞으로(`thin`/`oci` 별칭 포함) — 모르는 값은 무시(순서 그대로).
#[must_use]
pub fn ext_ids_for(dialect: Dialect, want: Option<&str>) -> Vec<&'static str> {
    let mut ids = ext_ids(dialect);
    let want = match want {
        Some("thin") => Some("oracle-thin"),
        Some("oci") => Some("oracle-oci"),
        other => other,
    };
    if let Some(w) = want {
        if let Some(pos) = ids.iter().position(|i| *i == w) {
            let id = ids.remove(pos);
            ids.insert(0, id);
        }
    }
    ids
}

/// 확장 id → 이 저장소의 크레이트 이름(cdylib 파일 = `library_file_name(크레이트)` · 패키징이 쓴다 · `oracle-oci`만 다르다).
#[must_use]
pub fn ext_crate_name(id: &str) -> String {
    match id {
        "oracle-oci" => "nsql-driver-oracle".to_string(),
        other => format!("nsql-driver-{other}"),
    }
}

/// 확장 id → 방언(색인·매니페스트 없이도 폴더 이름만으로).
#[must_use]
pub fn dialect_of_ext(id: &str) -> Option<Dialect> {
    DRIVER_IDS
        .iter()
        .find(|(_, ids)| ids.contains(&id))
        .map(|(d, _)| *d)
}

/// "이 빌드에 드라이버가 없고 확장도 설치되지 않았다" — 호출자(GUI·CLI)가 자동 설치로 잇는 오류 코드(D-350 C단계).
pub const ERR_DRIVER_MISSING: i64 = -1350;

/// 방언의 드라이버 상태.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverStatus {
    /// 이 빌드에 컴파일돼 있다.
    Builtin,
    /// 확장이 설치돼 있다(첫 접속 때 올린다).
    Extension { id: String, version: String },
    /// 둘 다 없다 — 후보 id(설치하면 된다).
    Missing { ids: Vec<&'static str> },
}

/// 확장 id가 **이 빌드에 내장**돼 있으면 그 버전(= `nsql_abi::DRIVER_VERSION` · 내장·확장 공통 번호 · 앱 버전과 독립) — 드라이버 관리자
/// 창의 "내장" 열·상태(사용자 10-23 "내장 여부와 내장/확장 버전 차이" · "내장도 자체 버전으로").
#[must_use]
pub fn builtin_version(id: &str) -> Option<&'static str> {
    let yes = match id {
        "sqlite" => cfg!(feature = "sqlite"),
        "pg" => cfg!(feature = "pg"),
        "mysql" => cfg!(feature = "mysql"),
        "mssql" => cfg!(feature = "mssql"),
        "odbc" => cfg!(feature = "odbc"),
        "oracle-thin" => OracleDriver::Thin.included(),
        "oracle-oci" => OracleDriver::Oci.included(),
        _ => false, // duckdb · clickhouse = 확장 전용(D-350 · D-354)
    };
    yes.then_some(nsql_abi::DRIVER_VERSION)
}

/// 이 빌드에 내장된 방언인가(Oracle = 설정이 고른 드라이버 기준).
#[must_use]
pub fn is_builtin(dialect: Dialect) -> bool {
    match dialect {
        Dialect::Sqlite => cfg!(feature = "sqlite"),
        Dialect::Postgres => cfg!(feature = "pg"),
        Dialect::Mysql => cfg!(feature = "mysql"),
        Dialect::Mssql => cfg!(feature = "mssql"),
        Dialect::Odbc | Dialect::Access => cfg!(feature = "odbc"), // 전 OS(10-23 · unixODBC · 10-25 정적 동봉)
        // Db2 = 확장 전용(네이티브 DRDA · D-361) · ODBC는 `?driver=odbc`일 때만 내장 경로.
        Dialect::Duckdb | Dialect::Clickhouse | Dialect::Db2 => false,
        Dialect::Oracle => oracle_driver().included(),
    }
}

/// 내장 → 확장 설치본 → 없음(폴더 한 번 읽기 · 접속 직전에만 부른다).
#[must_use]
pub fn driver_status(dialect: Dialect) -> DriverStatus {
    if is_builtin(dialect) {
        return DriverStatus::Builtin;
    }
    let ids = ext_ids(dialect);
    for id in &ids {
        if let Some(e) = ext::find_extension(id) {
            return DriverStatus::Extension {
                id: e.id,
                version: e.version,
            };
        }
    }
    DriverStatus::Missing { ids }
}

/// [`driver_status`]의 `?driver=` 변형(`want` = thin/oci/확장 id): 그 드라이버가 내장이면 Builtin · 설치돼 있으면 Extension · 아니면 Missing.
#[must_use]
pub fn driver_status_for(dialect: Dialect, want: Option<&str>) -> DriverStatus {
    let ids = ext_ids_for(dialect, want);
    let builtin = match want {
        Some("thin") | Some("oracle-thin") if dialect == Dialect::Oracle => {
            OracleDriver::Thin.included()
        }
        Some("oci") | Some("oracle-oci") if dialect == Dialect::Oracle => {
            OracleDriver::Oci.included()
        }
        Some(id) => {
            builtin_version(id).is_some() || (dialect_of_ext(id).is_none() && is_builtin(dialect))
        }
        None => is_builtin(dialect),
    };
    if builtin {
        return DriverStatus::Builtin;
    }
    for id in &ids {
        if let Some(e) = ext::find_extension(id) {
            return DriverStatus::Extension {
                id: e.id,
                version: e.version,
            };
        }
    }
    DriverStatus::Missing { ids }
}

/// 자동 설치가 실패한 오류 코드(호스트가 [`ERR_DRIVER_MISSING`]과 다른 안내를 낸다 · 상세는 message).
pub const ERR_DRIVER_INSTALL_FAILED: i64 = -1351;

/// 드라이버 없음 오류(코드 [`ERR_DRIVER_MISSING`] · 메시지에 설치 명령 · `want` = `?driver=` 지정이면 그 id를 안내).
#[must_use]
pub fn driver_missing(dialect: Dialect, want: Option<&str>) -> DbError {
    let ids = ext_ids_for(dialect, want);
    DbError {
        code: Some(ERR_DRIVER_MISSING),
        message: format!(
            // 고른 드라이버(예: lite의 Thin) 기준으로 말한다 — "not in this build"는 lite에 내장 OCI가 있어 틀린 말이었다(협업 10-23).
            "{} driver is not installed - install it: nsql driver install {} (or View ▸ Driver Manager)",
            ids.first().map_or_else(|| dialect.to_string(), |id| (*id).to_string()),
            ids.first().unwrap_or(&"<id>")
        ),
        position: None,
    }
}

/// ★ 접속 시 자동 설치 정책(D-350 C단계 · 설정 `driver.auto_install` · `driver.repo` · `driver.http_timeout_secs`): 호스트가 기동 때 넣는다 ·
/// `None` = 끔(없으면 [`driver_missing`] 오류 그대로).
static AUTO_INSTALL: std::sync::Mutex<Option<ext::install::Source>> = std::sync::Mutex::new(None);

pub fn set_auto_install(src: Option<ext::install::Source>) {
    *AUTO_INSTALL.lock().unwrap_or_else(|e| e.into_inner()) = src;
}

#[must_use]
pub fn auto_install_source() -> Option<ext::install::Source> {
    AUTO_INSTALL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// 자동 설치 진행 콜백: `(드라이버 id, 단계 = index/download/verify/install/installed, 받은 바이트, 전체)`.
pub type DriverProgress<'a> = &'a mut dyn FnMut(&str, &str, u64, Option<u64>);

/// ★ 접속 한 자리(D-350 · 사용자 10-23 "시도 전에 점검 → 설치 → 접속"): **먼저** 드라이버 상태를 본다([`driver_status_for`]) — 없으면 접속을
/// 시작하지 않고 자동 설치(설정이 켜져 있을 때 · 진행은 콜백)를 끝낸 뒤 연다. 점검 뒤에도 [`ERR_DRIVER_MISSING`]이면(설치본 손상 등) 한 번 더 같은
/// 설치 길을 탄다. 설치 실패 = [`ERR_DRIVER_INSTALL_FAILED`](상세 = 원래 오류 + 설치 오류). 네트워크는 사용자의 접속 동작 안에서만(26 §8).
pub fn open_auto(
    spec: &ConnectSpec,
    default_dialect: Dialect,
    progress: DriverProgress<'_>,
) -> Result<Box<dyn Session>, DbError> {
    let dialect = spec.dialect.unwrap_or(default_dialect);
    let want = spec.driver.as_deref();
    if matches!(
        driver_status_for(dialect, want),
        DriverStatus::Missing { .. }
    ) {
        ensure_driver(dialect, want, driver_missing(dialect, want), progress)?;
    }
    let first = open(spec, default_dialect);
    let Err(e) = first else { return first };
    if e.code != Some(ERR_DRIVER_MISSING) {
        return Err(e);
    }
    ensure_driver(dialect, want, e, progress)?;
    open(spec, default_dialect)
}

/// 드라이버 자동 설치(없을 때): 첫 후보 id를 내려받아 설치한다. `base` = 설치하지 않을/못할 때 돌려줄 "드라이버 없음" 오류.
fn ensure_driver(
    dialect: Dialect,
    want: Option<&str>,
    base: DbError,
    progress: DriverProgress<'_>,
) -> Result<(), DbError> {
    let Some(src) = auto_install_source() else {
        return Err(base);
    };
    let Some(root) = ext::driver_dir() else {
        return Err(base);
    };
    let ids = ext_ids_for(dialect, want);
    let Some(id) = ids.first() else {
        return Err(base);
    };
    let rep = ext::install::install(
        &src,
        &root,
        id,
        None,
        false,
        false,
        ext::keys::DRIVER_KEYS,
        &mut |stage, n, total| progress(id, stage, n, total),
    )
    .map_err(|ie| DbError {
        code: Some(ERR_DRIVER_INSTALL_FAILED),
        message: format!("{} - auto-install failed: {ie}", base.message),
        position: None,
    })?;
    progress(id, "installed", rep.bytes, Some(rep.bytes));
    Ok(())
}

/// 확장 후보를 차례로 열어 본다 · 전부 없으면 [`driver_missing`].
fn open_extension(dialect: Dialect, spec: &ConnectSpec) -> Result<Box<dyn Session>, DbError> {
    for id in ext_ids_for(dialect, spec.driver.as_deref()) {
        if let Some(s) = ext::open_ext(id, spec)? {
            return Ok(s);
        }
    }
    Err(driver_missing(dialect, spec.driver.as_deref()))
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
    ORACLE_DRIVER_THIN.store(
        d == OracleDriver::Thin,
        std::sync::atomic::Ordering::Relaxed,
    );
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

/// Oracle 열기 계획(순수 · 시험용): 고른 드라이버가 내장이면 그것 · 아니면 확장 먼저, 그다음 내장된 다른 판.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OraclePlan {
    Builtin(OracleDriver),
    ExtensionThenBuiltin { fallback: Option<OracleDriver> },
}

fn oracle_plan(want: OracleDriver, included: impl Fn(OracleDriver) -> bool) -> OraclePlan {
    if included(want) {
        return OraclePlan::Builtin(want);
    }
    let other = match want {
        OracleDriver::Thin => OracleDriver::Oci,
        OracleDriver::Oci => OracleDriver::Thin,
    };
    OraclePlan::ExtensionThenBuiltin {
        fallback: included(other).then_some(other),
    }
}

#[cfg(any(feature = "oracle", feature = "oracle-thin"))]
fn open_oracle(spec: &ConnectSpec) -> Result<Box<dyn Session>, DbError> {
    // ★ `?driver=thin|oci`(접속 하나에만) > 설정(`oracle.driver`).
    let want = match spec.driver.as_deref() {
        Some("oci") | Some("oracle-oci") => OracleDriver::Oci,
        Some("thin") | Some("oracle-thin") => OracleDriver::Thin,
        _ => oracle_driver(),
    };
    let chosen = match oracle_plan(want, OracleDriver::included) {
        OraclePlan::Builtin(d) => d,
        OraclePlan::ExtensionThenBuiltin { fallback } => {
            // 🔧 10-23(사용자 "Thin 설치했는데 OCI DPI-1047"): 고른 드라이버가 내장이 아니면 **설치된 확장을 먼저**(종전엔 내장된 다른 판으로
            //   조용히 폴백해 lite 설치본에서 Thin 확장을 두고도 OCI로 갔다) · 확장이 없을 때만 내장 폴백 · 그것도 없으면 드라이버 없음.
            for id in ext_ids_for(Dialect::Oracle, Some(want.as_str())) {
                if let Some(s) = ext::open_ext(id, spec)? {
                    return Ok(s);
                }
            }
            match fallback {
                Some(d) => d,
                None => return Err(driver_missing(Dialect::Oracle, spec.driver.as_deref())),
            }
        }
    };
    match chosen {
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

/// ★ MySQL/MariaDB TLS(T-388 · 설정 `mysql.tls` + `mysql.tls_ca`): 접속 문자열 `?tls=`가 있으면 그쪽이 이긴다 · 다음 접속부터.
pub fn set_mysql_tls(mode: &str, ca_path: &str) {
    #[cfg(feature = "mysql")]
    {
        let m = nsql_driver_mysql::TlsMode::parse(mode)
            .unwrap_or(nsql_driver_mysql::TlsMode::Preferred);
        let ca = ca_path.trim();
        let ca = (!ca.is_empty()).then(|| std::path::Path::new(ca));
        nsql_driver_mysql::set_tls(m, ca);
    }
    #[cfg(not(feature = "mysql"))]
    let _ = (mode, ca_path);
}

/// 네트워크 생존 옵션(docs/53): TCP keepalive 유휴(초 · PG·SQL Server · 0 = 끔) · Oracle 호출 상한(초 · 0 = 없음). 다음 접속부터.
pub fn set_net_options(keepalive_secs: u64, call_timeout_secs: u64) {
    #[cfg(feature = "pg")]
    nsql_driver_pg::set_keepalive_secs(keepalive_secs);
    #[cfg(feature = "mssql")]
    nsql_driver_mssql::set_keepalive_secs(keepalive_secs);
    #[cfg(feature = "mysql")]
    nsql_driver_mysql::set_keepalive_secs(keepalive_secs);
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
/// ★ ODBC 취소 방식(10-24 · `odbc.cancel` = auto|on|off): auto = unix의 Microsoft SQL Server ODBC 드라이버만 끔(SQLCancel 경합 SIGSEGV).
pub fn set_odbc_cancel(mode: &str) {
    #[cfg(feature = "odbc")]
    nsql_driver_odbc::set_cancel_mode(mode);
    #[cfg(not(feature = "odbc"))]
    let _ = mode;
}

pub fn set_mssql_cancel_socket(socket: bool) {
    #[cfg(feature = "mssql")]
    nsql_driver_mssql::set_cancel_socket(socket);
    #[cfg(not(feature = "mssql"))]
    let _ = socket;
}

/// ★ D-350: 접속 폼·CLI가 고를 수 있는 방언 = **이 OS에서 쓸 수 있는 전부**(내장이든 확장 자동 설치든 · ODBC는 Windows만).
#[must_use]
pub fn available() -> Vec<Dialect> {
    Dialect::ALL.to_vec()
}

/// 이 빌드에 컴파일된 방언(설정 창 DBMS 그룹 · 진단).
#[must_use]
pub fn builtin_dialects() -> Vec<Dialect> {
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
    // MySQL/MariaDB(135차) — 빠뜨리면 접속 폼 콤보가 "(이 빌드에 드라이버 없음)"을 붙인다(협업 V1 10-22).
    if cfg!(feature = "mysql") {
        v.push(Dialect::Mysql);
    }
    if cfg!(all(feature = "odbc", windows)) {
        v.push(Dialect::Odbc);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_auto_passes_through_builtin_and_missing_has_code() {
        let spec = ConnectSpec::parse("sqlite::memory:").expect("spec");
        let mut calls = 0;
        let s = open_auto(&spec, Dialect::Sqlite, &mut |_, _, _, _| calls += 1).expect("open");
        assert_eq!(s.dialect(), Dialect::Sqlite);
        assert_eq!(calls, 0, "내장이면 설치 진행 콜백이 없다");
        let e = driver_missing(Dialect::Postgres, None);
        assert_eq!(e.code, Some(ERR_DRIVER_MISSING));
        assert!(
            e.message.contains("nsql driver install pg"),
            "{}",
            e.message
        );
        assert_eq!(ext_ids(Dialect::Oracle).len(), 2);
        assert_eq!(ext_ids_for(Dialect::Oracle, Some("oci"))[0], "oracle-oci");
        assert_eq!(ext_ids_for(Dialect::Oracle, Some("thin"))[0], "oracle-thin");
        assert_eq!(ext_ids_for(Dialect::Postgres, Some("nope")), ["pg"]);
        assert!(available().contains(&Dialect::Postgres));
        assert!(available().contains(&Dialect::Odbc));
        assert!(matches!(
            driver_status(Dialect::Sqlite),
            DriverStatus::Builtin
        ));
        assert_eq!(ext_crate_name("oracle-oci"), "nsql-driver-oracle");
        assert_eq!(ext_crate_name("pg"), "nsql-driver-pg");
    }

    /// 기본 feature = 내장 드라이버 다섯(135차 MySQL 포함) — 접속 폼 콤보 "(no driver)" 라벨의 원천.
    /// ★ `?driver=` 변형별 상태(사용자 10-23 점검 → 설치 → 접속): duckdb = 늘 확장(없으면 Missing · 후보 id duckdb) · Oracle 변형은 feature별.
    #[test]
    fn driver_status_for_respects_variant() {
        let d = driver_status_for(Dialect::Duckdb, None);
        assert!(
            matches!(&d, DriverStatus::Missing { ids } if ids == &vec!["duckdb"])
                || matches!(d, DriverStatus::Extension { .. })
        );
        for (want, inc) in [
            ("thin", OracleDriver::Thin.included()),
            ("oci", OracleDriver::Oci.included()),
        ] {
            let st = driver_status_for(Dialect::Oracle, Some(want));
            assert_eq!(matches!(st, DriverStatus::Builtin), inc, "{want}");
        }
        assert_eq!(
            matches!(
                driver_status_for(Dialect::Sqlite, None),
                DriverStatus::Builtin
            ),
            is_builtin(Dialect::Sqlite)
        );
    }

    /// 🔧 10-23 lite 설치본(OCI만 내장) + 설정 thin + Thin 확장 설치: 확장을 먼저 쓴다(종전 = OCI 폴백 → DPI-1047).
    #[test]
    fn oracle_plan_prefers_extension_when_wanted_driver_not_builtin() {
        use OracleDriver::{Oci, Thin};
        // lite: OCI만 내장.
        let lite = |d: OracleDriver| d == Oci;
        assert_eq!(
            oracle_plan(Thin, lite),
            OraclePlan::ExtensionThenBuiltin {
                fallback: Some(Oci)
            }
        );
        assert_eq!(oracle_plan(Oci, lite), OraclePlan::Builtin(Oci));
        // full: 둘 다 내장 → 고른 대로.
        assert_eq!(oracle_plan(Thin, |_| true), OraclePlan::Builtin(Thin));
        // 아무것도 내장 아님 → 확장만 · 폴백 없음(없으면 드라이버 없음 오류).
        assert_eq!(
            oracle_plan(Thin, |_| false),
            OraclePlan::ExtensionThenBuiltin { fallback: None }
        );
    }

    /// 내장 버전 표(드라이버 관리자 창용)는 `is_builtin`·DuckDB 확장 전용과 맞는다.
    #[test]
    fn builtin_version_matches_is_builtin() {
        for (d, ids) in DRIVER_IDS {
            let any = ids.iter().any(|id| builtin_version(id).is_some());
            if *d == Dialect::Oracle {
                assert_eq!(
                    any,
                    OracleDriver::Thin.included() || OracleDriver::Oci.included()
                );
            } else {
                assert_eq!(any, is_builtin(*d), "{d}");
            }
        }
        assert_eq!(builtin_version("duckdb"), None);
        assert_eq!(builtin_version("nope"), None);
        if let Some(v) = builtin_version("sqlite") {
            assert_eq!(v, nsql_abi::DRIVER_VERSION);
            assert_ne!(
                v,
                env!("CARGO_PKG_VERSION"),
                "드라이버 버전은 앱 버전과 독립(같아도 되지만 지금은 다르다)"
            );
        }
    }

    #[test]
    fn available_lists_every_builtin_driver() {
        let v = available();
        for d in [
            Dialect::Sqlite,
            Dialect::Oracle,
            Dialect::Mssql,
            Dialect::Postgres,
            Dialect::Mysql,
        ] {
            assert!(v.contains(&d), "{d} 빠짐: {v:?}");
        }
        // ODBC = 전 OS(10-23 · 3a9f127: Linux/macOS는 unixODBC · 확장 드라이버) — 종전 "Windows만" 단언(T-383 · D-345)은 폐기.
        assert!(v.contains(&Dialect::Odbc), "ODBC 빠짐: {v:?}");
        assert!(v.contains(&Dialect::Duckdb), "DuckDB 빠짐: {v:?}");
    }

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
