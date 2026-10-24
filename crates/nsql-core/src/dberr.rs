//! DBMS 오류 정규화(docs/42 · 사용자 09-16 "각 DBMS 오류를 최대한 공통화하되 개별 특징도 포용") — 의존 0.
//!
//! 드라이버가 준 원본(코드 · 메시지)은 그대로 두고, 그 위에 **공통 분류**([`ErrorClass`])와 **대상 이름**(테이블·컬럼·제약 등)을
//! 얹는다. 표시는 호출자 몫(GUI 상태줄·토스트 · CLI 오류 줄 · i18n 라벨). 분류는 방언별 코드 표 + 메시지 패턴(코드가 없는
//! PostgreSQL/SQLite는 메시지로) · 모르면 [`ErrorClass::Unknown`](원본만 보여 준다).

use crate::Dialect;

/// 공통 분류(방언 무관).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorClass {
    /// 테이블/뷰 없음.
    NoTable,
    /// 컬럼 없음(부적합한 식별자).
    NoColumn,
    /// 그 밖의 객체 없음(프로시저·함수·시퀀스·스키마 …).
    NoObject,
    /// 구문 오류.
    Syntax,
    /// 권한 없음.
    Permission,
    /// 로그인 실패(계정/비밀번호).
    Login,
    /// 접속 끊김·서버 불가(네트워크·리스너).
    Connection,
    /// 유니크/PK 위반.
    Unique,
    /// 외래 키 위반.
    ForeignKey,
    /// NOT NULL 위반.
    NotNull,
    /// CHECK 위반.
    Check,
    /// 잠금 대기/타임아웃.
    Lock,
    /// 교착 상태.
    Deadlock,
    /// 값/형 변환 오류(숫자·날짜 형식 · 길이 초과).
    DataType,
    /// 자원 부족(테이블스페이스·디스크·메모리).
    Resource,
    /// ★ 클라이언트 드라이버·DSN 없음(10-24 맥 ODBC 실기 · "Can't open lib" · IM002 · 확장 "no build for this OS") — 서버 쪽이 아니라
    /// 이 PC의 설치·등록 문제라 "접속 끊김"으로 분류하지 않는다.
    Driver,
    /// 분류 안 됨(원본만).
    Unknown,
}

impl ErrorClass {
    /// 안정 식별자(로그·설정·테스트용 · 번역 아님).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            ErrorClass::NoTable => "no_table",
            ErrorClass::NoColumn => "no_column",
            ErrorClass::NoObject => "no_object",
            ErrorClass::Syntax => "syntax",
            ErrorClass::Permission => "permission",
            ErrorClass::Login => "login",
            ErrorClass::Connection => "connection",
            ErrorClass::Unique => "unique",
            ErrorClass::ForeignKey => "foreign_key",
            ErrorClass::NotNull => "not_null",
            ErrorClass::Check => "check",
            ErrorClass::Lock => "lock",
            ErrorClass::Deadlock => "deadlock",
            ErrorClass::DataType => "data_type",
            ErrorClass::Resource => "resource",
            ErrorClass::Driver => "driver",
            ErrorClass::Unknown => "unknown",
        }
    }

    /// 객체 없음 계열인가(토스트 대상 · 사용자 09-16).
    #[must_use]
    pub fn is_missing_object(self) -> bool {
        matches!(
            self,
            ErrorClass::NoTable | ErrorClass::NoColumn | ErrorClass::NoObject
        )
    }
}

/// 정규화 결과 — 분류 + 대상 이름(있으면) + 원본 코드 표기 · 원본 메시지는 호출자가 그대로 가진다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classified {
    pub class: ErrorClass,
    /// 사용자가 아는 형태의 코드(`ORA-00942` · `PLS-00201` · `Msg 208` · `SQLSTATE 42P01` · `#1146` · `SQLITE 1`) — 부각 표시용(사용자 09-16).
    pub code: Option<String>,
    /// 테이블·컬럼·제약 이름 등(메시지에서 뽑거나 실행문에서 추정).
    pub object: Option<String>,
}

