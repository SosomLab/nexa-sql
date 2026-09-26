//! 구분자 텍스트(CSV/TSV) **스트리밍** 리더(RFC 4180 · `""` 이스케이프 · 따옴표 안 줄바꿈 · BOM) — 대량 적재의 원료(docs/89 §2 5).
//! 파일 전체를 들지 않는다: 레코드 하나씩 `BufRead`에서 읽는다(따옴표가 열린 채 줄이 끝나면 다음 줄을 이어 붙인다).

use std::io::{self, BufRead};

/// 레코드 단위 리더.
#[derive(Debug)]
pub struct DelimReader<R: BufRead> {
    r: R,
    delim: u8,
    line: String,
    /// 마지막으로 돌려준 레코드의 **시작** 줄 번호(1부터 · 오류 지목용).
    pub line_no: u64,
    /// 지금까지 읽은 줄 수.
    cur_line: u64,
    first: bool,
}

impl<R: BufRead> DelimReader<R> {
    pub fn new(r: R, delim: u8) -> Self {
        DelimReader {
            r,
            delim,
            line: String::new(),
            line_no: 0,
            cur_line: 0,
            first: true,
        }
    }

    /// 다음 레코드(없으면 `None`). 따옴표 안 줄바꿈은 그 레코드에 포함된다.
    pub fn next_record(&mut self) -> io::Result<Option<Vec<String>>> {
        let mut rec = String::new();
        loop {
            self.line.clear();
            let n = self.r.read_line(&mut self.line)?;
            if n == 0 {
                if rec.is_empty() {
                    return Ok(None);
                }
                break;
            }
            if self.first {
                self.first = false;
                if let Some(rest) = self.line.strip_prefix('\u{feff}') {
                    self.line = rest.to_string();
                }
            }
            self.cur_line += 1;
            if rec.is_empty() {
                self.line_no = self.cur_line;
            }
            rec.push_str(&self.line);
            if !open_quote(&rec) {
                break;
            }
        }
        // 빈 줄은 건너뛴다.
        let trimmed = rec.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            return self.next_record();
        }
        Ok(Some(split_record(trimmed, self.delim)))
    }
}

/// 따옴표가 열린 채 끝났는가(레코드가 다음 줄로 이어진다).
fn open_quote(s: &str) -> bool {
    let mut in_q = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if in_q && chars.peek() == Some(&'"') {
                chars.next();
            } else {
                in_q = !in_q;
            }
        }
    }
    in_q
}

/// 레코드 한 줄 → 필드(따옴표 풀기).
pub fn split_record(s: &str, delim: u8) -> Vec<String> {
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_q = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_q = true,
            c if c as u32 == u32::from(delim) => row.push(std::mem::take(&mut field)),
            '\r' => {}
            c => field.push(c),
        }
    }
    row.push(field);
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_records_with_quotes_and_newlines() {
        let text = "\u{feff}a,b,c\n1,\"x, y\",\"multi\nline\"\n\n2,\"say \"\"hi\"\"\",\n";
        let mut r = DelimReader::new(text.as_bytes(), b',');
        assert_eq!(r.next_record().unwrap().unwrap(), vec!["a", "b", "c"]);
        assert_eq!(r.line_no, 1);
        assert_eq!(
            r.next_record().unwrap().unwrap(),
            vec!["1", "x, y", "multi\nline"]
        );
        assert_eq!(r.line_no, 2, "여러 줄 레코드 = 시작 줄");
        assert_eq!(
            r.next_record().unwrap().unwrap(),
            vec!["2", "say \"hi\"", ""]
        );
        assert_eq!(r.line_no, 5, "빈 줄(4)은 건너뜀");
        assert!(r.next_record().unwrap().is_none());
        let mut t = DelimReader::new("p\tq\r\n1\t2\r\n".as_bytes(), b'\t');
        assert_eq!(t.next_record().unwrap().unwrap(), vec!["p", "q"]);
        assert_eq!(t.next_record().unwrap().unwrap(), vec!["1", "2"]);
    }
}
