//! 내장(intrinsic) 변수 — VS Code의 `${workspaceFolder}` 같은 **앱이 아는 값**을 스크립트·설정에서 같은 문법으로 쓴다(사용자 09-23).
//!
//! 계층(위가 먼저): `DEFINE`/`&` 치환 변수 → **이 내장 층** → OS 환경 변수(`${env:이름}`) → 글자 그대로.
//! `${env:NSQL_PROJECT_DIR}`처럼 **환경 변수 문법으로도** 같은 값이 나온다(호스트가 OS 환경 위에 이 층을 겹친다) —
//! 그래서 이미 있던 `${env:…}` 사용법을 아는 사람은 새로 배울 것이 없다.
//!
//! 이름은 VS Code와 같게(`workspaceFolder` · `file` · `fileBasename` · `userHome` · `cwd` · `execPath` · `pathSeparator` · `config:키` ·
//! `workspaceFolder:이름`) + Nexa 전용(`workspaceFile` · `workspaceName` · `nsqlHome` · `profile` · `dialect` · `os`) + 대문자 별칭(`NSQL_*`).
//! 값은 호스트(GUI/CLI)가 [`build`]로 만들어 [`crate::engine::Settings::intrinsic`]에 넣는다 — 이 크레이트는 파일 시스템·설정을 모른다.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 호스트가 아는 문맥(전부 선택 — 모르는 것은 변수를 만들지 않는다 = `${…}`가 글자 그대로 남는다).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// 프로젝트 파일(`.nsql-project`) — 있으면 `workspaceFolder` = 그 폴더.
    pub project_file: Option<PathBuf>,
    /// 등록 폴더(이름 · 절대 경로) — `workspaceFolder:이름`.
    pub folders: Vec<(String, PathBuf)>,
    /// 활성 편집기의 파일(미저장 스크립트면 None).
    pub file: Option<PathBuf>,
    /// 캐럿(1 기준).
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub user_home: Option<PathBuf>,
    /// 앱 설정 폴더(`NSQL_HOME`).
    pub app_home: Option<PathBuf>,
    pub exec: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    /// 활성 접속 프로필 이름 · 방언 id(`oracle`·`mssql`·…).
    pub profile: Option<String>,
    pub dialect: Option<String>,
    /// 설정 값(`config:키`) — 호스트가 원하는 만큼(전부 또는 경로형만).
    pub config: Vec<(String, String)>,
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn put(map: &mut BTreeMap<String, String>, names: &[&str], value: String) {
    for n in names {
        map.insert((*n).to_string(), value.clone());
    }
}

/// 문맥 → 이름/값 표. 이름은 대소문자를 **구분**한다(VS Code와 같음) · 대문자 별칭은 따로 넣는다.
#[must_use]
pub fn build(ctx: &Context) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    put(
        &mut m,
        &["pathSeparator", "/"],
        std::path::MAIN_SEPARATOR.to_string(),
    );
    put(&mut m, &["os", "NSQL_OS"], std::env::consts::OS.to_string());
    if let Some(pf) = &ctx.project_file {
        put(&mut m, &["workspaceFile", "NSQL_PROJECT_FILE"], s(pf));
        if let Some(dir) = pf.parent() {
            put(&mut m, &["workspaceFolder", "NSQL_PROJECT_DIR"], s(dir));
            if let Some(b) = dir.file_name() {
                put(
                    &mut m,
                    &["workspaceFolderBasename"],
                    b.to_string_lossy().into_owned(),
                );
            }
        }
        if let Some(stem) = pf.file_stem() {
            put(
                &mut m,
                &["workspaceName", "NSQL_PROJECT_NAME"],
                stem.to_string_lossy().into_owned(),
            );
        }
    }
    for (name, dir) in &ctx.folders {
        if !name.is_empty() {
            m.insert(format!("workspaceFolder:{name}"), s(dir));
        }
    }
    if let Some(f) = &ctx.file {
        put(&mut m, &["file", "NSQL_FILE"], s(f));
        if let Some(d) = f.parent() {
            put(&mut m, &["fileDirname", "NSQL_FILE_DIR"], s(d));
            if let Some(b) = d.file_name() {
                put(
                    &mut m,
                    &["fileDirnameBasename"],
                    b.to_string_lossy().into_owned(),
                );
            }
        }
        if let Some(b) = f.file_name() {
            put(
                &mut m,
                &["fileBasename", "NSQL_FILE_NAME"],
                b.to_string_lossy().into_owned(),
            );
        }
        if let Some(st) = f.file_stem() {
            put(
                &mut m,
                &["fileBasenameNoExtension"],
                st.to_string_lossy().into_owned(),
            );
        }
        put(
            &mut m,
            &["fileExtname"],
            f.extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default(),
        );
        // 프로젝트 폴더(없으면 등록 폴더) 기준 상대 경로 — VS Code `relativeFile`.
        let base = ctx
            .project_file
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .into_iter()
            .chain(ctx.folders.iter().map(|(_, d)| d.clone()))
            .find(|b| f.starts_with(b));
        if let Some(b) = base {
            if let Ok(rel) = f.strip_prefix(&b) {
                put(&mut m, &["relativeFile"], s(rel));
                if let Some(rd) = rel.parent() {
                    put(&mut m, &["relativeFileDirname"], s(rd));
                }
                put(&mut m, &["fileWorkspaceFolder"], s(&b));
            }
        }
    }
    if let Some(l) = ctx.line {
        put(&mut m, &["lineNumber"], l.to_string());
    }
    if let Some(c) = ctx.column {
        put(&mut m, &["columnNumber"], c.to_string());
    }
    if let Some(h) = &ctx.user_home {
        put(&mut m, &["userHome", "NSQL_USER_HOME"], s(h));
    }
    if let Some(h) = &ctx.app_home {
        put(&mut m, &["nsqlHome", "NSQL_HOME"], s(h));
    }
    if let Some(e) = &ctx.exec {
        put(&mut m, &["execPath", "NSQL_EXEC"], s(e));
    }
    if let Some(c) = &ctx.cwd {
        put(&mut m, &["cwd", "NSQL_CWD"], s(c));
    }
    if let Some(p) = &ctx.profile {
        put(&mut m, &["profile", "NSQL_PROFILE"], p.clone());
    }
    if let Some(d) = &ctx.dialect {
        put(&mut m, &["dialect", "NSQL_DIALECT"], d.clone());
    }
    for (k, v) in &ctx.config {
        m.insert(format!("config:{k}"), v.clone());
    }
    m
}

