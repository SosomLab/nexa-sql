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

/// 스냅숏 폴더(상태줄 자동 저장 메뉴 "일반 파일 자동 저장 위치" · 09-28).
pub(crate) fn snapshot_dir() -> Option<PathBuf> {
    dir()
}

/// 그 파일 탭의 스냅숏 경로(있을 때만).
pub(crate) fn snapshot_file(path: &Path) -> Option<PathBuf> {
    file_of(path).filter(|f| f.is_file())
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
/// ★ 객체 변경 전 DDL 백업 폴더(114 §7 · D-302 · 10-15): `<설정 폴더>/backups/objects/` — DROP·REPLACE/ALTER·TRUNCATE/RENAME 공용.
pub(crate) fn objects_dir() -> Option<PathBuf> {
    Some(nsql_settings::config_dir()?.join("backups").join("objects"))
}

/// 종전(10-01 · D-233) 삭제 백업 폴더 — 새 파일은 안 쓰고 보관 정리만 같은 규칙으로(이주 없음 · 114 §7).
pub(crate) fn drop_dir() -> Option<PathBuf> {
    Some(nsql_settings::config_dir()?.join("backups").join("drop"))
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | '$' | '#') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 로컬 시각 → 17자리 `yyyyMMddHHmmssSSS`(D-305 확장자).
pub(crate) fn stamp17(t: &nsql_log::LocalTime) -> String {
    t.stamp().chars().filter(char::is_ascii_digit).collect()
}

/// 백업 파일 줄기 `<프로필>_<스키마>.<객체>_<종류>`(D-305 · 프로필 비면 `conn`).
pub(crate) fn object_stem(profile: &str, o: &nsql_catalog::ObjectInfo) -> String {
    let prof = if profile.trim().is_empty() {
        "conn".to_string()
    } else {
        safe_name(profile.trim())
    };
    format!(
        "{prof}_{}.{}_{}",
        safe_name(&o.schema),
        safe_name(&o.name),
        safe_name(&o.kind.code().to_ascii_uppercase())
    )
}

/// ★ 객체 DDL을 `<줄기>.<yyyyMMddHHmmssSSS>`로 쓴다(D-305 · 사용자 10-15 · 확장자 = 17자리 시각 → 탐색기 이름 내림차순 = 객체별 최신순 ·
/// 같은 이름이 있으면 1 ms 올림 · 원자적 · 머리 주석 = 시각 · 프로필 · 방언 · 객체 · 동작). 실패 = 이유 문자열.
pub(crate) fn write_object(
    profile: &str,
    o: &nsql_catalog::ObjectInfo,
    dialect: &str,
    action: &str,
    ddl: &str,
) -> Result<PathBuf, String> {
    let dir = objects_dir().ok_or_else(|| "no config dir".to_string())?;
    write_object_in(&dir, profile, o, dialect, action, ddl)
}

/// [`write_object`]의 폴더 인자 꼴(시험 · 폴더를 바꿔 쓰는 호출자).
pub(crate) fn write_object_in(
    dir: &Path,
    profile: &str,
    o: &nsql_catalog::ObjectInfo,
    dialect: &str,
    action: &str,
    ddl: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let now = nsql_log::now_local();
    let stamp = now.stamp();
    let stem = object_stem(profile, o);
    let mut ext: u64 = stamp17(&now).parse().unwrap_or(0);
    let mut file = dir.join(format!("{stem}.{ext:017}"));
    while file.exists() {
        ext += 1;
        file = dir.join(format!("{stem}.{ext:017}"));
    }
    let body = format!(
        "-- nexa-sql object backup · {stamp}\n-- profile: {profile} · dialect: {dialect} · object: {}.{} ({}) · action: {action}\n\n{}\n",
        o.schema,
        o.name,
        o.kind.code().to_ascii_uppercase(),
        ddl.trim_end()
    );
    let tmp = dir.join(format!("{stem}.{ext:017}.tmp"));
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &file).map_err(|e| format!("{}: {e}", file.display()))?;
    // 쓴 것을 다시 읽어 비어 있지 않은지 확인(정상 생성이 확인된 경우에만 변경이 이어진다).
    let n = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    if n == 0 {
        return Err(format!("{}: empty", file.display()));
    }
    Ok(file)
}

/// 백업 파일인가(`<줄기>.<17자리>` 또는 종전 `.sql`).
fn is_backup_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "sql" || (e.len() == 17 && e.bytes().all(|b| b.is_ascii_digit())))
}

/// ★ 객체 백업 보관(D-302 · D-305): `days` 지난 파일(수정 시각) 삭제 + `keep` > 0이면 **객체(줄기)별 최신 N**만 남김(이름 내림차순 =
/// 시각 내림차순) · 두 폴더(`objects/` · 종전 `drop/`) 모두 — 지운 개수.
pub(crate) fn prune_objects(days: u64, keep: usize) -> usize {
    let mut n = 0;
    for dir in [objects_dir(), drop_dir()].into_iter().flatten() {
        n += prune_dir(&dir, days, keep);
    }
    n
}

