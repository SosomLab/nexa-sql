//! `nsql bookmark list | add | rm | prune` — 북마크 CLI([docs/69 §6-1 · D-170](../../../docs/69-bookmarks.md) · T-167 B8).
//!
//! GUI와 **같은 워크스페이스 파일**을 읽고 쓴다(`NSQL_HOME/workspaces/<프로젝트 이름|default>.nsql-workspace` · `--project <파일>`로 고른다).
//! 코어(`nsql-bookmarks`)의 순수 함수만 부른다 — 본문은 파일에서 읽어 넣고, 시각은 시스템 시계.
//! 플래그는 위치 인자에 섞여 들어오므로(`main.rs` 파서는 모르는 `--x`를 positional로 넘긴다) 여기서 걷어 낸다.

use crate::Opts;
use nsql_bookmarks::{make_anchor, DocKey, RelocateOpts, State, Store};
use std::path::{Path, PathBuf};

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 위치 인자에서 `--키 값`/`--스위치`를 걷어 낸다 → (남은 위치 인자, 옵션들).
fn split_flags(args: &[String]) -> (Vec<String>, Vec<(String, Option<String>)>) {
    let mut pos = Vec::new();
    let mut flags = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(k) = a.strip_prefix("--") {
            let takes_value = matches!(k, "project" | "label" | "days" | "doc");
            if takes_value {
                flags.push((k.to_string(), args.get(i + 1).cloned()));
                i += 2;
                continue;
            }
            flags.push((k.to_string(), None));
        } else {
            pos.push(a.clone());
        }
        i += 1;
    }
    (pos, flags)
}

fn flag<'a>(flags: &'a [(String, Option<String>)], k: &str) -> Option<&'a str> {
    flags
        .iter()
        .find(|(n, _)| n == k)
        .and_then(|(_, v)| v.as_deref())
}

fn has(flags: &[(String, Option<String>)], k: &str) -> bool {
    flags.iter().any(|(n, _)| n == k)
}

/// 북마크의 원천 — 프로젝트 파일(`.nsql-project` · 안의 `bookmarks` 객체 · 09-23) 또는 워크스페이스 파일.
fn is_project_file(p: &str) -> bool {
    Path::new(p)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("nsql-project"))
}

/// 워크스페이스 파일 경로(GUI `bookmarks.rs workspace_path`와 같은 규칙).
fn workspace_path(project: Option<&str>) -> Option<PathBuf> {
    let dir = nsql_settings::config_dir()?.join("workspaces");
    let stem = project
        .map(Path::new)
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".into());
    Some(dir.join(format!("{stem}.nsql-workspace")))
}

fn load(path: &Path) -> Result<Store, String> {
    if is_project_file(&path.to_string_lossy()) {
        // 프로젝트 파일 안의 `bookmarks` 객체(없으면 빈 저장소).
        // 헤더(JSON) + 탭 payload 블록(`projfile` · 09-23) — 북마크는 헤더에 있다.
        let bytes = match std::fs::read(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err("project file not found".into())
            }
            Err(e) => return Err(e.to_string()),
        };
        let (text, _blobs) = nsql_settings::projfile::split(&bytes)?;
        let json = nsql_settings::json::parse(&text)?;
        let nsql_settings::json::Json::Obj(fields) = json else {
            return Err("project file is not a JSON object".into());
        };
        return match fields.iter().find(|(k, _)| k == "bookmarks") {
            Some((_, v)) => Store::from_json(&nsql_settings::json::dump(v)),
            None => Ok(Store::new()),
        };
    }
    match std::fs::read_to_string(path) {
        Ok(text) => Store::from_json(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::new()),
        Err(e) => Err(e.to_string()),
    }
}

