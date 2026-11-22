//! 상태줄 git 세그먼트(Sublime `main ⑥` · 사용자 09-16) — 활성 탭 파일의 폴더가 git 작업 트리면 **브랜치 · 변경 파일 수**.
//!
//! - ★ 11-22(C-1 · 19 §2): 추적 브랜치 대비 **앞/뒤 커밋 수**(`rev-list --left-right --count HEAD...@{u}`) — 로컬에 이미 있는 추적
//!   ref만 본다(**fetch 없음 · 네트워크 0** · 사용자가 fetch하기 전까지 "뒤"는 낡을 수 있다) · 추적 브랜치가 없으면 표시 없음.
//! - 구현 = `git` CLI 호출(`rev-parse --abbrev-ref HEAD` · `status --porcelain`) · **배경 스레드** · 결과는 채널로 · UI는 폴링.
//!   libgit2 같은 의존 없이(DR-3) · git이 없으면 세그먼트 없음.
//! - 부하([39 §3](../../../docs/39-resource-governance.md)): 폴더가 바뀌거나 저장한 직후, 그리고 `statusbar.git_secs`(15초)마다 1회 ·
//!   동시 1개 · 끄는 키 `statusbar.git`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitInfo {
    pub branch: String,
    /// `git status --porcelain` 줄 수(수정·추가·삭제·미추적).
    pub changed: usize,
    /// ★ 11-22: 추적 브랜치 대비 (앞선 커밋 수, 뒤진 커밋 수) — 추적 브랜치 없음·detached = None(표시하지 않는다).
    pub ab: Option<(usize, usize)>,
    /// ★ T-110 2-3(10-30): 감시 파일의 **HEAD 본문**(`git show HEAD:./<파일>` · 추적되지 않은 파일 = None) — 미니맵 git 변경 줄의 둘째 기준선.
    pub head: Option<String>,
}

pub(crate) struct GitWatch {
    dir: Option<PathBuf>,
    /// ★ T-110 2-3: 감시 파일(활성 탭 · `dir` 안의 파일 이름만 쓴다) — 바뀌면 다시 잰다.
    file: Option<PathBuf>,
    info: Option<GitInfo>,
    rx: Option<mpsc::Receiver<Option<GitInfo>>>,
    last: Option<Instant>,
    pub interval: Duration,
}

impl GitWatch {
    pub(crate) fn new() -> Self {
        GitWatch {
            dir: None,
            file: None,
            info: None,
            rx: None,
            last: None,
            interval: Duration::from_secs(15),
        }
    }

    /// 감시 폴더(활성 탭 파일의 부모) — 바뀌면 결과를 비우고 즉시 다시 잰다.
    pub(crate) fn set_dir(&mut self, dir: Option<PathBuf>) {
        if self.dir != dir {
            self.dir = dir;
            self.info = None;
            self.last = None;
            self.rx = None;
        }
    }

    /// ★ T-110 2-3: 감시 파일(활성 탭의 파일 · `dir` 안) — 바뀌면 HEAD 본문을 다시 받는다(브랜치·변경 수는 그대로 두고 즉시 재조회).
    pub(crate) fn set_file(&mut self, file: Option<PathBuf>) {
        if self.file != file {
            self.file = file;
            self.last = None;
            self.rx = None;
            // ★ 앞 파일의 HEAD 본문은 버린다(검토 11-23): 새 조회가 돌아오기 전에 `git.revert_hunk`·덩어리 팝업이 **다른 파일의
            //   HEAD**로 지금 탭을 고칠 수 있었다. 브랜치·변경 수는 폴더 것이라 그대로 둔다.
            if let Some(i) = self.info.as_mut() {
                i.head = None;
            }
        }
    }

    /// 주기(설정 `statusbar.git_secs`).
    pub(crate) fn set_interval(&mut self, secs: u64) {
        self.interval = Duration::from_secs(secs.max(2));
    }

