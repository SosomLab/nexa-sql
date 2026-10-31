//! parquet 작성기(★ D-432 · 11-01 · T-8) — `parquet` crate를 **이 파일 안에만** 가둔다(DR-3 예외 · 기본 기능 끔 = arrow 없이 저수준 작성기 ·
//! snappy 압축 · 공개 시그니처에 외부 타입 0 · [`crate::StreamWriter`]가 감싼다). **배치 = 행 그룹** 하나 — 행 그룹을 닫을 때마다 바이트를
//! `out`으로 흘려보내므로(작성기는 바이트 수를 스스로 세므로 안의 버퍼를 비워도 오프셋이 맞는다) 수백만 행도 메모리는 한 배치.
//! 열 형 = 첫 배치의 첫 NULL 아닌 값으로 정한다(Int → INT64 · Float/Decimal → DOUBLE · Bool → BOOLEAN · 그 밖·전부 NULL → UTF8 글자) ·
//! 뒤 배치에서 형이 다른 값은 그 형으로 바꿔 보고 안 되면 NULL(이진은 16진 · 커서는 표시 글).

use nsql_core::{RowSource, Value};
use parquet::basic::{Compression, LogicalType, Repetition, Type as PhysicalType};
use parquet::data_type::{BoolType, ByteArray, ByteArrayType, DoubleType, Int64Type};
use parquet::errors::ParquetError;
use parquet::file::properties::WriterProperties;
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::types::Type;
use std::io::{self, Write};
use std::sync::Arc;

fn pe(e: ParquetError) -> io::Error {
    io::Error::other(format!("parquet: {e}"))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Int,
    Float,
    Bool,
    Text,
}

pub(crate) struct ParquetOut {
    w: Option<SerializedFileWriter<Vec<u8>>>,
    kinds: Vec<Kind>,
}

impl std::fmt::Debug for ParquetOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ParquetOut(open={}, kinds={:?})",
            self.w.is_some(),
            self.kinds
        )
    }
}

impl ParquetOut {
    pub(crate) fn new() -> Self {
        ParquetOut {
            w: None,
            kinds: Vec::new(),
        }
    }

    /// 첫 배치에서 열 형을 정하고 파일 머리를 연다.
    fn open<S: RowSource + ?Sized>(&mut self, src: &S) -> io::Result<()> {
        let ncols = src.ncols();
        let mut kinds = vec![Kind::Text; ncols];
        for (c, k) in kinds.iter_mut().enumerate() {
            let first = (0..src.len())
                .map(|r| src.cell(r, c))
                .find(|v| !matches!(v, Value::Null));
            *k = match first {
                Some(Value::Int(_)) => Kind::Int,
                Some(Value::Float(_) | Value::Decimal(_)) => Kind::Float,
                Some(Value::Bool(_)) => Kind::Bool,
                _ => Kind::Text,
            };
        }
        let mut names: Vec<String> = Vec::with_capacity(ncols);
        let mut fields: Vec<Arc<Type>> = Vec::with_capacity(ncols);
        for (c, k) in kinds.iter().enumerate() {
            let name = unique_name(&src.column(c).name, c, &names);
            let b = match k {
                Kind::Int => Type::primitive_type_builder(&name, PhysicalType::INT64),
                Kind::Float => Type::primitive_type_builder(&name, PhysicalType::DOUBLE),
                Kind::Bool => Type::primitive_type_builder(&name, PhysicalType::BOOLEAN),
                Kind::Text => Type::primitive_type_builder(&name, PhysicalType::BYTE_ARRAY)
                    .with_logical_type(Some(LogicalType::String)),
            };
            let t = b
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .map_err(pe)?;
            fields.push(Arc::new(t));
            names.push(name);
        }
        let schema = Type::group_type_builder("nsql")
            .with_fields(fields)
            .build()
            .map_err(pe)?;
        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .set_created_by("Nexa SQL".to_string())
            .build();
        let w =
            SerializedFileWriter::new(Vec::new(), Arc::new(schema), Arc::new(props)).map_err(pe)?;
        self.w = Some(w);
        self.kinds = kinds;
        Ok(())
    }

