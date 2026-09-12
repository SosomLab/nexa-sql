//! # nsql-io — 결과 집합 직렬화(export) · CSV 파싱(import 원료). DB·화면 무의존.
//!
//! CLI `SET SQLFORMAT`·`nsql export`·GUI 내보내기가 **같은 작성기**를 쓴다(docs/11 §3).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{Dialect, ResultSet, Value};
use std::io::{self, Write};

/// 출력 형식.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Format {
    /// 사람이 읽는 텍스트 표(CLI 기본).
    Grid,
    Csv,
    Tsv,
    /// JSON 배열 `[{col: val}, …]`.
    Json,
    /// 줄당 객체 하나.
    JsonLines,
    /// `INSERT INTO table (cols) VALUES (…);` 행마다.
    Insert {
        table: String,
    },
}

impl Format {
    /// `csv` · `tsv` · `json` · `jsonl`/`jsonlines` · `insert[:table]` · `grid`/`default`.
    pub fn parse(s: &str) -> Option<Format> {
        let s = s.trim();
        let low = s.to_ascii_lowercase();
        Some(match low.as_str() {
            "grid" | "default" | "table" | "ansiconsole" => Format::Grid,
            "csv" => Format::Csv,
            "tsv" | "delimited" => Format::Tsv,
            "json" => Format::Json,
            "jsonl" | "jsonlines" | "ndjson" => Format::JsonLines,
            _ => {
                let t = low.strip_prefix("insert")?;
                let table = t.trim_start_matches(':').trim();
                Format::Insert {
                    table: if table.is_empty() {
                        "T".into()
                    } else {
                        s[s.len() - table.len()..].to_string()
                    },
                }
            }
        })
    }
}

/// 결과 집합을 `out`에 쓴다.
pub fn write_result_set(
    out: &mut dyn Write,
    rs: &ResultSet,
    fmt: &Format,
    dialect: Dialect,
) -> io::Result<()> {
    match fmt {
        Format::Grid => out.write_all(format_grid(rs, 60).as_bytes()),
        Format::Csv => write_delimited(out, rs, b','),
        Format::Tsv => write_delimited(out, rs, b'\t'),
        Format::Json => {
            out.write_all(b"[")?;
            for (i, row) in rs.rows.iter().enumerate() {
                if i > 0 {
                    out.write_all(b",")?;
                }
                out.write_all(b"\n  ")?;
                write_json_row(out, rs, row)?;
            }
            out.write_all(if rs.rows.is_empty() { b"]\n" } else { b"\n]\n" })
        }
        Format::JsonLines => {
            for row in &rs.rows {
                write_json_row(out, rs, row)?;
                out.write_all(b"\n")?;
            }
            Ok(())
        }
        Format::Insert { table } => {
            let cols: Vec<String> = rs.columns.iter().map(|c| c.name.clone()).collect();
            for row in &rs.rows {
                let vals: Vec<String> = row.iter().map(|v| v.to_sql_literal(dialect)).collect();
                writeln!(
                    out,
                    "INSERT INTO {table} ({}) VALUES ({});",
                    cols.join(", "),
                    vals.join(", ")
                )?;
            }
            Ok(())
        }
    }
}

fn write_delimited(out: &mut dyn Write, rs: &ResultSet, delim: u8) -> io::Result<()> {
    let d = delim as char;
    let header: Vec<String> = rs
        .columns
        .iter()
        .map(|c| quote_field(&c.name, delim))
        .collect();
    writeln!(out, "{}", header.join(&d.to_string()))?;
    for row in &rs.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|v| quote_field(&cell_text(v), delim))
            .collect();
        writeln!(out, "{}", cells.join(&d.to_string()))?;
    }
    Ok(())
}

/// RFC 4180 — 구분자·따옴표·줄바꿈이 있으면 `"…"`로 감싸고 `"`는 `""`.
pub fn quote_field(s: &str, delim: u8) -> String {
    let needs = s
        .bytes()
        .any(|b| b == delim || b == b'"' || b == b'\n' || b == b'\r');
    if needs {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bytes(b) => b.iter().map(|x| format!("{x:02x}")).collect(),
        other => other.display(),
    }
}

fn write_json_row(out: &mut dyn Write, rs: &ResultSet, row: &[Value]) -> io::Result<()> {
    out.write_all(b"{")?;
    for (i, (c, v)) in rs.columns.iter().zip(row.iter()).enumerate() {
        if i > 0 {
            out.write_all(b", ")?;
        }
        write!(out, "{}: {}", json_str(&c.name), json_value(v))?;
    }
    out.write_all(b"}")
}

pub fn json_str(s: &str) -> String {
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

pub fn json_value(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) if f.is_finite() => format!("{f:?}"),
        Value::Float(_) => "null".into(),
        Value::Decimal(d) => d.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => json_str(s),
        Value::Bytes(b) => json_str(&b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
        Value::Cursor(c) => json_str(&format!("<refcursor #{}>", c.0)),
    }
}