/// 이름 하나를 푼다 — 정확한 이름(`workspaceFolder:nexa-ui` · `config:log.file` 포함) → 없으면 None.
#[must_use]
pub fn lookup(map: &BTreeMap<String, String>, name: &str) -> Option<String> {
    map.get(name.trim()).cloned()
}

/// 글 안의 `${이름}` · `${env:이름}`을 푼다(설정 값·경로용 · 형식 접미 없음): 내장 → OS 환경 변수 → 글자 그대로.
/// 스크립트 본문은 [`crate::engine::Engine::substitute`](주석·형식·DEFINE 우선)가 따로 처리한다.
#[must_use]
pub fn expand(text: &str, map: &BTreeMap<String, String>) -> String {
    if !text.contains("${") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = &after[..close];
        let value = match inner.strip_prefix("env:") {
            Some(v) => lookup(map, v).or_else(|| std::env::var(v.trim()).ok()),
            None => lookup(map, inner),
        };
        match value {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[i..i + 2 + close + 1]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context {
            project_file: Some(PathBuf::from("/w/demo.nsql-project")),
            folders: vec![
                ("demo".into(), PathBuf::from("/w")),
                ("nexa-ui".into(), PathBuf::from("/x/nexa-ui")),
            ],
            file: Some(PathBuf::from("/w/sql/a.sql")),
            line: Some(12),
            column: Some(3),
            user_home: Some(PathBuf::from("/home/me")),
            app_home: Some(PathBuf::from("/home/me/.nsql")),
            exec: None,
            cwd: Some(PathBuf::from("/w")),
            profile: Some("BISCM".into()),
            dialect: Some("oracle".into()),
            config: vec![("log.file".into(), "${nsqlHome}/log.txt".into())],
        }
    }

    /// VS Code 이름 · 폴더 이름 참조 · 상대 경로 · 대문자 별칭 · config 접두.
    #[test]
    fn builds_vscode_names_and_aliases() {
        let m = build(&ctx());
        let p = |s: &str| PathBuf::from(s).to_string_lossy().into_owned();
        assert_eq!(m["workspaceFolder"], p("/w"));
        assert_eq!(m["NSQL_PROJECT_DIR"], p("/w"));
        assert_eq!(m["workspaceName"], "demo");
        assert_eq!(m["workspaceFolder:nexa-ui"], p("/x/nexa-ui"));
        assert_eq!(m["fileBasename"], "a.sql");
        assert_eq!(m["fileBasenameNoExtension"], "a");
        assert_eq!(m["fileExtname"], ".sql");
        assert_eq!(m["relativeFile"], p("sql/a.sql"));
        assert_eq!(m["relativeFileDirname"], "sql");
        assert_eq!(m["lineNumber"], "12");
        assert_eq!(m["profile"], "BISCM");
        assert_eq!(m["config:log.file"], "${nsqlHome}/log.txt");
        assert_eq!(m["os"], std::env::consts::OS);
        assert!(m.contains_key("pathSeparator") && m.contains_key("/"));
    }

    /// 없는 것은 변수를 만들지 않는다(글자 그대로 남는다).
    #[test]
    fn missing_context_makes_no_variable() {
        let m = build(&Context::default());
        assert!(!m.contains_key("workspaceFolder"));
        assert!(!m.contains_key("file"));
        assert_eq!(expand("${workspaceFolder}/x", &m), "${workspaceFolder}/x");
    }

    /// `expand` = 내장 → `env:`는 내장 별칭 우선 뒤 OS → 모르면 그대로 · 닫히지 않은 괄호는 그대로.
    #[test]
    fn expand_layers_intrinsic_over_env() {
        let m = build(&ctx());
        let home = PathBuf::from("/home/me/.nsql")
            .to_string_lossy()
            .into_owned();
        assert_eq!(expand("${nsqlHome}/log.txt", &m), format!("{home}/log.txt"));
        assert_eq!(
            expand("${env:NSQL_HOME}/log.txt", &m),
            format!("{home}/log.txt")
        );
        let path = std::env::var("PATH").expect("PATH");
        assert_eq!(expand("[${env:PATH}]", &m), format!("[{path}]"));
        assert_eq!(
            expand("${nope} ${env:NSQL_NO_SUCH_42} ${open", &m),
            "${nope} ${env:NSQL_NO_SUCH_42} ${open"
        );
    }
}
