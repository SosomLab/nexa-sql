//! **확장 매니저**(사용자 09-17 · Sublime Package Control 방식 · docs/50 §10): 저장소 루트의 `index.json`이 패키지
//! 폴더를 식별하고, 각 폴더의 `extension.json`이 설치에 필요한 것(파일 · sha256 · 설치 경로 · 종류)을 말한다.
//! 기본 저장소 = **이 저장소의 `extensions/` 폴더**(소스 트리에서 실행하면 그 폴더 · 아니면 GitHub raw URL). 사용자는
//! 팔레트 "Extension Manager: Add Repository"로 같은 구조의 URL/폴더를 더한다(`extensions.repositories`).
//!
//! 원격 읽기는 `curl -fsSL`(3-OS 기본 탑재 · 외부 crate 0 · 26 §8: 사용자 동작으로만 트래픽). 설치본은
//! `<설정 폴더>/extensions/<id>/<version>/`(SxS) + `installed.json`(설치 파일 목록 · 삭제 때 되감기).

use nsql_settings::json::{self, Json};
use nsql_settings::{config_dir, Settings};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 상세 추적(개발자 모드 · `ext` 층): 저장소 읽기 · 다운로드 바이트/시간/속도 · 검증 · 보관/배치 폴더 · 기록.
/// 매니저는 줄만 모으고 호스트가 `log.dev_mode`/층 마스크로 내보낸다(꺼져 있으면 문자열만 만들고 버림 — 사용자 동작 때만).
#[derive(Default, Debug)]
pub(crate) struct Trace(pub(crate) Vec<String>);

impl Trace {
    fn push(&mut self, s: String) {
        self.0.push(s);
    }
}

fn speed(bytes: usize, d: std::time::Duration) -> String {
    let secs = d.as_secs_f64();
    if secs <= 0.0 || bytes == 0 {
        return String::new();
    }
    format!(
        " · {}/s",
        nsql_core::fmt_bytes((bytes as f64 / secs) as u64)
    )
}

/// 메타 형식 버전(`"format": 1`).
pub(crate) const FORMAT: i64 = 1;
/// 기본 원격 저장소(소스 트리 밖에서 실행할 때).
pub(crate) const DEFAULT_REMOTE: &str =
    "https://raw.githubusercontent.com/SosomLab/nexa-sql/main/extensions";
/// 소스 트리의 `extensions/` 폴더(컴파일 시점 경로 · 있을 때만).
const SOURCE_TREE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../extensions");

/// 확장 종류 — `builtin`(앱에 컴파일됨 · 켜기/끄기만) · `data`(파일 배치) · `wasm`/`process`(T-118 · 아직 설치 불가).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Builtin,
    Data,
    Wasm,
    Process,
}

impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        match s {
            "builtin" => Some(Kind::Builtin),
            "data" => Some(Kind::Data),
            "wasm" => Some(Kind::Wasm),
            "process" => Some(Kind::Process),
            _ => None,
        }
    }
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::Builtin => "builtin",
            Kind::Data => "data",
            Kind::Wasm => "wasm",
            Kind::Process => "process",
        }
    }
}

/// 루트 `index.json`의 패키지 한 줄(폴더 식별용 · 상세는 폴더의 `extension.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    pub(crate) id: String,
    pub(crate) dir: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) kind: Kind,
    pub(crate) summary: String,
}

/// 루트 메타.
#[derive(Clone, Debug)]
pub(crate) struct Index {
    pub(crate) name: String,
    pub(crate) packages: Vec<Summary>,
}

/// 설치 파일 하나 — `path` = 패키지 폴더 안 상대 경로 · `sha256` = 소문자 16진 · `dest` = 설정 폴더 기준 배치 경로(비면 보관만).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileSpec {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) dest: String,
}

/// 패키지 메타(`extension.json`).
#[derive(Clone, Debug)]
pub(crate) struct Meta {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) kind: Kind,
    pub(crate) summary: String,
    pub(crate) description: String,
    pub(crate) author: String,
    pub(crate) license: String,
    pub(crate) homepage: String,
    pub(crate) min_app: String,
    pub(crate) platforms: Vec<String>,
    pub(crate) requires: Vec<String>,
    pub(crate) settings_prefix: String,
    pub(crate) files: Vec<FileSpec>,
    pub(crate) message_install: String,
}

/// 저장소 원천 — 폴더(로컬) 또는 URL(원격 · `curl`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Dir(PathBuf),
    Url(String),
}

