//! xlsx 작성기(★ D-432 · 11-01 · T-8) — `rust_xlsxwriter`를 **이 파일 안에만** 가둔다(DR-3 예외 · 공개 시그니처에 외부 타입 0 ·
//! [`crate::StreamWriter`]가 감싼다). 배치마다 행을 시트에 보태고 `finish`에서 한 번에 zip으로 써낸다(xlsx는 중앙 디렉터리가 끝에 오는
//! zip이라 진짜 스트리밍이 아니다 — 메모리 = 시트 전체 · 행 상한 1,048,576 = Excel 한계 · 더 크면 csv/parquet).
//! 값 = NULL 빈 칸 · 정수/실수 = 숫자(|정수| > 2^53은 글자로 · 유한하지 않은 실수는 글자) · Decimal = 15자리 안이면 숫자 · 불리언 = 불리언 ·
//! 그 밖(글자·이진 16진·커서) = 글자(32,767자에서 자름 = Excel 셀 한계).

use nsql_core::{RowSource, Value};
use rust_xlsxwriter::{Workbook, XlsxError};
use std::io::{self, Write};

/// Excel 행 상한(1,048,576) · 열 상한(16,384) · 셀 글자 상한.
const ROW_MAX: u32 = 1_048_576;
const COL_MAX: usize = 16_384;
const CELL_CHARS: usize = 32_767;
/// f64로 정확히 담기는 정수 범위.
const EXACT_INT: i64 = 1 << 53;

fn xe(e: XlsxError) -> io::Error {
    io::Error::other(format!("xlsx: {e}"))
}

pub(crate) struct XlsxOut {
    wb: Workbook,
    /// 다음에 쓸 행(0 = 머리 전).
    row: u32,
}

impl std::fmt::Debug for XlsxOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "XlsxOut(row={})", self.row)
    }
}

impl XlsxOut {
    pub(crate) fn new() -> Self {
        let mut wb = Workbook::new();
        wb.add_worksheet();
        XlsxOut { wb, row: 0 }
    }

    /// 배치 하나(첫 배치는 머리 행부터).
    pub(crate) fn write_batch<S: RowSource + ?Sized>(
        &mut self,
        src: &S,
        null: &str,
    ) -> io::Result<()> {
        let ncols = src.ncols();
        if ncols > COL_MAX {
            return Err(io::Error::other(format!(
                "xlsx: too many columns ({ncols} > {COL_MAX})"
            )));
        }
        let ws = self.wb.worksheet_from_index(0).map_err(xe)?;
        if self.row == 0 {
            for c in 0..ncols {
                let name = clip(&src.column(c).name);
                ws.write_string(0, c as u16, name).map_err(xe)?;
            }
            self.row = 1;
        }
        for r in 0..src.len() {
            if self.row >= ROW_MAX {
                return Err(io::Error::other(format!(
                    "xlsx: row limit reached ({ROW_MAX} rows incl. header) — use csv or parquet for larger exports"
                )));
            }
            for (c, v) in src.cells(r).enumerate() {
                let col = c as u16;
                match v {
                    Value::Null => {}
                    Value::Int(i) if i.abs() < EXACT_INT => {
                        ws.write_number(self.row, col, *i as f64).map_err(xe)?;
                    }
                    Value::Float(f) if f.is_finite() => {
                        ws.write_number(self.row, col, *f).map_err(xe)?;
                    }
                    Value::Decimal(d) if decimal_fits(d) => match d.parse::<f64>() {
                        Ok(f) if f.is_finite() => {
                            ws.write_number(self.row, col, f).map_err(xe)?;
                        }
                        _ => {
                            ws.write_string(self.row, col, clip(d)).map_err(xe)?;
                        }
                    },
                    Value::Bool(b) => {
                        ws.write_boolean(self.row, col, *b).map_err(xe)?;
                    }
                    other => {
                        let s = crate::cell_text(other, null);
                        ws.write_string(self.row, col, clip(&s)).map_err(xe)?;
                    }
                }
            }
            self.row += 1;
        }
        Ok(())
    }

    /// zip 전체를 `out`에(한 번).
    pub(crate) fn finish(&mut self, out: &mut dyn Write) -> io::Result<()> {
        let buf = self.wb.save_to_buffer().map_err(xe)?;
        out.write_all(&buf)
    }
}

/// Excel 셀 글자 상한에서 자른다(글자 경계).
fn clip(s: &str) -> &str {
    match s.char_indices().nth(CELL_CHARS) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// 유효 숫자 15자리 안(= f64가 그대로 보존 · Excel 표시 한계)이면 숫자로.
fn decimal_fits(d: &str) -> bool {
    let digits = d
        .trim_start_matches(['-', '+'])
        .chars()
        .filter(char::is_ascii_digit)
        .count();
    digits <= 15 && !d.contains(['e', 'E'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Column, ResultSet};

    fn rs() -> ResultSet {
        ResultSet {
            columns: vec![
                Column {
                    name: "ID".into(),
                    type_name: "NUMBER".into(),
                },
                Column {
                    name: "NAME".into(),
                    type_name: "VARCHAR2".into(),
                },
                Column {
                    name: "OK".into(),
                    type_name: "BOOL".into(),
                },
            ],
            rows: vec![
                vec![
                    Value::Int(1),
                    Value::Str("홍길동".into()),
                    Value::Bool(true),
                ],
                vec![Value::Null, Value::Decimal("12.50".into()), Value::Null],
            ],
        }
    }

    /// 두 배치 → zip(PK 머리 · 시트 1 · 공유 문자열에 이름) · 머리 행 1 + 행 4 · 셀 글자 상한 자르기 · Decimal 판정.
    #[test]
    fn writes_workbook_with_header_and_rows() {
        let mut x = XlsxOut::new();
        x.write_batch(&rs(), "").unwrap();
        x.write_batch(&rs(), "").unwrap();
        assert_eq!(x.row, 5);
        let mut out = Vec::new();
        x.finish(&mut out).unwrap();
        assert!(out.starts_with(b"PK"), "zip");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("xl/worksheets/sheet1.xml"), "sheet entry");
        assert!(clip(&"a".repeat(40_000)).chars().count() == CELL_CHARS);
        assert!(decimal_fits("123456789012345") && !decimal_fits("1234567890123456"));
        assert!(!decimal_fits("1e5"));
    }
}
