//! ★ T-76 ② 스키마 힌트(10-30 · 사용자 결정): 내장 편집기에서 연 `settings.json`의 캐럿 자리를 읽어(키 자리 / 값 자리 · 둘러싼
//! 객체 경로) 설정 레지스트리에서 후보를 만든다 — 키 = 현재 객체 경로의 **다음 조각**(`editor` 안이면 `tab_size`·`minimap` …) ·
//! 값 = 그 키의 허용값(Bool = true/false · 선택지 · 그 밖 = 기본값). 순수 함수(시험) · 팝업은 `Intel::request_external`이 연다.
//!
//! settings.json은 키를 `.`로 나눈 **중첩 객체**(nsql-settings `json.rs`)라 경로 = 열린 `{`마다 그 앞 키를 쌓은 것. 주석(`//` · `/* */`)은 건너뛴다.

use nsql_i18n::t;
use nsql_script::intel::{Cand, CandKind};
use nsql_settings::{entries_all, SettingKind};
use std::ops::Range;

/// 캐럿 자리.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    /// 키 자리 — `path` = 둘러싼 객체 경로 · `prefix` = 지금까지 친 글자 · `replace` = 바꿀 바이트 구간 · `quoted` = 따옴표 안인가.
    Key {
        path: Vec<String>,
        prefix: String,
        replace: Range<usize>,
        quoted: bool,
    },
    /// 값 자리 — `key` = 전체 키(`editor.tab_size`).
    Value {
        key: String,
        prefix: String,
        replace: Range<usize>,
        quoted: bool,
    },
}

impl Site {
    pub(crate) fn prefix(&self) -> &str {
        match self {
            Site::Key { prefix, .. } | Site::Value { prefix, .. } => prefix,
        }
    }
    pub(crate) fn replace(&self) -> Range<usize> {
        match self {
            Site::Key { replace, .. } | Site::Value { replace, .. } => replace.clone(),
        }
    }
    fn quoted(&self) -> bool {
        match self {
            Site::Key { quoted, .. } | Site::Value { quoted, .. } => *quoted,
        }
    }
}

/// 문자열 `"` 다음 `from`부터 닫는 따옴표까지 — (내용 끝 인덱스, 닫혔는가). 역슬래시 이스케이프를 건너뛴다.
fn scan_string(b: &[u8], from: usize) -> (usize, bool) {
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return (i, true),
            _ => i += 1,
        }
    }
    (b.len(), false)
}

/// 캐럿(바이트)의 자리. 키/값 자리가 아니면(배열 안 · 객체 밖 · 값 뒤) `None`.
pub(crate) fn site_at(text: &str, caret: usize) -> Option<Site> {
    let b = text.as_bytes();
    let caret = caret.min(b.len());
    let mut path: Vec<String> = Vec::new();
    // 열린 `{`마다 경로를 밀었는가(루트 `{`·키 없는 `{`는 안 민다).
    let mut pushed: Vec<bool> = Vec::new();
    let mut cur_key: Option<String> = None;
    let mut after_colon = false;
    let mut expect_key = false;
    let mut in_array = 0usize;
    let mut i = 0usize;
    while i < caret {
        let c = b[i];
        match c {
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                continue;
            }
            b'"' => {
                let (end, closed) = scan_string(b, i + 1);
                if caret <= end || !closed {
                    // 캐럿이 이 문자열 안.
                    let prefix = text.get(i + 1..caret).unwrap_or("").to_string();
                    let replace = (i + 1)..end;
                    if in_array > 0 {
                        return None;
                    }
                    return Some(if after_colon {
                        Site::Value {
                            key: full_key(&path, cur_key.as_deref()),
                            prefix,
                            replace,
                            quoted: true,
                        }
                    } else {
                        Site::Key {
                            path,
                            prefix,
                            replace,
                            quoted: true,
                        }
                    });
                }
                let s = text.get(i + 1..end).unwrap_or("").to_string();
                if after_colon {
                    after_colon = false;
                    expect_key = false;
                } else if in_array == 0 {
                    cur_key = Some(s);
                    expect_key = false;
                }
                i = end + 1;
                continue;
            }
            b':' => after_colon = true,
            b',' => {
                after_colon = false;
                if in_array == 0 {
                    cur_key = None;
                    expect_key = true;
                }
            }
            b'{' => {
                let push = after_colon && cur_key.is_some() && in_array == 0;
                if push {
                    if let Some(k) = cur_key.take() {
                        path.push(k);
                    }
                }
                pushed.push(push);
                after_colon = false;
                cur_key = None;
                expect_key = true;
            }
            b'}' => {
                if pushed.pop().unwrap_or(false) {
                    path.pop();
                }
                after_colon = false;
                cur_key = None;
                expect_key = false;
            }
            b'[' => in_array += 1,
            b']' => in_array = in_array.saturating_sub(1),
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => {
                // 따옴표 없는 값(true · 12 · null): 캐럿이 그 안이면 값 자리.
                if after_colon && in_array == 0 {
                    let start = i;
                    while i < b.len()
                        && !matches!(b[i], b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
                    {
                        i += 1;
                    }
                    if caret <= i {
                        return Some(Site::Value {
                            key: full_key(&path, cur_key.as_deref()),
                            prefix: text.get(start..caret).unwrap_or("").to_string(),
                            replace: start..i,
                            quoted: false,
                        });
                    }
                    after_colon = false;
                    expect_key = false;
                    continue;
                }
            }
        }
        i += 1;
    }
    if in_array > 0 {
        return None;
    }
    if after_colon {
        return Some(Site::Value {
            key: full_key(&path, cur_key.as_deref()),
            prefix: String::new(),
            replace: caret..caret,
            quoted: false,
        });
    }
    if expect_key || (pushed.is_empty() && path.is_empty() && cur_key.is_none() && caret == 0) {
        return Some(Site::Key {
            path,
            prefix: String::new(),
            replace: caret..caret,
            quoted: false,
        });
    }
    None
}

