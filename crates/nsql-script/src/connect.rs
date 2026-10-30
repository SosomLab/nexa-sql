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

/// 서버 유형 용어(사용자 10-01): **정식 용어**(표시) + **3자리 약어**(칩·목록) 두 가지만 보인다 — 입력은 별칭도 받는다.
/// 저장 값(`as_str`)은 종전 그대로(`dev`·`test`·`prod`) — 프로필 파일 호환.
pub const ENV_NONE_NAME: &str = "None";
pub const ENV_NONE_ABBR: &str = "NON";

/// 명령 설명에 쓰는 값 목록(정식 · 3자리 약어 — 대소문자 무관).
pub const ENV_CHOICES: &str = "none|non · development|dev · test|tst · production|prd";

impl ConnEnv {
    pub const ALL: [ConnEnv; 3] = [ConnEnv::Dev, ConnEnv::Test, ConnEnv::Prod];

    /// 유형 이름(정식 · 3자리 약어 · 별칭 · 대소문자 무관) → 유형. "없음"은 `None` — [`ConnEnv::parse`]가 구별한다.
    #[must_use]
    pub fn from_name(s: &str) -> Option<ConnEnv> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dev" | "development" | "devel" | "local" | "sandbox" => Some(ConnEnv::Dev),
            "test" | "tst" | "testing" | "qa" | "stage" | "stg" | "staging" | "uat" => {
                Some(ConnEnv::Test)
            }
            "prod" | "prd" | "production" | "product" | "live" => Some(ConnEnv::Prod),
            _ => None,
        }
    }

    /// "없음"까지 받는 해석: `Ok(None)` = 없음(`none`·`non`·`-`·빈 값) · `Ok(Some)` = 유형 · `Err` = 모르는 값.
    pub fn parse(s: &str) -> Result<Option<ConnEnv>, String> {
        let v = s.trim();
        if v.is_empty() || v == "-" || ["none", "non"].iter().any(|n| v.eq_ignore_ascii_case(n)) {
            return Ok(None);
        }
        ConnEnv::from_name(v)
            .map(Some)
            .ok_or_else(|| format!("{v}: {ENV_CHOICES}"))
    }

    /// 저장 값(프로필 `env=` · 접속 문자열 `?env=` · 메뉴 id).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ConnEnv::Dev => "dev",
            ConnEnv::Test => "test",
            ConnEnv::Prod => "prod",
        }
    }

    /// 정식 용어(영어 · CLI 표시) — GUI는 `nsql-i18n`의 같은 뜻 문구.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ConnEnv::Dev => "Development",
            ConnEnv::Test => "Test",
            ConnEnv::Prod => "Production",
        }
    }

    /// 3자리 약어(칩 · 목록 열).
    #[must_use]
    pub fn abbr(self) -> &'static str {
        match self {
            ConnEnv::Dev => "DEV",
            ConnEnv::Test => "TST",
            ConnEnv::Prod => "PRD",
        }
    }

    /// `Option` 그대로 받는 정식 용어 · 약어(없음 = `None`·`NON`).
    #[must_use]
    pub fn name_of(e: Option<ConnEnv>) -> &'static str {
        e.map_or(ENV_NONE_NAME, ConnEnv::name)
    }

    #[must_use]
    pub fn abbr_of(e: Option<ConnEnv>) -> &'static str {
        e.map_or(ENV_NONE_ABBR, ConnEnv::abbr)
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
    /// ★ TLS 모드(`?tls=` · `ssl-mode=` 별칭 · T-388 MySQL/MariaDB · 소문자 · `-` → `_`): disabled · preferred · required ·
    /// verify_ca · verify_identity. 값의 뜻과 검증은 드라이버 몫 · 없으면 설정(`mysql.tls`)을 따른다.
    pub tls: Option<String>,
    /// ★ TLS CA 파일(`?tls_ca=` · libpq 별칭 `sslrootcert=` · T-70 · 155차): verify_ca/verify_identity가 쓰는 PEM 경로 · 없으면 설정
    ///   (`pg.tls_ca`)을 허브가 넣는다.
    pub tls_ca: Option<String>,
    /// ★ `?driver=`(T-27 잔여 · 10-24): 이 접속에 쓸 드라이버 — Oracle `thin`|`oci` · 확장 id(`oracle-thin` · `duckdb` …). 호스트(허브)만 본다 ·
    ///   ABI 코덱에는 싣지 않는다(드라이버는 자기 id를 안다 · 기존 패키지 호환). 없으면 설정(`oracle.driver`)·후보 순서.
    pub driver: Option<String>,
    /// ★ SSH 터널(T-3 · D-382 · 159차): `?ssh=user@host[:port]` — 허브가 시스템 `ssh -N -L`로 포워딩하고 드라이버는 `127.0.0.1:<로컬>`로
    ///   붙는다(드라이버·ABI는 모른다). 프로필(볼트 `ssh=`)·CLI 문자열·GUI 공용.
    pub ssh: Option<String>,
    /// `?ssh_key=` = ssh `-i` 개인 키 경로(없으면 사용자의 ssh 설정·에이전트).
    pub ssh_key: Option<String>,
    /// ★ 접속별 자동 커밋(T-77 잔여 · 10-30): `?autocommit=on|off`(`true/false`·`1/0`·`yes/no`) · 프로필 키 `autocommit=`. 러너가 접속
    ///   직후 자기 모드로 삼는다(설정 `session.autocommit`보다 우선 · 스크립트 `SET AUTOCOMMIT`은 그 뒤에도 우선). 없으면 설정.
    pub autocommit: Option<bool>,
}

