//! 설정 ↔ JSON(사용자 09-15 "설정을 JSON으로 편집") — 키 `a.b.c`를 **객체 계층** `{"a":{"b":{"c":…}}}`로 내보내고,
//! 파일이 저장되면 호스트가 다시 읽어 바뀐 키만 반영한다. 외부 crate 0 — 필요한 만큼의 JSON 파서·작성기(객체·배열·문자열·숫자·불·null).
//!
//! - 값 타입: `Int` → 숫자 · `Bool` → true/false · 그 외 → 문자열. `HIDDEN`(비노출)도 함께 내보낸다(`_comment`에 안내).
//! - 가져오기: JSON에 **있는** 키만 적용(없는 키는 그대로) · 모르는 키·틀린 값은 보고만 하고 나머지는 적용.

use crate::{entry, normalize, SettingKind, Settings, REGISTRY};
use std::collections::BTreeMap;

/// 최소 JSON 값.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn scalar_text(&self) -> Option<String> {
        match self {
            Json::Null => Some(String::new()),
            Json::Bool(b) => Some(if *b { "on".into() } else { "off".into() }),
            Json::Num(n) => Some(if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }),
            Json::Str(s) => Some(s.clone()),
            Json::Arr(items) => Some(
                items
                    .iter()
                    .filter_map(Json::scalar_text)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            Json::Obj(_) => None,
        }
    }
}

/// JSON 텍스트 파싱(오류 = 위치 포함 메시지).
/// 값 → JSON 글(공백 없이 · 문자열 이스케이프). 프로젝트 파일에 내장된 객체(북마크)를 그대로 되돌릴 때 쓴다(09-23).
pub fn dump(v: &Json) -> String {
    fn esc(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
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
    match v {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Json::Str(s) => format!("\"{}\"", esc(s)),
        Json::Arr(a) => format!("[{}]", a.iter().map(dump).collect::<Vec<_>>().join(",")),
        Json::Obj(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("\"{}\":{}", esc(k), dump(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(format!("trailing characters at {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at {}", c as char, self.i))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'n') => self.lit("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected '{}' at {}", c as char, self.i)),
            None => Err("unexpected end".into()),
        }
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("invalid literal at {}", self.i))
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        while self.i < self.b.len()
            && matches!(
                self.b[self.i],
                b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
            )
        {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        s.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("bad number '{s}' at {start}"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err("unterminated string".into());
            };
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err("bad escape".into());
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self.b.get(self.i..self.i + 4).ok_or("bad \\u escape")?;
                            let cp = u32::from_str_radix(
                                std::str::from_utf8(hex).map_err(|e| e.to_string())?,
                                16,
                            )
                            .map_err(|e| e.to_string())?;
                            self.i += 4;
                            // 대리쌍
                            let ch = if (0xD800..0xDC00).contains(&cp)
                                && self.b.get(self.i) == Some(&b'\\')
                                && self.b.get(self.i + 1) == Some(&b'u')
                            {
                                let hex2 =
                                    self.b.get(self.i + 2..self.i + 6).ok_or("bad surrogate")?;
                                let lo = u32::from_str_radix(
                                    std::str::from_utf8(hex2).map_err(|e| e.to_string())?,
                                    16,
                                )
                                .map_err(|e| e.to_string())?;
                                self.i += 6;
                                0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                cp
                            };
                            out.push(char::from_u32(ch).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(format!("bad escape at {}", self.i)),
                    }
                }
                _ => {
                    // UTF-8 바이트 → 문자(멀티바이트는 통째로).
                    let start = self.i - 1;
                    let len = utf8_len(c);
                    let end = (start + len).min(self.b.len());
                    out.push_str(std::str::from_utf8(&self.b[start..end]).unwrap_or("\u{FFFD}"));
                    self.i = end;
                }
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at {}", self.i)),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(items));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            let v = self.value()?;
            items.push((k, v));
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(items));
                }
                _ => return Err(format!("expected ',' or '}}' at {}", self.i)),
            }
        }
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

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
                "Nexa SQL settings — edit and save; the app reloads changed keys immediately. Keys = registry (nsql config list all). Unknown keys are ignored.",
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
    fn parse_round_trip_and_unicode() {
        let v = parse(r#"{"a": {"b": [1, 2.5, "x\ny", true, null]}, "k": "한글 é"}"#).unwrap();
        match v {
            Json::Obj(items) => {
                assert_eq!(items[0].0, "a");
                assert_eq!(items[1].1, Json::Str("한글 é".into()));
            }
            _ => panic!(),
        }
        assert!(parse("{").is_err());
        assert!(parse("{\"a\":1} x").is_err());
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
