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

// ───────────────────────── ★ T-76 ③(10-31): `.sublime-keymap` 힌트 ─────────────────────────

/// 키맵 파일(바인딩 **배열**)의 캐럿 자리 — 바인딩 객체의 키(`keys`·`command`·`args`·`context`) · `context` 배열 안 객체의 키(`key`·`operator`·
/// `operand`·`match_all`) · 값(`command` · `context.key` · `context.operator` · `keys` 배열 원소). `args` 안·그 밖 = None.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KmSite {
    Key {
        /// context 객체 안인가(아니면 바인딩 객체).
        ctx: bool,
        prefix: String,
        replace: Range<usize>,
        quoted: bool,
    },
    Value {
        /// `command` · `context.key` · `context.operator` · `keys`.
        key: String,
        prefix: String,
        replace: Range<usize>,
        quoted: bool,
    },
}

impl KmSite {
    pub(crate) fn prefix(&self) -> &str {
        match self {
            KmSite::Key { prefix, .. } | KmSite::Value { prefix, .. } => prefix,
        }
    }
    pub(crate) fn replace(&self) -> Range<usize> {
        match self {
            KmSite::Key { replace, .. } | KmSite::Value { replace, .. } => replace.clone(),
        }
    }
    fn quoted(&self) -> bool {
        match self {
            KmSite::Key { quoted, .. } | KmSite::Value { quoted, .. } => *quoted,
        }
    }
}

/// 둘러싼 컨테이너(스캐너 스택).
#[derive(Clone, Debug)]
enum Frame {
    /// 배열 · `key` = 이 배열이 매달린 키(루트 = None).
    Arr { key: Option<String> },
    /// 객체 · `parent` = 이 객체가 든 배열의 키(루트 배열 = None · `context` · `args`) · `cur_key` = 지금 키 · `after_colon`/`expect_key`.
    Obj {
        parent: Option<String>,
        cur_key: Option<String>,
        after_colon: bool,
        expect_key: bool,
    },
}

/// 바인딩 객체/컨텍스트 객체의 값 자리 이름(`command` · `context.key` · `context.operator`) — 그 밖(`args` 안 · `operand` · `match_all`)은 None.
fn km_value_key(parent: Option<&str>, cur_key: Option<&str>) -> Option<String> {
    match (parent, cur_key?) {
        (None, "command") => Some("command".into()),
        (Some("context"), "key") => Some("context.key".into()),
        (Some("context"), "operator") => Some("context.operator".into()),
        _ => None,
    }
}

