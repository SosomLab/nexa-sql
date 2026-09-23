//! 프로젝트(워크스페이스) 모델(docs/67 §2 · T-165 P1 · 사용자 09-22) — `<이름>.nsql-project`(JSON · **어디에나** · VCS에 올림).
//!
//! ```json
//! { "version": 1, "folders": [ { "path": "sql" }, { "path": "D:/other" } ] }
//! ```
//! 폴더 경로는 프로젝트 파일이 있는 폴더 기준 **상대**(그 아래일 때) · 아니면 절대. 읽을 때는 파일 위치로 되돌린다.
//! 파서·작성기 = `nsql-settings::json`(외부 crate 0 · DR-3). 접속 정보는 담지 않는다(D-149 ①).

use nsql_settings::json::{dump as json_dump, parse, Json};
use std::path::{Path, PathBuf};

pub(crate) const EXT: &str = "nsql-project";
const VERSION: i64 = 1;

/// 열린 프로젝트 — `path` = 프로젝트 파일(없으면 "프로젝트 없음" = 기본 워크스페이스 · D-148).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Project {
    pub path: Option<PathBuf>,
    /// 탐색기 루트 폴더(절대 · 순서 = 표시 순서).
    pub folders: Vec<PathBuf>,
    /// 탐색기에서 마지막으로 고른 항목(절대 · 다시 열 때 그 자리에 선택 · 사용자 09-22).
    pub last_selected: Option<PathBuf>,
    /// ★ 작업 환경(사용자 09-23): 편집기 탭들(순서 · 파일 경로 또는 미저장 스크립트 본문 · 캐럿 + fuzzy 앵커) · 활성 탭 · 북마크(JSON).
    pub tabs: Vec<TabState>,
    pub active: usize,
    pub bookmarks: Option<String>,
    /// ★ 좌측 패널 상태(사용자 09-23 "각 좌측 기능별로 복원"): 프로젝트 탐색기에서 **펼친 폴더**(절대 · 저장은 상대) ·
    /// 보이던 좌측 패널(`project`/`bookmarks`/`search`/`ext`/`explorer`) · 파일 검색어 · 북마크 패널에서 접은 그룹 id ·
    /// 마지막 접속 프로필 이름(**표식만** — 복원 때 재접속하지 않는다 · docs/70 §2).
    pub expanded: Vec<PathBuf>,
    pub panel: Option<String>,
    pub search: Option<String>,
    pub bm_collapsed: Vec<u32>,
    pub profiles: Vec<String>,
}

/// 편집기 탭 하나의 저장 상태(docs/67 §2-4 · 사용자 09-23).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabState {
    /// 파일 탭이면 경로(파일 기준 상대로 저장) · 없으면 미저장 스크립트.
    pub path: Option<PathBuf>,
    pub title: String,
    /// 미저장 스크립트의 본문(파일 탭은 None — 원본을 만지지 않는다 · docs/70).
    pub text: Option<String>,
    /// 캐럿(0 기준) + 북마크와 같은 fuzzy 앵커(줄 본문 · 앞뒤 문맥 · 빈 값 = 없음).
    pub line: usize,
    pub col: usize,
    pub anchor_text: String,
    pub before: String,
    pub after: String,
    /// 미리보기 탭이었나(북마크·탐색기 한 번 클릭) — 복원도 미리보기로(사용자 09-23).
    pub preview: bool,
    /// 파일 탭의 미저장 본문을 담았을 때 그 순간의 **디스크 해시**(`backups::disk_hash`) — 로드 때 디스크가 바뀌었으면 알린다
    /// (사용자 09-23 "미저장 탭 본문을 프로젝트 파일에 탭별로 저장해 로드 때 자동 복구 · 백업은 백업용으로만").
    pub disk_hash: u64,
    /// 저장 당시의 탭 id(0 = 없음) — 이름 없는 탭의 북마크(`DocKey::Scratch { tab }`)를 복원 때 새 id로 **재매핑**(사용자 09-23 검토).
    pub id: u64,
}

impl Project {
    pub(crate) fn is_open(&self) -> bool {
        self.path.is_some()
    }

