//! 줄 번호·열·문맥 계산 — **일치가 있는 파일에서만** 실행된다(docs/36 §2 "줄 번호" 행).
//! 일치 오프셋은 오름차순이므로 마지막 위치부터 개행만 세어 나간다(파일당 한 번 훑기).

use crate::bytes::{memchr, memrchr};
use crate::Match;
use std::path::Path;
use std::sync::Arc;

/// 줄 하나의 경계(개행 제외).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineSpan {
    start: usize,
    end: usize,
}

#[inline]
fn line_at(hay: &[u8], pos: usize) -> LineSpan {
    let start = memrchr(b'\n', &hay[..pos]).map_or(0, |i| i + 1);
    let end = memchr(b'\n', &hay[pos..]).map_or(hay.len(), |i| pos + i);
    LineSpan { start, end }
}

#[inline]
fn text(hay: &[u8], l: LineSpan) -> String {
    let mut b = &hay[l.start..l.end];
    if let Some(stripped) = b.strip_suffix(b"\r") {
        b = stripped;
    }
    String::from_utf8_lossy(b).into_owned()
}

fn prev_line(hay: &[u8], l: LineSpan) -> Option<String> {
    if l.start == 0 {
        return None;
    }
    Some(text(hay, line_at(hay, l.start - 1)))
}

fn next_line(hay: &[u8], l: LineSpan) -> Option<String> {
    // 파일 끝의 개행 하나는 빈 마지막 줄로 치지 않는다.
    if l.end + 1 >= hay.len() {
        return None;
    }
    Some(text(hay, line_at(hay, l.end + 1)))
}

/// 직전 일치의 줄(같은 줄의 다음 일치는 재계산하지 않는다).
struct Current {
    span: LineSpan,
    line: Arc<str>,
    before: Option<Arc<str>>,
    after: Option<Arc<str>>,
}

/// 정렬된 `(start, end)` 바이트 범위를 [`Match`]로. `sink`가 false를 돌려주면 멈춘다(취소·배치 상한).
pub(crate) fn resolve(
    path: &Arc<Path>,
    hay: &[u8],
    spans: &[(usize, usize)],
    sink: &mut dyn FnMut(Match) -> bool,
) {
    let mut line_no = 1usize;
    let mut counted_to = 0usize; // 개행을 센 지점
    let mut cur: Option<Current> = None;
    for &(s, e) in spans {
        if s > counted_to {
            line_no += hay[counted_to..s].iter().filter(|&&b| b == b'\n').count();
            counted_to = s;
        }
        let reuse = cur
            .as_ref()
            .is_some_and(|c| s >= c.span.start && s <= c.span.end);
        if !reuse {
            let l = line_at(hay, s);
            cur = Some(Current {
                span: l,
                line: Arc::from(text(hay, l)),
                before: prev_line(hay, l).map(Arc::from),
                after: next_line(hay, l).map(Arc::from),
            });
        }
        let Some(c) = cur.as_ref() else {
            unreachable!()
        };
        let rel_s = s - c.span.start;
        let rel_e = e.min(c.span.end) - c.span.start;
        let m = Match {
            path: Arc::clone(path),
            line_no,
            col: rel_s + 1,
            span: (rel_s, rel_e),
            line: Arc::clone(&c.line),
            before: c.before.clone(),
            after: c.after.clone(),
        };
        if !sink(m) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(hay: &[u8], spans: &[(usize, usize)]) -> Vec<Match> {
        let p: Arc<Path> = Arc::from(Path::new("x.sql"));
        let mut v = Vec::new();
        resolve(&p, hay, spans, &mut |m| {
            v.push(m);
            true
        });
        v
    }

    #[test]
    fn line_numbers_columns_and_context() {
        let hay = b"first\r\nSELECT a, a\nlast";
        let spans = [(7, 13), (14, 15), (17, 18), (19, 23)];
        let v = collect(hay, &spans);
        assert_eq!(v.len(), 4);
        assert_eq!((v[0].line_no, v[0].col), (2, 1));
        assert_eq!(&*v[0].line, "SELECT a, a");
        assert_eq!(v[0].before.as_deref(), Some("first"), "\\r 제거");
        assert_eq!(v[0].after.as_deref(), Some("last"));
        assert_eq!((v[1].line_no, v[1].col, v[1].span), (2, 8, (7, 8)));
        assert_eq!((v[2].line_no, v[2].col), (2, 11));
        assert_eq!((v[3].line_no, v[3].col), (3, 1));
        assert_eq!(v[3].before.as_deref(), Some("SELECT a, a"));
        assert_eq!(v[3].after, None);
        assert_eq!(v[0].before.as_deref(), Some("first"));
        let first = collect(hay, &[(0, 5)]);
        assert_eq!((first[0].line_no, first[0].before.is_none()), (1, true));
    }

    #[test]
    fn sink_can_stop() {
        let hay = b"a a a a";
        let p: Arc<Path> = Arc::from(Path::new("x"));
        let mut n = 0;
        resolve(&p, hay, &[(0, 1), (2, 3), (4, 5)], &mut |_| {
            n += 1;
            n < 2
        });
        assert_eq!(n, 2);
    }
}
