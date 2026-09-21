//! `CONNECT` 인자 해석 — SQL*Plus logon 문법 + 방언 힌트(docs/04 §8.3 · docs/08 §5).
//!
//! 받아들이는 모양:
//! - `user/pass@host:1521/svc` · `user/pass@tns` · `user@host` (비밀번호 프롬프트)
//! - `/ as sysdba` · `user/pass@host:1521/svc as sysdba`
//! - `mssql://user:pass@host:1433/db` (스킴 = 방언) · `user/pass@host:1433/db?dialect=mssql`
//! - 따옴표로 감싼 인자: `CONNECT "oracle://scott/tiger@host:1521/svc"`(양끝 `"`·`'` 한 쌍 제거 · docs/52 §4)
//! - 짧은 스킴(`//` 없음): `oracle:host:1521/svc`(대상만 · 자격은 호스트가 채운다) · `oracle:scott/tiger@host:1521/svc`
//!   — 방언 이름 뒤 `:` 이고 ① `@`가 없거나 ② logon에 `/`가 있을 때만 스킴으로 본다(`postgres:pw@host` = 종전 user:pass 유지).
//! - `sqlite:경로` · `sqlite://경로` · `sqlite::memory:`(파일 방언 — 나머지 전부가 경로 · Windows `C:\…` 포함)

use nsql_core::Dialect;

/// **접속 유형**(docs/56 §4 2차 · 사용자 09-19): 같은 도구로 개발과 운영을 오가는 실수를 줄이는 표식 — 유형이 미커밋 경고
/// 기준과 실행 확인을 고른다. 서버 식별(같은 서버인가)에는 쓰지 않는다 — 표식일 뿐이다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConnEnv {
    Dev,
    Test,
    Prod,
}

