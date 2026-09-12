//! `CONNECT` 인자 해석 — SQL*Plus logon 문법 + 방언 힌트(docs/04 §8.3 · docs/08 §5).
//!
//! 받아들이는 모양:
//! - `user/pass@host:1521/svc` · `user/pass@tns` · `user@host` (비밀번호 프롬프트)
//! - `/ as sysdba` · `user/pass@host:1521/svc as sysdba`
//! - `mssql://user:pass@host:1433/db` (스킴 = 방언) · `user/pass@host:1433/db?dialect=mssql`

use nsql_core::Dialect;

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
}

impl ConnectSpec {
    pub fn parse(arg: &str) -> Result<ConnectSpec, String> {
        let mut s = arg.trim().to_string();
        if s.is_empty() {
            return Err("CONNECT: 접속 문자열이 없습니다".into());
        }
        let mut spec = ConnectSpec::default();
        let mut url_form = false;

        // 방언 스킴: mssql://…
        if let Some(i) = s.find("://") {
            url_form = true;
            let scheme = s[..i].to_string();
            spec.dialect = Dialect::from_name(&scheme);
            if spec.dialect.is_none() {
                return Err(format!("CONNECT: 알 수 없는 스킴 '{scheme}'"));
            }
            s = s[i + 3..].to_string();
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
        // logon@target
        let (logon, target) = match s.rfind('@') {
            Some(i) => (&s[..i], Some(&s[i + 1..])),
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
        spec.password = pass.filter(|p| !p.is_empty()).map(decode);
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
        let c = ConnectSpec::parse("mssql://sa:Nexa%40Sql2026@h:1433/master").unwrap();
        assert_eq!(c.password.as_deref(), Some("Nexa@Sql2026"));
        let c = ConnectSpec::parse("u/p%40x@h:1521/svc").unwrap();
        assert_eq!(
            c.password.as_deref(),
            Some("p%40x"),
            "SQL*Plus 형식은 디코드하지 않는다"
        );
        assert_eq!(c.host.as_deref(), Some("db.local"));
        let c = ConnectSpec::parse("u/p@h:5432/d?dialect=pg").unwrap();
        assert_eq!(c.dialect, Some(Dialect::Postgres));
        assert!(ConnectSpec::parse("foo://u@h").is_err());
        assert!(ConnectSpec::parse("").is_err());
    }

    #[test]
    fn password_prompt_when_missing() {
        let c = ConnectSpec::parse("scott@orcl").unwrap();
        assert_eq!(c.user.as_deref(), Some("scott"));
        assert!(c.password.is_none());
    }
}