/// 메시지 안의 첫 따옴표 이름(`'x'` · `"x"` · `[x]` · `«x»` 순).
fn quoted(m: &str) -> Option<String> {
    let cs: Vec<char> = m.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let close = match cs[i] {
            '\'' => '\'',
            '"' => '"',
            '[' => ']',
            '«' => '»',
            _ => {
                i += 1;
                continue;
            }
        };
        if let Some(j) = cs[i + 1..].iter().position(|c| *c == close) {
            let name: String = cs[i + 1..i + 1 + j].iter().collect();
            if !name.trim().is_empty() {
                return Some(name);
            }
        }
        i += 1;
    }
    None
}

/// 실행문에서 대상 테이블 추정(`FROM t` · `INTO t` · `UPDATE t`) — 메시지에 이름이 없을 때(Oracle ORA-00942).
fn table_of(stmt: &str) -> Option<String> {
    let toks: Vec<&str> = stmt
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .filter(|t| !t.is_empty())
        .collect();
    for (i, tok) in toks.iter().enumerate() {
        let up = tok.to_ascii_uppercase();
        let want = matches!(up.as_str(), "FROM" | "UPDATE" | "INTO" | "TABLE");
        if !want {
            continue;
        }
        if let Some(n) = toks.get(i + 1) {
            let nu = n.to_ascii_uppercase();
            if matches!(nu.as_str(), "SELECT" | "WHERE" | "DUAL" | "VALUES" | "IF") {
                continue;
            }
            if n.chars()
                .all(|c| c.is_alphanumeric() || "._$#\"[]`".contains(c))
            {
                return Some((*n).to_string());
            }
        }
    }
    None
}

/// 메시지 첫머리의 방언 코드 표기(`ORA-00942` · `PLS-00201` · `TNS-12541` · `SQLSTATE[42P01]`)를 찾거나 숫자 코드로 만든다.
#[must_use]
pub fn native_code(dialect: Dialect, code: Option<i64>, message: &str) -> Option<String> {
    // 접두 대문자 2~5자 + '-' + 숫자 4~6자(Oracle 계열).
    let t = message.trim_start();
    let head: String = t
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || *c == '-' || c.is_ascii_digit())
        .collect();
    if let Some((p, n)) = head.split_once('-') {
        if (2..=5).contains(&p.len())
            && (4..=6).contains(&n.len())
            && n.chars().all(|c| c.is_ascii_digit())
        {
            return Some(format!("{p}-{n}"));
        }
    }
    if let Some(i) = message.find("SQLSTATE") {
        let rest: String = message[i + 8..]
            .chars()
            .skip_while(|c| !c.is_ascii_alphanumeric())
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if rest.len() == 5 {
            return Some(format!("SQLSTATE {rest}"));
        }
    }
    // 내부 코드(음수 · 드라이버 없음 -1350 · 자동 설치 실패 -1351 등)는 서버 코드가 아니다 → 방언 접두 없이 숫자만(10-23 "ORA--1350").
    if code.is_some_and(|c| c < 0) {
        return code.map(|c| c.to_string());
    }
    match (dialect, code) {
        (Dialect::Oracle, Some(c)) => Some(format!("ORA-{c:05}")),
        (Dialect::Mssql, Some(c)) => Some(format!("Msg {c}")),
        (Dialect::Mysql, Some(c)) => Some(format!("#{c}")),
        (Dialect::Sqlite, Some(c)) => Some(format!("SQLITE {c}")),
        (Dialect::Postgres | Dialect::Odbc, Some(c)) if c != 0 => Some(format!("{c}")),
        _ => None,
    }
}

/// ODBC 진단 원문(`State: XXXXX, Native error: n, Message: …`)에서 SQLSTATE(5자)를 뽑는다(순수).
#[must_use]
pub fn odbc_sqlstate(message: &str) -> Option<String> {
    let rest = message.split("State: ").nth(1)?;
    let st: String = rest.chars().take(5).collect();
    (st.len() == 5 && st.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| st.to_ascii_uppercase())
}