impl ConnEnv {
    #[must_use]
    pub fn from_name(s: &str) -> Option<ConnEnv> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dev" | "development" => Some(ConnEnv::Dev),
            "test" | "qa" | "stage" | "staging" => Some(ConnEnv::Test),
            "prod" | "production" | "live" => Some(ConnEnv::Prod),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ConnEnv::Dev => "dev",
            ConnEnv::Test => "test",
            ConnEnv::Prod => "prod",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ConnectSpec {
    pub user: Option<String>,
    pub password: Option<String>,
    /// 호스트 또는 TNS 별칭. `None`이면 로컬/기본 접속(`/ as sysdba`).
    pub host: Option<String>,
    pub port: Option<u16>,
    /// Oracle 서비스명 · SQL Server/PG의 데이터베이스명.
    pub database: Option<String>,
    /// `AS SYSDBA` 등.
    pub role: Option<String>,
    /// 스킴이나 `?dialect=`로 명시된 방언. 없으면 호스트가 정한다(연결 프로필 기본값).
    pub dialect: Option<Dialect>,
    /// 접속 뒤 기본 스키마(`?schema=` · 사용자 09-18): Oracle `ALTER SESSION SET CURRENT_SCHEMA` · PostgreSQL `search_path` ·
    /// SQL Server `USE`(데이터베이스 전환) · SQLite 없음. 접속 문자열에 `?schema=HR`로 실린다.
    pub schema: Option<String>,
    /// 접속 유형(`?env=dev|test|prod` · 프로필 키 `env`) — 없으면 유형 없음(= 개발과 같은 기준).
    pub env: Option<ConnEnv>,
}

impl ConnectSpec {
    pub fn parse(arg: &str) -> Result<ConnectSpec, String> {
        let mut s = strip_quotes(arg.trim()).trim().to_string();
        if s.is_empty() {
            return Err("CONNECT: 접속 문자열이 없습니다".into());
        }
        let mut spec = ConnectSpec::default();
        let mut url_form = false;
        // 스킴이 있었는가(`://` · 짧은 `방언:`) — `@`가 없으면 나머지는 logon이 아니라 **대상**이다.
        let mut has_scheme = false;

        // 파일 방언 축약: sqlite:경로(나머지 전부가 경로 — `:`·`/`·`\\`를 해석하지 않는다).
        if let Some(i) = s.find(':') {
            if matches!(Dialect::from_name(&s[..i]), Some(Dialect::Sqlite)) {
                let rest = &s[i + 1..];
                // 종전 URL 꼴 `sqlite://host/db`(호스트는 무시되고 db만 쓰인다)는 그대로 아래 URL 경로로 — 그 외는 전부 경로.
                let legacy_url = rest
                    .strip_prefix("//")
                    .is_some_and(|r| !r.starts_with(['/', ':', '.', '~']) && r.contains('/'));
                if !legacy_url {
                    let db = rest.strip_prefix("//").unwrap_or(rest).trim();
                    spec.dialect = Some(Dialect::Sqlite);
                    spec.database = Some(if db.is_empty() { ":memory:" } else { db }.to_string());
                    return Ok(spec);
                }
            }
        }
        // 방언 스킴: mssql://…
        if let Some(i) = s.find("://") {
            url_form = true;
            has_scheme = true;
            let scheme = s[..i].to_string();
            spec.dialect = Dialect::from_name(&scheme);
            if spec.dialect.is_none() {
                return Err(format!("CONNECT: 알 수 없는 스킴 '{scheme}'"));
            }
            s = s[i + 3..].to_string();
        } else if let Some(i) = s.find(':') {
            // 짧은 스킴 `oracle:…` — 모호성 규칙은 모듈 문서 참조.
            if let Some(d) = Dialect::from_name(&s[..i]) {
                let rest = &s[i + 1..];
                let body = rest.split('?').next().unwrap_or(rest);
                let is_scheme = match body.rfind('@') {
                    None => !body.is_empty(),
                    Some(at) => body[..at].contains('/'),
                };
                if is_scheme {
                    has_scheme = true;
                    spec.dialect = Some(d);
                    s = rest.to_string();
                }
            }
        }
        // ?dialect=…
        if let Some(i) = s.find('?') {
            let q = s[i + 1..].to_string();
            s.truncate(i);
            for kv in q.split('&') {
                if let Some((k, v)) = kv.split_once('=') {
                    if k.eq_ignore_ascii_case("dialect") || k.eq_ignore_ascii_case("type") {
                        spec.dialect = Dialect::from_name(v);
                        if spec.dialect.is_none() {
                            return Err(format!("CONNECT: 알 수 없는 방언 '{v}'"));
                        }
                    } else if k.eq_ignore_ascii_case("schema") && !v.trim().is_empty() {
                        spec.schema = Some(percent_decode(v.trim()));
                    } else if k.eq_ignore_ascii_case("env") {
                        spec.env = ConnEnv::from_name(v);
                    } else if k.eq_ignore_ascii_case("role") && !v.trim().is_empty() {
                        spec.role = Some(v.trim().to_ascii_uppercase());
                    }
                }
            }
        }
        // 뒤쪽 `AS SYSDBA`
        let lower = s.to_ascii_lowercase();
        if let Some(i) = lower.rfind(" as ") {
            spec.role = Some(s[i + 4..].trim().to_ascii_uppercase());
            s.truncate(i);
        }
        let s = s.trim();
        if s == "/" {
            return Ok(spec);
        }
        // logon@target — 스킴이 있고 `@`가 없으면 전부 대상(`oracle:host:1521/svc` · `mssql://host/db`).
        let (logon, target) = match s.rfind('@') {
            Some(i) => (&s[..i], Some(&s[i + 1..])),
            None if has_scheme => ("", Some(s)),
            None => (s, None),
        };
        // user/pass 또는 user:pass(URL식)
        let (user, pass) = match logon.find('/').or_else(|| logon.find(':')) {
            Some(i) => (&logon[..i], Some(&logon[i + 1..])),
            None => (logon, None),
        };
        // URL 형식(`scheme://`)이면 `%40` 같은 퍼센트 인코딩을 푼다(비밀번호의 `@`·`/`·`:`).
        let decode = |v: &str| {
            if url_form {
                percent_decode(v)
            } else {
                v.to_string()
            }
        };
        if !user.is_empty() {
            spec.user = Some(decode(user));
        }
        // ★ 비밀번호 자리의 세 가지(사용자 09-21): `user@host` = **없음**(`None` → GUI·CLI가 물어본다) ·
        //   `user:@host`(`user/@host`) = **빈 비밀번호를 명시**(`Some("")` → 묻지 않고 그대로 접속) · `user:pw@host` = 그 값.
        spec.password = pass.map(decode);
        if let Some(t) = target {
            // host[:port][/database]
            let (hostport, db) = match t.find('/') {
                Some(i) => (&t[..i], Some(&t[i + 1..])),
                None => (t, None),
            };
            let (host, port) = match hostport.rfind(':') {
                Some(i) => {
                    let p = hostport[i + 1..]
                        .parse::<u16>()
                        .map_err(|_| format!("CONNECT: 포트 '{}'", &hostport[i + 1..]))?;
                    (&hostport[..i], Some(p))
                }
                None => (hostport, None),
            };
            if !host.is_empty() {
                spec.host = Some(host.to_string());
            }
            spec.port = port;
            spec.database = db.filter(|d| !d.is_empty()).map(str::to_string);
        }
        if spec.user.is_none() && spec.host.is_none() {
            return Err("CONNECT: 사용자 또는 대상이 필요합니다".into());
        }
        Ok(spec)
    }