fn full_key(path: &[String], last: Option<&str>) -> String {
    let mut k = path.join(".");
    if let Some(l) = last {
        if !k.is_empty() {
            k.push('.');
        }
        k.push_str(l);
    }
    k
}

/// ★ T-76 ②(10-31) 상세 카드 꼬리표: 키 후보 = `3<<56 | 항목 index` · 값 후보 = `4<<56 | 항목 index<<16 | 값 index`(0xFFFF = 값 index 없음) ·
///   index = `entries_all()` 순서(같은 세션 안에서 고정) · 카드는 `intel_card::Target::Setting`.
pub(crate) const TAG_SETTING_KEY: u64 = 3 << 56;
pub(crate) const TAG_SETTING_VALUE: u64 = 4 << 56;

fn cand(text: String, detail: String, order: u32, tag: u64) -> Cand {
    Cand {
        text,
        kind: CandKind::Keyword,
        detail,
        source: 5,
        tag,
        mark: String::new(),
        order,
        qualifier: String::new(),
        layer: 0,
    }
}

/// 자리에 맞는 후보 — 키 = 경로의 다음 조각(중복 없음 · 끝 조각이면 라벨을 설명으로 · 중간 조각이면 `…`) · 값 = Bool true/false ·
/// 선택지(값 글자) · 그 밖 = 기본값. 따옴표 밖이면 키·글자 값에 `"…"`를 붙여 넣는다(Bool·숫자는 그대로).
pub(crate) fn candidates(site: &Site) -> Vec<Cand> {
    let quoted = site.quoted();
    let q = |s: &str| {
        if quoted {
            s.to_string()
        } else {
            format!("\"{s}\"")
        }
    };
    match site {
        Site::Key { path, prefix, .. } => {
            // ★ 평키 보강(10-31 협업 관찰): 접두에 `.`가 있으면(`"editor.tab_"`) 점 앞은 경로로 보고 말단만 맞춘다 · 후보 글은 전체 평키
            //   (`editor.tab_size`)로 돌려주어 교체 구간(접두 전체)과 맞게.
            let (extra, head): (Vec<String>, String) = match prefix.rsplit_once('.') {
                Some((a, _)) => (a.split('.').map(str::to_string).collect(), format!("{a}.")),
                None => (Vec::new(), String::new()),
            };
            let mut path: Vec<String> = path.clone();
            path.extend(extra);
            let mut out: Vec<Cand> = Vec::new();
            for (idx, e) in entries_all().into_iter().enumerate() {
                if nsql_settings::is_internal(e.key) {
                    continue;
                }
                let segs: Vec<&str> = e.key.split('.').collect();
                if segs.len() <= path.len() || !segs.iter().zip(path.iter()).all(|(a, b)| a == b) {
                    continue;
                }
                let next = format!("{head}{}", segs[path.len()]);
                let next = next.as_str();
                if out.iter().any(|c| c.text == q(next)) {
                    continue;
                }
                let leaf = segs.len() == path.len() + 1;
                let detail = if leaf {
                    t(e.label).to_string()
                } else {
                    "…".to_string()
                };
                let n = out.len() as u32;
                let tag = if leaf {
                    TAG_SETTING_KEY | idx as u64
                } else {
                    0
                };
                out.push(cand(q(next), detail, n, tag));
            }
            out
        }
        Site::Value { key, .. } => {
            let Some((idx, e)) = entries_all()
                .into_iter()
                .enumerate()
                .find(|(_, x)| x.key == key)
            else {
                return Vec::new();
            };
            let vtag = |vi: u64| TAG_SETTING_VALUE | ((idx as u64) << 16) | vi;
            let mut out: Vec<Cand> = Vec::new();
            match e.kind {
                SettingKind::Bool => {
                    for (i, v) in ["true", "false"].iter().enumerate() {
                        let is_default = (e.default == "on") == (*v == "true");
                        out.push(cand(
                            (*v).to_string(),
                            if is_default {
                                t(e.label).to_string()
                            } else {
                                String::new()
                            },
                            i as u32,
                            vtag(0xFFFF),
                        ));
                    }
                }
                SettingKind::Choice(list) => {
                    for (i, (v, label)) in list.iter().enumerate() {
                        out.push(cand(q(v), t(*label).to_string(), i as u32, vtag(i as u64)));
                    }
                }
                SettingKind::Int { min, max } | SettingKind::Size { min, max } => {
                    out.push(cand(
                        e.default.to_string(),
                        format!("{min}~{max}"),
                        0,
                        vtag(0xFFFF),
                    ));
                }
                _ => {
                    out.push(cand(q(e.default), t(e.label).to_string(), 0, vtag(0xFFFF)));
                }
            }
            out
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn key(site: &Site) -> (Vec<String>, String, bool) {
        match site {
            Site::Key {
                path,
                prefix,
                quoted,
                ..
            } => (path.clone(), prefix.clone(), *quoted),
            other => panic!("키 자리가 아님: {other:?}"),
        }
    }

    /// 키 자리: 루트 · 중첩 경로 · 따옴표 안 접두 · 콤마 뒤 · 값 자리: 콜론 뒤(빈 값 · 따옴표 안 · 숫자 안) · 배열 안 = None · 주석 건너뜀.
    #[test]
    fn site_detection() {
        let s = r#"{"editor": {"tab_"#;
        let site = site_at(s, s.len()).unwrap();
        assert_eq!(key(&site), (vec!["editor".into()], "tab_".into(), true));
        assert_eq!(site.replace(), (s.len() - 4)..s.len());
        let s = r#"{"editor": {"tab_size": "#;
        match site_at(s, s.len()).unwrap() {
            Site::Value { key, quoted, .. } => {
                assert_eq!(key, "editor.tab_size");
                assert!(!quoted);
            }
            o => panic!("{o:?}"),
        }
        let s = r#"{"ui": {"theme": "da"}}"#;
        // 캐럿 19 = `"d|a"`(따옴표 17 · d 18 · a 19) → 접두 "d" · 바꿀 구간 = 문자열 내용 18..20.
        match site_at(s, 19).unwrap() {
            Site::Value {
                key,
                prefix,
                quoted,
                replace,
                ..
            } => {
                assert_eq!(key, "ui.theme");
                assert_eq!(prefix, "d");
                assert!(quoted);
                assert_eq!(replace, 18..20);
            }
            o => panic!("{o:?}"),
        }
        let s = r#"{"ui": {"theme": "dark"}, "#;
        assert_eq!(
            key(&site_at(s, s.len()).unwrap()),
            (vec![], String::new(), false)
        );
        let s = r#"{"editor": {"tab_size": 12"#;
        match site_at(s, s.len()).unwrap() {
            Site::Value {
                key,
                prefix,
                quoted,
                ..
            } => {
                assert_eq!(key, "editor.tab_size");
                assert_eq!(prefix, "12");
                assert!(!quoted);
            }
            o => panic!("{o:?}"),
        }
        let s = r#"{"a": [1, "#;
        assert_eq!(site_at(s, s.len()), None);
        let s = "{ // 주석 \"x\"\n  \"edi";
        assert_eq!(
            key(&site_at(s, s.len()).unwrap()),
            (vec![], "edi".into(), true)
        );
        assert_eq!(
            key(&site_at("", 0).unwrap()),
            (vec![], String::new(), false)
        );
    }

    /// 후보: 루트 = `editor`·`ui` 같은 첫 조각(중복 없음 · `…`) · `editor` 안 = `tab_size`(라벨) · 값 = Bool true/false · Int 기본값 ·
    /// 선택지 · 따옴표 밖이면 `"…"`.
    #[test]
    fn candidates_from_registry() {
        let root = candidates(&Site::Key {
            path: vec![],
            prefix: String::new(),
            replace: 0..0,
            quoted: true,
        });
        assert!(root.iter().any(|c| c.text == "editor" && c.detail == "…"));
        assert_eq!(root.iter().filter(|c| c.text == "editor").count(), 1);
        let ed = candidates(&Site::Key {
            path: vec!["editor".into()],
            prefix: String::new(),
            replace: 0..0,
            quoted: false,
        });
        let ts = ed
            .iter()
            .find(|c| c.text == "\"tab_size\"")
            .expect("tab_size");
        assert!(!ts.detail.is_empty() && ts.detail != "…");
        let v = candidates(&Site::Value {
            key: "editor.minimap".into(),
            prefix: String::new(),
            replace: 0..0,
            quoted: false,
        });
        assert_eq!(
            v.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["true", "false"]
        );
        let v = candidates(&Site::Value {
            key: "editor.tab_size".into(),
            prefix: String::new(),
            replace: 0..0,
            quoted: false,
        });
        assert_eq!(v[0].text, "4");
        let v = candidates(&Site::Value {
            key: "ui.theme".into(),
            prefix: String::new(),
            replace: 0..0,
            quoted: false,
        });
        assert!(v.len() >= 2 && v.iter().all(|c| c.text.starts_with('"')));
        assert!(candidates(&Site::Value {
            key: "no.such".into(),
            prefix: String::new(),
            replace: 0..0,
            quoted: true,
        })
        .is_empty());
    }

    /// ★ T-76 ②: 카드 꼬리표 — 키 후보(말단) = `TAG_SETTING_KEY | index` · 중간 경로(`…`) = 0 · 값 후보 = `TAG_SETTING_VALUE` + 항목·값 index.
    #[test]
    fn candidates_carry_setting_tags() {
        let s = r#"{"editor": {"tab_"#;
        let site = site_at(s, s.len()).unwrap();
        let cands = candidates(&site);
        let tab = cands
            .iter()
            .find(|c| c.text == "tab_size")
            .expect("editor.tab_size 후보");
        assert_eq!(tab.tag >> 56, 3);
        let idx = (tab.tag & 0xFF_FFFF_FFFF_FFFF) as usize;
        assert_eq!(entries_all()[idx].key, "editor.tab_size");
        let s2 = r#"{"format": {"comma": "#;
        let site2 = site_at(s2, s2.len()).unwrap();
        let vals = candidates(&site2);
        assert!(vals.iter().all(|c| c.tag >> 56 == 4), "{vals:?}");
        let lead = vals
            .iter()
            .find(|c| c.text.contains("leading"))
            .expect("leading");
        assert_eq!(lead.tag & 0xFFFF, 0, "첫 선택지 index");
        assert_eq!(
            entries_all()[((lead.tag >> 16) & 0xFF_FFFF_FFFF) as usize].key,
            "format.comma"
        );
    }

    /// ★ 평키 접두(`"editor.tab_"`) = 점 앞을 경로로 · 후보 글은 전체 평키 · 교체 구간은 접두 전체.
    #[test]
    fn flat_key_prefix_completes_full_key() {
        let s = r#"{"editor.tab_"#;
        let site = site_at(s, s.len()).unwrap();
        assert_eq!(site.prefix(), "editor.tab_");
        let cands = candidates(&site);
        assert!(
            cands.iter().any(|c| c.text == "editor.tab_size"),
            "{cands:?}"
        );
        assert!(cands.iter().all(|c| c.text.starts_with("editor.")));
    }
}
