//! SQLCARD(SQLCAGRP) 해석 — 오류·경고·영향 행 수. 레이아웃 = pydrda `parse_sqlcard`(Db2 LUW 실측) · Derby `NetSqlca`.
//!
//! `지시자(1 · 0xFF = 없음) · SQLCODE(4 · TYPDEF 순서) · SQLSTATE(5) · SQLERRPROC(8) · SQLCAXGRP 지시자(1) · SQLERRD(6×4) · SQLWARN(11) ·
//! SQLRDBNAME(VCS = 2B 길이 + 바이트) · SQLERRMSG_m(VCM) · SQLERRMSG_s(VCS) · SQLDIAGGRP 지시자(1)`. 메시지 토큰 구분자 = 0xFF.

use nsql_core::DbError;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sqlca {
    pub code: i32,
    pub state: String,
    /// SQLERRD[0..6] — `errd[2]` = 영향 행 수(DML).
    pub errd: [i32; 6],
    /// SQLERRMC 토큰(0xFF 구분)을 `, `로 이은 메시지 본문.
    pub message: String,
}

impl Sqlca {
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.code < 0
    }

    /// `DbError`(SQLCODE = code · 메시지 = `SQL0204N … SQLSTATE=42704` 꼴은 서버가 주지 않으므로 토큰 + 상태).
    #[must_use]
    pub fn to_db_error(&self) -> DbError {
        let id = format!("SQL{:04}N", -self.code);
        let msg = if self.message.is_empty() {
            format!("{id} SQLSTATE={}", self.state)
        } else {
            format!("{id} {} SQLSTATE={}", self.message, self.state)
        };
        DbError {
            code: Some(i64::from(self.code)),
            message: msg,
            position: None,
        }
    }
}

fn i32_at(b: &[u8], at: usize, le: bool) -> Option<i32> {
    let s: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
    Some(if le {
        i32::from_le_bytes(s)
    } else {
        i32::from_be_bytes(s)
    })
}

/// VCS/VCM(2바이트 큰 끝 길이 + 바이트) 하나 — (바이트, 다음 오프셋).
pub(crate) fn var_bytes(b: &[u8], at: usize) -> Option<(&[u8], usize)> {
    let n = u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]) as usize;
    let end = at + 2 + n;
    Some((b.get(at + 2..end)?, end))
}

fn text_lossy(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => crate::ebcdic::to_utf8(b),
    }
}

/// SQLCARD 해석 — (SQLCA · 끝 오프셋). 지시자 0xFF = SQLCA 없음(정상 · 오프셋 1).
pub fn parse_sqlca(b: &[u8], le: bool) -> Result<(Option<Sqlca>, usize), DbError> {
    let bad = |w: &str| DbError {
        code: None,
        message: format!("DRDA SQLCARD: truncated {w}"),
        position: None,
    };
    if b.first().copied() == Some(0xFF) {
        return Ok((None, 1));
    }
    let code = i32_at(b, 1, le).ok_or_else(|| bad("sqlcode"))?;
    let state = text_lossy(b.get(5..10).ok_or_else(|| bad("sqlstate"))?);
    let mut errd = [0i32; 6];
    let mut message = String::new();
    let mut at = 18;
    match b.get(at) {
        Some(0xFF) => {
            at += 1;
        }
        Some(_) => {
            at += 1;
            for (i, e) in errd.iter_mut().enumerate() {
                *e = i32_at(b, at + i * 4, le).ok_or_else(|| bad("sqlerrd"))?;
            }
            at += 24;
            at += 11; // SQLWARN
            let (_rdb, n1) = var_bytes(b, at).ok_or_else(|| bad("sqlrdbname"))?;
            let (m, n2) = var_bytes(b, n1).ok_or_else(|| bad("sqlerrmsg_m"))?;
            let (s, n3) = var_bytes(b, n2).ok_or_else(|| bad("sqlerrmsg_s"))?;
            let raw = if m.is_empty() { s } else { m };
            message = raw
                .split(|&c| c == 0xFF)
                .map(text_lossy)
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            at = n3;
        }
        None => return Err(bad("sqlcaxgrp")),
    }
    // SQLDIAGGRP 지시자(0xFF = 없음) — 있으면 1차는 건너뛰지 못한다(길이를 모름) → 끝으로 본다.
    if b.get(at).copied() == Some(0xFF) {
        at += 1;
    }
    Ok((
        Some(Sqlca {
            code,
            state,
            errd,
            message,
        }),
        at,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vcs(s: &[u8]) -> Vec<u8> {
        let mut v = (s.len() as u16).to_be_bytes().to_vec();
        v.extend_from_slice(s);
        v
    }

    #[test]
    fn parses_error_with_tokens_and_rows_affected() {
        let mut b = vec![0x00];
        b.extend_from_slice(&(-204i32).to_le_bytes());
        b.extend_from_slice(b"42704");
        b.extend_from_slice(b"SQLRA0F9");
        b.push(0x00); // SQLCAXGRP 있음
        for v in [0i32, 0, 3, 0, 0, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[b' '; 11]);
        b.extend_from_slice(&vcs(b"NEXA"));
        b.extend_from_slice(&vcs(b"DB2INST1.NOPE\xFFT")); // _m
        b.extend_from_slice(&vcs(b"")); // _s
        b.push(0xFF);
        let (ca, at) = parse_sqlca(&b, true).unwrap();
        let ca = ca.unwrap();
        assert_eq!(at, b.len());
        assert_eq!((ca.code, ca.state.as_str(), ca.errd[2]), (-204, "42704", 3));
        assert_eq!(ca.message, "DB2INST1.NOPE, T");
        let e = ca.to_db_error();
        assert_eq!(e.code, Some(-204));
        assert!(
            e.message
                .starts_with("SQL0204N DB2INST1.NOPE, T SQLSTATE=42704"),
            "{}",
            e.message
        );
        // 0xFF = 없음.
        assert_eq!(parse_sqlca(&[0xFF, 1, 2], true).unwrap(), (None, 1));
    }
}
