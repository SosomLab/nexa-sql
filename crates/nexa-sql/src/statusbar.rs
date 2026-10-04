//! 상태바 오른쪽 항목의 순서·표시(사용자 10-04 "상태바 위치와 사용 여부를 설정 화면에서" · nexa-dir3 순서 편집 창과 같은 방식).
//! 값 = 설정 `statusbar.layout`(nexa-ctl `order` 문법 · 빈 값 = 기본). 여기는 **정의와 순수 함수**만 — 그리기는 `app/paint.rs`,
//! 편집 창은 `order_win.rs`.
//!
//! ★ 위치 조정 = **그룹 단위**(사용자 10-04 "위치 조정은 그룹으로 묶어줘"): 서로 붙어 다니는 칸은 한 그룹(블록)의 자식이다 —
//! 그룹을 옮기면 통째로 가고, 자식은 그룹 안에서만 순서를 바꾼다. 그룹을 숨기면 자식 전부가 숨는다(자식 체크는 보존).

use nexa_ctl::order::{self, Hidden, OrderBlock, OrderDefs};
use nsql_i18n::Msg;

/// 설정 키.
pub(crate) const KEY: &str = "statusbar.layout";

/// 그룹(블록)과 그 안의 칸(자식) — 정의 순 = 기본 표시 순서(왼쪽 → 오른쪽). 자식 없는 블록 = 그 자체가 칸 하나.
/// 칸이 실제로 나오는지는 그때의 상태가 정한다(예: 읽기 전용 탭일 때만 `readonly`).
/// 기본 순서의 끝 = … 파일 형식 · **메모리 · 라이선스**(사용자 10-04 "라이선스 왼쪽에 메모리").
pub(crate) const BLOCKS: OrderDefs = &[
    ("tx", &[]),
    ("state", &["large", "readonly", "bookmark"]),
    ("pos", &[]),
    ("result", &["rows", "time"]),
    ("project", &["autosave", "git"]),
    ("format", &["enc", "eol", "indent", "syntax"]),
    ("mem", &[]),
    ("license", &[]),
];

/// 기본 숨김 없음(전부 표시).
pub(crate) const HIDDEN: Hidden = &[];

/// 숨길 수 없는 항목 — 라이선스 배지(무료판 "non-commercial use only"는 상시 표시 · docs/23 §4-4 D-44).
pub(crate) const LOCKED: &[&str] = &["license"];

/// 라벨(편집 창) — 그룹 행 = `(블록, None)` · 자식 행 = `(블록, Some(자식))`.
pub(crate) fn label(block: &str, item: Option<&str>) -> Msg {
    match item.unwrap_or(block) {
        "tx" => Msg::SbTx,
        "state" => Msg::SbGrpState,
        "large" => Msg::SbLarge,
        "readonly" => Msg::SbReadOnly,
        "bookmark" => Msg::SbBookmark,
        "pos" => Msg::SbPos,
        "result" => Msg::SbGrpResult,
        "rows" => Msg::SbRows,
        "time" => Msg::SbTime,
        "project" => Msg::SbGrpProject,
        "autosave" => Msg::SbAutosave,
        "git" => Msg::SbGit,
        "format" => Msg::SbGrpFormat,
        "enc" => Msg::SbEnc,
        "eol" => Msg::SbEol,
        "indent" => Msg::SbIndent,
        "syntax" => Msg::SbSyntax,
        "mem" => Msg::SbMem,
        _ => Msg::SbLicense,
    }
}

/// 설정값 → 블록 목록(순서대로 · 잠긴 블록은 늘 표시).
pub(crate) fn blocks(layout: &str) -> Vec<OrderBlock> {
    order::parse(BLOCKS, HIDDEN, layout)
        .into_iter()
        .map(|(id, vis, items)| {
            let vis = vis || LOCKED.contains(&id.as_str());
            (id, vis, items)
        })
        .collect()
}

/// 블록 목록 → 저장할 설정값(기본과 같으면 빈 문자열 · 잠긴 블록은 표시로 적는다).
pub(crate) fn to_setting(blocks: &[OrderBlock]) -> String {
    let fixed: Vec<OrderBlock> = blocks
        .iter()
        .map(|(id, vis, items)| {
            (
                id.clone(),
                *vis || LOCKED.contains(&id.as_str()),
                items.clone(),
            )
        })
        .collect();
    order::normalize(BLOCKS, HIDDEN, &order::serialize(&fixed))
}

/// 칸 하나를 숨긴 새 설정값(우클릭 "숨기기") — 그룹의 자식이면 그 자식만 · 단독 블록이면 블록 · 잠긴 칸/모르는 id = `None`.
pub(crate) fn hide(layout: &str, id: &str) -> Option<String> {
    if LOCKED.contains(&id) {
        return None;
    }
    let mut b = blocks(layout);
    let mut hit = false;
    for (bid, vis, items) in &mut b {
        if items.is_empty() && bid == id {
            *vis = false;
            hit = true;
        }
        for (_, v) in items.iter_mut().filter(|(k, _)| k == id) {
            *v = false;
            hit = true;
        }
    }
    hit.then(|| to_setting(&b))
}

/// 칸별 추가 메뉴(우클릭) — 그 칸과 관련된 명령(명령 id · 라벨). 클릭으로 열리는 자기 메뉴(구문·들여쓰기 등)는 좌클릭 그대로.
pub(crate) fn area_commands(id: &str) -> &'static [(&'static str, Msg)] {
    match id {
        "tx" => &[("view.txlog", Msg::MnTxLogWindow)],
        "mem" => &[("view.memory", Msg::MnMemoryWindow)],
        "rows" | "time" => &[("view.output", Msg::MnOutput)],
        _ => &[],
    }
}