pub(crate) fn prune_dir(dir: &Path, days: u64, keep: usize) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let limit = std::time::Duration::from_secs(days.max(1) * 86_400);
    let mut n = 0;
    let mut by_stem: std::collections::HashMap<String, Vec<(String, PathBuf)>> =
        std::collections::HashMap::new();
    for e in rd.flatten() {
        let p = e.path();
        if !is_backup_file(&p) {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old {
            if std::fs::remove_file(&p).is_ok() {
                n += 1;
            }
            continue;
        }
        if keep > 0 {
            if let (Some(stem), Some(name)) = (
                p.file_stem().and_then(|s| s.to_str()),
                p.file_name().and_then(|s| s.to_str()),
            ) {
                by_stem
                    .entry(stem.to_string())
                    .or_default()
                    .push((name.to_string(), p.clone()));
            }
        }
    }
    if keep > 0 {
        for (_, mut files) in by_stem {
            files.sort_by(|a, b| b.0.cmp(&a.0));
            for (_, p) in files.into_iter().skip(keep) {
                if std::fs::remove_file(&p).is_ok() {
                    n += 1;
                }
            }
        }
    }
    n
}

pub(crate) fn remove(path: &Path) {
    if let Some(f) = file_of(path) {
        let _ = std::fs::remove_file(f);
    }
}

/// 스냅숏 읽기(검증용) — 지금 디스크 해시가 스냅숏의 것과 같으면 `Some(본문)` · 다르거나 없으면 None(`Err` = 밖에서 바뀜).
/// ★ 복원에는 쓰지 않는다(사용자 09-23 "백업은 백업용으로만" — 복원 원천 = 프로젝트 파일 `tabs[].text`) → 시험에서만.
/// 자동 저장 캡처의 탭별 기억(사용자 09-30 · 72 §3) — 바뀌지 않은 탭은 앵커·스냅숏·디스크 해시를 다시 만들지 않는다.
#[derive(Default)]
pub(crate) struct CaptureMark {
    /// 앵커를 만든 때의 (epoch, change_seq, 캐럿).
    pub(crate) anchor_key: Option<(u64, u64, usize)>,
    /// (줄 원문, 앞 줄, 뒷 줄).
    pub(crate) anchor: (String, String, String),
    /// 마지막으로 쓴 스냅숏의 (epoch, change_seq).
    pub(crate) backup_key: Option<(u64, u64)>,
    /// 디스크 파일 (수정 시각, 길이) → 해시 — 원본이 그대로면 다시 읽지 않는다.
    disk: Option<(std::time::SystemTime, u64, u64)>,
    /// 상한 초과 안내를 이미 했다(다시 상한 안으로 들어오면 풀림).
    pub(crate) over_noticed: bool,
}

impl CaptureMark {
    /// 디스크 파일 해시(`backups::disk_hash`) — 수정 시각·길이가 지난번과 같으면 캐시.
    pub(crate) fn disk_hash(&mut self, p: &std::path::Path) -> u64 {
        let meta = std::fs::metadata(p)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        if let (Some((t, n)), Some((ct, cn, h))) = (meta, self.disk) {
            if t == ct && n == cn {
                return h;
            }
        }
        let h = disk_hash(p);
        self.disk = meta.map(|(t, n)| (t, n, h));
        h
    }
}

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod object_tests {
    use super::*;

    fn obj(schema: &str, name: &str) -> nsql_catalog::ObjectInfo {
        nsql_catalog::ObjectInfo {
            db: String::new(),
            schema: schema.into(),
            name: name.into(),
            kind: nsql_catalog::ObjectKind::Table,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        }
    }

    #[test]
    fn object_backup_name_rule_collision_and_keep() {
        let dir = std::env::temp_dir().join(format!("nsql-objbk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let o = obj("HR", "EMP");
        assert_eq!(object_stem("APPDB", &o), "APPDB_HR.EMP_TABLE");
        assert_eq!(object_stem("", &obj("a b", "x/y")), "conn_a_b.x_y_TABLE");
        // 같은 ms에 둘 = 뒤 것은 1 ms 올림(이름 다름 · 둘 다 존재).
        let a = write_object_in(
            &dir,
            "APPDB",
            &o,
            "oracle",
            "drop",
            "CREATE TABLE EMP (A INT)",
        )
        .unwrap();
        let b = write_object_in(
            &dir,
            "APPDB",
            &o,
            "oracle",
            "alter",
            "CREATE TABLE EMP (A INT, B INT)",
        )
        .unwrap();
        assert_ne!(a, b);
        let ea = a.extension().unwrap().to_str().unwrap().to_string();
        let eb = b.extension().unwrap().to_str().unwrap().to_string();
        assert_eq!(ea.len(), 17);
        assert!(ea.bytes().all(|c| c.is_ascii_digit()));
        assert!(eb > ea, "{ea} < {eb}");
        assert!(a
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("APPDB_HR.EMP_TABLE."));
        let body = std::fs::read_to_string(&b).unwrap();
        assert!(body.starts_with("-- nexa-sql object backup"));
        assert!(body.contains("action: alter"));
        // 다른 객체 1 + 같은 객체 셋째 → keep 2 = EMP 가장 옛 것(a) 하나만 지움.
        let c = write_object_in(&dir, "APPDB", &o, "oracle", "drop", "x").unwrap();
        let other =
            write_object_in(&dir, "APPDB", &obj("HR", "DEPT"), "oracle", "drop", "y").unwrap();
        assert_eq!(prune_dir(&dir, 365, 2), 1);
        assert!(!a.exists() && b.exists() && c.exists() && other.exists());
        // keep 0 = 개수 정리 없음 · 일수만.
        assert_eq!(prune_dir(&dir, 365, 0), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
