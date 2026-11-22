//! ★ 로컬 히스토리 = 시점 스냅샷 저장소(docs/19 §3 · C-1 파일 저장 시점(11-06) · O-1 오브젝트 열기/컴파일/서버 본 공용).
//!
//! 위치 = `NSQL_HOME/history/files/<fnv64(소문자 · `/` 정규화 경로)>/` · `…/history/objects/<object_stem>/`(D-378 = 앱 데이터 폴더만 ·
//! 프로젝트 연계 파일 0 = D-407). 파일 이름 = `<yyyyMMddHHmmssSSS>_<종류>_<내용 해시 16진>.sql`(고정 = `…_pin.sql`) — 이름만으로
//! 시각·종류·해시를 알아 색인 파일이 없다. 같은 해시가 **마지막 시점**이면 새로 만들지 않는다(19 §3). 상한 = 항목당
//! `history.max_per_item`(오래된 것부터 · 고정은 보존) · 전체 `history.max_mb`(폴더 전체 · 오래된 것부터). 폴더 인자 `*_in` 꼴 =
//! 단위 시험은 `temp_dir()`에만 쓴다(61 §2-4).

use std::path::{Path, PathBuf};

use crate::backups::{fnv64, stamp17};

/// ★ T-513: 표식을 실어 보낸 서버 소스 요청 한 건 — 응답(`OpenSql { token }`)·실패(`SourceError(token)`)가 표식으로 이 기록을
/// 찾는다. 폴더·탭·용도를 **요청 때** 적어 두므로 응답 때 활성 탭·세션이 달라져도 섞이지 않는다.
pub(crate) struct HistReq {
    pub token: u64,
    /// 요청을 건 오브젝트 탭(알림 카드가 이 탭으로 간다).
    pub tab: u64,
    pub origin: crate::explorer::ObjectOrigin,
    /// 히스토리 항목 폴더(요청 때의 세션 프로필 기준 · 설정 폴더를 못 얻으면 None).
    pub dir: Option<PathBuf>,
    /// 서버 변경 확인인가(아니면 컴파일 뒤 `server` 시점 저장).
    pub check: bool,
    /// 자동 확인인가(바뀌지 않았으면 조용히).
    pub auto: bool,
    pub at: std::time::Instant,
}

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
    /// 서버 변경 확인(`edit.history_check`)에서 **마지막 서버 본과 달랐던** 서버 소스(O-1 외부 변경 · 19 §3).
    External,
}