/// `on/off`·`true/false`·`1/0`·`yes/no`(대소문자 무시) → 불린 · 그 밖 = `None`.
#[must_use]
pub fn parse_on_off(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// ★ ODBC 접속 문자열(T-383)의 `key=value;` 토큰 — `{…}` 안의 `;`는 구분자가 아니다 · `}}` = `}`.
fn odbc_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '{' => {
                depth += 1;
                cur.push(ch);
            }
            '}' if depth > 0 => {
                depth -= 1;
                cur.push(ch);
            }
            ';' if depth == 0 => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// `{value}` 이스케이프 풀기(ODBC 규칙 · `}}` → `}`).
fn odbc_unescape(v: &str) -> String {
    let t = v.trim();
    match t.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some(inner) => inner.replace("}}", "}"),
        None => t.to_string(),
    }
}

/// 값에 `;`·`{`·`}`가 있으면 `{…}`로(`}` → `}}`).
fn odbc_escape(v: &str) -> String {
    if v.contains([';', '{', '}']) {
        format!("{{{}}}", v.replace('}', "}}"))
    } else {
        v.to_string()
    }
}

/// ODBC 문자열에서 `UID=`/`PWD=`를 떼어 (나머지, user, password)로.
fn odbc_split_logon(s: &str) -> (String, Option<String>, Option<String>) {
    let mut rest = Vec::new();
    let (mut user, mut pw) = (None, None);
    for tok in odbc_tokens(s) {
        let up = tok.to_ascii_uppercase();
        if let Some(v) = up.strip_prefix("UID=").map(|_| &tok[4..]) {
            user = Some(odbc_unescape(v));
        } else if let Some(v) = up.strip_prefix("PWD=").map(|_| &tok[4..]) {
            pw = Some(odbc_unescape(v));
        } else {
            rest.push(tok);
        }
    }
    (rest.join(";"), user, pw)
}

/// 드라이브 문자(`C:\…` · `C:/…`) 또는 UNC(`\\서버\…`) 경로인가(T-439) — 스킴 없는 접속 문자열에서만 묻는다.
pub fn is_drive_or_unc_path(s: &str) -> bool {
    let b = s.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    drive || s.starts_with("\\\\")
}

/// 드라이브·UNC 경로에 붙일 파일 방언 스킴(확장자 기준 · 모르면 sqlite) — 경로가 아니면 `None`.
pub fn file_path_scheme(s: &str) -> Option<&'static str> {
    if !is_drive_or_unc_path(s) {
        return None;
    }
    let ext = s
        .rsplit(['\\', '/'])
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    Some(match ext.as_str() {
        "duckdb" | "ddb" => "duckdb",
        "mdb" | "accdb" => "access",
        _ => "sqlite",
    })
}

impl ConnectSpec {
    /// 서버 끝점(호스트, 포트): 네트워크 방언은 host/port · ODBC는 접속 문자열에서([`odbc_endpoint`]) · 파일 방언·DSN만 = None.
    #[must_use]
    pub fn endpoint(&self) -> Option<(String, u16)> {
        if let (Some(h), Some(p)) = (&self.host, self.port) {
            return Some((h.clone(), p));
        }
        if self.dialect == Some(Dialect::Odbc) {
            return self.database.as_deref().and_then(odbc_endpoint);
        }
        None
    }