pub(crate) fn keymap_site_at(text: &str, caret: usize) -> Option<KmSite> {
    let b = text.as_bytes();
    let caret = caret.min(b.len());
    let mut stack: Vec<Frame> = Vec::new();
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
                    let prefix = text.get(i + 1..caret).unwrap_or("").to_string();
                    let replace = (i + 1)..end;
                    return match stack.last() {
                        Some(Frame::Obj {
                            parent,
                            after_colon: false,
                            ..
                        }) => match parent.as_deref() {
                            None => Some(KmSite::Key {
                                ctx: false,
                                prefix,
                                replace,
                                quoted: true,
                            }),
                            Some("context") => Some(KmSite::Key {
                                ctx: true,
                                prefix,
                                replace,
                                quoted: true,
                            }),
                            _ => None,
                        },
                        Some(Frame::Obj {
                            parent,
                            cur_key,
                            after_colon: true,
                            ..
                        }) => km_value_key(parent.as_deref(), cur_key.as_deref()).map(|key| {
                            KmSite::Value {
                                key,
                                prefix,
                                replace,
                                quoted: true,
                            }
                        }),
                        Some(Frame::Arr { key: Some(k) }) if k == "keys" => Some(KmSite::Value {
                            key: "keys".into(),
                            prefix,
                            replace,
                            quoted: true,
                        }),
                        _ => None,
                    };
                }
                let s = text.get(i + 1..end).unwrap_or("").to_string();
                if let Some(Frame::Obj {
                    cur_key,
                    after_colon,
                    expect_key,
                    ..
                }) = stack.last_mut()
                {
                    if *after_colon {
                        *after_colon = false;
                    } else {
                        *cur_key = Some(s);
                    }
                    *expect_key = false;
                }
                i = end + 1;
                continue;
            }
            b':' => {
                if let Some(Frame::Obj { after_colon, .. }) = stack.last_mut() {
                    *after_colon = true;
                }
            }
            b',' => {
                if let Some(Frame::Obj {
                    cur_key,
                    after_colon,
                    expect_key,
                    ..
                }) = stack.last_mut()
                {
                    *cur_key = None;
                    *after_colon = false;
                    *expect_key = true;
                }
            }
            b'{' => {
                let parent = match stack.last() {
                    Some(Frame::Arr { key }) => key.clone(),
                    // 배열 밖의 객체(키 값으로 바로 온 객체 = `args`) = 그 키.
                    Some(Frame::Obj { cur_key, .. }) => cur_key.clone().or(Some("?".into())),
                    None => Some("?".into()),
                };
                stack.push(Frame::Obj {
                    parent,
                    cur_key: None,
                    after_colon: false,
                    expect_key: true,
                });
            }
            b'[' => {
                let key = match stack.last() {
                    Some(Frame::Obj {
                        cur_key,
                        after_colon: true,
                        ..
                    }) => cur_key.clone(),
                    _ => None,
                };
                stack.push(Frame::Arr { key });
            }
            b'}' | b']' => {
                stack.pop();
                if let Some(Frame::Obj {
                    after_colon,
                    expect_key,
                    ..
                }) = stack.last_mut()
                {
                    *after_colon = false;
                    *expect_key = false;
                }
            }
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => {
                // 따옴표 없는 값(true · 숫자) = 지나간다.
                if let Some(Frame::Obj {
                    after_colon,
                    expect_key,
                    ..
                }) = stack.last_mut()
                {
                    *after_colon = false;
                    *expect_key = false;
                }
            }
        }
        i += 1;
    }
    // 문자열 밖(따옴표를 아직 안 쳤다).
    match stack.last() {
        Some(Frame::Obj {
            parent,
            expect_key: true,
            after_colon: false,
            ..
        }) => match parent.as_deref() {
            None => Some(KmSite::Key {
                ctx: false,
                prefix: String::new(),
                replace: caret..caret,
                quoted: false,
            }),
            Some("context") => Some(KmSite::Key {
                ctx: true,
                prefix: String::new(),
                replace: caret..caret,
                quoted: false,
            }),
            _ => None,
        },
        Some(Frame::Obj {
            parent,
            cur_key,
            after_colon: true,
            ..
        }) => km_value_key(parent.as_deref(), cur_key.as_deref()).map(|key| KmSite::Value {
            key,
            prefix: String::new(),
            replace: caret..caret,
            quoted: false,
        }),
        Some(Frame::Arr { key: Some(k) }) if k == "keys" => Some(KmSite::Value {
            key: "keys".into(),
            prefix: String::new(),
            replace: caret..caret,
            quoted: false,
        }),
        _ => None,
    }
}

/// context 조건 키(`app/input.rs` `ContextSource for App`와 같은 목록 · `setting.` 접두는 글자 그대로).
pub(crate) const KEYMAP_CONTEXT_KEYS: &[&str] = &[
    "selector",
    "selection_empty",
    "has_selection",
    "num_selections",
    "preceding_text",
    "following_text",
    "text",
    "read_only",
    "focus",
    "panel",
    "panel_visible",
    "panel_has_focus",
    "overlay_visible",
    "auto_complete_visible",
    "connected",
    "dialect",
    "setting.",
];
const KEYMAP_OPERATORS: &[&str] = &[
    "equal",
    "not_equal",
    "regex_match",
    "not_regex_match",
    "regex_contains",
    "not_regex_contains",
];
const KEYMAP_MODIFIERS: &[&str] = &["ctrl+", "shift+", "alt+", "super+"];
const KEYMAP_KEY_NAMES: &[&str] = &[
    "enter",
    "tab",
    "space",
    "escape",
    "backspace",
    "delete",
    "insert",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
];

