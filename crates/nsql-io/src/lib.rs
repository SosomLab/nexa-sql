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

/// 표가 줄 폭을 넘을 때(사용자 09-16 · CLI 결과가 터미널 폭에서 접혀 깨짐).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Overflow {
    /// 컬럼을 **폭에 맞는 묶음**으로 나눠 묶음마다 표를 찍는다(맨 앞에 `#` 행 번호 · sqlplus LINESIZE 줄바꿈의 읽기 쉬운 형태 · 기본).
    #[default]
    Wrap,
    /// 넓은 컬럼부터 줄여 한 줄에 맞춘다(최소 4칸 · 잘린 값은 `…`).
    Truncate,
    /// 레코드 보기 — 행마다 `컬럼 | 값` 세로 나열(psql `\x` · mysql `\G`).
    Expanded,
    /// 그대로(터미널이 접는다 · 파이프/파일용).
    None,
}

impl Overflow {
    /// `wrap` · `truncate` · `expanded`(`x`) · `none`.
    pub fn parse(s: &str) -> Option<Overflow> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "wrap" | "wrapped" => Overflow::Wrap,
            "truncate" | "trunc" | "cut" => Overflow::Truncate,
            "expanded" | "expand" | "x" | "vertical" | "record" => Overflow::Expanded,
            "none" | "off" | "no" => Overflow::None,
            _ => return None,
        })
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Overflow::Wrap => "wrap",
            Overflow::Truncate => "truncate",
            Overflow::Expanded => "expanded",
            Overflow::None => "none",
        }
    }
}

/// 텍스트 표 옵션(CLI `--width`/`--max-col-width`/`--overflow` · 설정 `cli.*` · 셸 `set`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridOpts {
    /// 컬럼 하나의 최대 폭(0 = 무제한 · sqlcmd `-y`).
    pub max_col: usize,
    /// 줄 폭(0 = 무제한 · sqlplus LINESIZE · CLI는 터미널 폭 자동).
    pub line_width: usize,
    /// 줄 폭을 넘을 때.
    pub overflow: Overflow,
}

impl Default for GridOpts {
    /// 종전 동작(컬럼 60 · 줄 폭 무제한).
    fn default() -> Self {
        GridOpts {
            max_col: 60,
            line_width: 0,
            overflow: Overflow::None,
        }
    }
}

/// 결과 집합을 `out`에 쓴다(표는 기본 옵션 = 컬럼 60 · 줄 폭 무제한).
pub fn write_result_set(
    out: &mut dyn Write,
    rs: &ResultSet,
    fmt: &Format,
    dialect: Dialect,
) -> io::Result<()> {
    write_result_set_opts(out, rs, fmt, dialect, &GridOpts::default())
}