    /// 표시 이름 = 파일 이름에서 확장자를 뺀 것.
    pub(crate) fn name(&self) -> Option<String> {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
    }

    /// 프로젝트 파일이 있는 폴더(상대 경로 기준).
    fn base(&self) -> Option<PathBuf> {
        self.path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
    }

    /// 파일에서 읽는다(경로는 절대로 되돌린다 · 없는 폴더도 목록에는 남긴다 — 탐색기가 "없음"으로 보인다).
    pub(crate) fn load(path: &Path) -> Result<Project, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        // 헤더(JSON) + 탭 payload 블록(`projfile` · 09-23) — 옛 파일(마커 없음)은 블록 0 · 본문은 헤더의 `text`.
        let (text, blobs) = nsql_settings::projfile::split(&bytes)?;
        let json = parse(&text)?;
        let mut p = Project {
            path: Some(path.to_path_buf()),
            ..Project::default()
        };
        let base = p.base();
        let Json::Obj(fields) = json else {
            return Err("project file is not a JSON object".into());
        };
        for (k, v) in &fields {
            if k == "active" {
                if let Json::Num(n) = v {
                    p.active = (*n).max(0.0) as usize;
                }
                continue;
            }
            if k == "bookmarks" {
                if matches!(v, Json::Obj(_)) {
                    p.bookmarks = Some(json_dump(v));
                }
                continue;
            }
            if k == "tabs" {
                if let Json::Arr(items) = v {
                    for it in items {
                        let Json::Obj(f) = it else { continue };
                        let mut t = TabState::default();
                        for (fk, fv) in f {
                            match (fk.as_str(), fv) {
                                ("path", Json::Str(s)) if !s.trim().is_empty() => {
                                    t.path = Some(resolve(s, base.as_deref()))
                                }
                                ("title", Json::Str(s)) => t.title = s.clone(),
                                ("text", Json::Str(s)) => t.text = Some(s.clone()),
                                ("line", Json::Num(n)) => t.line = (*n).max(0.0) as usize,
                                ("col", Json::Num(n)) => t.col = (*n).max(0.0) as usize,
                                ("anchor", Json::Str(s)) => t.anchor_text = s.clone(),
                                ("before", Json::Str(s)) => t.before = s.clone(),
                                ("after", Json::Str(s)) => t.after = s.clone(),
                                ("preview", Json::Bool(b)) => t.preview = *b,
                                // 64비트 해시는 JSON 숫자(f64)로는 정밀도가 깨진다 → 글자열.
                                ("hash", Json::Str(s)) => t.disk_hash = s.parse().unwrap_or(0),
                                ("id", Json::Num(n)) => t.id = (*n).max(0.0) as u64,
                                _ => {}
                            }
                        }
                        p.tabs.push(t);
                    }
                }
                continue;
            }
            if k == "expanded" {
                if let Json::Arr(items) = v {
                    for it in items {
                        if let Json::Str(s) = it {
                            if !s.trim().is_empty() {
                                p.expanded.push(resolve(s, base.as_deref()));
                            }
                        }
                    }
                }
                continue;
            }
            if k == "panel" {
                if let Json::Str(s) = v {
                    if !s.trim().is_empty() {
                        p.panel = Some(s.clone());
                    }
                }
                continue;
            }
            if k == "search" {
                if let Json::Str(s) = v {
                    if !s.is_empty() {
                        p.search = Some(s.clone());
                    }
                }
                continue;
            }
            if k == "bm_collapsed" {
                if let Json::Arr(items) = v {
                    for it in items {
                        if let Json::Num(n) = it {
                            p.bm_collapsed.push((*n).max(0.0) as u32);
                        }
                    }
                }
                continue;
            }
            if k == "profiles" {
                if let Json::Arr(items) = v {
                    for it in items {
                        if let Json::Str(s) = it {
                            if !s.trim().is_empty() {
                                p.profiles.push(s.clone());
                            }
                        }
                    }
                }
                continue;
            }
            if k == "selected" {
                if let Json::Str(s) = v {
                    if !s.trim().is_empty() {
                        p.last_selected = Some(resolve(s, base.as_deref()));
                    }
                }
                continue;
            }
            if k == "folders" {
                if let Json::Arr(items) = v {
                    for it in items {
                        let rel = match it {
                            Json::Str(s) => Some(s.clone()),
                            Json::Obj(f) => f.iter().find_map(|(k, v)| match (k.as_str(), v) {
                                ("path", Json::Str(s)) => Some(s.clone()),
                                _ => None,
                            }),
                            _ => None,
                        };
                        if let Some(rel) = rel.filter(|s| !s.trim().is_empty()) {
                            p.folders.push(resolve(&rel, base.as_deref()));
                        }
                    }
                }
            }
        }
        // 탭 payload 블록 → 같은 `id`의 탭 본문(블록이 있으면 헤더의 옛 `text`보다 우선).
        for b in blobs {
            if b.tab == 0 {
                continue;
            }
            if let Some(t) = p.tabs.iter_mut().find(|t| t.id == b.tab) {
                t.text = Some(String::from_utf8_lossy(&b.data).into_owned());
            }
        }
        Ok(p)
    }

    /// JSON 본문(폴더 = 파일 위치 기준 상대 · 아니면 절대 · 구분자 `/`).
    pub(crate) fn to_json(&self) -> String {
        let base = self.base();
        let mut out = String::new();
        out.push_str("{\n  \"version\": ");
        out.push_str(&VERSION.to_string());
        out.push_str(",\n  \"folders\": [");
        for (i, f) in self.folders.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("\n    { \"path\": \"");
            out.push_str(&escape(&relativize(f, base.as_deref())));
            out.push_str("\" }");
        }
        if !self.folders.is_empty() {
            out.push('\n');
        }
        out.push_str("  ]");
        if let Some(sel) = &self.last_selected {
            out.push_str(",\n  \"selected\": \"");
            out.push_str(&escape(&relativize(sel, base.as_deref())));
            out.push('"');
        }
        // 작업 환경(사용자 09-23): 탭 순서대로 · 파일은 경로만(원본 본문은 쓰지 않는다 · docs/70) · 스크립트는 본문 · 캐럿 앵커.
        if !self.tabs.is_empty() {
            out.push_str(&format!(",\n  \"active\": {}", self.active));
            out.push_str(",\n  \"tabs\": [");
            for (i, t) in self.tabs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("\n    { ");
                if let Some(p) = &t.path {
                    out.push_str(&format!(
                        "\"path\": \"{}\", ",
                        escape(&relativize(p, base.as_deref()))
                    ));
                }
                out.push_str(&format!(
                    "\"title\": \"{}\", \"line\": {}, \"col\": {}",
                    escape(&t.title),
                    t.line,
                    t.col
                ));
                if !t.anchor_text.is_empty() {
                    out.push_str(&format!(
                        ", \"anchor\": \"{}\", \"before\": \"{}\", \"after\": \"{}\"",
                        escape(&t.anchor_text),
                        escape(&t.before),
                        escape(&t.after)
                    ));
                }
                // 본문(`text`)은 헤더에 넣지 않는다 — 마커 아래 탭별 payload 블록(`projfile` · 사용자 09-23 "메타는 텍스트 · payload는 바이너리").
                if t.text.is_some() && t.id != 0 {
                    out.push_str(", \"blob\": true");
                }
                if t.preview {
                    out.push_str(", \"preview\": true");
                }
                if t.disk_hash != 0 {
                    out.push_str(&format!(", \"hash\": \"{}\"", t.disk_hash));
                }
                if t.id != 0 {
                    out.push_str(&format!(", \"id\": {}", t.id));
                }
                out.push_str(" }");
            }
            out.push_str("\n  ]");
        }
        // 좌측 패널 상태(사용자 09-23) — 비어 있으면 키를 쓰지 않는다(파일은 짧게 · 옛 파일과 호환).
        if !self.expanded.is_empty() {
            out.push_str(",\n  \"expanded\": [");
            for (i, d) in self.expanded.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("\"{}\"", escape(&relativize(d, base.as_deref()))));
            }
            out.push(']');
        }
        if let Some(p) = self.panel.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!(",\n  \"panel\": \"{}\"", escape(p)));
        }
        if let Some(s) = self.search.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!(",\n  \"search\": \"{}\"", escape(s)));
        }
        if !self.bm_collapsed.is_empty() {
            let ids: Vec<String> = self.bm_collapsed.iter().map(u32::to_string).collect();
            out.push_str(&format!(",\n  \"bm_collapsed\": [{}]", ids.join(", ")));
        }
        if !self.profiles.is_empty() {
            let names: Vec<String> = self
                .profiles
                .iter()
                .map(|n| format!("\"{}\"", escape(n)))
                .collect();
            out.push_str(&format!(",\n  \"profiles\": [{}]", names.join(", ")));
        }
        if let Some(b) = &self.bookmarks {
            out.push_str(",\n  \"bookmarks\": ");
            out.push_str(b.trim());
        }
        out.push_str("\n}\n");
        out
    }

    /// 파일에 쓴다(`path`가 없으면 오류).
    pub(crate) fn save(&self) -> Result<(), String> {
        let Some(p) = &self.path else {
            return Err("no project path".into());
        };
        std::fs::write(p, self.to_document()).map_err(|e| e.to_string())
    }

    /// 파일 전체 = JSON 헤더(`to_json`) + `%%NSQL-BLOBS%%` + 탭별 payload 블록(본문이 있는 탭 · `tab=<id> len=<n>` + 원문 바이트 ·
    /// [`nsql_settings::projfile`] · 사용자 09-23). 바뀜 비교(`project_last_json`)도 이것으로.
    pub(crate) fn to_document(&self) -> Vec<u8> {
        let blobs: Vec<nsql_settings::projfile::Blob> = self
            .tabs
            .iter()
            .filter_map(|t| {
                t.text.as_ref().map(|txt| nsql_settings::projfile::Blob {
                    tab: t.id,
                    data: txt.as_bytes().to_vec(),
                })
            })
            .collect();
        nsql_settings::projfile::join(&self.to_json(), &blobs)
    }

    /// 다른 이름으로 — 경로만 바꾼다(상대 경로는 저장 때 새 위치 기준으로 다시 계산).
    pub(crate) fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }

    /// 폴더 추가(중복 무시) — 추가됐으면 true.
    pub(crate) fn add_folder(&mut self, dir: &Path) -> bool {
        if self.folders.iter().any(|f| f == dir) {
            return false;
        }
        self.folders.push(dir.to_path_buf());
        true
    }

    pub(crate) fn remove_folder(&mut self, i: usize) -> Option<PathBuf> {
        (i < self.folders.len()).then(|| self.folders.remove(i))
    }
}

