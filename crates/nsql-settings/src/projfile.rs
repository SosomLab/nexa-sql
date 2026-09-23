//! 프로젝트 파일(`.nsql-project`)의 **두 층** 구조(사용자 09-23 "메타는 일반 텍스트 · payload는 바이너리 · 탭별 CDATA식"):
//!
//! ```text
//! { …JSON 헤더(폴더 · 탭 메타 · 패널 · 북마크)… }
//! %%NSQL-BLOBS%%
//! tab=<id> len=<바이트 수>
//! <payload · len 바이트 그대로(이스케이프 없음)>
//! tab=<id> len=<바이트 수>
//! <payload>
//! ```
//!
//! - 헤더는 마커 줄 `%%NSQL-BLOBS%%` **앞까지**의 텍스트 = 종전 JSON 그대로(`json::parse`). 마커가 없으면 옛 파일(v1 · 본문이 JSON 안 `text`).
//! - 블록 = 메타 한 줄(`tab=<id> len=<n>` · 일반 텍스트) + 줄바꿈 + **payload n 바이트** + 줄바꿈. payload는 길이로 자르므로
//!   어떤 바이트가 와도(마커 · 줄바꿈 · 따옴표) 안전하고, JSON 이스케이프·재파싱 비용이 없다(큰 본문에 유리).
//! - 이 부품은 헤더를 해석하지 않는다(GUI `project.rs`와 CLI `nsql bookmark --project`가 같은 파일을 쓰므로 나누기/합치기만 공유).

/// 헤더와 블록을 가르는 마커 줄.
pub const MARKER: &str = "%%NSQL-BLOBS%%";

/// 탭 id별 payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blob {
    pub tab: u64,
    pub data: Vec<u8>,
}

/// 파일 바이트 → (헤더 텍스트, 블록들). 헤더가 UTF-8이 아니면 `Err`. 깨진 블록(길이 부족 · 메타 줄 형식 오류)은 그 자리에서 멈춘다(앞 것은 유지).
pub fn split(data: &[u8]) -> Result<(String, Vec<Blob>), String> {
    let needle = format!("\n{MARKER}\n");
    let nb = needle.as_bytes();
    let pos = data.windows(nb.len()).position(|w| w == nb);
    let (head, mut rest) = match pos {
        Some(i) => (&data[..i], &data[i + nb.len()..]),
        None => (data, &data[data.len()..]),
    };
    let header = std::str::from_utf8(head)
        .map_err(|e| format!("project header is not UTF-8: {e}"))?
        .to_string();
    let mut blobs = Vec::new();
    while !rest.is_empty() {
        let Some(nl) = rest.iter().position(|&b| b == b'\n') else {
            break;
        };
        let Ok(meta) = std::str::from_utf8(&rest[..nl]) else {
            break;
        };
        let Some((tab, len)) = parse_meta(meta) else {
            break;
        };
        let start = nl + 1;
        let Some(end) = start.checked_add(len).filter(|&e| e <= rest.len()) else {
            break;
        };
        blobs.push(Blob {
            tab,
            data: rest[start..end].to_vec(),
        });
        rest = &rest[end..];
        if rest.first() == Some(&b'\n') {
            rest = &rest[1..];
        }
    }
    Ok((header, blobs))
}

fn parse_meta(line: &str) -> Option<(u64, usize)> {
    let mut tab = None;
    let mut len = None;
    for kv in line.split_whitespace() {
        let (k, v) = kv.split_once('=')?;
        match k {
            "tab" => tab = v.parse().ok(),
            "len" => len = v.parse().ok(),
            _ => {}
        }
    }
    Some((tab?, len?))
}

/// (헤더 텍스트, 블록들) → 파일 바이트. 블록이 없으면 헤더만(옛 파일과 같은 모양).
#[must_use]
pub fn join(header: &str, blobs: &[Blob]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(header.len() + blobs.iter().map(|b| b.data.len() + 32).sum::<usize>());
    out.extend_from_slice(header.trim_end_matches('\n').as_bytes());
    out.push(b'\n');
    if blobs.is_empty() {
        return out;
    }
    out.extend_from_slice(MARKER.as_bytes());
    out.push(b'\n');
    for b in blobs {
        out.extend_from_slice(format!("tab={} len={}\n", b.tab, b.data.len()).as_bytes());
        out.extend_from_slice(&b.data);
        out.push(b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 헤더 + 블록 둘 왕복 · payload 안의 마커·줄바꿈·따옴표·비UTF-8 바이트도 그대로.
    #[test]
    fn round_trip_with_hostile_payload() {
        let header = "{ \"version\": 2, \"tabs\": [ { \"id\": 7 }, { \"id\": 9 } ] }";
        let hostile = format!("line1\n{MARKER}\n\"quoted\" \\ tab=1 len=2\n").into_bytes();
        let mut bin = hostile.clone();
        bin.extend_from_slice(&[0xFF, 0x00, 0xFE]);
        let blobs = vec![
            Blob {
                tab: 7,
                data: hostile,
            },
            Blob { tab: 9, data: bin },
        ];
        let bytes = join(header, &blobs);
        let (h, b) = split(&bytes).expect("split");
        assert_eq!(h, header);
        assert_eq!(b, blobs);
        assert!(
            std::str::from_utf8(&bytes).is_err(),
            "payload is raw bytes, not escaped"
        );
    }

    /// 옛 파일(마커 없음) = 헤더만 · 빈 블록 목록. 블록 없이 합치면 헤더 + 줄바꿈뿐.
    #[test]
    fn legacy_header_only() {
        let (h, b) = split(b"{ \"version\": 1 }\n").expect("split");
        assert_eq!(h, "{ \"version\": 1 }\n");
        assert!(b.is_empty());
        assert_eq!(join("{ \"version\": 1 }\n", &[]), b"{ \"version\": 1 }\n");
    }

    /// 잘린 블록(길이 부족)은 거기서 멈추고 앞 것은 유지 · 메타 줄이 깨져도 마찬가지.
    #[test]
    fn truncated_block_stops_gracefully() {
        let ok = join(
            "{}",
            &[Blob {
                tab: 1,
                data: b"abc".to_vec(),
            }],
        );
        let mut cut = ok.clone();
        cut.extend_from_slice(b"tab=2 len=100\nshort\n");
        let (_, b) = split(&cut).expect("split");
        assert_eq!(b.len(), 1);
        let mut bad = ok;
        bad.extend_from_slice(b"garbage line\nxyz\n");
        let (_, b) = split(&bad).expect("split");
        assert_eq!(b.len(), 1);
    }
}
