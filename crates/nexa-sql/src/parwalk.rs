//! 병렬 폴더 열거 부품(사용자 09-23 "폴더/파일 개수가 많아도 검색이 멈추지 않게 · 별도 스레드로 메인에 영향 없이 · 분할 병렬 탐색 ·
//! 병렬이어도 로그는 병합") — 프로젝트 탐색기 필터가 쓴다(30 §2 · 둘째 사용처가 생기면 nsql-search `walk`와 합칠 후보).
//!
//! - **구조**: 코디네이터 스레드 1 + 워커 `threads`(폴더 단위 작업 큐 `Mutex<VecDeque> + Condvar` · 큐가 비고 일하는 워커가 0이면 종료).
//!   워커는 폴더 하나를 `nexa_fs::list_opts`로 읽어 **폴더째 한 메시지**로 보내고 하위 폴더를 큐에 넣는다(부모 메시지가 자식보다 먼저).
//! - **채널 하나**: 워커가 여럿이어도 `mpsc` 하나로 모이므로 수신 쪽(UI 틱)이 보는 순서 = 도착 순 = 병합된 로그. 실패 폴더도 같은 채널에
//!   `Err(사유)`로 온다.
//! - **취소**: `AtomicBool` — 다음 폴더에서 멈춘다 · 수신자를 버려도(필터 변경) 보내기가 실패해 스스로 취소한다.
//! - **자원**(39 §3): 스레드 = `threads`(설정 `project.scan_threads` · 상한 16) · 열린 핸들 = 워커당 1 · 네트워크 0 · 열거가 끝나면 전부 종료.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

/// 스레드 수 상한(디스크가 병목).
pub(crate) const MAX_THREADS: usize = 16;

/// 채널로 오는 조각.
pub(crate) enum DirMsg {
    /// 폴더 하나의 열거 결과(실패 = 사유).
    Dir {
        path: PathBuf,
        entries: Result<Vec<nexa_fs::Entry>, String>,
    },
    /// 끝(취소 포함) — 읽은 폴더 수.
    Done { dirs: usize },
}

#[derive(Clone, Copy)]
pub(crate) struct ListOpts {
    pub show_hidden: bool,
    pub show_dot: bool,
}

struct Queue {
    state: Mutex<(VecDeque<PathBuf>, usize)>,
    cv: Condvar,
}