/// ★ 작업 모드 셋(사용자 09-23): **파일 모드**(인자 없음 · 로컬 상태 = 전역 설정 폴더 `%APPDATA%`) · **폴더 모드**(`nexa-sql .` ·
/// `nexa-sql <폴더>` · 로컬 상태 = `<폴더>/.nsql/`) · **프로젝트 모드**(`.nsql-project` · 상태는 프로젝트 파일 안).
/// 지금 모드가 나누는 것은 **북마크**뿐 — 설정 오버라이드·최근 파일·되돌리기 기록 같은 다른 로컬 상태도 앞으로는 [`WorkMode::local_dir`]
/// 한 자리에서 나눈다(67 §6). 프로젝트 모드는 `local_dir = None`(파일 안에 담는다 · 기기별 오버라이드는 73 §4-2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkMode {
    File,
    Folder(PathBuf),
    Project(PathBuf),
}

impl WorkMode {
    /// 폴더 모드의 로컬 상태 폴더 이름.
    pub(crate) const LOCAL_DIR: &'static str = ".nsql";

    /// 우선순위 = 프로젝트 > 폴더 > 파일.
    pub(crate) fn of(project: Option<&Path>, folder: Option<&Path>) -> WorkMode {
        match (project, folder) {
            (Some(p), _) => WorkMode::Project(p.to_path_buf()),
            (None, Some(d)) => WorkMode::Folder(d.to_path_buf()),
            (None, None) => WorkMode::File,
        }
    }

