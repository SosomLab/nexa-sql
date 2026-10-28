//! DSS(Data Stream Structure) + DDM 객체 코덱 — DRDA의 바깥 틀.
//!
//! - **DSS 머리 6바이트** = 길이(2 · 머리 포함 · 큰 엔디안) · `0xD0`(DDMID) · 포맷(1) · 요청 상관 번호(2).
//!   포맷 바이트(pydrda `write_request_dss` · Wireshark packet-drda) = `0x40` 체인(다음 DSS가 이어짐) · `0x10` **다음 DSS가 같은 상관 번호**(명령 뒤에 딸린
//!   SQLSTT·SQLDTA 객체) · `0x20` 오류 뒤에도 계속 · 하위 4비트 = 종류(1 RQSDSS · 2 RPYDSS · 3 OBJDSS · 4 CMNDSS · 5 NORPYDSS).
//! - DSS 하나의 본문 = **DDM 객체 하나**: 길이(2 · 자기 포함) · 코드포인트(2) · 값(스칼라 바이트 또는 하위 객체들). DDM 길이의 최상위 비트가 서면
//!   **확장 길이**(`0x8004` = 코드포인트 뒤 4바이트가 실제 길이 · Derby `Reply.parseLengthAndMatchCodePoint`).
//! - 큰 응답(QRYDTA · EXTDTA)은 DSS **연속(continuation · Derby `Reply.readDssHeader`)**: DSS 길이의 최상위 비트가 서면(Db2 = `0xFFFF`) 이 조각은
//!   32 767바이트이고, 뒤에 `2바이트 조각 길이(자기 포함 · 최상위 비트 = 또 이어짐) + 바이트`가 반복된다 — 읽을 때 이어 붙인다.
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
        if self.value.len() + 4 > 0x7FFF {
            // DDM 확장 길이(2차 ⑥ · 큰 EXTDTA 입력 · derby `buildLengthAndCodePointForLob` · jb sql_dta.rs): 2 B 필드 = `0x8000 | (4 + 확장 바이트 수)`
            //   → `80 08` + 코드포인트 + u32 큰 끝 = **머리 4 B를 뺀 본문 길이**(jb 시험: 32 768 B 데이터 + 널 1 B → `80 08 14 6C 00 00 80 01`).
            out.extend_from_slice(&0x8008u16.to_be_bytes());
            out.extend_from_slice(&self.cp.to_be_bytes());
            out.extend_from_slice(&(self.value.len() as u32).to_be_bytes());
        } else {
            let len = (self.value.len() + 4) as u16;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(&self.cp.to_be_bytes());
        }
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

/// DSS 인코딩(요청 쪽). 본문이 상한을 넘으면 **연속 DSS**로 쪼갠다(2차 ⑥ · [`decode_dss`]의 거울상 = DSS 길이 `0xFFFF` · 첫 조각 32 767 B(머리 포함) ·
/// 이후 조각 = 2 B 머리(`0x8000` = 더 있음 | 길이 · 길이는 머리 2 B 포함 · 최대 32 767) + 데이터).
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
    let mut f = kind as u8;
    if chained {
        f |= 0x40;
    }
    if same_corr {
        f |= 0x10;
    }
    if body.len() + 6 <= DSS_MAX {
        out.extend_from_slice(&((body.len() + 6) as u16).to_be_bytes());
        out.push(0xD0);
        out.push(f);
        out.extend_from_slice(&corr.to_be_bytes());
        out.extend_from_slice(&body);
        return;
    }
    out.extend_from_slice(&0xFFFFu16.to_be_bytes());
    out.push(0xD0);
    out.push(f);
    out.extend_from_slice(&corr.to_be_bytes());
    let first = DSS_MAX - 6;
    out.extend_from_slice(&body[..first]);
    let mut rest = &body[first..];
    while !rest.is_empty() {
        let take = rest.len().min(DSS_MAX - 2);
        let more = take < rest.len();
        let hdr = ((take + 2) as u16) | if more { 0x8000 } else { 0 };
        out.extend_from_slice(&hdr.to_be_bytes());
        out.extend_from_slice(&rest[..take]);
        rest = &rest[take..];
    }
}