/// ★ ODBC 오류 분류(순수 · 10-24): ① 벤더 꼬리표 = `[SQL Server]` → SQL Server 네이티브 코드 표 · `ORA-nnnnn` → Oracle 표 ②
///   표준 SQLSTATE 표(28000 로그인 · 08xxx/HYT00/HYT01 접속 · 42S02 테이블 없음 · 42S22 컬럼 없음 · 42501 권한 · 42000 구문 · 23xxx
///   제약(낱말로 세분) · 40001/40P01 교착 · 22xxx 값/형 · 53xxx/HY001 자원 · IM002/IM003/01000+"can't open lib" 드라이버) ③ 모르면 None
///   (호출자가 공용 메시지 규칙으로). 코드 표시는 네이티브 코드(있으면)로 통일.
#[must_use]
pub fn odbc_classify(code: Option<i64>, message: &str, stmt: &str) -> Option<Classified> {
    let low = message.to_ascii_lowercase();
    let native = code.filter(|c| *c != 0).map(|c| c.to_string());
    let with_code = |mut c: Classified| {
        if c.code.is_none() {
            c.code = native.clone();
        }
        c
    };
    if low.contains("[sql server]") {
        let c = classify(Dialect::Mssql, code, message, stmt);
        if c.class != ErrorClass::Unknown {
            return Some(with_code(c));
        }
    }
    if let Some(pos) = low.find("ora-") {
        let digits: String = low[pos + 4..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = digits.parse::<i64>() {
            let c = classify(Dialect::Oracle, Some(n), message, stmt);
            if c.class != ErrorClass::Unknown {
                return Some(c);
            }
        }
    }
    let st = odbc_sqlstate(message)?;
    let has = |s: &str| low.contains(s);
    let object = || quoted(message).or_else(|| table_of(stmt));
    let class = match st.as_str() {
        "28000" => ErrorClass::Login,
        "08001" | "08S01" | "08003" | "08004" | "08006" | "08007" | "HYT00" | "HYT01" => {
            ErrorClass::Connection
        }
        "42S02" | "42P01" => ErrorClass::NoTable,
        "42S22" | "42703" => ErrorClass::NoColumn,
        "42501" => ErrorClass::Permission,
        "42000" | "42601" => {
            if has("permission") || has("denied") || has("not authorized") {
                ErrorClass::Permission
            } else {
                ErrorClass::Syntax
            }
        }
        "40001" | "40P01" => ErrorClass::Deadlock,
        "IM002" | "IM003" | "IM004" => ErrorClass::Driver,
        "01000" if has("can't open lib") => ErrorClass::Driver,
        s if s.starts_with("23") => {
            if has("foreign") {
                ErrorClass::ForeignKey
            } else if has("null") {
                ErrorClass::NotNull
            } else if has("check") {
                ErrorClass::Check
            } else {
                ErrorClass::Unique
            }
        }
        s if s.starts_with("22") => ErrorClass::DataType,
        s if s.starts_with("53") || s == "HY001" => ErrorClass::Resource,
        s if s.starts_with("55") || s == "HY000" && has("lock") => ErrorClass::Lock,
        _ => return None,
    };
    let object = matches!(class, ErrorClass::NoTable | ErrorClass::NoColumn)
        .then(object)
        .flatten();
    Some(Classified {
        class,
        code: native,
        object,
    })
}

/// ★ ClickHouse 메시지의 객체 이름(10-24): `Table default.t does not exist` · `Database x does not exist` · `Unknown table expression identifier 't'`.
fn ch_object(message: &str) -> Option<String> {
    let low = message.to_ascii_lowercase();
    for key in ["table ", "database "] {
        if let Some(i) = low.find(key) {
            let rest = &message[i + key.len()..];
            let tok: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '(' && *c != ',')
                .collect();
            let tok = tok.trim_end_matches('.').trim_matches(['`', '"']);
            if !tok.is_empty() && !tok.eq_ignore_ascii_case("expression") {
                return Some(tok.to_string());
            }
        }
    }
    None
}

