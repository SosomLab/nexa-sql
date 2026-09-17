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

/// 접속 문자열 → 스펙(SQLite 축약형 처리 후 `ConnectSpec::parse`).
pub fn parse_target(s: &str, default_dialect: Dialect) -> Result<ConnectSpec, String> {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("sqlite:") {
        let db = rest.trim_start_matches("//");
        return Ok(ConnectSpec {
            dialect: Some(Dialect::Sqlite),
            database: Some(db.to_string()),
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
        #[cfg(feature = "oracle")]
        Dialect::Oracle => Ok(Box::new(nsql_driver_oracle::OracleSession::connect(spec)?)),
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

/// 이 빌드에 들어 있는 방언.
/// SQL Server 암호화 범위(설정 `mssql.encrypt` · T-108): `login_only`면 로그인만 암호화 → 실행 취소가 TDS Attention으로 세션을 유지한다.
pub fn set_mssql_encryption(login_only: bool) {
    #[cfg(feature = "mssql")]
    nsql_driver_mssql::set_encryption(login_only);
    #[cfg(not(feature = "mssql"))]
    let _ = login_only;
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
    if cfg!(feature = "oracle") {
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
}