impl Queue {
    fn push(&self, p: PathBuf) {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        g.0.push_back(p);
        self.cv.notify_one();
    }
    /// 다음 작업 — 큐가 비고 일하는 워커가 없으면 `None`(전원 종료).
    fn next(&self, cancel: &AtomicBool) -> Option<PathBuf> {
        let mut g = self.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if cancel.load(Ordering::Relaxed) {
                g.0.clear();
                self.cv.notify_all();
                return None;
            }
            if let Some(p) = g.0.pop_front() {
                g.1 += 1;
                return Some(p);
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

/// 실제 스레드 수(0 = 코어 수/2 · 최소 1 · 상한 [`MAX_THREADS`]).
pub(crate) fn thread_count(requested: usize) -> usize {
    if requested > 0 {
        return requested.min(MAX_THREADS);
    }
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(2)
        .clamp(1, MAX_THREADS)
}

/// `roots`(폴더들) 아래를 병렬로 열거한다 — 바로 돌아오고 결과는 채널로. 취소 = 플래그 또는 수신자 버림.
pub(crate) fn spawn(
    roots: Vec<PathBuf>,
    opts: ListOpts,
    threads: usize,
) -> (Receiver<DirMsg>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel::<DirMsg>();
    let cancel = Arc::new(AtomicBool::new(false));
    let c = Arc::clone(&cancel);
    let n = thread_count(threads);
    let spawned = std::thread::Builder::new()
        .name("nsql-parwalk".into())
        .spawn(move || run(roots, opts, n, tx, c));
    if spawned.is_err() {
        cancel.store(true, Ordering::Relaxed);
    }
    (rx, cancel)
}

fn run(roots: Vec<PathBuf>, opts: ListOpts, n: usize, tx: Sender<DirMsg>, cancel: Arc<AtomicBool>) {
    let queue = Queue {
        state: Mutex::new((VecDeque::new(), 0)),
        cv: Condvar::new(),
    };
    for r in roots {
        queue.push(r);
    }
    let dirs = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                while let Some(path) = queue.next(&cancel) {
                    let entries = nexa_fs::list_opts(&path, opts.show_hidden, opts.show_dot)
                        .map_err(|e| e.to_string());
                    let child_dirs: Vec<PathBuf> = entries
                        .as_ref()
                        .map(|v| {
                            v.iter()
                                .filter(|e| e.is_dir)
                                .map(|e| e.path.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    dirs.fetch_add(1, Ordering::Relaxed);
                    // ★ 부모 메시지를 **먼저** 보내고 나서 자식 폴더를 큐에 넣는다 — 다른 워커가 자식을 먼저 읽어 보내는 경주를 막는다
                    //   (수신 쪽은 부모가 트리에 있어야 자식을 붙일 수 있다).
                    if tx.send(DirMsg::Dir { path, entries }).is_err() {
                        // 수신자가 사라졌다(필터가 바뀌어 새 열거로 교체) → 자기 취소.
                        cancel.store(true, Ordering::Relaxed);
                    } else {
                        for d in child_dirs {
                            queue.push(d);
                        }
                    }
                    queue.finish();
                }
            });
        }
    });
    let _ = tx.send(DirMsg::Done {
        dirs: dirs.load(Ordering::Relaxed),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 깊은 트리를 병렬로 전부 열거 · 부모 메시지가 자식보다 먼저 · 실패 폴더는 사유와 함께 · Done은 마지막 · 취소는 멈춘다.
    #[test]
    fn walks_everything_parent_first_and_cancels() {
        let dir = std::env::temp_dir().join(format!("nsql-parwalk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for a in 0..6 {
            for b in 0..4 {
                let d = dir.join(format!("a{a}/b{b}"));
                std::fs::create_dir_all(&d).expect("mkdir");
                std::fs::write(d.join("f.sql"), "x").expect("write");
            }
        }
        let (rx, _c) = spawn(
            vec![dir.clone(), dir.join("nope")],
            ListOpts {
                show_hidden: false,
                show_dot: true,
            },
            3,
        );
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut errs = 0;
        let mut done = None;
        for m in rx {
            match m {
                DirMsg::Dir { path, entries } => {
                    // 루트 둘(dir · dir/nope)은 서로 순서가 없다(둘 다 처음부터 큐에) — 루트 아래 폴더만 부모 먼저를 검사.
                    if let Some(parent) = path.parent() {
                        if path != dir && path != dir.join("nope") && path.starts_with(&dir) {
                            assert!(
                                seen.contains(&parent.to_path_buf()),
                                "부모가 먼저: {path:?}"
                            );
                        }
                    }
                    if entries.is_err() {
                        errs += 1;
                    }
                    seen.push(path);
                }
                DirMsg::Done { dirs } => done = Some(dirs),
            }
        }
        assert_eq!(errs, 1, "없는 루트 = 실패 1");
        assert_eq!(seen.len(), 1 + 6 + 24 + 1);
        assert_eq!(done, Some(seen.len()));
        // 취소: 플래그를 먼저 켜면 루트도 읽지 않고 Done.
        let (rx, c) = spawn(
            vec![dir.clone()],
            ListOpts {
                show_hidden: false,
                show_dot: true,
            },
            2,
        );
        c.store(true, Ordering::Relaxed);
        let msgs: Vec<DirMsg> = rx.iter().collect();
        assert!(msgs.iter().all(|m| matches!(m, DirMsg::Done { .. })) || msgs.len() <= 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
