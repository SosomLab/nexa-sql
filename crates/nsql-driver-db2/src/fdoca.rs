//! FD:OCA — 질의 서술자(QRYDSC) · 행 데이터(QRYDTA) · 열 메타(SQLDARD) · 입력 데이터(SQLDTA). 규칙 = pydrda `utils.read_field`/`ddm.packSQLDTA`(Db2 LUW 실측) ·
//! Derby `FdocaConstants`/`DRDAConstants`.
//!
//! - QRYDSC = `len(1) 0x76 0xD0 [타입(1) 길이(2)]×열` … (84열 넘으면 트리플릿 여럿 · 0x7F = 이어짐).
//! - QRYDTA 행 = `0xFF 0x00`(SQLCAGRP 없음 · 행 그룹) + 열 값. 널 허용 타입(홀수) = 지시자 1바이트(0xFF = NULL · 값 생략).
//! - 정수·실수 = 서버 TYPDEFNAM 순서(x86 LUW = 작은 끝) · **가변 길이의 길이 접두는 큰 끝**(pydrda 실측) · 문자 = UTF-8(CCSID 1208).
//! - 입력(SQLDTA) = FDODSC(`[(1+n)*3] 76 D0 [타입 길이]×n 06 71 E4 D0 00 01`) + FDODTA(행 지시자 0x00 + 매개변수마다 널 지시자 + 값) · 문자열은
//!   VARGRAPHIC(0x39 · UTF-16BE · 2B 큰 끝 길이)로 보낸다(pydrda).

use nsql_core::{DbError, Value};

/// QRYDSC 열 하나(DRDA 타입 + 2바이트 길이/정밀도·스케일).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColDesc {
    pub drda_type: u8,
    pub len: [u8; 2],
}

impl ColDesc {
    #[must_use]
    pub fn nullable(self) -> bool {
        self.drda_type & 1 == 1
    }
    #[must_use]
    pub fn base(self) -> u8 {
        self.drda_type & !1
    }
    #[must_use]
    pub fn len_u16(self) -> usize {
        u16::from_be_bytes(self.len) as usize
    }
}

fn err(msg: impl Into<String>) -> DbError {
    DbError {
        code: None,
        message: msg.into(),
        position: None,
    }
}

/// QRYDSC 객체 값 → 열 서술자 목록.
pub fn parse_qrydsc(obj: &[u8]) -> Result<Vec<ColDesc>, DbError> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < obj.len() {
        let ln = obj[at] as usize;
        if ln < 3 || at + ln > obj.len() {
            return Err(err(format!("DRDA QRYDSC: bad triplet length {ln} at {at}")));
        }
        let t = obj[at + 1];
        let body = &obj[at + 3..at + ln];
        // 0x76 = NGDA(열 묶음 · id 0xD0) · 0x7F = CPT(이어지는 열) — 둘 다 (타입·길이2) 반복.
        if t == 0x76 || t == 0x7F {
            for c in body.as_chunks::<3>().0 {
                out.push(ColDesc {
                    drda_type: c[0],
                    len: [c[1], c[2]],
                });
            }
        }
        at += ln;
    }
    Ok(out)
}

/// 행 바이트 읽기 커서 — 부족하면 `Truncated`(행이 다음 QRYDTA 블록으로 이어짐).
#[derive(Debug)]
pub struct Reader<'a> {
    pub b: &'a [u8],
    pub pos: usize,
}

#[derive(Debug)]
pub enum RowErr {
    Truncated,
    Bad(DbError),
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], RowErr> {
        if self.pos + n > self.b.len() {
            return Err(RowErr::Truncated);
        }
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, RowErr> {
        Ok(self.take(1)?[0])
    }
    fn len_be16(&mut self) -> Result<usize, RowErr> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]) as usize)
    }
}

