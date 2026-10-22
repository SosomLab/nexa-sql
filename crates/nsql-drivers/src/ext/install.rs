//! ★ 확장 드라이버 **내려받기·설치**(T-29 · docs/22 §2-3·§3): 색인(`releases/latest/download/index.json` · ETag 캐시) →
//! 버전 해석(색인 `latest` > GitHub API 태그 접두) → 자산 `nsql-driver-<id>-<ver>-<os>-<arch>.zip` + `.sha256` + `.sig` →
//! sha256 일치 → Ed25519 검증([`super::keys::DRIVER_KEYS`] · **우회 없음**) → 임시 폴더에 풀기 → 매니페스트·라이브러리 확인 →
//! `drivers/<id>/<ver>/`로 **원자적 rename**(같은 볼륨 `.tmp/` 아래 · 중단돼도 반쪽 설치 없음).
//!
//! 네트워크(26 §8): 사용자 동작(install·update·search)에서만 · 재시도 없음 · 동시 1 · 색인은 ETag 조건부 GET(304 = 캐시) ·
//! GitHub API(익명 60회/시간)는 색인에 `latest`가 없을 때만 · 토큰 = `NSQL_GITHUB_TOKEN`(환경 변수만 · 설정 파일에 안 적는다).
use super::sha256::{hex, sha256, unhex};
use super::{library_file_name_for, parse_manifest, version_cmp, ExtInfo, Manifest, MANIFEST};
use nexa_license::ed25519::Ed25519Verifier;
use nexa_license::keys::RootKey;
use nexa_license::types::Alg;
use nexa_license::verify::SigVerifier;
use nsql_json::Json;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 기본 배포 저장소(설정 `driver.repo`).
pub const DEFAULT_REPO: &str = "SosomLab/nexa-sql-drivers";
/// 색인 자산 이름 · 캐시 파일 이름(드라이버 폴더 바로 아래).
pub const INDEX_FILE: &str = "index.json";
const INDEX_ETAG: &str = "index.etag";
const TMP_DIR: &str = ".tmp";
/// 색인 캐시를 그대로 쓰는 시간(`--refresh`·install은 조건부 GET으로 확인).
const INDEX_FRESH: Duration = Duration::from_secs(3600);

/// 어디서 받는가.
#[derive(Clone, Debug)]
pub struct Source {
    /// `owner/name`.
    pub repo: String,
    /// GitHub 토큰(사설 저장소·한도) — 환경 변수에서만.
    pub token: Option<String>,
    pub timeout: Duration,
    /// 자산 기본 URL(`https://github.com` · 시험 = 로컬 http 서버).
    pub base: String,
    /// API 기본 URL(`https://api.github.com`).
    pub api: String,
}

impl Source {
    #[must_use]
    pub fn new(repo: &str) -> Source {
        Source {
            repo: repo.to_string(),
            token: std::env::var("NSQL_GITHUB_TOKEN")
                .ok()
                .filter(|t| !t.is_empty()),
            timeout: Duration::from_secs(30),
            base: "https://github.com".into(),
            api: "https://api.github.com".into(),
        }
    }

    fn options(&self, api: bool) -> nsql_net::Options {
        let mut o = nsql_net::Options {
            timeout: self.timeout,
            ..nsql_net::Options::default()
        };
        if api {
            o.headers
                .push(("Accept".into(), "application/vnd.github+json".into()));
            o.headers
                .push(("X-GitHub-Api-Version".into(), "2022-11-28".into()));
        }
        if let Some(t) = &self.token {
            o.headers
                .push(("Authorization".into(), format!("Bearer {t}")));
        }
        o
    }
}

/// 색인의 드라이버 하나.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub dialect: String,
    /// 자산이 있는 저장소(없으면 색인 저장소).
    pub repo: Option<String>,
    /// 태그 접두(없으면 `<id>-v`).
    pub tag_prefix: Option<String>,
    /// 최신 정식 버전(파이프라인이 적는다 · 없으면 API로 해석).
    pub latest: Option<String>,
    pub license: Option<String>,
    pub sxs: bool,
    /// 지원 `os-arch` 목록(비면 전부).
    pub platforms: Vec<String>,
}

