//! ★ 우클릭 메뉴 아이콘 공용 입구(사용자 10-13 "모든 우클릭 메뉴에 이미지 영역"): 항목 id → 아이콘을 **한 자리**에서 붙이고,
//! 모든 메뉴는 [`open_at`]으로 연다(30 §2 부품 규칙 · 메뉴마다 따로 아이콘을 고르지 않는다).
//!
//! - 아이콘 칸 = nexa-ctl `icon_col_always`(기동 때 켬)로 **늘 예약** → 아이콘이 없는 메뉴도 글 시작 x가 같다.
//! - 향상 모드(`ui.menu_icons` FORCED off) = **칸만 두고** 아이콘을 만들지도(래스터 0) 그리지도 않는다([`decorate`]가 먼저 거른다).
//! - 탐색기 메뉴는 종류 아이콘(테이블 등)이 따로 있어 자기 `decorate_menu`를 먼저 거친 뒤 여기로 온다(이미 아이콘이 있는 항목은 건드리지 않는다).

use crate::toolicons as ti;
use nexa_ctl::controls::ctxmenu::{menu_icons_enabled, ContextMenu, CtxItem, MenuIcon};
use nexa_ctl::geom::Rect;

/// 모든 우클릭 메뉴의 열기 입구 — 아이콘을 붙인 뒤 `ContextMenu::open_at`.
pub(crate) fn open_at(
    menu: &mut ContextMenu,
    x: i32,
    y: i32,
    items: Vec<CtxItem>,
    host: Rect,
    text_w: i32,
) {
    menu.open_at(x, y, decorate(items), host, text_w);
}

/// 항목(하위 메뉴 포함)에 id별 아이콘을 붙인다 — 이미 있는 아이콘은 유지 · 향상 모드(메뉴 아이콘 끔)면 그대로 돌려준다(비용 0).
pub(crate) fn decorate(items: Vec<CtxItem>) -> Vec<CtxItem> {
    if !menu_icons_enabled() {
        return items;
    }
    items.into_iter().map(decorate_one).collect()
}

fn decorate_one(it: CtxItem) -> CtxItem {
    match it {
        CtxItem::Item {
            id,
            label,
            enabled,
            icon,
            shortcut,
            sub,
            emph,
            marks,
            children,
            checked,
            active,
            mark,
        } => {
            let icon = icon.or_else(|| icon_for(&id));
            let children = children.into_iter().map(decorate_one).collect();
            CtxItem::Item {
                id,
                label,
                enabled,
                icon,
                shortcut,
                sub,
                emph,
                marks,
                children,
                checked,
                active,
                mark,
            }
        }
        other => other,
    }
}

