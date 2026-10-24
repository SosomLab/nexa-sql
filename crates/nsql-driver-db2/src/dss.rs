//! DSS(Data Stream Structure) + DDM 객체 코덱 — DRDA의 바깥 틀.
//!
//! - **DSS 머리 6바이트** = 길이(2 · 머리 포함 · 큰 엔디안) · `0xD0`(DDMID) · 포맷(1) · 요청 상관 번호(2).
//!   포맷 바이트 = `0x40` 체인(다음 DSS가 이어짐) · `0x20` 같은 상관 번호 유지 · `0x10` 오류 뒤에도 계속 · 하위 4비트 = 종류(1 RQSDSS · 2 RPYDSS · 3 OBJDSS ·
//!   4 CMNDSS · 5 NORPYDSS).
//! - DSS 하나의 본문 = **DDM 객체 하나**: 길이(2 · 자기 포함) · 코드포인트(2) · 값(스칼라 바이트 또는 하위 객체들).
//! - 큰 객체(> 32 KB)는 DSS **연속(continuation)** 으로 쪼개진다: 길이의 최상위 비트(`0x8000`)가 서면 다음 DSS가 같은 객체의 뒷부분 — 읽을 때 이어 붙인다.
//!
//! 이 모듈은 순수 함수(소켓 없음) · 시험으로 왕복을 고정한다.

use std::fmt;

/// DSS 종류(포맷 바이트 하위 4비트).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DssKind {
    Request = 1,
    Reply = 2,
    Object = 3,
}

/// DDM 객체 — 코드포인트 + 값(바이트) · 하위 객체는 값 안에 다시 `len·cp·value`로 들어 있다(필요할 때 [`Ddm::children`]로 펼친다).
#[derive(Clone, PartialEq, Eq)]
pub struct Ddm {
    pub cp: u16,
    pub value: Vec<u8>,
}

impl fmt::Debug for Ddm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ddm(0x{:04X}, {} bytes)", self.cp, self.value.len())
    }
}

impl Ddm {
    #[must_use]
    pub fn new(cp: u16, value: Vec<u8>) -> Self {
        Ddm { cp, value }
    }

    /// 스칼라 u16 값 객체.
    #[must_use]
    pub fn u16(cp: u16, v: u16) -> Self {
        Ddm::new(cp, v.to_be_bytes().to_vec())
    }

    /// 스칼라 u32 값 객체.
    #[must_use]
    pub fn u32(cp: u16, v: u32) -> Self {
        Ddm::new(cp, v.to_be_bytes().to_vec())
    }

    /// 하위 객체들을 값으로 품는 객체(집합 객체).
    #[must_use]
    pub fn group(cp: u16, children: &[Ddm]) -> Self {
        let mut v = Vec::new();
        for c in children {
            c.encode_into(&mut v);
        }
        Ddm::new(cp, v)
    }

    /// `len(2) · cp(2) · value`.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        let len = (self.value.len() + 4) as u16;
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.cp.to_be_bytes());
        out.extend_from_slice(&self.value);
    }

    /// 값이 하위 객체 목록이면 펼친다(형식이 맞지 않으면 빈 목록).
    #[must_use]
    pub fn children(&self) -> Vec<Ddm> {
        parse_objects(&self.value).unwrap_or_default()
    }

    /// 첫 번째 하위 객체.
    #[must_use]
    pub fn child(&self, cp: u16) -> Option<Ddm> {
        self.children().into_iter().find(|c| c.cp == cp)
    }

    #[must_use]
    pub fn as_u16(&self) -> Option<u16> {
        (self.value.len() == 2).then(|| u16::from_be_bytes([self.value[0], self.value[1]]))
    }

    #[must_use]
    pub fn as_u32(&self) -> Option<u32> {
        (self.value.len() == 4).then(|| {
            u32::from_be_bytes([self.value[0], self.value[1], self.value[2], self.value[3]])
        })
    }

    #[must_use]
    pub fn as_u8(&self) -> Option<u8> {
        (self.value.len() == 1).then(|| self.value[0])
    }
}