    /// 배치 하나 = 행 그룹 하나 → 닫고 바이트를 `out`으로.
    pub(crate) fn write_batch<S: RowSource + ?Sized>(
        &mut self,
        out: &mut dyn Write,
        src: &S,
        null: &str,
    ) -> io::Result<()> {
        if self.w.is_none() {
            self.open(src)?;
        }
        let n = src.len();
        let kinds = self.kinds.clone();
        let w = self.w.as_mut().expect("opened");
        if n > 0 {
            let mut rg = w.next_row_group().map_err(pe)?;
            for (c, k) in kinds.iter().enumerate() {
                let mut cw = rg
                    .next_column()
                    .map_err(pe)?
                    .ok_or_else(|| io::Error::other("parquet: column count mismatch"))?;
                let mut defs: Vec<i16> = Vec::with_capacity(n);
                match k {
                    Kind::Int => {
                        let mut vals: Vec<i64> = Vec::with_capacity(n);
                        for r in 0..n {
                            match as_int(src.cell(r, c)) {
                                Some(v) => {
                                    vals.push(v);
                                    defs.push(1);
                                }
                                None => defs.push(0),
                            }
                        }
                        cw.typed::<Int64Type>()
                            .write_batch(&vals, Some(&defs), None)
                            .map_err(pe)?;
                    }
                    Kind::Float => {
                        let mut vals: Vec<f64> = Vec::with_capacity(n);
                        for r in 0..n {
                            match as_float(src.cell(r, c)) {
                                Some(v) => {
                                    vals.push(v);
                                    defs.push(1);
                                }
                                None => defs.push(0),
                            }
                        }
                        cw.typed::<DoubleType>()
                            .write_batch(&vals, Some(&defs), None)
                            .map_err(pe)?;
                    }
                    Kind::Bool => {
                        let mut vals: Vec<bool> = Vec::with_capacity(n);
                        for r in 0..n {
                            match src.cell(r, c) {
                                Value::Bool(b) => {
                                    vals.push(*b);
                                    defs.push(1);
                                }
                                Value::Int(i) => {
                                    vals.push(*i != 0);
                                    defs.push(1);
                                }
                                _ => defs.push(0),
                            }
                        }
                        cw.typed::<BoolType>()
                            .write_batch(&vals, Some(&defs), None)
                            .map_err(pe)?;
                    }
                    Kind::Text => {
                        let mut vals: Vec<ByteArray> = Vec::with_capacity(n);
                        for r in 0..n {
                            match src.cell(r, c) {
                                Value::Null => defs.push(0),
                                v => {
                                    vals.push(ByteArray::from(crate::cell_text(v, null).as_str()));
                                    defs.push(1);
                                }
                            }
                        }
                        cw.typed::<ByteArrayType>()
                            .write_batch(&vals, Some(&defs), None)
                            .map_err(pe)?;
                    }
                }
                cw.close().map_err(pe)?;
            }
            rg.close().map_err(pe)?;
        }
        // 지금까지 쌓인 바이트를 흘려보내고 안의 버퍼는 비운다(작성기의 바이트 수는 그대로 → 꼬리 오프셋 보존).
        w.flush()?;
        let buf = w.inner_mut();
        out.write_all(buf)?;
        buf.clear();
        Ok(())
    }

    /// 꼬리(메타데이터 · `PAR1`) — 배치가 한 번도 없었으면 열 0개 파일은 만들 수 없으므로 빈 파일 대신 오류.
    pub(crate) fn finish(&mut self, out: &mut dyn Write) -> io::Result<()> {
        let Some(w) = self.w.take() else {
            return Err(io::Error::other(
                "parquet: nothing written (no result set) — no file produced",
            ));
        };
        let buf = w.into_inner().map_err(pe)?;
        out.write_all(&buf)
    }
}

fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Int(i) => Some(*i),
        Value::Float(f) if f.is_finite() && f.fract() == 0.0 => Some(*f as i64),
        Value::Decimal(d) | Value::Str(d) => d.trim().parse::<i64>().ok(),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

