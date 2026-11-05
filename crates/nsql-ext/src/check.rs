//! 패키지 폴더 검사(`nsql ext check` · T-166 E8): 메타 파싱 · 파일 존재 · sha256 일치 · wasm 매직 · id/버전/플랫폼/`min_app` 꼴 ·
//! 루트 `index.json`과의 일치. 순수 판정은 바이트·문자열을 받고, 폴더 읽기만 얇게 감싼다(시험 = 임시 폴더).

use crate::meta::{parse_index, parse_meta, Kind, Meta, Summary};
use crate::sha256;
use std::path::Path;

/// 문제 종류(CLI가 사용자 문구로 바꾼다 · 코드는 안정 식별자).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueCode {
    /// `extension.json`이 없거나 형식 오류.
    MetaParse,
    /// `files[].path`가 폴더에 없다(`url`이 없는 파일만).
    FileMissing,
    /// 파일 sha256이 메타와 다르다.
    Sha256Mismatch,
    /// `kind: wasm`인데 `\0asm` 매직으로 시작하는 `.wasm` 파일이 하나도 없다.
    WasmMagic,
    /// id가 소문자·숫자·`-`가 아니다.
    BadId,
    /// 버전이 `주.부[.수][-태그]` 꼴이 아니다.
    BadVersion,
    /// `platforms[]`에 windows·macos·linux 밖 값.
    BadPlatform,
    /// `min_app`이 버전 꼴이 아니다.
    BadMinApp,
    /// 루트 `index.json`에 이 id 행이 없다.
    IndexMissing,
    /// 루트 행의 name/version/kind/dir가 메타·폴더와 다르다(summary는 비교하지 않는다 — 색인 summary = 메뉴 한 줄 · 메타는 긴 글).
    IndexMismatch,
    /// `kind: data`인데 설치 파일이 없다.
    NoFiles,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub code: IssueCode,
    /// 보조 글(파일 이름 · 기대/실제 값) — 사용자 문구의 `{n}` 자리.
    pub detail: String,
}

/// 패키지 하나의 검사 결과.
#[derive(Clone, Debug)]
pub struct Report {
    pub dir_name: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: Option<Kind>,
    pub files: usize,
    pub issues: Vec<Issue>,
}

impl Report {
    #[must_use]
    pub fn ok(&self) -> bool {
        self.issues.is_empty()
    }
}

/// id 꼴: 소문자·숫자·`-`(비어 있지 않음 · `-`로 시작/끝 안 함).
#[must_use]
pub fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