impl Source {
    /// 문자열 → 원천. GitHub 주소는 raw 주소로 바꾼다(사용자 09-17 "기본 저장소 = github.com/SosomLab/nexa-sql/extensions"):
    /// `github.com/O/R/tree/B/P` → `raw.githubusercontent.com/O/R/B/P` · `github.com/O/R/P`(tree 없음) → 브랜치 `main`.
    pub(crate) fn parse(s: &str) -> Source {
        let s = s.trim().trim_end_matches('/');
        if let Some(rest) = s
            .strip_prefix("https://github.com/")
            .or_else(|| s.strip_prefix("http://github.com/"))
        {
            let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() >= 2 {
                let (owner, repo) = (parts[0], parts[1]);
                let (branch, path) = match parts.get(2) {
                    Some(&"tree") | Some(&"blob") if parts.len() >= 4 => {
                        (parts[3], parts[4..].join("/"))
                    }
                    Some(_) => ("main", parts[2..].join("/")),
                    None => ("main", String::new()),
                };
                let mut url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{branch}");
                if !path.is_empty() {
                    url.push('/');
                    url.push_str(&path);
                }
                return Source::Url(url);
            }
        }
        if s.starts_with("http://") || s.starts_with("https://") {
            Source::Url(s.to_string())
        } else {
            Source::Dir(PathBuf::from(s))
        }
    }

    pub(crate) fn display(&self) -> String {
        match self {
            Source::Dir(p) => nexa_fs::path::display(p),
            Source::Url(u) => u.clone(),
        }
    }

    /// 루트 기준 상대 경로의 바이트를 읽는다(추적 없이).
    pub(crate) fn read(&self, rel: &str) -> Result<Vec<u8>, String> {
        self.read_traced(rel, &mut Trace::default())
    }

    /// 읽기 + 추적 줄(원격 = `GET <url> → n B · ms · 속도` · 로컬 = `READ <path> → n B`).
    pub(crate) fn read_traced(&self, rel: &str, tr: &mut Trace) -> Result<Vec<u8>, String> {
        let t0 = Instant::now();
        let r = self.read_raw(rel);
        match &r {
            Ok(b) => tr.push(format!(
                "{} {} → {} · {} ms{}",
                if matches!(self, Source::Url(_)) {
                    "GET"
                } else {
                    "READ"
                },
                self.rel_display(rel),
                nsql_core::fmt_bytes(b.len() as u64),
                t0.elapsed().as_millis(),
                speed(b.len(), t0.elapsed())
            )),
            Err(e) => tr.push(format!("FAIL {} — {e}", self.rel_display(rel))),
        }
        r
    }

    fn rel_display(&self, rel: &str) -> String {
        match self {
            Source::Dir(p) => nexa_fs::path::display(&p.join(rel)),
            Source::Url(u) => format!("{u}/{rel}"),
        }
    }

    fn read_raw(&self, rel: &str) -> Result<Vec<u8>, String> {
        match self {
            Source::Dir(p) => std::fs::read(p.join(rel)).map_err(|e| format!("{}: {e}", rel)),
            Source::Url(u) => {
                let url = format!("{u}/{rel}");
                let out = std::process::Command::new("curl")
                    .args(["-fsSL", "--max-time", "30", &url])
                    .output()
                    .map_err(|e| format!("curl: {e}"))?;
                if !out.status.success() {
                    let err = String::from_utf8_lossy(&out.stderr);
                    return Err(format!("{url}: {}", err.trim()));
                }
                Ok(out.stdout)
            }
        }
    }

    fn read_text(&self, rel: &str, tr: &mut Trace) -> Result<String, String> {
        String::from_utf8(self.read_traced(rel, tr)?).map_err(|_| format!("{rel}: not UTF-8"))
    }
}

/// 기본 저장소 — 설정 `extensions.default_repository`(비면 GitHub raw). 값이 공식 URL 그대로이고 **소스 트리
/// `extensions/`가 있으면 그 폴더**(개발 중 네트워크 0 · push 전에도 같은 메타). 폴더 경로면 그 폴더.
pub(crate) fn default_source(s: &Settings) -> Source {
    let conf = s
        .get("extensions.default_repository")
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_REMOTE);
    let local = PathBuf::from(SOURCE_TREE_DIR);
    if conf == DEFAULT_REMOTE && local.join("index.json").is_file() {
        return Source::Dir(local.canonicalize().unwrap_or(local));
    }
    Source::parse(conf)
}