fn int_from(b: &[u8], le: bool) -> i64 {
    let mut v: i64 = 0;
    if le {
        for &x in b.iter().rev() {
            v = (v << 8) | i64::from(x);
        }
    } else {
        for &x in b {
            v = (v << 8) | i64::from(x);
        }
    }
    // 부호 확장.
    let bits = b.len() * 8;
    if bits < 64 && (v >> (bits - 1)) & 1 == 1 {
        v -= 1i64 << bits;
    }
    v
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// packed BCD → 십진 문자열(`-12.34`).
fn packed_decimal(b: &[u8], scale: usize) -> String {
    let mut digits = String::with_capacity(b.len() * 2);
    for (i, &x) in b.iter().enumerate() {
        digits.push(char::from(b'0' + (x >> 4)));
        if i + 1 < b.len() {
            digits.push(char::from(b'0' + (x & 0x0F)));
        }
    }
    let sign = b.last().map_or(0x0C, |x| x & 0x0F);
    let neg = sign == 0x0D || sign == 0x0B;
    let trimmed = digits.trim_start_matches('0');
    let mut s = if scale == 0 {
        trimmed.to_string()
    } else if trimmed.len() <= scale {
        format!("0.{:0>width$}", trimmed, width = scale)
    } else {
        let (a, f) = trimmed.split_at(trimmed.len() - scale);
        format!("{a}.{f}")
    };
    if s.is_empty() {
        s = "0".into();
    }
    if neg && s.trim_start_matches(['0', '.']).is_empty() {
        s
    } else if neg {
        format!("-{s}")
    } else {
        s
    }
}

/// DPD(Densely Packed Decimal) 10비트 → 세 자리(IEEE 754-2008 표 · 자체 구현).
fn dpd_decode(d: u16) -> (u8, u8, u8) {
    let b = |i: u16| ((d >> i) & 1) as u8;
    let (b9, b8, b7, b6, b5, b4, b3, b2, b1, b0) =
        (b(9), b(8), b(7), b(6), b(5), b(4), b(3), b(2), b(1), b(0));
    if b3 == 0 {
        return (
            b9 * 4 + b8 * 2 + b7,
            b6 * 4 + b5 * 2 + b4,
            b2 * 4 + b1 * 2 + b0,
        );
    }
    match (b2, b1) {
        (0, 0) => (b9 * 4 + b8 * 2 + b7, b6 * 4 + b5 * 2 + b4, 8 + b0),
        (0, 1) => (b9 * 4 + b8 * 2 + b7, 8 + b4, b6 * 4 + b5 * 2 + b0),
        (1, 0) => (8 + b7, b6 * 4 + b5 * 2 + b4, b9 * 4 + b8 * 2 + b0),
        _ => match (b6, b5) {
            (0, 0) => (8 + b7, 8 + b4, b9 * 4 + b8 * 2 + b0),
            (0, 1) => (8 + b7, b9 * 4 + b8 * 2 + b4, 8 + b0),
            (1, 0) => (b9 * 4 + b8 * 2 + b7, 8 + b4, 8 + b0),
            _ => (8 + b7, 8 + b4, 8 + b0),
        },
    }
}

/// DECFLOAT(16/34) 바이트(큰 끝 · DPD) → 십진 문자열.
fn decfloat(b: &[u8]) -> String {
    let (bias, groups, exp_bits): (i64, usize, u32) = if b.len() == 8 {
        (398, 5, 8)
    } else {
        (6176, 11, 12)
    };
    let mut w: u128 = 0;
    for &x in b {
        w = (w << 8) | u128::from(x);
    }
    let total = (b.len() * 8) as u32;
    let sign = (w >> (total - 1)) & 1 == 1;
    let g = ((w >> (total - 6)) & 0x1F) as u32;
    let coeff_bits = (groups * 10) as u32;
    let e = ((w >> coeff_bits) & ((1u128 << exp_bits) - 1)) as i64;
    let t = w & ((1u128 << coeff_bits) - 1);
    if g >= 0x1C {
        return if g & 0x02 != 0 {
            "NaN".into()
        } else if sign {
            "-Infinity".into()
        } else {
            "Infinity".into()
        };
    }
    let (biased, lead) = if g >= 0x18 {
        (
            (i64::from(g & 0x03) << exp_bits) | e,
            8 + ((g >> 2) & 1) as u8,
        )
    } else {
        ((i64::from(g >> 3) << exp_bits) | e, (g & 0x07) as u8)
    };
    let mut digits = vec![lead];
    for i in (0..groups).rev() {
        let (d2, d1, d0) = dpd_decode(((t >> (i * 10)) & 0x3FF) as u16);
        digits.extend([d2, d1, d0]);
    }
    let exp = biased - bias;
    let ds: String = digits.iter().map(|d| char::from(b'0' + d)).collect();
    let ds = ds.trim_start_matches('0');
    let ds = if ds.is_empty() { "0" } else { ds };
    let mut s = if exp >= 0 {
        format!("{ds}{}", "0".repeat(exp as usize))
    } else {
        let k = (-exp) as usize;
        if ds.len() <= k {
            format!("0.{:0>width$}", ds, width = k)
        } else {
            let (a, f) = ds.split_at(ds.len() - k);
            format!("{a}.{f}")
        }
    };
    if sign {
        s.insert(0, '-');
    }
    s
}

/// 열 값 하나 — `le` = 서버 TYPDEF가 작은 끝인가.
pub fn read_value(r: &mut Reader<'_>, c: ColDesc, le: bool) -> Result<Value, RowErr> {
    // 널 지시자: 0xFF = NULL · 0x80(CI 10-25 CALL FDODTA의 IN 매개변수 = "값 없음") 등 최상위 비트 = 값 생략 → NULL로.
    if c.nullable() && r.u8()? & 0x80 != 0 {
        return Ok(Value::Null);
    }
    let n = c.len_u16();
    Ok(match c.base() {
        0x02 | 0x04 | 0x16 | 0x06 => Value::Int(int_from(r.take(n)?, le)),
        0x32 | 0x34 | 0x3E | 0x40 => {
            let ln = r.len_be16()?;
            Value::Str(text(r.take(ln)?))
        }
        0x30 | 0x3C => Value::Str(text(r.take(n)?).trim_end_matches(' ').to_string()),
        0x0A => {
            let s = r.take(8)?;
            let a: [u8; 8] = s.try_into().map_err(|_| RowErr::Truncated)?;
            Value::Float(if le {
                f64::from_le_bytes(a)
            } else {
                f64::from_be_bytes(a)
            })
        }
        0x0C => {
            let s = r.take(4)?;
            let a: [u8; 4] = s.try_into().map_err(|_| RowErr::Truncated)?;
            Value::Float(f64::from(if le {
                f32::from_le_bytes(a)
            } else {
                f32::from_be_bytes(a)
            }))
        }
        0x0E => {
            let (p, s) = (c.len[0] as usize, c.len[1] as usize);
            let ln = (p + 2) / 2;
            Value::Decimal(packed_decimal(r.take(ln)?, s))
        }
        0xBA => Value::Decimal(decfloat(r.take(n)?)),
        0x24 => {
            // `YYYY-MM-DD-HH.MM.SS.ffffff` → `YYYY-MM-DD HH:MM:SS.ffffff`.
            let s = text(r.take(n)?).trim_end().to_string();
            let mut out = String::with_capacity(s.len());
            for (i, ch) in s.chars().enumerate() {
                out.push(match (i, ch) {
                    (10, '-') => ' ',
                    (13 | 16, '.') => ':',
                    _ => ch,
                });
            }
            Value::Str(out)
        }
        0x20 => Value::Str(text(r.take(n)?)),
        0x22 => Value::Str(text(r.take(n)?).replace('.', ":")),
        0x36 | 0x38 | 0x3A => Value::Str(text(r.take(n)?).trim_end_matches(' ').to_string()),
        0xBE => Value::Bool(r.take(n)?.iter().any(|&x| x != 0)),
        0x1E => Value::Bytes(r.take(n)?.to_vec()),
        // ★ LOB(T-426 2차 · pydrda `_parse_response`): 행 안에는 널 지시자(+ 로케이터 바이트)뿐이고 값은 체인의 EXTDTA 객체로 온다 →
        //   자리표시(빈 Bytes)를 두고 [`attach_lobs`]가 행·열 순서로 채운다. 로케이터 타입(0x18~0x1D)은 로케이터 바이트(길이 = 서술자)를 소비.
        //   ★ 실측(CI 40156275674 · Db2 12.1): QRYDSC 길이 `0x8009` = 최상위 비트(EXTDTA 값) + 행 안 자리표시 **9바이트**(상태 1 + 길이 8 큰 끝 ·
        //   `02 00 00 00 00 00 00 09 60` = 2400 B) → `len & 0x7FFF`만큼 소비(NULL이면 지시자만). 0바이트로 보면 다음 열·다음 행 경계가 어긋나 2행째가 사라진다.
        0x18 | 0x1A | 0x1C | 0xC8 | 0xCA | 0xCC | 0xCE => {
            let _placeholder = r.take(n & 0x7FFF)?;
            Value::Bytes(Vec::new())
        }
        0x26 | 0xC0 => Value::Bytes(r.take(n & 0x7FFF)?.to_vec()),
        0x28 | 0xC2 => {
            let ln = r.len_be16()?;
            Value::Bytes(r.take(ln)?.to_vec())
        }
        0x2A => {
            let s = r.take(4)?;
            let ln = u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize;
            Value::Bytes(r.take(ln)?.to_vec())
        }
        t => {
            return Err(RowErr::Bad(err(format!(
                "DRDA: column type 0x{t:02X} is not supported yet (LOB/XML = phase 2)"
            ))))
        }
    })
}

/// LOB 열인가(값이 EXTDTA로 따로 온다).
#[must_use]
pub fn is_lob(c: ColDesc) -> bool {
    matches!(c.base(), 0x18 | 0x1A | 0x1C | 0xC8 | 0xCA | 0xCC | 0xCE)
}

/// 문자 LOB인가(CLOB/DBCLOB · UTF-8로 복원) — 아니면 BLOB(Bytes).
fn is_char_lob(c: ColDesc) -> bool {
    matches!(c.base(), 0x1A | 0x1C | 0xCA | 0xCC | 0xCE)
}

/// 인라인 LOB(0xC8~0xCF)의 EXTDTA는 상태 바이트 1개가 앞에 온다(pydrda · 0x00 = 정상).
fn inline_lob(c: ColDesc) -> bool {
    matches!(c.base(), 0xC8 | 0xCA | 0xCC | 0xCE)
}

/// EXTDTA 객체들을 행·열 순서(NULL 아닌 LOB 칸)로 붙인다 — 모자라면 남은 칸은 자리표시 그대로(빈 값).
pub fn attach_lobs(rows: &mut [Vec<Value>], cols: &[ColDesc], extdta: &[Vec<u8>]) {
    let lob_cols: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter(|(_, c)| is_lob(**c))
        .map(|(i, _)| i)
        .collect();
    if lob_cols.is_empty() || extdta.is_empty() {
        return;
    }
    let mut next = 0usize;
    for row in rows.iter_mut() {
        for &ci in &lob_cols {
            if matches!(row.get(ci), Some(Value::Null)) {
                continue;
            }
            let Some(data) = extdta.get(next) else {
                return;
            };
            next += 1;
            let c = cols[ci];
            let body: &[u8] = if inline_lob(c) && !data.is_empty() {
                &data[1..]
            } else {
                data
            };
            row[ci] = if matches!(c.base(), 0x1A | 0xCC) {
                // DBCLOB(DBCS · CCSID 1200) = UTF-16BE.
                let units: Vec<u16> = body
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_be_bytes(*b))
                    .collect();
                Value::Str(String::from_utf16_lossy(&units))
            } else if is_char_lob(c) {
                Value::Str(String::from_utf8_lossy(body).into_owned())
            } else {
                Value::Bytes(body.to_vec())
            };
        }
    }
}