    pub fn parse(arg: &str) -> Result<ConnectSpec, String> {
        let mut s = strip_quotes(arg.trim()).trim().to_string();
        if s.is_empty() {
            return Err(tr!(
                "CONNECT: connection string is missing",
                "CONNECT: 접속 문자열이 없습니다"
            ));
        }
        let mut spec = ConnectSpec::default();
        let mut url_form = false;
        // 스킴이 있었는가(`://` · 짧은 `방언:`) — `@`가 없으면 나머지는 logon이 아니라 **대상**이다.
        let mut has_scheme = false;
        // ★ ODBC 계열 벤더 스킴(T-384 · 사용자 10-23 "tibero 등"): `tibero://u:p@h:8629/db` → ODBC 접속 문자열(드라이버 이름 프리셋).
        let mut odbc_preset: Option<(&'static str, u16)> = None;
        // ★ PostgreSQL 와이어 호환 스킴(10-24 · `redshift://` 등) = 방언 Postgres + 기본 포트만 다르다.
        let mut wire_port: Option<u16> = None;

        // 파일 방언 축약: sqlite:경로(나머지 전부가 경로 — `:`·`/`·`\\`를 해석하지 않는다).
        // ★ ODBC(T-383 · 136차)도 같은 꼴: `odbc:DSN=…` · `odbc:Driver={…};Server=…;Uid=u;Pwd=p` = 나머지 전부가 ODBC 접속 문자열 ·
        //   `UID=`/`PWD=`는 스펙의 user/password로 **올려** 문자열에서 뺀다(금고 저장 · `redacted` 마스킹 · 드라이버가 다시 붙인다).
        // ★ 드라이브 문자·UNC 경로(T-439 · 10-27 V1 관찰 · 150차): `C:\data\x.db`가 `user:pass` 꼴로 읽혀 프로필에 `user=C`가 들어갔다 →
        //   파일 방언 축약형(`sqlite:…` 등)으로 되감아 아래 분기가 처리하게 한다(확장자 = .duckdb/.ddb → duckdb · .mdb/.accdb → access · 그 밖 = sqlite).
        //   호출자가 기본 방언을 파일형으로 아는 경우(CLI `-d duckdb`)는 nsql-drivers `parse_target`이 먼저 그 스킴을 붙인다.
        if let Some(scheme) = file_path_scheme(&s) {
            return Self::parse(&format!("{scheme}:{s}"));
        }
        if let Some(i) = s.find(':') {
            let d = Dialect::from_name(&s[..i]);
            // 벤더 스킴 URL(`tibero://…` · T-384)은 아래 URL 경로가 ODBC 프리셋으로 바꾼다 — 축약 분기에서 제외.
            let vendor_url = odbc_preset_for(&s[..i]).is_some() && s[i + 1..].starts_with("//");
            if matches!(
                d,
                Some(Dialect::Sqlite | Dialect::Odbc | Dialect::Duckdb | Dialect::Access)
            ) && !vendor_url
            {
                let rest = &s[i + 1..];
                // 종전 URL 꼴 `sqlite://host/db`(호스트는 무시되고 db만 쓰인다)는 그대로 아래 URL 경로로 — 그 외는 전부 경로.
                let legacy_url = matches!(d, Some(Dialect::Sqlite | Dialect::Duckdb))
                    && rest
                        .strip_prefix("//")
                        .is_some_and(|r| !r.starts_with(['/', ':', '.', '~']) && r.contains('/'));
                if !legacy_url {
                    let mut db = rest.strip_prefix("//").unwrap_or(rest).trim();
                    // ★ 축약형의 꼬리 `?driver=…`·`?env=…`(10-24): 알려진 키뿐일 때만 떼어 해석(경로에 `?`가 드물다 · Windows는 금지 문자).
                    if let Some((path, q)) = db.rsplit_once('?') {
                        let known = q.split('&').all(|kv| {
                            kv.split_once('=').is_some_and(|(k, _)| {
                                matches!(
                                    k.to_ascii_lowercase().as_str(),
                                    "driver" | "env" | "autocommit"
                                )
                            })
                        });
                        if known && !q.is_empty() {
                            for kv in q.split('&') {
                                if let Some((k, v)) = kv.split_once('=') {
                                    if k.eq_ignore_ascii_case("driver") && !v.trim().is_empty() {
                                        spec.driver = Some(v.trim().to_ascii_lowercase());
                                    } else if k.eq_ignore_ascii_case("env") {
                                        spec.env = ConnEnv::from_name(v);
                                    } else if k.eq_ignore_ascii_case("autocommit") {
                                        spec.autocommit = parse_on_off(v);
                                    }
                                }
                            }
                            db = path.trim();
                        }
                    }
                    spec.dialect = d;
                    if d == Some(Dialect::Odbc) {
                        if db.is_empty() {
                            return Err(tr!("ODBC: a DSN is required", "ODBC: DSN이 필요합니다"));
                        }
                        let (body, user, pw) = odbc_split_logon(db);
                        spec.database = Some(body);
                        spec.user = user;
                        spec.password = pw;
                        return Ok(spec);
                    }
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
            odbc_preset = odbc_preset_for(&scheme);
            wire_port = wire_preset_port(&scheme);
            if spec.dialect.is_none() {
                return Err(tr!(
                    "CONNECT: unknown scheme '{scheme}'",
                    "CONNECT: 알 수 없는 스킴 '{scheme}'"
                ));
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
                            return Err(tr!(
                                "CONNECT: unknown dialect '{v}'",
                                "CONNECT: 알 수 없는 방언 '{v}'"
                            ));
                        }
                    } else if k.eq_ignore_ascii_case("schema") && !v.trim().is_empty() {
                        spec.schema = Some(percent_decode(v.trim()));
                    } else if k.eq_ignore_ascii_case("env") {
                        spec.env = ConnEnv::from_name(v);
                    } else if k.eq_ignore_ascii_case("autocommit") {
                        // T-77(10-30): 모르는 값은 무시(= 설정을 따른다).
                        spec.autocommit = parse_on_off(v);
                    } else if k.eq_ignore_ascii_case("tls")
                        || k.eq_ignore_ascii_case("ssl-mode")
                        || k.eq_ignore_ascii_case("sslmode")
                    {
                        let t = v.trim().to_ascii_lowercase().replace('-', "_");
                        if !t.is_empty() {
                            spec.tls = Some(t);
                        }
                    } else if (k.eq_ignore_ascii_case("tls_ca")
                        || k.eq_ignore_ascii_case("tls-ca")
                        || k.eq_ignore_ascii_case("sslrootcert"))
                        && !v.trim().is_empty()
                    {
                        spec.tls_ca = Some(percent_decode(v.trim()));
                    } else if (k.eq_ignore_ascii_case("ssh")
                        || k.eq_ignore_ascii_case("ssh_host")
                        || k.eq_ignore_ascii_case("ssh-host"))
                        && !v.trim().is_empty()
                    {
                        spec.ssh = Some(percent_decode(v.trim()));
                    } else if (k.eq_ignore_ascii_case("ssh_key")
                        || k.eq_ignore_ascii_case("ssh-key")
                        || k.eq_ignore_ascii_case("ssh_identity"))
                        && !v.trim().is_empty()
                    {
                        spec.ssh_key = Some(percent_decode(v.trim()));
                    } else if k.eq_ignore_ascii_case("role") && !v.trim().is_empty() {
                        spec.role = Some(v.trim().to_ascii_uppercase());
                    } else if k.eq_ignore_ascii_case("driver") && !v.trim().is_empty() {
                        spec.driver = Some(v.trim().to_ascii_lowercase());
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
                    let p = hostport[i + 1..].parse::<u16>().map_err(|_| {
                        tr!(
                            "CONNECT: bad port '{}'",
                            "CONNECT: 포트 '{}'",
                            &hostport[i + 1..]
                        )
                    })?;
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
            return Err(tr!(
                "CONNECT: a user or a target is required",
                "CONNECT: 사용자 또는 대상이 필요합니다"
            ));
        }
        if spec.port.is_none() && spec.host.is_some() {
            spec.port = wire_port;
        }
        if let (Some((driver, default_port)), Some(Dialect::Odbc)) = (odbc_preset, spec.dialect) {
            if let Some(host) = spec.host.take() {
                let port = spec.port.take().unwrap_or(default_port);
                let db = spec.database.take().unwrap_or_default();
                spec.database = Some(format!(
                    "Driver={{{driver}}};Server={host};Port={port};Database={db}"
                ));
            }
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
                    Dialect::Sqlite => tr!(
                        "SQLite: a file path (or :memory:) is required",
                        "SQLite: 파일 경로(또는 :memory:)가 필요합니다"
                    ),
                    Dialect::Duckdb => tr!(
                        "DuckDB: a file path (or :memory:) is required",
                        "DuckDB: 파일 경로(또는 :memory:)가 필요합니다"
                    ),
                    _ => tr!("ODBC: a DSN is required", "ODBC: DSN이 필요합니다"),
                });
            };
            spec.database = Some(db);
        } else {
            let Some(h) = opt(host) else {
                return Err(tr!("a host is required", "호스트가 필요합니다"));
            };
            spec.host = Some(h);
            spec.port = port.or_else(|| dialect.default_port());
            spec.database = opt(database);
        }
        Ok(spec)
    }

