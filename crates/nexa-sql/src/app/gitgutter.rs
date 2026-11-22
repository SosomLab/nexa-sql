//! App — git 거터(C-1 · docs/19 §2 M3 일부 · 11-06): 둘째 기준선(HEAD 본문 · `gitstat` 감시 스레드)의 변경 덩어리로 캐럿 이동
//! (`git.next_hunk`/`git.prev_hunk` · 순환). 띠 그리기는 nexa-ctl `TextBox::set_git_gutter`(설정 `editor.git_gutter`).
//! hunk diff 팝업 = `githunk.rs` · ★ 11-22 `git.revert_hunk` = 캐럿 줄의 덩어리를 HEAD 내용으로 되돌린다(본문 치환 한 번 = 되돌리기
//! 한 단계 · 디스크 파일·git 저장소는 건드리지 않는다) · 상태줄 ahead/behind는 다음 차수(19 §2 나머지).

use crate::*;
use nexa_ctl::diff;

/// 되돌리기 계산의 상한(전체 정렬 · 팝업과 같은 값) — 이보다 긴 본문은 하지 않는다.
const REVERT_MAX_LINES: usize = 20_000;

/// 덩어리 되돌리기의 본문 치환 한 건: 글자 범위 `from..to`를 `text`로 · `line` = 덩어리 첫 작업본 줄 · `lines` = 되살린 HEAD 줄 수.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RevertEdit {
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub line: usize,
    pub lines: usize,
}

