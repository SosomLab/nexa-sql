//! JSON Lines(한 줄 = 객체 하나) **스트리밍** 리더 — 대량 적재의 두 번째 원료(docs/89 · B-4). 외부 crate 0: 평평한 객체만
//! (문자열 · 숫자 · true/false · null) · 중첩 객체/배열은 오류 · 값은 글로 돌려준다(null = `None`).

use std::io::{self, BufRead};

/// 레코드 단위 리더(줄마다 객체 하나 · 빈 줄 건너뜀).
#[derive(Debug)]
pub struct JsonlReader<R: BufRead> {
    r: R,
    line: String,
    /// 마지막으로 돌려준 레코드의 줄 번호(1부터).
    pub line_no: u64,
    cur: u64,
}

impl<R: BufRead> JsonlReader<R> {
    pub fn new(r: R) -> Self {
        JsonlReader {
            r,
            line: String::new(),
            line_no: 0,
            cur: 0,
        }
    }

    /// 다음 객체 = (키, 값) 목록(등장 순서 · null = None). 끝 = `Ok(None)` · 문법 오류 = `Err`(줄 번호 포함).
    #[allow(clippy::type_complexity)]
    pub fn next_object(&mut self) -> io::Result<Option<Vec<(String, Option<String>)>>> {
        loop {
            self.line.clear();
            let n = self.r.read_line(&mut self.line)?;
            if n == 0 {
                return Ok(None);
            }
            self.cur += 1;
            let t = self.line.trim();
            if t.is_empty() {
                continue;
            }
            let t = t.strip_prefix('\u{feff}').unwrap_or(t);
            self.line_no = self.cur;
            return parse_object(t).map(Some).map_err(|m| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {}: {m}", self.cur),
                )
            });
        }
    }
}

/// 평평한 JSON 객체 → (키, 값) 목록.
pub fn parse_object(s: &str) -> Result<Vec<(String, Option<String>)>, String> {
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let skip_ws = |i: &mut usize| {
        while *i < b.len() && b[*i].is_whitespace() {
            *i += 1;
        }
    };
    skip_ws(&mut i);
    if b.get(i) != Some(&'{') {
        return Err("expected '{'".into());
    }
    i += 1;
    let mut out = Vec::new();
    loop {
        skip_ws(&mut i);
        match b.get(i) {
            Some('}') => {
                i += 1;
                break;
            }
            Some('"') => {}
            _ => return Err("expected key".into()),
        }
        let key = parse_string(&b, &mut i)?;
        skip_ws(&mut i);
        if b.get(i) != Some(&':') {
            return Err(format!("expected ':' after key {key:?}"));
        }
        i += 1;
        skip_ws(&mut i);
        let val = match b.get(i) {
            Some('"') => Some(parse_string(&b, &mut i)?),
            Some('t') if b[i..].starts_with(&['t', 'r', 'u', 'e']) => {
                i += 4;
                Some("true".into())
            }
            Some('f') if b[i..].starts_with(&['f', 'a', 'l', 's', 'e']) => {
                i += 5;
                Some("false".into())
            }
            Some('n') if b[i..].starts_with(&['n', 'u', 'l', 'l']) => {
                i += 4;
                None
            }
            Some('{') | Some('[') => {
                return Err(format!("nested value for key {key:?} is not supported"))
            }
            Some(c) if *c == '-' || c.is_ascii_digit() => {
                let start = i;
                while i < b.len()
                    && (b[i].is_ascii_digit() || matches!(b[i], '-' | '+' | '.' | 'e' | 'E'))
                {
                    i += 1;
                }
                Some(b[start..i].iter().collect())
            }
            _ => return Err(format!("bad value for key {key:?}")),
        };
        out.push((key, val));
        skip_ws(&mut i);
        match b.get(i) {
            Some(',') => i += 1,
            Some('}') => {
                i += 1;
                break;
            }
            _ => return Err("expected ',' or '}'".into()),
        }
    }
    skip_ws(&mut i);
    if i != b.len() {
        return Err("trailing characters".into());
    }
    Ok(out)
}

fn parse_string(b: &[char], i: &mut usize) -> Result<String, String> {
    // b[*i] == '"'
    *i += 1;
    let mut s = String::new();
    while *i < b.len() {
        let c = b[*i];
        *i += 1;
        match c {
            '"' => return Ok(s),
            '\\' => {
                let e = *b.get(*i).ok_or("bad escape")?;
                *i += 1;
                match e {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'b' => s.push('\u{8}'),
                    'f' => s.push('\u{c}'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'u' => {
                        let hex: String = b.get(*i..*i + 4).ok_or("bad \\u")?.iter().collect();
                        *i += 4;
                        let cp = u32::from_str_radix(&hex, 16).map_err(|_| "bad \\u")?;
                        // 대리쌍.
                        let ch = if (0xD800..0xDC00).contains(&cp)
                            && b.get(*i..*i + 2) == Some(&['\\', 'u'])
                        {
                            let lo: String =
                                b.get(*i + 2..*i + 6).ok_or("bad \\u")?.iter().collect();
                            let lo = u32::from_str_radix(&lo, 16).map_err(|_| "bad \\u")?;
                            *i += 6;
                            0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00)
                        } else {
                            cp
                        };
                        s.push(char::from_u32(ch).unwrap_or('\u{fffd}'));
                    }
                    _ => return Err("bad escape".into()),
                }
            }
            c => s.push(c),
        }
    }
    Err("unterminated string".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_objects_and_streams() {
        let text = "{\"id\": 1, \"name\": \"kim \\\"k\\\"\", \"amt\": -1.5e2, \"ok\": true, \"memo\": null}\n\n{\"id\":2,\"name\":\"\\ud55c\\uae00\"}\n";
        let mut r = JsonlReader::new(text.as_bytes());
        let o = r.next_object().unwrap().unwrap();
        assert_eq!(
            o,
            vec![
                ("id".into(), Some("1".into())),
                ("name".into(), Some("kim \"k\"".into())),
                ("amt".into(), Some("-1.5e2".into())),
                ("ok".into(), Some("true".into())),
                ("memo".into(), None),
            ]
        );
        assert_eq!(r.line_no, 1);
        let o2 = r.next_object().unwrap().unwrap();
        assert_eq!(o2[1].1.as_deref(), Some("한글"));
        assert_eq!(r.line_no, 3);
        assert!(r.next_object().unwrap().is_none());
        assert!(parse_object("{\"a\": [1]}").is_err());
        assert!(parse_object("{\"a\": 1} x").is_err());
        assert!(parse_object("[1]").is_err());
    }
}