    /// 필요하면 배경 조회를 시작한다(`force` = 저장 직후 · 주기 무시).
    pub(crate) fn refresh(&mut self, force: bool) {
        let Some(dir) = self.dir.clone() else {
            return;
        };
        if self.rx.is_some() {
            return;
        }
        let stale = self.last.is_none_or(|t| t.elapsed() >= self.interval);
        if !force && !stale {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let file = self.file.clone();
        self.last = Some(Instant::now());
        let _ = std::thread::Builder::new()
            .name("nsql-git".into())
            .spawn(move || {
                let _ = tx.send(probe(&dir, file.as_deref()));
            });
    }

    /// 결과 회수 — 바뀌었으면 true(다시 그린다).
    pub(crate) fn poll(&mut self) -> bool {
        let Some(rx) = self.rx.as_ref() else {
            return false;
        };
        match rx.try_recv() {
            Ok(info) => {
                self.rx = None;
                let changed = self.info != info;
                self.info = info;
                changed
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.rx = None;
                false
            }
        }
    }

    pub(crate) fn info(&self) -> Option<&GitInfo> {
        self.info.as_ref()
    }

    pub(crate) fn pending(&self) -> bool {
        self.rx.is_some()
    }
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    // 콘솔 창 없이(부품 `quiet_command` = CREATE_NO_WINDOW · 11-10 규칙 61 §2-2 · 종전 자체 creation_flags를 통일).
    let mut cmd = nsql_ext::net::quiet_command("git");
    cmd.arg("-C").arg(dir).args(args);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 폴더가 git 작업 트리면 (브랜치 · 변경 수 · ★ 감시 파일의 HEAD 본문(T-110 2-3 · `git show HEAD:./<이름>` · 미추적/없음 = None)).
pub(crate) fn probe(dir: &Path, file: Option<&Path>) -> Option<GitInfo> {
    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let branch = branch.trim().to_string();
    if branch.is_empty() {
        return None;
    }
    let status = git(dir, &["status", "--porcelain"]).unwrap_or_default();
    let changed = status.lines().filter(|l| !l.trim().is_empty()).count();
    // 파일 이름만(`-C dir` 기준 상대 경로 `./이름`) — 경로에 `:`·공백이 있어도 인자 하나라 안전.
    let head = file
        .and_then(|f| f.file_name())
        .and_then(|n| n.to_str())
        .and_then(|n| git(dir, &["show", &format!("HEAD:./{n}")]))
        // UTF-8 BOM은 뗀다(편집기도 열 때 뗀다 — 남기면 첫 줄이 늘 "다름"이고 되돌리기가 U+FEFF를 본문에 넣는다 · 검토 11-23).
        .map(|h| match h.strip_prefix('\u{feff}') {
            Some(rest) => rest.to_string(),
            None => h,
        });
    // 추적 브랜치 대비 앞/뒤(로컬 ref만 · 추적 브랜치가 없으면 git이 실패 코드 → None).
    let ab = git(dir, &["rev-list", "--left-right", "--count", "HEAD...@{u}"])
        .and_then(|out| parse_ab(&out));
    Some(GitInfo {
        branch,
        changed,
        ab,
        head,
    })
}

/// `rev-list --left-right --count A...B` 출력(`<왼쪽> <오른쪽>` · 탭 구분) → (앞, 뒤). 꼴이 다르면 None.
pub(crate) fn parse_ab(out: &str) -> Option<(usize, usize)> {
    let mut it = out.split_whitespace();
    let ahead = it.next()?.parse().ok()?;
    let behind = it.next()?.parse().ok()?;
    it.next().is_none().then_some((ahead, behind))
}

/// 상태줄 git 세그먼트 글: `브랜치 (변경 수)` · 앞/뒤가 있으면 `브랜치 ↑앞 ↓뒤 (변경 수)`(0인 쪽은 생략 — 둘 다 0이면 종전 글 그대로).
pub(crate) fn segment_text(info: &GitInfo) -> String {
    let mut s = info.branch.clone();
    if let Some((ahead, behind)) = info.ab {
        if ahead > 0 {
            s.push_str(&format!(" ↑{ahead}"));
        }
        if behind > 0 {
            s.push_str(&format!(" ↓{behind}"));
        }
    }
    format!("{s} ({})", info.changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 감시 파일이 바뀌면 앞 파일의 HEAD 본문을 버린다(검토 11-23) — 새 조회 전에 되돌리기·팝업이 다른 파일의 HEAD를 쓰지 않게.
    /// 브랜치·변경 수(폴더 것)는 그대로 · 같은 파일로 다시 지정하면 그대로.
    #[test]
    fn set_file_drops_previous_head() {
        let mut w = GitWatch::new();
        w.set_file(Some(PathBuf::from("a.sql")));
        w.info = Some(GitInfo {
            branch: "main".into(),
            changed: 2,
            ab: None,
            head: Some("x\n".into()),
        });
        w.set_file(Some(PathBuf::from("a.sql")));
        assert_eq!(w.info().and_then(|g| g.head.as_deref()), Some("x\n"));
        w.set_file(Some(PathBuf::from("b.sql")));
        let g = w.info().unwrap();
        assert!(g.head.is_none() && g.branch == "main" && g.changed == 2);
    }

    #[test]
    fn parse_ab_reads_two_counts_only() {
        assert_eq!(
            parse_ab(
                "2	1
"
            ),
            Some((2, 1))
        );
        assert_eq!(parse_ab("0	0"), Some((0, 0)));
        assert_eq!(parse_ab("  12   3  "), Some((12, 3)));
        for bad in ["", "2", "a	b", "1	2	3", "-1	0"] {
            assert_eq!(parse_ab(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn segment_text_omits_zero_sides() {
        let info = |ab| GitInfo {
            branch: "main".into(),
            changed: 3,
            ab,
            head: None,
        };
        assert_eq!(segment_text(&info(None)), "main (3)");
        assert_eq!(segment_text(&info(Some((0, 0)))), "main (3)");
        assert_eq!(segment_text(&info(Some((2, 0)))), "main ↑2 (3)");
        assert_eq!(segment_text(&info(Some((0, 5)))), "main ↓5 (3)");
        assert_eq!(segment_text(&info(Some((2, 1)))), "main ↑2 ↓1 (3)");
    }
}