/// 작업본 `work`의 줄 `line`(0 기준)이 든 git 덩어리를 HEAD 본문 `head`의 내용으로 되돌리는 치환(순수) — 덩어리가 없거나 본문이
/// 상한을 넘으면 `None`. 좌표 = 글자 인덱스(61 §1) · 줄끝은 작업본의 것(`\r\n`이 있으면 그것)을 따른다 · 마지막 줄에 줄끝이 없으면
/// 되돌린 뒤에도 없다(덩어리 밖의 글자는 하나도 바꾸지 않는다).
pub(crate) fn revert_edit(head: &str, work: &str, line: usize) -> Option<RevertEdit> {
    let a: Vec<String> = head.lines().map(str::to_string).collect();
    let b: Vec<String> = work.lines().map(str::to_string).collect();
    if a.len().max(b.len()) > REVERT_MAX_LINES {
        return None;
    }
    let rows = diff::align(&a, &b);
    let hunks = diff::hunks(&rows);
    let h = app::githunk::hunk_at(&hunks, line)?;
    let sep = if work.contains("\r\n") { "\r\n" } else { "\n" };
    // 줄 시작 글자 위치(줄 i의 시작 = starts[i]) + 본문 길이.
    let mut starts = vec![0usize];
    let mut len = 0usize;
    for (i, c) in work.chars().enumerate() {
        len = i + 1;
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    let n = b.len();
    let ends_nl = work.ends_with('\n');
    starts.truncate(n + 1);
    let (bs, be) = (h.b.start.min(n), h.b.end.min(n));
    let new: Vec<&str> = a[h.a.clone()].iter().map(String::as_str).collect();
    let (from, to, text) = if be < n || ends_nl {
        // 덩어리 뒤에 줄(또는 마지막 줄끝)이 있다 → 줄 단위로 통째 교체.
        let end = if be < n { starts[be] } else { len };
        let start = if bs < n { starts[bs] } else { len };
        (
            start,
            end,
            new.iter().map(|l| format!("{l}{sep}")).collect(),
        )
    } else if new.is_empty() {
        // 줄끝 없는 꼬리의 추가분을 지운다 → 앞 줄의 줄끝까지 함께.
        let start = if bs == 0 {
            0
        } else {
            let e = starts[bs] - 1;
            if work.chars().nth(e.saturating_sub(1)) == Some('\r') && e > 0 {
                e - 1
            } else {
                e
            }
        };
        (start, len, String::new())
    } else if bs < n {
        (starts[bs], len, new.join(sep))
    } else {
        // 꼬리에서 지워진 줄을 되살린다(작업본에 줄끝 없음) → 줄끝을 앞에 붙여 잇는다.
        let lead = if n > 0 { sep } else { "" };
        (len, len, format!("{lead}{}", new.join(sep)))
    };
    Some(RevertEdit {
        from,
        to,
        text,
        line: bs,
        lines: new.len(),
    })
}

/// 다음/이전 덩어리의 첫 줄(순수 · 순환): `hunks` = 줄 순 `(첫 줄, 끝 줄+1)` · `cur` = 캐럿 줄.
/// 앞으로 = `cur`보다 뒤에서 시작하는 첫 덩어리(없으면 맨 앞) · 뒤로 = `cur`보다 앞에서 시작하는 마지막 덩어리(없으면 맨 뒤).
pub(crate) fn hunk_target(hunks: &[(usize, usize)], cur: usize, forward: bool) -> Option<usize> {
    if hunks.is_empty() {
        return None;
    }
    if forward {
        hunks
            .iter()
            .map(|h| h.0)
            .find(|&s| s > cur)
            .or_else(|| hunks.first().map(|h| h.0))
    } else {
        hunks
            .iter()
            .rev()
            .map(|h| h.0)
            .find(|&s| s < cur)
            .or_else(|| hunks.last().map(|h| h.0))
    }
}

impl App {
    /// 명령 `git.next_hunk`/`git.prev_hunk`: 활성 편집기 탭의 git 변경 덩어리로(없으면 상태줄 안내).
    pub(crate) fn git_hunk_step(&mut self, forward: bool) {
        let target = if self.editors.is_view_tab_active() {
            None
        } else {
            let tb = self.editors.cur();
            hunk_target(&tb.git_hunks(), tb.caret_line(), forward)
        };
        match target {
            Some(line) => {
                self.editors.cur_mut().goto_line(line + 1);
                self.set_focus(Focus::Editor);
            }
            None => self.sess.status = t(Msg::StGitNoHunk).into(),
        }
        self.redraw();
    }

    /// ★ 명령 `git.revert_hunk`(C-1 · 19 §2 · 11-22): 캐럿 줄의 git 변경 덩어리를 HEAD 내용으로 되돌린다 — 편집기 본문 치환 한 번
    /// (`replace_range` = 되돌리기 한 단계 · Ctrl+Z로 복구) · 디스크 파일·git 저장소는 건드리지 않는다(저장은 사용자가). 읽기 전용 탭 ·
    /// 뷰 탭 · HEAD를 아직 못 받은 탭 · 덩어리 밖의 캐럿 = 아무것도 바꾸지 않고 상태줄 안내.
    pub(crate) fn git_revert_hunk(&mut self) {
        let edit = if self.editors.is_view_tab_active() || self.editors.cur().is_read_only() {
            None
        } else {
            let tb = self.editors.cur();
            let (line, work) = (tb.caret_line(), tb.text());
            let head = self.git.info().and_then(|g| g.head.clone());
            // ★ HEAD 본문은 UTF-8로만 읽는다(`git show` 출력) — 다른 인코딩의 파일은 한글이 U+FFFD로 깨져 있다. 거터 표시는
            //   틀려도 해가 없지만 되돌리기는 그 글자를 **버퍼에 써넣으므로** 거부하고 알린다(검토 11-23 · 데이터 보호).
            if head.as_deref().is_some_and(|h| h.contains('\u{fffd}')) {
                self.sess.status = t(Msg::StGitRevertEncoding).into();
                self.redraw();
                return;
            }
            head.and_then(|head| revert_edit(&head, &work, line))
        };
        match edit {
            Some(e) => {
                let mut inv = Invalidations::default();
                self.editors
                    .cur_mut()
                    .replace_range(e.from, e.to, &e.text, &mut inv);
                self.editors.cur_mut().goto_line(e.line + 1);
                self.sess.status = tf(
                    Msg::StGitHunkReverted,
                    &[&(e.line + 1).to_string(), &e.lines.to_string()],
                );
                self.set_focus(Focus::Editor);
            }
            None => self.sess.status = t(Msg::StGitNoHunkAtCaret).into(),
        }
        self.redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::hunk_target as f;
    use super::revert_edit;

    /// 치환을 글자 좌표로 본문에 입힌다(편집기의 `replace_range`와 같은 뜻).
    fn apply(work: &str, e: &super::RevertEdit) -> String {
        let cs: Vec<char> = work.chars().collect();
        let mut out: String = cs[..e.from].iter().collect();
        out.push_str(&e.text);
        out.extend(cs[e.to..].iter());
        out
    }

    /// 덩어리 하나만 HEAD로 돌아가고 덩어리 밖은 글자 하나 안 바뀐다 — 변경 · 추가 · 삭제(쐐기 = 아래 줄) · 꼬리(줄끝 유무) · CRLF.
    #[test]
    fn revert_edit_restores_one_hunk_only() {
        let head = "a\nb\nc\nd\ne\n";
        // 변경(1줄) + 뒤쪽 추가(1줄): 줄 1의 덩어리만 되돌리면 X는 남는다.
        let work = "a\nB\nc\nd\nX\ne\n";
        let e = revert_edit(head, work, 1).expect("hunk");
        assert_eq!((e.line, e.lines), (1, 1));
        assert_eq!(apply(work, &e), "a\nb\nc\nd\nX\ne\n");
        // 추가 줄 되돌리기 = 그 줄 삭제.
        let e = revert_edit(head, work, 4).expect("hunk");
        assert_eq!(apply(work, &e), "a\nB\nc\nd\ne\n");
        assert_eq!(revert_edit(head, work, 2), None, "덩어리 밖의 줄");
        // 삭제만인 덩어리(c 삭제) = 그 아래 줄(d · 줄 2)에서 부른다 → c가 돌아온다.
        let del = "a\nb\nd\ne\n";
        let e = revert_edit(head, del, 2).expect("hunk");
        assert_eq!((e.from, e.to, e.lines), (4, 4, 1));
        assert_eq!(apply(del, &e), head);
        // 꼬리 · 줄끝 없는 작업본: 되돌린 뒤에도 줄끝이 없다.
        for (w, line, want) in [
            ("a\nb\nc\nd\nE", 4, "a\nb\nc\nd\ne"),
            ("a\nb\nc\nd\ne\nZ", 5, "a\nb\nc\nd\ne"),
            ("a\nb\nc", 3, "a\nb\nc\nd\ne"),
            ("Z", 0, "a\nb\nc\nd\ne"),
        ] {
            let e = revert_edit(head, w, line).unwrap_or_else(|| panic!("hunk {w:?}"));
            assert_eq!(apply(w, &e), want, "{w:?}");
        }
        // 빈 작업본 · 줄끝 있는 꼬리 삭제.
        let e = revert_edit(head, "", 0).expect("hunk");
        assert_eq!(apply("", &e), "a\nb\nc\nd\ne");
        let e = revert_edit(head, "a\nb\nc\n", 3).expect("hunk");
        assert_eq!(apply("a\nb\nc\n", &e), head);
        // CRLF 작업본 = 되살린 줄도 CRLF · 줄끝 없는 꼬리의 추가분 삭제는 앞 줄의 CRLF까지.
        let crlf = "a\r\nB\r\nc\r\nd\r\ne\r\n";
        let e = revert_edit(head, crlf, 1).expect("hunk");
        assert_eq!(apply(crlf, &e), "a\r\nb\r\nc\r\nd\r\ne\r\n");
        let tail = "a\r\nb\r\nc\r\nd\r\ne\r\nZ";
        let e = revert_edit(head, tail, 5).expect("hunk");
        assert_eq!(apply(tail, &e), "a\r\nb\r\nc\r\nd\r\ne");
        // 한글(여러 바이트) = 좌표는 글자 인덱스.
        let e = revert_edit("가나\n다라\n", "가나\n다라마\n", 1).expect("hunk");
        assert_eq!((e.from, e.to), (3, 7));
    }

    /// 난수 대조: 임의의 HEAD·작업본에서 덩어리를 하나씩 되돌리면(줄 수 + 1번 안에) 줄 목록이 HEAD와 같아진다 · 매 단계 덩어리 수는 줄어든다.
    #[test]
    fn revert_edit_random_converges_to_head() {
        use nexa_ctl::diff;
        let mut seed = 0x2026_1122_u64;
        let mut rnd = |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        let lines_of = |s: &str| s.lines().map(str::to_string).collect::<Vec<_>>();
        for round in 0..300 {
            let make = |rnd: &mut dyn FnMut(u64) -> u64| {
                let n = rnd(9);
                let mut s = String::new();
                for i in 0..n {
                    s.push((b'a' + rnd(4) as u8) as char);
                    if i + 1 < n || rnd(2) == 0 {
                        s.push('\n');
                    }
                }
                s
            };
            let head = make(&mut rnd);
            let mut work = make(&mut rnd);
            let count = |w: &str| diff::hunks(&diff::align(&lines_of(&head), &lines_of(w))).len();
            let mut left = count(&work);
            let mut guard = 0;
            while left > 0 {
                let hs = diff::hunks(&diff::align(&lines_of(&head), &lines_of(&work)));
                // 삭제만인 덩어리는 그 아래 줄에서 부른다(`b.start == b.end`).
                let line = hs[0].b.start;
                let e = revert_edit(&head, &work, line)
                    .unwrap_or_else(|| panic!("round {round}: {head:?} {work:?} line {line}"));
                work = apply(&work, &e);
                let now = count(&work);
                assert!(now < left, "round {round}: {head:?} → {work:?}");
                left = now;
                guard += 1;
                assert!(guard < 32, "round {round}");
            }
            assert_eq!(lines_of(&work), lines_of(&head), "round {round}");
        }
    }

    #[test]
    fn hunk_target_cycles() {
        let h = [(2usize, 4usize), (7, 8), (10, 13)];
        assert_eq!(f(&[], 0, true), None);
        assert_eq!(f(&h, 0, true), Some(2));
        assert_eq!(
            f(&h, 2, true),
            Some(7),
            "덩어리 첫 줄에서 앞으로 = 다음 덩어리"
        );
        assert_eq!(f(&h, 3, true), Some(7));
        assert_eq!(f(&h, 11, true), Some(2), "마지막 뒤 = 맨 앞으로 순환");
        assert_eq!(f(&h, 11, false), Some(10));
        assert_eq!(f(&h, 10, false), Some(7));
        assert_eq!(f(&h, 1, false), Some(10), "맨 앞 앞 = 맨 뒤로 순환");
        assert_eq!(f(&h, 7, false), Some(2));
    }
}
