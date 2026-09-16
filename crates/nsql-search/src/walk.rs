//! 병렬 폴더 걷기 — std 스레드 풀 + 폴더 단위 작업 큐(`Mutex<VecDeque>` + `Condvar`) · 심볼릭 링크 루프 차단 · 무시 규칙 · 크기 상한.
//!
//! 한 워커가 폴더 하나를 읽어 파일은 그 자리에서 `visit`(읽기·매칭·전송)하고 하위 폴더는 큐에 넣는다.
//! 큐가 비고 일하는 워커가 0이면 전원 종료. 취소 플래그는 폴더 단위·파일 단위로 본다.
//!
//! 자원(docs/39): 스레드 = `threads`(0 = 코어 수 · 상한 [`MAX_THREADS`]) · 열린 파일 = 워커당 1 · 네트워크 0.

use crate::ignore::{IgnoreNode, DEFAULT_EXCLUDES};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// 자동 스레드 수 상한(코어가 더 많아도 디스크가 병목).
pub const MAX_THREADS: usize = 16;

#[derive(Debug, Clone)]
pub(crate) struct WalkOpts {
    pub threads: usize,
    pub gitignore: bool,
    /// 사용자 제외 패턴(gitignore 문법 · 루트 기준).
    pub excludes: Vec<String>,
}

/// 걷기 중 호출되는 훅(워커 스레드에서 실행).
pub(crate) struct Hooks<'a> {
    /// 검색할 파일 하나(크기 상한·이진 판정은 여는 쪽이 `fstat`로 — 걷기는 파일마다 `stat`를 부르지 않는다).
    pub visit: &'a (dyn Fn(PathBuf) + Sync),
    /// 폴더 읽기 실패 등(경로 · 메시지).
    pub error: &'a (dyn Fn(PathBuf, String) + Sync),
}

/// 폴더 작업 하나.
struct Job {
    path: PathBuf,
    /// 루트 기준 상대 경로(`/` 구분 · 루트 자신은 빈 문자열).
    rel: String,
    depth: usize,
    ignore: Arc<IgnoreNode>,
}

/// 방문한 폴더의 신원 — unix는 (dev, inode).
#[cfg(unix)]
#[derive(Hash, PartialEq, Eq)]
struct DirKey(u64, u64);

/// 방문한 폴더의 신원 — Windows 등은 정규화 경로로 대신한다(파일 ID API 없이).
#[cfg(not(unix))]
#[derive(Hash, PartialEq, Eq)]
struct DirKey(PathBuf);

#[cfg(unix)]
fn dir_key(path: &Path) -> Option<DirKey> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).ok()?;
    Some(DirKey(m.dev(), m.ino()))
}

#[cfg(not(unix))]
fn dir_key(path: &Path) -> Option<DirKey> {
    std::fs::canonicalize(path).ok().map(DirKey)
}

struct Queue {
    state: Mutex<(VecDeque<Job>, usize)>,
    cv: Condvar,
}

impl Queue {
    fn push(&self, job: Job) {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.0.push_back(job);
        self.cv.notify_one();
    }