    /// 로컬 상태를 두는 폴더 — 파일 모드 = 전역 설정 폴더 · 폴더 모드 = `<폴더>/.nsql` · 프로젝트 모드 = None.
    pub(crate) fn local_dir(&self) -> Option<PathBuf> {
        match self {
            WorkMode::File => nsql_settings::config_dir(),
            WorkMode::Folder(d) => Some(d.join(Self::LOCAL_DIR)),
            WorkMode::Project(_) => None,
        }
    }
}

/// 상대 경로 → 절대(`base` 아래) · 이미 절대면 그대로.
fn resolve(rel: &str, base: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(rel);
    if p.is_absolute() {
        return p;
    }
    match base {
        Some(b) => b.join(p),
        None => p,
    }
}

/// 절대 → `base` 아래면 상대(`/` 구분) · 아니면 절대(`/` 구분).
fn relativize(p: &Path, base: Option<&Path>) -> String {
    let s = match base.and_then(|b| p.strip_prefix(b).ok()) {
        Some(r) if !r.as_os_str().is_empty() => r.to_string_lossy().into_owned(),
        Some(_) => ".".into(),
        None => p.to_string_lossy().into_owned(),
    };
    s.replace('\\', "/")
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 최근 프로젝트 목록(설정 `project.recent` · `|` 구분 · 최신 먼저 · 존재하는 파일만 · 최대 10).
pub(crate) fn recent_list(raw: &str) -> Vec<PathBuf> {
    raw.split('|')
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .take(10)
        .collect()
}

/// 최근 목록 앞에 넣는다(중복 제거 · 10개).
pub(crate) fn push_recent(raw: &str, path: &Path) -> String {
    let mut v = recent_list(raw);
    v.retain(|p| p != path);
    v.insert(0, path.to_path_buf());
    v.truncate(10);
    v.iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// 작업 모드 셋(사용자 09-23): 프로젝트 > 폴더 > 파일 · 로컬 상태 폴더 = 전역 / `<폴더>/.nsql` / 없음.
    #[test]
    fn work_mode_priority_and_local_dir() {
        let pj = Path::new("D:/w/x.nsql-project");
        let dir = Path::new("D:/w");
        assert_eq!(
            WorkMode::of(Some(pj), Some(dir)),
            WorkMode::Project(pj.to_path_buf())
        );
        assert_eq!(
            WorkMode::of(None, Some(dir)),
            WorkMode::Folder(dir.to_path_buf())
        );
        assert_eq!(WorkMode::of(None, None), WorkMode::File);
        assert_eq!(
            WorkMode::Folder(dir.to_path_buf()).local_dir(),
            Some(dir.join(".nsql"))
        );
        assert_eq!(WorkMode::Project(pj.to_path_buf()).local_dir(), None);
        assert_eq!(WorkMode::File.local_dir(), nsql_settings::config_dir());
    }

    #[test]
    fn json_round_trip_keeps_folders_relative_under_base() {
        let dir = std::env::temp_dir().join(format!("nsql-proj-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sql"));
        let file = dir.join("my.nsql-project");
        let mut p = Project {
            path: Some(file.clone()),
            ..Project::default()
        };
        assert!(p.add_folder(&dir.join("sql")));
        assert!(!p.add_folder(&dir.join("sql")));
        let other = if cfg!(windows) {
            PathBuf::from("Q:\\elsewhere")
        } else {
            PathBuf::from("/elsewhere")
        };
        p.add_folder(&other);
        // 마지막 선택 위치도 상대 경로로 저장·복원(사용자 09-22).
        p.last_selected = Some(dir.join("sql/a.sql"));
        let text = p.to_json();
        assert!(text.contains("\"path\": \"sql\""), "{text}");
        assert!(text.contains("\"selected\": \"sql/a.sql\""), "{text}");
        assert!(!text.contains('\\'), "slashes only: {text}");
        p.save().unwrap();
        let back = Project::load(&file).unwrap();
        assert_eq!(back.folders[0], dir.join("sql"));
        assert_eq!(back.folders[1], other);
        assert_eq!(back.name().as_deref(), Some("my"));
        assert_eq!(back.last_selected, Some(dir.join("sql/a.sql")));
        // 작업 환경(탭 · 활성 · 북마크 JSON) 왕복(사용자 09-23).
        p.tabs = vec![
            TabState {
                path: Some(dir.join("sql/a.sql")),
                title: "a.sql".into(),
                line: 3,
                col: 2,
                anchor_text: "SELECT 1;".into(),
                before: "-- a".into(),
                after: "".into(),
                ..TabState::default()
            },
            TabState {
                title: "Script_2".into(),
                text: Some("select \"q\"\n from dual".into()),
                ..TabState::default()
            },
        ];
        p.active = 1;
        p.bookmarks = Some("{\"version\": 1, \"items\": []}".into());
        // 좌측 패널 상태 + 미리보기 표식(사용자 09-23) — 상대 경로로 쓰고 절대로 돌아온다.
        p.tabs[0].preview = true;
        // 파일 탭의 미저장 본문 + 디스크 해시(64비트 → 글자열로 왕복) + 탭 id(사용자 09-23).
        p.tabs[0].text = Some("-- unsaved edit".into());
        p.tabs[0].disk_hash = 0xFFFF_FFFF_FFFF_FF01;
        p.tabs[0].id = 77;
        // 본문(payload)은 `id`로 탭에 되붙는다(`projfile` 블록 · 09-23) — 스크립트 탭도 id가 있어야 본문이 왕복한다.
        p.tabs[1].id = 78;
        p.expanded = vec![dir.join("sql"), other.join("x")];
        p.panel = Some("bookmarks".into());
        p.search = Some("select \"q\"".into());
        p.bm_collapsed = vec![0, 7];
        p.profiles = vec!["BISCM".into(), "Local".into()];
        p.save().unwrap();
        let text = p.to_json();
        assert!(text.contains("\"preview\": true"), "{text}");
        assert!(text.contains("\"expanded\": [\"sql\", "), "{text}");
        let back = Project::load(&file).unwrap();
        assert_eq!(back.tabs, p.tabs);
        assert_eq!(back.active, 1);
        assert_eq!(back.expanded, p.expanded);
        assert_eq!(back.panel.as_deref(), Some("bookmarks"));
        assert_eq!(back.search.as_deref(), Some("select \"q\""));
        assert_eq!(back.bm_collapsed, vec![0, 7]);
        assert_eq!(back.profiles, p.profiles);
        assert!(back
            .bookmarks
            .as_deref()
            .is_some_and(|b| b.contains("\"version\":1")));
        assert!(p.remove_folder(5).is_none());
        assert_eq!(p.remove_folder(0), Some(dir.join("sql")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_non_object_and_accepts_plain_strings() {
        let dir = std::env::temp_dir().join(format!("nsql-proj2-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let bad = dir.join("bad.nsql-project");
        std::fs::write(&bad, "[1,2]").unwrap();
        assert!(Project::load(&bad).is_err());
        let ok = dir.join("ok.nsql-project");
        std::fs::write(
            &ok,
            "{\"version\":1,\"folders\":[\"a\", {\"path\":\"b\"}, 3]}",
        )
        .unwrap();
        let p = Project::load(&ok).unwrap();
        assert_eq!(p.folders, vec![dir.join("a"), dir.join("b")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_list_rules() {
        let dir = std::env::temp_dir().join(format!("nsql-proj3-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.nsql-project");
        let b = dir.join("b.nsql-project");
        std::fs::write(&a, "{}").unwrap();
        std::fs::write(&b, "{}").unwrap();
        let raw = push_recent("", &a);
        let raw = push_recent(&raw, &b);
        let raw = push_recent(&raw, &a);
        let v = recent_list(&raw);
        assert_eq!(v, vec![a.clone(), b.clone()]);
        // 없는 파일은 걸러진다.
        let raw2 = format!("{}|{}", dir.join("gone.nsql-project").display(), raw);
        assert_eq!(recent_list(&raw2), vec![a, b]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