    /// 폼·CLI 필드에서 조립(접속 대화상자 · `nsql conn add --host …`). GUI와 CLI가 **같은 검증**을 탄다.
    ///
    /// - 파일 기반 방언(SQLite·ODBC): `database` = 파일 경로/DSN(필수 · SQLite는 `:memory:` 허용) · host/port 무시.
    /// - 그 외: `host` 필수 · `port` 비면 [`Dialect::default_port`] · `database` = 서비스명/DB명(선택).
    /// - 빈 문자열은 `None`으로 정규화(양끝 공백 제거).
    pub fn from_parts(
        dialect: Dialect,
        host: &str,
        port: Option<u16>,
        database: &str,
        user: &str,
        password: &str,
    ) -> Result<ConnectSpec, String> {
        let opt = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        let mut spec = ConnectSpec {
            dialect: Some(dialect),
            user: opt(user),
            password: if password.is_empty() {
                None
            } else {
                Some(password.to_string())
            },
            ..ConnectSpec::default()
        };
        if dialect.is_file_based() {
            let Some(db) = opt(database) else {
                return Err(match dialect {
                    Dialect::Sqlite => "SQLite: 파일 경로(또는 :memory:)가 필요합니다".into(),
                    _ => "ODBC: DSN이 필요합니다".into(),
                });
            };
            spec.database = Some(db);
        } else {
            let Some(h) = opt(host) else {
                return Err("호스트가 필요합니다".into());
            };
            spec.host = Some(h);
            spec.port = port.or_else(|| dialect.default_port());
            spec.database = opt(database);
        }
        Ok(spec)
    }

    /// 표시용(비밀번호 가림).
    pub fn redacted(&self) -> String {
        let mut s = String::new();
        if let Some(d) = self.dialect {
            s.push_str(&format!("{d}://"));
        }
        if let Some(u) = &self.user {
            s.push_str(u);
            if self.password.is_some() {
                s.push_str("/***");
            }
        }
        if let Some(h) = &self.host {
            s.push('@');
            s.push_str(h);
            if let Some(p) = self.port {
                s.push_str(&format!(":{p}"));
            }
            if let Some(db) = &self.database {
                s.push('/');
                s.push_str(db);
            }
        }
        if self.host.is_none() {
            if let Some(db) = &self.database {
                s.push_str(db);
            }
        }
        if let Some(r) = &self.role {
            s.push_str(" AS ");
            s.push_str(r);
        }
        if let Some(sc) = &self.schema {
            s.push_str("?schema=");
            s.push_str(sc);
        }
        s
    }

