//! ODBC 어댑터(T-383 · 136차 · docs/121 P2) — 뒤의 DBMS를 모르는 **3급 틀**: Tibero · Altibase · CUBRID · Db2 · HANA · Access · Excel …
//! 벤더 ODBC 드라이버가 있는 곳이면 접속·조회·DML·export가 된다.
//!
//! - 바인딩 = `odbc-api` 29(MIT · DR-3 예외) · Windows = 인박스 odbc32 · 그 밖 = unixODBC 정적(vendored).
//! - 접속 문자열 = [`connection_string`]: `ConnectSpec.database`가 `=`를 품으면 ODBC 접속 문자열 전체(`Driver={…};Server=…`) ·
//!   아니면 DSN 이름(`DSN=…`) · `UID`/`PWD`는 스펙의 user/password로(이미 들어 있으면 그대로). 접속 문자열 꼴 = `odbc:DSN=Sales` ·
//!   `odbc:Driver={PostgreSQL Unicode};Server=h;Port=5432;Database=d;Uid=u;Pwd=p`.
//! - 바인드 = `?` 위치(ODBC 표준) · 값 = 정수 i64 · 실수 f64 · 문자열 · 바이트 · 비트 · NULL(문자열 NULL).
//! - 결과 = 열 버퍼 블록 페치([`FETCH_ROWS`]행씩): 정수 → `Int` · 실수 → `Float` · 비트 → `Bool` · 이진 → `Bytes` ·
//!   DECIMAL/NUMERIC → `Decimal`(문자열) · 그 밖(문자열·날짜·시각·모름) → `Str`(UTF-16 wide 텍스트 · 상한 [`MAX_TEXT_CHARS`] · 잘리면 오류).
//! - 다중 결과 집합 = `more_results` · 영향 행 수 = 커서 없는 문장의 `SQLRowCount`.
//! - 트랜잭션 = 접속 때 `SQL_ATTR_AUTOCOMMIT` 끔 → [`Session::commit`]/[`Session::rollback`]이 ODBC API로(Caps ODBC = 시작문 없음).
//! - 1차 제한 = 실행 취소 없음(`DriverLimit::Cancel`) · 서버 커서(REFCURSOR) 없음 · 벌크 = 러너 다중 행 폴백 · 긴 LOB 스트리밍 없음.
//! - ★ D-345/D-346 ⓐ(10-24): 내장은 **Windows만**(인박스 odbc32) · macOS/Linux는 **확장 cdylib**(unixODBC 동적 링크 · 시스템 `libodbc` 필요 · D-350 자동 설치).
mod imp;
pub use imp::{connection_string, set_cancel_mode, OdbcSession};

// ★ 확장 드라이버 내보내기(D-350 · ABI v2): cdylib로 빌드하면 `nsql_driver_vtable`이 나간다(id = 패키지 id · 방언 = 레지스트리 키).
fn nsql_abi_open_from_spec(
    spec: &nsql_script::ConnectSpec,
) -> Result<imp::OdbcSession, nsql_core::DbError> {
    imp::OdbcSession::connect(spec)
}
nsql_abi::export_driver!("odbc", "odbc", nsql_abi_open_from_spec);
