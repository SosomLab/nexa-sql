//! **변수 표 보존**(D-136 · docs/63 V4): 탭의 변수(탭 층)를 **파일 경로별로** `<설정 폴더>/vars/<경로 해시>.sql`에 두고, 그 파일을
//! 다시 열 때 되살린다 — 앱을 껐다 켜도 `:V` 값이 이어진다. hot exit(S-1)이 생기면 이름 없는 탭까지 그쪽으로 넓힌다.
//!
//! - 내용은 **실행할 수 있는 스크립트**다(`VAR 이름 타입` + `EXEC :이름 := 리터럴` — nsql-script `vars_to_script`). 내보내기와 같은 형식이라
//!   사람이 읽고 고칠 수 있고, 그대로 실행해도 같은 결과다.
//! - **비밀 값·커서·여러 줄 글은 쓰지 않는다**(D-140). 끄면(`vars.persist`) 쓰지도 읽지도 않고, 오래된 파일은 `vars.persist_days` 뒤에 지운다.
//!   부하원 원장 = docs/39 §3(디스크 · 실행이 변수를 바꿨을 때만 몇 백 바이트).
//! - 폴더는 인자로 받는다(`*_in`) — 단위 테스트가 사용자의 실제 설정 폴더를 건드리지 않게(되돌리기 기록 파일과 같은 규칙).

use nsql_script::VarState;
use std::path::{Path, PathBuf};

/// 읽을 최대 크기(그보다 크면 우리 것이 아니다).
const MAX_BYTES: u64 = 4 << 20;

/// 보존 폴더(`<설정 폴더>/vars`).
pub(crate) fn dir() -> Option<PathBuf> {
    nsql_settings::config_dir().map(|d| d.join("vars"))
}

fn name_for(file: &Path) -> String {
    let full = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let mut key = full.to_string_lossy().into_owned();
    if cfg!(windows) {
        key = key.to_lowercase();
    }
    let mut h = nexa_ctl::edit::Hash64::new();
    h.write(key.as_bytes());
    format!("{:016x}.sql", h.finish())
}

/// 쓴다(임시 파일 → 이름 바꾸기) — 남길 것이 없으면 옛 파일을 지운다.
pub(crate) fn store_in(dir: &Path, file: &Path, vars: &[VarState]) {
    let p = dir.join(name_for(file));
    let text = nsql_script::vars_to_script(vars);
    if text.is_empty() {
        let _ = std::fs::remove_file(&p);
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    let tmp = p.with_extension("sql-tmp");
    if std::fs::write(&tmp, text.as_bytes())
        .and_then(|()| std::fs::rename(&tmp, &p))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// ★ 글로벌 층 파일(docs/63 §11): `vars/global.sql` — 실행 가능한 스크립트 · 비밀·커서 값 제외.
pub(crate) fn store_global(dir: &Path, vars: &[VarState]) {
    let p = dir.join("global.sql");
    let text = nsql_script::vars_to_script(vars);
    if text.is_empty() {
        let _ = std::fs::remove_file(&p);
        return;
    }
    let _ = std::fs::create_dir_all(dir);
    let tmp = p.with_extension("sql-tmp");
    if std::fs::write(&tmp, text.as_bytes())
        .and_then(|()| std::fs::rename(&tmp, &p))
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
    }
}

pub(crate) fn load_global(dir: &Path) -> Vec<VarState> {
    let p = dir.join("global.sql");
    match std::fs::metadata(&p) {
        Ok(m) if m.len() > 0 && m.len() <= MAX_BYTES => {}
        _ => return Vec::new(),
    }
    std::fs::read_to_string(&p)
        .map(|t| {
            let mut v = nsql_script::vars_from_script(&t);
            for s in &mut v {
                s.layer = nsql_script::Layer::Global;
            }
            v
        })
        .unwrap_or_default()
}

/// 읽는다(없거나 너무 크면 빈 목록).
pub(crate) fn load_in(dir: &Path, file: &Path) -> Vec<VarState> {
    let p = dir.join(name_for(file));
    match std::fs::metadata(&p) {
        Ok(m) if m.len() > 0 && m.len() <= MAX_BYTES => {}
        _ => return Vec::new(),
    }
    std::fs::read_to_string(&p)
        .map(|t| nsql_script::vars_from_script(&t))
        .unwrap_or_default()
}

/// 오래된 파일을 지운다(`days` 0 = 안 지움) — 지운 수.
pub(crate) fn prune_in(dir: &Path, days: u64) -> usize {
    if days == 0 {
        return 0;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let limit = std::time::Duration::from_secs(days * 86_400);
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "sql") {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Value, VarType};
    use nsql_script::Layer;

    #[test]
    fn store_load_and_remove_when_empty() {
        let dir = std::env::temp_dir().join(format!("nsql_varsfile_{}", std::process::id()));
        let file = dir.join("script.sql");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&file, "x");
        let vars = vec![VarState {
            name: "V_CD".into(),
            ty: VarType::Varchar2(6),
            value: Value::Str("SEBANG".into()),
            declared: false,
            secret: false,
            layer: Layer::Local,
        }];
        store_in(&dir, &file, &vars);
        let back = load_in(&dir, &file);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].value, Value::Str("SEBANG".into()));
        store_in(&dir, &file, &[]);
        assert!(
            load_in(&dir, &file).is_empty(),
            "남길 것이 없으면 파일도 없다"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
