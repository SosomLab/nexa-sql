//! 패키지 폴더 다듬기(`nsql ext pack` · T-166 E8): `extension.json`의 `files[].sha256`을 폴더의 실제 파일로 재계산하고, 루트
//! `index.json`의 그 id 행(name·version·kind)을 메타와 맞춘다(없으면 추가 · summary는 메뉴 한 줄이라 그대로). **원문 서식 보존** — JSON을 통째로 다시 쓰지 않고 값 자리만 바꾼다
//! (`scripts/ext-build.sh`의 python 치환과 같은 뜻 · 3-OS 공통 · python 불필요).

use crate::meta::{json_str, parse_index, parse_meta, Meta};
use crate::sha256;
use std::path::Path;

/// 바뀐 것 요약.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packed {
    pub id: String,
    pub version: String,
    /// (파일, 새 sha256) — 값이 바뀐 것만.
    pub updated: Vec<(String, String)>,
    pub meta_written: bool,
    /// 색인 행을 고쳤다 / 새로 넣었다.
    pub index_updated: bool,
    pub index_added: bool,
}

/// `"<key>": "<old>"`를 찾아 값만 바꾼다(`from` 위치부터 · `until` 전까지) — 바뀐 뒤 텍스트를 돌려준다(없으면 None).
fn set_str_value(text: &str, from: usize, until: usize, key: &str, value: &str) -> Option<String> {
    let seg = &text[from..until];
    let k = format!("\"{key}\"");
    let kpos = seg.find(&k)?;
    let after = &seg[kpos + k.len()..];
    let colon = after.find(':')?;
    let rest = &after[colon + 1..];
    let q1 = rest.find('"')?;
    // 값 문자열의 끝 따옴표(이스케이프 건너뜀).
    let bytes = rest.as_bytes();
    let mut i = q1 + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => break,
            _ => i += 1,
        }
    }
    if i >= bytes.len() {
        return None;
    }
    let vstart = from + kpos + k.len() + colon + 1 + q1;
    let vend = from + kpos + k.len() + colon + 1 + i + 1;
    let mut out = String::with_capacity(text.len() + value.len());
    out.push_str(&text[..vstart]);
    out.push_str(&json_str(value));
    out.push_str(&text[vend..]);
    Some(out)
}

/// `files[]`의 `"path": "<p>"` 항목 객체 범위(그 `{`부터 짝 `}`까지).
fn file_object_span(text: &str, path: &str) -> Option<(usize, usize)> {
    let needle = "\"path\"";
    let mut search = 0;
    while let Some(p) = text[search..].find(needle) {
        let at = search + p;
        let after = &text[at + needle.len()..];
        let colon = after.find(':')?;
        let v = after[colon + 1..].trim_start();
        if v.starts_with(&json_str(path)) {
            let start = text[..at].rfind('{')?;
            let end = at + text[at..].find('}')?;
            return Some((start, end + 1));
        }
        search = at + needle.len();
    }
    None
}

/// 메타 텍스트의 `files[].sha256`을 새 값으로(값이 같으면 그대로) — (새 텍스트, 바뀐 목록).
pub fn update_meta_sha256(
    text: &str,
    sums: &[(String, String)],
) -> (String, Vec<(String, String)>) {
    let mut out = text.to_string();
    let mut changed = Vec::new();
    for (path, sha) in sums {
        let Some((s, e)) = file_object_span(&out, path) else {
            continue;
        };
        let cur = parse_meta(&out)
            .ok()
            .and_then(|m| {
                m.files
                    .into_iter()
                    .find(|f| &f.path == path)
                    .map(|f| f.sha256)
            })
            .unwrap_or_default();
        if cur == *sha {
            continue;
        }
        if let Some(n) = set_str_value(&out, s, e, "sha256", sha) {
            out = n;
            changed.push((path.clone(), sha.clone()));
        }
    }
    (out, changed)
}

