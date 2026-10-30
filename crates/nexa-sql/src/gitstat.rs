//! 상태줄 git 세그먼트(Sublime `main ⑥` · 사용자 09-16) — 활성 탭 파일의 폴더가 git 작업 트리면 **브랜치 · 변경 파일 수**.
//!
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
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(dir).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW — 콘솔 창이 깜빡이지 않게.
    }
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
        .and_then(|n| git(dir, &["show", &format!("HEAD:./{n}")]));
    Some(GitInfo {
        branch,
        changed,
        head,
    })
}