fn save(path: &Path, st: &Store) -> Result<(), String> {
    if is_project_file(&path.to_string_lossy()) {
        // 프로젝트 파일의 다른 키는 그대로 두고 `bookmarks`만 바꿔 쓴다(GUI가 다음 저장 때 자기 형식으로 다시 쓴다).
        use nsql_settings::json::{dump, parse, Json};
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        // 헤더만 바꾸고 탭 payload 블록은 그대로 되붙인다(`projfile` · 09-23).
        let (text, blobs) = nsql_settings::projfile::split(&bytes)?;
        let Json::Obj(mut fields) = parse(&text)? else {
            return Err("project file is not a JSON object".into());
        };
        let bm = parse(&st.to_json())?;
        match fields.iter_mut().find(|(k, _)| k == "bookmarks") {
            Some(slot) => slot.1 = bm,
            None => fields.push(("bookmarks".into(), bm)),
        }
        let tmp = path.with_extension("nsql-project.tmp");
        let out = nsql_settings::projfile::join(&dump(&Json::Obj(fields)), &blobs);
        std::fs::write(&tmp, out).map_err(|e| e.to_string())?;
        return std::fs::rename(&tmp, path).map_err(|e| e.to_string());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("nsql-workspace.tmp");
    std::fs::write(&tmp, st.to_json()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn usage() -> i32 {
    eprintln!(
        "nsql bookmark list [--project <file>] [--doc <path>] [--md]\n\
         nsql bookmark add <file> <line> [--label <text>] [--project <file>]\n\
         nsql bookmark rm <id> [--project <file>]\n\
         nsql bookmark prune [--days N] [--project <file>]"
    );
    2
}

pub(crate) fn cmd_bookmark(o: &Opts) -> i32 {
    let (pos, flags) = split_flags(&o.positional);
    let Some(sub) = pos.first() else {
        return usage();
    };
    // `--project x.nsql-project` = 그 파일 안의 북마크 · 그 밖 = 워크스페이스 파일.
    let ws = match flag(&flags, "project") {
        Some(p) if is_project_file(p) => Some(PathBuf::from(p)),
        other => workspace_path(other),
    };
    let Some(ws) = ws else {
        eprintln!("사용자 설정 폴더를 알 수 없습니다(NSQL_HOME)");
        return 1;
    };
    let mut st = match load(&ws) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", ws.display());
            return 1;
        }
    };
    let ci = cfg!(any(windows, target_os = "macos"));
    match sub.as_str() {
        "list" | "ls" => {
            let only = flag(&flags, "doc").map(DocKey::file);
            let md = has(&flags, "md");
            let mut items: Vec<&nsql_bookmarks::Bookmark> = st
                .items
                .iter()
                .filter(|b| only.as_ref().is_none_or(|d| b.doc.same(d, ci)))
                .collect();
            items.sort_by(|a, b| {
                a.doc
                    .short_name()
                    .cmp(&b.doc.short_name())
                    .then(a.anchor.line.cmp(&b.anchor.line))
            });
            if md {
                println!("| id | document | line | text | state |");
                println!("|---:|---|---:|---|---|");
            } else {
                println!(
                    "{:>5}  {:<28} {:>6}  {:<40} state",
                    "id", "document", "line", "text"
                );
            }
            for b in &items {
                let state = match b.state {
                    State::Live => "live".to_string(),
                    State::Invalid { .. } => "invalid".to_string(),
                };
                let text = b.display();
                let text: String = text.chars().take(40).collect();
                let doc = b.doc.short_name();
                let mn = b.mnemonic.map(|m| format!(" [{m}]")).unwrap_or_default();
                if md {
                    println!(
                        "| {} | {} | {} | {}{} | {} |",
                        b.id,
                        doc,
                        b.anchor.line + 1,
                        text.replace('|', "\\|"),
                        mn,
                        state
                    );
                } else {
                    println!(
                        "{:>5}  {:<28} {:>6}  {:<40} {}{}",
                        b.id,
                        doc,
                        b.anchor.line + 1,
                        text,
                        state,
                        mn
                    );
                }
            }
            if !md {
                let (live, inv) = st.counts();
                eprintln!(
                    "{} bookmark(s) · {inv} invalid · {}",
                    live + inv,
                    ws.display()
                );
            }
            0
        }
        "add" => {
            let (Some(file), Some(line)) = (pos.get(1), pos.get(2)) else {
                return usage();
            };
            let Ok(line1) = line.parse::<usize>() else {
                eprintln!("line must be a number (1-based)");
                return 2;
            };
            // 정규화한 절대 경로(GUI가 여는 경로와 같은 표기 · Windows `\\?\` 접두는 뗀다).
            let path = std::fs::canonicalize(file)
                .map(|p| PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\")))
                .unwrap_or_else(|_| PathBuf::from(file));
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("{file}: {e}");
                    return 1;
                }
            };
            let lines: Vec<&str> = text.lines().collect();
            if line1 == 0 || line1 > lines.len() {
                eprintln!("line {line1} is out of range (1..={})", lines.len());
                return 2;
            }
            let opts = RelocateOpts::default();
            let anchor = make_anchor(&lines, line1 - 1, 0, &opts);
            let doc = DocKey::file(&path.to_string_lossy());
            match st.add(doc, anchor, now_epoch(), ci, 0, 0) {
                Ok(id) => {
                    if let Some(l) = flag(&flags, "label") {
                        if let Some(b) = st.get_mut(id) {
                            b.label = Some(l.to_string());
                        }
                    }
                    if let Err(e) = save(&ws, &st) {
                        eprintln!("{}: {e}", ws.display());
                        return 1;
                    }
                    println!("added #{id} at line {line1} of {}", path.display());
                    0
                }
                Err(e) => {
                    eprintln!("refused: {e}");
                    1
                }
            }
        }
        "rm" | "remove" => {
            let Some(id) = pos.get(1).and_then(|s| s.parse::<u64>().ok()) else {
                return usage();
            };
            if st.remove(id).is_none() {
                eprintln!("no bookmark #{id}");
                return 1;
            }
            if let Err(e) = save(&ws, &st) {
                eprintln!("{}: {e}", ws.display());
                return 1;
            }
            println!("removed #{id}");
            0
        }
        "prune" => {
            let days: u32 = flag(&flags, "days")
                .and_then(|d| d.parse().ok())
                .unwrap_or(30);
            let n = st.prune(now_epoch(), days);
            if n > 0 {
                if let Err(e) = save(&ws, &st) {
                    eprintln!("{}: {e}", ws.display());
                    return 1;
                }
            }
            println!("pruned {n} invalid bookmark(s) older than {days} day(s)");
            0
        }
        _ => usage(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn split_flags_separates_values_and_switches() {
        let args: Vec<String> = [
            "add",
            "a.sql",
            "3",
            "--label",
            "hello",
            "--md",
            "--project",
            "p.nsql-project",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (pos, flags) = split_flags(&args);
        assert_eq!(pos, vec!["add", "a.sql", "3"]);
        assert_eq!(flag(&flags, "label"), Some("hello"));
        assert_eq!(flag(&flags, "project"), Some("p.nsql-project"));
        assert!(has(&flags, "md"));
        assert!(!has(&flags, "days"));
    }

    #[test]
    fn workspace_path_uses_project_stem_or_default() {
        let d = workspace_path(Some("D:/x/my.nsql-project")).unwrap();
        assert!(
            d.ends_with("workspaces/my.nsql-workspace")
                || d.ends_with("workspaces\\my.nsql-workspace")
        );
        let d = workspace_path(None).unwrap();
        assert!(d.to_string_lossy().ends_with("default.nsql-workspace"));
    }
}