/// 텍스트 표 — 컬럼 폭은 셀 최대 폭(`max_col` 상한 · 한글 등 전각은 폭 2로 센다).
pub fn format_grid(rs: &ResultSet, max_col: usize) -> String {
    let n = rs.columns.len();
    let mut widths: Vec<usize> = rs.columns.iter().map(|c| disp_width(&c.name)).collect();
    let cells: Vec<Vec<String>> = rs
        .rows
        .iter()
        .map(|r| r.iter().map(cell_text).collect())
        .collect();
    for row in &cells {
        for (i, c) in row.iter().enumerate().take(n) {
            widths[i] = widths[i].max(disp_width(c)).min(max_col);
        }
    }
    let mut o = String::new();
    let line = |o: &mut String, cells: &[String]| {
        for (i, c) in cells.iter().enumerate().take(n) {
            let text = truncate_width(c, widths[i]);
            let pad = widths[i].saturating_sub(disp_width(&text));
            if i > 0 {
                o.push_str("  ");
            }
            let right = matches!(
                rs.rows.first().and_then(|r| r.get(i)),
                Some(Value::Int(_) | Value::Float(_) | Value::Decimal(_))
            );
            if right {
                o.push_str(&" ".repeat(pad));
                o.push_str(&text);
            } else {
                o.push_str(&text);
                o.push_str(&" ".repeat(pad));
            }
        }
        o.push('\n');
    };
    let header: Vec<String> = rs.columns.iter().map(|c| c.name.clone()).collect();
    line(&mut o, &header);
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    line(&mut o, &rule);
    for row in &cells {
        line(&mut o, row);
    }
    o
}

/// 표시 폭 — CJK 전각·한글은 2.
pub fn disp_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn char_width(c: char) -> usize {
    let u = c as u32;
    if (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE30..=0xFE4F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x20000..=0x3FFFD).contains(&u)
    {
        2
    } else if u < 0x20 {
        0
    } else {
        1
    }
}

fn truncate_width(s: &str, max: usize) -> String {
    let mut w = 0;
    let mut o = String::new();
    for c in s.chars() {
        let cw = char_width(c);
        if w + cw > max {
            if max >= 1 {
                o.pop();
                o.push('…');
            }
            return o;
        }
        w += cw;
        o.push(c);
    }
    o
}

/// CSV 파서(RFC 4180 · `""` 이스케이프 · 따옴표 안 줄바꿈) — import·bulk의 원료. 구분자 지정.
pub fn parse_delimited(text: &str, delim: u8) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_q = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_q = true,
            c if c as u32 == delim as u32 => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::Column;

    fn rs() -> ResultSet {
        ResultSet {
            columns: vec![
                Column {
                    name: "ID".into(),
                    type_name: "int".into(),
                },
                Column {
                    name: "이름".into(),
                    type_name: "text".into(),
                },
            ],
            rows: vec![
                vec![Value::Int(1), Value::Str("홍길동".into())],
                vec![Value::Int(22), Value::Str("a,\"b\"".into())],
                vec![Value::Null, Value::Null],
            ],
        }
    }

    #[test]
    fn csv_quotes_and_nulls() {
        let mut out = Vec::new();
        write_result_set(&mut out, &rs(), &Format::Csv, Dialect::Oracle).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "ID,이름\n1,홍길동\n22,\"a,\"\"b\"\"\"\n,\n"
        );
    }

    #[test]
    fn json_and_jsonl() {
        let mut out = Vec::new();
        write_result_set(&mut out, &rs(), &Format::JsonLines, Dialect::Oracle).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(
            s.lines().next().unwrap(),
            "{\"ID\": 1, \"이름\": \"홍길동\"}"
        );
        assert_eq!(s.lines().nth(2).unwrap(), "{\"ID\": null, \"이름\": null}");
        let mut out = Vec::new();
        write_result_set(&mut out, &rs(), &Format::Json, Dialect::Oracle).unwrap();
        assert!(String::from_utf8(out)
            .unwrap()
            .starts_with("[\n  {\"ID\": 1"));
    }

    #[test]
    fn insert_uses_dialect_literals() {
        let mut out = Vec::new();
        write_result_set(
            &mut out,
            &rs(),
            &Format::Insert {
                table: "EMP".into(),
            },
            Dialect::Mssql,
        )
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(
            s.lines().next().unwrap(),
            "INSERT INTO EMP (ID, 이름) VALUES (1, N'홍길동');"
        );
        assert_eq!(
            s.lines().nth(2).unwrap(),
            "INSERT INTO EMP (ID, 이름) VALUES (NULL, NULL);"
        );
    }

    #[test]
    fn grid_aligns_with_cjk_width() {
        let g = format_grid(&rs(), 60);
        let lines: Vec<&str> = g.lines().collect();
        assert_eq!(lines[0], "ID  이름  ");
        assert_eq!(lines[1], "--  ------");
        assert_eq!(lines[2], " 1  홍길동");
        assert_eq!(disp_width("홍길동"), 6);
    }

    #[test]
    fn csv_parse_round_trip() {
        let rows = parse_delimited("a,b\n1,\"x,\"\"y\"\"\nz\"\n,\n", b',');
        assert_eq!(
            rows,
            vec![vec!["a", "b"], vec!["1", "x,\"y\"\nz"], vec!["", ""]]
        );
        assert_eq!(
            Format::parse("insert:EMP"),
            Some(Format::Insert {
                table: "EMP".into()
            })
        );
        assert_eq!(Format::parse("JSONL"), Some(Format::JsonLines));
    }
}
