//! 구문 레지스트리(사용자 09-14 · Sublime 차용) — 내장(SQL · Plain Text) + **플러그인 패키지**.
//!
//! - 기본 선택 = 탭 제목의 확장자(`.sql`·`.txt` …) · 확장자 없는 새 스크립트(`Script_N`)는 SQL.
//! - 사용자 변경 = 명령 팔레트 `Set Syntax: <이름>` · 상태줄 구문 이름 클릭.
//! - 플러그인 = `<설정 폴더>/Packages/<패키지>/<이름>.nexa-syntax`(Sublime 패키지 배치 · DR-5).
//!   같은 이름이면 나중 것(사용자 패키지)이 내장을 덮는다. 파싱 실패는 stderr + [`SyntaxRegistry::errors`].

use nexa_ctl::SyntaxSpec;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct SyntaxRegistry {
    specs: Vec<Rc<SyntaxSpec>>,
    errors: Vec<String>,
}

impl SyntaxRegistry {
    /// 내장 + `Packages/` 스캔.
    pub(crate) fn load() -> Self {
        let mut r = SyntaxRegistry {
            specs: vec![Rc::new(SyntaxSpec::sql()), Rc::new(SyntaxSpec::plain())],
            errors: Vec::new(),
        };
        if let Some(dir) = Self::packages_dir() {
            r.scan(&dir);
        }
        r
    }

    /// `<설정 폴더>/Packages`.
    pub(crate) fn packages_dir() -> Option<PathBuf> {
        nsql_settings::config_dir().map(|d| d.join("Packages"))
    }

    fn scan(&mut self, root: &std::path::Path) {
        let Ok(pkgs) = std::fs::read_dir(root) else {
            return;
        };
        let mut pkg_dirs: Vec<PathBuf> = pkgs
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        pkg_dirs.sort();
        for pkg in pkg_dirs {
            let Ok(files) = std::fs::read_dir(&pkg) else {
                continue;
            };
            let mut files: Vec<PathBuf> = files
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "nexa-syntax"))
                .collect();
            files.sort();
            for f in files {
                match std::fs::read_to_string(&f)
                    .map_err(|e| e.to_string())
                    .and_then(|t| SyntaxSpec::parse(&t))
                {
                    Ok(spec) => self.add(spec),
                    Err(e) => {
                        let msg = format!("{}: {e}", f.display());
                        eprintln!("syntax: {msg}");
                        self.errors.push(msg);
                    }
                }
            }
        }
    }

    fn add(&mut self, spec: SyntaxSpec) {
        self.specs.retain(|s| s.name != spec.name);
        self.specs.push(Rc::new(spec));
    }

    pub(crate) fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.specs.iter().map(|s| s.name.clone()).collect();
        v.sort();
        v
    }

    pub(crate) fn get(&self, name: &str) -> Option<Rc<SyntaxSpec>> {
        self.specs.iter().find(|s| s.name == name).cloned()
    }

    #[allow(dead_code)]
    pub(crate) fn errors(&self) -> &[String] {
        &self.errors
    }

    /// 탭 제목(파일명)에서 기본 구문 — 확장자 매칭 · 없으면 SQL.
    pub(crate) fn for_title(&self, title: &str) -> Rc<SyntaxSpec> {
        let ext = title.rsplit_once('.').map(|(_, e)| e.to_lowercase());
        if let Some(ext) = ext {
            if let Some(s) = self.specs.iter().find(|s| s.extensions.contains(&ext)) {
                return s.clone();
            }
        }
        self.get("SQL")
            .or_else(|| self.specs.first().cloned())
            .unwrap_or_else(|| Rc::new(SyntaxSpec::plain()))
    }
}