/// 색인 전체.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    pub drivers: Vec<IndexEntry>,
}

impl Index {
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&IndexEntry> {
        self.drivers.iter().find(|d| d.id == id)
    }
}

fn field<'a>(o: &'a [(String, Json)], k: &str) -> Option<&'a Json> {
    o.iter().find(|(n, _)| n == k).map(|(_, v)| v)
}

fn text(o: &[(String, Json)], k: &str) -> Option<String> {
    match field(o, k) {
        Some(Json::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// `index.json` 파싱(순수): `{"drivers":[{id,name,dialect,repo,tag_prefix,latest,license,sxs,platforms},…]}`.
pub fn parse_index(text_: &str) -> Result<Index, String> {
    let root = nsql_json::parse(text_)?;
    let Json::Obj(o) = &root else {
        return Err("index: top level must be an object".into());
    };
    let Some(Json::Arr(list)) = field(o, "drivers") else {
        return Err("index: \"drivers\" array missing".into());
    };
    let mut drivers = Vec::new();
    for d in list {
        let Json::Obj(f) = d else { continue };
        let Some(id) = text(f, "id").filter(|s| !s.is_empty()) else {
            continue;
        };
        drivers.push(IndexEntry {
            name: text(f, "name").unwrap_or_else(|| id.clone()),
            dialect: text(f, "dialect").unwrap_or_else(|| id.clone()),
            repo: text(f, "repo"),
            tag_prefix: text(f, "tag_prefix"),
            latest: text(f, "latest"),
            license: text(f, "license"),
            sxs: matches!(field(f, "sxs"), Some(Json::Bool(true))),
            platforms: match field(f, "platforms") {
                Some(Json::Arr(a)) => a.iter().filter_map(Json::scalar_text).collect(),
                _ => Vec::new(),
            },
            id,
        });
    }
    Ok(Index { drivers })
}

/// 이 빌드의 `os-arch`(`windows-x86_64` · `macos-aarch64` · `linux-x86_64` · docs/22 §2-3).
#[must_use]
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// 자산 파일 이름(`.zip` 포함).
#[must_use]
pub fn asset_name(id: &str, ver: &str) -> String {
    format!("nsql-driver-{id}-{ver}-{}.zip", platform())
}

/// 자산 이름에서 `(id, ver)` — `nsql-driver-<id>-<ver>-<os>-<arch>.zip`(id에 `-`가 있어도 버전은 숫자로 시작하므로 가른다).
#[must_use]
pub fn parse_asset_name(name: &str) -> Option<(String, String)> {
    let stem = name.strip_suffix(".zip")?.strip_prefix("nsql-driver-")?;
    let plat = platform();
    let stem = stem.strip_suffix(&format!("-{plat}"))?;
    let (id, ver) = stem.rsplit_once('-')?;
    let mut id = id.to_string();
    let mut ver = ver.to_string();
    // 버전에 `-`가 있을 수 있다(프리릴리스): 숫자로 시작하는 가장 왼쪽 조각부터 버전.
    while let Some((l, r)) = id.rsplit_once('-') {
        if r.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            ver = format!("{r}-{ver}");
            id = l.to_string();
        } else {
            break;
        }
    }
    (!id.is_empty() && ver.chars().next().is_some_and(|c| c.is_ascii_digit())).then_some((id, ver))
}

fn repo_of<'a>(src: &'a Source, e: &'a IndexEntry) -> &'a str {
    e.repo.as_deref().unwrap_or(&src.repo)
}

fn tag_prefix(e: &IndexEntry) -> String {
    e.tag_prefix
        .clone()
        .unwrap_or_else(|| format!("{}-v", e.id))
}

/// 자산 URL(`<base>/<repo>/releases/download/<tag>/<name>`).
#[must_use]
pub fn asset_url(src: &Source, e: &IndexEntry, ver: &str, name: &str) -> String {
    format!(
        "{}/{}/releases/download/{}{ver}/{name}",
        src.base,
        repo_of(src, e),
        tag_prefix(e)
    )
}

/// 색인 URL(저장소 최신 릴리스의 `index.json` · API 아님).
#[must_use]
pub fn index_url(src: &Source) -> String {
    format!(
        "{}/{}/releases/latest/download/{INDEX_FILE}",
        src.base, src.repo
    )
}

/// 캐시된 색인(드라이버 폴더의 `index.json`).
#[must_use]
pub fn cached_index(root: &Path) -> Option<Index> {
    let t = std::fs::read_to_string(root.join(INDEX_FILE)).ok()?;
    parse_index(&t).ok()
}

/// 캐시 나이.
#[must_use]
pub fn cached_index_age(root: &Path) -> Option<Duration> {
    let m = std::fs::metadata(root.join(INDEX_FILE))
        .ok()?
        .modified()
        .ok()?;
    m.elapsed().ok()
}

/// 색인 가져오기: 1시간 안 캐시 = 그대로(`refresh` = 조건부 GET) · 304 = 캐시 · 실패 + 캐시 = 캐시(`from_cache` true) · 둘 다 없음 = 오류.
pub fn fetch_index(src: &Source, root: &Path, refresh: bool) -> Result<(Index, bool), String> {
    let cache = cached_index(root);
    if !refresh && cache.is_some() && cached_index_age(root).is_some_and(|a| a < INDEX_FRESH) {
        return Ok((cache.unwrap_or_default(), true));
    }
    let mut o = src.options(false);
    if cache.is_some() {
        if let Ok(etag) = std::fs::read_to_string(root.join(INDEX_ETAG)) {
            if !etag.trim().is_empty() {
                o.headers
                    .push(("If-None-Match".into(), etag.trim().to_string()));
            }
        }
    }
    match nsql_net::get_bytes(&index_url(src), &o) {
        Ok((r, body)) if r.status == 200 => {
            let text_ = String::from_utf8_lossy(&body).into_owned();
            let idx = parse_index(&text_)?;
            let _ = std::fs::create_dir_all(root);
            std::fs::write(root.join(INDEX_FILE), text_.as_bytes())
                .map_err(|e| format!("write index cache: {e}"))?;
            let _ = std::fs::write(
                root.join(INDEX_ETAG),
                r.header("etag").unwrap_or("").as_bytes(),
            );
            Ok((idx, false))
        }
        Ok((r, _)) if r.status == 304 => {
            // 캐시 시각을 지금으로(다음 1시간은 묻지 않는다).
            let p = root.join(INDEX_FILE);
            if let Ok(t) = std::fs::read(&p) {
                let _ = std::fs::write(&p, t);
            }
            Ok((cache.ok_or("304 without cache")?, true))
        }
        Ok((r, _)) => match cache {
            Some(c) => Ok((c, true)),
            None => Err(format!("index: HTTP {} from {}", r.status, index_url(src))),
        },
        Err(e) => match cache {
            Some(c) => Ok((c, true)),
            None => Err(format!("index: {e} ({})", index_url(src))),
        },
    }
}

/// GitHub API에서 접두가 맞는 태그 중 최신(순수 부분 = [`pick_latest_tag`]).
fn api_latest(src: &Source, e: &IndexEntry, pre: bool) -> Result<String, String> {
    let url = format!(
        "{}/repos/{}/releases?per_page=100",
        src.api,
        repo_of(src, e)
    );
    let (r, body) = nsql_net::get_bytes(&url, &src.options(true)).map_err(|er| er.to_string())?;
    if r.status != 200 {
        return Err(format!("GitHub API: HTTP {} ({url})", r.status));
    }
    let json = nsql_json::parse(&String::from_utf8_lossy(&body))?;
    pick_latest_tag(&json, &tag_prefix(e), pre)
        .ok_or_else(|| format!("no release with tag prefix {:?}", tag_prefix(e)))
}

/// 릴리스 목록 JSON에서 `prefix`로 시작하는 태그 중 최신 버전(순수): draft 제외 · prerelease는 `pre`일 때만.
#[must_use]
pub fn pick_latest_tag(releases: &Json, prefix: &str, pre: bool) -> Option<String> {
    let Json::Arr(list) = releases else {
        return None;
    };
    let mut best: Option<String> = None;
    for r in list {
        let Json::Obj(f) = r else { continue };
        if matches!(field(f, "draft"), Some(Json::Bool(true))) {
            continue;
        }
        if !pre && matches!(field(f, "prerelease"), Some(Json::Bool(true))) {
            continue;
        }
        let Some(tag) = text(f, "tag_name") else {
            continue;
        };
        let Some(ver) = tag.strip_prefix(prefix) else {
            continue;
        };
        if best
            .as_deref()
            .is_none_or(|b| version_cmp(ver, b) == std::cmp::Ordering::Greater)
        {
            best = Some(ver.to_string());
        }
    }
    best
}

/// 설치할 버전: 지정 > 색인 `latest`(정식) > API.
pub fn resolve_version(
    src: &Source,
    e: &IndexEntry,
    want: Option<&str>,
    pre: bool,
) -> Result<String, String> {
    if let Some(v) = want.filter(|v| !v.is_empty() && *v != "latest") {
        return Ok(v.to_string());
    }
    if !pre {
        if let Some(l) = e.latest.as_deref().filter(|l| !l.is_empty()) {
            return Ok(l.to_string());
        }
    }
    api_latest(src, e, pre)
}

/// 설치 결과.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallReport {
    pub id: String,
    pub version: String,
    pub dir: PathBuf,
    pub files: usize,
    pub bytes: u64,
    /// 서명에 맞은 키 id.
    pub key_id: String,
    pub manifest: Option<Manifest>,
}

/// `.sha256` 글(`<hex>  <이름>` 또는 `<hex>`)이 바이트와 맞는가.
pub fn verify_sha256_text(text_: &str, data: &[u8]) -> Result<(), String> {
    let want = text_
        .split_whitespace()
        .next()
        .and_then(unhex)
        .filter(|v| v.len() == 32)
        .ok_or("sha256 file: no 64-hex digest")?;
    let got = sha256(data);
    if want.as_slice() == got {
        Ok(())
    } else {
        Err(format!(
            "sha256 mismatch: expected {}, got {}",
            hex(&want),
            hex(&got)
        ))
    }
}

/// `.sig` 글 = `ed25519 <key-id> <hex 128>`(또는 `<hex 128>`만) → 맞은 키 id.
pub fn verify_sig_text(text_: &str, data: &[u8], keys: &[RootKey]) -> Result<String, String> {
    if keys.is_empty() {
        return Err(
            "no trusted driver signing keys are embedded in this build (docs/22 §6 · D-348)".into(),
        );
    }
    let toks: Vec<&str> = text_.split_whitespace().collect();
    let (key_id, sig_hex) = match toks.as_slice() {
        [alg, id, sig] if alg.eq_ignore_ascii_case("ed25519") => (Some(*id), *sig),
        [sig] => (None, *sig),
        _ => return Err("sig file: expected `ed25519 <key-id> <hex>`".into()),
    };
    let sig = unhex(sig_hex)
        .filter(|s| s.len() == 64)
        .ok_or("sig file: signature must be 128 hex chars")?;
    let v = Ed25519Verifier;
    for k in keys {
        if k.alg != Alg::Ed25519 || key_id.is_some_and(|id| id != k.id) {
            continue;
        }
        if v.verify(Alg::Ed25519, k.public, data, &sig) {
            return Ok(k.id.to_string());
        }
    }
    Err(match key_id {
        Some(id) if !keys.iter().any(|k| k.id == id) => {
            format!("signature key {id:?} is not trusted by this build")
        }
        _ => "signature verification failed — the package was not signed by a trusted key".into(),
    })
}

/// ★ 핵심(순수에 가깝다 · 파일계만): 바이트 → 검증 → 풀기 → 확인 → 원자적 설치. `expect` = 자산 이름이 말한 `(id, ver)`(매니페스트와 대조).
pub fn install_bytes(
    root: &Path,
    zip: &[u8],
    sha_text: Option<&str>,
    sig_text: &str,
    keys: &[RootKey],
    expect: Option<(&str, &str)>,
    force: bool,
) -> Result<InstallReport, String> {
    if let Some(t) = sha_text {
        verify_sha256_text(t, zip)?;
    }
    let key_id = verify_sig_text(sig_text, zip, keys)?;
    let tmp = root.join(TMP_DIR).join(format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let pkg = tmp.join("pkg");
    let _ = std::fs::remove_dir_all(&tmp);
    let result = (|| {
        let ex = super::zip::extract(zip, &pkg, true)?;
        let manifest = match std::fs::read_to_string(pkg.join(MANIFEST)) {
            Ok(t) => Some(parse_manifest(&t)?),
            Err(_) => None,
        };
        if let Some(m) = &manifest {
            if m.protocol.is_some_and(|p| p != nsql_abi::ABI_VERSION) {
                return Err(format!(
                    "manifest protocol {} ≠ this build's ABI {}",
                    m.protocol.unwrap_or(0),
                    nsql_abi::ABI_VERSION
                ));
            }
        }
        let (id, ver) = match (expect, &manifest) {
            (Some((id, ver)), Some(m)) => {
                if !m.id.is_empty() && m.id != id {
                    return Err(format!("manifest id {:?} ≠ asset id {id:?}", m.id));
                }
                if !m.version.is_empty() && m.version != ver {
                    return Err(format!(
                        "manifest version {:?} ≠ asset version {ver:?}",
                        m.version
                    ));
                }
                (id.to_string(), ver.to_string())
            }
            (Some((id, ver)), None) => (id.to_string(), ver.to_string()),
            (None, Some(m)) if !m.id.is_empty() && !m.version.is_empty() => {
                (m.id.clone(), m.version.clone())
            }
            (None, _) => return Err(format!("{MANIFEST} with id and version is required")),
        };
        if id.contains(['/', '\\', '.']) || ver.contains(['/', '\\']) || ver.starts_with('.') {
            return Err(format!("unsafe id/version: {id}@{ver}"));
        }
        let lib = manifest
            .as_ref()
            .and_then(|m| m.lib.clone())
            .unwrap_or_else(|| library_file_name_for(&id));
        if !pkg.join(&lib).is_file() {
            return Err(format!("package has no driver library {lib:?}"));
        }
        let dest = root.join(&id).join(&ver);
        if dest.exists() {
            if !force {
                return Err(format!(
                    "{id}@{ver} is already installed ({})",
                    dest.display()
                ));
            }
            std::fs::remove_dir_all(&dest)
                .map_err(|e| format!("remove {}: {e}", dest.display()))?;
        }
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        std::fs::rename(&pkg, &dest)
            .map_err(|e| format!("install {} → {}: {e}", pkg.display(), dest.display()))?;
        Ok(InstallReport {
            id,
            version: ver,
            dir: dest,
            files: ex.files,
            bytes: ex.bytes,
            key_id: key_id.clone(),
            manifest,
        })
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// 진행 콜백: `(단계, 받은 바이트, 전체)` — 단계 = `index` · `download` · `verify` · `install`.
pub type Progress<'a> = &'a mut dyn FnMut(&str, u64, Option<u64>);

/// 네트워크 설치: 색인 → 버전 → 자산 셋 내려받기 → [`install_bytes`].
#[allow(clippy::too_many_arguments)]
pub fn install(
    src: &Source,
    root: &Path,
    id: &str,
    want: Option<&str>,
    pre: bool,
    force: bool,
    keys: &[RootKey],
    progress: Progress<'_>,
) -> Result<InstallReport, String> {
    progress("index", 0, None);
    let (index, _) = fetch_index(src, root, false)?;
    let entry = index
        .find(id)
        .cloned()
        .ok_or_else(|| format!("{id:?} is not in the driver index (nsql driver search)"))?;
    let plat = platform();
    if !entry.platforms.is_empty() && !entry.platforms.contains(&plat) {
        return Err(format!(
            "{id} has no build for {plat} (available: {})",
            entry.platforms.join(", ")
        ));
    }
    let ver = resolve_version(src, &entry, want, pre)?;
    if !force && root.join(id).join(&ver).exists() {
        return Err(format!("{id}@{ver} is already installed"));
    }
    let name = asset_name(id, &ver);
    let url = asset_url(src, &entry, &ver, &name);
    let stem = url.trim_end_matches(".zip").to_string();
    let o = src.options(false);
    progress("download", 0, None);
    let mut zip = Vec::new();
    let r = nsql_net::get(&url, &o, &mut zip, &mut |n, t| progress("download", n, t))
        .map_err(|e| format!("download {url}: {e}"))?;
    if r.status != 200 {
        return Err(format!("download {url}: HTTP {}", r.status));
    }
    let small = |suffix: &str| -> Result<String, String> {
        let u = format!("{stem}.{suffix}");
        let (r, b) = nsql_net::get_bytes(&u, &o).map_err(|e| format!("{u}: {e}"))?;
        if r.status != 200 {
            return Err(format!("{u}: HTTP {}", r.status));
        }
        Ok(String::from_utf8_lossy(&b).into_owned())
    };
    let sha = small("sha256")?;
    let sig = small("sig")?;
    progress("verify", zip.len() as u64, Some(zip.len() as u64));
    let rep = install_bytes(root, &zip, Some(&sha), &sig, keys, Some((id, &ver)), force)?;
    progress("install", rep.bytes, Some(rep.bytes));
    Ok(rep)
}

/// 오프라인 설치(`--file <zip>`): 옆의 `<이름 없는 .zip>.sha256`·`.sig`(없으면 `<이름>.zip.sha256`·`.zip.sig`)를 읽는다.
pub fn install_file(
    root: &Path,
    zip_path: &Path,
    force: bool,
    keys: &[RootKey],
) -> Result<InstallReport, String> {
    let zip = std::fs::read(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let sidecar = |suffix: &str| -> Option<String> {
        let s = zip_path.to_string_lossy();
        let a = format!("{}.{suffix}", s.trim_end_matches(".zip"));
        let b = format!("{s}.{suffix}");
        std::fs::read_to_string(a)
            .or_else(|_| std::fs::read_to_string(b))
            .ok()
    };
    let sig = sidecar("sig").ok_or_else(|| {
        format!(
            "no signature next to {} (expected .sig)",
            zip_path.display()
        )
    })?;
    let sha = sidecar("sha256");
    let expect = zip_path
        .file_name()
        .and_then(|n| parse_asset_name(&n.to_string_lossy()));
    install_bytes(
        root,
        &zip,
        sha.as_deref(),
        &sig,
        keys,
        expect.as_ref().map(|(i, v)| (i.as_str(), v.as_str())),
        force,
    )
}

/// 갱신 후보 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub id: String,
    pub installed: String,
    pub latest: String,
}

/// 설치본(id별 최신) vs 색인 `latest`(순수).
#[must_use]
pub fn updates(index: &Index, installed: &[ExtInfo]) -> Vec<Update> {
    let mut out = Vec::new();
    for e in installed {
        let Some(d) = index.find(&e.id) else { continue };
        let Some(latest) = d.latest.as_deref().filter(|l| !l.is_empty()) else {
            continue;
        };
        if version_cmp(latest, &e.version) == std::cmp::Ordering::Greater {
            out.push(Update {
                id: e.id.clone(),
                installed: e.version.clone(),
                latest: latest.to_string(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_parse_and_names() {
        let idx = parse_index(
            r#"{"drivers":[{"id":"duckdb","name":"DuckDB","dialect":"duckdb","latest":"1.2.0","sxs":false,
                "platforms":["windows-x86_64","linux-x86_64"]},{"id":"tibero-odbc","tag_prefix":"tibero-v","repo":"SosomLab/x"},{"name":"no id"}]}"#,
        )
        .expect("index");
        assert_eq!(idx.drivers.len(), 2);
        let d = idx.find("duckdb").expect("duckdb");
        assert_eq!(d.latest.as_deref(), Some("1.2.0"));
        assert_eq!(d.platforms.len(), 2);
        let t = idx.find("tibero-odbc").expect("tibero");
        assert_eq!(t.dialect, "tibero-odbc");
        let src = Source {
            token: None,
            ..Source::new("SosomLab/nexa-sql-drivers")
        };
        assert_eq!(
            asset_url(&src, d, "1.2.0", "a.zip"),
            "https://github.com/SosomLab/nexa-sql-drivers/releases/download/duckdb-v1.2.0/a.zip"
        );
        assert_eq!(
            asset_url(&src, t, "7.0.1", "b.zip"),
            "https://github.com/SosomLab/x/releases/download/tibero-v7.0.1/b.zip"
        );
        assert_eq!(
            index_url(&src),
            "https://github.com/SosomLab/nexa-sql-drivers/releases/latest/download/index.json"
        );
        let n = asset_name("tibero-odbc", "7.0.1-rc.1");
        assert_eq!(
            parse_asset_name(&n),
            Some(("tibero-odbc".into(), "7.0.1-rc.1".into()))
        );
        assert_eq!(
            parse_asset_name(&asset_name("duckdb", "1.2.0")),
            Some(("duckdb".into(), "1.2.0".into()))
        );
        assert_eq!(parse_asset_name("other.zip"), None);
        assert!(parse_index("[]").is_err());
    }

    #[test]
    fn latest_tag_and_updates() {
        let rel = nsql_json::parse(
            r#"[{"tag_name":"duckdb-v1.3.0","prerelease":true},{"tag_name":"duckdb-v1.2.0"},
                {"tag_name":"duckdb-v1.10.0","draft":true},{"tag_name":"duckdb-v1.9.1"},{"tag_name":"other-v9"}]"#,
        )
        .expect("json");
        assert_eq!(
            pick_latest_tag(&rel, "duckdb-v", false).as_deref(),
            Some("1.9.1")
        );
        assert_eq!(
            pick_latest_tag(&rel, "duckdb-v", true).as_deref(),
            Some("1.9.1")
        );
        assert_eq!(pick_latest_tag(&rel, "nope-v", true), None);
        let idx = parse_index(
            r#"{"drivers":[{"id":"a","latest":"2.0.0"},{"id":"b","latest":"1.0.0"},{"id":"c"}]}"#,
        )
        .expect("idx");
        let inst = |id: &str, v: &str| ExtInfo {
            id: id.into(),
            version: v.into(),
            path: PathBuf::new(),
            dialect: None,
            abi: None,
            manifest: false,
        };
        let u = updates(
            &idx,
            &[
                inst("a", "1.9.9"),
                inst("b", "1.0.0"),
                inst("c", "0.1"),
                inst("z", "1"),
            ],
        );
        assert_eq!(
            u,
            vec![Update {
                id: "a".into(),
                installed: "1.9.9".into(),
                latest: "2.0.0".into()
            }]
        );
    }

    #[test]
    fn sha_and_sig_text() {
        let data = b"hello";
        let h = hex(&sha256(data));
        assert!(verify_sha256_text(&format!("{h}  x.zip\n"), data).is_ok());
        assert!(verify_sha256_text(&h, data).is_ok());
        assert!(verify_sha256_text("ab", data).is_err());
        assert!(verify_sha256_text(&hex(&sha256(b"other")), data)
            .expect_err("mismatch")
            .contains("mismatch"));
        assert!(verify_sig_text("ed25519 k 00", data, &[])
            .expect_err("no keys")
            .contains("no trusted"));
    }
}
