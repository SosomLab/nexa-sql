//! 구문 레지스트리(사용자 09-14 · Sublime 차용) — 내장(SQL · Plain Text) + **플러그인 패키지**.
//!
//! - 기본 선택 = 탭 제목의 확장자(`.sql`·`.txt` …) · 확장자 없는 새 스크립트(`Script_N`)는 SQL.
//! - 사용자 변경 = 명령 팔레트 `Set Syntax: <이름>` · 상태줄 구문 이름 클릭.
//! - 플러그인 = `<설정 폴더>/Packages/<패키지>/<이름>.nexa-syntax`(Sublime 패키지 배치 · DR-5).
//!   같은 이름이면 나중 것(사용자 패키지)이 내장을 덮는다. 파싱 실패는 stderr.

use nexa_ctl::SyntaxSpec;
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct SyntaxRegistry {
    specs: Vec<Rc<SyntaxSpec>>,
}

impl SyntaxRegistry {
    /// 내장 + `Packages/` 스캔.
    pub(crate) fn load() -> Self {
        let mut r = SyntaxRegistry {
            specs: vec![Rc::new(SyntaxSpec::sql()), Rc::new(SyntaxSpec::plain())],
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
                        eprintln!("syntax: {}: {e}", f.display());
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

    /// 탭 제목(파일명)에서 기본 구문 — 확장자 매칭 · **확장자가 있는데 아무 구문에도 없으면 Plain Text**(`.rs`·`.toml`·`.exe`… 를 SQL로 열면
    /// `--`가 주석이 돼 `[--flag`가 "짝 없는 괄호"로 그어졌다 · 사용자 10-07 캡처) · 확장자 없음(`Script_N`·`NoName`) = SQL.
    pub(crate) fn for_title(&self, title: &str) -> Rc<SyntaxSpec> {
        let ext = title
            .rsplit_once('.')
            .map(|(stem, e)| (stem, e.to_lowercase()))
            .filter(|(stem, e)| !stem.is_empty() && !e.is_empty() && !e.contains(' '));
        if let Some((_, ext)) = ext {
            if let Some(s) = self.specs.iter().find(|s| s.extensions.contains(&ext)) {
                return s.clone();
            }
            return self
                .get("Plain Text")
                .unwrap_or_else(|| Rc::new(SyntaxSpec::plain()));
        }
        self.get("SQL")
            .or_else(|| self.specs.first().cloned())
            .unwrap_or_else(|| Rc::new(SyntaxSpec::plain()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> SyntaxRegistry {
        SyntaxRegistry {
            specs: vec![Rc::new(SyntaxSpec::sql()), Rc::new(SyntaxSpec::plain())],
        }
    }

    /// 확장자 → 구문: .sql = SQL · .txt/.log/.md = Plain Text · **모르는 확장자(.rs/.toml/.exe) = Plain Text** · 확장자 없음 = SQL ·
    /// `.`으로 시작하는 이름(.gitignore)은 확장자가 아니다 = SQL(종전과 같음).
    #[test]
    fn for_title_falls_back_to_plain_for_unknown_extension() {
        let r = reg();
        assert_eq!(r.for_title("a.sql").name, "SQL");
        assert_eq!(r.for_title("A.SQL").name, "SQL");
        assert_eq!(r.for_title("readme.md").name, "Plain Text");
        assert_eq!(r.for_title("main.rs").name, "Plain Text");
        assert_eq!(r.for_title("rust-toolchain.toml").name, "Plain Text");
        assert_eq!(
            r.for_title("nexa-clip-0.1.6-windows-x64-setup.exe").name,
            "Plain Text"
        );
        assert_eq!(r.for_title("Script_1").name, "SQL");
        assert_eq!(r.for_title("NoName1").name, "SQL");
        assert_eq!(r.for_title("테스트1").name, "SQL");
        assert_eq!(r.for_title(".gitignore").name, "SQL");
    }
}