/// id → 아이콘(모든 메뉴 공통 어휘 · 정확한 id 먼저 · 그 다음 접두/낱말 규칙). 없으면 `None`(칸은 비운 채).
pub(crate) fn icon_for(id: &str) -> Option<MenuIcon> {
    let exact = match id {
        // 복사·편집
        "copy"
        | "edit.copy"
        | "copy_cs"
        | "copy_file"
        | "copy_bound"
        | "combo_item"
        | "combo_list"
        | "dup"
        | "objlink.copy_desc"
        | "objlink.copy_name_desc"
        | "bookmark.copy" => ti::mi_copy(),
        "copy_name" => ti::mi_description(),
        "copy_path" => ti::mi_link(),
        "copy_csv" => ti::mi_csv(),
        "copy_json" => ti::mi_braces(),
        "copy_md" => ti::mi_markdown(),
        "copy_txt" => ti::mi_text(),
        "cut" | "edit.cut" => ti::mi_cut(),
        "paste" | "edit.paste" => ti::mi_paste(),
        "all" | "select_all" | "edit.select_all" | "bookmark.select_all" | "doc.select" => {
            ti::mi_select_all()
        }
        "edit.undo" | "undo" => ti::mi_undo()?,
        "edit.redo" | "redo" => ti::mi_redo()?,
        // 실행(툴바 ▾ 메뉴 · 같은 그림)
        "run.statement" | "run.statement_new_tab" | "run.selection" => ti::mi_run()?,
        "run.all" | "file.run_file" => ti::mi_run_all()?,
        "run.stop" => ti::mi_stop()?,
        "run.commit" => ti::mi_commit()?,
        "run.rollback" => ti::mi_rollback()?,
        "run.explain" => ti::mi_data_object(),
        // 탭·창
        "close" | "close_all" | "close_left" | "close_right" | "close_others"
        | "file.close_tab" | "guard_discard" | "guard_cancel" => ti::mi_close(),
        "keep_open" => ti::mi_push_pin(),
        "rename" | "grp.rename" | "tab.rename" => ti::mi_edit(),
        "open" | "doc.open" | "file.open" | "open_bound" | "reveal" => ti::mi_folder_open(),
        "reveal_project" | "reveal_object" | "objlink.reveal" | "project.reveal" => {
            ti::mi_my_location()
        }
        "first" | "go_first" => ti::mi_first_page(),
        "last" | "go_last" => ti::mi_last_page(),
        // 지우기·새로 만들기·저장
        "remove" | "remove_invalid" | "doc.remove" | "drop" | "drop_all" | "delete" | "clear"
        | "bookmark.clear_doc" | "edit.delete_line" => ti::mi_delete(),
        "new" | "new_group" | "file.new" | "newtab" | "tab" => ti::mi_add(),
        "save" | "file.save" | "file.save_as" | "guard_save" => ti::mi_save(),
        // 설정·초기화
        "settings" | "sb.settings" | "tb.settings" | "edit.prefs" => ti::mi_gear(),
        "sb.reset" | "tb.reset" | "view.layout_reset" | "view.toolbar_reset" | "reset" => {
            ti::mi_restart()
        }
        // 접속·세션
        "refresh" | "again" | "intel.refresh" | "intel.refresh_all" | "intel.refresh_server" => {
            ti::mi_refresh()
        }
        "use" => ti::mi_check(),
        "connect" | "conn.toggle" => ti::mi_connect(),
        "disconnect" | "conn.disconnect" => ti::mi_disconnect(),
        // 그리드
        "sort.asc" => ti::mi_arrow_up(),
        "sort.desc" => ti::mi_arrow_down(),
        "filter.values" | "filter" | "cond" => ti::mi_filter(),
        "sql_select" | "sql_insert" | "sql_update" | "sql_delete" | "sql_merge" | "sql"
        | "copy_sql" | "source" | "body" => ti::mi_code(),
        "adv" => ti::mi_copy(),
        "grid.edit.view_value" | "view_value" => ti::mi_visibility(),
        // 북마크
        "bookmark.toggle" | "bmg.toggle" | "bmg.view" | "bookmark.next" | "bookmark.prev"
        | "view.bookmarks" => ti::mi_bookmark(),
        "bookmark.label" | "label" => ti::mi_label(),
        "view.output" | "output" => ti::mi_terminal(),
        _ => return by_rule(id),
    };
    Some(exact)
}

/// 접두·낱말 규칙(정확한 id가 없을 때): `filter.*`/`cond.*` = 깔때기 · `*hide*` = 눈 가림 · `*output*` = 터미널 · `copy*` = 복사.
fn by_rule(id: &str) -> Option<MenuIcon> {
    let i = id;
    if i.starts_with("filter.") || i.starts_with("cond.") {
        return Some(ti::mi_filter());
    }
    if i.contains("hide") {
        return Some(ti::mi_visibility_off());
    }
    if i.contains("output") {
        return Some(ti::mi_terminal());
    }
    if i.starts_with("copy") {
        return Some(ti::mi_copy());
    }
    if i.starts_with("close") {
        return Some(ti::mi_close());
    }
    if i.starts_with("remove") || i.starts_with("delete") {
        return Some(ti::mi_delete());
    }
    if i.starts_with("rename") {
        return Some(ti::mi_edit());
    }
    if i.starts_with("open") {
        return Some(ti::mi_folder_open());
    }
    if i.starts_with("settings") || i.ends_with(".settings") {
        return Some(ti::mi_gear());
    }
    if i.ends_with(".reset") {
        return Some(ti::mi_restart());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_ids_get_icons_and_boost_skips_them() {
        nexa_ctl::controls::set_menu_icons(true);
        for id in [
            "copy",
            "close",
            "rename",
            "reveal",
            "remove",
            "settings",
            "sb.reset",
            "filter.contains",
            "cond.eq",
            "sort.asc",
            "keep_open",
            "copy_path",
        ] {
            assert!(icon_for(id).is_some(), "{id}");
        }
        assert!(icon_for("something.unknown").is_none());
        let items = vec![
            CtxItem::item("copy", "복사"),
            CtxItem::Separator,
            CtxItem::item("x", "모름"),
        ];
        let d = decorate(items);
        assert!(matches!(&d[0], CtxItem::Item { icon: Some(_), .. }));
        assert!(matches!(&d[2], CtxItem::Item { icon: None, .. }));
        // 향상 모드(메뉴 아이콘 끔) = 아이콘을 만들지 않는다(칸은 `icon_col_always`가 따로 예약).
        nexa_ctl::controls::set_menu_icons(false);
        let d = decorate(vec![CtxItem::item("copy", "복사")]);
        assert!(matches!(&d[0], CtxItem::Item { icon: None, .. }));
        nexa_ctl::controls::set_menu_icons(true);
    }
}