/// 연속된 `len·cp·value` 객체들을 전부 파싱한다.
pub fn parse_objects(mut b: &[u8]) -> Result<Vec<Ddm>, DssError> {
    let mut out = Vec::new();
    while !b.is_empty() {
        if b.len() < 4 {
            return Err(DssError::Truncated("ddm header"));
        }
        let len = u16::from_be_bytes([b[0], b[1]]) as usize;
        let cp = u16::from_be_bytes([b[2], b[3]]);
        if len < 4 || len > b.len() {
            return Err(DssError::BadLength { cp, len });
        }
        out.push(Ddm::new(cp, b[4..len].to_vec()));
        b = &b[len..];
    }
    Ok(out)
}

/// DSS 하나(머리 해석 뒤).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dss {
    pub kind: DssKind,
    pub chained: bool,
    pub same_corr: bool,
    pub cont_on_err: bool,
    pub corr: u16,
    pub obj: Ddm,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DssError {
    Truncated(&'static str),
    BadMagic(u8),
    BadKind(u8),
    BadLength { cp: u16, len: usize },
}

impl fmt::Display for DssError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DssError::Truncated(w) => write!(f, "DRDA: truncated {w}"),
            DssError::BadMagic(b) => write!(f, "DRDA: bad DSS magic 0x{b:02X}"),
            DssError::BadKind(b) => write!(f, "DRDA: bad DSS kind {b}"),
            DssError::BadLength { cp, len } => {
                write!(f, "DRDA: bad DDM length {len} for 0x{cp:04X}")
            }
        }
    }
}

/// DSS 본문 상한(머리 포함 32 767) — 넘는 객체는 연속 DSS로 쪼갠다.
pub const DSS_MAX: usize = 32_767;

/// DSS 인코딩 — 객체가 상한을 넘으면 연속 비트(`0x8000`)로 쪼갠다(DRDA "DSS continuation").
pub fn encode_dss(
    kind: DssKind,
    chained: bool,
    same_corr: bool,
    corr: u16,
    obj: &Ddm,
    out: &mut Vec<u8>,
) {
    let mut body = Vec::with_capacity(obj.value.len() + 4);
    obj.encode_into(&mut body);
    let fmt_byte = |last: bool| -> u8 {
        let mut f = kind as u8;
        if chained || !last {
            f |= 0x40;
        }
        if same_corr || !last {
            f |= 0x20;
        }
        f
    };
    let max_body = DSS_MAX - 6;
    if body.len() <= max_body {
        let len = (body.len() + 6) as u16;
        out.extend_from_slice(&len.to_be_bytes());
        out.push(0xD0);
        out.push(fmt_byte(true));
        out.extend_from_slice(&corr.to_be_bytes());
        out.extend_from_slice(&body);
        return;
    }
    // 연속: 첫 조각의 DDM 길이에 0x8000 · 뒤 조각은 머리 뒤에 바로 값(길이 2바이트만 · 0x8000 유지 · 마지막은 실제 길이).
    let mut rest: &[u8] = &body;
    let mut first = true;
    while !rest.is_empty() {
        let take = rest.len().min(max_body);
        let last = take == rest.len();
        let len = (take + 6) as u16;
        out.extend_from_slice(&len.to_be_bytes());
        out.push(0xD0);
        out.push(fmt_byte(last));
        out.extend_from_slice(&corr.to_be_bytes());
        if first {
            // DDM 길이 자리에 0x8000 표식.
            let mut chunk = rest[..take].to_vec();
            chunk[0] = 0x80;
            chunk[1] = 0x00;
            out.extend_from_slice(&chunk);
            first = false;
        } else {
            let seg_len = (take + 2) as u16 | if last { 0 } else { 0x8000 };
            // 연속 조각 = 길이(2) + 바이트(길이에 2를 더한 만큼이 조각 전체).
            let len2 = (take + 2 + 6) as u16;
            let n = out.len();
            // 머리 길이를 조각 길이(길이 2바이트 포함)에 맞춰 다시 쓴다.
            let hdr_at = n - 6;
            out[hdr_at..hdr_at + 2].copy_from_slice(&len2.to_be_bytes());
            out.extend_from_slice(&seg_len.to_be_bytes());
            out.extend_from_slice(&rest[..take]);
        }
        rest = &rest[take..];
    }
}

