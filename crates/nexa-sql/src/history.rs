//! ★ 로컬 히스토리 = 시점 스냅샷 저장소(docs/19 §3 · C-1 파일 저장 시점(11-06) · O-1 오브젝트 열기/컴파일/서버 본 공용).
//!
//! 위치 = `NSQL_HOME/history/files/<fnv64(소문자 · `/` 정규화 경로)>/` · `…/history/objects/<object_stem>/`(D-378 = 앱 데이터 폴더만 ·
//! 프로젝트 연계 파일 0 = D-407). 파일 이름 = `<yyyyMMddHHmmssSSS>_<종류>_<내용 해시 16진>.sql`(고정 = `…_pin.sql`) — 이름만으로
//! 시각·종류·해시를 알아 색인 파일이 없다. 같은 해시가 **마지막 시점**이면 새로 만들지 않는다(19 §3). 상한 = 항목당
//! `history.max_per_item`(오래된 것부터 · 고정은 보존) · 전체 `history.max_mb`(폴더 전체 · 오래된 것부터). 폴더 인자 `*_in` 꼴 =
//! 단위 시험은 `temp_dir()`에만 쓴다(61 §2-4).

use std::path::{Path, PathBuf};

use crate::backups::{fnv64, stamp17};

/// 시점의 종류(파일 이름 둘째 조각).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tag {
    /// 파일 저장 직후의 본문(C-1).
    Save,
    /// 오브젝트를 열 때 서버에서 받은 원본(O-1 `_open`).
    Open,
    /// 컴파일 직전의 버퍼(O-1 `_compile`).
    Compile,
    /// 컴파일 성공 뒤 서버에서 다시 읽은 본(O-1 `_server`).
    Server,
    /// 시점으로 되돌린 본(O-1 복원).
    Restore,
}

impl Tag {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Tag::Save => "save",
            Tag::Open => "open",
            Tag::Compile => "compile",
            Tag::Server => "server",
            Tag::Restore => "restore",
        }
    }

    fn parse(s: &str) -> Option<Tag> {
        Some(match s {
            "save" => Tag::Save,
            "open" => Tag::Open,
            "compile" => Tag::Compile,
            "server" => Tag::Server,
            "restore" => Tag::Restore,
            _ => return None,
        })
    }
}

/// 시점 하나(파일 이름에서 읽은 것 + 크기).
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub path: PathBuf,
    /// 17자리 `yyyyMMddHHmmssSSS`.
    pub stamp: String,
    pub tag: Tag,
    pub hash: u64,
    pub bytes: u64,
    pub pinned: bool,
}

impl Entry {
    /// 표시용 시각 `yyyy-MM-dd HH:mm:ss`.
    pub(crate) fn when(&self) -> String {
        let s = &self.stamp;
        if s.len() >= 14 {
            format!(
                "{}-{}-{} {}:{}:{}",
                &s[0..4],
                &s[4..6],
                &s[6..8],
                &s[8..10],
                &s[10..12],
                &s[12..14]
            )
        } else {
            s.clone()
        }
    }
}

/// 저장소 뿌리 `NSQL_HOME/history`.
pub(crate) fn root() -> Option<PathBuf> {
    Some(nsql_settings::config_dir()?.join("history"))
}

/// 파일 항목 폴더(경로 소문자 · `/` 정규화 → fnv64 · `backups` 파일 스냅숏과 같은 열쇠 규칙).
pub(crate) fn file_dir_in(root: &Path, path: &Path) -> PathBuf {
    let s = path.to_string_lossy().to_lowercase().replace('\\', "/");
    root.join("files")
        .join(format!("{:016x}", fnv64(s.as_bytes())))
}

pub(crate) fn file_dir(path: &Path) -> Option<PathBuf> {
    Some(file_dir_in(&root()?, path))
}

/// 오브젝트 항목 폴더(`backups::object_stem` 줄기 · O-1).
#[allow(dead_code)] // O-1(오브젝트 시점)에서 쓴다 — C-1은 파일 항목만.
pub(crate) fn object_dir_in(root: &Path, stem: &str) -> PathBuf {
    root.join("objects").join(stem)
}

