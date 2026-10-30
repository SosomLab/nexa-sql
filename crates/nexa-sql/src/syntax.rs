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
    /// ★ T-69 문법 층(10-30 · 67 §8-3): `Packages/<패키지>/<이름>.nexa-settings`(Sublime `<Syntax>.sublime-settings` 차용 · JSON 객체) —
    ///   (구문 이름 소문자, (키, 원시 값)) · 같은 이름은 나중 패키지가 덮는다.
    settings: Vec<(String, Vec<(String, String)>)>,
}

impl SyntaxRegistry {
    /// 내장 + `Packages/` 스캔.
    pub(crate) fn load() -> Self {
        let mut r = SyntaxRegistry {
            // ★ T-76 잔여(10-30): JSON(settings.json · `.sublime-keymap` 등) 내장 — `Packages/`의 같은 이름 규격이 있으면 그쪽이 덮는다.
            specs: vec![
                Rc::new(SyntaxSpec::sql()),
                Rc::new(SyntaxSpec::json()),
                Rc::new(SyntaxSpec::plain()),
            ],
            settings: Vec::new(),
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
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e == "nexa-syntax" || e == "nexa-settings")
                })
                .collect();
            files.sort();
            for f in files {
                if f.extension().is_some_and(|e| e == "nexa-settings") {
                    match std::fs::read_to_string(&f)
                        .map_err(|e| e.to_string())
                        .and_then(|t| nsql_settings::json::parse(&t))
                    {
                        Ok(js) => {
                            let name = f
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_lowercase())
                                .unwrap_or_default();
                            self.add_settings(&name, &js);
                        }
                        Err(e) => eprintln!("syntax settings: {}: {e}", f.display()),
                    }
                    continue;
                }
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

    /// `<이름>.nexa-settings` 본문(JSON 객체) 등록 — 객체가 아니면 무시 · 값은 원시 글자(`project::json_raw`).
    fn add_settings(&mut self, name: &str, js: &nsql_settings::json::Json) {
        let nsql_settings::json::Json::Obj(items) = js else {
            return;
        };
        let kv: Vec<(String, String)> = items
            .iter()
            .filter_map(|(k, v)| crate::project::json_raw(v).map(|r| (k.clone(), r)))
            .collect();
        self.settings.retain(|(n, _)| n != name);
        self.settings.push((name.to_string(), kv));
    }

    /// 문법 층의 들여쓰기(구문 이름 소문자 → (탭 폭, 공백 여부) · 각각 있을 때만) — `editor.tab_size`·`editor.indent_spaces`만 본다.
    pub(crate) fn indent_defaults(
        &self,
    ) -> std::collections::BTreeMap<String, (Option<u8>, Option<bool>)> {
        let mut out = std::collections::BTreeMap::new();
        for (name, kv) in &self.settings {
            let mut v: (Option<u8>, Option<bool>) = (None, None);
            for (k, raw) in kv {
                match k.as_str() {
                    "editor.tab_size" => {
                        v.0 = raw.parse::<u8>().ok().filter(|n| (1..=16).contains(n));
                    }
                    "editor.indent_spaces" => {
                        v.1 = match raw.as_str() {
                            "on" | "true" => Some(true),
                            "off" | "false" => Some(false),
                            _ => None,
                        };
                    }
                    _ => {}
                }
            }
            if v.0.is_some() || v.1.is_some() {
                out.insert(name.clone(), v);
            }
        }
        out
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
            // ★ 객체 백업 파일(D-305 · 10-15) = 확장자가 17자리 시각 `yyyyMMddHHmmssSSS` → SQL(모르는 확장자 규칙의 예외).
            if ext.len() == 17 && ext.bytes().all(|b| b.is_ascii_digit()) {
                if let Some(s) = self.get("SQL") {
                    return s;
                }
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
            specs: vec![
                Rc::new(SyntaxSpec::sql()),
                Rc::new(SyntaxSpec::json()),
                Rc::new(SyntaxSpec::plain()),
            ],
            settings: Vec::new(),
        }
    }

    /// ★ T-69 문법 층: `<이름>.nexa-settings` 객체 → 들여쓰기 기본(탭 폭·공백 각각) · 객체 아님·범위 밖 = 무시 · 같은 이름은 덮음.
    #[test]
    fn syntax_settings_layer_indent_defaults() {
        let mut r = reg();
        r.add_settings(
            "sql",
            &nsql_settings::json::parse(r#"{"editor.tab_size": 2, "other": "x"}"#).expect("json"),
        );
        r.add_settings(
            "json",
            &nsql_settings::json::parse(
                r#"{"editor.indent_spaces": false, "editor.tab_size": 99}"#,
            )
            .expect("json"),
        );
        r.add_settings(
            "plain text",
            &nsql_settings::json::parse("[1]").expect("json"),
        );
        let d = r.indent_defaults();
        assert_eq!(d.get("sql"), Some(&(Some(2), None)));
        assert_eq!(d.get("json"), Some(&(None, Some(false))));
        assert!(!d.contains_key("plain text"));
        r.add_settings(
            "sql",
            &nsql_settings::json::parse(r#"{"editor.tab_size": 8}"#).expect("json"),
        );
        assert_eq!(r.indent_defaults().get("sql"), Some(&(Some(8), None)));
    }

    /// ★ T-76 잔여(10-30): settings.json · 사용자 키맵 파일은 JSON 규격 · SQL은 그대로.
    #[test]
    fn json_files_get_json_syntax() {
        let r = reg();
        assert_eq!(r.for_title("settings.json").name, "JSON");
        assert_eq!(r.for_title("Default (Windows).sublime-keymap").name, "JSON");
        assert_eq!(r.for_title("query.sql").name, "SQL");
        assert!(r.names().contains(&"JSON".to_string()));
    }

    /// 확장자 → 구문: .sql = SQL · .txt/.log/.md = Plain Text · **모르는 확장자(.rs/.toml/.exe) = Plain Text** · 확장자 없음 = SQL ·
    /// `.`으로 시작하는 이름(.gitignore)은 확장자가 아니다 = SQL(종전과 같음).
    #[test]
    fn for_title_falls_back_to_plain_for_unknown_extension() {
        let r = reg();
        assert_eq!(r.for_title("a.sql").name, "SQL");
        assert_eq!(r.for_title("A.SQL").name, "SQL");
        assert_eq!(r.for_title("readme.md").name, "Plain Text");
        // 객체 백업 파일(D-305) = 17자리 시각 확장자 → SQL · 16자리·글자 섞임 = Plain Text.
        assert_eq!(
            r.for_title("APPDB_HR.EMP_TABLE.20261015174436123").name,
            "SQL"
        );
        assert_eq!(r.for_title("x.2026101517443612").name, "Plain Text");
        assert_eq!(r.for_title("x.2026101517443612a").name, "Plain Text");
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