    /// 다음 작업 — 큐가 비고 일하는 워커가 없으면 `None`(전원 종료).
    fn next(&self, cancel: &AtomicBool) -> Option<Job> {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if cancel.load(Ordering::Relaxed) {
                g.0.clear();
                self.cv.notify_all();
                return None;
            }
            if let Some(j) = g.0.pop_front() {
                g.1 += 1;
                return Some(j);
            }
            if g.1 == 0 {
                self.cv.notify_all();
                return None;
            }
            g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn finish(&self) {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.1 -= 1;
        if g.0.is_empty() && g.1 == 0 {
            self.cv.notify_all();
        }
    }
}

/// 실제 스레드 수.
pub(crate) fn thread_count(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
        .clamp(1, MAX_THREADS)
}

/// 루트들을 병렬로 걷는다(호출 스레드에서 워커를 만들고 전부 끝날 때까지 기다린다).
pub(crate) fn walk(roots: &[PathBuf], opts: &WalkOpts, cancel: &AtomicBool, hooks: &Hooks<'_>) {
    let queue = Queue {
        state: Mutex::new((VecDeque::new(), 0)),
        cv: Condvar::new(),
    };
    let visited: Mutex<HashSet<DirKey>> = Mutex::new(HashSet::new());
    let base_rules: Vec<&str> = DEFAULT_EXCLUDES
        .iter()
        .copied()
        .chain(opts.excludes.iter().map(String::as_str))
        .collect();

    for root in roots {
        let meta = match std::fs::metadata(root) {
            Ok(m) => m,
            Err(e) => {
                (hooks.error)(root.clone(), e.to_string());
                continue;
            }
        };
        if meta.is_file() {
            (hooks.visit)(root.clone());
            continue;
        }
        let mut ignore = Arc::new(IgnoreNode::from_lines(base_rules.iter().copied(), 0, None));
        if opts.gitignore {
            if let Some(child) = IgnoreNode::load_child(root, 0, &ignore) {
                ignore = child;
            }
        }
        queue.push(Job {
            path: root.clone(),
            rel: String::new(),
            depth: 0,
            ignore,
        });
    }

    let n = thread_count(opts.threads);
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                while let Some(job) = queue.next(cancel) {
                    process_dir(job, opts, cancel, hooks, &queue, &visited);
                    queue.finish();
                }
            });
        }
    });
}

fn process_dir(
    job: Job,
    opts: &WalkOpts,
    cancel: &AtomicBool,
    hooks: &Hooks<'_>,
    queue: &Queue,
    visited: &Mutex<HashSet<DirKey>>,
) {
    if let Some(key) = dir_key(&job.path) {
        let mut v = visited.lock().unwrap_or_else(|e| e.into_inner());
        if !v.insert(key) {
            return; // 링크 루프 · 겹치는 루트
        }
    }
    let rd = match std::fs::read_dir(&job.path) {
        Ok(rd) => rd,
        Err(e) => {
            (hooks.error)(job.path, e.to_string());
            return;
        }
    };
    let parent_comps: Vec<&str> = if job.rel.is_empty() {
        Vec::new()
    } else {
        job.rel.split('/').collect()
    };
    for entry in rd {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let Ok(entry) = entry else { continue };
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            continue;
        }; // UTF-8이 아닌 이름은 건너뛴다(규칙 매칭 불가)
        let path = entry.path();
        // 종류는 readdir가 준 것(d_type)으로 — 파일마다 stat를 부르지 않는다(10만 파일에서 syscall 절반).
        // 링크만 대상 종류로 판단(파일 링크는 따라가고, 폴더 링크는 방문 집합이 루프를 막는다).
        let (is_dir, is_file) = match entry.file_type() {
            Ok(ft) if ft.is_symlink() => match std::fs::metadata(&path) {
                Ok(m) => (m.is_dir(), m.is_file()),
                Err(_) => continue,
            },
            Ok(ft) => (ft.is_dir(), ft.is_file()),
            Err(_) => continue,
        };
        let mut comps = parent_comps.clone();
        comps.push(name);
        if job.ignore.is_ignored(&comps, is_dir) {
            continue;
        }
        if is_dir {
            let depth = job.depth + 1;
            let mut ignore = Arc::clone(&job.ignore);
            if opts.gitignore {
                if let Some(child) = IgnoreNode::load_child(&path, depth, &ignore) {
                    ignore = child;
                }
            }
            let rel = if job.rel.is_empty() {
                name.to_string()
            } else {
                format!("{}/{name}", job.rel)
            };
            queue.push(Job {
                path,
                rel,
                depth,
                ignore,
            });
        } else if is_file {
            (hooks.visit)(path);
        }
    }
}
