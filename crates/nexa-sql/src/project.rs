//! 프로젝트(워크스페이스) 모델(docs/67 §2 · T-165 P1 · 사용자 09-22) — `<이름>.nsql-project`(JSON · **어디에나** · VCS에 올림).
//!
//! ```json
//! { "version": 1, "folders": [ { "path": "sql" }, { "path": "D:/other" } ] }
//! ```
//! 폴더 경로는 프로젝트 파일이 있는 폴더 기준 **상대**(그 아래일 때) · 아니면 절대. 읽을 때는 파일 위치로 되돌린다.
//! 파서·작성기 = `nsql-settings::json`(외부 crate 0 · DR-3). 접속 정보는 담지 않는다(D-149 ①).

use nsql_settings::json::{parse, Json};
use std::path::{Path, PathBuf};

pub(crate) const EXT: &str = "nsql-project";
const VERSION: i64 = 1;

/// 열린 프로젝트 — `path` = 프로젝트 파일(없으면 "프로젝트 없음" = 기본 워크스페이스 · D-148).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Project {
    pub path: Option<PathBuf>,
    /// 탐색기 루트 폴더(절대 · 순서 = 표시 순서).
    pub folders: Vec<PathBuf>,
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
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let json = parse(&text)?;
        let mut p = Project {
            path: Some(path.to_path_buf()),
            folders: Vec::new(),
        };
        let base = p.base();
        let Json::Obj(fields) = json else {
            return Err("project file is not a JSON object".into());
        };
        for (k, v) in &fields {
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
        out.push_str("  ]\n}\n");
        out
    }

    /// 파일에 쓴다(`path`가 없으면 오류).
    pub(crate) fn save(&self) -> Result<(), String> {
        let Some(p) = &self.path else {
            return Err("no project path".into());
        };
        std::fs::write(p, self.to_json()).map_err(|e| e.to_string())
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

    #[test]
    fn json_round_trip_keeps_folders_relative_under_base() {
        let dir = std::env::temp_dir().join(format!("nsql-proj-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sql"));
        let file = dir.join("my.nsql-project");
        let mut p = Project {
            path: Some(file.clone()),
            folders: Vec::new(),
        };
        assert!(p.add_folder(&dir.join("sql")));
        assert!(!p.add_folder(&dir.join("sql")));
        let other = if cfg!(windows) {
            PathBuf::from("Q:\\elsewhere")
        } else {
            PathBuf::from("/elsewhere")
        };
        p.add_folder(&other);
        let text = p.to_json();
        assert!(text.contains("\"path\": \"sql\""), "{text}");
        assert!(!text.contains('\\'), "slashes only: {text}");
        p.save().unwrap();
        let back = Project::load(&file).unwrap();
        assert_eq!(back.folders[0], dir.join("sql"));
        assert_eq!(back.folders[1], other);
        assert_eq!(back.name().as_deref(), Some("my"));
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
