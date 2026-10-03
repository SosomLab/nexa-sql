//! 상태바 오른쪽 항목의 순서·표시(사용자 10-04 "상태바 위치와 사용 여부를 설정 화면에서" · nexa-dir3 순서 편집 창과 같은 방식).
//! 값 = 설정 `statusbar.layout`(nexa-ctl `order` 문법 · 빈 값 = 기본). 여기는 **정의와 순수 함수**만 — 그리기는 `app/paint.rs`,
//! 편집 창은 `order_win.rs`.

use nexa_ctl::order::{self, Hidden, OrderDefs};
use nsql_i18n::Msg;

/// 설정 키.
pub(crate) const KEY: &str = "statusbar.layout";

/// 항목(정의 순 = 기본 표시 순서 · 왼쪽 → 오른쪽). 항목이 실제로 나오는지는 그때의 상태가 정한다(예: 읽기 전용 탭일 때만 `readonly`).
pub(crate) const BLOCKS: OrderDefs = &[
    ("tx", &[]),
    ("mem", &[]),
    ("large", &[]),
    ("readonly", &[]),
    ("bookmark", &[]),
    ("pos", &[]),
    ("rows", &[]),
    ("time", &[]),
    ("autosave", &[]),
    ("git", &[]),
    ("enc", &[]),
    ("eol", &[]),
    ("indent", &[]),
    ("syntax", &[]),
    ("license", &[]),
];

/// 기본 숨김 없음(전부 표시 = 종전 화면).
pub(crate) const HIDDEN: Hidden = &[];

/// 숨길 수 없는 항목 — 라이선스 배지(무료판 "non-commercial use only"는 상시 표시 · docs/23 §4-4 D-44).
pub(crate) const LOCKED: &[&str] = &["license"];

/// 항목 라벨(편집 창).
pub(crate) fn label(id: &str) -> Msg {
    match id {
        "tx" => Msg::SbTx,
        "mem" => Msg::SbMem,
        "large" => Msg::SbLarge,
        "readonly" => Msg::SbReadOnly,
        "bookmark" => Msg::SbBookmark,
        "pos" => Msg::SbPos,
        "rows" => Msg::SbRows,
        "time" => Msg::SbTime,
        "autosave" => Msg::SbAutosave,
        "git" => Msg::SbGit,
        "enc" => Msg::SbEnc,
        "eol" => Msg::SbEol,
        "indent" => Msg::SbIndent,
        "syntax" => Msg::SbSyntax,
        _ => Msg::SbLicense,
    }
}

/// 설정값 → (항목 id, 표시) 목록(순서대로 · 잠긴 항목은 늘 표시).
pub(crate) fn items(layout: &str) -> Vec<(String, bool)> {
    order::parse(BLOCKS, HIDDEN, layout)
        .into_iter()
        .map(|(id, vis, _)| {
            let vis = vis || LOCKED.contains(&id.as_str());
            (id, vis)
        })
        .collect()
}

/// (항목 id, 표시) 목록 → 저장할 설정값(기본과 같으면 빈 문자열).
pub(crate) fn to_setting(items: &[(String, bool)]) -> String {
    let blocks: Vec<order::OrderBlock> = items
        .iter()
        .map(|(id, vis)| {
            (
                id.clone(),
                *vis || LOCKED.contains(&id.as_str()),
                Vec::new(),
            )
        })
        .collect();
    order::normalize(BLOCKS, HIDDEN, &order::serialize(&blocks))
}

/// 지금 만들어진 칸들(id가 붙은 것)을 설정의 순서로 다시 놓고 숨긴 것은 뺀다. 같은 id가 여럿이면 원래 순서를 지킨다.
pub(crate) fn arrange<T>(layout: &str, segs: Vec<(&'static str, T)>) -> Vec<(&'static str, T)> {
    if layout.is_empty() {
        return segs;
    }
    let order = items(layout);
    let mut keyed: Vec<(usize, (&'static str, T))> = segs
        .into_iter()
        .filter_map(|seg| {
            let (i, (_, vis)) = order.iter().enumerate().find(|(_, (id, _))| id == seg.0)?;
            vis.then_some((i, seg))
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

    #[test]
    fn empty_layout_keeps_everything_as_built() {
        let segs = vec![("tx", 1), ("pos", 2), ("syntax", 3), ("license", 4)];
        assert_eq!(ids(&arrange("", segs)), ["tx", "pos", "syntax", "license"]);
        assert_eq!(items("").len(), BLOCKS.len());
        assert!(items("").iter().all(|(_, v)| *v));
        assert_eq!(to_setting(&items("")), "", "기본 = 빈 값");
    }

    #[test]
    fn layout_reorders_hides_and_never_hides_locked() {
        // 구문을 맨 앞으로 · 위치 숨김 · 라이선스 숨김 시도(잠금 = 무시).
        let layout = "syntax:1|tx:1|pos:0|license:0";
        let segs = vec![("tx", 1), ("pos", 2), ("syntax", 3), ("license", 4)];
        assert_eq!(ids(&arrange(layout, segs)), ["syntax", "tx", "license"]);
        let it = items(layout);
        assert_eq!(it[0], ("syntax".to_string(), true));
        assert!(it.iter().any(|(id, v)| id == "pos" && !*v));
        assert!(it.iter().any(|(id, v)| id == "license" && *v));
        // 저장값은 잠긴 항목을 표시로 적는다 · 왕복 안정.
        let saved = to_setting(&it);
        assert!(
            saved.contains("license:1") && saved.contains("pos:0"),
            "{saved}"
        );
        assert_eq!(to_setting(&items(&saved)), saved);
        // 모든 정의 항목에 라벨이 있다(다른 항목이 라이선스 라벨로 새지 않는다).
        for (id, _) in BLOCKS {
            assert_eq!(label(id) == Msg::SbLicense, *id == "license", "{id}");
        }
    }
}