/// 버퍼 맨 앞의 DSS 하나(연속 조각 포함)를 해석한다 — 돌려주는 값 = (DSS, 먹은 바이트 수). 부족하면 `None`(더 읽어야 함).
pub fn decode_dss(buf: &[u8]) -> Result<Option<(Dss, usize)>, DssError> {
    if buf.len() < 6 {
        return Ok(None);
    }
    let raw_len = u16::from_be_bytes([buf[0], buf[1]]);
    if buf[2] != 0xD0 {
        return Err(DssError::BadMagic(buf[2]));
    }
    // 최상위 비트 = 연속 DSS(이 조각 = 32 767바이트 · Db2는 0xFFFF로 보낸다).
    let continued = raw_len & 0x8000 != 0;
    let len = if continued { DSS_MAX } else { raw_len as usize };
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
    if continued {
        loop {
            if buf.len() < used + 2 {
                return Ok(None);
            }
            let seg = u16::from_be_bytes([buf[used], buf[used + 1]]);
            let more = seg & 0x8000 != 0;
            let seg_len = (seg & 0x7FFF) as usize;
            if seg_len < 2 {
                return Err(DssError::Truncated("dss continuation"));
            }
            if buf.len() < used + seg_len {
                return Ok(None);
            }
            body.extend_from_slice(&buf[used + 2..used + seg_len]);
            used += seg_len;
            if !more {
                break;
            }
        }
    }
    if body.len() < 4 {
        return Err(DssError::Truncated("ddm header"));
    }
    let ddm_len = u16::from_be_bytes([body[0], body[1]]);
    let cp = u16::from_be_bytes([body[2], body[3]]);
    // DDM 확장 길이(derby `Reply.readLengthAndCodePoint` = `ddmLength - 0x8004` = 확장 바이트 수): `0x8000 | (4 + n)` → 코드포인트 뒤 n(4/6/8)바이트가
    // 본문 길이(값은 그 뒤부터 · 길이 값 자체는 DSS 연속을 합친 본문이 말해 주므로 쓰지 않는다).
    // ★ `0x8004`(n = 0 · 157차 · CI 40641635510 = Db2 12.1이 32 KB 넘는 QRYDTA를 이 꼴로 보낸다) = 확장 길이 바이트 없음 →
    //   본문 = 연속 DSS를 합친 나머지 전부(derby `case 0`).
    let start = if ddm_len & 0x8000 != 0 {
        let n = ((ddm_len & 0x7FFF) as usize).saturating_sub(4);
        if !matches!(n, 0 | 4 | 6 | 8) {
            return Err(DssError::BadLength {
                cp,
                len: ddm_len as usize,
            });
        }
        if body.len() < 4 + n {
            return Err(DssError::Truncated("ddm extended length"));
        }
        4 + n
    } else {
        4
    };
    let obj = Ddm::new(cp, body[start..].to_vec());
    Ok(Some((
        Dss {
            kind,
            chained: f & 0x40 != 0,
            same_corr: f & 0x10 != 0,
            cont_on_err: f & 0x20 != 0,
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
        let mut o2 = Vec::new();
        encode_dss(DssKind::Request, true, true, 1, &obj, &mut o2);
        assert_eq!(o2[3], 0x51, "chained + next same corr(0x10)");
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

    /// 157차: `80 04 24 1B` = 확장 길이 바이트 0(본문 = 나머지 전부) — Db2 12.1 큰 QRYDTA(CI 40641635510 실제 바이트 꼴).
    #[test]
    fn ddm_extended_length_zero_bytes_means_rest_of_body() {
        let wire = [
            0x00, 0x0C, 0xD0, 0x03, 0x00, 0x02, 0x80, 0x04, 0x24, 0x1B, 0xAA, 0xBB,
        ];
        let (d, used) = decode_dss(&wire).expect("decode").expect("complete");
        assert_eq!(used, 12);
        assert_eq!(d.obj.cp, 0x241B);
        assert_eq!(d.obj.value, vec![0xAA, 0xBB]);
        // 모르는 확장 크기(n = 2)는 여전히 거절.
        let bad = [
            0x00, 0x0C, 0xD0, 0x03, 0x00, 0x02, 0x80, 0x06, 0x24, 0x1B, 0xAA, 0xBB,
        ];
        assert!(matches!(decode_dss(&bad), Err(DssError::BadLength { .. })));
    }

    #[test]
    fn dss_continuation_segments_and_ddm_extended_length() {
        // Db2/derby 꼴: DSS 길이 0xFFFF(연속) · 본문 32 761바이트 = DDM 머리 `80 08 24 1B`(확장 4 B) + u32 본문 길이 + 값 앞부분 · 뒤에 조각 2개(첫 조각 high bit).
        let total: usize = 40_000;
        let mut val = vec![0xAB; total];
        val[0] = 0x01;
        val[total - 1] = 0x02;
        let mut wire = Vec::new();
        wire.extend_from_slice(&[0xFF, 0xFF, 0xD0, 0x03, 0x00, 0x02]);
        wire.extend_from_slice(&[0x80, 0x08, 0x24, 0x1B]);
        wire.extend_from_slice(&(total as u32).to_be_bytes());
        let first = DSS_MAX - 6 - 4 - 4;
        wire.extend_from_slice(&val[..first]);
        let rest = &val[first..];
        let seg1 = 3000usize;
        wire.extend_from_slice(&(((seg1 + 2) as u16) | 0x8000).to_be_bytes());
        wire.extend_from_slice(&rest[..seg1]);
        let seg2 = rest.len() - seg1;
        wire.extend_from_slice(&((seg2 + 2) as u16).to_be_bytes());
        wire.extend_from_slice(&rest[seg1..]);
        let (d, used) = decode_dss(&wire).unwrap().expect("joined");
        assert_eq!(used, wire.len());
        assert_eq!(d.obj.cp, 0x241B);
        assert_eq!(d.obj.value.len(), total);
        assert_eq!((d.obj.value[0], d.obj.value[total - 1]), (0x01, 0x02));
        // 조각이 아직 안 왔으면 None.
        assert_eq!(decode_dss(&wire[..wire.len() - 10]).unwrap(), None);
    }

    #[test]
    fn encode_dss_splits_large_objects_and_round_trips() {
        // 작은 객체 = 한 DSS · 32 767 B 넘는 객체(EXTDTA 200 000 B) = 연속 조각 · DDM 확장 길이 → 우리 해독기로 그대로 복원.
        for total in [10usize, DSS_MAX - 10, DSS_MAX - 9, 70_000, 200_000] {
            let mut val = vec![0x5A; total];
            if total > 1 {
                val[0] = 0x01;
                val[total - 1] = 0x02;
            }
            let obj = Ddm::new(0x146C, val.clone());
            let mut wire = Vec::new();
            encode_dss(DssKind::Object, true, false, 7, &obj, &mut wire);
            if total + 4 + 6 <= DSS_MAX {
                assert_eq!(wire.len(), total + 10, "{total}");
            } else {
                assert_eq!(&wire[..2], &[0xFF, 0xFF], "{total}");
            }
            let (d, used) = decode_dss(&wire).unwrap().expect("decoded");
            assert_eq!(used, wire.len(), "{total}");
            assert_eq!(
                (d.kind, d.chained, d.same_corr, d.corr),
                (DssKind::Object, true, false, 7)
            );
            assert_eq!(d.obj.cp, 0x146C);
            assert_eq!(d.obj.value, val, "{total}");
        }
        // 뒤에 다른 DSS가 이어져도 경계가 맞는다.
        let big = Ddm::new(0x146C, vec![0x33; 50_000]);
        let small = Ddm::new(0x2113, vec![0x44; 64]);
        let mut wire = Vec::new();
        encode_dss(DssKind::Object, true, true, 1, &big, &mut wire);
        encode_dss(DssKind::Object, false, false, 1, &small, &mut wire);
        let (d1, u1) = decode_dss(&wire).unwrap().expect("first");
        assert_eq!(d1.obj.value.len(), 50_000);
        let (d2, u2) = decode_dss(&wire[u1..]).unwrap().expect("second");
        assert_eq!(
            (d2.obj.cp, d2.obj.value.len(), u1 + u2),
            (0x2113, 64, wire.len())
        );
    }
}
