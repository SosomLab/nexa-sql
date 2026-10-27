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

/// 쓴다(임시 파일 → 이름 바꾸기) — 남길 것이 없으면 옛 파일을 지운다. 파일에 이미 있는 치환 변수 글로벌 층(`DEFINE` 줄)은 그대로 남긴다
/// (바인드만 아는 호출자용 · 둘 다 알면 [`store_global_full`]).
pub fn store_global(dir: &Path, vars: &[VarState]) {
    let macros = load_global_macros(dir);
    store_global_full(dir, vars, &macros);
}

/// ★ 바인드 글로벌 층 + 치환 변수 글로벌 층(`DEFINE 이름 = 원문` 줄 · T-153 후속 · 10-27 141차 win)을 한 파일에 쓴다 —
/// 같은 스위치(`vars.global_persist`) · 같은 파일 · 그대로 실행해도 같은 결과. 원문에 줄바꿈이 있으면 그 변수는 건너뛴다(한 줄 명령).
pub fn store_global_full(dir: &Path, vars: &[VarState], macros: &[(String, String)]) {
    let p = global_path(dir);
    let mut text = crate::vars_to_script(vars);
    for (name, raw) in macros {
        if raw.contains('\n') || raw.contains('\r') || name.is_empty() {
            continue;
        }
        // ★ 값은 늘 `'…'`로 감싼다(`'` → `''`) — 치환 값의 앞뒤 공백을 지키기 위해(협업 V1 10-27 `' x '` → `x`로 깎이던 결함).
        //   DEFINE 명령은 감싼 따옴표를 벗기므로 그대로 실행해도 같은 값.
        text.push_str(&format!("DEFINE {name} = '{}'\n", raw.replace('\'', "''")));
    }
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

/// ★ 치환 변수 글로벌 층을 읽는다(`DEFINE 이름 = 원문` 줄만 · 없거나 너무 크면 빈 목록) — 바인드 줄(`VAR`/`EXEC`)은 [`load_global`]이 읽는다.
#[must_use]
pub fn load_global_macros(dir: &Path) -> Vec<(String, String)> {
    let p = global_path(dir);
    match std::fs::metadata(&p) {
        Ok(m) if m.len() > 0 && m.len() <= MAX_BYTES => {}
        _ => return Vec::new(),
    }
    std::fs::read_to_string(&p)
        .map(|t| macros_from_script(&t))
        .unwrap_or_default()
}

/// `DEFINE 이름 = 원문` 줄 → (이름, 원문) 목록(줄 순서 · 같은 이름은 뒤가 이긴다 · `DEF` 동의어 · 값이 없는 줄(조회·층 이동)은 무시).
#[must_use]
pub fn macros_from_script(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        let Some((head, rest)) = l.split_once(char::is_whitespace) else {
            continue;
        };
        if !(head.eq_ignore_ascii_case("DEFINE") || head.eq_ignore_ascii_case("DEF")) {
            continue;
        }
        let Some((name, raw)) = rest.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_start_matches('&');
        if name.is_empty()
            || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
            || name.starts_with(|c: char| c.is_ascii_digit())
        {
            continue;
        }
        out.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        out.push((name.to_string(), unquote_macro(raw)));
    }
    out
}

/// `'…'`로 감싼 값은 벗기고 `''` → `'`(공백 보존) · 감싸지 않은 값(사람이 쓴 줄)은 양끝 공백만 깎는다.
fn unquote_macro(raw: &str) -> String {
    let t = raw.trim();
    if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        t[1..t.len() - 1].replace("''", "'")
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Value, VarType};

    /// ★ 치환 변수 글로벌 층(T-153 후속): 바인드와 한 파일에 `DEFINE` 줄로 · 바인드만 다시 써도 DEFINE 줄은 남는다 · 바인드 읽기는 DEFINE 줄을 무시한다 ·
    /// 줄바꿈이 든 원문은 건너뛴다 · 둘 다 비면 파일 삭제.
    #[test]
    fn global_macros_share_the_file_and_survive_bind_rewrite() {
        let dir = std::env::temp_dir().join(format!("nsql_globalmacros_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let vars = vec![VarState {
            name: "B".into(),
            ty: VarType::Number,
            value: Value::Decimal("1".into()),
            declared: true,
            secret: false,
            layer: Layer::Global,
            formula: None,
            stale: false,
        }];
        let macros = vec![
            ("PROJECT".to_string(), "M4PLAN".to_string()),
            ("ML".to_string(), "a\nb".to_string()),
            ("OWNER".to_string(), "scott = tiger".to_string()),
            // ★ 앞뒤 공백 · 따옴표가 든 값도 그대로 돌아온다(협업 V1 10-27 결함).
            ("TS".to_string(), " x ".to_string()),
            ("Q".to_string(), "it's".to_string()),
        ];
        store_global_full(&dir, &vars, &macros);
        let text = std::fs::read_to_string(global_path(&dir)).unwrap_or_default();
        assert!(text.contains("DEFINE TS = ' x '\n"), "{text}");
        assert!(text.contains("DEFINE Q = 'it''s'\n"), "{text}");
        assert_eq!(
            load_global_macros(&dir),
            vec![
                ("PROJECT".to_string(), "M4PLAN".to_string()),
                ("OWNER".to_string(), "scott = tiger".to_string()),
                ("TS".to_string(), " x ".to_string()),
                ("Q".to_string(), "it's".to_string()),
            ]
        );
        // 그대로 실행해도 같은 값(DEFINE 명령이 따옴표를 벗긴다).
        let mut e = crate::engine::Engine::new(nsql_core::Dialect::Oracle);
        for item in crate::split::split_script(&text) {
            let _ = e.plan(&item);
        }
        assert_eq!(e.defines.get("TS").map(String::as_str), Some(" x "));
        assert_eq!(e.defines.get("Q").map(String::as_str), Some("it's"));
        let binds = load_global(&dir);
        assert_eq!(
            binds.len(),
            1,
            "DEFINE 줄은 바인드 읽기에서 무시: {binds:?}"
        );
        // 바인드만 아는 호출자(CLI 종전 경로)가 다시 써도 DEFINE 줄은 보존.
        store_global(&dir, &[]);
        assert_eq!(load_global(&dir).len(), 0);
        assert_eq!(load_global_macros(&dir).len(), 4);
        store_global_full(&dir, &[], &[]);
        assert!(!global_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `DEFINE` 줄 파서: `DEF`·`&` 접두·대소문자 · 값 없는 줄(조회 `DEFINE x` · 층 이동 `DEFINE x SHARE`)은 건너뜀 · 같은 이름은 뒤가 이긴다.
    #[test]
    fn macros_from_script_rules() {
        let got = macros_from_script(
            "VAR B NUMBER\ndefine A = 1\nDEF &A = 2\nDEFINE X\nDEFINE X SHARE\n  DEFINE  C=  'q'  \nDEFINE 1bad = v\nDEFINE D = ' a ''b'' '\n",
        );
        assert_eq!(
            got,
            vec![
                ("A".to_string(), "2".to_string()),
                ("C".to_string(), "q".to_string()),
                ("D".to_string(), " a 'b' ".to_string())
            ]
        );
    }

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
            formula: None,
            stale: false,
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
