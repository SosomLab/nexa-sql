//! **글로벌 변수 층 파일**(docs/63 §11 · T-307): `<폴더>/global.sql` — 실행할 수 있는 스크립트(`VAR 이름 타입` + `EXEC :이름 := 리터럴` ·
//! [`crate::vars_to_script`]) · 비밀·커서 값은 쓰지 않는다 · 폴더는 인자(단위 시험이 실제 설정 폴더를 안 건드리게). GUI(nexa-sql)와 CLI(`nsql run` ·
//! 설정 `vars.cli_global`)가 같은 함수를 쓴다 — 종전엔 GUI 전용이라 CLI가 글로벌을 보지도 쓰지도 못했다(사용자 10-07).

use crate::{Layer, VarState};
use std::path::{Path, PathBuf};

/// 읽을 최대 크기(그보다 크면 우리 것이 아니다).
const MAX_BYTES: u64 = 4 << 20;

/// 글로벌 파일 경로.
#[must_use]
pub fn global_path(dir: &Path) -> PathBuf {
    dir.join("global.sql")
}

/// 쓴다(임시 파일 → 이름 바꾸기) — 남길 것이 없으면 옛 파일을 지운다.
pub fn store_global(dir: &Path, vars: &[VarState]) {
    let p = global_path(dir);
    let text = crate::vars_to_script(vars);
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

/// 읽는다(없거나 너무 크면 빈 목록) — 층은 늘 `Global`.
#[must_use]
pub fn load_global(dir: &Path) -> Vec<VarState> {
    let p = global_path(dir);
    match std::fs::metadata(&p) {
        Ok(m) if m.len() > 0 && m.len() <= MAX_BYTES => {}
        _ => return Vec::new(),
    }
    std::fs::read_to_string(&p)
        .map(|t| {
            let mut v = crate::vars_from_script(&t);
            for s in &mut v {
                s.layer = Layer::Global;
            }
            v
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Value, VarType};

    /// 쓰고 읽으면 같은 값 · 층 = Global · 비우면 파일 삭제.
    #[test]
    fn global_roundtrip_and_remove_when_empty() {
        let dir = std::env::temp_dir().join(format!("nsql_globalvars_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let vars = vec![VarState {
            name: "KEEP".into(),
            ty: VarType::Number,
            value: Value::Decimal("77".into()),
            declared: true,
            secret: false,
            layer: Layer::Global,
        }];
        store_global(&dir, &vars);
        let back = load_global(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "KEEP");
        assert_eq!(back[0].layer, Layer::Global);
        store_global(&dir, &[]);
        assert!(load_global(&dir).is_empty());
        assert!(!global_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