fn as_float(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => f.is_finite().then_some(*f),
        Value::Decimal(d) | Value::Str(d) => d.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

/// parquet 열 이름 = 비어 있지 않고 유일하게(`col3` · `NAME_2`).
fn unique_name(name: &str, idx: usize, taken: &[String]) -> String {
    let base = if name.trim().is_empty() {
        format!("col{}", idx + 1)
    } else {
        name.to_string()
    };
    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    let mut n = 2;
    loop {
        let cand = format!("{base}_{n}");
        if !taken.iter().any(|t| t == &cand) {
            return cand;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Column, ResultSet};
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use parquet::record::RowAccessor;

    fn col(n: &str) -> Column {
        Column {
            name: n.into(),
            type_name: String::new(),
        }
    }

    fn rs(a: i64) -> ResultSet {
        ResultSet {
            columns: vec![col("ID"), col("NAME"), col("AMT"), col("OK"), col("ID")],
            rows: vec![
                vec![
                    Value::Int(a),
                    Value::Str("홍길동".into()),
                    Value::Decimal("1.5".into()),
                    Value::Bool(true),
                    Value::Null,
                ],
                vec![
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    Value::Bytes(vec![0xAB]),
                ],
            ],
        }
    }

    /// 두 배치 = 행 그룹 2 · 배치마다 바이트가 흘러나옴(중간 출력 > 0) · `PAR1` 머리·꼬리 · 되읽기 = 행 4 · 형(INT64/UTF8/DOUBLE/BOOLEAN) ·
    /// 중복 열 이름 `ID_2` · 전부 NULL인 열은 글자(이진은 16진) · 임시 파일은 temp_dir 아래(실제 설정 폴더에 쓰지 않는다).
    #[test]
    fn writes_row_groups_streaming_and_reads_back() {
        let mut p = ParquetOut::new();
        let mut out: Vec<u8> = Vec::new();
        p.write_batch(&mut out, &rs(1), "").unwrap();
        let mid = out.len();
        assert!(mid > 4, "첫 행 그룹이 바로 흘러나온다");
        p.write_batch(&mut out, &rs(2), "").unwrap();
        assert!(out.len() > mid);
        p.finish(&mut out).unwrap();
        assert!(out.starts_with(b"PAR1") && out.ends_with(b"PAR1"));
        let dir = std::env::temp_dir().join(format!("nsql-io-parquet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.parquet");
        std::fs::write(&path, &out).unwrap();
        let reader = SerializedFileReader::new(std::fs::File::open(&path).unwrap()).unwrap();
        let meta = reader.metadata();
        assert_eq!(meta.num_row_groups(), 2);
        assert_eq!(meta.file_metadata().num_rows(), 4);
        let schema = meta.file_metadata().schema_descr();
        let names: Vec<String> = (0..schema.num_columns())
            .map(|i| schema.column(i).name().to_string())
            .collect();
        assert_eq!(names, ["ID", "NAME", "AMT", "OK", "ID_2"]);
        assert_eq!(schema.column(0).physical_type(), PhysicalType::INT64);
        assert_eq!(schema.column(2).physical_type(), PhysicalType::DOUBLE);
        assert_eq!(schema.column(3).physical_type(), PhysicalType::BOOLEAN);
        assert_eq!(schema.column(4).physical_type(), PhysicalType::BYTE_ARRAY);
        let rows: Vec<_> = reader
            .get_row_iter(None)
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].get_long(0).unwrap(), 1);
        assert_eq!(rows[0].get_string(1).unwrap(), "홍길동");
        assert!((rows[0].get_double(2).unwrap() - 1.5).abs() < 1e-12);
        assert!(rows[0].get_bool(3).unwrap());
        assert_eq!(rows[1].get_string(4).unwrap(), "ab");
        assert_eq!(rows[2].get_long(0).unwrap(), 2);
        let _ = std::fs::remove_dir_all(&dir);
        // 배치 없이 finish = 오류(빈 파일을 만들지 않는다).
        let mut q = ParquetOut::new();
        assert!(q.finish(&mut Vec::new()).is_err());
    }
}