/// 내용 해시(FNV-1a 64 · 파일 이름의 셋째 조각).
pub(crate) fn content_hash(text: &str) -> u64 {
    fnv64(text.as_bytes())
}

/// 시점 하나 저장 — 마지막 시점과 해시가 같으면 `Ok(None)`(새로 만들지 않음) · 원자적(.tmp → rename) · 저장 뒤 항목 상한 적용.
/// 같은 시각(ms)이 이미 있으면 1 ms씩 올린다(backups와 같은 규칙). 실패 = 이유 문자열.
pub(crate) fn store_in(
    dir: &Path,
    text: &str,
    tag: Tag,
    keep: usize,
) -> Result<Option<PathBuf>, String> {
    let hash = content_hash(text);
    let list = list_in(dir);
    if list.first().is_some_and(|e| e.hash == hash) {
        return Ok(None);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let now = nsql_log::now_local();
    let mut stamp: u64 = stamp17(&now).parse().unwrap_or(0);
    while list.iter().any(|e| e.stamp == format!("{stamp:017}")) {
        stamp += 1;
    }
    let name = format!("{stamp:017}_{}_{hash:016x}.sql", tag.as_str());
    let path = dir.join(&name);
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, text.as_bytes())
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", path.display())
        })?;
    prune_item_in(dir, keep);
    Ok(Some(path))
}

/// 파일 이름 → 시점(규칙에 맞지 않으면 None · 경로·크기는 비움).
pub(crate) fn entry_of_name(name: &str) -> Option<Entry> {
    let stem = name.strip_suffix(".sql")?;
    let (stem, pinned) = match stem.strip_suffix("_pin") {
        Some(s) => (s, true),
        None => (stem, false),
    };
    let mut it = stem.splitn(3, '_');
    let stamp = it.next()?;
    let tag = Tag::parse(it.next()?)?;
    let hash = u64::from_str_radix(it.next()?, 16).ok()?;
    if stamp.len() != 17 || !stamp.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(Entry {
        path: PathBuf::new(),
        stamp: stamp.to_string(),
        tag,
        hash,
        bytes: 0,
        pinned,
    })
}

/// 경로 → 시점(파일 이름 규칙 + 실제 크기).
pub(crate) fn entry_of(path: &Path) -> Option<Entry> {
    let name = path.file_name()?.to_str()?;
    let mut e = entry_of_name(name)?;
    e.path = path.to_path_buf();
    e.bytes = std::fs::metadata(path).map_or(0, |m| m.len());
    Some(e)
}

/// 항목 폴더의 시점 목록(**최신 먼저** · 규칙에 맞는 파일만 · 폴더 없음 = 빈 목록).
pub(crate) fn list_in(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = rd.flatten().filter_map(|d| entry_of(&d.path())).collect();
    out.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    out
}

/// 항목 상한: 고정 아닌 시점 중 오래된 것부터 지워 `keep`개만 남긴다. 반환 = 지운 수.
pub(crate) fn prune_item_in(dir: &Path, keep: usize) -> usize {
    let mut n = 0;
    let mut kept = 0;
    for e in list_in(dir) {
        if e.pinned {
            continue;
        }
        if kept < keep {
            kept += 1;
            continue;
        }
        if std::fs::remove_file(&e.path).is_ok() {
            n += 1;
        }
    }
    n
}

