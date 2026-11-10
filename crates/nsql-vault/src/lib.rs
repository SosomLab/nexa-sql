//! `nsql-vault` — 연결 프로필 저장소(T-16b · DR-22). **CLI `nsql conn`과 GUI가 같은 폴더를 본다.**
//!
//! ```text
//! <사용자 설정 폴더>/nexa-sql/           nexa_conf::user_config_dir("nexa-sql")
//! ├─ device.key                         기기 키(Windows DPAPI 봉투 · 그 외 0600 평문) — devkey.rs
//! └─ profiles/<해시>.conf               프로필 1개 = 파일 1개(nexa-conf key=value) · 해시 = SHA-256(이름) 앞 16 hex
//!      name=appdb                       ★ 프로필 이름(파일 이름이 아니라 **이 값**이 이름 · 대/소문자 구별)
//!      dialect=oracle · user=scott · host=… · port=1521 · database=orcl · role=SYSDBA
//!      secret=<hex>                     비밀번호 봉투(ChaCha20-Poly1305 · 도메인 "profile-v1/<이름>")
//! ```
//!
//! ★ **파일 이름 = 이름의 해시**(사용자 09-22): 종전 `profiles/<이름>.conf`는 Windows·macOS의 대/소문자 무시 파일 시스템에서
//! `appdb`→`APPDB` 이름 변경이 **같은 파일을 덮어쓴 뒤 옛 이름을 지워** 프로필과 파일이 함께 사라졌다. 해시 파일 이름은 OS와 무관하게
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

/// OS 난수 `n`바이트(PKCE 검증자 · OAuth state 같은 1회용 값 · nexa-sql AI WIF 로그인 11-11).
pub fn random_bytes(n: usize) -> io::Result<Vec<u8>> {
    let mut v = vec![0u8; n];
    getrandom::getrandom(&mut v).map_err(|_| io::Error::other("OS 난수 실패"))?;
    Ok(v)
}
pub mod session;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use nsql_core::Dialect;
use nsql_script::ConnectSpec;

/// 앱 이름 — 설정 폴더 하위 이름(`%APPDATA%\nexa-sql` · `~/.config/nexa-sql` · `~/Library/Application Support/nexa-sql`).
pub const APP_DIR: &str = nsql_settings::APP_DIR;
const PROFILES_DIR: &str = "profiles";
/// ★ 임의 이름 비밀(API 키 · 토큰 · 11-11) 폴더.
const SECRETS_DIR: &str = "secrets";
const DEVICE_KEY: &str = "device.key";
const EXT: &str = "conf";
/// ★ 지운 프로필 보관 폴더(T-31b · 11-04): `profiles/.deleted/<해시>.<유닉스 초>[-n].conf` — `list()`는 `.deleted` 폴더를 보지 않는다(하위 폴더).
const DELETED_DIR: &str = ".deleted";
/// 내보내기 파일 첫 줄(형식 표식 · 비밀번호 없음을 사람도 알게).
pub const EXPORT_HEADER: &str = "# nexa-sql connection profiles v1 (passwords are never exported)";

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `.deleted/`에 보관 중인 프로필(비밀번호 없음 · 복구 후보).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedProfile {
    pub name: String,
    pub spec: ConnectSpec,
    /// 지운 시각(유닉스 초 · 파일 이름에서).
    pub deleted_at: u64,
    pub path: PathBuf,
}

/// 가져오기 결과.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportReport {
    pub added: usize,
    pub replaced: usize,
    /// 건너뛴 이름(이미 있음 · 이름 규칙 위반 · 이름 없음).
    pub skipped: Vec<String>,
}

/// 프로필 이름 규칙 — 접속 문자열에 반드시 들어가는 문자(`:` `@` `/`)를 배제한다.
/// 프로필 변수 줄의 키 접두(`var.NAME=값`).
pub const VAR_PREFIX: &str = "var.";