/// 버전 꼴: `주.부[.수]` 숫자 + 선택 `-태그`/`+빌드`.
#[must_use]
pub fn version_ok(v: &str) -> bool {
    let core = v.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

const PLATFORMS: [&str; 3] = ["windows", "macos", "linux"];

/// 메타 값만으로 보는 꼴 검사(파일 접근 없음).
pub fn check_meta_shape(m: &Meta, issues: &mut Vec<Issue>) {
    if !id_ok(&m.id) {
        issues.push(Issue {
            code: IssueCode::BadId,
            detail: m.id.clone(),
        });
    }
    if !version_ok(&m.version) {
        issues.push(Issue {
            code: IssueCode::BadVersion,
            detail: m.version.clone(),
        });
    }
    for p in &m.platforms {
        if !PLATFORMS.contains(&p.as_str()) {
            issues.push(Issue {
                code: IssueCode::BadPlatform,
                detail: p.clone(),
            });
        }
    }
    if !m.min_app.is_empty() && !version_ok(&m.min_app) {
        issues.push(Issue {
            code: IssueCode::BadMinApp,
            detail: m.min_app.clone(),
        });
    }
    if m.kind == Kind::Data && m.files.is_empty() {
        issues.push(Issue {
            code: IssueCode::NoFiles,
            detail: String::new(),
        });
    }
}

/// 루트 행 ↔ 메타·폴더 이름 일치(순수).
pub fn check_index_row(m: &Meta, dir_name: &str, row: Option<&Summary>, issues: &mut Vec<Issue>) {
    let Some(r) = row else {
        issues.push(Issue {
            code: IssueCode::IndexMissing,
            detail: m.id.clone(),
        });
        return;
    };
    let mut diffs = Vec::new();
    if r.dir != dir_name {
        diffs.push(format!("dir {:?} != {:?}", r.dir, dir_name));
    }
    if r.name != m.name {
        diffs.push(format!("name {:?} != {:?}", r.name, m.name));
    }
    if r.version != m.version {
        diffs.push(format!("version {:?} != {:?}", r.version, m.version));
    }
    if r.kind != m.kind {
        diffs.push(format!("kind {} != {}", r.kind.as_str(), m.kind.as_str()));
    }
    if !diffs.is_empty() {
        issues.push(Issue {
            code: IssueCode::IndexMismatch,
            detail: diffs.join(" · "),
        });
    }
}

/// 패키지 폴더 검사. `index` = 루트 색인(있으면 행 일치까지 · 없으면 색인 검사 생략).
pub fn check_dir(dir: &Path, index: Option<&crate::meta::Index>) -> Report {
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut issues = Vec::new();
    let meta_text = std::fs::read_to_string(dir.join("extension.json"));
    let m = match meta_text
        .map_err(|e| e.to_string())
        .and_then(|t| parse_meta(&t))
    {
        Ok(m) => m,
        Err(e) => {
            issues.push(Issue {
                code: IssueCode::MetaParse,
                detail: e,
            });
            return Report {
                dir_name,
                id: String::new(),
                name: String::new(),
                version: String::new(),
                kind: None,
                files: 0,
                issues,
            };
        }
    };
    check_meta_shape(&m, &mut issues);
    let mut wasm_ok = m.kind != Kind::Wasm;
    for f in &m.files {
        if !f.url.is_empty() {
            // 원격 자산(Releases)은 폴더에 없다 — sha256은 설치 때 본다.
            continue;
        }
        match std::fs::read(dir.join(&f.path)) {
            Ok(bytes) => {
                let got = sha256::hex(&bytes);
                if got != f.sha256 {
                    issues.push(Issue {
                        code: IssueCode::Sha256Mismatch,
                        detail: format!("{} expected {} got {got}", f.path, f.sha256),
                    });
                }
                if f.path.ends_with(".wasm") && bytes.starts_with(b"\0asm") {
                    wasm_ok = true;
                }
            }
            Err(_) => issues.push(Issue {
                code: IssueCode::FileMissing,
                detail: f.path.clone(),
            }),
        }
    }
    if !wasm_ok {
        issues.push(Issue {
            code: IssueCode::WasmMagic,
            detail: String::new(),
        });
    }
    if let Some(idx) = index {
        let row = idx.packages.iter().find(|p| p.id == m.id);
        check_index_row(&m, &dir_name, row, &mut issues);
    }
    Report {
        dir_name,
        id: m.id,
        name: m.name,
        version: m.version,
        kind: Some(m.kind),
        files: m.files.len(),
        issues,
    }
}

/// 저장소 루트 검사: `index.json`의 모든 패키지 폴더를 검사한다(색인 파싱 실패 = Err).
pub fn check_root(root: &Path) -> Result<Vec<Report>, String> {
    let text =
        std::fs::read_to_string(root.join("index.json")).map_err(|e| format!("index.json: {e}"))?;
    let idx = parse_index(&text)?;
    Ok(idx
        .packages
        .iter()
        .map(|p| check_dir(&root.join(&p.dir), Some(&idx)))
        .collect())
}

/// 경로가 저장소 루트인가(`index.json`이 있다) · 패키지 폴더인가(`extension.json`).
#[must_use]
pub fn is_root(p: &Path) -> bool {
    p.join("index.json").is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nsql-ext-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("tmp");
        d
    }

    #[test]
    fn shapes() {
        assert!(id_ok("hello-ext"));
        assert!(!id_ok("Hello"));
        assert!(!id_ok("-x"));
        assert!(!id_ok(""));
        assert!(version_ok("1.2.3"));
        assert!(version_ok("0.5"));
        assert!(version_ok("1.0.0-beta+7"));
        assert!(!version_ok("1"));
        assert!(!version_ok("a.b"));
        assert!(!version_ok("1..2"));
    }

    /// 폴더 검사: 정상 → 0 · sha256 틀림 · 파일 없음 · wasm 매직 없음 · 색인 행 불일치/없음.
    #[test]
    fn check_dir_reports_each_issue_once() {
        let root = tmp("check");
        let pkg = root.join("demo-ext");
        std::fs::create_dir_all(&pkg).expect("mkdir");
        let wasm = b"\0asm\x01\0\0\0demo".to_vec();
        std::fs::write(pkg.join("demo.wasm"), &wasm).expect("w");
        let sha = sha256::hex(&wasm);
        let meta = |sha: &str| {
            format!(
                r#"{{"format":1,"id":"demo-ext","name":"Demo","version":"1.0.0","kind":"wasm","summary":"s",
                "min_app":"0.5.0","platforms":["windows"],"files":[{{"path":"demo.wasm","sha256":"{sha}"}}]}}"#
            )
        };
        std::fs::write(pkg.join("extension.json"), meta(&sha)).expect("w");
        let index = parse_index(
            r#"{"format":1,"packages":[{"id":"demo-ext","name":"Demo","version":"1.0.0","kind":"wasm","summary":"s"}]}"#,
        )
        .expect("idx");
        let r = check_dir(&pkg, Some(&index));
        assert!(r.ok(), "{:?}", r.issues);
        assert_eq!(
            (r.id.as_str(), r.files, r.kind),
            ("demo-ext", 1, Some(Kind::Wasm))
        );
        // sha256 틀림.
        std::fs::write(pkg.join("extension.json"), meta("00")).expect("w");
        let r = check_dir(&pkg, Some(&index));
        assert_eq!(r.issues.len(), 1);
        assert_eq!(r.issues[0].code, IssueCode::Sha256Mismatch);
        // 파일 없음 + wasm 매직 없음(읽을 파일이 없으니 매직도 못 봄).
        std::fs::write(pkg.join("extension.json"), meta(&sha)).expect("w");
        std::fs::remove_file(pkg.join("demo.wasm")).expect("rm");
        let codes: Vec<IssueCode> = check_dir(&pkg, Some(&index))
            .issues
            .iter()
            .map(|i| i.code)
            .collect();
        assert_eq!(codes, vec![IssueCode::FileMissing, IssueCode::WasmMagic]);
        // 색인 행 불일치(버전) · 없음.
        std::fs::write(pkg.join("demo.wasm"), &wasm).expect("w");
        let idx2 = parse_index(
            r#"{"format":1,"packages":[{"id":"demo-ext","name":"Demo","version":"1.0.1","kind":"wasm","summary":"menu line"}]}"#,
        )
        .expect("idx");
        let r = check_dir(&pkg, Some(&idx2));
        assert_eq!(r.issues[0].code, IssueCode::IndexMismatch);
        assert!(r.issues[0].detail.contains("version"));
        assert!(
            !r.issues[0].detail.contains("summary"),
            "summary는 비교 안 함"
        );
        let idx3 = parse_index(r#"{"format":1,"packages":[]}"#).expect("idx");
        assert_eq!(
            check_dir(&pkg, Some(&idx3)).issues[0].code,
            IssueCode::IndexMissing
        );
        // 색인 없이(패키지 폴더만) = 색인 검사 생략.
        assert!(check_dir(&pkg, None).ok());
        // 메타 없음.
        let r = check_dir(&root.join("nope"), None);
        assert_eq!(r.issues[0].code, IssueCode::MetaParse);
        // 루트 검사 = 색인의 폴더 전부.
        std::fs::write(
            root.join("index.json"),
            r#"{"format":1,"packages":[{"id":"demo-ext","name":"Demo","version":"1.0.0","kind":"wasm","summary":"s"}]}"#,
        )
        .expect("w");
        assert!(is_root(&root));
        let all = check_root(&root).expect("root");
        assert_eq!(all.len(), 1);
        assert!(all[0].ok());
        let _ = std::fs::remove_dir_all(&root);
    }
}