/// 설정 `extensions.repositories`(쉼표 구분) — 사용자가 더한 것만.
pub(crate) fn user_sources(s: &Settings) -> Vec<Source> {
    s.get("extensions.repositories")
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(Source::parse)
        .collect()
}

/// 모든 저장소(기본 + 사용자 · 중복 제거).
pub(crate) fn sources(s: &Settings) -> Vec<Source> {
    let mut out = vec![default_source(s)];
    for u in user_sources(s) {
        if !out.contains(&u) {
            out.push(u);
        }
    }
    out
}

fn field<'a>(obj: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    obj.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(obj: &[(String, Json)], key: &str) -> String {
    match field(obj, key) {
        Some(Json::Str(s)) => s.clone(),
        Some(Json::Num(n)) => n.to_string(),
        Some(Json::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn list(obj: &[(String, Json)], key: &str) -> Vec<String> {
    match field(obj, key) {
        Some(Json::Arr(items)) => items
            .iter()
            .filter_map(|v| match v {
                Json::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn object(v: &Json) -> Result<&[(String, Json)], String> {
    match v {
        Json::Obj(o) => Ok(o),
        _ => Err("root is not an object".into()),
    }
}

fn check_format(obj: &[(String, Json)]) -> Result<(), String> {
    match field(obj, "format") {
        Some(Json::Num(n)) if *n as i64 == FORMAT => Ok(()),
        other => Err(format!("unsupported format {other:?} (expected {FORMAT})")),
    }
}

/// `index.json` 파싱.
pub(crate) fn parse_index(text_in: &str) -> Result<Index, String> {
    let v = json::parse(text_in)?;
    let obj = object(&v)?;
    check_format(obj)?;
    let mut packages = Vec::new();
    if let Some(Json::Arr(items)) = field(obj, "packages") {
        for it in items {
            let o = object(it)?;
            let id = text(o, "id");
            if id.is_empty() {
                return Err("package without id".into());
            }
            let dir = {
                let d = text(o, "dir");
                if d.is_empty() {
                    id.clone()
                } else {
                    d
                }
            };
            if dir.contains("..") || dir.contains('/') || dir.contains('\\') {
                return Err(format!("{id}: bad dir {dir:?}"));
            }
            let kind = Kind::parse(&text(o, "kind")).ok_or_else(|| format!("{id}: bad kind"))?;
            packages.push(Summary {
                id,
                dir,
                name: text(o, "name"),
                version: text(o, "version"),
                kind,
                summary: text(o, "summary"),
            });
        }
    }
    Ok(Index {
        name: text(obj, "name"),
        packages,
    })
}

/// `extension.json` 파싱.
pub(crate) fn parse_meta(text_in: &str) -> Result<Meta, String> {
    let v = json::parse(text_in)?;
    let obj = object(&v)?;
    check_format(obj)?;
    let id = text(obj, "id");
    if id.is_empty() {
        return Err("extension without id".into());
    }
    let kind = Kind::parse(&text(obj, "kind")).ok_or_else(|| format!("{id}: bad kind"))?;
    let mut files = Vec::new();
    if let Some(Json::Arr(items)) = field(obj, "files") {
        for it in items {
            let o = object(it)?;
            let path = text(o, "path");
            let dest = text(o, "dest");
            for p in [&path, &dest] {
                if p.contains("..") || p.starts_with('/') || p.starts_with('\\') || p.contains(':')
                {
                    return Err(format!("{id}: unsafe path {p:?}"));
                }
            }
            files.push(FileSpec {
                path,
                sha256: text(o, "sha256").to_ascii_lowercase(),
                dest,
            });
        }
    }
    let message_install = match field(obj, "messages") {
        Some(Json::Obj(m)) => text(m, "install"),
        _ => String::new(),
    };
    Ok(Meta {
        id,
        name: text(obj, "name"),
        version: text(obj, "version"),
        kind,
        summary: text(obj, "summary"),
        description: text(obj, "description"),
        author: text(obj, "author"),
        license: text(obj, "license"),
        homepage: text(obj, "homepage"),
        min_app: text(obj, "min_app"),
        platforms: list(obj, "platforms"),
        requires: list(obj, "requires"),
        settings_prefix: text(obj, "settings_prefix"),
        files,
        message_install,
    })
}

pub(crate) fn fetch_index(src: &Source) -> Result<Index, String> {
    fetch_index_traced(src, &mut Trace::default())
}

pub(crate) fn fetch_index_traced(src: &Source, tr: &mut Trace) -> Result<Index, String> {
    let idx = parse_index(&src.read_text("index.json", tr)?)?;
    tr.push(format!(
        "index {:?} · {} package(s)",
        idx.name,
        idx.packages.len()
    ));
    Ok(idx)
}

pub(crate) fn fetch_meta(src: &Source, dir: &str, tr: &mut Trace) -> Result<Meta, String> {
    parse_meta(&src.read_text(&format!("{dir}/extension.json"), tr)?)
}

/// 이 OS의 플랫폼 이름(`platforms` 비교용).
fn platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// 설치 루트 `<설정 폴더>/extensions`.
pub(crate) fn root_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("extensions"))
}

/// 설치된 확장 한 줄(`installed.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) kind: Kind,
    /// 설정 폴더에 배치한 파일(`dest`) — 삭제 때 되감기.
    pub(crate) placed: Vec<String>,
}

fn installed_path(root: &Path, id: &str) -> PathBuf {
    root.join(id).join("installed.json")
}

fn write_installed(root: &Path, rec: &Installed) -> Result<(), String> {
    let dir = root.join(&rec.id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let placed: Vec<String> = rec.placed.iter().map(|p| json_str(p)).collect();
    let text = format!(
        "{{\n  \"format\": {FORMAT},\n  \"id\": {},\n  \"name\": {},\n  \"version\": {},\n  \"kind\": \"{}\",\n  \"placed\": [{}]\n}}\n",
        json_str(&rec.id),
        json_str(&rec.name),
        json_str(&rec.version),
        rec.kind.as_str(),
        placed.join(", ")
    );
    std::fs::write(installed_path(root, &rec.id), text).map_err(|e| e.to_string())
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn parse_installed(text_in: &str) -> Result<Installed, String> {
    let v = json::parse(text_in)?;
    let obj = object(&v)?;
    let kind = Kind::parse(&text(obj, "kind")).ok_or("bad kind")?;
    Ok(Installed {
        id: text(obj, "id"),
        name: text(obj, "name"),
        version: text(obj, "version"),
        kind,
        placed: list(obj, "placed"),
    })
}

/// 설치된 확장 목록(파일 기록 기준 · builtin은 호스트 레지스트리가 따로 안다).
pub(crate) fn installed_in(root: &Path) -> Vec<Installed> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Installed> = rd
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("installed.json")).ok())
        .filter_map(|t| parse_installed(&t).ok())
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub(crate) fn installed() -> Vec<Installed> {
    root_dir().map(|r| installed_in(&r)).unwrap_or_default()
}

/// 설치 — 메타를 읽고 파일마다 sha256을 확인해 `<root>/<id>/<version>/`에 보관하고 `dest`에 배치한다.
/// `config` = 설정 폴더(`dest` 기준) · `root` = 설치 루트. 반환 = 메타(안내문 표시용).
pub(crate) fn install_into(
    src: &Source,
    sum: &Summary,
    root: &Path,
    config: &Path,
) -> Result<Meta, String> {
    install_traced(src, sum, root, config, &mut Trace::default())
}

/// 설치 + 추적(다운로드마다 바이트·시간·속도 · 검증 · 보관/배치 경로 · 기록 파일).
pub(crate) fn install_traced(
    src: &Source,
    sum: &Summary,
    root: &Path,
    config: &Path,
    tr: &mut Trace,
) -> Result<Meta, String> {
    let t0 = Instant::now();
    tr.push(format!(
        "install {} {} [{}] from {}",
        sum.id,
        sum.version,
        sum.kind.as_str(),
        src.display()
    ));
    let meta = fetch_meta(src, &sum.dir, tr)?;
    if meta.id != sum.id {
        return Err(format!(
            "index says {} but package says {}",
            sum.id, meta.id
        ));
    }
    if !meta.platforms.is_empty() && !meta.platforms.iter().any(|p| p == platform()) {
        return Err(format!("{}: not for {}", meta.id, platform()));
    }
    if matches!(meta.kind, Kind::Wasm | Kind::Process) {
        return Err(format!(
            "{}: kind {} is not installable yet (T-118)",
            meta.id,
            meta.kind.as_str()
        ));
    }
    let keep = root.join(&meta.id).join(&meta.version);
    tr.push(format!(
        "store dir {} · place root {}",
        nexa_fs::path::display(&keep),
        nexa_fs::path::display(config)
    ));
    let mut placed = Vec::new();
    let mut total = 0usize;
    for f in &meta.files {
        let bytes = src.read_traced(&format!("{}/{}", sum.dir, f.path), tr)?;
        total += bytes.len();
        let got = super::sha256::hex(&bytes);
        if got != f.sha256 {
            tr.push(format!(
                "verify {} — sha256 MISMATCH {got} ≠ {}",
                f.path, f.sha256
            ));
            return Err(format!(
                "{}: sha256 mismatch ({} ≠ {})",
                f.path, got, f.sha256
            ));
        }
        tr.push(format!("verify {} — sha256 ok ({}…)", f.path, &got[..12]));
        let kept = keep.join(&f.path);
        if let Some(d) = kept.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&kept, &bytes).map_err(|e| e.to_string())?;
        tr.push(format!("store → {}", nexa_fs::path::display(&kept)));
        if !f.dest.is_empty() {
            let target = config.join(&f.dest);
            if let Some(d) = target.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
            tr.push(format!("place → {}", nexa_fs::path::display(&target)));
            placed.push(f.dest.clone());
        }
    }
    // 메타 사본(오프라인 목록·재설치용).
    if let Ok(raw) = src.read(&format!("{}/extension.json", sum.dir)) {
        std::fs::create_dir_all(&keep).map_err(|e| e.to_string())?;
        let _ = std::fs::write(keep.join("extension.json"), raw);
        tr.push(format!(
            "meta copy → {}",
            nexa_fs::path::display(&keep.join("extension.json"))
        ));
    }
    write_installed(
        root,
        &Installed {
            id: meta.id.clone(),
            name: meta.name.clone(),
            version: meta.version.clone(),
            kind: meta.kind,
            placed,
        },
    )?;
    tr.push(format!(
        "record → {} · files {} · {} · {} ms{}",
        nexa_fs::path::display(&installed_path(root, &meta.id)),
        meta.files.len(),
        nsql_core::fmt_bytes(total as u64),
        t0.elapsed().as_millis(),
        speed(total, t0.elapsed())
    ));
    Ok(meta)
}

pub(crate) fn install(src: &Source, sum: &Summary, tr: &mut Trace) -> Result<Meta, String> {
    let root = root_dir().ok_or("no config folder")?;
    let config = config_dir().ok_or("no config folder")?;
    install_traced(src, sum, &root, &config, tr)
}

/// 삭제 — 배치한 파일을 되감고 `<root>/<id>` 폴더를 지운다.
pub(crate) fn remove_from(id: &str, root: &Path, config: &Path) -> Result<(), String> {
    remove_traced(id, root, config, &mut Trace::default())
}

pub(crate) fn remove_traced(
    id: &str,
    root: &Path,
    config: &Path,
    tr: &mut Trace,
) -> Result<(), String> {
    let rec = std::fs::read_to_string(installed_path(root, id))
        .ok()
        .and_then(|t| parse_installed(&t).ok())
        .ok_or_else(|| format!("{id}: not installed"))?;
    for d in &rec.placed {
        let p = config.join(d);
        let ok = std::fs::remove_file(&p).is_ok();
        tr.push(format!(
            "unplace {} {}",
            nexa_fs::path::display(&p),
            if ok { "✓" } else { "(missing)" }
        ));
    }
    let dir = root.join(id);
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    tr.push(format!("remove dir {}", nexa_fs::path::display(&dir)));
    Ok(())
}

pub(crate) fn remove(id: &str, tr: &mut Trace) -> Result<(), String> {
    let root = root_dir().ok_or("no config folder")?;
    let config = config_dir().ok_or("no config folder")?;
    remove_traced(id, &root, &config, tr)
}

/// 쉼표 목록 설정에 항목 넣기/빼기(`extensions.disabled` · `extensions.repositories`).
pub(crate) fn list_toggle(current: &str, item: &str, on: bool) -> String {
    let mut items: Vec<String> = current
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect();
    if on {
        if !items.iter().any(|x| x == item) {
            items.push(item.to_string());
        }
    } else {
        items.retain(|x| x != item);
    }
    items.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{ "format": 1, "name": "test", "packages": [
        { "id": "demo-data", "dir": "demo-data", "name": "Demo Data", "version": "1.0.0", "kind": "data", "summary": "a data package" },
        { "id": "rainbow-pairs", "name": "Rainbow Pairs", "version": "1.0.0", "kind": "builtin", "summary": "colors" } ] }"#;

    #[test]
    fn index_and_meta_parse() {
        let idx = parse_index(INDEX).expect("test");
        assert_eq!(idx.packages.len(), 2);
        assert_eq!(idx.packages[1].dir, "rainbow-pairs", "dir가 비면 id");
        assert!(parse_index(r#"{"format": 2, "packages": []}"#).is_err());
        assert!(parse_index(
            r#"{"format": 1, "packages": [{"id":"x","dir":"../y","kind":"data"}]}"#
        )
        .is_err());
        let meta = parse_meta(r#"{"format":1,"id":"demo-data","name":"Demo","version":"1.0.0","kind":"data",
            "platforms":["windows","macos","linux"],"files":[{"path":"a.txt","sha256":"x","dest":"Packages/Demo/a.txt"}],
            "messages":{"install":"hi"}}"#).expect("test");
        assert_eq!(meta.files[0].dest, "Packages/Demo/a.txt");
        assert_eq!(meta.message_install, "hi");
        assert!(parse_meta(
            r#"{"format":1,"id":"x","kind":"data","files":[{"path":"/etc/passwd"}]}"#
        )
        .is_err());
    }

    /// 로컬 저장소 → 설치(sha256 검증 · 보관 + 배치 + installed.json) → 목록 → 삭제(되감기).
    #[test]
    fn install_and_remove_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("nexa-ext-{}", std::process::id()));
        let repo = tmp.join("repo");
        let root = tmp.join("root");
        let config = tmp.join("config");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(repo.join("demo-data")).expect("test");
        std::fs::write(repo.join("demo-data/a.txt"), b"hello").expect("test");
        let sha = super::super::sha256::hex(b"hello");
        std::fs::write(repo.join("index.json"), INDEX).expect("test");
        std::fs::write(
            repo.join("demo-data/extension.json"),
            format!(
                r#"{{"format":1,"id":"demo-data","name":"Demo Data","version":"1.0.0","kind":"data",
              "files":[{{"path":"a.txt","sha256":"{sha}","dest":"Packages/Demo/a.txt"}}]}}"#
            ),
        )
        .expect("test");
        let src = Source::Dir(repo.clone());
        let idx = fetch_index(&src).expect("test");
        let sum = idx
            .packages
            .iter()
            .find(|p| p.id == "demo-data")
            .expect("test");
        let meta = install_into(&src, sum, &root, &config).expect("test");
        assert_eq!(meta.name, "Demo Data");
        assert!(root.join("demo-data/1.0.0/a.txt").is_file());
        assert_eq!(
            std::fs::read(config.join("Packages/Demo/a.txt")).expect("test"),
            b"hello"
        );
        let list = installed_in(&root);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].placed, vec!["Packages/Demo/a.txt"]);
        // 체크섬 불일치 → 거부.
        std::fs::write(repo.join("demo-data/a.txt"), b"tampered").expect("test");
        match install_into(&src, sum, &root, &config) {
            Err(e) => assert!(e.contains("sha256")),
            Ok(_) => panic!("변조된 파일이 설치됨"),
        }
        remove_from("demo-data", &root, &config).expect("test");
        assert!(!config.join("Packages/Demo/a.txt").exists());
        assert!(installed_in(&root).is_empty());
        assert!(remove_from("demo-data", &root, &config).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn list_toggle_add_remove() {
        assert_eq!(list_toggle("", "a", true), "a");
        assert_eq!(list_toggle("a, b", "a", true), "a,b");
        assert_eq!(list_toggle("a,b", "a", false), "b");
        assert_eq!(
            Source::parse("https://x/y/"),
            Source::Url("https://x/y".into())
        );
        // GitHub 주소 → raw(사용자 표기 두 가지 + tree/브랜치).
        let raw = "https://raw.githubusercontent.com/SosomLab/nexa-sql/main/extensions";
        assert_eq!(
            Source::parse("https://github.com/SosomLab/nexa-sql/extensions"),
            Source::Url(raw.into())
        );
        assert_eq!(
            Source::parse("https://github.com/SosomLab/nexa-sql/tree/main/extensions/"),
            Source::Url(raw.into())
        );
        assert_eq!(
            Source::parse("https://github.com/SosomLab/nexa-sql/tree/dev/ext"),
            Source::Url("https://raw.githubusercontent.com/SosomLab/nexa-sql/dev/ext".into())
        );
        assert_eq!(Source::parse(raw), Source::Url(raw.into()));
    }
}