    /// ★ 빠진 필수 로그인 항목(T-133 잔여 · 10-30 · 원장 = [`Dialect::login_required`] = 접속 폼과 같은 표). CLI `-c` 인자 검사용.
    /// `with_password = false`면 Password는 세지 않는다(CLI는 env/프롬프트가 뒤에 채운다 · D-115 별개). Oracle 예외 = `user@TNS`
    /// (포트·서비스 없이 별칭이 host 칸) · `/ as sysdba`(OS 인증 · 사용자·호스트 없음) → 드라이버 판단에 맡긴다(빈 목록).
    #[must_use]
    pub fn missing_login(
        &self,
        dialect: Dialect,
        with_password: bool,
    ) -> Vec<nsql_core::LoginField> {
        use nsql_core::LoginField as F;
        let empty = |v: &Option<String>| v.as_deref().is_none_or(|s| s.trim().is_empty());
        if dialect == Dialect::Oracle {
            let tns = !empty(&self.host) && self.port.is_none() && empty(&self.database);
            let os_auth = empty(&self.user) && empty(&self.host) && self.role.is_some();
            if tns || os_auth {
                return Vec::new();
            }
        }
        dialect
            .login_required()
            .iter()
            .copied()
            .filter(|f| match f {
                F::Host => empty(&self.host),
                F::Database => empty(&self.database),
                F::User => empty(&self.user),
                F::Password => with_password && empty(&self.password),
                F::Port | F::Schema => false,
            })
            .collect()
    }

    /// 표시용(비밀번호 가림).
    pub fn redacted(&self) -> String {
        // ★ ODBC = 문자열 그대로 + `;UID=u;PWD=***`(비밀번호는 parse 때 스펙으로 올라와 본문에 없다).
        if self.dialect == Some(Dialect::Odbc) {
            let mut s = self.connection_string();
            if self.password.is_some() {
                s.push_str(";PWD=***");
            }
            return s;
        }
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
        if let Some(t) = &self.ssh {
            s.push_str(" via ssh ");
            s.push_str(t);
        }
        s
    }

