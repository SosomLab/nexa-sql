//! **다중 인스턴스 프로젝트 동기화 — 순수 계획**(docs/67 §9 · 사용자 11-19).
//!
//! 같은 `.nsql-project`를 두 인스턴스가 열고 있을 때, 다른 인스턴스가 쓴 문서(`theirs`)를 우리 상태(`ours`)에 **항목 단위 3-way**로
//! 합친다 — 공통 조상(`base`) = 이 인스턴스가 마지막으로 읽거나 쓴 문서. 이 파일은 "무엇을 할지"만 정한다(순수 · 시험 대상):
//! 탭 본문을 실제로 합치는 일(`extfile::decide` · `merge3`) · 탭 열기/닫기 · 띠는 호스트(`app/project.rs`)가 한다.
//!
//! 탭의 열쇠 = 파일 탭은 **경로**, 그 밖(무제 · 뷰)은 **문서 id**(인스턴스가 달라도 같은 값 · `App::tab_doc_ids`).
//! 화면 상태(활성 탭 · 캐럿 · 패널 · 펼침 · 검색어)는 합치지 않는다 — 인스턴스마다 자기 것.

use crate::project::{Project, TabState};
use std::path::PathBuf;

/// 탭의 병합 열쇠.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TabKey {
    File(PathBuf),
    Id(u64),
}

/// 탭의 열쇠 — 경로도 id도 없는 탭(옛 파일의 id 0)은 맞출 수 없어 `None`.
pub(crate) fn key_of(t: &TabState) -> Option<TabKey> {
    match &t.path {
        Some(p) => Some(TabKey::File(p.clone())),
        None if t.id != 0 => Some(TabKey::Id(t.id)),
        None => None,
    }
}

/// 탭 하나에 할 일.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TabOp {
    /// 저쪽에서 새로 연 탭(또는 우리가 닫았는데 저쪽이 본문을 고친 탭) — 배경 탭으로 연다.
    Open(Box<TabState>),
    /// 저쪽에서 닫았고 우리는 건드리지 않은 탭 — 닫는다.
    Close(TabKey),
    /// 저쪽에서 본문이 바뀌었다 — `base` = 공통 조상 본문(`None` = 조상에 본문이 없었다: 파일 탭이면 디스크 기준) · `theirs` = 저쪽 본문.
    /// 우리 버퍼와의 3-way는 호스트가 한다.
    Body {
        key: TabKey,
        base: Option<String>,
        theirs: String,
    },
    /// 저쪽이 파일 탭의 미저장 편집을 내려놓았다(저장했거나 되돌림) — 우리 버퍼가 조상 본문(`base`) 그대로면 디스크 내용으로.
    Revert { key: TabKey, base: String },
}

/// 병합 계획.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    pub tabs: Vec<TabOp>,
    /// 합친 폴더 목록(우리 순서 + 저쪽 추가 − 저쪽 삭제).
    pub folders: Vec<PathBuf>,
    /// 합친 `settings` 절(같은 키를 양쪽이 바꿨으면 우리 것).
    pub settings: Vec<(Option<String>, String, String)>,
}

fn find<'a>(p: &'a Project, k: &TabKey) -> Option<&'a TabState> {
    p.tabs.iter().find(|t| key_of(t).as_ref() == Some(k))
}

/// 3-way 계획 — `base` = 공통 조상 · `ours` = 지금 우리 상태(방금 담은 것) · `theirs` = 디스크의 문서.
pub(crate) fn plan(base: &Project, ours: &Project, theirs: &Project) -> Plan {
    let mut tabs = Vec::new();
    for t in &theirs.tabs {
        let Some(k) = key_of(t) else { continue };
        let b = find(base, &k);
        match (b, find(ours, &k)) {
            // 양쪽에 다 있는 탭 — 본문만 본다(뷰 탭은 본문이 없다).
            (_, Some(_)) => {
                let bt = b.and_then(|b| b.text.as_ref());
                if t.view.is_some() || t.text.as_ref() == bt {
                    continue;
                }
                match &t.text {
                    Some(x) => tabs.push(TabOp::Body {
                        key: k,
                        base: bt.cloned(),
                        theirs: x.clone(),
                    }),
                    // 무제 탭의 본문 없음 = 상한 초과로 못 담은 것 — 할 일이 없다.
                    None => {
                        if let (true, Some(b)) = (t.path.is_some(), bt) {
                            tabs.push(TabOp::Revert {
                                key: k,
                                base: b.clone(),
                            });
                        }
                    }
                }
            }
            // 저쪽이 새로 연 탭 — 미리보기 탭(한 번 클릭)은 옮기지 않는다.
            (None, None) => {
                if !t.preview {
                    tabs.push(TabOp::Open(Box::new(t.clone())));
                }
            }
            // 우리가 닫은 탭 — 저쪽이 본문을 고쳤을 때만 다시 연다(우리가 저장할 때 저쪽 편집이 파일에서 빠지지 않게).
            (Some(b), None) => {
                if t.text.is_some() && t.text != b.text {
                    tabs.push(TabOp::Open(Box::new(t.clone())));
                }
            }
        }
    }
    // 저쪽이 닫은 탭 — 우리 본문이 조상과 같을 때만 닫는다. 본문을 못 담은 무제 탭(상한 초과)은 같다고 볼 수 없어 남긴다.
    for b in &base.tabs {
        let Some(k) = key_of(b) else { continue };
        if find(theirs, &k).is_some() {
            continue;
        }
        if let Some(o) = find(ours, &k) {
            let comparable = o.text.is_some() || o.path.is_some() || o.view.is_some();
            if comparable && o.text == b.text {
                tabs.push(TabOp::Close(k));
            }
        }
    }
    Plan {
        tabs,
        folders: merge_list(&base.folders, &ours.folders, &theirs.folders),
        settings: merge_settings(&base.settings, &ours.settings, &theirs.settings),
    }
}