/// `?` 자리(문자열·주석 밖) 가운데 `which`(0 기준)번째를 `repl`로 바꾼다 — XML 바인드 재작성(`XMLPARSE(DOCUMENT CAST(? AS CLOB(2G)))` ·
/// XML(989)을 LOB으로 직접 보내면 서버가 "lob lengths don't match"(SYNTAXRM 0x124C · CI 40182063764) → CLOB 경로를 재사용한다).
#[must_use]
pub fn replace_placeholder(sql: &str, which: usize, repl: &str) -> String {
    let b = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + repl.len());
    let (mut i, mut n) = (0usize, 0usize);
    while i < b.len() {
        let c = b[i];
        match c {
            b'\'' | b'"' => {
                let s = i;
                i += 1;
                while i < b.len() {
                    if b[i] == c {
                        if i + 1 < b.len() && b[i + 1] == c {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.push_str(&sql[s..i]);
            }
            b'-' if i + 1 < b.len() && b[i + 1] == b'-' => {
                let s = i;
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                out.push_str(&sql[s..i]);
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                let s = i;
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                out.push_str(&sql[s..i]);
            }
            b'?' => {
                if n == which {
                    out.push_str(repl);
                } else {
                    out.push('?');
                }
                n += 1;
                i += 1;
            }
            _ => {
                let ch_len = if c < 0x80 {
                    1
                } else if c >> 5 == 0b110 {
                    2
                } else if c >> 4 == 0b1110 {
                    3
                } else {
                    4
                };
                let end = (i + ch_len).min(b.len());
                out.push_str(&sql[i..end]);
                i = end;
            }
        }
    }
    out
}

/// QRYDTA 블록의 행들 — (행 목록 · 소비한 바이트 수 · 끝에 SQLCA 표식(0x00)을 만났는가).
/// 잘린 행은 소비하지 않는다(호출자가 다음 블록 앞에 붙인다).
pub fn read_rows(
    b: &[u8],
    cols: &[ColDesc],
    le: bool,
) -> Result<(Vec<Vec<Value>>, usize, bool), DbError> {
    let mut rows = Vec::new();
    let mut at = 0;
    loop {
        if at + 2 > b.len() {
            return Ok((rows, at, false));
        }
        if b[at] != 0xFF {
            // SQLCAGRP 있음 = 끝 또는 경고(SQLCARD가 이 자리에 온다) — 호출자가 처리.
            return Ok((rows, at, true));
        }
        let mut r = Reader { b, pos: at + 2 };
        let mut row = Vec::with_capacity(cols.len());
        let mut truncated = false;
        for &c in cols {
            match read_value(&mut r, c, le) {
                Ok(v) => row.push(v),
                Err(RowErr::Truncated) => {
                    truncated = true;
                    break;
                }
                Err(RowErr::Bad(e)) => return Err(e),
            }
        }
        if truncated {
            return Ok((rows, at, false));
        }
        rows.push(row);
        at = r.pos;
    }
}

// ── SQLDARD(열·매개변수 메타) — pydrda `parse_sqldard`/`_parse_column_db2` 레이아웃.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlDaCol {
    pub name: String,
    pub label: String,
    /// Db2 SQLTYPE(448 VARCHAR · 496 INTEGER … 홀수 = 널 허용).
    pub sqltype: u16,
    pub length: u64,
    pub precision: u16,
    pub scale: u16,
    pub ccsid: u16,
}

impl SqlDaCol {
    /// 표시용 타입 이름(`VARCHAR(50)` · `DECIMAL(10,2)` · `INTEGER` …).
    #[must_use]
    pub fn type_name(&self) -> String {
        let t = self.sqltype & !1;
        let n = self.length;
        match t {
            384 => "DATE".into(),
            388 => "TIME".into(),
            392 => "TIMESTAMP".into(),
            404 => "BLOB".into(),
            408 => "CLOB".into(),
            412 => "DBCLOB".into(),
            448 => format!("VARCHAR({n})"),
            452 => format!("CHAR({n})"),
            456 => "LONG VARCHAR".into(),
            464 => format!("VARGRAPHIC({n})"),
            468 => format!("GRAPHIC({n})"),
            480 => {
                if n == 4 {
                    "REAL".into()
                } else {
                    "DOUBLE".into()
                }
            }
            484 => format!("DECIMAL({},{})", self.precision, self.scale),
            492 => "BIGINT".into(),
            496 => "INTEGER".into(),
            500 => "SMALLINT".into(),
            908 => format!("VARBINARY({n})"),
            912 => format!("BINARY({n})"),
            988 => "XML".into(),
            996 => format!("DECFLOAT({})", if n == 8 { 16 } else { 34 }),
            2436 => "BOOLEAN".into(),
            904 => "ROWID".into(),
            other => format!("SQLTYPE {other}"),
        }
    }
}

fn u16_at(b: &[u8], at: usize, le: bool) -> Option<u16> {
    let s: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
    Some(if le {
        u16::from_le_bytes(s)
    } else {
        u16::from_be_bytes(s)
    })
}

fn u64_at(b: &[u8], at: usize, le: bool) -> Option<u64> {
    let s: [u8; 8] = b.get(at..at + 8)?.try_into().ok()?;
    Some(if le {
        u64::from_le_bytes(s)
    } else {
        u64::from_be_bytes(s)
    })
}

/// VCM + VCS 이름 하나(둘 중 비지 않은 쪽) — (이름 · 다음 오프셋).
fn var_name(b: &[u8], at: usize) -> Option<(String, usize)> {
    let (m, n1) = crate::sqlca::var_bytes(b, at)?;
    let (s, n2) = crate::sqlca::var_bytes(b, n1)?;
    let raw = if m.is_empty() { s } else { m };
    Some((String::from_utf8_lossy(raw).into_owned(), n2))
}

/// SQLDARD 객체 → (SQLCA · 열 목록). 첫 바이트 0x00 = SQLCA 있음(결과 열) · 0xFF = SQLCA 없음(입력 매개변수 기술 · 이름 = 번호).
pub fn parse_sqldard(
    obj: &[u8],
    le: bool,
) -> Result<(Option<crate::sqlca::Sqlca>, Vec<SqlDaCol>), DbError> {
    let (ca, at) = crate::sqlca::parse_sqlca(obj, le)?;
    if ca.as_ref().is_some_and(crate::sqlca::Sqlca::is_error) {
        return Ok((ca, Vec::new()));
    }
    Ok((ca, parse_sqlda_cols(obj, at, le)?))
}

/// SQLCINRD(0x240B · CALL 결과 집합의 열 정보 = SQLCA 없는 SQLDA · 121b ⑥) → 열 목록.
pub fn parse_sqlcinrd(obj: &[u8], le: bool) -> Result<Vec<SqlDaCol>, DbError> {
    parse_sqlda_cols(obj, 0, le)
}

/// SQLDHGRP 지시자부터 시작하는 SQLDA 열 묶음 해석(SQLDARD·SQLCINRD 공용).
///
/// ★ 실측 레이아웃(Db2 12.1 · SQLAM 11 · CI 40116752054 SQLDARD 3벌 + 40156275674 SQLCINRD · 네 벌 모두 같은 자리):
/// - 머리 = SQLDHGRP 지시자(FF = 없음 / 00 = 12 B 고정 + VC 길이들(실측 0 ×12)) → 열 수 2 B(서버 TYPDEF 끝) — SQLCINRD는 SQLCA 없이 여기서 시작.
/// - 열 하나 = 16 B(정밀도 2 · 소수 2 · 길이 8 · SQLTYPE 2 · CCSID 2 큰 끝) · SQLDOPTGRP 지시자 1 · **10 B**(용도 미상 · VARCHAR 1208이면 `08 00`) ·
///   SQLUNNAMED 2(LE) · 이름 VCM+VCS · 라벨 VCM+VCS · 설명 VCM+VCS · SQLUDTGRP 지시자(FF) · SQLDXGRP 지시자(FF / 00 = 12 B + RDBNAM VCS +
///   상관명·기본 테이블·스키마·열 이름 VCM+VCS ×4 + 4 B) · 꼬리 지시자(FF). SQLDARD(FF FF FF)와 SQLCINRD(FF 00 … FF)가 여기서 갈린다.
/// - 구조 해석이 어긋나면 옛 규칙(끝 표식 `FF FF FF` 역탐색)으로 그 열만 다시 시도 · 그래도 안 되면 남은 열은 이름 없이(호스트 = COLn).
fn parse_sqlda_cols(obj: &[u8], mut at: usize, le: bool) -> Result<Vec<SqlDaCol>, DbError> {
    let bad = |w: &str| err(format!("DRDA SQLDARD: truncated {w}"));
    match obj.get(at) {
        Some(0x00) => {
            at += 13;
            // VC 길이들(RDBNAM VCS · 스키마 VCM/VCS · 실측은 그 밖 0 ×6) — 0은 건너뛰고 ≤255는 문자열로 소비 · 그보다 크면 열 수(LE · 상위 바이트 0).
            for _ in 0..8 {
                match u16_at(obj, at, false) {
                    Some(0) => at += 2,
                    Some(n) if n <= 255 => at += 2 + n as usize,
                    _ => break,
                }
            }
        }
        Some(_) => at += 1,
        None => return Err(bad("sqldhgrp")),
    }
    let count = u16_at(obj, at, le).ok_or_else(|| bad("sqlnumrow"))? as usize;
    at += 2;
    let mut cols = Vec::with_capacity(count);
    for _ in 0..count {
        let precision = u16_at(obj, at, le).ok_or_else(|| bad("precision"))?;
        let scale = u16_at(obj, at + 2, le).ok_or_else(|| bad("scale"))?;
        let length = u64_at(obj, at + 4, le).ok_or_else(|| bad("length"))?;
        let sqltype = u16_at(obj, at + 12, le).ok_or_else(|| bad("sqltype"))?;
        let ccsid = u16_at(obj, at + 14, false).ok_or_else(|| bad("ccsid"))?;
        at += 16;
        let mut col = SqlDaCol {
            sqltype,
            length,
            precision,
            scale,
            ccsid,
            ..SqlDaCol::default()
        };
        match sqldoptgrp(obj, at) {
            OptGrp::Done { name, label, next } => {
                col.name = name;
                col.label = label;
                cols.push(col);
                at = next;
            }
            OptGrp::Partial { name, label } => {
                // 이름까지는 읽었지만 꼬리가 잘림(트레이스 덤프 등) — 이 열은 살리고 나머지는 이름 없이.
                col.name = name;
                col.label = label;
                cols.push(col);
                while cols.len() < count {
                    cols.push(SqlDaCol::default());
                }
                break;
            }
            OptGrp::Unknown => match legacy_name_scan(obj, at) {
                Some((name, next)) if next <= obj.len() => {
                    col.name = name;
                    cols.push(col);
                    at = next;
                }
                _ => {
                    cols.push(col);
                    while cols.len() < count {
                        cols.push(SqlDaCol::default());
                    }
                    break;
                }
            },
        }
    }
    Ok(cols)
}

enum OptGrp {
    Done {
        name: String,
        label: String,
        next: usize,
    },
    Partial {
        name: String,
        label: String,
    },
    Unknown,
}

/// SQLDOPTGRP(열 16 B 뒤) 구조 해석 — 위 `parse_sqlda_cols` 주석의 실측 레이아웃.
fn sqldoptgrp(obj: &[u8], at: usize) -> OptGrp {
    match obj.get(at) {
        Some(0xFF) => {
            return OptGrp::Done {
                name: String::new(),
                label: String::new(),
                next: at + 1,
            }
        }
        Some(0x00) => {}
        _ => return OptGrp::Unknown,
    }
    // 지시자 1 + 미상 10 + SQLUNNAMED 2 → 이름.
    let p = at + 13;
    let Some((name, p)) = var_name(obj, p) else {
        return OptGrp::Unknown;
    };
    // 이름은 식별자(0xFF·제어 문자 없음) — 어긋나면 레이아웃이 다른 것.
    if name.chars().any(|c| c.is_control() || c == '\u{FFFD}') || name.len() > 128 {
        return OptGrp::Unknown;
    }
    let Some((label, p)) = var_name(obj, p) else {
        return OptGrp::Partial {
            name,
            label: String::new(),
        };
    };
    let partial = |name: String, label: String| OptGrp::Partial { name, label };
    let Some((_comments, p)) = var_name(obj, p) else {
        return partial(name, label);
    };
    // SQLUDTGRP 지시자.
    let mut p = p;
    match obj.get(p) {
        Some(0xFF) => p += 1,
        _ => return partial(name, label), // UDT 열 = 아직 미지원(크기 미상) → 이 열까지만
    }
    // SQLDXGRP 지시자.
    match obj.get(p) {
        Some(0xFF) => p += 1,
        Some(0x00) => {
            p += 1 + 12;
            let Some((_, q)) = crate::sqlca::var_bytes(obj, p) else {
                return partial(name, label);
            };
            p = q;
            for _ in 0..4 {
                let Some((_, q)) = var_name(obj, p) else {
                    return partial(name, label);
                };
                p = q;
            }
            p += 4;
        }
        _ => return partial(name, label),
    }
    // 꼬리 지시자.
    match obj.get(p) {
        Some(0xFF) => OptGrp::Done {
            name,
            label,
            next: p + 1,
        },
        _ => partial(name, label),
    }
}

/// 옛 규칙(1차 · CI 4차): 끝 표식 `FF FF FF`에서 거꾸로 이름을 찾는다 — 구조 해석이 어긋난 열의 예비 경로.
fn legacy_name_scan(obj: &[u8], seg0: usize) -> Option<(String, usize)> {
    let end = obj[seg0..]
        .windows(3)
        .position(|w| w == [0xFF, 0xFF, 0xFF])?
        + seg0;
    if end < seg0 + 10 {
        return None;
    }
    let tail_zero = obj[end - 10..end].iter().all(|&x| x == 0);
    let mut name = String::new();
    if tail_zero {
        let seg = &obj[seg0..end - 10];
        let mut best: Option<String> = None;
        for n in 0..=seg.len().saturating_sub(2) {
            let lp = seg.len() - 2 - n;
            if seg[lp..lp + 2] == (n as u16).to_be_bytes() {
                if let Ok(t) = std::str::from_utf8(&seg[lp + 2..]) {
                    if n == 0 || !t.is_empty() {
                        best = Some(t.to_string());
                    }
                }
            }
        }
        name = best?;
    }
    Some((name, end + 3))
}

/// SQLDTARD의 FDODTA(OUT 매개변수 값 한 행 · 121b ⑥-4): 첫 바이트 0xFF = `FF 00` 뒤 값 · 아니면 SQLCA + 그룹 지시자 1B 뒤 값.
pub fn read_out_row(fdodta: &[u8], cols: &[ColDesc], le: bool) -> Result<Vec<Value>, DbError> {
    let start = if fdodta.first() == Some(&0xFF) {
        2
    } else {
        let (_, at) = crate::sqlca::parse_sqlca(fdodta, le)?;
        at + 1
    };
    let mut r = Reader {
        b: fdodta,
        pos: start.min(fdodta.len()),
    };
    let mut row = Vec::with_capacity(cols.len());
    for &c in cols {
        match read_value(&mut r, c, le) {
            Ok(v) => row.push(v),
            Err(RowErr::Truncated) => break,
            Err(RowErr::Bad(e)) => return Err(e),
        }
    }
    Ok(row)
}

// ── SQLDTA(입력 매개변수) 인코딩 — 서버가 DSCSQLSTT로 알려 준 타입(SqlDaCol)에 맞춰 값을 변환한다.

/// LOB 바인드 타입(BLOB 405 · CLOB 409 · DBCLOB 413 · XML 989)인가 — 값은 SQLDTA가 아니라 **EXTDTA**로 간다(2차 ⑥).
fn is_lob_type(sqltype: u16) -> bool {
    matches!(sqltype | 1, 405 | 409 | 413 | 989)
}

/// LOB 값의 전송 바이트 — BLOB = 그대로 · CLOB/DBCLOB/XML = UTF-8(CCSID 1208 · 0xCF NLOBCMIXED · 서버가 DBCLOB으로 변환).
/// ★ DBCLOB을 0xCD(NLOBCDBCS · UTF-16BE)로 보내면 길이를 바이트·문자 어느 쪽으로 해도 DTAMCHRM −30020(CI 40182500195 · 40183010063) →
///   CLOB 경로와 같은 형식으로(추정 ⓐ → CI 확정).
fn lob_bytes(p: &SqlDaCol, v: &Value) -> Vec<u8> {
    match (p.sqltype | 1, v) {
        (405, Value::Bytes(b)) => b.clone(),
        (405, other) => value_text(other).into_bytes(),
        // 재판정 손잡이(10-26): `NSQL_DB2_DBCLOB_UTF16=1`이면 DBCLOB을 0xCD(NLOBCDBCS) + UTF-16BE로(행 지시자 결함 수정 뒤 재확인용 · CI 정보성 단계).
        (413, Value::Bytes(b)) if dbclob_utf16() => utf16be(&String::from_utf8_lossy(b)),
        (413, other) if dbclob_utf16() => utf16be(&value_text(other)),
        (_, Value::Bytes(b)) => b.clone(),
        (_, other) => value_text(other).into_bytes(),
    }
}

fn dbclob_utf16() -> bool {
    std::env::var_os("NSQL_DB2_DBCLOB_UTF16").is_some()
}

/// LOB 자리표시 길이 = **9 B 고정**(FDODSC `0x8009` · Db2가 조회 때 보내는 꼴 `CF 8009`/`C9 8009`과 같음 · jb도 늘 9 · derby의 2/4/8 B 가변은
/// Db2 수용 미확인) = 표지 `0x02` + 길이 u64 큰 끝.
const LOB_PLACEHOLDER: u8 = 9;

fn fdodsc_entry(p: &SqlDaCol) -> Result<Vec<u8>, DbError> {
    let t = p.sqltype | 1; // 전부 널 허용 타입으로 기술(pydrda)
    if is_lob_type(t) {
        // ★ 2차 ⑥(derby `computeProtocolTypesAndLengths`): 행 안에는 길이 자리표시만(널 지시자 + n B) · 값은 EXTDTA 객체로(매개변수 순서).
        //   VARGRAPHIC(0x39)으로 보내면 DTAMCHRM −30020(CI 40149862498).
        let code = if t == 405 {
            0xC9
        } else if t == 413 && dbclob_utf16() {
            0xCD
        } else {
            0xCF
        };
        return Ok(vec![code, 0x80, LOB_PLACEHOLDER]);
    }
    Ok(match t {
        449 | 453 | 457 | 465 | 469 => vec![0x39, 0x3F, 0xFF],
        485 => vec![0x0F, p.precision as u8, p.scale as u8],
        501 => vec![0x05, 0x00, 2],
        497 => vec![0x03, 0x00, 4],
        493 => vec![0x17, 0x00, 8],
        481 => vec![
            if p.length == 4 { 0x0D } else { 0x0B },
            0x00,
            p.length as u8,
        ],
        385 => vec![0x21, 0x00, 0x0A],
        389 => vec![0x23, 0x00, 0x08],
        393 => vec![0x25, 0x00, 0x20],
        2437 => vec![0xBF, 0x00, 0x01],
        913 => vec![0x27, (p.length >> 8) as u8, p.length as u8],
        909 => vec![0x29, (p.length >> 8) as u8, p.length as u8],
        997 => vec![0xBB, 0x00, p.length as u8],
        905 => vec![0x1F, 0x00, p.length as u8],
        other => {
            return Err(err(format!(
                "DRDA: bind parameter SQLTYPE {other} is not supported yet"
            )))
        }
    })
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Decimal(s) | Value::Str(s) => s.clone(),
        Value::Bool(b) => {
            if *b {
                "1".into()
            } else {
                "0".into()
            }
        }
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Cursor(_) => String::new(),
    }
}