    /// **접속 문자열**(우리 형식 · `dialect://user@host:port/db?schema=x` · 비밀번호 없음) — 접속 창 열 · 복사(사용자 09-18).
    /// `redacted()`와 달리 `/***`를 넣지 않아 그대로 `CONNECT`에 쓸 수 있다(비밀번호는 저장소가 채운다).
    pub fn connection_string(&self) -> String {
        // ★ ODBC = 짧은 스킴 그대로(`odbc:…;UID=u` · 비밀번호는 다른 방언처럼 뺀다 · 다시 parse하면 같은 스펙).
        if self.dialect == Some(Dialect::Odbc) {
            let mut s = format!("odbc:{}", self.database.as_deref().unwrap_or(""));
            if let Some(u) = &self.user {
                s.push_str(&format!(";UID={}", odbc_escape(u)));
            }
            return s;
        }
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
        if let Some(t) = &self.tls {
            q.push(format!("tls={t}"));
        }
        if let Some(c) = &self.tls_ca {
            q.push(format!("tls_ca={c}"));
        }
        if let Some(d) = &self.driver {
            q.push(format!("driver={d}"));
        }
        if let Some(t) = &self.ssh {
            q.push(format!("ssh={t}"));
        }
        if let Some(k) = &self.ssh_key {
            q.push(format!("ssh_key={k}"));
        }
        if let Some(a) = self.autocommit {
            q.push(format!("autocommit={}", if a { "on" } else { "off" }));
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

/// ★ ODBC 접속 문자열에서 서버 끝점(호스트, 포트)을 읽는다(순수 · 사용자 10-23 "ODBC는 서버 상태 점이 회색"): `Server=`·`Host=`·`Hostname=`·
/// `Address=`·`Addr=`(SQL Server식 `host,port` · `tcp:host,port` · `host:port`) + 따로 `Port=` · 포트가 없으면 `Driver=` 이름으로 기본 포트
/// (SQL Server 1433 · PostgreSQL 5432 · MySQL/MariaDB 3306 · Tibero 8629 · Altibase 20300 · CUBRID 33000 · Oracle 1521 · Db2 50000).
/// DSN만 있거나 호스트를 모르면 None(TNS 별칭 같은 것은 호스트가 아니다).
#[must_use]
pub fn odbc_endpoint(conn: &str) -> Option<(String, u16)> {
    let mut host: Option<String> = None;
    let mut port: Option<u16> = None;
    let mut driver = String::new();
    for part in conn.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().trim_matches(|c| c == '{' || c == '}').trim();
        match k.as_str() {
            "driver" => driver = v.to_ascii_lowercase(),
            "port" => port = v.parse().ok(),
            "server" | "host" | "hostname" | "address" | "addr" if host.is_none() => {
                let v = v
                    .strip_prefix("tcp:")
                    .or_else(|| v.strip_prefix("TCP:"))
                    .unwrap_or(v);
                // `host,port`(SQL Server) · `host:port`(IPv6 대괄호는 제외)
                if let Some((h, p)) = v.split_once(',').or_else(|| {
                    (!v.starts_with('[') && v.matches(':').count() == 1)
                        .then(|| v.split_once(':'))
                        .flatten()
                }) {
                    host = Some(h.trim().to_string());
                    if port.is_none() {
                        port = p.trim().parse().ok();
                    }
                } else {
                    host = Some(v.to_string());
                }
            }
            _ => {}
        }
    }
    let host = host.filter(|h| !h.is_empty())?;
    let port = port.or_else(|| {
        [
            ("sql server", 1433u16),
            ("postgres", 5432),
            ("mysql", 3306),
            ("mariadb", 3306),
            ("tibero", 8629),
            ("altibase", 20300),
            ("cubrid", 33000),
            ("oracle", 1521),
            ("db2", 50000),
            ("hdbodbc", 30015),
            ("informix", 9088),
        ]
        .iter()
        .find(|(k, _)| driver.contains(k))
        .map(|(_, p)| *p)
    })?;
    Some((host, port))
}

/// ODBC 계열 벤더 스킴 → (드라이버 이름, 기본 포트). 벤더 ODBC 드라이버가 설치돼 있어야 한다(docs/22 §4 · 121).
fn odbc_preset_for(scheme: &str) -> Option<(&'static str, u16)> {
    match scheme.to_ascii_lowercase().as_str() {
        "tibero" => Some(("Tibero 7 ODBC Driver", 8629)),
        "altibase" => Some(("Altibase ODBC Driver", 20300)),
        "cubrid" => Some(("CUBRID Driver", 33000)),
        // 10-24 추가(121 P5 · ODBC로 커버되는 레거시·대기업 DBMS): 드라이버 이름 = 벤더 배포본의 odbcinst 등록 이름.
        "hana" => Some(("HDBODBC", 30015)),
        "informix" => Some(("IBM INFORMIX ODBC DRIVER", 9088)),
        _ => None,
    }
}

/// ★ PostgreSQL 와이어 호환 스킴 → 기본 포트(10-24 · 121 P5): Redshift 5439 · CockroachDB 26257 · YugabyteDB 5433 · Greenplum·TimescaleDB 5432.
///   방언은 [`Dialect::from_name`]이 Postgres로 돌린다 — 드라이버·문법·카탈로그 전부 PostgreSQL 것(차이는 서버가 알려 준다).
fn wire_preset_port(scheme: &str) -> Option<u16> {
    match scheme.to_ascii_lowercase().as_str() {
        "redshift" => Some(5439),
        "cockroach" | "cockroachdb" => Some(26257),
        "yugabyte" | "yugabytedb" => Some(5433),
        "greenplum" | "timescale" | "timescaledb" => Some(5432),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10-24(121 P5): PG 와이어 스킴 = Postgres + 기본 포트 · 새 ODBC 벤더 스킴 3 · ClickHouse URL.
    #[test]
    fn wire_odbc_and_clickhouse_presets() {
        let r = ConnectSpec::parse("redshift://u:p@h/dev").expect("redshift");
        assert_eq!(
            (r.dialect, r.port, r.host.as_deref()),
            (Some(Dialect::Postgres), Some(5439), Some("h"))
        );
        let c = ConnectSpec::parse("cockroachdb://u:p@h:9999/db").expect("cockroach");
        assert_eq!(c.port, Some(9999), "명시한 포트가 이긴다");
        assert_eq!(
            ConnectSpec::parse("yugabyte://u:p@h/db").unwrap().port,
            Some(5433)
        );
        assert_eq!(
            ConnectSpec::parse("timescaledb://u:p@h/db").unwrap().port,
            Some(5432)
        );
        for (url, drv, port) in [
            ("hana://u:p@h/HXE", "HDBODBC", 30015),
            ("informix://u:p@h/stores", "IBM INFORMIX ODBC DRIVER", 9088),
        ] {
            let s = ConnectSpec::parse(url).expect(url);
            assert_eq!(s.dialect, Some(Dialect::Odbc));
            let db = s.database.unwrap();
            assert!(
                db.starts_with(&format!("Driver={{{drv}}};Server=h;Port={port};")),
                "{db}"
            );
            assert_eq!(s.user.as_deref(), Some("u"));
        }
        let ch = ConnectSpec::parse("clickhouse://u:p@h/db?tls=required").expect("clickhouse");
        assert_eq!(
            (ch.dialect, ch.port, ch.tls.as_deref()),
            (Some(Dialect::Clickhouse), None, Some("required"))
        );
        assert_eq!(ch.database.as_deref(), Some("db"));
        let ch2 = ConnectSpec::parse("clickhouse://h:8124").expect("host only");
        assert_eq!((ch2.port, ch2.user.is_none()), (Some(8124), true));
        // Db2(D-359) = 전용 방언 · 호스트/포트/DB 필드 그대로(ODBC 문자열은 드라이버가 조립) · Access(D-360) = 파일 축약.
        let d = ConnectSpec::parse("db2://db2inst1:pw@h/SAMPLE").expect("db2");
        assert_eq!(
            (d.dialect, d.host.as_deref(), d.port, d.database.as_deref()),
            (Some(Dialect::Db2), Some("h"), None, Some("SAMPLE"))
        );
        let a = ConnectSpec::parse("access:/tmp/dir/north wind.accdb").expect("access");
        assert_eq!(
            (a.dialect, a.database.as_deref()),
            (Some(Dialect::Access), Some("/tmp/dir/north wind.accdb"))
        );
        let a2 = ConnectSpec::parse("access:C:\\data\\x.mdb").expect("access win");
        assert_eq!(a2.database.as_deref(), Some("C:\\data\\x.mdb"));
    }

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

    /// 서버 유형 용어(사용자 10-01): 정식·3자리 약어·별칭(대소문자 무관) → 유형 · 표시는 정식/약어 두 가지.
    #[test]
    fn conn_env_terms_and_aliases() {
        for (inputs, want) in [
            (&["None", "none", "non", "NON", "-", " "][..], None),
            (
                &["Development", "development", "Dev", "dev", "DEV"][..],
                Some(ConnEnv::Dev),
            ),
            (
                &[
                    "Test", "test", "TST", "Stage", "stage", "Stg", "stg", "staging", "qa",
                ][..],
                Some(ConnEnv::Test),
            ),
            (
                &[
                    "Production",
                    "Product",
                    "product",
                    "Prod",
                    "prod",
                    "Prd",
                    "prd",
                    "live",
                ][..],
                Some(ConnEnv::Prod),
            ),
        ] {
            for s in inputs {
                assert_eq!(ConnEnv::parse(s), Ok(want), "{s}");
            }
        }
        assert!(ConnEnv::parse("bogus").is_err());
        assert_eq!(ConnEnv::from_name("none"), None);
        // 표시 = 정식 · 3자리 약어 · 저장 값은 종전 그대로.
        let shown: Vec<_> = ConnEnv::ALL
            .iter()
            .map(|e| (e.name(), e.abbr(), e.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Development", "DEV", "dev"),
                ("Test", "TST", "test"),
                ("Production", "PRD", "prod")
            ]
        );
        assert_eq!(
            (ConnEnv::name_of(None), ConnEnv::abbr_of(None)),
            ("None", "NON")
        );
        // 저장 값·약어는 다시 읽어도 같은 유형(왕복).
        for e in ConnEnv::ALL {
            assert_eq!(ConnEnv::from_name(e.as_str()), Some(e));
            assert_eq!(ConnEnv::from_name(e.abbr()), Some(e));
            assert_eq!(ConnEnv::from_name(e.name()), Some(e));
        }
    }

    /// 09-18: `?schema=` 기본 스키마 · 접속 문자열(우리 형식) 왕복.
    #[test]
    fn schema_query_and_connection_string() {
        let c = ConnectSpec::parse("oracle://scott/tiger@h:1521/svc?schema=HR").unwrap();
        assert_eq!(c.schema.as_deref(), Some("HR"));
        assert_eq!(c.connection_string(), "oracle://scott@h:1521/svc?schema=HR");
        assert_eq!(c.redacted(), "oracle://scott/***@h:1521/svc?schema=HR");
        // ★ 접속별 자동 커밋 `?autocommit=`(T-77 · 10-30) — 왕복 · 별칭 · 모르는 값 = 없음 · 축약형 꼬리에서도.
        let p = ConnectSpec::parse("pg://u@h/db?autocommit=off").unwrap();
        assert_eq!(p.autocommit, Some(false));
        assert_eq!(p.connection_string(), "postgres://u@h/db?autocommit=off");
        assert_eq!(
            ConnectSpec::parse("pg://u@h/db?autocommit=YES")
                .unwrap()
                .autocommit,
            Some(true)
        );
        assert_eq!(
            ConnectSpec::parse("pg://u@h/db?autocommit=maybe")
                .unwrap()
                .autocommit,
            None
        );
        assert_eq!(
            ConnectSpec::parse("sqlite:/tmp/a.db?autocommit=0")
                .unwrap()
                .autocommit,
            Some(false)
        );
        assert_eq!(parse_on_off(" On "), Some(true));
        assert_eq!(parse_on_off("x"), None);
        // ★ T-133 잔여(10-30): 빠진 필수 항목 = 방언 표 그대로 · 비밀번호는 요청할 때만 · Oracle TNS/OS 인증 예외.
        use nsql_core::LoginField as F;
        let p = ConnectSpec::parse("pg://h").unwrap();
        assert_eq!(p.missing_login(Dialect::Postgres, false), vec![F::User]);
        assert_eq!(
            p.missing_login(Dialect::Postgres, true),
            vec![F::User, F::Password]
        );
        let p = ConnectSpec {
            user: Some("scott".into()),
            host: Some("h".into()),
            port: Some(1521),
            ..Default::default()
        };
        assert_eq!(p.missing_login(Dialect::Oracle, false), vec![F::Database]);
        let p = ConnectSpec::parse("scott/x@PRODDB").unwrap();
        assert!(
            p.missing_login(Dialect::Oracle, true).is_empty(),
            "TNS 별칭"
        );
        let p = ConnectSpec::parse("/ as sysdba").unwrap();
        assert!(p.missing_login(Dialect::Oracle, true).is_empty(), "OS 인증");
        let p = ConnectSpec::default();
        assert_eq!(p.missing_login(Dialect::Sqlite, false), vec![F::Database]);
        let p = ConnectSpec::parse("clickhouse://h").unwrap();
        assert!(p.missing_login(Dialect::Clickhouse, true).is_empty());
        // 접속 유형 `?env=` — 왕복 · 별칭 · 모르는 값은 무시.
        let p = ConnectSpec::parse("pg://u@h/db?schema=s&env=production").unwrap();
        assert_eq!(p.env, Some(ConnEnv::Prod));
        assert_eq!(p.connection_string(), "postgres://u@h/db?schema=s&env=prod");
        assert_eq!(
            ConnectSpec::parse("pg://u@h/db?env=nope").unwrap().env,
            None
        );
        // ★ ODBC 짧은 스킴(T-383): 나머지 전부 · UID/PWD는 스펙으로(`{…}` 이스케이프 풀어서) · 왕복 = 비밀번호 뺀 문자열 · redacted 마스킹.
        let o =
            ConnectSpec::parse("odbc:Driver={SQL Server};Server=h,1433;Database=d;Uid=u;Pwd={p;w}")
                .unwrap();
        assert_eq!(o.dialect, Some(Dialect::Odbc));
        assert_eq!(
            o.database.as_deref(),
            Some("Driver={SQL Server};Server=h,1433;Database=d")
        );
        assert_eq!(o.user.as_deref(), Some("u"));
        assert_eq!(o.password.as_deref(), Some("p;w"));
        assert_eq!(
            o.connection_string(),
            "odbc:Driver={SQL Server};Server=h,1433;Database=d;UID=u"
        );
        assert_eq!(
            o.redacted(),
            "odbc:Driver={SQL Server};Server=h,1433;Database=d;UID=u;PWD=***"
        );
        let dsn = ConnectSpec::parse("odbc:Sales").unwrap();
        assert_eq!(dsn.database.as_deref(), Some("Sales"));
        assert!(dsn.user.is_none() && dsn.password.is_none());
        assert_eq!(
            ConnectSpec::parse(&o.connection_string()).unwrap().database,
            o.database
        );
        assert!(ConnectSpec::parse("odbc:").is_err(), "빈 DSN = 오류");
        // ★ `?tls=`(T-388): 소문자 정규화 · `ssl-mode=` 별칭 · 왕복.
        let t = ConnectSpec::parse("mysql://u@h/db?tls=REQUIRED").unwrap();
        assert_eq!(t.tls.as_deref(), Some("required"));
        assert_eq!(t.connection_string(), "mysql://u@h/db?tls=required");
        // ★ T-3(159차): SSH 터널 = `ssh=` · `ssh_key=` 왕복 · 가림 표시 " via ssh …" · 별칭 ssh_host/ssh-key.
        let sh = ConnectSpec::parse("postgres://u@h:5432/db?ssh=me@bastion:2222&ssh_key=/k/id")
            .expect("ssh spec");
        assert_eq!(sh.ssh.as_deref(), Some("me@bastion:2222"));
        assert_eq!(sh.ssh_key.as_deref(), Some("/k/id"));
        let cs = sh.connection_string();
        assert!(
            cs.contains("ssh=me@bastion:2222") && cs.contains("ssh_key=/k/id"),
            "{cs}"
        );
        let back = ConnectSpec::parse(&cs).expect("round trip");
        assert_eq!(
            (back.ssh, back.ssh_key),
            (sh.ssh.clone(), sh.ssh_key.clone())
        );
        assert!(
            sh.redacted().ends_with(" via ssh me@bastion:2222"),
            "{}",
            sh.redacted()
        );
        let alias = ConnectSpec::parse("mysql://u@h/db?ssh-host=b&ssh-key=k").expect("alias");
        assert_eq!(
            (alias.ssh.as_deref(), alias.ssh_key.as_deref()),
            (Some("b"), Some("k"))
        );
        // ★ T-70(155차): CA 파일 = `tls_ca` · libpq 별칭 `sslrootcert` · 왕복.
        let pgc =
            ConnectSpec::parse("postgres://u@h/db?sslmode=verify-full&sslrootcert=/etc/ca.pem")
                .unwrap();
        assert_eq!(pgc.tls.as_deref(), Some("verify_full"));
        assert_eq!(pgc.tls_ca.as_deref(), Some("/etc/ca.pem"));
        assert_eq!(
            pgc.connection_string(),
            "postgres://u@h/db?tls=verify_full&tls_ca=/etc/ca.pem"
        );
        assert_eq!(
            ConnectSpec::parse("mariadb://u@h/db?ssl-mode=Verify-Identity")
                .unwrap()
                .tls
                .as_deref(),
            Some("verify_identity")
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
        let none = ConnectSpec::parse("oracle://APPDB@172.16.1.58:1521/APPDB").unwrap();
        assert_eq!(none.user.as_deref(), Some("APPDB"));
        assert!(none.password.is_none());
        let empty = ConnectSpec::parse("oracle://APPDB:@172.16.1.58:1521/APPDB").unwrap();
        assert_eq!(empty.user.as_deref(), Some("APPDB"));
        assert_eq!(empty.password.as_deref(), Some(""));
        assert_eq!(empty.host.as_deref(), Some("172.16.1.58"));
        assert_eq!(empty.database.as_deref(), Some("APPDB"));
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
    /// ★ T-384 Tibero 등 ODBC 계열 벤더 스킴 = ODBC 접속 문자열 프리셋(드라이버 이름 · 기본 포트) · 자격은 스펙으로 · 왕복.
    #[test]
    fn vendor_schemes_become_odbc_presets() {
        let t = ConnectSpec::parse("tibero://scott:tiger@db.example:8629/TIBERO").expect("tibero");
        assert_eq!(t.dialect, Some(Dialect::Odbc));
        assert_eq!(
            t.database.as_deref(),
            Some("Driver={Tibero 7 ODBC Driver};Server=db.example;Port=8629;Database=TIBERO")
        );
        assert_eq!(
            (t.user.as_deref(), t.password.as_deref()),
            (Some("scott"), Some("tiger"))
        );
        assert_eq!((t.host.as_deref(), t.port), (None, None));
        let back = ConnectSpec::parse(&t.connection_string()).expect("round trip");
        assert_eq!(back.database, t.database);
        assert_eq!(back.user, t.user);
        assert!(!t.redacted().contains("tiger"));
        let a = ConnectSpec::parse("altibase://u:p@h/mydb").expect("altibase");
        assert!(
            a.database.as_deref().unwrap().contains("Port=20300"),
            "{a:?}"
        );
        let c = ConnectSpec::parse("cubrid://u:p@h/demodb").expect("cubrid");
        assert!(c
            .database
            .as_deref()
            .unwrap()
            .starts_with("Driver={CUBRID Driver}"));
        // 평범한 odbc: 스킴은 그대로.
        let o = ConnectSpec::parse("odbc:DSN=Sales").expect("odbc");
        assert_eq!(o.database.as_deref(), Some("DSN=Sales"));
    }
    /// ★ `?driver=`(10-24): 소문자 · 왕복 · 없으면 None.
    #[test]
    fn driver_param_round_trips() {
        let o = ConnectSpec::parse("oracle://scott:tiger@db:1521/orcl?driver=OCI").unwrap();
        assert_eq!(o.driver.as_deref(), Some("oci"));
        assert_eq!(
            o.connection_string(),
            "oracle://scott@db:1521/orcl?driver=oci"
        );
        let d = ConnectSpec::parse("duckdb:/tmp/x.duckdb?driver=duckdb").unwrap();
        assert_eq!(d.driver.as_deref(), Some("duckdb"));
        assert!(ConnectSpec::parse("mysql://u@h/db")
            .unwrap()
            .driver
            .is_none());
    }

    /// ★ ODBC 접속 문자열 → 끝점(사용자 10-23 로그인 목록 서버 상태 점).
    /// ★ T-439(150차): 드라이브 문자·UNC 경로는 파일 방언으로(user/pass로 읽지 않는다) · 확장자로 duckdb/access · 스킴이 있으면 종전대로.
    #[test]
    fn drive_and_unc_paths_are_file_dialects() {
        let s = ConnectSpec::parse("C:\\data\\x.db").expect("parse");
        assert_eq!(s.dialect, Some(Dialect::Sqlite));
        assert_eq!(s.database.as_deref(), Some("C:\\data\\x.db"));
        assert!(s.user.is_none() && s.password.is_none());
        let s = ConnectSpec::parse("D:/w/a.duckdb").expect("parse");
        assert_eq!(s.dialect, Some(Dialect::Duckdb));
        let s = ConnectSpec::parse("\\\\srv\\share\\a.accdb").expect("parse");
        assert_eq!(s.dialect, Some(Dialect::Access));
        assert_eq!(s.database.as_deref(), Some("\\\\srv\\share\\a.accdb"));
        assert_eq!(file_path_scheme("scott/tiger@C:\\x"), None);
        assert_eq!(file_path_scheme("c:"), None);
    }

    #[test]
    fn odbc_endpoint_from_connection_string() {
        assert_eq!(
            odbc_endpoint(
                "Driver={SQL Server};Server=172.16.1.58,1433;Database=PLANDB;Uid=u;Pwd=p"
            ),
            Some(("172.16.1.58".into(), 1433))
        );
        assert_eq!(
            odbc_endpoint("Driver={SQL Server};Server=tcp:db.example,1444"),
            Some(("db.example".into(), 1444))
        );
        assert_eq!(
            odbc_endpoint("Driver={ODBC Driver 18 for SQL Server};Server=db;Database=x"),
            Some(("db".into(), 1433))
        );
        assert_eq!(
            odbc_endpoint("Driver={PostgreSQL Unicode};Server=pg;Port=5433;Database=d"),
            Some(("pg".into(), 5433))
        );
        assert_eq!(
            odbc_endpoint("Driver={Tibero 7 ODBC Driver};Server=t;Port=8629;Database=tb"),
            Some(("t".into(), 8629))
        );
        assert_eq!(
            odbc_endpoint("Driver={MySQL ODBC 9.0 Unicode Driver};Server=m;Database=d"),
            Some(("m".into(), 3306))
        );
        assert_eq!(odbc_endpoint("DSN=Sales;Uid=u"), None, "DSN만 = 모름");
        assert_eq!(
            odbc_endpoint("Driver={Unknown};Server=h"),
            None,
            "포트 모름"
        );
        let o = ConnectSpec::parse("odbc:Driver={SQL Server};Server=h,1433").expect("odbc");
        assert_eq!(o.endpoint(), Some(("h".into(), 1433)));
        let t = ConnectSpec::parse("tibero://u:p@th:8629/db").expect("tibero");
        assert_eq!(
            t.endpoint(),
            Some(("th".into(), 8629)),
            "프리셋은 ODBC 문자열로 바뀌어도 끝점이 나온다"
        );
        let pg = ConnectSpec::parse("postgres://u:p@h:5432/d").expect("pg");
        assert_eq!(pg.endpoint(), Some(("h".into(), 5432)));
        assert_eq!(
            ConnectSpec::parse("sqlite:a.db")
                .expect("sqlite")
                .endpoint(),
            None
        );
    }
}