/// 키맵 자리의 후보 — `commands` = (명령 id, 라벨 "메뉴: 항목") 호스트가 팔레트 표에서. 따옴표 밖이면 `"…"`를 붙인다.
pub(crate) fn keymap_candidates(site: &KmSite, commands: &[(String, String)]) -> Vec<Cand> {
    let quoted = site.quoted();
    let q = |s: &str| {
        if quoted {
            s.to_string()
        } else {
            format!("\"{s}\"")
        }
    };
    let plain = |list: &[&str]| -> Vec<Cand> {
        list.iter()
            .enumerate()
            .map(|(i, k)| cand(q(k), String::new(), i as u32, 0))
            .collect()
    };
    match site {
        KmSite::Key { ctx: false, .. } => plain(&["keys", "command", "args", "context"]),
        KmSite::Key { ctx: true, .. } => plain(&["key", "operator", "operand", "match_all"]),
        KmSite::Value { key, prefix, .. } => match key.as_str() {
            "command" => commands
                .iter()
                .enumerate()
                .map(|(i, (id, label))| cand(q(id), label.clone(), i as u32, 0))
                .collect(),
            "context.key" => plain(KEYMAP_CONTEXT_KEYS),
            "context.operator" => plain(KEYMAP_OPERATORS),
            "keys" => {
                // `ctrl+shift+` 뒤 = 키 이름만(앞 조각은 그대로 붙여 교체 구간과 맞춤) · 처음 = 수식키 + 키 이름.
                let head = prefix.rfind('+').map_or("", |p| &prefix[..=p]);
                let mut out: Vec<String> = Vec::new();
                if head.is_empty() {
                    out.extend(KEYMAP_MODIFIERS.iter().map(|m| q(m)));
                } else {
                    out.extend(
                        KEYMAP_MODIFIERS
                            .iter()
                            .filter(|m| !head.contains(&m[..m.len() - 1]))
                            .map(|m| q(&format!("{head}{m}"))),
                    );
                }
                out.extend(KEYMAP_KEY_NAMES.iter().map(|k| q(&format!("{head}{k}"))));
                out.into_iter()
                    .enumerate()
                    .map(|(i, text)| cand(text, String::new(), i as u32, 0))
                    .collect()
            }
            _ => Vec::new(),
        },
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

    /// ★ T-76 ③(10-31): 키맵 파일 자리 — 바인딩 키(따옴표 안/밖) · `command` 값 · `keys` 원소(수식키 뒤 = 키 이름만) · context 객체 키/값 ·
    /// `args` 안·`operand` = None · 후보 조립(명령 라벨 · 조합 글 접두 유지).
    #[test]
    fn keymap_site_and_candidates() {
        let src = r#"[
  { "keys": ["ctrl+s"], "command": "file.save" },
  { "keys": ["ctrl+shift+"], "command": "run.", "args": {"x": "y"},
    "context": [ { "key": "sel", "operator": "equ", "operand": "editor" } ] },
  { "
]"#;
        let at = |needle: &str, back: usize| -> usize {
            src.find(needle).expect(needle) + needle.len() - back
        };
        // `"ctrl+shift+"` 안 끝 = keys 원소 · 접두 유지.
        let s = keymap_site_at(src, at("\"ctrl+shift+", 0)).expect("keys");
        assert!(
            matches!(&s, KmSite::Value { key, prefix, quoted: true, .. } if key == "keys" && prefix == "ctrl+shift+")
        );
        let c = keymap_candidates(&s, &[]);
        assert!(
            c.iter().any(|c| c.text == "ctrl+shift+f5"),
            "{:?}",
            c.iter().map(|c| &c.text).collect::<Vec<_>>()
        );
        assert!(
            c.iter().any(|c| c.text == "ctrl+shift+alt+"),
            "남은 수식키만"
        );
        assert!(!c.iter().any(|c| c.text == "ctrl+shift+ctrl+"));
        // `"run."` 안 = command 값 · 명령 후보 라벨.
        let s = keymap_site_at(src, at("\"run.", 0)).expect("command");
        assert!(
            matches!(&s, KmSite::Value { key, prefix, .. } if key == "command" && prefix == "run.")
        );
        let cmds = vec![("run.statement".to_string(), "Run: Statement".to_string())];
        let c = keymap_candidates(&s, &cmds);
        assert_eq!(
            (c[0].text.as_str(), c[0].detail.as_str()),
            ("run.statement", "Run: Statement")
        );
        // args 안 = None.
        assert_eq!(keymap_site_at(src, at("\"x\": \"y", 0)), None);
        // context 객체: key 값 · operator 값 · operand = None · 객체 키.
        let s = keymap_site_at(src, at("\"sel", 0)).expect("ctx key");
        assert!(matches!(&s, KmSite::Value { key, .. } if key == "context.key"));
        assert!(keymap_candidates(&s, &[])
            .iter()
            .any(|c| c.text == "preceding_text"));
        let s = keymap_site_at(src, at("\"equ", 0)).expect("op");
        assert!(matches!(&s, KmSite::Value { key, .. } if key == "context.operator"));
        assert_eq!(keymap_site_at(src, at("\"editor", 0)), None, "operand");
        let s = keymap_site_at(src, at("\"operator", 0)).expect("ctx obj key");
        assert!(matches!(&s, KmSite::Key { ctx: true, prefix, .. } if prefix == "operator"));
        // 새 바인딩 객체의 첫 키(따옴표 안 · 빈 접두) · 따옴표 밖(`{ ` 뒤) = 후보에 따옴표를 붙인다.
        let s = keymap_site_at(src, at("{ \"\n]", 2)).expect("new key");
        assert!(matches!(
            &s,
            KmSite::Key {
                ctx: false,
                quoted: true,
                ..
            }
        ));
        let s = keymap_site_at(src, at("{ \"\n]", 3)).expect("new key unquoted");
        assert!(matches!(
            &s,
            KmSite::Key {
                ctx: false,
                quoted: false,
                ..
            }
        ));
        let c = keymap_candidates(&s, &[]);
        assert_eq!(
            c.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["\"keys\"", "\"command\"", "\"args\"", "\"context\""]
        );
    }
}
