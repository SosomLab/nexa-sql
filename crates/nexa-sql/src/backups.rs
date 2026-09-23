//! 파일 탭의 **미저장 본문 스냅숏**([docs/70 §5](../../../docs/70-autosave-and-restore.md) · 사용자 09-23) — 원본은 절대 만지지 않는다.
//!
//! `backups/files/<경로 해시>.txt` = 프리앰블 두 줄(원본 경로 · 스냅숏을 쓸 때의 **디스크 본문 해시**) + 본문 전체(변경분이 아니라 전체 —
//! 복원 단순 · 손상에 강함). 쓰기 = 프로젝트 자동 저장 때 dirty 파일 탭만 · 지우기 = 사용자가 저장/버림 · 복원 = 프로젝트를 열 때
//! 디스크 해시가 프리앰블과 **같을 때만** 본문을 올리고 dirty 표식(다르면 = 밖에서 바뀜 → 디스크 본문 그대로 · 스냅숏은 남긴다 · D-180 ①).
//! 유효기간 `project.backup_days`(7) 넘은 스냅숏은 시작 때 정리.

use std::path::{Path, PathBuf};

/// FNV-1a 64(순수 · 외부 crate 0).
pub(crate) fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn dir() -> Option<PathBuf> {
    Some(nsql_settings::config_dir()?.join("backups").join("files"))
}

fn key(path: &Path) -> String {
    let s = path.to_string_lossy().to_lowercase().replace('\\', "/");
    format!("{:016x}", fnv64(s.as_bytes()))
}

fn file_of(path: &Path) -> Option<PathBuf> {
    Some(dir()?.join(format!("{}.txt", key(path))))
}

/// 디스크 본문의 해시(없으면 0).
pub(crate) fn disk_hash(path: &Path) -> u64 {
    std::fs::read(path).map(|b| fnv64(&b)).unwrap_or(0)
}

/// 스냅숏 쓰기(임시 파일 → rename · 원자적).
pub(crate) fn write(path: &Path, text: &str) -> bool {
    let Some(f) = file_of(path) else {
        return false;
    };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let body = format!(
        "{}\n{:016x}\n{}",
        path.to_string_lossy(),
        disk_hash(path),
        text
    );
    let tmp = f.with_extension("txt.tmp");
    std::fs::write(&tmp, body).is_ok() && std::fs::rename(&tmp, &f).is_ok()
}

/// 스냅숏 지우기(저장 · 버림).
pub(crate) fn remove(path: &Path) {
    if let Some(f) = file_of(path) {
        let _ = std::fs::remove_file(f);
    }
}

/// 스냅숏 읽기(검증용) — 지금 디스크 해시가 스냅숏의 것과 같으면 `Some(본문)` · 다르거나 없으면 None(`Err` = 밖에서 바뀜).
/// ★ 복원에는 쓰지 않는다(사용자 09-23 "백업은 백업용으로만" — 복원 원천 = 프로젝트 파일 `tabs[].text`) → 시험에서만.
#[cfg(test)]
pub(crate) fn read_matching(path: &Path) -> Result<Option<String>, ()> {
    let Some(f) = file_of(path) else {
        return Ok(None);
    };
    let Ok(raw) = std::fs::read_to_string(&f) else {
        return Ok(None);
    };
    let mut lines = raw.splitn(3, '\n');
    let _p = lines.next();
    let h = lines
        .next()
        .and_then(|s| u64::from_str_radix(s.trim(), 16).ok())
        .unwrap_or(0);
    let body = lines.next().unwrap_or("").to_string();
    if h == disk_hash(path) {
        Ok(Some(body))
    } else {
        Err(())
    }
}

/// `days`일 넘은 스냅숏 정리(시작 때 1회) — 지운 개수.
pub(crate) fn prune(days: u64) -> usize {
    let Some(d) = dir() else {
        return 0;
    };
    let Ok(rd) = std::fs::read_dir(&d) else {
        return 0;
    };
    let limit = std::time::Duration::from_secs(days.max(1) * 86_400);
    let mut n = 0;
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip_and_external_change() {
        let dir = std::env::temp_dir().join(format!("nsql-backup-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // 격리: NSQL_HOME을 시험 폴더로(단위 테스트는 실제 설정 폴더에 쓰지 않는다).
        std::env::set_var("NSQL_HOME", &dir);
        let src = dir.join("a.sql");
        std::fs::write(&src, "select 1;").unwrap();
        assert!(write(&src, "select 1;\n-- unsaved edit"));
        assert_eq!(
            read_matching(&src).unwrap().as_deref(),
            Some("select 1;\n-- unsaved edit")
        );
        // 밖에서 원본이 바뀌면 = Err(올리지 않는다 · 스냅숏은 남는다).
        std::fs::write(&src, "select 2;").unwrap();
        assert!(read_matching(&src).is_err());
        remove(&src);
        assert_eq!(read_matching(&src), Ok(None));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
