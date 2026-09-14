//! `nsql-vault` — 연결 프로필 저장소(T-16b · DR-22). **CLI `nsql conn`과 GUI가 같은 폴더를 본다.**
//!
//! ```text
//! <사용자 설정 폴더>/nexa-sql/           nexa_conf::user_config_dir("nexa-sql")
//! ├─ device.key                         기기 키(Windows DPAPI 봉투 · 그 외 0600 평문) — devkey.rs
//! └─ profiles/<이름>.conf               프로필 1개 = 파일 1개(nexa-conf key=value)
//!      dialect=oracle · user=scott · host=… · port=1521 · database=orcl · role=SYSDBA
//!      secret=<hex>                     비밀번호 봉투(ChaCha20-Poly1305 · 도메인 "profile-v1/<이름>")
//! ```
//!
//! - 비밀번호 **외** 필드는 평문이다 — 호스트·사용자명은 비밀이 아니며 `nsql conn list`가 그대로
//!   보여 줘야 한다. 비밀번호만 봉투에 넣는다.
//! - 봉투 도메인에 프로필 이름이 들어가므로 `dev.conf`의 `secret=`을 `prod.conf`에 옮겨 붙여도
//!   열리지 않는다(fail-closed).
//! - 프로필 이름 = `[A-Za-z0-9_.-]{1,64}` — 접속 문자열과 겹치지 않는다(`:`·`@`·`/` 없음). 그래서
//!   `-c prod`처럼 **이름을 접속 문자열 자리에 그대로** 쓸 수 있다([`is_profile_name`]).

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod devkey;
mod sealed;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use nsql_core::Dialect;
use nsql_script::ConnectSpec;

/// 앱 이름 — 설정 폴더 하위 이름(`%APPDATA%\nexa-sql` · `~/.config/nexa-sql` · `~/Library/Application Support/nexa-sql`).
pub const APP_DIR: &str = nsql_settings::APP_DIR;
const PROFILES_DIR: &str = "profiles";
const DEVICE_KEY: &str = "device.key";
const EXT: &str = "conf";

/// 프로필 이름 규칙 — 접속 문자열에 반드시 들어가는 문자(`:` `@` `/`)를 배제한다.
#[must_use]
pub fn is_profile_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && !s.starts_with('.')
}

/// 저장된 프로필 요약(비밀번호 없음 — 목록·표시용).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    /// 비밀번호를 뺀 스펙.
    pub spec: ConnectSpec,
    /// 비밀번호 봉투가 있는가.
    pub has_password: bool,
}

impl Profile {
    /// `oracle://scott/***@host:1521/orcl` 꼴.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut s = self.spec.clone();
        if self.has_password {
            s.password = Some(String::new());
        }
        s.redacted()
    }
}

/// 프로필 저장소 — 열면 기기 키를 메모리에 든다.
pub struct Vault {
    dir: PathBuf,
    key: [u8; 32],
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault")
            .field("dir", &self.dir)
            .field("key", &"[redacted]")
            .finish()
    }
}

impl Vault {
    /// 기본 폴더 — `user_config_dir("nexa-sql")`. 환경변수 `NSQL_HOME`이 있으면 그것(테스트·포터블).
    #[must_use]
    pub fn default_dir() -> Option<PathBuf> {
        nsql_settings::config_dir()
    }

    /// 기본 폴더로 연다.
    pub fn open_default() -> io::Result<Vault> {
        let dir = Self::default_dir().ok_or_else(|| {
            io::Error::other(
                "사용자 설정 폴더를 알 수 없습니다(APPDATA/HOME 없음) — NSQL_HOME을 지정하세요",
            )
        })?;
        Self::open(dir)
    }