/// 버퍼 맨 앞의 DSS 하나(연속 조각 포함)를 해석한다 — 돌려주는 값 = (DSS, 먹은 바이트 수). 부족하면 `None`(더 읽어야 함).
pub fn decode_dss(buf: &[u8]) -> Result<Option<(Dss, usize)>, DssError> {
    if buf.len() < 6 {
        return Ok(None);
    }
    let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    if buf[2] != 0xD0 {
        return Err(DssError::BadMagic(buf[2]));
    }
    if len < 6 {
        return Err(DssError::BadLength { cp: 0, len });
    }
    if buf.len() < len {
        return Ok(None);
    }
    let f = buf[3];
    let kind = match f & 0x0F {
        1 => DssKind::Request,
        2 => DssKind::Reply,
        3 => DssKind::Object,
        k => return Err(DssError::BadKind(k)),
    };
    let corr = u16::from_be_bytes([buf[4], buf[5]]);
    let mut body = buf[6..len].to_vec();
    let mut used = len;
    if body.len() < 4 {
        return Err(DssError::Truncated("ddm header"));
    }
    let mut ddm_len = u16::from_be_bytes([body[0], body[1]]);
    // 연속 조각 이어 붙이기.
    while ddm_len & 0x8000 != 0 {
        if buf.len() < used + 6 {
            return Ok(None);
        }
        let l2 = u16::from_be_bytes([buf[used], buf[used + 1]]) as usize;
        if buf[used + 2] != 0xD0 || l2 < 8 {
            return Err(DssError::Truncated("dss continuation"));
        }
        if buf.len() < used + l2 {
            return Ok(None);
        }
        let seg = &buf[used + 6..used + l2];
        ddm_len = u16::from_be_bytes([seg[0], seg[1]]);
        body.extend_from_slice(&seg[2..]);
        used += l2;
    }
    let cp = u16::from_be_bytes([body[2], body[3]]);
    let obj = Ddm::new(cp, body[4..].to_vec());
    Ok(Some((
        Dss {
            kind,
            chained: f & 0x40 != 0,
            same_corr: f & 0x20 != 0,
            cont_on_err: f & 0x10 != 0,
            corr,
            obj,
        },
        used,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddm_round_trip_and_children() {
        let g = Ddm::group(
            0x1041,
            &[Ddm::u16(0x1404, 7), Ddm::new(0x115E, b"nexa".to_vec())],
        );
        let kids = g.children();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].as_u16(), Some(7));
        assert_eq!(kids[1].value, b"nexa");
        assert_eq!(g.child(0x115E).map(|c| c.value), Some(b"nexa".to_vec()));
        let mut v = Vec::new();
        g.encode_into(&mut v);
        assert_eq!(&v[..4], &[0x00, 0x12, 0x10, 0x41], "4 + (4+2) + (4+4) = 18");
        assert_eq!(parse_objects(&v).unwrap(), vec![g]);
    }

    #[test]
    fn dss_header_bits_and_round_trip() {
        let obj = Ddm::group(0x2001, &[Ddm::u16(0x210F, 0x2407)]);
        let mut out = Vec::new();
        encode_dss(DssKind::Request, true, false, 1, &obj, &mut out);
        assert_eq!(out[2], 0xD0);
        assert_eq!(out[3], 0x41, "RQSDSS + chained");
        let (d, used) = decode_dss(&out).unwrap().expect("one dss");
        assert_eq!(used, out.len());
        assert_eq!(
            (d.kind, d.chained, d.same_corr, d.corr),
            (DssKind::Request, true, false, 1)
        );
        assert_eq!(d.obj, obj);
        // 부족한 버퍼 = None · 마법 바이트 틀림 = 오류.
        assert_eq!(decode_dss(&out[..5]).unwrap(), None);
        let mut bad = out.clone();
        bad[2] = 0x00;
        assert!(decode_dss(&bad).is_err());
    }

    #[test]
    fn dss_continuation_splits_and_joins_large_objects() {
        let big = Ddm::new(0x241B, vec![0xAB; 70_000]);
        let mut out = Vec::new();
        encode_dss(DssKind::Object, false, false, 2, &big, &mut out);
        // 첫 DSS는 상한 길이 · 뒤 조각이 있다.
        let l0 = u16::from_be_bytes([out[0], out[1]]) as usize;
        assert_eq!(l0, DSS_MAX);
        assert!(out.len() > l0 + 6);
        let (d, used) = decode_dss(&out).unwrap().expect("joined");
        assert_eq!(used, out.len());
        assert_eq!(d.obj.cp, 0x241B);
        assert_eq!(d.obj.value.len(), 70_000);
        assert!(d.obj.value.iter().all(|&b| b == 0xAB));
    }
}
