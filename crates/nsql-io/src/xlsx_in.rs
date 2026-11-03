//! xlsx 읽기(★ D-449 · 11-04 · T-8 가져오기 ②) — `calamine` crate를 **이 파일 안에만** 가둔다(DR-3 예외 원장 10 §4 · 순수 Rust · xlsx/xlsm/xls/xlsb/ods ·
//! chrono 없음 = 날짜 시리얼을 직접 환산). 시트 범위를 메모리에 올린 뒤 행을 하나씩 돌려준다(셀 수 비례 · `total_hint` = 행 수).
//! 셀 → 글: 정수 = 그대로 · 실수 = 소수 자리 없으면 정수 글 · 날짜/일시 = ISO(`yyyy-MM-dd` · `yyyy-MM-dd HH:mm:ss`) · 기간 = `HH:mm:ss` ·
//! 불 = `true`/`false` · 오류 셀 = NULL · 빈 셀 = NULL · 끝의 전부 빈 행은 버린다.

use crate::read::{tidy_columns, Detected, Record, RecordSource, SourceOpts};
use calamine::{open_workbook_auto, Data, Range, Reader};
use std::io;
use std::path::Path;

pub(crate) struct XlsxSource {
    cols: Vec<String>,
    rows: std::vec::IntoIter<Vec<Option<String>>>,
    total: u64,
    row: u64,
    sheet: String,
    sheets: Vec<String>,
}

impl XlsxSource {
    pub(crate) fn open(path: &Path, opts: &SourceOpts) -> io::Result<Self> {
        let mut wb = open_workbook_auto(path)
            .map_err(|e| io::Error::other(format!("{}: xlsx: {e}", path.display())))?;
        let sheets: Vec<String> = wb.sheet_names().to_vec();
        if sheets.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: xlsx: no sheets", path.display()),
            ));
        }
        let sheet = match opts.sheet.as_deref().map(str::trim) {
            None | Some("") => sheets[0].clone(),
            Some(s) => {
                if let Some(found) = sheets.iter().find(|n| n.eq_ignore_ascii_case(s)) {
                    found.clone()
                } else if let Some(i) = s.parse::<usize>().ok().filter(|i| *i < sheets.len()) {
                    sheets[i].clone()
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!(
                            "xlsx: sheet '{s}' not found (sheets: {})",
                            sheets.join(", ")
                        ),
                    ));
                }
            }
        };
        let range: Range<Data> = wb
            .worksheet_range(&sheet)
            .map_err(|e| io::Error::other(format!("xlsx: sheet '{sheet}': {e}")))?;
        let mut rows: Vec<Vec<Option<String>>> = range
            .rows()
            .map(|r| r.iter().map(cell_text).collect())
            .collect();
        while rows.last().is_some_and(|r| r.iter().all(Option::is_none)) {
            rows.pop();
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let cols = if opts.header && !rows.is_empty() {
            let head = rows.remove(0);
            let mut names: Vec<String> = head.into_iter().map(|c| c.unwrap_or_default()).collect();
            names.resize(width, String::new());
            tidy_columns(names)
        } else {
            (1..=width).map(|i| format!("col{i}")).collect()
        };
        let rows: Vec<Vec<Option<String>>> = rows
            .into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|c| c.and_then(|s| apply_opts(s, opts)))
                    .collect()
            })
            .collect();
        Ok(XlsxSource {
            cols,
            total: rows.len() as u64,
            rows: rows.into_iter(),
            row: 0,
            sheet,
            sheets,
        })
    }
}

fn apply_opts(s: String, opts: &SourceOpts) -> Option<String> {
    let s = if opts.trim { s.trim().to_string() } else { s };
    if !opts.null_text.is_empty() && s == opts.null_text {
        None
    } else {
        Some(s)
    }
}

impl RecordSource for XlsxSource {
    fn columns(&mut self) -> io::Result<Vec<String>> {
        Ok(self.cols.clone())
    }

    fn next(&mut self) -> io::Result<Option<Record>> {
        Ok(self.rows.next().map(|cells| {
            self.row += 1;
            Record {
                cells,
                row: self.row,
                line: self.row,
            }
        }))
    }

    fn total_hint(&self) -> Option<u64> {
        Some(self.total)
    }

    fn detected(&self) -> Detected {
        Detected {
            sheet: Some(self.sheet.clone()),
            sheets: self.sheets.clone(),
            ..Detected::default()
        }
    }
}

/// 셀 → 글(없음 = NULL).
fn cell_text(d: &Data) -> Option<String> {
    match d {
        Data::Empty | Data::Error(_) => None,
        Data::Int(i) => Some(i.to_string()),
        Data::Float(f) => Some(float_text(*f)),
        Data::String(s) => Some(s.clone()),
        Data::Bool(b) => Some(if *b { "true" } else { "false" }.into()),
        Data::DateTime(dt) => Some(serial_text(dt.as_f64(), dt.is_duration())),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Some(s.clone()),
    }
}

/// 실수 → 글(정수면 소수점 없이 · 그 밖은 Rust 최단 표기).
fn float_text(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        f.to_string()
    }
}

/// Excel 시리얼(1900 체계 · 1899-12-30 기준 일수 + 하루의 분수) → ISO 글 · 기간 = 총 시:분:초.
pub(crate) fn serial_text(v: f64, duration: bool) -> String {
    let total_secs = (v * 86_400.0).round() as i64;
    if duration {
        let (h, m, s) = (total_secs / 3600, (total_secs % 3600) / 60, total_secs % 60);
        return format!("{h:02}:{m:02}:{s:02}");
    }
    let days = total_secs.div_euclid(86_400);
    let secs = total_secs.rem_euclid(86_400);
    // 1899-12-30 = 유닉스 기원 1970-01-01에서 -25569일.
    let (y, mo, d) = civil_from_days(days - 25_569);
    if secs == 0 {
        format!("{y:04}-{mo:02}-{d:02}")
    } else {
        format!(
            "{y:04}-{mo:02}-{d:02} {:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    }
}

/// 유닉스 기원 기준 일수 → (년, 월, 일) — Howard Hinnant `civil_from_days`.
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excel_serial_dates() {
        assert_eq!(serial_text(45_000.0, false), "2023-03-15");
        assert_eq!(serial_text(45_000.5, false), "2023-03-15 12:00:00");
        assert_eq!(serial_text(1.0, false), "1899-12-31");
        assert_eq!(serial_text(1.5, true), "36:00:00");
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn float_text_trims_integral() {
        assert_eq!(float_text(3.0), "3");
        assert_eq!(float_text(3.25), "3.25");
        assert_eq!(float_text(-0.0), "0");
    }
}