    /// 지정 폴더로 연다(없으면 만든다 · 기기 키가 없으면 생성).
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Vault> {
        let dir = dir.into();
        std::fs::create_dir_all(dir.join(PROFILES_DIR))?;
        let key = devkey::load_or_create(&dir.join(DEVICE_KEY))?;
        Ok(Vault { dir, key })
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path_of(&self, name: &str) -> io::Result<PathBuf> {
        if !is_profile_name(name) {
            return Err(io::Error::other(format!(
                "프로필 이름 '{name}': 영문·숫자·`_ - .`만, 64자 이내"
            )));
        }
        Ok(self.dir.join(PROFILES_DIR).join(format!("{name}.{EXT}")))
    }

    fn domain(name: &str) -> Vec<u8> {
        format!("profile-v1/{name}").into_bytes()
    }

    /// 저장 — 비밀번호는 봉투에, 나머지는 평문 key=value. 같은 이름은 덮어쓴다(원자적).
    pub fn save(&self, name: &str, spec: &ConnectSpec) -> io::Result<()> {
        let path = self.path_of(name)?;
        let mut known: Vec<(&str, String)> = Vec::new();
        if let Some(d) = spec.dialect {
            known.push(("dialect", d.to_string()));
        }
        if let Some(v) = &spec.user {
            known.push(("user", v.clone()));
        }
        if let Some(v) = &spec.host {
            known.push(("host", v.clone()));
        }
        if let Some(p) = spec.port {
            known.push(("port", p.to_string()));
        }
        if let Some(v) = &spec.database {
            known.push(("database", v.clone()));
        }
        if let Some(v) = &spec.role {
            known.push(("role", v.clone()));
        }
        if let Some(pw) = &spec.password {
            let env = sealed::seal(&Self::domain(name), &self.key, pw.as_bytes())?;
            known.push(("secret", hex_encode(&env)));
        }
        let pairs: Vec<(&str, &str)> = known.iter().map(|(k, v)| (*k, v.as_str())).collect();
        nexa_conf::write_atomic(&path, &nexa_conf::serialize(&pairs, &[]))
    }

    /// 읽기 — 비밀번호를 풀어 완전한 스펙을 돌려준다. 없으면 `Ok(None)`.
    /// 봉투가 열리지 않으면(다른 기기 키·손상) 오류 — 평문인 척 통과하지 않는다.
    pub fn get(&self, name: &str) -> io::Result<Option<ConnectSpec>> {
        let path = self.path_of(name)?;
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let (mut spec, secret) = Self::parse_doc(&text);
        if let Some(hex) = secret {
            let env = hex_decode(&hex)
                .ok_or_else(|| io::Error::other(format!("{name}: secret 손상(hex)")))?;
            let pw = sealed::open(&Self::domain(name), &self.key, &env).ok_or_else(|| {
                io::Error::other(format!(
                    "{name}: 비밀번호 봉투를 열 수 없습니다(다른 기기 키 또는 손상) — `nsql conn add {name} …`로 다시 저장하세요"
                ))
            })?;
            spec.password = Some(String::from_utf8_lossy(&pw).into_owned());
        }
        Ok(Some(spec))
    }

    /// 비밀번호 없이 요약만(목록용 · 봉투를 열지 않는다).
    pub fn peek(&self, name: &str) -> io::Result<Option<Profile>> {
        let path = self.path_of(name)?;
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let (spec, secret) = Self::parse_doc(&text);
        Ok(Some(Profile {
            name: name.to_string(),
            spec,
            has_password: secret.is_some(),
        }))
    }

    /// 삭제 — 있었으면 true.
    pub fn remove(&self, name: &str) -> io::Result<bool> {
        let path = self.path_of(name)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// 이름순 목록.
    pub fn list(&self) -> io::Result<Vec<Profile>> {
        let mut names: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(self.dir.join(PROFILES_DIR))? {
            let entry = entry?;
            let file = entry.file_name();
            let Some(file) = file.to_str() else { continue };
            let Some(stem) = file.strip_suffix(&format!(".{EXT}")) else {
                continue;
            };
            if is_profile_name(stem) {
                names.push(stem.to_string());
            }
        }
        names.sort();
        let mut out = Vec::with_capacity(names.len());
        for n in names {
            if let Some(p) = self.peek(&n)? {
                out.push(p);
            }
        }
        Ok(out)
    }

    /// `-c <대상>` 해석 — 프로필 이름이면 저장소에서, 아니면 `None`(호출측이 접속 문자열로 파싱).
    /// 이름 꼴인데 프로필이 없으면 오류(오타를 접속 문자열로 오해하지 않게).
    pub fn resolve(&self, target: &str) -> io::Result<Option<ConnectSpec>> {
        let t = target.trim();
        if !is_profile_name(t) {
            return Ok(None);
        }
        match self.get(t)? {
            Some(s) => Ok(Some(s)),
            None => Err(io::Error::other(format!(
                "프로필 '{t}'이(가) 없습니다 — `nsql conn list`로 확인하거나 접속 문자열을 쓰세요"
            ))),
        }
    }

    fn parse_doc(text: &str) -> (ConnectSpec, Option<String>) {
        let doc = nexa_conf::parse(text);
        let mut spec = ConnectSpec::default();
        let mut secret = None;
        for (k, v) in doc.pairs {
            match k.as_str() {
                "dialect" => spec.dialect = Dialect::from_name(&v),
                "user" => spec.user = Some(v),
                "host" => spec.host = Some(v),
                "port" => spec.port = v.trim().parse().ok(),
                "database" => spec.database = Some(v),
                "role" => spec.role = Some(v),
                "secret" => secret = Some(v),
                _ => {}
            }
        }
        (spec, secret)
    }
}

fn hex_encode(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let b = s.as_bytes();
    for i in (0..b.len()).step_by(2) {
        let hi = (b[i] as char).to_digit(16)?;
        let lo = (b[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("nsql-vault-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn spec(pw: Option<&str>) -> ConnectSpec {
        ConnectSpec {
            user: Some("scott".into()),
            password: pw.map(str::to_string),
            host: Some("db.example.com".into()),
            port: Some(1521),
            database: Some("orcl".into()),
            role: None,
            dialect: Some(Dialect::Oracle),
        }
    }

    #[test]
    fn names() {
        for ok in ["dev", "prod-1", "a.b_c", "X"] {
            assert!(is_profile_name(ok), "{ok}");
        }
        for bad in [
            "",
            "sqlite::memory:",
            "u/p@h",
            "oracle://x",
            ".hidden",
            "한글",
            "a b",
        ] {
            assert!(!is_profile_name(bad), "{bad}");
        }
    }

    #[test]
    fn save_get_list_remove_roundtrip() {
        let d = tmp("vault");
        let v = Vault::open(&d).unwrap();
        v.save("dev", &spec(Some("Nexa@Sql/2026:x"))).unwrap();
        v.save(
            "mem",
            &nsql_script::ConnectSpec {
                dialect: Some(Dialect::Sqlite),
                database: Some(":memory:".into()),
                ..Default::default()
            },
        )
        .unwrap();

        // 디스크에 비밀번호 평문이 없다.
        let raw = std::fs::read_to_string(d.join("profiles/dev.conf")).unwrap();
        assert!(raw.contains("host=db.example.com"));
        assert!(!raw.contains("Nexa@Sql"), "{raw}");
        assert!(raw.contains("secret="));

        let got = v.get("dev").unwrap().unwrap();
        assert_eq!(got, spec(Some("Nexa@Sql/2026:x")));

        let list = v.list().unwrap();
        assert_eq!(
            list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["dev", "mem"]
        );
        assert!(list[0].has_password);
        assert_eq!(list[0].spec.password, None, "목록은 비밀번호를 풀지 않는다");
        assert_eq!(
            list[0].describe(),
            "oracle://scott/***@db.example.com:1521/orcl"
        );
        assert!(!list[1].has_password);

        assert!(v.get("nope").unwrap().is_none());
        assert!(v.remove("dev").unwrap());
        assert!(!v.remove("dev").unwrap());
        assert!(v.get("dev").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 다시 연 저장소(같은 device.key)가 같은 비밀번호를 푼다 · 다른 기기 키면 fail-closed.
    #[test]
    fn reopen_same_key_and_foreign_key_fails() {
        let d = tmp("vault-reopen");
        Vault::open(&d)
            .unwrap()
            .save("p", &spec(Some("pw")))
            .unwrap();
        let again = Vault::open(&d).unwrap();
        assert_eq!(
            again.get("p").unwrap().unwrap().password.as_deref(),
            Some("pw")
        );

        // 다른 기기의 키로 바꿔치기 → 봉투가 열리지 않아야 한다(평문 통과 금지).
        std::fs::write(d.join("device.key"), [1u8; 32]).unwrap();
        let foreign = Vault::open(&d).unwrap();
        let err = foreign.get("p").unwrap_err();
        assert!(err.to_string().contains("봉투"), "{err}");
        // 요약(peek·list)은 여전히 된다.
        assert!(foreign.peek("p").unwrap().unwrap().has_password);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 봉투를 다른 프로필 파일로 옮겨 붙이면 열리지 않는다(도메인 = 프로필 이름).
    #[test]
    fn secret_bound_to_profile_name() {
        let d = tmp("vault-domain");
        let v = Vault::open(&d).unwrap();
        v.save("dev", &spec(Some("pw"))).unwrap();
        std::fs::copy(d.join("profiles/dev.conf"), d.join("profiles/prod.conf")).unwrap();
        assert!(v.get("prod").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn resolve_name_vs_connect_string() {
        let d = tmp("vault-resolve");
        let v = Vault::open(&d).unwrap();
        v.save("dev", &spec(None)).unwrap();
        assert!(v.resolve("dev").unwrap().is_some());
        assert!(
            v.resolve("oracle://u:p@h/x").unwrap().is_none(),
            "접속 문자열은 통과"
        );
        assert!(v.resolve("sqlite::memory:").unwrap().is_none());
        assert!(v.resolve("typo").is_err(), "이름 꼴인데 없으면 오류");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hex() {
        let b = [0u8, 1, 0xab, 0xff];
        assert_eq!(hex_encode(&b), "0001abff");
        assert_eq!(hex_decode("0001abff").unwrap(), b);
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("zz").is_none());
    }
}
