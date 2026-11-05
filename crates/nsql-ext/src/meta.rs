//! 확장 저장소 메타 형식 v1(`extensions/README.md` · docs/50 §10) — 루트 `index.json`(패키지 폴더 식별) + 패키지 `extension.json`(설치에
//! 필요한 것). 종전 nexa-sql `extensions/manager.rs`에서 이관(T-166 E8 · 11-05) · 매니저와 `nsql ext`가 같은 파서를 쓴다.

use nsql_json::Json;

/// 메타 형식 버전(다르면 저장소 전체 거부).
pub const FORMAT: i64 = 1;

/// 확장 종류 — `builtin`(앱에 컴파일됨 · 켜기/끄기만) · `data`(파일 배치) · `wasm`(SDK 코드) · `process`(아직 설치 불가).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Builtin,
    Data,
    Wasm,
    Process,
}

impl Kind {
    #[must_use]
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "builtin" => Some(Kind::Builtin),
            "data" => Some(Kind::Data),
            "wasm" => Some(Kind::Wasm),
            "process" => Some(Kind::Process),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
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
pub struct Summary {
    pub id: String,
    pub dir: String,
    pub name: String,
    pub version: String,
    pub kind: Kind,
    pub summary: String,
}

/// 루트 메타.
#[derive(Clone, Debug)]
pub struct Index {
    pub name: String,
    pub packages: Vec<Summary>,
}

/// 설치 파일 하나 — `path` = 패키지 폴더 안 상대 경로 · `sha256` = 소문자 16진 · `dest` = 설정 폴더 기준 배치 경로(비면 보관만).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSpec {
    pub path: String,
    pub sha256: String,
    pub dest: String,
    /// 절대 URL(`https://…`) — 있으면 패키지 폴더 대신 **이 주소**에서 받는다(GitHub Releases 자산 · docs/75 §5 · `path` = 보관 이름).
    pub url: String,
}

/// 패키지 메타(`extension.json`).
#[derive(Clone, Debug)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: Kind,
    pub summary: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub homepage: String,
    /// 필요한 최소 앱 버전(비면 제한 없음 · `check`가 꼴만 본다 · 설치 때 비교 = 후속).
    pub min_app: String,
    pub platforms: Vec<String>,
    pub requires: Vec<String>,
    pub settings_prefix: String,
    pub files: Vec<FileSpec>,
    pub message_install: String,
}

/// 객체에서 키 값(매니저의 `installed.json` 읽기도 같이 쓴다).
#[must_use]
pub fn field<'a>(obj: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    obj.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

#[must_use]
pub fn text(obj: &[(String, Json)], key: &str) -> String {
    match field(obj, key) {
        Some(Json::Str(s)) => s.clone(),
        Some(Json::Num(n)) => n.to_string(),
        Some(Json::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

#[must_use]
pub fn list(obj: &[(String, Json)], key: &str) -> Vec<String> {
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

pub fn object(v: &Json) -> Result<&[(String, Json)], String> {
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
pub fn parse_index(text_in: &str) -> Result<Index, String> {
    let v = nsql_json::parse(text_in)?;
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
pub fn parse_meta(text_in: &str) -> Result<Meta, String> {
    let v = nsql_json::parse(text_in)?;
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
            let url = text(o, "url");
            if !url.is_empty() && !url.starts_with("https://") {
                return Err(format!("{id}: file url must be https ({url})"));
            }
            files.push(FileSpec {
                path,
                sha256: text(o, "sha256").to_ascii_lowercase(),
                dest,
                url,
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

/// JSON 문자열 리터럴(따옴표 포함 · 최소 이스케이프) — 색인 행 생성·갱신에.
#[must_use]
pub fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
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
        let meta = parse_meta(r#"{"format":1,"id":"demo-data","name":"Demo","version":"1.0.0","kind":"data","min_app":"0.5.0",
            "platforms":["windows","macos","linux"],"files":[{"path":"a.txt","sha256":"x","dest":"Packages/Demo/a.txt"}],
            "messages":{"install":"hi"}}"#).expect("test");
        assert_eq!(meta.files[0].dest, "Packages/Demo/a.txt");
        assert_eq!(meta.message_install, "hi");
        assert_eq!(meta.min_app, "0.5.0");
        assert!(parse_meta(
            r#"{"format":1,"id":"x","kind":"data","files":[{"path":"/etc/passwd"}]}"#
        )
        .is_err());
        assert!(parse_meta(
            r#"{"format":1,"id":"x","kind":"wasm","files":[{"path":"a.wasm","url":"http://x/a.wasm"}]}"#
        )
        .is_err());
        assert_eq!(json_str("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