/// 프로필 변수 이름(바인드 변수 이름 규칙 · 식별자).
#[must_use]
pub fn is_var_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !s.as_bytes()[0].is_ascii_digit()
}

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
                "사용자 설정 폴더를 알 수 없습니다(APPDATA/HOME 없음) - NSQL_HOME을 지정하세요",
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
            // 라이브러리 오류 = 영어(호스트 CLI/GUI가 `is_profile_name`으로 먼저 검사해 Msg로 안내 · 10-23 ui.lang=en 한국어 지적).
            return Err(io::Error::other(format!(
                "profile name '{name}': letters, digits and `_ - .` only, up to 64 characters"
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
    /// `Path::exists`/`read_to_string`은 대/소문자 무시 파일 시스템에서 `APPDB.conf`로 `appdb.conf`를 열어 버린다(09-22 결함의 뿌리).
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

    /// ★ 임의 이름의 비밀(API 키 · 토큰 · docs/110 §4 · 11-11) — 프로필 비밀번호와 같은 봉투(기기 키 + ChaCha20-Poly1305) ·
    ///   `secrets/<해시>.conf` 두 줄(`name=` 평문 · `secret=` 봉투 hex). 설정·프로젝트 파일에는 절대 두지 않는다.
    pub fn put_secret(&self, name: &str, plain: &[u8]) -> io::Result<()> {
        Self::check_name(name)?;
        let dir = self.dir.join(SECRETS_DIR);
        std::fs::create_dir_all(&dir)?;
        let env = sealed::seal(&Self::secret_domain(name), &self.key, plain)?;
        let mut text = format!("name={name}");
        text.push('\n');
        text.push_str("secret=");
        text.push_str(&hex_encode(&env));
        text.push('\n');
        let path = dir.join(Self::file_name(name));
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)
    }

    /// 비밀 읽기 — 없거나 봉투를 못 열면 `None`(기기가 바뀌면 못 연다 · 프로필 비밀번호와 같다).
    #[must_use]
    pub fn get_secret(&self, name: &str) -> Option<Vec<u8>> {
        let path = self.dir.join(SECRETS_DIR).join(Self::file_name(name));
        let text = std::fs::read_to_string(path).ok()?;
        let mut stored_name = None;
        let mut secret = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("name=") {
                stored_name = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("secret=") {
                secret = hex_decode(v);
            }
        }
        if stored_name.as_deref() != Some(name) {
            return None;
        }
        sealed::open(&Self::secret_domain(name), &self.key, &secret?)
    }

    /// 비밀 지우기(없으면 그대로 성공).
    pub fn delete_secret(&self, name: &str) -> io::Result<()> {
        let path = self.dir.join(SECRETS_DIR).join(Self::file_name(name));
        match std::fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }

    /// 저장된 비밀 이름 목록(파일의 `name=` 줄 · 이름순).
    #[must_use]
    pub fn secret_names(&self) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(self.dir.join(SECRETS_DIR)) else {
            return Vec::new();
        };
        let mut v: Vec<String> = rd
            .flatten()
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("name=").map(str::to_string))
            })
            .collect();
        v.sort();
        v
    }

    fn secret_domain(name: &str) -> Vec<u8> {
        format!("secret-v1/{name}").into_bytes()
    }

    /// 저장 — 비밀번호는 봉투에, 나머지는 평문 key=value. 같은 이름은 덮어쓴다(원자적). 종전 꼴 파일이 **글자 그대로 같은 이름**으로
    /// 있었으면 해시 파일로 옮기고 그것만 지운다(대/소문자만 다른 파일은 다른 프로필 — 건드리지 않는다).
    pub fn save(&self, name: &str, spec: &ConnectSpec) -> io::Result<()> {
        let path = self.path_of(name)?;
        // 해시 충돌(다른 이름이 같은 파일로) — 사실상 없지만 덮어쓰지 않고 거절한다.
        if let Ok(t) = std::fs::read_to_string(&path) {
            if let Some(other) = Self::parse_doc(&t).2.filter(|n| n != name) {
                return Err(io::Error::other(format!(
                    "프로필 '{name}'의 파일 이름이 '{other}'와 겹칩니다 - 다른 이름을 쓰세요"
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
        // ★ SSH 터널(T-3 · 159차) — 프로필에 같이 산다(비밀 아님 · 키 **경로**만).
        if let Some(v) = &spec.ssh {
            known.push(("ssh", v.clone()));
        }
        if let Some(v) = &spec.ssh_key {
            known.push(("ssh_key", v.clone()));
        }
        // ★ 접속별 자동 커밋(T-77 · 10-30) — 프로필 키 `autocommit=on|off`.
        if let Some(a) = spec.autocommit {
            known.push(("autocommit", if a { "on" } else { "off" }.to_string()));
        }
        // ★ 읽기 전용(T-31b · D-458) — 프로필 키 `readonly=on|off`.
        if let Some(r) = spec.read_only {
            known.push(("readonly", if r { "on" } else { "off" }.to_string()));
        }
        if let Some(pw) = &spec.password {
            let env = sealed::seal(&Self::domain(name), &self.key, pw.as_bytes())?;
            known.push(("secret", hex_encode(&env)));
        }
        let pairs: Vec<(&str, &str)> = known.iter().map(|(k, v)| (*k, v.as_str())).collect();
        // ★ 프로필 변수(`var.<이름>=값` · T-153 · 사용자 10-26)는 스펙 밖 줄 — 스펙만 다시 저장해도 그대로 남긴다.
        let keep: Vec<(String, String)> = std::fs::read_to_string(&path)
            .map(|t| {
                nexa_conf::parse(&t)
                    .pairs
                    .into_iter()
                    .filter(|(k, _)| k.starts_with(VAR_PREFIX))
                    .collect()
            })
            .unwrap_or_default();
        nexa_conf::write_atomic(&path, &nexa_conf::serialize(&pairs, &keep))?;
        // 종전 꼴 → 해시 파일로 이관(글자 그대로 같은 이름만 · 자기 자신이면 그대로).
        if let Some(old) = self.legacy_path(name) {
            if old != path {
                let _ = std::fs::remove_file(old);
            }
        }
        Ok(())
    }

    /// ★ 프로필 변수(T-153 · docs/63 §11 프로필 층) — `var.<이름>=값` 줄들(이름 = 파일에 적힌 표기 · 값 = 글자 그대로). 없으면 빈 목록.
    pub fn vars(&self, name: &str) -> io::Result<Vec<(String, String)>> {
        let Some(path) = self.read_path(name)? else {
            return Ok(Vec::new());
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(nexa_conf::parse(&text)
            .pairs
            .into_iter()
            .filter_map(|(k, v)| k.strip_prefix(VAR_PREFIX).map(|n| (n.to_string(), v)))
            .collect())
    }

    /// 프로필 변수를 통째로 바꾼다(스펙·봉투 줄은 그대로 · 원자적). 프로필이 없으면 `NotFound`. 이름은 식별자(`A-Z 0-9 _` · 대문자 정규화).
    pub fn set_vars(&self, name: &str, vars: &[(String, String)]) -> io::Result<()> {
        let Some(path) = self.read_path(name)? else {
            return Err(io::Error::new(io::ErrorKind::NotFound, name.to_string()));
        };
        for (k, _) in vars {
            if !is_var_name(k) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("bad variable name: {k}"),
                ));
            }
        }
        let text = std::fs::read_to_string(&path)?;
        let doc = nexa_conf::parse(&text);
        let mut all: Vec<(String, String)> = doc
            .pairs
            .into_iter()
            .filter(|(k, _)| !k.starts_with(VAR_PREFIX))
            .collect();
        all.extend(
            vars.iter()
                .map(|(k, v)| (format!("{VAR_PREFIX}{}", k.to_ascii_uppercase()), v.clone())),
        );
        nexa_conf::write_atomic(&path, &nexa_conf::serialize(&[], &all))
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
                    "{name}: 비밀번호 봉투를 열 수 없습니다(다른 기기 키 또는 손상) - `nsql conn add {name} …`로 다시 저장하세요"
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

    /// ★ 삭제 = **`profiles/.deleted/`로 이동**(T-31b · 11-04 · 사용자 09-14 실수 삭제 사례 · 복구 가능 · 봉투 도메인은 이름이라 같은 이름으로
    /// 되살리면 그대로 열린다) — 있었으면 true. 옮긴 파일 이름 = `<해시>.<유닉스 초>[-n].conf`(같은 이름을 여러 번 지워도 겹치지 않음) ·
    /// 종전 꼴 파일(글자 그대로 같은 이름)도 함께 옮긴다. 보존 기간은 호스트가 [`Self::purge_deleted`]로 다룬다.
    pub fn remove(&self, name: &str) -> io::Result<bool> {
        let path = self.path_of(name)?;
        let bin = self.dir.join(PROFILES_DIR).join(DELETED_DIR);
        let stamp = now_secs();
        let stem = Self::file_name(name);
        let stem = stem.trim_end_matches(&format!(".{EXT}")).to_string();
        let mut removed = false;
        let mut n = 0u32;
        let legacy = self.legacy_path(name).filter(|o| *o != path);
        for p in std::iter::once(path).chain(legacy) {
            if !p.is_file() {
                continue;
            }
            std::fs::create_dir_all(&bin)?;
            // 같은 초에 같은 이름을 두 번 지워도 덮어쓰지 않는다(`-k` 꼬리 · 빈 자리까지).
            let mut k = n;
            let dst = loop {
                let suffix = if k == 0 {
                    String::new()
                } else {
                    format!("-{k}")
                };
                let cand = bin.join(format!("{stem}.{stamp}{suffix}.{EXT}"));
                if !cand.exists() {
                    break cand;
                }
                k += 1;
            };
            if std::fs::rename(&p, &dst).is_err() {
                std::fs::copy(&p, &dst)?;
                std::fs::remove_file(&p)?;
            }
            removed = true;
            n += 1;
        }
        Ok(removed)
    }

    /// `.deleted/`의 프로필(최근 것부터 · 이름을 모르는 파일은 건너뜀).
    pub fn deleted(&self) -> io::Result<Vec<DeletedProfile>> {
        let bin = self.dir.join(PROFILES_DIR).join(DELETED_DIR);
        let mut out = Vec::new();
        let rd = match std::fs::read_dir(&bin) {
            Ok(r) => r,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e),
        };
        for e in rd.flatten() {
            let file = e.file_name();
            let Some(file) = file.to_str() else { continue };
            let Some(body) = file.strip_suffix(&format!(".{EXT}")) else {
                continue;
            };
            // `<해시>.<초>[-n]` — 둘째 조각이 지운 시각.
            let deleted_at = body
                .split('.')
                .nth(1)
                .and_then(|t| t.split('-').next())
                .and_then(|t| t.parse::<u64>().ok())
                .unwrap_or(0);
            let Ok(text) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            let (spec, _, name) = Self::parse_doc(&text);
            let Some(name) = name.filter(|n| is_profile_name(n)) else {
                continue;
            };
            out.push(DeletedProfile {
                name,
                spec,
                deleted_at,
                path: e.path(),
            });
        }
        out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then(a.name.cmp(&b.name)));
        Ok(out)
    }

    /// 되살리기 — `.deleted/`의 **가장 최근** 같은 이름을 제자리로(있었으면 true). 살아 있는 같은 이름이 있으면 `AlreadyExists`
    /// (덮어쓰지 않는다 · 호스트가 먼저 지우거나 다른 이름으로).
    pub fn restore(&self, name: &str) -> io::Result<bool> {
        let path = self.path_of(name)?;
        if path.is_file() || self.legacy_path(name).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                name.to_string(),
            ));
        }
        let Some(d) = self.deleted()?.into_iter().find(|d| d.name == name) else {
            return Ok(false);
        };
        if std::fs::rename(&d.path, &path).is_err() {
            std::fs::copy(&d.path, &path)?;
            std::fs::remove_file(&d.path)?;
        }
        Ok(true)
    }

    /// 보존 기간이 지난 `.deleted/` 파일을 지운다(`keep_days` 0 = 전부 즉시) — 지운 수. 폴더가 없으면 0.
    pub fn purge_deleted(&self, keep_days: u64) -> io::Result<usize> {
        let cutoff = now_secs().saturating_sub(keep_days.saturating_mul(86_400));
        let mut n = 0;
        for d in self.deleted()? {
            if d.deleted_at <= cutoff && std::fs::remove_file(&d.path).is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }

    /// ★ 내보내기 글(T-31b · D-459 · **비밀번호 제외** · 프로필마다 블록 · 첫 줄 `name=` · 변수 줄 포함 · 블록 사이 빈 줄) — `names`가 비면 전부.
    /// 모르는 이름은 조용히 건너뛴다(호스트가 목록으로 고른다).
    pub fn export_text(&self, names: &[String]) -> io::Result<String> {
        let mut out = String::from(EXPORT_HEADER);
        out.push('\n');
        for p in self.list()? {
            if !names.is_empty() && !names.contains(&p.name) {
                continue;
            }
            let Some(path) = self.read_path(&p.name)? else {
                continue;
            };
            let text = std::fs::read_to_string(&path)?;
            out.push('\n');
            out.push_str(&format!("name={}\n", p.name));
            for line in text.lines() {
                let l = line.trim_end_matches('\r');
                let key = l.split_once('=').map(|(k, _)| k.trim()).unwrap_or("");
                if l.trim().is_empty()
                    || l.trim_start().starts_with('#')
                    || key == "name"
                    || key == "secret"
                {
                    continue;
                }
                out.push_str(l);
                out.push('\n');
            }
        }
        Ok(out)
    }

    /// ★ 가져오기 — 블록마다 저장(비밀번호 **없음** · 있어도 무시) · 같은 이름 = `overwrite`면 덮어쓰기(변수 포함) 아니면 건너뜀 ·
    /// 이름 규칙에 어긋나는 블록도 건너뜀(보고에 이름 그대로). 파일 전체가 블록 하나도 못 만들면 `InvalidData`.
    pub fn import_text(&self, text: &str, overwrite: bool) -> io::Result<ImportReport> {
        let mut rep = ImportReport::default();
        let existing: Vec<String> = self.list()?.into_iter().map(|p| p.name).collect();
        let mut blocks = 0usize;
        for block in text.replace("\r\n", "\n").split("\n\n") {
            let body: String = block
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
                .map(|l| format!("{l}\n"))
                .collect();
            if body.is_empty() {
                continue;
            }
            blocks += 1;
            let (mut spec, _secret, name) = Self::parse_doc(&body);
            spec.password = None;
            let Some(name) = name else {
                rep.skipped.push("(no name)".into());
                continue;
            };
            if !is_profile_name(&name) {
                rep.skipped.push(name);
                continue;
            }
            let was = existing.contains(&name);
            if was && !overwrite {
                rep.skipped.push(name);
                continue;
            }
            self.save(&name, &spec)?;
            let vars: Vec<(String, String)> = nexa_conf::parse(&body)
                .pairs
                .into_iter()
                .filter_map(|(k, v)| k.strip_prefix(VAR_PREFIX).map(|n| (n.to_string(), v)))
                .collect();
            if !vars.is_empty() || was {
                let _ = self.set_vars(&name, &vars);
            }
            if was {
                rep.replaced += 1;
            } else {
                rep.added += 1;
            }
        }
        if blocks == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no profile blocks (expected `name=` lines)",
            ));
        }
        Ok(rep)
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
                "프로필 '{t}'이(가) 없습니다 - `nsql conn list`로 확인하거나 접속 문자열을 쓰세요"
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
                "ssh" => spec.ssh = Some(v),
                "ssh_key" => spec.ssh_key = Some(v),
                "autocommit" => spec.autocommit = nsql_script::parse_on_off(&v),
                "readonly" => spec.read_only = nsql_script::parse_on_off(&v),
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

    /// T-153: 프로필 변수 = `var.<이름>=값` 줄 · 스펙 재저장에도 보존 · 이름 대문자 정규화 · 잘못된 이름 거부.
    #[test]
    fn profile_vars_round_trip_and_survive_save() {
        let d = std::env::temp_dir().join(format!("nsql-vault-vars-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let v = Vault::open(&d).unwrap();
        let spec = ConnectSpec {
            dialect: Some(Dialect::Sqlite),
            database: Some(":memory:".into()),
            ..ConnectSpec::default()
        };
        v.save("dev", &spec).unwrap();
        assert!(v.vars("dev").unwrap().is_empty());
        v.set_vars(
            "dev",
            &[("env".into(), "prod".into()), ("Limit".into(), "10".into())],
        )
        .unwrap();
        assert_eq!(
            v.vars("dev").unwrap(),
            vec![
                ("ENV".to_string(), "prod".to_string()),
                ("LIMIT".to_string(), "10".to_string())
            ]
        );
        // 스펙만 다시 저장해도 변수 줄은 남는다.
        v.save("dev", &spec).unwrap();
        assert_eq!(v.vars("dev").unwrap().len(), 2);
        assert_eq!(
            v.get("dev").unwrap().unwrap().database.as_deref(),
            Some(":memory:")
        );
        assert!(v.set_vars("dev", &[("1bad".into(), "x".into())]).is_err());
        assert!(v.set_vars("nope", &[]).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ T-31b(11-04): 삭제 = `.deleted/` 이동 → 목록에서 사라짐 · `deleted()`에 보임 · `restore` = 제자리(비밀번호 봉투도 그대로 열림) ·
    /// 같은 이름이 살아 있으면 AlreadyExists · `purge_deleted(0)` = 즉시 비움.
    #[test]
    fn delete_moves_to_deleted_and_restore_brings_back() {
        let d = tmp("deleted");
        let v = Vault::open(&d).unwrap();
        v.save("p1", &spec(Some("pw1"))).unwrap();
        assert!(v.remove("p1").unwrap());
        assert!(v.list().unwrap().is_empty());
        assert_eq!(
            v.file_count().unwrap(),
            0,
            "profiles 폴더의 .conf 수 = 0(.deleted는 하위 폴더)"
        );
        let del = v.deleted().unwrap();
        assert_eq!(del.len(), 1);
        assert_eq!(del[0].name, "p1");
        assert!(del[0].deleted_at > 0);
        assert!(v.restore("p1").unwrap());
        assert_eq!(
            v.get("p1").unwrap().unwrap().password.as_deref(),
            Some("pw1"),
            "봉투 그대로"
        );
        assert!(v.deleted().unwrap().is_empty());
        // 살아 있는 이름은 덮어쓰지 않는다 · 없는 이름 = false.
        v.save("p2", &spec(None)).unwrap();
        v.remove("p2").unwrap();
        v.save("p2", &spec(None)).unwrap();
        assert_eq!(
            v.restore("p2").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(!v.restore("nope").unwrap());
        // 두 번 지운 같은 이름 = 파일 둘 · 보존 0일 = 전부 삭제.
        v.remove("p2").unwrap();
        assert_eq!(v.deleted().unwrap().len(), 2);
        assert_eq!(v.purge_deleted(0).unwrap(), 2);
        assert!(v.deleted().unwrap().is_empty());
        assert_eq!(v.purge_deleted(30).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// ★ T-31b(D-459): 내보내기 = 비밀번호 제외 · 변수 포함 · 가져오기 = 새 이름 추가 · 같은 이름은 건너뜀 · `overwrite` = 교체(변수까지) ·
    /// `readonly=` 키 왕복 · 빈 파일 = InvalidData.
    #[test]
    fn export_import_round_trip_without_passwords() {
        let d1 = tmp("export");
        let d2 = tmp("import");
        let v1 = Vault::open(&d1).unwrap();
        let mut ro = spec(Some("secret"));
        ro.read_only = Some(true);
        v1.save("prod", &ro).unwrap();
        v1.set_vars("prod", &[("REGION".into(), "kr".into())])
            .unwrap();
        v1.save("dev", &spec(None)).unwrap();
        let text = v1.export_text(&[]).unwrap();
        assert!(text.starts_with(EXPORT_HEADER));
        assert!(!text.contains("secret="), "비밀번호 봉투는 나가지 않는다");
        assert!(
            text.contains("name=prod")
                && text.contains("readonly=on")
                && text.contains("var.REGION=kr")
        );
        let only: String = v1.export_text(&["dev".into()]).unwrap();
        assert!(only.contains("name=dev") && !only.contains("name=prod"));
        let v2 = Vault::open(&d2).unwrap();
        let r = v2.import_text(&text, false).unwrap();
        assert_eq!((r.added, r.replaced, r.skipped.len()), (2, 0, 0));
        let p = v2.get("prod").unwrap().unwrap();
        assert_eq!(
            (p.read_only, p.password.is_none(), p.host.as_deref()),
            (Some(true), true, Some("db.example.com"))
        );
        assert_eq!(
            v2.vars("prod").unwrap(),
            vec![("REGION".to_string(), "kr".to_string())]
        );
        let r = v2.import_text(&text, false).unwrap();
        assert_eq!((r.added, r.replaced), (0, 0));
        assert_eq!(r.skipped, vec!["dev".to_string(), "prod".to_string()]);
        let r = v2.import_text(&text, true).unwrap();
        assert_eq!((r.added, r.replaced, r.skipped.len()), (0, 2, 0));
        assert!(v2.import_text("# nothing\n\n", false).is_err());
        let _ = std::fs::remove_dir_all(&d1);
        let _ = std::fs::remove_dir_all(&d2);
    }

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
            tls: None,
            tls_ca: None,
            ssh: None,
            ssh_key: None,
            autocommit: None,
            read_only: None,
            driver: None,
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
    /// `APPDB.conf`가 `appdb.conf`를 덮어쓴 뒤 삭제돼 둘 다 사라졌다. 해시 파일 이름은 OS와 무관하게 둘을 구별한다.
    #[test]
    fn case_only_rename_keeps_new_profile() {
        let d = tmp("vault-case");
        let v = Vault::open(&d).unwrap();
        v.save("appdb", &spec(Some("pw"))).unwrap();
        v.save("APPDB", &spec(Some("pw2"))).unwrap();
        assert_ne!(Vault::file_name("appdb"), Vault::file_name("APPDB"));
        assert!(v.remove("appdb").unwrap());
        let got = v.get("APPDB").unwrap().expect("새 이름은 남는다");
        assert_eq!(got.password.as_deref(), Some("pw2"));
        assert!(v.get("appdb").unwrap().is_none());
        assert_eq!(
            v.list()
                .unwrap()
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["APPDB"]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// 종전 꼴(`<이름>.conf` · `name=` 없음) 읽기 호환 + 저장 시 이관 · 대/소문자만 다른 종전 파일은 건드리지 않는다.
    #[test]
    fn legacy_files_are_read_and_migrated_exact_case_only() {
        let d = tmp("vault-legacy");
        let v = Vault::open(&d).unwrap();
        let legacy = "dialect=oracle\nuser=scott\nhost=h\nport=1521\ndatabase=orcl\n";
        std::fs::write(d.join("profiles/appdb.conf"), legacy).unwrap();
        assert_eq!(v.list().unwrap()[0].name, "appdb");
        assert_eq!(
            v.get("appdb").unwrap().unwrap().user.as_deref(),
            Some("scott")
        );
        // 대/소문자만 다른 새 이름으로 저장 — 종전 `appdb.conf`는 남아야 한다(대/소문자 무시 FS에서도 글자 비교).
        v.save("APPDB", &spec(None)).unwrap();
        let names: Vec<String> = v.list().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["APPDB", "appdb"]);
        assert!(d.join("profiles/appdb.conf").is_file());
        // 같은 이름으로 다시 저장 = 해시 파일로 이관 · 종전 파일 삭제.
        v.save("appdb", &spec(None)).unwrap();
        assert!(v.path_of("appdb").unwrap().is_file());
        assert!(v.legacy_path("appdb").is_none());
        let names: Vec<String> = v.list().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["APPDB", "appdb"]);
        // 경로 찾기(키 없이): 해시 파일 → 종전 꼴(글자 그대로) → 저장될 해시 경로.
        assert_eq!(v.locate("appdb"), v.path_of("appdb").unwrap());
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

#[cfg(test)]
mod secret_tests {
    use super::Vault;

    /// 임의 이름 비밀: 저장 → 읽기 같음 · 이름 목록 · 지우기 · 없는 이름 None · 파일에 평문 없음(11-11).
    #[test]
    fn put_get_delete_secret_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nsql-vault-secret-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let v = Vault::open(&dir).expect("open");
        v.put_secret("ai.claude.default", b"sk-ant-test-123")
            .expect("put");
        assert_eq!(
            v.get_secret("ai.claude.default").as_deref(),
            Some(&b"sk-ant-test-123"[..])
        );
        assert_eq!(v.secret_names(), vec!["ai.claude.default".to_string()]);
        let raw = std::fs::read_dir(dir.join("secrets"))
            .expect("dir")
            .flatten()
            .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
            .collect::<String>();
        assert!(!raw.contains("sk-ant-test"), "평문이 파일에 없다: {raw}");
        assert!(v.get_secret("nope").is_none());
        v.delete_secret("ai.claude.default").expect("delete");
        assert!(v.get_secret("ai.claude.default").is_none());
        v.delete_secret("ai.claude.default")
            .expect("두 번 지워도 오류 없음");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