/// 순서 있는 집합의 3-way: 우리 순서를 지키고 · 저쪽이 지운 것은 빼고 · 저쪽이 더한 것은 뒤에 붙인다.
fn merge_list<T: Clone + PartialEq>(base: &[T], ours: &[T], theirs: &[T]) -> Vec<T> {
    let mut out: Vec<T> = ours
        .iter()
        .filter(|x| !(base.contains(x) && !theirs.contains(x)))
        .cloned()
        .collect();
    for x in theirs {
        if !base.contains(x) && !out.contains(x) {
            out.push(x.clone());
        }
    }
    out
}

type Setting = (Option<String>, String, String);

fn setting<'a>(list: &'a [Setting], g: &Option<String>, k: &str) -> Option<&'a str> {
    list.iter()
        .find(|(lg, lk, _)| lg == g && lk == k)
        .map(|(_, _, v)| v.as_str())
}

/// `settings` 절의 3-way(열쇠 = 묶음 + 키): 저쪽만 바꾼 것은 받고 · 양쪽이 바꾼 것은 우리 것(나중에 쓰는 쪽).
fn merge_settings(base: &[Setting], ours: &[Setting], theirs: &[Setting]) -> Vec<Setting> {
    let mut out = Vec::new();
    for (g, k, v) in ours {
        let b = setting(base, g, k);
        let t = setting(theirs, g, k);
        if t != b && b == Some(v.as_str()) {
            // 우리는 그대로 · 저쪽이 바꿨거나 지웠다.
            if let Some(tv) = t {
                out.push((g.clone(), k.clone(), tv.to_string()));
            }
        } else {
            out.push((g.clone(), k.clone(), v.clone()));
        }
    }
    for (g, k, v) in theirs {
        // 저쪽이 새로 넣은 키(조상에 없고 우리에게도 없다) — 조상에 있던 키를 우리가 지운 것은 지운 채로.
        if setting(ours, g, k).is_none() && setting(base, g, k).is_none() {
            out.push((g.clone(), k.clone(), v.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn file(path: &str, text: Option<&str>) -> TabState {
        TabState {
            path: Some(PathBuf::from(path)),
            text: text.map(str::to_string),
            ..TabState::default()
        }
    }

    fn scratch(id: u64, text: &str) -> TabState {
        TabState {
            id,
            text: Some(text.to_string()),
            ..TabState::default()
        }
    }

    fn proj(tabs: Vec<TabState>) -> Project {
        Project {
            tabs,
            ..Project::default()
        }
    }

    #[test]
    fn unchanged_document_plans_nothing() {
        let p = proj(vec![file("a.sql", None), scratch(7, "select 1")]);
        assert!(plan(&p, &p, &p).tabs.is_empty());
    }

    /// 저쪽만 무제 탭 본문을 고쳤다 → 본문 반영(조상 본문 동봉).
    #[test]
    fn their_body_change_is_planned_with_base() {
        let base = proj(vec![scratch(7, "a")]);
        let theirs = proj(vec![scratch(7, "a\nb")]);
        assert_eq!(
            plan(&base, &base, &theirs).tabs,
            vec![TabOp::Body {
                key: TabKey::Id(7),
                base: Some("a".into()),
                theirs: "a\nb".into()
            }]
        );
        // 우리만 고쳤다 = 저쪽 본문이 조상과 같다 → 할 일 없음.
        let ours = proj(vec![scratch(7, "a\nmine")]);
        assert!(plan(&base, &ours, &base).tabs.is_empty());
    }

    /// 파일 탭: 저쪽이 미저장 편집을 새로 담았다(조상에는 본문 없음) → base None · 내려놓았다 → Revert.
    #[test]
    fn file_tab_dirty_and_revert() {
        let clean = proj(vec![file("a.sql", None)]);
        let dirty = proj(vec![file("a.sql", Some("x"))]);
        assert_eq!(
            plan(&clean, &clean, &dirty).tabs,
            vec![TabOp::Body {
                key: TabKey::File("a.sql".into()),
                base: None,
                theirs: "x".into()
            }]
        );
        assert_eq!(
            plan(&dirty, &dirty, &clean).tabs,
            vec![TabOp::Revert {
                key: TabKey::File("a.sql".into()),
                base: "x".into()
            }]
        );
    }

    /// 열기 전파: 저쪽의 새 탭은 연다 · 미리보기 탭은 옮기지 않는다 · 우리가 닫은 탭은 저쪽이 본문을 고쳤을 때만 다시 연다.
    #[test]
    fn open_propagation_rules() {
        let base = proj(vec![scratch(1, "a")]);
        let mut pv = file("p.sql", None);
        pv.preview = true;
        let theirs = proj(vec![scratch(1, "a"), scratch(2, "new"), pv]);
        assert_eq!(
            plan(&base, &base, &theirs).tabs,
            vec![TabOp::Open(Box::new(scratch(2, "new")))]
        );
        // 우리가 1번을 닫았다: 저쪽이 그대로면 닫힌 채 · 저쪽이 고쳤으면 다시 연다.
        let ours = proj(vec![]);
        assert!(plan(&base, &ours, &base).tabs.is_empty());
        let edited = proj(vec![scratch(1, "a2")]);
        assert_eq!(
            plan(&base, &ours, &edited).tabs,
            vec![TabOp::Open(Box::new(scratch(1, "a2")))]
        );
    }

    /// 닫기 전파: 우리가 건드리지 않았을 때만 · 본문을 못 담은 무제 탭은 남긴다.
    #[test]
    fn close_propagation_rules() {
        let base = proj(vec![scratch(1, "a"), file("f.sql", None)]);
        let theirs = proj(vec![]);
        assert_eq!(
            plan(&base, &base, &theirs).tabs,
            vec![
                TabOp::Close(TabKey::Id(1)),
                TabOp::Close(TabKey::File("f.sql".into()))
            ]
        );
        // 우리가 둘 다 고쳤다 → 닫지 않는다.
        let ours = proj(vec![scratch(1, "mine"), file("f.sql", Some("mine"))]);
        assert!(plan(&base, &ours, &theirs).tabs.is_empty());
        // 본문이 상한을 넘어 안 담긴 무제 탭(text None) — 조상도 None이어도 닫지 않는다.
        let big = TabState {
            id: 9,
            ..TabState::default()
        };
        let base = proj(vec![big]);
        assert!(plan(&base, &base, &theirs).tabs.is_empty());
    }

    #[test]
    fn folders_merge_keeps_our_order_and_applies_their_delta() {
        let p = |v: &[&str]| v.iter().map(PathBuf::from).collect::<Vec<_>>();
        // 조상 a,b · 우리는 c를 더함 · 저쪽은 a를 지우고 d를 더함.
        let out = merge_list(&p(&["a", "b"]), &p(&["a", "b", "c"]), &p(&["b", "d"]));
        assert_eq!(out, p(&["b", "c", "d"]));
    }

    #[test]
    fn settings_merge_three_way_ours_wins_on_conflict() {
        let s = |v: &[(&str, &str)]| -> Vec<Setting> {
            v.iter()
                .map(|(k, x)| (None, (*k).to_string(), (*x).to_string()))
                .collect()
        };
        let base = s(&[("a", "1"), ("b", "1"), ("c", "1"), ("d", "1")]);
        // 우리: b를 바꿈 · c를 바꿈 · d를 지움 · e를 더함.
        let ours = s(&[("a", "1"), ("b", "2"), ("c", "2"), ("e", "9")]);
        // 저쪽: a를 바꿈 · c를 바꿈(충돌) · d를 바꿈(우리는 지움) · f를 더함.
        let theirs = s(&[("a", "5"), ("b", "1"), ("c", "7"), ("d", "8"), ("f", "3")]);
        assert_eq!(
            merge_settings(&base, &ours, &theirs),
            s(&[("a", "5"), ("b", "2"), ("c", "2"), ("e", "9"), ("f", "3")])
        );
        // 저쪽이 지운 키는 우리가 안 바꿨으면 지운다.
        let theirs = s(&[("b", "1"), ("c", "1"), ("d", "1")]);
        assert_eq!(
            merge_settings(&base, &base, &theirs),
            s(&[("b", "1"), ("c", "1"), ("d", "1")])
        );
    }
}
