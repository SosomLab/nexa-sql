//! 시험용 임시 경로 가드(T-465 · 11-03 · D-445 임시 산출물 정리): 시험이 `std::env::temp_dir()`에 만든 파일·폴더를
//! **끝날 때(패닉 포함) 지운다**. Windows에서는 다른 스레드의 접속(SQLite 파일)이 아직 잡고 있으면 삭제가 실패하므로 짧게
//! 재시도한다(50 ms × 40 = 최대 2초). SQLite 동반 파일(`-journal` · `-wal` · `-shm`)도 함께 지운다.
//!
//! 쓰는 법: `let tp = TempPath::with_ext("t146", "db"); let path = tp.to_path_buf(); … drop(접속); // tp가 범위를 벗어나며 삭제`
//! 적재한 cdylib처럼 프로세스가 끝날 때까지 못 내리는 것은 여기로 못 지운다 — 그런 시험은 `CARGO_TARGET_TMPDIR`(`target/tmp`)에
//! 만들어 다음 실행이 지우게 한다(`crates/nsql-drivers/tests/ext.rs`).

use std::path::{Path, PathBuf};

/// Drop 때 지워지는 임시 경로.
#[derive(Debug)]
pub struct TempPath(PathBuf);

impl TempPath {
    /// `<temp_dir>/nsql-<name>-<pid>` — 앞선 실행이 남긴 같은 이름은 먼저 지운다.
    pub fn new(name: &str) -> Self {
        Self::at(std::env::temp_dir().join(format!("nsql-{name}-{}", std::process::id())))
    }

    /// `<temp_dir>/nsql-<name>-<pid>.<ext>`(파일용).
    pub fn with_ext(name: &str, ext: &str) -> Self {
        Self::at(std::env::temp_dir().join(format!("nsql-{name}-{}.{ext}", std::process::id())))
    }

    /// 주어진 경로를 가드로 감싼다(있던 것은 먼저 지운다).
    pub fn at(path: PathBuf) -> Self {
        remove_retry(&path);
        TempPath(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.0.clone()
    }

    pub fn join(&self, tail: impl AsRef<Path>) -> PathBuf {
        self.0.join(tail)
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        remove_retry(&self.0);
    }
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// 경로(파일이든 폴더든)와 SQLite 동반 파일을 지운다 · 잠금이면 재시도 · 다 지웠으면 참.
pub fn remove_retry(p: &Path) -> bool {
    for i in 0..40 {
        let mut ok = true;
        for q in [
            p.to_path_buf(),
            with_suffix(p, "-journal"),
            with_suffix(p, "-wal"),
            with_suffix(p, "-shm"),
        ] {
            let r = if q.is_dir() {
                std::fs::remove_dir_all(&q)
            } else {
                std::fs::remove_file(&q)
            };
            match r {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => ok = false,
            }
        }
        if ok {
            return true;
        }
        if i + 1 < 40 {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_removes_file_dir_and_sqlite_siblings() {
        let tp = TempPath::with_ext("testpath", "db");
        let p = tp.to_path_buf();
        std::fs::write(&p, b"x").unwrap();
        std::fs::write(with_suffix(&p, "-journal"), b"j").unwrap();
        let d = TempPath::new("testpath-dir");
        std::fs::create_dir_all(d.join("a").join("b")).unwrap();
        std::fs::write(d.join("a").join("b").join("f"), b"f").unwrap();
        let (pd, dd) = (p, d.to_path_buf());
        drop(tp);
        drop(d);
        assert!(!pd.exists() && !with_suffix(&pd, "-journal").exists());
        assert!(!dd.exists());
        assert!(remove_retry(&pd), "없는 경로 = 참");
    }
}