/// 설정값 → 보일 칸 id를 표시 순서대로(그룹은 자식으로 펼친다 · 숨긴 그룹/자식 제외).
pub(crate) fn visible_ids(layout: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (id, vis, items) in blocks(layout) {
        if !vis {
            continue;
        }
        if items.is_empty() {
            out.push(id);
        } else {
            out.extend(items.into_iter().filter(|(_, v)| *v).map(|(k, _)| k));
        }
    }
    out
}

/// 지금 만들어진 칸들(id가 붙은 것)을 설정의 순서로 다시 놓고 숨긴 것은 뺀다. 같은 id가 여럿이면 만들어진 순서를 지킨다.
/// (빈 설정값도 거친다 — 기본 순서가 만든 순서와 다를 수 있다: 메모리는 라이선스 왼쪽.)
pub(crate) fn arrange<T>(layout: &str, segs: Vec<(&'static str, T)>) -> Vec<(&'static str, T)> {
    let order = visible_ids(layout);
    let mut keyed: Vec<(usize, (&'static str, T))> = segs
        .into_iter()
        .filter_map(|seg| {
            let i = order.iter().position(|id| id == seg.0)?;
            Some((i, seg))
        })
        .collect();
    // 안정 정렬 = 같은 id끼리는 만들어진 순서.
    keyed.sort_by_key(|(i, _)| *i);
    keyed.into_iter().map(|(_, seg)| seg).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[(&'static str, i32)]) -> Vec<&'static str> {
        v.iter().map(|(id, _)| *id).collect()
    }

    /// 그리기 코드가 만드는 순서(메모리가 앞쪽)로 칸을 준다.
    fn built() -> Vec<(&'static str, i32)> {
        [
            "tx", "mem", "large", "readonly", "bookmark", "pos", "rows", "time", "autosave", "git",
            "enc", "eol", "indent", "syntax", "license",
        ]
        .into_iter()
        .map(|id| (id, 0))
        .collect()
    }

    #[test]
    fn default_layout_shows_all_with_memory_left_of_license() {
        let v = arrange("", built());
        assert_eq!(
            ids(&v),
            [
                "tx", "large", "readonly", "bookmark", "pos", "rows", "time", "autosave", "git",
                "enc", "eol", "indent", "syntax", "mem", "license"
            ]
        );
        assert_eq!(to_setting(&blocks("")), "", "기본 = 빈 값");
        // 정의의 모든 칸이 그리기 코드의 id와 맞는다(빠지거나 남는 칸 없음).
        assert_eq!(visible_ids("").len(), built().len());
    }

    #[test]
    fn groups_move_as_a_unit_children_reorder_inside_and_hide() {
        // 파일 형식 그룹을 맨 앞으로 · 그 안에서 구문을 첫째로 · 줄끝 숨김 · 실행 결과 그룹 통째 숨김 · 라이선스 숨김 시도(잠금).
        let layout = "format:1[syntax:1,enc:1,eol:0,indent:1]|tx:1|result:0|license:0";
        // 값에 빠진 블록(mem · state · pos · project)은 정의상 앞 형제 바로 뒤에 보충된다(mem = format 뒤).
        let v = arrange(layout, built());
        assert_eq!(
            ids(&v),
            [
                "syntax", "enc", "indent", "mem", "tx", "large", "readonly", "bookmark", "pos",
                "autosave", "git", "license"
            ]
        );
        // 그룹을 숨겨도 자식 체크는 보존된다.
        let b = blocks(layout);
        let result = b.iter().find(|(id, _, _)| id == "result").expect("result");
        assert!(!result.1 && result.2.iter().all(|(_, v)| *v));
        // 저장값: 잠긴 블록은 표시로 · 왕복 안정.
        let saved = to_setting(&b);
        assert!(
            saved.contains("license:1") && saved.contains("result:0"),
            "{saved}"
        );
        assert_eq!(to_setting(&blocks(&saved)), saved);
    }

    #[test]
    fn hide_one_cell_child_or_single_block_never_locked() {
        // 그룹의 자식 = 그 자식만.
        let v = hide("", "eol").expect("hide");
        assert!(v.contains("format:1[enc:1,eol:0,indent:1,syntax:1]"), "{v}");
        // 단독 블록.
        let v2 = hide(&v, "mem").expect("hide");
        assert!(v2.contains("mem:0") && v2.contains("eol:0"), "{v2}");
        assert!(!visible_ids(&v2).iter().any(|i| i == "mem" || i == "eol"));
        // 잠긴 칸 · 모르는 id · 그룹 id(칸이 아님).
        assert_eq!(hide("", "license"), None);
        assert_eq!(hide("", "nope"), None);
        assert_eq!(hide("", "format"), None);
    }

    #[test]
    fn old_flat_layout_falls_back_gracefully_and_labels_are_distinct() {
        // 그룹 도입 전의 평면 값(자식이던 칸이 블록 자리에) = 모르는 블록은 버리고 그룹은 정의 순으로 보충.
        let v = arrange("syntax:1|tx:1|pos:0|license:1", built());
        assert!(!ids(&v).contains(&"pos"));
        assert!(ids(&v).contains(&"syntax") && ids(&v).contains(&"mem"));
        // 라벨: 라이선스 라벨로 새는 id가 없다.
        for (b, items) in BLOCKS {
            assert_eq!(label(b, None) == Msg::SbLicense, *b == "license", "{b}");
            for i in *items {
                assert_ne!(label(b, Some(i)), Msg::SbLicense, "{i}");
            }
        }
    }
}
