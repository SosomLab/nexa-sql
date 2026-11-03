//! parquet 읽기(11-04 · T-8 가져오기 ②) — `parquet` crate를 **이 파일 안에만**(D-432 예외 공유 · arrow 없음 · 행 API `RowIter`) · 행 그룹 단위
//! 스트리밍 · `total_hint` = 파일 메타 행 수. 값 → 글: 정수·실수 그대로 · Decimal = 자리수 복원 · Date/Timestamp = ISO(UTC) · 바이트 = 16진 ·
//! 중첩(그룹·목록·맵) = JSON 글 · NULL = 없음.

use crate::read::{tidy_columns, Record, RecordSource, SourceOpts};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::reader::RowIter;
use parquet::record::Field;
use std::io;
use std::path::Path;

pub(crate) struct ParquetSource {
    cols: Vec<String>,
    iter: RowIter<'static>,
    total: u64,
    row: u64,
    trim: bool,
    null_text: String,
}

impl ParquetSource {
    pub(crate) fn open(path: &Path, opts: &SourceOpts) -> io::Result<Self> {
        let file = std::fs::File::open(path)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
        let reader = SerializedFileReader::new(file)
            .map_err(|e| io::Error::other(format!("{}: parquet: {e}", path.display())))?;
        let md = reader.metadata().file_metadata();
        let total = md.num_rows().max(0) as u64;
        let cols: Vec<String> = md
            .schema_descr()
            .root_schema()
            .get_fields()
            .iter()
            .map(|f| f.name().to_string())
            .collect();
        let iter = RowIter::from_file_into(Box::new(reader));
        Ok(ParquetSource {
            cols: tidy_columns(cols),
            iter,
            total,
            row: 0,
            trim: opts.trim,
            null_text: opts.null_text.clone(),
        })
    }
}

impl RecordSource for ParquetSource {
    fn columns(&mut self) -> io::Result<Vec<String>> {
        Ok(self.cols.clone())
    }

    fn next(&mut self) -> io::Result<Option<Record>> {
        let Some(row) = self.iter.next() else {
            return Ok(None);
        };
        let row = row.map_err(|e| io::Error::other(format!("parquet: {e}")))?;
        self.row += 1;
        let cells = row
            .get_column_iter()
            .map(|(_, f)| {
                field_text(f).and_then(|s| {
                    let s = if self.trim { s.trim().to_string() } else { s };
                    if !self.null_text.is_empty() && s == self.null_text {
                        None
                    } else {
                        Some(s)
                    }
                })
            })
            .collect();
        Ok(Some(Record {
            cells,
            row: self.row,
            line: self.row,
        }))
    }

    fn total_hint(&self) -> Option<u64> {
        Some(self.total)
    }
}

fn field_text(f: &Field) -> Option<String> {
    Some(match f {
        Field::Null => return None,
        Field::Bool(b) => if *b { "true" } else { "false" }.into(),
        Field::Byte(v) => v.to_string(),
        Field::Short(v) => v.to_string(),
        Field::Int(v) => v.to_string(),
        Field::Long(v) => v.to_string(),
        Field::UByte(v) => v.to_string(),
        Field::UShort(v) => v.to_string(),
        Field::UInt(v) => v.to_string(),
        Field::ULong(v) => v.to_string(),
        Field::Float(v) => v.to_string(),
        Field::Double(v) => v.to_string(),
        Field::Str(s) => s.clone(),
        Field::Bytes(b) => b
            .data()
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect::<String>(),
        Field::Date(days) => {
            let (y, m, d) = crate::xlsx_in::civil_from_days(i64::from(*days));
            format!("{y:04}-{m:02}-{d:02}")
        }
        Field::TimestampMillis(ms) => stamp_text(*ms, 1_000),
        Field::TimestampMicros(us) => stamp_text(*us, 1_000_000),
        Field::Decimal(d) => decimal_text(d),
        other => format!("{other}"),
    })
}

/// 유닉스 기원 기준 정수(초당 `per_sec` 단위) → `yyyy-MM-dd HH:mm:ss[.fff]`(UTC).
fn stamp_text(v: i64, per_sec: i64) -> String {
    let secs = v.div_euclid(per_sec);
    let frac = v.rem_euclid(per_sec);
    let days = secs.div_euclid(86_400);
    let s = secs.rem_euclid(86_400);
    let (y, m, d) = crate::xlsx_in::civil_from_days(days);
    let mut out = format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        s / 3600,
        (s % 3600) / 60,
        s % 60
    );
    if frac != 0 {
        let digits = if per_sec == 1_000 { 3 } else { 6 };
        out.push_str(&format!(".{frac:0digits$}"));
    }
    out
}

/// 두 값 보수 빅엔디언 바이트 + scale → 십진 글.
fn decimal_text(d: &parquet::data_type::Decimal) -> String {
    let bytes = d.data();
    let scale = d.scale().max(0) as usize;
    let mut v: i128 = 0;
    if bytes.len() <= 16 {
        let neg = bytes.first().is_some_and(|b| b & 0x80 != 0);
        v = if neg { -1 } else { 0 };
        for b in bytes {
            v = (v << 8) | i128::from(*b);
        }
    }
    let neg = v < 0;
    let mut digits = v.unsigned_abs().to_string();
    if scale > 0 {
        if digits.len() <= scale {
            digits = format!("{}{digits}", "0".repeat(scale + 1 - digits.len()));
        }
        digits.insert(digits.len() - scale, '.');
    }
    if neg {
        format!("-{digits}")
    } else {
        digits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_and_decimals() {
        assert_eq!(stamp_text(0, 1_000), "1970-01-01 00:00:00");
        assert_eq!(
            stamp_text(1_700_000_000_123, 1_000),
            "2023-11-14 22:13:20.123"
        );
        assert_eq!(stamp_text(-1_000, 1_000), "1969-12-31 23:59:59");
        let d = parquet::data_type::Decimal::from_i32(-12345, 9, 2);
        assert_eq!(decimal_text(&d), "-123.45");
        let d2 = parquet::data_type::Decimal::from_i64(5, 9, 3);
        assert_eq!(decimal_text(&d2), "0.005");
    }
}
