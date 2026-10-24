//! IBM Db2 어댑터 — **네이티브 DRDA**(10-25 · 138차 · D-361 · 사용자 "Nexa SQL과 드라이버 파일만으로 실행 · Oracle/MSSQL처럼").
//!
//! - 경로 = DRDA(Open Group 공개 표준 · Db2 JDBC 타입 4·pydrda·Derby 클라이언트와 같은 길) over TCP(+ TLS = `nsql-net` rustls) —
//!   IBM clidriver·unixODBC **불필요**. 외부 crate 0(DR-3 예외 없음).
//! - 방언 = [`Dialect::Db2`](nsql_core::Dialect::Db2)(138차 ODBC 경로에서 만든 SYSCAT 카탈로그 · SQLCODE 분류 · `db2.sqlg` · 페이징 · 세이브포인트를
//!   그대로 재사용 · 전송층만 교체) · ODBC 경로(`nsql-driver-odbc` + `Dialect::Db2`)는 Windows 폴백.
//! - 구조 = [`dss`](DSS/DDM 코덱 · 순수) · [`ebcdic`](CP500 ↔ UTF-8) · [`codepoint`] · [`conn`](EXCSAT → ACCSEC → SECCHK → ACCRDB) ·
//!   [`fdoca`](QRYDSC/QRYDTA/SQLDARD/SQLDTA) · [`sqlca`] · [`session`]([`nsql_core::Session`] 구현 · `export_driver!("db2", "db2", …)`).
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod codepoint;
pub mod conn;
pub mod dss;
pub mod ebcdic;
pub mod fdoca;
pub mod session;
pub mod sqlca;

pub use session::Db2Session;
