//! ★ 헤드리스 캡처용 PNG 인코더(D-436 · T-459 · 11-01) — 외부 crate 0 · zlib **저장 블록**(압축 없음 · 시험 산출물이라 크기보다 단순함) ·
//! 입력 = softbuffer 프레임(`0x00RRGGBB` u32 · 행 우선) → RGB 8비트 PNG. 기동 명령 `capture:<파일>`이 쓴다(`app/paint.rs`).

/// CRC-32(IEEE · PNG 청크).
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    let mut c = Vec::with_capacity(4 + body.len());
    c.extend_from_slice(kind);
    c.extend_from_slice(body);
    out.extend_from_slice(&c);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

/// zlib 스트림(저장 블록 · 최대 65,535바이트씩) + adler32.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut z = Vec::with_capacity(raw.len() + raw.len() / 65_535 * 5 + 16);
    z.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = raw.chunks(65_535).peekable();
    if chunks.peek().is_none() {
        z.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(c) = chunks.next() {
        let last = chunks.peek().is_none();
        z.push(u8::from(last));
        let n = c.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&nexa_gfx::inflate::adler32(raw).to_be_bytes());
    z
}

/// `0x00RRGGBB` 프레임 → PNG 바이트(행마다 필터 0).
#[must_use]
pub(crate) fn encode(width: u32, height: u32, px: &[u32]) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let p = px.get(y * w + x).copied().unwrap_or(0);
            raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut out = Vec::with_capacity(raw.len() + 64);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8비트 · RGB · deflate · 필터 0 · 비인터레이스
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn be32(b: &[u8]) -> u32 {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    }

    fn inflate(z: &[u8]) -> Vec<u8> {
        match nexa_gfx::inflate::inflate_zlib(z) {
            Ok(v) => v,
            Err(e) => panic!("inflate: {e}"),
        }
    }

    /// 2×2 프레임 → PNG 머리·IHDR 값·IEND · zlib 저장 블록을 nexa-gfx 인플레이터로 되풀면 원시 행이 그대로 · CRC 표준값.
    #[test]
    fn encodes_png_round_trip_with_inflate() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926, "CRC-32 표준 검증값");
        let px = [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0x00FF_FFFF];
        let png = encode(2, 2, &px);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(be32(&png[16..20]), 2);
        assert_eq!(be32(&png[20..24]), 2);
        assert!(png.ends_with(&[b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]));
        // IDAT 본문 = 8(헤더)+25(IHDR 청크) 뒤 길이 4 + "IDAT".
        let idat_len = be32(&png[33..37]) as usize;
        assert_eq!(&png[37..41], b"IDAT");
        let raw = inflate(&png[41..41 + idat_len]);
        assert_eq!(
            raw,
            vec![0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255]
        );
        // 큰 프레임(저장 블록 2개 이상)도 되풀린다.
        let big = vec![0x0012_3456u32; 300 * 100];
        let png = encode(300, 100, &big);
        let idat_len = be32(&png[33..37]) as usize;
        let raw = inflate(&png[41..41 + idat_len]);
        assert_eq!(raw.len(), 100 * (1 + 300 * 3));
        assert_eq!(&raw[1..4], &[0x12, 0x34, 0x56]);
    }
}

/// ★ PNG 디코더(AI 첨부 미리보기 · 사용자 11-11): 8비트 · 비인터레이스 · 색 0/2/3/4/6 → RGBA 바이트. 그 밖은 `None`.
/// 압축 해제 = `nsql_drivers::ext::zip::inflate_zlib`(miniz_oxide · 이미 트리 안) · 상한 64 MB.
pub(crate) fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.len() < 33 || bytes[..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    let be32 = |o: usize| -> Option<u32> {
        bytes
            .get(o..o + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let (mut w, mut h, mut depth, mut color, mut interlace) = (0u32, 0u32, 0u8, 0u8, 0u8);
    let mut idat: Vec<u8> = Vec::new();
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut p = 8usize;
    while p + 8 <= bytes.len() {
        let len = be32(p)? as usize;
        let kind = &bytes[p + 4..p + 8];
        let body = bytes.get(p + 8..p + 8 + len)?;
        match kind {
            b"IHDR" => {
                w = be32(p + 8)?;
                h = be32(p + 12)?;
                depth = body[8];
                color = body[9];
                interlace = body[12];
            }
            b"PLTE" => palette = body.as_chunks::<3>().0.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        p += 12 + len;
    }
    if w == 0 || h == 0 || depth != 8 || interlace != 0 || w.checked_mul(h)? > 16_000_000 {
        return None;
    }
    let ch: usize = match color {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    let stride = w as usize * ch;
    let raw = nsql_drivers::ext::zip::inflate_zlib(&idat, (stride + 1) * h as usize + 1).ok()?;
    if raw.len() < (stride + 1) * h as usize {
        return None;
    }
    let mut out = vec![0u8; stride * h as usize];
    for y in 0..h as usize {
        let f = raw[y * (stride + 1)];
        let src = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        let (prev, cur) = out.split_at_mut(y * stride);
        let prev: &[u8] = if y == 0 {
            &[]
        } else {
            &prev[(y - 1) * stride..]
        };
        let cur = &mut cur[..stride];
        for i in 0..stride {
            let a = if i >= ch { cur[i - ch] } else { 0 };
            let b = if y > 0 { prev[i] } else { 0 };
            let c = if y > 0 && i >= ch { prev[i - ch] } else { 0 };
            let x = src[i];
            cur[i] = match f {
                0 => x,
                1 => x.wrapping_add(a),
                2 => x.wrapping_add(b),
                3 => x.wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                4 => {
                    let (pa, pb, pc) = {
                        let p = i16::from(a) + i16::from(b) - i16::from(c);
                        (
                            (p - i16::from(a)).abs(),
                            (p - i16::from(b)).abs(),
                            (p - i16::from(c)).abs(),
                        )
                    };
                    let pred = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    x.wrapping_add(pred)
                }
                _ => return None,
            };
        }
    }
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for px in out.chunks_exact(ch) {
        match color {
            0 => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
            2 => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
            3 => {
                let c = palette.get(px[0] as usize).copied().unwrap_or([0, 0, 0]);
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
            4 => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            _ => rgba.extend_from_slice(&[px[0], px[1], px[2], px[3]]),
        }
    }
    Some((w, h, rgba))
}

#[cfg(test)]
mod decode_tests {
    #[test]
    fn roundtrip_rgb() {
        let px = [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0x0012_3456];
        let png = super::encode(2, 2, &px);
        let (w, h, rgba) = super::decode(&png).expect("decode");
        assert_eq!((w, h), (2, 2));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[12..16], &[0x12, 0x34, 0x56, 255]);
        assert!(super::decode(b"not a png").is_none());
    }
}
