//! App — git 거터(C-1 · docs/19 §2 M3 일부 · 11-06): 둘째 기준선(HEAD 본문 · `gitstat` 감시 스레드)의 변경 덩어리로 캐럿 이동
//! (`git.next_hunk`/`git.prev_hunk` · 순환). 띠 그리기는 nexa-ctl `TextBox::set_git_gutter`(설정 `editor.git_gutter`).
//! hunk diff 팝업 · 상태줄 ahead/behind는 다음 차수(19 §2 나머지).

use crate::*;

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
}

#[cfg(test)]
mod tests {
    use super::hunk_target as f;

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
