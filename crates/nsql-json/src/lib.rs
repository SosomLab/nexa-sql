//! ★ 최소 JSON(의존 0 · 136차 부품화 · 30 §2): 객체·배열·문자열·숫자·불·null 파서와 작성기. 원래 `nsql_settings::json`에 있던
//! 것을 설정 밖(확장 드라이버 매니페스트 `nexa-driver.json` · nexa-sql 확장 관리자)도 쓰도록 옮겼다 — settings는 재수출로 경로 유지.

/// 최소 JSON 값.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn scalar_text(&self) -> Option<String> {
        match self {
            Json::Null => Some(String::new()),
            Json::Bool(b) => Some(if *b { "on".into() } else { "off".into() }),
            Json::Num(n) => Some(if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }),
            Json::Str(s) => Some(s.clone()),
            Json::Arr(items) => Some(
                items
                    .iter()
                    .filter_map(Json::scalar_text)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            Json::Obj(_) => None,
        }
    }
}

/// JSON 텍스트 파싱(오류 = 위치 포함 메시지).
/// 값 → JSON 글(공백 없이 · 문자열 이스케이프). 프로젝트 파일에 내장된 객체(북마크)를 그대로 되돌릴 때 쓴다(09-23).
pub fn dump(v: &Json) -> String {
    fn esc(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }
    match v {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Json::Str(s) => format!("\"{}\"", esc(s)),
        Json::Arr(a) => format!("[{}]", a.iter().map(dump).collect::<Vec<_>>().join(",")),
        Json::Obj(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("\"{}\":{}", esc(k), dump(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(format!("trailing characters at {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at {}", c as char, self.i))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'n') => self.lit("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected '{}' at {}", c as char, self.i)),
            None => Err("unexpected end".into()),
        }
    }

    fn lit(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("invalid literal at {}", self.i))
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        while self.i < self.b.len()
            && matches!(
                self.b[self.i],
                b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
            )
        {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        s.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("bad number '{s}' at {start}"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err("unterminated string".into());
            };
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err("bad escape".into());
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self.b.get(self.i..self.i + 4).ok_or("bad \\u escape")?;
                            let cp = u32::from_str_radix(
                                std::str::from_utf8(hex).map_err(|e| e.to_string())?,
                                16,
                            )
                            .map_err(|e| e.to_string())?;
                            self.i += 4;
                            // 대리쌍
                            let ch = if (0xD800..0xDC00).contains(&cp)
                                && self.b.get(self.i) == Some(&b'\\')
                                && self.b.get(self.i + 1) == Some(&b'u')
                            {
                                let hex2 =
                                    self.b.get(self.i + 2..self.i + 6).ok_or("bad surrogate")?;
                                let lo = u32::from_str_radix(
                                    std::str::from_utf8(hex2).map_err(|e| e.to_string())?,
                                    16,
                                )
                                .map_err(|e| e.to_string())?;
                                self.i += 6;
                                0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                cp
                            };
                            out.push(char::from_u32(ch).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(format!("bad escape at {}", self.i)),
                    }
                }
                _ => {
                    // UTF-8 바이트 → 문자(멀티바이트는 통째로).
                    let start = self.i - 1;
                    let len = utf8_len(c);
                    let end = (start + len).min(self.b.len());
                    out.push_str(std::str::from_utf8(&self.b[start..end]).unwrap_or("\u{FFFD}"));
                    self.i = end;
                }
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at {}", self.i)),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(items));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            let v = self.value()?;
            items.push((k, v));
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(items));
                }
                _ => return Err(format!("expected ',' or '}}' at {}", self.i)),
            }
        }
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip_and_unicode() {
        let v =
            parse(r#"{"a": {"b": [1, 2.5, "x\ny", true, null]}, "k": "한글 é"}"#).expect("json");
        match v {
            Json::Obj(items) => {
                assert_eq!(items[0].0, "a");
                assert_eq!(items[1].1, Json::Str("한글 é".into()));
            }
            _ => panic!(),
        }
        assert!(parse("{").is_err());
        assert!(parse("{\"a\":1} x").is_err());
    }
}