/// 오류 정규화 — `code` = 드라이버 숫자 코드(없으면 None) · `message` = 원문 · `stmt` = 실패한 문장(테이블 추정용).
#[must_use]
pub fn classify(dialect: Dialect, code: Option<i64>, message: &str, stmt: &str) -> Classified {
    let low = message.to_ascii_lowercase();
    let has = |s: &str| low.contains(s);
    let obj_q = || quoted(message);
    let obj_stmt = || table_of(stmt);
    let native = native_code(dialect, code, message);
    let mk = |class: ErrorClass, object: Option<String>| Classified {
        class,
        code: native.clone(),
        object,
    };
    // ★ ODBC(10-24 맥 실기 "[18456]에 분류 라벨이 없음"): 벤더 꼬리표(`[SQL Server]` · `ORA-nnnnn`)가 있으면 그 방언 표로 ·
    //   아니면 표준 SQLSTATE(`State: 28000` 등)로 — 둘 다 모르면 아래 공용 메시지 규칙.
    if dialect == Dialect::Odbc {
        if let Some(c) = odbc_classify(code, message, stmt) {
            return c;
        }
    }
    match dialect {
        Dialect::Oracle => match code {
            Some(942) => mk(ErrorClass::NoTable, obj_stmt()),
            Some(904) => mk(ErrorClass::NoColumn, obj_q()),
            Some(4043) | Some(2289) | Some(4080) | Some(1418) => {
                mk(ErrorClass::NoObject, obj_q().or_else(obj_stmt))
            }
            Some(6550) if has("pls-00201") => mk(ErrorClass::NoObject, obj_q()),
            Some(6550) | Some(900) | Some(933) | Some(923) | Some(907) | Some(936) | Some(921)
            | Some(917) => mk(ErrorClass::Syntax, None),
            Some(1031) | Some(1017) if code == Some(1031) => mk(ErrorClass::Permission, None),
            Some(1017) | Some(1005) | Some(28000) => mk(ErrorClass::Login, None),
            Some(1) => mk(ErrorClass::Unique, obj_q()),
            Some(2291) | Some(2292) => mk(ErrorClass::ForeignKey, obj_q()),
            Some(1400) | Some(1407) => mk(ErrorClass::NotNull, obj_q()),
            Some(2290) => mk(ErrorClass::Check, obj_q()),
            Some(54) | Some(30006) => mk(ErrorClass::Lock, None),
            Some(60) => mk(ErrorClass::Deadlock, None),
            Some(1722) | Some(1858) | Some(1861) | Some(1830) | Some(1476) | Some(12899)
            | Some(1438) => mk(ErrorClass::DataType, obj_q()),
            Some(1652) | Some(1653) | Some(1654) | Some(1688) | Some(4031) => {
                mk(ErrorClass::Resource, obj_q())
            }
            Some(3113) | Some(3114) | Some(12541) | Some(12154) | Some(12170) | Some(12514)
            | Some(1012) | Some(28) | Some(3135) => mk(ErrorClass::Connection, None),
            _ => mk(ErrorClass::Unknown, None),
        },
        Dialect::Mssql => match code {
            Some(208) => mk(ErrorClass::NoTable, obj_q().or_else(obj_stmt)),
            Some(207) => mk(ErrorClass::NoColumn, obj_q()),
            Some(2812) | Some(4121) | Some(2760) => mk(ErrorClass::NoObject, obj_q()),
            Some(102) | Some(105) | Some(156) | Some(170) | Some(319) => {
                mk(ErrorClass::Syntax, None)
            }
            Some(229) | Some(230) | Some(262) | Some(297) | Some(300) => {
                mk(ErrorClass::Permission, obj_q())
            }
            Some(18456) | Some(18452) | Some(4060) => mk(ErrorClass::Login, None),
            Some(2627) | Some(2601) => mk(ErrorClass::Unique, obj_q()),
            Some(547) if has("foreign key") => mk(ErrorClass::ForeignKey, obj_q()),
            Some(547) => mk(ErrorClass::Check, obj_q()),
            Some(515) => mk(ErrorClass::NotNull, obj_q()),
            Some(1222) => mk(ErrorClass::Lock, None),
            Some(1205) => mk(ErrorClass::Deadlock, None),
            Some(245) | Some(8114) | Some(241) | Some(242) | Some(8152) | Some(2628)
            | Some(8115) => mk(ErrorClass::DataType, obj_q()),
            Some(1105) | Some(9002) | Some(701) => mk(ErrorClass::Resource, obj_q()),
            Some(-2) | Some(53) | Some(233) | Some(10053) | Some(10054) | Some(10060)
            | Some(64) => mk(ErrorClass::Connection, None),
            _ => mk(ErrorClass::Unknown, None),
        },
        Dialect::Postgres => {
            if has("does not exist") || has("존재하지 않") {
                if has("relation") || has("table") || has("릴레이션") || has("테이블") {
                    mk(ErrorClass::NoTable, obj_q().or_else(obj_stmt))
                } else if has("column") || has("칼럼") || has("컬럼") {
                    mk(ErrorClass::NoColumn, obj_q())
                } else {
                    mk(ErrorClass::NoObject, obj_q())
                }
            } else if has("syntax error") || has("구문 오류") {
                mk(ErrorClass::Syntax, None)
            } else if has("password authentication failed")
                || has("no pg_hba.conf entry")
                || has("role \"") && has("does not exist")
            {
                mk(ErrorClass::Login, None)
            } else if has("permission denied") || has("권한") {
                mk(ErrorClass::Permission, obj_q())
            } else if has("duplicate key") || has("중복된 키") {
                mk(ErrorClass::Unique, obj_q())
            } else if has("foreign key") || has("외래 키") {
                mk(ErrorClass::ForeignKey, obj_q())
            } else if has("null value in column") || has("not-null") || has("null 값") {
                mk(ErrorClass::NotNull, obj_q())
            } else if has("check constraint") || has("체크 제약") {
                mk(ErrorClass::Check, obj_q())
            } else if has("deadlock") || has("교착") {
                mk(ErrorClass::Deadlock, None)
            } else if has("could not obtain lock")
                || has("lock timeout")
                || has("canceling statement due to lock")
            {
                mk(ErrorClass::Lock, None)
            } else if has("invalid input syntax")
                || has("out of range")
                || has("value too long")
                || has("cannot cast")
            {
                mk(ErrorClass::DataType, obj_q())
            } else if has("no space left") || has("out of memory") || has("disk full") {
                mk(ErrorClass::Resource, None)
            } else if has("connection")
                && (has("refused")
                    || has("closed")
                    || has("reset")
                    || has("timed out")
                    || has("terminated"))
            {
                mk(ErrorClass::Connection, None)
            } else {
                mk(ErrorClass::Unknown, None)
            }
        }
        Dialect::Mysql => match code {
            Some(1146) => mk(ErrorClass::NoTable, obj_q().or_else(obj_stmt)),
            Some(1054) => mk(ErrorClass::NoColumn, obj_q()),
            Some(1305) | Some(1049) | Some(1370) => mk(ErrorClass::NoObject, obj_q()),
            Some(1064) | Some(1065) => mk(ErrorClass::Syntax, None),
            Some(1142) | Some(1044) | Some(1143) | Some(1227) => {
                mk(ErrorClass::Permission, obj_q())
            }
            Some(1045) | Some(1698) => mk(ErrorClass::Login, None),
            Some(1062) | Some(1586) => mk(ErrorClass::Unique, obj_q()),
            Some(1451) | Some(1452) | Some(1216) | Some(1217) => {
                mk(ErrorClass::ForeignKey, obj_q())
            }
            Some(1048) | Some(1364) => mk(ErrorClass::NotNull, obj_q()),
            Some(3819) => mk(ErrorClass::Check, obj_q()),
            Some(1205) => mk(ErrorClass::Lock, None),
            Some(1213) => mk(ErrorClass::Deadlock, None),
            Some(1366) | Some(1292) | Some(1406) | Some(1264) => mk(ErrorClass::DataType, obj_q()),
            Some(1021) | Some(1114) | Some(1037) => mk(ErrorClass::Resource, None),
            Some(2002) | Some(2003) | Some(2006) | Some(2013) => mk(ErrorClass::Connection, None),
            _ => mk(ErrorClass::Unknown, None),
        },
        // ★ ClickHouse(10-24 · 138차): 서버 오류 코드(`Code: N.` · 헤더 X-ClickHouse-Exception-Code · 드라이버가 `code`로) → 분류 ·
        //   객체 = 메시지의 따옴표 토막 또는 `Table x`/`Database x` 다음 낱말.
        Dialect::Clickhouse => {
            let obj = || obj_q().or_else(|| ch_object(message));
            match code {
                // UNKNOWN_TABLE · UNKNOWN_DATABASE
                Some(60) | Some(81) => mk(ErrorClass::NoTable, obj()),
                // UNKNOWN_IDENTIFIER · NOT_FOUND_COLUMN_IN_BLOCK · NO_SUCH_COLUMN_IN_TABLE
                Some(47) | Some(10) | Some(16) => mk(ErrorClass::NoColumn, obj()),
                // UNKNOWN_FUNCTION · CANNOT_GET_CREATE_TABLE_QUERY
                Some(46) | Some(390) => mk(ErrorClass::NoObject, obj()),
                // SYNTAX_ERROR · NUMBER_OF_ARGUMENTS_DOESNT_MATCH · BAD_ARGUMENTS
                Some(62) | Some(42) | Some(36) => mk(ErrorClass::Syntax, None),
                // AUTHENTICATION_FAILED · UNKNOWN_USER · REQUIRED_PASSWORD · WRONG_PASSWORD
                Some(516) | Some(192) | Some(193) | Some(194) => mk(ErrorClass::Login, None),
                // ACCESS_DENIED · READONLY · QUERY_IS_PROHIBITED
                Some(497) | Some(164) | Some(392) => mk(ErrorClass::Permission, None),
                // NETWORK_ERROR · SOCKET_TIMEOUT · DNS_ERROR · ALL_CONNECTION_TRIES_FAILED
                Some(210) | Some(209) | Some(198) | Some(279) => mk(ErrorClass::Connection, None),
                // TYPE_MISMATCH · CANNOT_PARSE_TEXT · CANNOT_CONVERT_TYPE · ILLEGAL_TYPE_OF_ARGUMENT · CANNOT_PARSE_NUMBER
                Some(53) | Some(6) | Some(70) | Some(43) | Some(72) => {
                    mk(ErrorClass::DataType, None)
                }
                // MEMORY_LIMIT_EXCEEDED · TIMEOUT_EXCEEDED · TOO_MANY_SIMULTANEOUS_QUERIES · TOO_MANY_ROWS_OR_BYTES
                Some(241) | Some(159) | Some(202) | Some(396) => mk(ErrorClass::Resource, None),
                // TABLE_IS_READ_ONLY · DEADLOCK_AVOIDED
                Some(242) => mk(ErrorClass::Lock, None),
                None if has("connect") || has("network") || has("timed out") => {
                    mk(ErrorClass::Connection, None)
                }
                _ => mk(ErrorClass::Unknown, None),
            }
        }
        Dialect::Sqlite | Dialect::Odbc | Dialect::Duckdb => {
            if let Some(r) = low.strip_prefix("no such table: ") {
                mk(ErrorClass::NoTable, Some(r.trim().to_string()))
            } else if let Some(r) = low.strip_prefix("no such column: ") {
                mk(ErrorClass::NoColumn, Some(r.trim().to_string()))
            } else if has("no such ") {
                mk(
                    ErrorClass::NoObject,
                    low.split(':').nth(1).map(|s| s.trim().to_string()),
                )
            } else if has("syntax error") {
                mk(ErrorClass::Syntax, None)
            } else if has("unique constraint failed") || has("primary key must be unique") {
                mk(
                    ErrorClass::Unique,
                    low.split(':').nth(1).map(|s| s.trim().to_string()),
                )
            } else if has("foreign key constraint failed") {
                mk(ErrorClass::ForeignKey, None)
            } else if has("not null constraint failed") {
                mk(
                    ErrorClass::NotNull,
                    low.split(':').nth(1).map(|s| s.trim().to_string()),
                )
            } else if has("check constraint failed") {
                mk(
                    ErrorClass::Check,
                    low.split(':').nth(1).map(|s| s.trim().to_string()),
                )
            } else if has("database is locked") || has("busy") {
                mk(ErrorClass::Lock, None)
            } else if has("readonly") || has("access denied") || has("permission") {
                mk(ErrorClass::Permission, None)
            } else if has("datatype mismatch") || has("too big") {
                mk(ErrorClass::DataType, None)
            } else if has("disk") && has("full") || has("out of memory") {
                mk(ErrorClass::Resource, None)
            } else if has("can't open lib")
                || has("data source name not found")
                || has("no default driver")
                || has("has no build for this os")
            {
                // ODBC 드라이버 관리자(unixODBC · odbc32)의 "드라이버/DSN 없음" · 확장 드라이버 미지원 OS — 접속 끊김 아님.
                mk(ErrorClass::Driver, None)
            } else if has("unable to open database") || has("connection") {
                mk(ErrorClass::Connection, None)
            } else {
                mk(ErrorClass::Unknown, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ODBC 분류(10-24): 벤더 꼬리표(SQL Server 18456 → Login · ORA-00942 → NoTable) · SQLSTATE(28000 · 08001 · 42S02 · IM002 · 23000+unique) ·
    /// 모르는 상태 = None(공용 규칙으로).
    #[test]
    fn odbc_vendor_tag_and_sqlstate() {
        let c = classify(
            Dialect::Odbc,
            Some(18456),
            "State: 28000, Native error: 18456, Message: [Microsoft][ODBC Driver 18 for SQL Server][SQL Server]사용자 'BISCM_MS'이(가) 로그인하지 못했습니다.",
            "",
        );
        // 코드 표기 = SQL Server 표의 꼴(`Msg 18456`) 그대로.
        assert_eq!(
            (c.class, c.code.as_deref()),
            (ErrorClass::Login, Some("Msg 18456"))
        );
        let c = classify(Dialect::Odbc, Some(942), "State: 42S02, Native error: 942, Message: [Oracle][ODBC]ORA-00942: table or view does not exist", "SELECT * FROM HR.X");
        assert_eq!(c.class, ErrorClass::NoTable);
        assert_eq!(
            classify(
                Dialect::Odbc,
                None,
                "State: 28000, Native error: 0, Message: [Driver]login refused",
                ""
            )
            .class,
            ErrorClass::Login
        );
        assert_eq!(
            classify(
                Dialect::Odbc,
                None,
                "State: 08001, Native error: 0, Message: client unable to establish connection",
                ""
            )
            .class,
            ErrorClass::Connection
        );
        let c = classify(
            Dialect::Odbc,
            Some(1),
            "State: 42S02, Native error: 1, Message: no such table: 'T1'",
            "SELECT 1 FROM T1",
        );
        assert_eq!(
            (c.class, c.object.as_deref()),
            (ErrorClass::NoTable, Some("T1"))
        );
        assert_eq!(
            classify(
                Dialect::Odbc,
                None,
                "State: IM002, Native error: 0, Message: data source name not found",
                ""
            )
            .class,
            ErrorClass::Driver
        );
        assert_eq!(
            classify(
                Dialect::Odbc,
                None,
                "State: 23000, Native error: 19, Message: UNIQUE constraint failed: t.id",
                ""
            )
            .class,
            ErrorClass::Unique
        );
        assert_eq!(
            odbc_sqlstate("State: HYT00, Native error: 0, Message: timeout"),
            Some("HYT00".into())
        );
        assert_eq!(odbc_sqlstate("no state here"), None);
        // 모르는 SQLSTATE + 공용 규칙에도 없음 = Unknown.
        assert_eq!(
            classify(
                Dialect::Odbc,
                None,
                "State: HY010, Native error: 0, Message: function sequence error",
                ""
            )
            .class,
            ErrorClass::Unknown
        );
    }

    #[test]
    fn oracle_codes() {
        let c = classify(
            Dialect::Oracle,
            Some(942),
            "ORA-00942: 테이블 또는 뷰가 존재하지 않습니다",
            "SELECT * FROM HR.EMP e",
        );
        assert_eq!(c.class, ErrorClass::NoTable);
        assert_eq!(c.object.as_deref(), Some("HR.EMP"));
        let c = classify(
            Dialect::Oracle,
            Some(904),
            "ORA-00904: \"A\".\"X\": 부적합한 식별자",
            "",
        );
        assert_eq!(
            (c.class, c.object.as_deref()),
            (ErrorClass::NoColumn, Some("A"))
        );
        assert_eq!(
            classify(
                Dialect::Oracle,
                Some(1),
                "ORA-00001: unique constraint (HR.PK_EMP) violated",
                ""
            )
            .class,
            ErrorClass::Unique
        );
        assert_eq!(
            classify(Dialect::Oracle, Some(1017), "ORA-01017", "").class,
            ErrorClass::Login
        );
        assert_eq!(
            classify(
                Dialect::Oracle,
                Some(1031),
                "ORA-01031: insufficient privileges",
                ""
            )
            .class,
            ErrorClass::Permission
        );
        assert_eq!(
            classify(Dialect::Oracle, Some(3113), "end-of-file", "").class,
            ErrorClass::Connection
        );
        assert_eq!(
            classify(Dialect::Oracle, Some(99999), "x", "").class,
            ErrorClass::Unknown
        );
        // ★ ODBC 드라이버/DSN 없음 = Driver(10-24 맥 실기 · 안내문에 "connection string"이 있어도 접속 끊김이 아니다).
        assert_eq!(
            classify(
                Dialect::Odbc,
                Some(0),
                "State: 01000, Native error: 0, Message: [unixODBC][Driver Manager]Can't open lib 'SQL Server' : file not found\nInstalled ODBC drivers: none. The driver or DSN in the connection string is not registered",
                ""
            )
            .class,
            ErrorClass::Driver
        );
        assert_eq!(
            classify(
                Dialect::Odbc,
                None,
                "State: IM002 … Data source name not found and no default driver specified",
                ""
            )
            .class,
            ErrorClass::Driver
        );
        assert_eq!(c.code.as_deref(), Some("ORA-00904"));
        assert_eq!(
            native_code(Dialect::Oracle, Some(942), "ORA-00942: x").as_deref(),
            Some("ORA-00942")
        );
        assert_eq!(
            native_code(
                Dialect::Oracle,
                Some(6550),
                "PLS-00201: identifier 'X' must be declared"
            )
            .as_deref(),
            Some("PLS-00201")
        );
        assert_eq!(
            native_code(Dialect::Mssql, Some(208), "Invalid object").as_deref(),
            Some("Msg 208")
        );
        assert_eq!(
            native_code(
                Dialect::Postgres,
                None,
                "ERROR: relation x (SQLSTATE 42P01)"
            )
            .as_deref(),
            Some("SQLSTATE 42P01")
        );
    }

    #[test]
    fn mssql_pg_mysql_sqlite() {
        let c = classify(
            Dialect::Mssql,
            Some(208),
            "Invalid object name 'dbo.Emp'.",
            "",
        );
        assert_eq!(
            (c.class, c.object.as_deref()),
            (ErrorClass::NoTable, Some("dbo.Emp"))
        );
        assert_eq!(
            classify(
                Dialect::Mssql,
                Some(547),
                "The INSERT statement conflicted with the FOREIGN KEY constraint \"FK_x\"",
                ""
            )
            .class,
            ErrorClass::ForeignKey
        );
        assert_eq!(
            classify(
                Dialect::Mssql,
                Some(547),
                "conflicted with the CHECK constraint \"CK_x\"",
                ""
            )
            .class,
            ErrorClass::Check
        );
        let c = classify(
            Dialect::Postgres,
            None,
            "relation \"emp\" does not exist",
            "",
        );
        assert_eq!(
            (c.class, c.object.as_deref()),
            (ErrorClass::NoTable, Some("emp"))
        );
        assert_eq!(
            classify(
                Dialect::Postgres,
                None,
                "duplicate key value violates unique constraint \"emp_pkey\"",
                ""
            )
            .class,
            ErrorClass::Unique
        );
        assert_eq!(
            classify(
                Dialect::Postgres,
                None,
                "password authentication failed for user \"u\"",
                ""
            )
            .class,
            ErrorClass::Login
        );
        assert_eq!(
            classify(
                Dialect::Mysql,
                Some(1146),
                "Table 'db.emp' doesn't exist",
                ""
            )
            .class,
            ErrorClass::NoTable
        );
        let c = classify(Dialect::Sqlite, Some(1), "no such table: emp", "");
        assert_eq!(
            (c.class, c.object.as_deref()),
            (ErrorClass::NoTable, Some("emp"))
        );
        assert_eq!(
            classify(
                Dialect::Sqlite,
                Some(19),
                "UNIQUE constraint failed: emp.id",
                ""
            )
            .object
            .as_deref(),
            Some("emp.id")
        );
    }
}