fn utf16be(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out
}

fn packed_encode(s: &str, precision: usize, scale: usize) -> Result<Vec<u8>, DbError> {
    let t = s.trim();
    let neg = t.starts_with('-');
    let t = t.trim_start_matches(['-', '+']);
    let (int_part, frac_part) = t.split_once('.').unwrap_or((t, ""));
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return Err(err(format!("DRDA: '{s}' is not a decimal number")));
    }
    let mut frac: String = frac_part.chars().take(scale).collect();
    while frac.len() < scale {
        frac.push('0');
    }
    let digits = format!("{int_part}{frac}");
    if digits.len() > precision {
        return Err(err(format!(
            "DRDA: '{s}' does not fit DECIMAL({precision},{scale})"
        )));
    }
    let padded = format!("{:0>width$}", digits, width = precision);
    let mut nibbles: Vec<u8> = padded.bytes().map(|c| c - b'0').collect();
    nibbles.push(if neg { 0x0D } else { 0x0C });
    if nibbles.len() % 2 == 1 {
        nibbles.insert(0, 0);
    }
    Ok(nibbles
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| (p[0] << 4) | p[1])
        .collect())
}

fn fdodta_value(p: &SqlDaCol, v: &Value, le: bool) -> Result<Vec<u8>, DbError> {
    if matches!(v, Value::Null) {
        return Ok(vec![0xFF]);
    }
    let t = p.sqltype | 1;
    let mut out = vec![0x00];
    if is_lob_type(t) {
        // 자리표시 = `02` + 길이 u64 큰 끝(9 B · 값은 EXTDTA · jb/Db2 실측). 길이 = **바이트 수**(DBCLOB도 · CI 40182500195: 문자 수로 보내면
        //   DTAMCHRM −30020 · 바이트로 보내면 통과 기대 — 40182063764의 "lob lengths don't match"는 같은 체인의 XML 쪽 · 추정).
        let len = lob_bytes(p, v).len() as u64;
        out.push(0x02);
        out.extend_from_slice(&len.to_be_bytes());
        return Ok(out);
    }
    let num = |name: &str| -> Result<i64, DbError> {
        match v {
            Value::Int(i) => Ok(*i),
            Value::Float(f) => Ok(*f as i64),
            Value::Bool(b) => Ok(i64::from(*b)),
            _ => value_text(v)
                .trim()
                .parse::<i64>()
                .map_err(|_| err(format!("DRDA: '{}' is not a valid {name}", value_text(v)))),
        }
    };
    let put = |out: &mut Vec<u8>, b: &[u8]| {
        if le {
            out.extend(b.iter().rev());
        } else {
            out.extend_from_slice(b);
        }
    };
    match t {
        449 | 453 | 457 | 465 | 469 => {
            let bytes = match v {
                Value::Bytes(b) => b.clone(),
                other => utf16be(&value_text(other)),
            };
            out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
            out.extend_from_slice(&bytes);
        }
        485 => out.extend(packed_encode(
            &value_text(v),
            p.precision as usize,
            p.scale as usize,
        )?),
        501 => put(&mut out, &(num("SMALLINT")? as i16).to_be_bytes()),
        497 => put(&mut out, &(num("INTEGER")? as i32).to_be_bytes()),
        493 => put(&mut out, &num("BIGINT")?.to_be_bytes()),
        481 => {
            let f = match v {
                Value::Float(f) => *f,
                Value::Int(i) => *i as f64,
                other => value_text(other)
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| err("DRDA: not a valid FLOAT"))?,
            };
            if p.length == 4 {
                put(&mut out, &(f as f32).to_be_bytes());
            } else {
                put(&mut out, &f.to_be_bytes());
            }
        }
        385 => out.extend_from_slice(format!("{:<10}", value_text(v).trim()).as_bytes()),
        389 => out
            .extend_from_slice(format!("{:<8}", value_text(v).trim().replace(':', ".")).as_bytes()),
        393 => {
            // `YYYY-MM-DD HH:MM:SS.ffffff` → `YYYY-MM-DD-HH.MM.SS.ffffff`(32자 채움).
            let s = value_text(v);
            let s = s.trim();
            let mut t = String::with_capacity(32);
            for (i, ch) in s.chars().enumerate() {
                t.push(match (i, ch) {
                    (10, ' ' | 'T') => '-',
                    (13 | 16, ':') => '.',
                    _ => ch,
                });
            }
            if t.len() == 19 {
                t.push_str(".000000");
            }
            out.extend_from_slice(format!("{t:<32}").as_bytes());
        }
        2437 => out.push(u8::from(
            matches!(v, Value::Bool(true)) || num("BOOLEAN").unwrap_or(0) != 0,
        )),
        913 | 909 | 405 => {
            let b = match v {
                Value::Bytes(b) => b.clone(),
                other => value_text(other).into_bytes(),
            };
            if t == 913 {
                let mut fixed = b;
                fixed.resize(p.length as usize, 0);
                out.extend_from_slice(&fixed);
            } else {
                out.extend_from_slice(&(b.len() as u16).to_be_bytes());
                if t == 909 {
                    out.extend_from_slice(&b);
                }
            }
        }
        other => {
            return Err(err(format!(
                "DRDA: bind parameter SQLTYPE {other} is not supported yet"
            )))
        }
    }
    Ok(out)
}