/// 결과 집합을 `out`에 쓴다 — 표 형식은 `grid` 옵션(폭·넘침)을 따른다.
pub fn write_result_set_opts(
    out: &mut dyn Write,
    rs: &ResultSet,
    fmt: &Format,
    dialect: Dialect,
    grid: &GridOpts,
) -> io::Result<()> {
    match fmt {
        Format::Grid => out.write_all(format_grid_opts(rs, grid).as_bytes()),
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

/// 텍스트 표 — 컬럼 폭은 셀 최대 폭(`max_col` 상한 · 한글 등 전각은 폭 2로 센다). 줄 폭 무제한.
pub fn format_grid(rs: &ResultSet, max_col: usize) -> String {
    format_grid_opts(
        rs,
        &GridOpts {
            max_col,
            line_width: 0,
            overflow: Overflow::None,
        },
    )
}

const SEP: usize = 2;
const MIN_COL: usize = 4;

/// 텍스트 표 — 옵션(컬럼 상한 · 줄 폭 · 넘침 처리).
pub fn format_grid_opts(rs: &ResultSet, o: &GridOpts) -> String {
    let n = rs.columns.len();
    let cap = |w: usize| if o.max_col > 0 { w.min(o.max_col) } else { w };
    let mut widths: Vec<usize> = rs
        .columns
        .iter()
        .map(|c| cap(disp_width(&c.name)))
        .collect();
    let cells: Vec<Vec<String>> = rs
        .rows
        .iter()
        .map(|r| r.iter().map(cell_text).collect())
        .collect();
    for row in &cells {
        for (i, c) in row.iter().enumerate().take(n) {
            widths[i] = cap(widths[i].max(disp_width(c)));
        }
    }
    let right: Vec<bool> = (0..n)
        .map(|i| {
            matches!(
                rs.rows.first().and_then(|r| r.get(i)),
                Some(Value::Int(_) | Value::Float(_) | Value::Decimal(_))
            )
        })
        .collect();
    let total = |ws: &[usize]| ws.iter().sum::<usize>() + SEP * ws.len().saturating_sub(1);
    let fits = o.line_width == 0 || n == 0 || total(&widths) <= o.line_width;
    let mode = if fits { Overflow::None } else { o.overflow };
    match mode {
        Overflow::None => {
            let cols: Vec<usize> = (0..n).collect();
            render_table(rs, &cells, &widths, &right, &cols, None)
        }
        Overflow::Truncate => {
            // 가장 넓은 컬럼부터 1칸씩 줄인다(최소 4칸).
            let mut ws = widths.clone();
            while total(&ws) > o.line_width {
                let Some((i, _)) = ws
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| **w > MIN_COL)
                    .max_by_key(|(_, w)| **w)
                else {
                    break;
                };
                ws[i] -= 1;
            }
            let cols: Vec<usize> = (0..n).collect();
            render_table(rs, &cells, &ws, &right, &cols, None)
        }
        Overflow::Expanded => render_expanded(rs, &cells, o.max_col),
        Overflow::Wrap => {
            // 행 번호 열 + 폭에 맞는 컬럼 묶음(한 컬럼이 줄 폭을 넘으면 그 컬럼만 잘라서).
            let rn_w = cells.len().to_string().len().max(1);
            let room = o.line_width.saturating_sub(rn_w + SEP).max(MIN_COL);
            let mut ws = widths.clone();
            for w in &mut ws {
                *w = (*w).min(room);
            }
            let mut groups: Vec<Vec<usize>> = Vec::new();
            let mut cur: Vec<usize> = Vec::new();
            let mut cur_w = rn_w;
            for (i, w) in ws.iter().enumerate() {
                let add = SEP + *w;
                if !cur.is_empty() && cur_w + add > o.line_width {
                    groups.push(std::mem::take(&mut cur));
                    cur_w = rn_w;
                }
                cur.push(i);
                cur_w += add;
            }
            if !cur.is_empty() {
                groups.push(cur);
            }
            let mut out = String::new();
            for (gi, g) in groups.iter().enumerate() {
                if gi > 0 {
                    out.push('\n');
                }
                out.push_str(&render_table(rs, &cells, &ws, &right, g, Some(rn_w)));
            }
            out
        }
    }
}

/// 표 하나 — `cols` 순서의 컬럼만 · `rownum` = 행 번호 열 폭(`#`).
fn render_table(
    rs: &ResultSet,
    cells: &[Vec<String>],
    widths: &[usize],
    right: &[bool],
    cols: &[usize],
    rownum: Option<usize>,
) -> String {
    let mut o = String::new();
    let put = |o: &mut String, text: &str, w: usize, right: bool| {
        let text = truncate_width(text, w);
        let pad = w.saturating_sub(disp_width(&text));
        if right {
            o.push_str(&" ".repeat(pad));
            o.push_str(&text);
        } else {
            o.push_str(&text);
            o.push_str(&" ".repeat(pad));
        }
    };
    let line = |o: &mut String, first: Option<&str>, get: &dyn Fn(usize) -> String, rule: bool| {
        if let Some(w) = rownum {
            match first {
                Some(f) => put(o, f, w, true),
                None => o.push_str(&"-".repeat(w)),
            }
            o.push_str(&" ".repeat(SEP));
        }
        for (k, &i) in cols.iter().enumerate() {
            if k > 0 {
                o.push_str(&" ".repeat(SEP));
            }
            if rule {
                o.push_str(&"-".repeat(widths[i]));
            } else {
                put(o, &get(i), widths[i], right[i]);
            }
        }
        o.push('\n');
    };
    line(&mut o, Some("#"), &|i| rs.columns[i].name.clone(), false);
    line(&mut o, None, &|_| String::new(), true);
    for (r, row) in cells.iter().enumerate() {
        let num = (r + 1).to_string();
        line(
            &mut o,
            Some(&num),
            &|i| row.get(i).cloned().unwrap_or_default(),
            false,
        );
    }
    o
}

/// 레코드 보기 — `-[ RECORD n ]-` 머리 + `컬럼 | 값`(psql `\x`).
fn render_expanded(rs: &ResultSet, cells: &[Vec<String>], max_col: usize) -> String {
    let name_w = rs
        .columns
        .iter()
        .map(|c| disp_width(&c.name))
        .max()
        .unwrap_or(0);
    let mut o = String::new();
    for (r, row) in cells.iter().enumerate() {
        let head = format!("-[ RECORD {} ]", r + 1);
        o.push_str(&head);
        o.push_str(&"-".repeat((name_w + 3).saturating_sub(head.len()).max(4)));
        o.push('\n');
        for (i, c) in rs.columns.iter().enumerate() {
            let v = row.get(i).cloned().unwrap_or_default();
            let v = if max_col > 0 {
                truncate_width(&v, max_col)
            } else {
                v
            };
            let pad = name_w.saturating_sub(disp_width(&c.name));
            o.push_str(&c.name);
            o.push_str(&" ".repeat(pad));
            o.push_str(" | ");
            o.push_str(&v);
            o.push('\n');
        }
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

#[cfg(test)]
mod grid_width_tests {
    use super::*;
    use nsql_core::Column;

    fn rs() -> ResultSet {
        ResultSet {
            columns: (0..6)
                .map(|i| Column {
                    name: format!("COL_{i}"),
                    type_name: "VARCHAR2".into(),
                })
                .collect(),
            rows: (1..=12)
                .map(|r| {
                    (0..6)
                        .map(|c| Value::Str(format!("value{r}_{c}_{}", "x".repeat(c * 3))))
                        .collect()
                })
                .collect(),
        }
    }

    fn max_line(s: &str) -> usize {
        s.lines().map(disp_width).max().unwrap_or(0)
    }

    #[test]
    fn wrap_splits_into_groups_within_width() {
        let o = GridOpts {
            max_col: 60,
            line_width: 40,
            overflow: Overflow::Wrap,
        };
        let out = format_grid_opts(&rs(), &o);
        assert!(max_line(&out) <= 40, "모든 줄이 폭 안: {}", max_line(&out));
        // 묶음마다 `#` 열 · 빈 줄로 구분 · 12행이 묶음마다 반복.
        assert!(
            out.lines()
                .filter(|l| l.starts_with(" 1  ") || l.starts_with("1  "))
                .count()
                >= 2
        );
        assert!(out.contains("COL_5"));
    }

    #[test]
    fn truncate_fits_width_and_marks_cut() {
        let o = GridOpts {
            max_col: 0,
            line_width: 50,
            overflow: Overflow::Truncate,
        };
        let out = format_grid_opts(&rs(), &o);
        assert!(max_line(&out) <= 50);
        assert!(out.contains('…'));
    }

    #[test]
    fn expanded_lists_records() {
        let o = GridOpts {
            max_col: 0,
            line_width: 10,
            overflow: Overflow::Expanded,
        };
        let out = format_grid_opts(&rs(), &o);
        assert!(out.starts_with("-[ RECORD 1 ]"));
        assert_eq!(out.matches("-[ RECORD ").count(), 12);
        assert!(out.contains("COL_0 | value1_0_"));
    }

    #[test]
    fn fits_or_none_keeps_single_table() {
        let o = GridOpts {
            max_col: 60,
            line_width: 10_000,
            overflow: Overflow::Wrap,
        };
        let out = format_grid_opts(&rs(), &o);
        assert_eq!(out, format_grid(&rs(), 60));
        let o = GridOpts {
            max_col: 60,
            line_width: 40,
            overflow: Overflow::None,
        };
        assert_eq!(format_grid_opts(&rs(), &o), format_grid(&rs(), 60));
    }
}
