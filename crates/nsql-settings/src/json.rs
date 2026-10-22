//! 설정 ↔ JSON(사용자 09-15 "설정을 JSON으로 편집") — 키 `a.b.c`를 **객체 계층** `{"a":{"b":{"c":…}}}`로 내보내고,
//! 파일이 저장되면 호스트가 다시 읽어 바뀐 키만 반영한다. 외부 crate 0 — 필요한 만큼의 JSON 파서·작성기(객체·배열·문자열·숫자·불·null).
//!
//! - 값 타입: `Int` → 숫자 · `Bool` → true/false · 그 외 → 문자열. `HIDDEN`(비노출)도 함께 내보낸다(`_comment`에 안내).
//! - 가져오기: JSON에 **있는** 키만 적용(없는 키는 그대로) · 모르는 키·틀린 값은 보고만 하고 나머지는 적용.

use crate::{entry, normalize, SettingKind, Settings, REGISTRY};
pub use nsql_json::{dump, parse, Json};
use std::collections::BTreeMap;

fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// 중첩 트리(내보내기용).
#[derive(Default)]
struct Node {
    leaf: Option<String>,
    kids: BTreeMap<String, Node>,
}

fn json_value_for(kind: SettingKind, v: &str) -> String {
    match kind {
        SettingKind::Int { .. } => v
            .parse::<i64>()
            .map_or_else(|_| escape(v), |n| n.to_string()),
        // 글꼴 크기: `13`은 숫자로 · `10pt`는 문자열로(단위 보존).
        SettingKind::Size { .. } => v.parse::<f64>().map_or_else(
            |_| escape(v),
            |n| {
                if n.fract() == 0.0 {
                    format!("{}", n as i64)
                } else {
                    n.to_string()
                }
            },
        ),
        SettingKind::Bool => (v == "on").to_string(),
        _ => escape(v),
    }
}

fn write_node(out: &mut String, node: &Node, depth: usize) {
    let ind = "  ".repeat(depth);
    out.push_str("{\n");
    let n = node.kids.len();
    for (i, (k, child)) in node.kids.iter().enumerate() {
        out.push_str(&ind);
        out.push_str("  ");
        out.push_str(&escape(k));
        out.push_str(": ");
        match &child.leaf {
            Some(v) if child.kids.is_empty() => out.push_str(v),
            _ => write_node(out, child, depth + 1),
        }
        if i + 1 < n {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&ind);
    out.push('}');
}

/// 전체 설정을 객체 계층 JSON으로(현재 값 · 기본값 포함 · 키 사전순 · 2칸 들여쓰기).
#[must_use]
pub fn to_json(s: &Settings) -> String {
    let mut root = Node::default();
    root.kids.insert(
        "_comment".into(),
        Node {
            leaf: Some(escape(
                "Nexa SQL settings - edit and save; the app reloads changed keys immediately. Keys = registry (nsql config list all). Unknown keys are ignored.",
            )),
            kids: BTreeMap::new(),
        },
    );
    for e in REGISTRY {
        let v = s.get(e.key).unwrap_or(e.default);
        let mut cur = &mut root;
        for seg in e.key.split('.') {
            cur = cur.kids.entry(seg.to_string()).or_default();
        }
        cur.leaf = Some(json_value_for(e.kind, v));
    }
    let mut out = String::new();
    write_node(&mut out, &root, 0);
    out.push('\n');
    out
}

/// 가져오기 결과.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Import {
    /// 값이 실제로 바뀐 키.
    pub changed: Vec<String>,
    /// 레지스트리에 없는 키(무시).
    pub unknown: Vec<String>,
    /// 허용 범위 밖 값(키, 값) — 무시.
    pub invalid: Vec<(String, String)>,
}

fn flatten(prefix: &str, v: &Json, out: &mut Vec<(String, Json)>) {
    match v {
        Json::Obj(items) => {
            for (k, child) in items {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, child, out);
            }
        }
        other => out.push((prefix.to_string(), other.clone())),
    }
}

impl Settings {
    /// JSON 텍스트를 설정에 적용(메모리) — 있는 키만 · 바뀐 키 목록.
    pub fn import_json(&mut self, text: &str) -> Result<Import, String> {
        let v = parse(text)?;
        let mut flat = Vec::new();
        flatten("", &v, &mut flat);
        let mut r = Import::default();
        for (k, jv) in flat {
            if k.starts_with('_') {
                continue;
            }
            let Some(e) = entry(&k) else {
                r.unknown.push(k);
                continue;
            };
            let Some(raw) = jv.scalar_text() else {
                r.unknown.push(k);
                continue;
            };
            let Some(n) = normalize(e.kind, &raw) else {
                r.invalid.push((k, raw));
                continue;
            };
            let cur = self.get(&k).unwrap_or(e.default).to_string();
            if n != cur {
                let _ = self.set(&k, &n);
                r.changed.push(k);
            }
        }
        Ok(r)
    }

    /// `settings.json` 경로(설정 파일 옆).
    #[must_use]
    pub fn json_path(&self) -> std::path::PathBuf {
        self.path().with_file_name("settings.json")
    }

    /// 현재 설정을 `settings.json`으로 써 낸다(경로 반환).
    pub fn export_json(&self) -> std::io::Result<std::path::PathBuf> {
        let p = self.json_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, to_json(self))?;
        Ok(p)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn fresh() -> Settings {
        Settings::open(std::path::PathBuf::from("__json_test_nonexistent__.conf"))
    }

    #[test]
    fn export_is_nested_and_import_applies_changed_keys_only() {
        let mut s = fresh();
        let text = to_json(&s);
        assert!(text.contains("\"ui\": {"));
        assert!(text.contains("\"font_size\": 15"));
        assert!(
            text.contains("\"line_numbers\": true") || text.contains("\"line_numbers\": false")
        );
        // 다시 읽으면 바뀐 키 0.
        let r = s.import_json(&text).unwrap();
        assert!(r.changed.is_empty(), "{r:?}");
        assert!(r.unknown.is_empty());
        // 일부만 바꾼 JSON
        let r = s
            .import_json(r#"{"ui": {"font_size": 18, "theme": "dark"}, "nope": {"x": 1}, "grid": {"max_rows": "abc"}}"#)
            .unwrap();
        assert_eq!(r.changed, vec!["ui.font_size", "ui.theme"]);
        assert_eq!(r.unknown, vec!["nope.x"]);
        assert_eq!(
            r.invalid,
            vec![("grid.max_rows".to_string(), "abc".to_string())]
        );
        assert_eq!(s.int("ui.font_size"), 18);
        assert_eq!(s.get("ui.theme"), Some("dark"));
    }
}