/// SQLDTA 값(FDODSC 객체 + FDODTA 객체 · 둘을 담은 상위 객체 값) — 호출자가 `Ddm::new(SQLDTA, …)`로 감싼다.
/// 둘째 = **EXTDTA 객체 값들**(NULL 아닌 LOB 매개변수 순서 · 각각 `0x00`(널 지시자) + 바이트 · derby `writeEXTDTA`) — 호출자가 SQLDTA 뒤에
/// 같은 상관 번호의 객체 DSS로 잇는다(32 767 B 초과 = DSS 연속 · [`crate::dss::encode_dss`]).
pub fn encode_sqldta(
    params: &[SqlDaCol],
    values: &[Value],
    le: bool,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), DbError> {
    if params.len() != values.len() {
        return Err(err(format!(
            "DRDA: statement has {} parameter(s) but {} value(s) were bound",
            params.len(),
            values.len()
        )));
    }
    let mut dsc = vec![((1 + params.len()) * 3) as u8, 0x76, 0xD0];
    let mut dta = Vec::new();
    let mut ext = Vec::new();
    for (p, v) in params.iter().zip(values) {
        dsc.extend(fdodsc_entry(p)?);
        dta.extend(fdodta_value(p, v, le)?);
        if is_lob_type(p.sqltype) && !matches!(v, Value::Null) {
            let mut payload = vec![0x00];
            payload.extend(lob_bytes(p, v));
            ext.push(payload);
        }
    }
    // ★ FDODTA 맨 앞 1 B = 행(그룹) 지시자 `0x00`(derby `buildFDODTA`의 NULL_LID · SQLDTARD 쪽 `read_out_row`와 대칭) — **늘** 넣는다.
    //   1차는 "총 길이 홀수면 패드"로 알고 있었고 홀수인 경우에만 우연히 맞았다(CI 40183353540: 짝수가 된 DBCLOB·XML 바인드에서 ID 지시자가
    //   1 B 밀려 DTAMCHRM −30020 · 앞 두 회차 0xCD 판정도 이 버그에 가려짐).
    dta.insert(0, 0x00);
    dsc.extend_from_slice(&[0x06, 0x71, 0xE4, 0xD0, 0x00, 0x01]);
    let mut out = Vec::new();
    crate::dss::Ddm::new(0x0010, dsc).encode_into(&mut out);
    crate::dss::Ddm::new(0x147A, dta).encode_into(&mut out);
    Ok((out, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qrydsc_and_rows_little_endian() {
        // 3열: NINTEGER(4) · NVARMIX · NDECIMAL(5,2)
        let dsc = [
            0x0C, 0x76, 0xD0, 0x03, 0x00, 0x04, 0x3F, 0x7F, 0xFF, 0x0F, 0x05, 0x02, 0x06, 0x71,
            0xE4, 0xD0, 0x00, 0x01,
        ];
        let cols = parse_qrydsc(&dsc).unwrap();
        assert_eq!(cols.len(), 3);
        assert!(cols[0].nullable() && cols[0].base() == 0x02);
        // 행 2 + 잘린 행 1.
        let mut b = vec![0xFF, 0x00];
        b.extend_from_slice(&[0x00, 0x2A, 0x00, 0x00, 0x00]); // 42
        b.extend_from_slice(&[0x00, 0x00, 0x06]);
        b.extend_from_slice("한글".as_bytes()); // 6바이트
        b.extend_from_slice(&[0x00, 0x12, 0x34, 0x5D]); // -123.45 (p5 s2 → 3바이트)
        b.extend_from_slice(&[0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C]); // NULL · '' · 0.00
        b.extend_from_slice(&[0xFF, 0x00, 0x00, 0x01]); // 잘림
        let (rows, used, sqlca) = read_rows(&b, &cols, true).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            vec![
                Value::Int(42),
                Value::Str("한글".into()),
                Value::Decimal("-123.45".into())
            ]
        );
        assert_eq!(
            rows[1],
            vec![
                Value::Null,
                Value::Str(String::new()),
                Value::Decimal("0.00".into())
            ]
        );
        assert_eq!(used, b.len() - 4);
        assert!(!sqlca);
        // SQLCA 표식(0x00)에서 멈춤.
        let (r2, u2, s2) = read_rows(&[0x00, 0x00], &cols, true).unwrap();
        assert!(r2.is_empty() && u2 == 0 && s2);
    }

    #[test]
    fn packed_decimal_and_decfloat() {
        assert_eq!(packed_decimal(&[0x12, 0x34, 0x5C], 2), "123.45");
        assert_eq!(packed_decimal(&[0x00, 0x00, 0x0D], 0), "0");
        assert_eq!(packed_decimal(&[0x00, 0x05, 0x1D], 3), "-0.051");
        assert_eq!(
            packed_encode("-123.45", 5, 2).unwrap(),
            vec![0x12, 0x34, 0x5D]
        );
        assert_eq!(packed_encode("7", 5, 2).unwrap(), vec![0x00, 0x70, 0x0C]);
        assert!(packed_encode("123456", 5, 2).is_err());
        // DECFLOAT(16) 1.5 = 0x2238000000000015(IEEE 754-2008 DPD · 1.5 = 15 × 10^-1)
        // 1.5 = 부호 0 · G = 01000(지수 상위 01 · 선행 자릿수 0) · E = 0x8D(397 = -1 + 398) · T = DPD(0,1,5) = 0x15.
        assert_eq!(decfloat(&0x2234_0000_0000_0015u64.to_be_bytes()), "1.5");
        assert_eq!(decfloat(&0x2234_0000_0000_0000u64.to_be_bytes()), "0.0");
        assert_eq!(dpd_decode(0x3FF), (9, 9, 9));
        assert_eq!(dpd_decode(0x015), (0, 1, 5));
    }

    #[test]
    fn sqldard_named_columns_and_sqldta_encoding() {
        // SQLDARD: 지시자 0 · SQLCODE 0 · … · SQLDHGRP 0xFF · 열 1(VARCHAR(20) · 이름 NAME)
        let mut b = vec![0x00];
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(b"00000");
        b.extend_from_slice(b"SQLRA0F9");
        b.push(0xFF); // SQLCAXGRP 없음
        b.push(0xFF); // SQLDIAGGRP 없음
        b.push(0xFF); // SQLDHGRP 없음
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes()); // precision
        b.extend_from_slice(&0u16.to_le_bytes()); // scale
        b.extend_from_slice(&20u64.to_le_bytes()); // length
        b.extend_from_slice(&449u16.to_le_bytes()); // NVARCHAR
        b.extend_from_slice(&1208u16.to_be_bytes()); // ccsid
        b.extend_from_slice(&[0; 8]); // 앞머리
        b.extend_from_slice(&[0x08, 0x00, 0x00, 0x00]); // 2B + UNNAMED
        b.extend_from_slice(&[0x00, 0x04]);
        b.extend_from_slice(b"NAME"); // SQLNAME
        b.extend_from_slice(&[0; 10]); // 나머지 이름·라벨·설명 길이 0
        b.extend_from_slice(&[0xFF; 3]); // 지시자
        let (ca, cols) = parse_sqldard(&b, true).unwrap();
        assert_eq!(ca.unwrap().code, 0);
        assert_eq!(cols.len(), 1);
        assert_eq!(
            (cols[0].name.as_str(), cols[0].type_name().as_str()),
            ("NAME", "VARCHAR(20)")
        );
        // SQLDTA: 문자열 → VARGRAPHIC UTF-16BE · 정수 작은 끝 · NULL.
        let params = vec![
            cols[0].clone(),
            SqlDaCol {
                sqltype: 497,
                length: 4,
                ..SqlDaCol::default()
            },
            SqlDaCol {
                sqltype: 485,
                precision: 5,
                scale: 2,
                ..SqlDaCol::default()
            },
        ];
        let (v, ext) = encode_sqldta(
            &params,
            &[Value::Str("ab".into()), Value::Int(-2), Value::Null],
            true,
        )
        .unwrap();
        assert!(ext.is_empty());
        let objs = crate::dss::parse_objects(&v).unwrap();
        assert_eq!(objs[0].cp, 0x0010);
        assert_eq!(&objs[0].value[..3], &[12, 0x76, 0xD0]);
        assert_eq!(&objs[0].value[3..6], &[0x39, 0x3F, 0xFF]);
        assert_eq!(&objs[0].value[6..9], &[0x03, 0x00, 0x04]);
        assert_eq!(&objs[0].value[9..12], &[0x0F, 5, 2]);
        assert_eq!(objs[1].cp, 0x147A);
        let d = &objs[1].value;
        // 맨 앞 = 행 지시자 0x00(늘) · 그 뒤 매개변수마다 널 지시자 + 값.
        assert_eq!(d[0], 0x00);
        assert_eq!(&d[1..8], &[0x00, 0x00, 0x04, 0x00, b'a', 0x00, b'b']);
        assert_eq!(&d[8..13], &[0x00, 0xFE, 0xFF, 0xFF, 0xFF]);
        assert_eq!(d[13], 0xFF);
    }

    /// LOB: 행의 자리표시가 EXTDTA 순서로 채워진다(NULL 건너뜀 · 인라인 상태 바이트 제거 · CLOB = 문자 · BLOB = 바이트) · `?` 재작성.
    #[test]
    fn lobs_attach_in_row_order_and_placeholder_rewrite() {
        let cols = [
            ColDesc {
                drda_type: 0x03,
                len: [0, 4],
            },
            ColDesc {
                drda_type: 0xCF,
                len: [0x80, 0x09],
            },
            ColDesc {
                drda_type: 0xC9,
                len: [0x80, 0x09],
            },
        ];
        assert!(is_lob(cols[1]) && is_lob(cols[2]) && !is_lob(cols[0]));
        // 행 2 = (1, '한', NULL) · (2, NULL, x'0102') — 자리표시 9 B(상태 + 길이 8 큰 끝 · CI 40156275674 실측 레이아웃).
        let ph = |n: u8| [0x00, 0x02, 0, 0, 0, 0, 0, 0, 0, n];
        let mut b = vec![0xFF, 0x00, 0x00, 1, 0, 0, 0];
        b.extend_from_slice(&ph(3));
        b.push(0xFF);
        b.extend_from_slice(&[0xFF, 0x00, 0x00, 2, 0, 0, 0, 0xFF]);
        b.extend_from_slice(&ph(2));
        let (mut rows, used, sqlca) = read_rows(&b, &cols, true).unwrap();
        assert_eq!((used, sqlca, rows.len()), (b.len(), false, 2));
        // CI 실측 행(ID 1 CLOB 2400 B · BLOB 5000 B / ID 2 NULL·NULL) — 2행째가 살아야 한다.
        let ci: Vec<u8> = [
            &[
                0xFF, 0x00, 0x01, 0, 0, 0, 0x00, 0x02, 0, 0, 0, 0, 0, 0, 0x09, 0x60,
            ][..],
            &[0x00, 0x02, 0, 0, 0, 0, 0, 0, 0x13, 0x88],
            &[0xFF, 0x00, 0x02, 0, 0, 0, 0xFF, 0xFF],
        ]
        .concat();
        // 실측 QRYDSC = `02 0004 | CF 8009 | C9 8009`(ID는 NOT NULL → 지시자 없음).
        let ci_cols = [
            ColDesc {
                drda_type: 0x02,
                len: [0, 4],
            },
            cols[1],
            cols[2],
        ];
        let (ci_rows, ci_used, ci_sqlca) = read_rows(&ci, &ci_cols, true).unwrap();
        assert_eq!((ci_used, ci_sqlca, ci_rows.len()), (ci.len(), false, 2));
        assert_eq!(ci_rows[1], vec![Value::Int(2), Value::Null, Value::Null]);
        // 지시자 0x80 = 값 없음(NULL) · 뒤 값 없이 다음 열로.
        let c2 = [
            ColDesc {
                drda_type: 0x03,
                len: [0, 4],
            },
            ColDesc {
                drda_type: 0x03,
                len: [0, 4],
            },
        ];
        let (rows2, _, _) = read_rows(&[0xFF, 0x00, 0x80, 0x00, 2, 0, 0, 0], &c2, true).unwrap();
        assert_eq!(rows2[0], vec![Value::Null, Value::Int(2)]);
        assert_eq!(
            rows[0],
            vec![Value::Int(1), Value::Bytes(vec![]), Value::Null]
        );
        let ext = vec![
            {
                let mut v = vec![0x00];
                v.extend_from_slice("한".as_bytes());
                v
            },
            vec![0x00, 0x01, 0x02],
        ];
        attach_lobs(&mut rows, &cols, &ext);
        assert_eq!(rows[0][1], Value::Str("한".into()));
        assert_eq!(rows[1][2], Value::Bytes(vec![0x01, 0x02]));
    }

    /// 2차 ⑥ LOB 바인드 = FDODSC `C9/CF 80 n` · FDODTA 자리표시(지시자 + 길이 n B) · EXTDTA 값(0x00 + 바이트 · 매개변수 순서 · NULL 제외).
    #[test]
    fn sqldta_lob_binds_go_to_extdta() {
        let params = [
            SqlDaCol {
                sqltype: 497,
                length: 4,
                ..SqlDaCol::default()
            },
            SqlDaCol {
                sqltype: 409,
                length: 1_048_576,
                ccsid: 1208,
                ..SqlDaCol::default()
            },
            SqlDaCol {
                sqltype: 405,
                length: 1_048_576,
                ..SqlDaCol::default()
            },
            SqlDaCol {
                sqltype: 405,
                ..SqlDaCol::default()
            },
        ];
        let big = vec![0x7Eu8; 40_000];
        let (v, ext) = encode_sqldta(
            &params,
            &[
                Value::Int(1),
                Value::Str("한".into()),
                Value::Bytes(big),
                Value::Null,
            ],
            true,
        )
        .unwrap();
        let objs = crate::dss::parse_objects(&v).unwrap();
        let dsc = &objs[0].value;
        assert_eq!(&dsc[3..6], &[0x03, 0x00, 0x04]);
        assert_eq!(
            &dsc[6..9],
            &[0xCF, 0x80, 0x09],
            "CLOB = NLOBCMIXED · 자리 9 B"
        );
        assert_eq!(
            &dsc[9..12],
            &[0xC9, 0x80, 0x09],
            "BLOB = NLOBBYTES · 자리 9 B"
        );
        assert_eq!(&dsc[12..15], &[0xC9, 0x80, 0x09], "NULL LOB도 같은 서술");
        let d = &objs[1].value;
        // 맨 앞 = 행 지시자 0x00(늘).
        assert_eq!(d[0], 0x00);
        assert_eq!(&d[1..6], &[0x00, 1, 0, 0, 0]);
        assert_eq!(
            &d[6..16],
            &[0x00, 0x02, 0, 0, 0, 0, 0, 0, 0, 3],
            "CLOB 길이 3 B(UTF-8 '한')"
        );
        assert_eq!(
            &d[16..26],
            &[0x00, 0x02, 0, 0, 0, 0, 0, 0, 0x9C, 0x40],
            "BLOB 길이 40 000"
        );
        assert_eq!(d[26], 0xFF);
        assert_eq!(ext.len(), 2);
        assert_eq!(ext[0], [&[0x00u8][..], "한".as_bytes()].concat());
        assert_eq!((ext[1][0], ext[1].len()), (0x00, 40_001));
        // DBCLOB(413) = CLOB과 같은 0xCF · UTF-8(서버 변환) · 자리표시 길이 = 바이트.
        let dp = [SqlDaCol {
            sqltype: 413,
            length: 1_048_576,
            ccsid: 1200,
            ..SqlDaCol::default()
        }];
        let (v, ext) = encode_sqldta(&dp, &[Value::Str("한a".into())], true).unwrap();
        let objs = crate::dss::parse_objects(&v).unwrap();
        assert_eq!(&objs[0].value[3..6], &[0xCF, 0x80, 0x09]);
        let d = &objs[1].value;
        let at = d.len() - 10;
        assert_eq!(
            &d[at..],
            &[0x00, 0x02, 0, 0, 0, 0, 0, 0, 0, 4],
            "길이 = 4 B(UTF-8 '한a')"
        );
        assert_eq!(ext[0], [&[0x00u8][..], "한a".as_bytes()].concat());
        // 총 길이가 짝수인 꼴(INTEGER 하나)에서도 행 지시자가 있다(CI 40183353540 회귀).
        let ip = [SqlDaCol {
            sqltype: 497,
            length: 4,
            ..SqlDaCol::default()
        }];
        let (v, _) = encode_sqldta(&ip, &[Value::Int(7)], true).unwrap();
        let objs = crate::dss::parse_objects(&v).unwrap();
        assert_eq!(objs[1].value, vec![0x00, 0x00, 7, 0, 0, 0]);
        // XML 재작성: k번째 `?`만 · 문자열·주석 안 `?`는 세지 않는다.
        assert_eq!(
            replace_placeholder(
                "INSERT INTO t VALUES (?, 'a?b''c', /* ? */ ?) -- ?",
                1,
                "XMLPARSE(DOCUMENT CAST(? AS CLOB(2G)))"
            ),
            "INSERT INTO t VALUES (?, 'a?b''c', /* ? */ XMLPARSE(DOCUMENT CAST(? AS CLOB(2G)))) -- ?"
        );
    }

    /// ★ 실서버 덤프(Db2 12.1.2 · CI 4차 40116752054): (a) `VALUES TRIM(CURRENT SCHEMA)` 식 열 1 · (b) `SELECT ID, NAME, AMT, D …` 이름 4열 ·
    /// (p) `WHERE ID <= ?` 입력 매개변수 기술(SQLCA 없음 · 0xFF) — 이름·타입·정밀도·CCSID 전부 실측값.
    #[test]
    fn sqldard_real_server_dumps() {
        let hx = |h: &str| -> Vec<u8> {
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect()
        };
        let a = hx("0000000000303030303053514C3132303132000000000000000000010000000100000021FFFFFF00000000202020202020202020202000124E455841202020202020202020202020202000000000FFFF0100000000008000000000000000C00104B80000000000000000080000010000013100000000000000000000FFFFFF");
        let (ca, cols) = parse_sqldard(&a, true).unwrap();
        assert_eq!(ca.unwrap().code, 0);
        assert_eq!(cols.len(), 1);
        assert_eq!(
            (
                cols[0].name.as_str(),
                cols[0].sqltype,
                cols[0].length,
                cols[0].ccsid
            ),
            ("1", 448, 128, 1208)
        );
        let b = hx("0000000000303030303053514C313230313200000000000000000001000000070000000000000000000000202020202020202020202000124E455841202020202020202020202020202000000000FFFF0400000000000400000000000000F0010000000000000000000000000000000002494400000000000000000000FFFFFF000000003200000000000000C10104B80000000000000000080000000000044E414D4500000000000000000000FFFFFF0A0002000000000000000000E5010000000000000000000000000000000003414D5400000000000000000000FFFFFF000000000A00000000000000810104B80000000000000000000000000000014400000000000000000000FFFFFF");
        let (_, cols) = parse_sqldard(&b, true).unwrap();
        let got: Vec<(String, String)> = cols
            .iter()
            .map(|c| (c.name.clone(), c.type_name()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("ID".to_string(), "INTEGER".to_string()),
                ("NAME".to_string(), "VARCHAR(50)".to_string()),
                ("AMT".to_string(), "DECIMAL(10,2)".to_string()),
                ("D".to_string(), "DATE".to_string()),
            ]
        );
        assert_eq!(
            (
                cols[1].sqltype,
                cols[1].ccsid,
                cols[2].sqltype,
                cols[3].sqltype
            ),
            (449, 1208, 485, 385)
        );
        // CI 40156275674 SQLCINRD(0x240B · DDM 머리 제외 · 트레이스 512 B 상한으로 열2 SQLDXGRP에서 잘림) = ID INTEGER · NAME VARCHAR(20) 1208 ·
        // SQLDHGRP 있음(00 + 24 B) · 열마다 SQLDXGRP(기본 테이블 NSQLT_DB2P · 스키마 DB2INST1).
        let c = hx("000000010000000000550001000000000000000000000000000200000000000400000000000000F0010000000000000000000000000000000002494400000000000000000000FF0001000000000000000000000000044E455841000A4E53514C545F444232500000000A4E53514C545F4442325000000008444232494E535431000000024944000000000000FF000000001400000000000000C10104B80000000000000000080000000000044E414D4500000000000000000000FF0000000000000000000000000000044E45584100");
        let cin = parse_sqlcinrd(&c, true).unwrap();
        let got: Vec<(String, String)> = cin
            .iter()
            .map(|c| (c.name.clone(), c.type_name()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("ID".to_string(), "INTEGER".to_string()),
                ("NAME".to_string(), "VARCHAR(20)".to_string())
            ]
        );
        // 라벨이 있는 열(LABEL_m = 'Customer ID')도 이름이 비지 않는다 — 2차 ⑦.
        let mut lab = hx("FF0100000000000400000000000000F101000000000000000000000000000000000249440000000B437573746F6D6572204944000000000000FFFFFF");
        lab.insert(0, 0xFF);
        let (_, lc) = parse_sqldard(&lab, true).unwrap();
        assert_eq!(
            (lc[0].name.as_str(), lc[0].label.as_str()),
            ("ID", "Customer ID")
        );
        let p = hx("FFFF0100000000000400000000000000F10100000000000000000000000000010000013100000000000000000000FFFFFF");
        let (ca, params) = parse_sqldard(&p, true).unwrap();
        assert!(ca.is_none());
        assert_eq!(params.len(), 1);
        assert_eq!(
            (params[0].name.as_str(), params[0].sqltype, params[0].length),
            ("1", 497, 4)
        );
    }
}