/// 전체 상한: `root` 아래(files · objects) 전체 크기가 `max_bytes`를 넘으면 고정 아닌 것 중 **전체에서 가장 오래된** 것부터 지운다.
/// 반환 = 지운 수. 새 시점을 쓴 직후에만 부른다(저장 때 1회 · 폴더 훑기).
pub(crate) fn prune_total_in(root: &Path, max_bytes: u64) -> usize {
    let mut all: Vec<Entry> = Vec::new();
    for sub in ["files", "objects"] {
        let Ok(rd) = std::fs::read_dir(root.join(sub)) else {
            continue;
        };
        for d in rd.flatten() {
            let p = d.path();
            if p.is_dir() {
                all.extend(list_in(&p));
            }
        }
    }
    let mut total: u64 = all.iter().map(|e| e.bytes).sum();
    if total <= max_bytes {
        return 0;
    }
    all.sort_by(|a, b| a.stamp.cmp(&b.stamp));
    let mut n = 0;
    for e in all {
        if total <= max_bytes {
            break;
        }
        if e.pinned {
            continue;
        }
        if std::fs::remove_file(&e.path).is_ok() {
            total = total.saturating_sub(e.bytes);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    // 시험 = 실패가 곧 판정(backups.rs·explorer.rs 시험 모듈과 같은 관례).
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nsql-hist-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn name_roundtrip_and_reject() {
        let e = entry_of_name("20261106143000123_save_00000000deadbeef.sql").unwrap();
        assert_eq!(e.stamp, "20261106143000123");
        assert_eq!(e.tag, Tag::Save);
        assert_eq!(e.hash, 0xdead_beef);
        assert!(!e.pinned);
        assert_eq!(e.when(), "2026-11-06 14:30:00");
        let p = entry_of_name("20261106143000123_open_0000000000000001_pin.sql").unwrap();
        assert!(p.pinned);
        assert_eq!(p.tag, Tag::Open);
        assert!(entry_of_name("x.sql").is_none());
        assert!(entry_of_name("2026_save_1.sql").is_none());
        assert!(entry_of_name("20261106143000123_what_0.sql").is_none());
        assert!(entry_of_name("20261106143000123_save_zz.sql").is_none());
        assert!(entry_of_name("20261106143000123_save_1.txt").is_none());
    }

    #[test]
    fn store_skips_same_content_and_prunes_per_item() {
        let root = tmp("store");
        let dir = file_dir_in(&root, Path::new("C:\\Work\\a.sql"));
        assert_eq!(dir, file_dir_in(&root, Path::new("c:/work/A.SQL")));
        let p1 = store_in(&dir, "select 1", Tag::Save, 2).unwrap();
        assert!(p1.is_some());
        // 같은 내용 = 새 시점 없음.
        assert!(store_in(&dir, "select 1", Tag::Save, 2).unwrap().is_none());
        assert!(store_in(&dir, "select 2", Tag::Save, 2).unwrap().is_some());
        assert!(store_in(&dir, "select 3", Tag::Compile, 2)
            .unwrap()
            .is_some());
        let list = list_in(&dir);
        assert_eq!(
            list.len(),
            2,
            "항목 상한 2 → 가장 오래된 `select 1`이 지워진다"
        );
        assert_eq!(list[0].tag, Tag::Compile);
        assert_eq!(std::fs::read_to_string(&list[0].path).unwrap(), "select 3");
        assert_eq!(list[1].tag, Tag::Save);
        assert!(list[0].stamp > list[1].stamp, "최신 먼저");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_total_oldest_first_keeps_pinned() {
        let root = tmp("total");
        let d1 = file_dir_in(&root, Path::new("/a.sql"));
        let d2 = object_dir_in(&root, "p_s.o_PROCEDURE");
        store_in(&d1, "aaaaaaaaaa", Tag::Save, 10).unwrap();
        store_in(&d2, "bbbbbbbbbb", Tag::Open, 10).unwrap();
        store_in(&d2, "cccccccccc", Tag::Compile, 10).unwrap();
        // 가장 오래된 d1 시점을 고정 → 그 다음 오래된 것(d2 open)이 먼저 지워진다.
        let first = list_in(&d1).pop().unwrap();
        let pinned = first.path.with_file_name(format!(
            "{}_{}_{:016x}_pin.sql",
            first.stamp,
            first.tag.as_str(),
            first.hash
        ));
        std::fs::rename(&first.path, &pinned).unwrap();
        assert_eq!(prune_total_in(&root, 100), 0, "상한 안이면 0");
        assert_eq!(prune_total_in(&root, 25), 1);
        assert_eq!(list_in(&d1).len(), 1, "고정은 보존");
        assert_eq!(list_in(&d2).len(), 1);
        assert_eq!(
            list_in(&d2)[0].tag,
            Tag::Compile,
            "가장 오래된 open이 지워졌다"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
