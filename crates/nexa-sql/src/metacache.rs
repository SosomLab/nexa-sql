//! ★ L1 이름 층 디스크 캐시(docs/85 §9 · T-222 ① · 09-25): 접속(자격 단위 · 비밀번호 없음)마다 `NSQL_HOME/meta/<해시>.names`에
//! (스키마, 종류, 이름)을 남겨 **다음 실행의 첫 검색·완성이 즉시**(DataGrip식) — 서버 재읽기(L1)가 곧 덮어쓰므로 낡음은 잠깐.
//! 형식 = 텍스트(1행 `nsql-names 1` · 2행 `stamp <epoch>` · `S\t<스키마>` 목록 · `<스키마>\t<종류 코드>\t<이름>`) · 쓰기 = tmp + rename.

use nsql_catalog::ObjectKind;
use std::path::{Path, PathBuf};

pub(crate) type Entry = (String, ObjectKind, String);

/// 캐시 폴더(`NSQL_HOME/meta`).
pub(crate) fn dir() -> Option<PathBuf> {
    nsql_settings::config_dir().map(|d| d.join("meta"))
}

/// 자격 열쇠(`worker::cred_id` · 비밀번호 없음) → 파일 경로(FNV-1a 64 해시 · 파일 이름에 서버 정보를 남기지 않는다).
pub(crate) fn path_for(key: &str) -> Option<PathBuf> {
    dir().map(|d| d.join(format!("{:016x}.names", fnv64(key))))
}

fn fnv64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn kind_from_code(code: &str) -> Option<ObjectKind> {
    ObjectKind::ALL.iter().copied().find(|k| k.code() == code)
}

/// 저장(있던 파일은 통째로 교체). 실패는 조용히(캐시일 뿐).
pub(crate) fn store(path: &Path, stamp: u64, schemas: &[String], entries: &[Entry]) {
    let mut text = String::with_capacity(entries.len() * 32 + 64);
    text.push_str("nsql-names 1\n");
    text.push_str(&format!("stamp {stamp}\n"));
    for s in schemas {
        text.push_str("S\t");
        text.push_str(s);
        text.push('\n');
    }
    for (schema, kind, name) in entries {
        if schema.contains('\t') || name.contains('\t') || name.contains('\n') {
            continue;
        }
        text.push_str(schema);
        text.push('\t');
        text.push_str(kind.code());
        text.push('\t');
        text.push_str(name);
        text.push('\n');
    }
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = path.with_extension("names-tmp");
    if std::fs::write(&tmp, text.as_bytes())
        .and_then(|()| std::fs::rename(&tmp, path))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// 읽기 → (stamp, 스키마 목록, 항목). 형식이 다르면 None.
pub(crate) fn load(path: &Path) -> Option<(u64, Vec<String>, Vec<Entry>)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "nsql-names 1" {
        return None;
    }
    let stamp = lines
        .next()?
        .strip_prefix("stamp ")?
        .trim()
        .parse::<u64>()
        .ok()?;
    let mut schemas = Vec::new();
    let mut entries = Vec::new();
    for l in lines {
        if let Some(s) = l.strip_prefix("S\t") {
            schemas.push(s.to_string());
            continue;
        }
        let mut it = l.splitn(3, '\t');
        let (Some(schema), Some(code), Some(name)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let Some(kind) = kind_from_code(code) else {
            continue;
        };
        entries.push((schema.to_string(), kind, name.to_string()));
    }
    Some((stamp, schemas, entries))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// 왕복 · 탭이 든 이름은 건너뜀 · 모르는 종류 코드는 건너뜀 · 형식 불일치 = None · 열쇠 해시는 안정.
    #[test]
    fn roundtrip_and_robustness() {
        let d = std::env::temp_dir().join(format!("nsql-metacache-{}", std::process::id()));
        let p = d.join("x.names");
        let entries: Vec<Entry> = vec![
            ("HR".into(), ObjectKind::Table, "EMP".into()),
            ("HR".into(), ObjectKind::View, "V_EMP".into()),
            ("HR".into(), ObjectKind::Table, "BAD\tNAME".into()),
        ];
        store(&p, 42, &["HR".into(), "SALES".into()], &entries);
        let (stamp, schemas, got) = load(&p).expect("읽힘");
        assert_eq!(stamp, 42);
        assert_eq!(schemas, vec!["HR", "SALES"]);
        assert_eq!(got.len(), 2, "탭 든 이름 제외");
        assert_eq!(got[1].1, ObjectKind::View);
        std::fs::write(&p, "nsql-names 9\nstamp 1\n").unwrap();
        assert!(load(&p).is_none());
        std::fs::write(&p, "nsql-names 1\nstamp 1\nHR\tnope\tX\nHR\ttable\tY\n").unwrap();
        assert_eq!(load(&p).unwrap().2.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(fnv64("a"), fnv64("a"));
        assert_ne!(fnv64("oracle://a@h:1/db#"), fnv64("oracle://b@h:1/db#"));
    }
}
