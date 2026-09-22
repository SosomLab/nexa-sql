//! `nsql-vault` — 연결 프로필 저장소(T-16b · DR-22). **CLI `nsql conn`과 GUI가 같은 폴더를 본다.**
//!
//! ```text
//! <사용자 설정 폴더>/nexa-sql/           nexa_conf::user_config_dir("nexa-sql")
//! ├─ device.key                         기기 키(Windows DPAPI 봉투 · 그 외 0600 평문) — devkey.rs
//! └─ profiles/<해시>.conf               프로필 1개 = 파일 1개(nexa-conf key=value) · 해시 = SHA-256(이름) 앞 16 hex
//!      name=biscm                       ★ 프로필 이름(파일 이름이 아니라 **이 값**이 이름 · 대/소문자 구별)
//!      dialect=oracle · user=scott · host=… · port=1521 · database=orcl · role=SYSDBA
//!      secret=<hex>                     비밀번호 봉투(ChaCha20-Poly1305 · 도메인 "profile-v1/<이름>")
//! ```
//!
//! ★ **파일 이름 = 이름의 해시**(사용자 09-22): 종전 `profiles/<이름>.conf`는 Windows·macOS의 대/소문자 무시 파일 시스템에서
//! `biscm`→`BISCM` 이름 변경이 **같은 파일을 덮어쓴 뒤 옛 이름을 지워** 프로필과 파일이 함께 사라졌다. 해시 파일 이름은 OS와 무관하게
//! 이름을 대/소문자까지 구별하고, 이름은 파일 안 `name=`에 산다. 종전 꼴(`<이름>.conf` · `name=` 없음)은 **읽기 호환** — 목록·읽기에서
//! 그대로 보이고, 그 이름으로 다시 저장하면 해시 파일로 옮기며 옛 파일은 **파일 이름이 글자 그대로 같을 때만** 지운다
//! (`Path::exists`는 대/소문자를 무시하므로 폴더 목록의 실제 이름과 비교한다 · [`Vault::legacy_path`]).
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
pub mod session;

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
    /// 기본 폴더 — `user_config_dir("nexa-sql")`. 환경변수 `NSQL_HOME`이 있으면 그것(테스트·개발용 재지정).
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

    fn check_name(name: &str) -> io::Result<()> {
        if !is_profile_name(name) {
            return Err(io::Error::other(format!(
                "프로필 이름 '{name}': 영문·숫자·`_ - .`만, 64자 이내"
            )));
        }
        Ok(())
    }

    /// 프로필 파일 이름(`<해시>.conf`) — SHA-256(이름)의 앞 16 hex. 이름의 대/소문자가 다르면 다른 파일.
    #[must_use]
    pub fn file_name(name: &str) -> String {
        use sha2::{Digest, Sha256};
        let h = Sha256::digest(name.as_bytes());
        format!("{}.{EXT}", hex_encode(&h[..8]))
    }

    /// 프로필 파일의 실제(또는 저장될) 경로 — 기기 키 없이 폴더만으로. 해시 파일이 있으면 그것 · 종전 꼴이 글자 그대로 있으면 그것 ·
    /// 둘 다 없으면 저장될 해시 경로(GUI 상세 보기의 파일 이름 줄 · 우클릭 "설정 파일 경로 복사" · CLI `conn show`).
    #[must_use]
    pub fn locate_in(dir: &Path, name: &str) -> PathBuf {
        let profiles = dir.join(PROFILES_DIR);
        let hashed = profiles.join(Self::file_name(name));
        if hashed.is_file() {
            return hashed;
        }
        let want = format!("{name}.{EXT}");
        if let Ok(rd) = std::fs::read_dir(&profiles) {
            if let Some(e) = rd
                .flatten()
                .find(|e| e.file_name().to_str() == Some(want.as_str()))
            {
                return e.path();
            }
        }
        hashed
    }

    /// [`Self::locate_in`]을 기본 폴더로(폴더를 모르면 `None`).
    #[must_use]
    pub fn locate_default(name: &str) -> Option<PathBuf> {
        Self::default_dir().map(|d| Self::locate_in(&d, name))
    }

    /// 이 이름의 프로필이 저장되는(될) 경로.
    pub fn path_of(&self, name: &str) -> io::Result<PathBuf> {
        Self::check_name(name)?;
        Ok(self.dir.join(PROFILES_DIR).join(Self::file_name(name)))
    }

    /// 종전 꼴 `<이름>.conf`가 **글자 그대로 그 이름으로** 있으면 그 경로. 폴더 목록의 실제 파일 이름과 바이트 비교한다 —
    /// `Path::exists`/`read_to_string`은 대/소문자 무시 파일 시스템에서 `BISCM.conf`로 `biscm.conf`를 열어 버린다(09-22 결함의 뿌리).
    fn legacy_path(&self, name: &str) -> Option<PathBuf> {
        let want = format!("{name}.{EXT}");
        let rd = std::fs::read_dir(self.dir.join(PROFILES_DIR)).ok()?;
        rd.flatten()
            .find(|e| e.file_name().to_str() == Some(want.as_str()))
            .map(|e| e.path())
    }

    #[cfg(test)]
    fn locate(&self, name: &str) -> PathBuf {
        Self::locate_in(&self.dir, name)
    }

    /// 읽을 파일 — 해시 파일이 있으면 그것, 없으면 종전 꼴(글자 그대로 같은 이름만).
    fn read_path(&self, name: &str) -> io::Result<Option<PathBuf>> {
        let p = self.path_of(name)?;
        if p.is_file() {
            return Ok(Some(p));
        }
        Ok(self.legacy_path(name))
    }

    fn domain(name: &str) -> Vec<u8> {
        format!("profile-v1/{name}").into_bytes()
    }

    /// 저장 — 비밀번호는 봉투에, 나머지는 평문 key=value. 같은 이름은 덮어쓴다(원자적). 종전 꼴 파일이 **글자 그대로 같은 이름**으로
    /// 있었으면 해시 파일로 옮기고 그것만 지운다(대/소문자만 다른 파일은 다른 프로필 — 건드리지 않는다).
    pub fn save(&self, name: &str, spec: &ConnectSpec) -> io::Result<()> {
        let path = self.path_of(name)?;
        // 해시 충돌(다른 이름이 같은 파일로) — 사실상 없지만 덮어쓰지 않고 거절한다.
        if let Ok(t) = std::fs::read_to_string(&path) {
            if let Some(other) = Self::parse_doc(&t).2.filter(|n| n != name) {
                return Err(io::Error::other(format!(
                    "프로필 '{name}'의 파일 이름이 '{other}'와 겹칩니다 — 다른 이름을 쓰세요"
                )));
            }
        }
        let mut known: Vec<(&str, String)> = vec![("name", name.to_string())];
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
        if let Some(v) = &spec.schema {
            known.push(("schema", v.clone()));
        }
        if let Some(e) = spec.env {
            known.push(("env", e.as_str().to_string()));
        }
        if let Some(pw) = &spec.password {
            let env = sealed::seal(&Self::domain(name), &self.key, pw.as_bytes())?;
            known.push(("secret", hex_encode(&env)));
        }
        let pairs: Vec<(&str, &str)> = known.iter().map(|(k, v)| (*k, v.as_str())).collect();
        nexa_conf::write_atomic(&path, &nexa_conf::serialize(&pairs, &[]))?;
        // 종전 꼴 → 해시 파일로 이관(글자 그대로 같은 이름만 · 자기 자신이면 그대로).
        if let Some(old) = self.legacy_path(name) {
            if old != path {
                let _ = std::fs::remove_file(old);
            }
        }
        Ok(())
    }

    /// 읽기 — 비밀번호를 풀어 완전한 스펙을 돌려준다. 없으면 `Ok(None)`.
    /// 봉투가 열리지 않으면(다른 기기 키·손상) 오류 — 평문인 척 통과하지 않는다.
    pub fn get(&self, name: &str) -> io::Result<Option<ConnectSpec>> {
        let Some(path) = self.read_path(name)? else {
            return Ok(None);
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let (mut spec, secret, _) = Self::parse_doc(&text);
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
        let Some(path) = self.read_path(name)? else {
            return Ok(None);
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let (spec, secret, _) = Self::parse_doc(&text);
        Ok(Some(Profile {
            name: name.to_string(),
            spec,
            has_password: secret.is_some(),
        }))
    }

    /// 삭제 — 있었으면 true. 해시 파일과 종전 꼴(글자 그대로 같은 이름) 둘 다 본다.
    pub fn remove(&self, name: &str) -> io::Result<bool> {
        let path = self.path_of(name)?;
        let mut removed = match std::fs::remove_file(&path) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        if let Some(old) = self.legacy_path(name) {
            if old != path {
                std::fs::remove_file(old)?;
                removed = true;
            }
        }
        Ok(removed)
    }

    /// 이름순 목록 — 파일 안 `name=`이 이름(해시 파일) · 없으면 파일 이름의 어간(종전 꼴 · 글자 그대로).
    pub fn list(&self) -> io::Result<Vec<Profile>> {
        let mut out: Vec<Profile> = Vec::new();
        for entry in std::fs::read_dir(self.dir.join(PROFILES_DIR))? {
            let entry = entry?;
            let file = entry.file_name();
            let Some(file) = file.to_str() else { continue };
            let Some(stem) = file.strip_suffix(&format!(".{EXT}")) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let (spec, secret, name) = Self::parse_doc(&text);
            let name = match name {
                Some(n) if is_profile_name(&n) => n,
                Some(_) => continue,
                None if is_profile_name(stem) => stem.to_string(),
                None => continue,
            };
            // ★ 불변식(사용자 09-22): **프로필 수 == 파일 수**. 같은 이름이 해시 파일과 종전 꼴 둘로 있으면(이관 중 중단) 해시 파일을
            //   남기고 종전 파일은 여기서 치운다 — 목록이 곧 복구다.
            if let Some(i) = out.iter().position(|p| p.name == name) {
                if file == Self::file_name(&name) {
                    let _ = std::fs::remove_file(
                        self.dir.join(PROFILES_DIR).join(format!("{name}.{EXT}")),
                    );
                    out.remove(i);
                } else {
                    let _ = std::fs::remove_file(entry.path());
                    continue;
                }
            }
            out.push(Profile {
                name,
                spec,
                has_password: secret.is_some(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// 폴더의 `.conf` 파일 수 — 불변식 점검용(프로필 수와 같아야 한다).
    pub fn file_count(&self) -> io::Result<usize> {
        Ok(std::fs::read_dir(self.dir.join(PROFILES_DIR))?
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some(EXT))
            .count())
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

    /// (스펙 · 봉투 hex · `name=`).
    fn parse_doc(text: &str) -> (ConnectSpec, Option<String>, Option<String>) {
        let doc = nexa_conf::parse(text);
        let mut spec = ConnectSpec::default();
        let mut secret = None;
        let mut name = None;
        for (k, v) in doc.pairs {
            match k.as_str() {
                "name" => name = Some(v),
                "dialect" => spec.dialect = Dialect::from_name(&v),
                "user" => spec.user = Some(v),
                "host" => spec.host = Some(v),
                "port" => spec.port = v.trim().parse().ok(),
                "database" => spec.database = Some(v),
                "role" => spec.role = Some(v),
                "schema" => spec.schema = Some(v),
                "env" => spec.env = nsql_script::ConnEnv::from_name(&v),
                "secret" => secret = Some(v),
                _ => {}
            }
        }
        (spec, secret, name)
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
    if !s.len().is_multiple_of(2) {
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
            schema: None,
            env: None,
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
        let raw = std::fs::read_to_string(v.path_of("dev").unwrap()).unwrap();
        assert!(raw.contains("name=dev"));
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
        // 종전 꼴로 옮겨 붙임(`name=`은 dev지만 파일 이름이 prod — 이름 꼴 파일은 어간이 이름).
        let mut t = std::fs::read_to_string(v.path_of("dev").unwrap()).unwrap();
        t = t.replace("name=dev", "");
        std::fs::write(d.join("profiles/prod.conf"), t).unwrap();
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

    /// ★ 09-22 결함 재현: 대/소문자만 다른 이름으로 저장 뒤 옛 이름 삭제(GUI의 이름 변경 순서) — 종전 `<이름>.conf`는 Windows·macOS에서
    /// `BISCM.conf`가 `biscm.conf`를 덮어쓴 뒤 삭제돼 둘 다 사라졌다. 해시 파일 이름은 OS와 무관하게 둘을 구별한다.
    #[test]
    fn case_only_rename_keeps_new_profile() {
        let d = tmp("vault-case");
        let v = Vault::open(&d).unwrap();
        v.save("biscm", &spec(Some("pw"))).unwrap();
        v.save("BISCM", &spec(Some("pw2"))).unwrap();
        assert_ne!(Vault::file_name("biscm"), Vault::file_name("BISCM"));
        assert!(v.remove("biscm").unwrap());
        let got = v.get("BISCM").unwrap().expect("새 이름은 남는다");
        assert_eq!(got.password.as_deref(), Some("pw2"));
        assert!(v.get("biscm").unwrap().is_none());
        assert_eq!(
            v.list()
                .unwrap()
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["BISCM"]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 종전 꼴(`<이름>.conf` · `name=` 없음) 읽기 호환 + 저장 시 이관 · 대/소문자만 다른 종전 파일은 건드리지 않는다.
    #[test]
    fn legacy_files_are_read_and_migrated_exact_case_only() {
        let d = tmp("vault-legacy");
        let v = Vault::open(&d).unwrap();
        let legacy = "dialect=oracle\nuser=scott\nhost=h\nport=1521\ndatabase=orcl\n";
        std::fs::write(d.join("profiles/biscm.conf"), legacy).unwrap();
        assert_eq!(v.list().unwrap()[0].name, "biscm");
        assert_eq!(
            v.get("biscm").unwrap().unwrap().user.as_deref(),
            Some("scott")
        );
        // 대/소문자만 다른 새 이름으로 저장 — 종전 `biscm.conf`는 남아야 한다(대/소문자 무시 FS에서도 글자 비교).
        v.save("BISCM", &spec(None)).unwrap();
        let names: Vec<String> = v.list().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["BISCM", "biscm"]);
        assert!(d.join("profiles/biscm.conf").is_file());
        // 같은 이름으로 다시 저장 = 해시 파일로 이관 · 종전 파일 삭제.
        v.save("biscm", &spec(None)).unwrap();
        assert!(v.path_of("biscm").unwrap().is_file());
        assert!(v.legacy_path("biscm").is_none());
        let names: Vec<String> = v.list().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["BISCM", "biscm"]);
        // 경로 찾기(키 없이): 해시 파일 → 종전 꼴(글자 그대로) → 저장될 해시 경로.
        assert_eq!(v.locate("biscm"), v.path_of("biscm").unwrap());
        std::fs::write(d.join("profiles/Legacy.conf"), legacy).unwrap();
        assert_eq!(v.locate("Legacy"), d.join("profiles/Legacy.conf"));
        assert_eq!(
            v.locate("legacy"),
            v.path_of("legacy").unwrap(),
            "대/소문자가 다르면 종전 파일이 아니다"
        );
        assert_eq!(v.locate("nothing"), v.path_of("nothing").unwrap());
        // 종전 꼴 삭제도 된다.
        std::fs::write(d.join("profiles/old.conf"), legacy).unwrap();
        assert!(v.remove("old").unwrap());
        assert!(!v.remove("old").unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ 불변식(사용자 09-22 "프로필 개수와 남은 파일 개수가 항상 일치"): 저장 · 덧쓰기 · 이름 변경(GUI 순서 = 새 이름 저장 → 옛 이름 삭제 ·
    /// 대/소문자만 바뀌는 경우 포함) · 복사 · 삭제 · 종전 꼴 이관 — **모든 연산 뒤** `list().len() == file_count()`.
    #[test]
    fn profile_count_always_equals_file_count() {
        let d = tmp("vault-invariant");
        let v = Vault::open(&d).unwrap();
        let check = |step: &str| {
            let n = v.list().unwrap().len();
            let f = v.file_count().unwrap();
            assert_eq!(n, f, "{step}: 프로필 {n} ≠ 파일 {f}");
        };
        check("empty");
        v.save("a", &spec(None)).unwrap();
        check("save a");
        v.save("a", &spec(Some("pw"))).unwrap();
        check("overwrite a");
        v.save("b", &spec(None)).unwrap();
        check("save b");
        // 이름 변경(GUI 순서) — 일반 · 대/소문자만.
        v.save("c", &spec(None)).unwrap();
        v.remove("b").unwrap();
        check("rename b→c");
        v.save("C", &spec(None)).unwrap();
        v.remove("c").unwrap();
        check("rename c→C (case only)");
        // 복사(GUI `dup`) · 삭제.
        v.save("C_Copied", &v.get("C").unwrap().unwrap()).unwrap();
        check("dup");
        v.remove("C_Copied").unwrap();
        check("remove dup");
        // 종전 꼴 파일 둘(그중 하나는 대/소문자만 다른 이름) → 목록에 보이고 · 이관 뒤에도 수가 맞다.
        let legacy = "dialect=oracle\nuser=u\nhost=h\nport=1521\ndatabase=x\n";
        std::fs::write(d.join("profiles/old.conf"), legacy).unwrap();
        std::fs::write(d.join("profiles/Old2.conf"), legacy).unwrap();
        check("legacy present");
        v.save("old", &spec(None)).unwrap();
        check("migrate old");
        v.save("OLD", &spec(None)).unwrap();
        check("save OLD next to legacy old(migrated)");
        // 이관 중 중단을 흉내: 해시 파일과 종전 파일이 같은 이름으로 공존 → 목록이 복구한다.
        std::fs::write(d.join("profiles/old.conf"), legacy).unwrap();
        assert_eq!(
            v.list().unwrap().len(),
            v.file_count().unwrap(),
            "목록이 종전 중복을 치운다"
        );
        check("after repair");
        assert!(v.remove("Old2").unwrap());
        check("remove legacy");
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