/// 색인 텍스트의 id 행을 메타와 맞춘다(없으면 `packages` 끝에 추가) — (새 텍스트, 고침, 추가).
pub fn update_index_row(
    text: &str,
    dir_name: &str,
    m: &Meta,
) -> Result<(String, bool, bool), String> {
    let idx = parse_index(text)?;
    let id_lit = json_str(&m.id);
    if let Some(row) = idx.packages.iter().find(|p| p.id == m.id) {
        // 행 범위 = `"id": "<id>"`를 품은 `{ … }`.
        let needle = "\"id\"";
        let mut search = 0;
        let mut span = None;
        while let Some(p) = text[search..].find(needle) {
            let at = search + p;
            let after = &text[at + needle.len()..];
            if let Some(colon) = after.find(':') {
                if after[colon + 1..].trim_start().starts_with(&id_lit) {
                    let s = text[..at].rfind('{').ok_or("index row")?;
                    let e = at + text[at..].find('}').ok_or("index row")? + 1;
                    span = Some((s, e));
                    break;
                }
            }
            search = at + needle.len();
        }
        let (s, e) = span.ok_or("index row not found")?;
        let mut out = text.to_string();
        let mut changed = false;
        let wants: [(&str, &str, &str); 3] = [
            ("name", &row.name, &m.name),
            ("version", &row.version, &m.version),
            ("kind", row.kind.as_str(), m.kind.as_str()),
        ];
        let mut e_cur = e;
        for (k, cur, want) in wants {
            if cur == want {
                continue;
            }
            match set_str_value(&out, s, e_cur, k, want) {
                Some(n) => {
                    e_cur = e_cur + n.len() - out.len();
                    out = n;
                    changed = true;
                }
                None => return Err(format!("index row has no {k:?} - add it by hand")),
            }
        }
        if row.dir != dir_name {
            return Err(format!(
                "index row dir {:?} != folder {:?} - fix index.json by hand",
                row.dir, dir_name
            ));
        }
        Ok((out, changed, false))
    } else {
        // `"packages": [ … ]`의 닫는 `]` 앞에 한 행 추가(들여쓰기 = 앞 행과 같게 · 첫 행이면 4칸).
        let kp = text.find("\"packages\"").ok_or("index has no packages")?;
        let open = kp + text[kp..].find('[').ok_or("packages array")?;
        // 짝 `]` = 중첩 없음(패키지 행은 평면 객체) → 깊이 추적.
        let bytes = text.as_bytes();
        let mut depth = 0i32;
        let mut close = None;
        for (i, b) in bytes.iter().enumerate().skip(open) {
            match b {
                b'[' | b'{' => depth += 1,
                b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.ok_or("packages array end")?;
        let body = &text[open + 1..close];
        let has_rows = body.trim().len() > 1;
        let indent = body
            .lines()
            .find(|l| l.trim_start().starts_with('{'))
            .map(|l| l[..l.len() - l.trim_start().len()].to_string())
            .unwrap_or_else(|| "    ".to_string());
        let inner = format!("{indent}  ");
        let dir_line = if dir_name == m.id {
            String::new()
        } else {
            format!("{inner}\"dir\": {},\n", json_str(dir_name))
        };
        let row = format!(
            "{indent}{{\n{inner}\"id\": {},\n{dir_line}{inner}\"name\": {},\n{inner}\"version\": {},\n{inner}\"kind\": {},\n{inner}\"summary\": {}\n{indent}}}",
            json_str(&m.id),
            json_str(&m.name),
            json_str(&m.version),
            json_str(m.kind.as_str()),
            json_str(&m.summary)
        );
        let mut out = String::with_capacity(text.len() + row.len() + 8);
        let body_trim_end = text[..close].trim_end();
        out.push_str(body_trim_end);
        if has_rows {
            out.push_str(",\n");
        } else {
            out.push('\n');
        }
        out.push_str(&row);
        out.push('\n');
        // 닫는 `]` 앞 들여쓰기 = 원문 그대로(닫는 줄의 앞 공백).
        let close_line_start = text[..close].rfind('\n').map_or(close, |p| p + 1);
        out.push_str(&text[close_line_start..]);
        Ok((out, false, true))
    }
}

/// 폴더 다듬기: sha256 재계산 → 메타 쓰기(바뀐 것 있을 때만) → 루트 색인 행(루트가 주어졌을 때만).
pub fn pack_dir(dir: &Path, root: Option<&Path>) -> Result<Packed, String> {
    let meta_path = dir.join("extension.json");
    let text = std::fs::read_to_string(&meta_path).map_err(|e| format!("extension.json: {e}"))?;
    let m0 = parse_meta(&text)?;
    let mut sums = Vec::new();
    for f in &m0.files {
        if !f.url.is_empty() {
            continue;
        }
        let bytes = std::fs::read(dir.join(&f.path)).map_err(|e| format!("{}: {e}", f.path))?;
        sums.push((f.path.clone(), sha256::hex(&bytes)));
    }
    let (new_text, updated) = update_meta_sha256(&text, &sums);
    let mut out = Packed {
        id: m0.id,
        version: m0.version,
        updated,
        ..Default::default()
    };
    if new_text != text {
        std::fs::write(&meta_path, new_text.as_bytes())
            .map_err(|e| format!("extension.json: {e}"))?;
        out.meta_written = true;
    }
    if let Some(root) = root {
        let ipath = root.join("index.json");
        let itext = std::fs::read_to_string(&ipath).map_err(|e| format!("index.json: {e}"))?;
        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let m = parse_meta(&new_text)?;
        let (ntext, changed, added) = update_index_row(&itext, &dir_name, &m)?;
        if changed || added {
            std::fs::write(&ipath, ntext.as_bytes()).map_err(|e| format!("index.json: {e}"))?;
        }
        out.index_updated = changed;
        out.index_added = added;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_sha_rewrite_keeps_format() {
        let text = "{\n  \"format\": 1,\n  \"id\": \"x\",\n  \"kind\": \"wasm\",\n  \"files\": [\n    {\n      \"path\": \"a.wasm\",\n      \"sha256\": \"old\",\n      \"dest\": \"\"\n    },\n    { \"path\": \"b.txt\", \"url\": \"https://h/b\", \"sha256\": \"keep\" }\n  ]\n}\n";
        let (n, ch) = update_meta_sha256(text, &[("a.wasm".into(), "ab".into())]);
        assert_eq!(ch, vec![("a.wasm".to_string(), "ab".to_string())]);
        assert!(n.contains("\"sha256\": \"ab\""), "{n}");
        assert!(n.contains("\"sha256\": \"keep\""));
        assert_eq!(n.lines().count(), text.lines().count(), "서식 보존");
        // 같은 값 = 변화 없음.
        let (n2, ch2) = update_meta_sha256(&n, &[("a.wasm".into(), "ab".into())]);
        assert_eq!((n2 == n, ch2.is_empty()), (true, true));
    }

    #[test]
    fn index_row_update_and_append() {
        let idx = "{\n  \"format\": 1,\n  \"packages\": [\n    {\n      \"id\": \"a\",\n      \"name\": \"A\",\n      \"version\": \"1.0.0\",\n      \"kind\": \"wasm\",\n      \"summary\": \"sa\"\n    }\n  ]\n}\n";
        let m = parse_meta(
            r#"{"format":1,"id":"a","name":"A","version":"1.1.0","kind":"wasm","summary":"sa2"}"#,
        )
        .expect("m");
        let (n, changed, added) = update_index_row(idx, "a", &m).expect("row");
        assert!(changed && !added);
        assert!(
            n.contains("\"version\": \"1.1.0\"") && n.contains("\"summary\": \"sa\""),
            "{n}"
        );
        assert!(parse_index(&n).is_ok());
        // 새 행 추가(dir ≠ id면 dir 줄).
        let m2 = parse_meta(r#"{"format":1,"id":"b-ext","name":"B","version":"0.1.0","kind":"data","summary":"sb"}"#).expect("m");
        let (n2, changed2, added2) = update_index_row(&n, "bdir", &m2).expect("row");
        assert!(!changed2 && added2);
        let p = parse_index(&n2).expect("parse");
        assert_eq!(p.packages.len(), 2);
        assert_eq!(
            (p.packages[1].id.as_str(), p.packages[1].dir.as_str()),
            ("b-ext", "bdir")
        );
        // 빈 배열에 추가.
        let (n3, _, added3) =
            update_index_row("{\"format\": 1, \"packages\": []}", "c", &m2).expect("row");
        assert!(added3);
        assert_eq!(parse_index(&n3).expect("p").packages.len(), 1);
        // dir 불일치 = 손으로(오류).
        assert!(update_index_row(&n2, "other", &m2).is_err());
    }

    #[test]
    fn pack_dir_roundtrip() {
        let root = std::env::temp_dir().join(format!("nsql-ext-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pkg = root.join("p");
        std::fs::create_dir_all(&pkg).expect("mkdir");
        std::fs::write(pkg.join("p.wasm"), b"\0asm1234").expect("w");
        std::fs::write(
            pkg.join("extension.json"),
            "{\"format\":1,\"id\":\"p\",\"name\":\"P\",\"version\":\"1.0.0\",\"kind\":\"wasm\",\"summary\":\"s\",\"files\":[{\"path\":\"p.wasm\",\"sha256\":\"\"}]}",
        )
        .expect("w");
        std::fs::write(root.join("index.json"), "{\"format\": 1, \"packages\": []}").expect("w");
        let r = pack_dir(&pkg, Some(&root)).expect("pack");
        assert_eq!(r.updated.len(), 1);
        assert!(r.meta_written && r.index_added && !r.index_updated);
        let again = pack_dir(&pkg, Some(&root)).expect("pack");
        assert_eq!(
            again,
            Packed {
                id: "p".into(),
                version: "1.0.0".into(),
                ..Default::default()
            }
        );
        let rep = crate::check::check_root(&root).expect("check");
        assert!(rep[0].ok(), "{:?}", rep[0].issues);
        let _ = std::fs::remove_dir_all(&root);
    }
}