impl Tag {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Tag::Save => "save",
            Tag::Open => "open",
            Tag::Compile => "compile",
            Tag::Server => "server",
            Tag::Restore => "restore",
            Tag::External => "external",
        }
    }

    fn parse(s: &str) -> Option<Tag> {
        Some(match s {
            "save" => Tag::Save,
            "open" => Tag::Open,
            "compile" => Tag::Compile,
            "server" => Tag::Server,
            "restore" => Tag::Restore,
            "external" => Tag::External,
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

/// 오브젝트 항목 폴더(`backups::object_stem` 줄기 `<프로필>_<스키마>.<객체>_<종류>` · O-1 · 19 §3).
pub(crate) fn object_dir_in(root: &Path, stem: &str) -> PathBuf {
    root.join("objects").join(stem)
}

pub(crate) fn object_dir(stem: &str) -> Option<PathBuf> {
    Some(object_dir_in(&root()?, stem))
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

/// 서버 변경 확인의 판정(O-1 외부 변경 · 19 §3) — 기준 = 시점 목록에서 **서버가 가졌던 본**(열기 · 컴파일 뒤 서버 본 · 앞선 외부 변경) 중 최신.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ServerCheck {
    /// 견줄 서버 본이 없다(히스토리를 켜기 전에 연 탭 · 시점이 정리됨) — 지금 본을 기준으로 삼는다.
    NoBaseline,
    /// 기준 시점(표시용 시각)과 같다.
    Unchanged(String),
    /// 기준 시점(표시용 시각) 뒤로 서버 소스가 달라졌다.
    Changed(String),
}

/// `list`(최신 먼저)와 방금 받은 서버 소스의 해시로 판정한다. 버퍼에서 나온 시점(컴파일 직전 · 복원 · 저장)은 기준이 아니다.
pub(crate) fn server_check(list: &[Entry], hash: u64) -> ServerCheck {
    let base = list
        .iter()
        .find(|e| matches!(e.tag, Tag::Open | Tag::Server | Tag::External));
    match base {
        None => ServerCheck::NoBaseline,
        Some(e) if e.hash == hash => ServerCheck::Unchanged(e.when()),
        Some(e) => ServerCheck::Changed(e.when()),
    }
}

/// ★ 가장 최신 시점이 `hash`와 같은 **버퍼 시점**(compile · restore · save)이면 `server`로 다시 표시한다(파일 이름의 종류 조각만
/// 바꾼다 · 본문·시각·고정 그대로) — 컴파일 뒤 서버 본이 그 버퍼와 같아 따로 저장되지 않았을 때 서버 기준을 남기는 길(검토 11-23).
/// 이미 서버 쪽 시점이거나 해시가 다르면 아무것도 하지 않는다. 반환 = 바꿨는가.
pub(crate) fn mark_newest_server_in(dir: &Path, hash: u64) -> bool {
    mark_newest_in(dir, hash, Tag::Server)
}

/// [`mark_newest_server_in`]의 일반형 — 다시 표시할 종류를 받는다(서버 쪽 종류 `open`·`server`·`external`만 뜻이 있다).
pub(crate) fn mark_newest_in(dir: &Path, hash: u64, tag: Tag) -> bool {
    let Some(e) = list_in(dir).into_iter().next() else {
        return false;
    };
    if e.hash != hash || matches!(e.tag, Tag::Open | Tag::Server | Tag::External) {
        return false;
    }
    let pin = if e.pinned { "_pin" } else { "" };
    let name = format!("{}_{}_{:016x}{pin}.sql", e.stamp, tag.as_str(), e.hash);
    std::fs::rename(&e.path, e.path.with_file_name(name)).is_ok()
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

/// ★ O-1 고정(pin) 켜기/끄기(19 §3 "고정 보존"): 시점 파일 이름의 `_pin` 꼬리를 붙이거나 뗀다(본문은 건드리지 않는다 · 이름만).
/// 고정된 시점은 항목 상한·전체 상한 정리에서 빠진다. 반환 = (새 경로, 바뀐 뒤 고정 여부). 규칙에 맞지 않는 이름·이름 바꾸기 실패 = `Err`.
pub(crate) fn toggle_pin(path: &Path) -> Result<(PathBuf, bool), String> {
    let e = entry_of(path).ok_or_else(|| format!("{}: not a history snapshot", path.display()))?;
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".sql"))
        .ok_or_else(|| format!("{}: not a history snapshot", path.display()))?;
    let name = match stem.strip_suffix("_pin") {
        Some(base) => format!("{base}.sql"),
        None => format!("{stem}_pin.sql"),
    };
    let to = path.with_file_name(name);
    std::fs::rename(path, &to).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok((to, !e.pinned))
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

    /// O-1 외부 변경 판정: 기준 = 서버가 가졌던 본(open · server · external) 중 최신 — 버퍼에서 나온 시점은 건너뛴다.
    #[test]
    fn server_check_uses_latest_server_side_snapshot() {
        let e = |stamp: &str, tag: Tag, hash: u64| Entry {
            path: PathBuf::new(),
            stamp: stamp.to_string(),
            tag,
            hash,
            bytes: 0,
            pinned: false,
        };
        assert_eq!(server_check(&[], 1), ServerCheck::NoBaseline);
        // 버퍼 시점뿐 = 기준 없음.
        let only_buf = [
            e("20261122100000000", Tag::Compile, 1),
            e("20261122090000000", Tag::Save, 1),
        ];
        assert_eq!(server_check(&only_buf, 2), ServerCheck::NoBaseline);
        assert_eq!(server_check(&only_buf, 1), ServerCheck::NoBaseline);
        // 최신 먼저: compile(7) · server(5) · open(3) → 기준 = server(5).
        let list = [
            e("20261122120000000", Tag::Compile, 7),
            e("20261122110000000", Tag::Server, 5),
            e("20261122100000000", Tag::Open, 3),
        ];
        assert_eq!(
            server_check(&list, 5),
            ServerCheck::Unchanged("2026-11-22 11:00:00".into())
        );
        // 컴파일 직전 버퍼(7)와 같아도 서버 본(5)과 다르면 바뀐 것 · 옛 open(3)으로 돌아가도 바뀐 것.
        assert_eq!(
            server_check(&list, 7),
            ServerCheck::Changed("2026-11-22 11:00:00".into())
        );
        // ★ 자기 컴파일(검토 11-23): 컴파일 성공 뒤 서버 본이 버퍼와 같으면 그 버퍼 시점이 `server`로 다시 표시돼 기준이 된다
        //   (`mark_newest_server_in`) → 그 뒤에 버퍼 시점(복원·실패한 컴파일)이 더 쌓여도 "바뀌지 않음".
        let own = [
            e("20261122140000000", Tag::Restore, 9),
            e("20261122130000000", Tag::Compile, 8),
            e("20261122120000000", Tag::Server, 7),
            e("20261122100000000", Tag::Open, 3),
        ];
        assert_eq!(
            server_check(&own, 7),
            ServerCheck::Unchanged("2026-11-22 12:00:00".into())
        );
        assert_eq!(
            server_check(&list, 3),
            ServerCheck::Changed("2026-11-22 11:00:00".into())
        );
        // 앞선 외부 변경이 기준이 된다(같은 변경을 두 번 알리지 않는다).
        let ext = [
            e("20261122130000000", Tag::External, 9),
            e("20261122110000000", Tag::Server, 5),
        ];
        assert_eq!(
            server_check(&ext, 9),
            ServerCheck::Unchanged("2026-11-22 13:00:00".into())
        );
        assert_eq!(
            entry_of_name("20261122130000000_external_0000000000000009.sql").map(|x| x.tag),
            Some(Tag::External)
        );
    }

    /// 컴파일 뒤 서버 본이 직전 버퍼 시점과 같을 때: 새 파일을 만들지 않고 그 시점을 `server`로 다시 표시한다(본문·시각·고정 그대로) ·
    /// 해시가 다르거나 이미 서버 쪽 시점이면 그대로.
    #[test]
    fn mark_newest_server_retags_matching_buffer_snapshot() {
        let dir = tmp("mark-server");
        store_in(&dir, "open text", Tag::Open, 50).unwrap();
        store_in(&dir, "compiled text", Tag::Compile, 50).unwrap();
        let h = content_hash("compiled text");
        // 같은 내용의 server 저장은 생략된다(최신과 같은 해시).
        assert!(store_in(&dir, "compiled text", Tag::Server, 50)
            .unwrap()
            .is_none());
        assert!(!mark_newest_server_in(&dir, content_hash("other")));
        assert!(mark_newest_server_in(&dir, h));
        let list = list_in(&dir);
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].tag, list[0].hash), (Tag::Server, h));
        assert_eq!(
            std::fs::read_to_string(&list[0].path).unwrap(),
            "compiled text"
        );
        assert_eq!(
            server_check(&list, h),
            ServerCheck::Unchanged(list[0].when())
        );
        // 이미 서버 쪽 시점이면 다시 바꾸지 않는다 · 고정 꼬리는 지킨다.
        assert!(!mark_newest_server_in(&dir, h));
        store_in(&dir, "restored", Tag::Restore, 50).unwrap();
        let newest = list_in(&dir)[0].path.clone();
        let (pinned, on) = toggle_pin(&newest).unwrap();
        assert!(on);
        assert!(mark_newest_server_in(&dir, content_hash("restored")));
        let top = &list_in(&dir)[0];
        assert!(top.pinned && top.tag == Tag::Server);
        let _ = pinned;
        let _ = std::fs::remove_dir_all(&dir);
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

    /// ★ O-1: 고정 = 이름만 바뀐다(본문·시각·종류·해시 그대로) · 고정된 시점은 항목 상한 정리에서 남는다 · 다시 부르면 풀린다.
    #[test]
    fn toggle_pin_renames_and_survives_item_prune() {
        let root = tmp("pin");
        let dir = file_dir_in(&root, Path::new("/pin.sql"));
        store_in(&dir, "select 1", Tag::Save, 10).unwrap();
        let old = list_in(&dir).pop().unwrap();
        let (to, on) = toggle_pin(&old.path).unwrap();
        assert!(on && to.to_string_lossy().ends_with("_pin.sql") && !old.path.exists());
        let e = entry_of(&to).unwrap();
        assert!(e.pinned && e.stamp == old.stamp && e.tag == old.tag && e.hash == old.hash);
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "select 1");
        // 상한 1 — 새 시점 둘을 더 써도 고정된 가장 오래된 것은 남는다(고정 1 + 고정 아닌 1).
        store_in(&dir, "select 2", Tag::Save, 1).unwrap();
        store_in(&dir, "select 3", Tag::Save, 1).unwrap();
        let list = list_in(&dir);
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|x| x.pinned && x.stamp == old.stamp));
        // 다시 = 풀림 · 규칙에 맞지 않는 파일은 손대지 않는다.
        let (back, on) = toggle_pin(&to).unwrap();
        assert!(!on && !entry_of(&back).unwrap().pinned && back == old.path);
        let other = dir.join("notes.txt");
        std::fs::write(&other, "x").unwrap();
        assert!(toggle_pin(&other).is_err() && other.exists());
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