    /// **접속 문자열**(우리 형식 · `dialect://user@host:port/db?schema=x` · 비밀번호 없음) — 접속 창 열 · 복사(사용자 09-18).
    /// `redacted()`와 달리 `/***`를 넣지 않아 그대로 `CONNECT`에 쓸 수 있다(비밀번호는 저장소가 채운다).
    pub fn connection_string(&self) -> String {
        let mut s = String::new();
        if let Some(d) = self.dialect {
            s.push_str(&format!("{d}://"));
        }
        match (&self.host, &self.database) {
            (Some(h), db) => {
                if let Some(u) = &self.user {
                    s.push_str(u);
                    s.push('@');
                }
                s.push_str(h);
                if let Some(p) = self.port {
                    s.push_str(&format!(":{p}"));
                }
                if let Some(db) = db {
                    s.push('/');
                    s.push_str(db);
                }
            }
            (None, Some(db)) => s.push_str(db),
            (None, None) => {
                if let Some(u) = &self.user {
                    s.push_str(u);
                }
            }
        }
        let mut q: Vec<String> = Vec::new();
        if let Some(r) = &self.role {
            q.push(format!("role={r}"));
        }
        if let Some(sc) = &self.schema {
            q.push(format!("schema={sc}"));
        }
        if let Some(e) = self.env {
            q.push(format!("env={}", e.as_str()));
        }
        if !q.is_empty() {
            s.push('?');
            s.push_str(&q.join("&"));
        }
        s
    }
}

/// 양끝의 같은 따옴표 한 쌍(`"…"` · `'…'`)을 벗긴다 — `CONNECT "…"`(docs/52 §4).
fn strip_quotes(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// `%XX` → 바이트(UTF-8). 잘못된 시퀀스는 그대로 둔다.
pub fn percent_decode(v: &str) -> String {
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(&v[i + 1..i + 3], 16) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ezconnect_form() {
        let c = ConnectSpec::parse("user/pass@192.168.1.1:1521/db").unwrap();
        assert_eq!(c.user.as_deref(), Some("user"));
        assert_eq!(c.password.as_deref(), Some("pass"));
        assert_eq!(c.host.as_deref(), Some("192.168.1.1"));
        assert_eq!(c.port, Some(1521));
        assert_eq!(c.database.as_deref(), Some("db"));
        assert_eq!(c.dialect, None);
        assert_eq!(c.redacted(), "user/***@192.168.1.1:1521/db");
    }

    #[test]
    fn tns_alias_and_sysdba() {
        let c = ConnectSpec::parse("sys/x@PRODDB as sysdba").unwrap();
        assert_eq!(c.host.as_deref(), Some("PRODDB"));
        assert_eq!(c.port, None);
        assert_eq!(c.role.as_deref(), Some("SYSDBA"));
        let c = ConnectSpec::parse("/ as sysdba").unwrap();
        assert!(c.user.is_none() && c.host.is_none());
        assert_eq!(c.role.as_deref(), Some("SYSDBA"));
    }

    #[test]
    fn scheme_and_query_dialect() {
        let c = ConnectSpec::parse("mssql://sa:p@ss@db.local:1433/master").unwrap();
        assert_eq!(c.dialect, Some(Dialect::Mssql));
        assert_eq!(c.user.as_deref(), Some("sa"));
        assert_eq!(c.password.as_deref(), Some("p@ss"));
        assert_eq!(c.host.as_deref(), Some("db.local"));
        let c = ConnectSpec::parse("mssql://sa:Nexa%40Sql2026@h:1433/master").unwrap();
        assert_eq!(c.password.as_deref(), Some("Nexa@Sql2026"));
        let c = ConnectSpec::parse("u/p%40x@h:1521/svc").unwrap();
        assert_eq!(
            c.password.as_deref(),
            Some("p%40x"),
            "SQL*Plus 형식은 디코드하지 않는다"
        );
        let c = ConnectSpec::parse("u/p@h:5432/d?dialect=pg").unwrap();
        assert_eq!(c.dialect, Some(Dialect::Postgres));
        assert!(ConnectSpec::parse("foo://u@h").is_err());
        assert!(ConnectSpec::parse("").is_err());
    }

    /// docs/52 §4: 따옴표 · 짧은 스킴(대상만/자격 포함) · sqlite 축약 · 프로필 이름(=user만).
    #[test]
    fn quoted_short_scheme_sqlite_and_profile_name() {
        let c = ConnectSpec::parse("\"oracle:192.0.0.1:1521/DB\"").unwrap();
        assert_eq!(c.dialect, Some(Dialect::Oracle));
        assert_eq!(c.user, None);
        assert_eq!(c.host.as_deref(), Some("192.0.0.1"));
        assert_eq!(c.port, Some(1521));
        assert_eq!(c.database.as_deref(), Some("DB"));
        let c = ConnectSpec::parse("'oracle:scott/tiger@h:1521/svc'").unwrap();
        assert_eq!(c.dialect, Some(Dialect::Oracle));
        assert_eq!(c.user.as_deref(), Some("scott"));
        assert_eq!(c.password.as_deref(), Some("tiger"));
        assert_eq!(c.host.as_deref(), Some("h"));
        // URL 형식도 자격 없이 대상만 줄 수 있다.
        let c = ConnectSpec::parse("mssql://db.local:1433/master").unwrap();
        assert_eq!(c.user, None);
        assert_eq!(c.host.as_deref(), Some("db.local"));
        assert_eq!(c.database.as_deref(), Some("master"));
        // 모호한 모양은 종전 해석 유지: user:pass@host.
        let c = ConnectSpec::parse("postgres:pw@h").unwrap();
        assert_eq!(c.dialect, None);
        assert_eq!(c.user.as_deref(), Some("postgres"));
        assert_eq!(c.password.as_deref(), Some("pw"));
        // sqlite 축약 — 나머지 전부가 경로.
        let c = ConnectSpec::parse("sqlite:C:\\data\\a.db").unwrap();
        assert_eq!(c.dialect, Some(Dialect::Sqlite));
        assert_eq!(c.database.as_deref(), Some("C:\\data\\a.db"));
        let c = ConnectSpec::parse("\"sqlite://:memory:\"").unwrap();
        assert_eq!(c.database.as_deref(), Some(":memory:"));
        let c = ConnectSpec::parse("sqlite:///Users/me/a.db").unwrap();
        assert_eq!(c.database.as_deref(), Some("/Users/me/a.db"));
        // 종전 URL 꼴은 그대로(호스트 자리 무시 · db만).
        let c = ConnectSpec::parse("sqlite://x/:memory:").unwrap();
        assert_eq!(c.database.as_deref(), Some(":memory:"));
        // 프로필 이름 = user만 있는 스펙(호스트의 해석기가 저장소에서 푼다).
        let c = ConnectSpec::parse("\"prod-db.1\"").unwrap();
        assert_eq!(c.user.as_deref(), Some("prod-db.1"));
        assert!(c.host.is_none() && c.password.is_none());
    }

    /// 09-18: `?schema=` 기본 스키마 · 접속 문자열(우리 형식) 왕복.
    #[test]
    fn schema_query_and_connection_string() {
        let c = ConnectSpec::parse("oracle://scott/tiger@h:1521/svc?schema=HR").unwrap();
        assert_eq!(c.schema.as_deref(), Some("HR"));
        assert_eq!(c.connection_string(), "oracle://scott@h:1521/svc?schema=HR");
        assert_eq!(c.redacted(), "oracle://scott/***@h:1521/svc?schema=HR");
        // 접속 유형 `?env=` — 왕복 · 별칭 · 모르는 값은 무시.
        let p = ConnectSpec::parse("pg://u@h/db?schema=s&env=production").unwrap();
        assert_eq!(p.env, Some(ConnEnv::Prod));
        assert_eq!(p.connection_string(), "postgres://u@h/db?schema=s&env=prod");
        assert_eq!(
            ConnectSpec::parse("pg://u@h/db?env=nope").unwrap().env,
            None
        );
        let again = ConnectSpec::parse(&c.connection_string()).unwrap();
        assert_eq!(again.schema.as_deref(), Some("HR"));
        assert_eq!(again.user.as_deref(), Some("scott"));
        assert!(again.password.is_none());
        let s = ConnectSpec::parse("sqlite:/tmp/a.db").unwrap();
        assert_eq!(s.connection_string(), "sqlite:///tmp/a.db");
        let r = ConnectSpec::parse("oracle://sys/x@h:1521/svc?role=sysdba").unwrap();
        assert_eq!(r.role.as_deref(), Some("SYSDBA"));
        assert_eq!(r.connection_string(), "oracle://sys@h:1521/svc?role=SYSDBA");
    }

    /// `user@host` = 비밀번호 없음(물어본다) · `user:@host` = 빈 비밀번호를 명시(묻지 않는다) — 둘을 구분한다.
    #[test]
    fn explicit_empty_password_is_not_missing() {
        let none = ConnectSpec::parse("oracle://BISCM@192.168.0.58:1521/BISCM").unwrap();
        assert_eq!(none.user.as_deref(), Some("BISCM"));
        assert!(none.password.is_none());
        let empty = ConnectSpec::parse("oracle://BISCM:@192.168.0.58:1521/BISCM").unwrap();
        assert_eq!(empty.user.as_deref(), Some("BISCM"));
        assert_eq!(empty.password.as_deref(), Some(""));
        assert_eq!(empty.host.as_deref(), Some("192.168.0.58"));
        assert_eq!(empty.database.as_deref(), Some("BISCM"));
        // sqlplus식도 같다.
        let sp = ConnectSpec::parse("scott/@orcl").unwrap();
        assert_eq!(sp.password.as_deref(), Some(""));
        // 표시에는 값도 "비어 있음"도 드러나지 않는다.
        assert!(!empty.redacted().contains(":@"));
    }

    #[test]
    fn password_prompt_when_missing() {
        let c = ConnectSpec::parse("scott@orcl").unwrap();
        assert_eq!(c.user.as_deref(), Some("scott"));
        assert!(c.password.is_none());
    }
}
