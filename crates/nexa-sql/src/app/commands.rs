//! ★ T-14 1단계(10-29 · D-393 · 사용자 "Command 레지스트리 먼저"): 명령 어휘의 **한 표**.
//!
//! 지금까지 명령 id는 네 곳에 따로 살았다 — 키맵 표(`keymap::COMMANDS` · id·라벨·기본 키) · 메뉴 트리(`build_menus_with` · 코드) ·
//! 팔레트 목록(`palette_commands` · 손으로 push한 100여 줄) · 핸들러(`menu_action` match). 1단계는 **팔레트의 정적 항목**을 이 표
//! ([`PALETTE`])로 옮기고, 세 원천(키맵·메뉴·팔레트)의 어긋남을 **시험**이 잡게 한다(id 중복 · 라벨 불일치 · 메뉴에만 있는 항목).
//! 메뉴 트리·핸들러 코드는 그대로(동작 불변). 2단계(T-14 후속) = `.sublime-keymap` 파서·context 평가기가 이 표 위에 겹친다.
//!
//! 표로 옮기며 드러난 것(시험이 다시 못 들어오게 막는다): 팔레트에 `view.theme`·`view.lang`이 두 번, `view.bookmarks`가 일반 항목과
//! 보기 토글로 두 번 들어가 있었다 → 각각 하나로.

use nsql_i18n::Msg;

/// 팔레트 항목의 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalKind {
    /// 늘 보이는 일반 명령 — 라벨 "메뉴: 항목".
    Plain,
    /// 보기 토글 — 지금 상태에 따라 "보기: <항목> 보이기/숨기기" 한 줄(사용자 09-27 · 메뉴 그룹은 늘 View).
    Toggle,
    /// 확장 관리자가 **켜져** 있을 때만.
    ExtOn,
    /// 확장 관리자가 **꺼져** 있을 때만.
    ExtOff,
    /// 이 자리에 **확장이 등록한 동적 명령**이 들어간다(표식 행 · `id`는 자리 이름 · 항목이 아니다).
    ExtDynamic,
}

/// 팔레트 한 행.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PalCmd {
    pub id: &'static str,
    pub menu: Msg,
    pub item: Msg,
    pub kind: PalKind,
}

const fn p(id: &'static str, menu: Msg, item: Msg) -> PalCmd {
    PalCmd {
        id,
        menu,
        item,
        kind: PalKind::Plain,
    }
}

const fn tv(id: &'static str, item: Msg) -> PalCmd {
    PalCmd {
        id,
        menu: Msg::MnView,
        item,
        kind: PalKind::Toggle,
    }
}

const fn ext_on(id: &'static str, menu: Msg, item: Msg) -> PalCmd {
    PalCmd {
        id,
        menu,
        item,
        kind: PalKind::ExtOn,
    }
}

/// 팔레트의 정적 항목 — **순서가 곧 팔레트 순서**(빈 질의일 때). 동적 항목(확장 명령 · `syntax.set:*`)은 코드가 제자리에 넣는다.
pub(crate) const PALETTE: &[PalCmd] = &[
    p("file.new", Msg::MnFile, Msg::MnNew),
    p("file.open", Msg::MnFile, Msg::MnOpen),
    p("file.save", Msg::MnFile, Msg::MnSave),
    p("file.save_as", Msg::MnFile, Msg::MnSaveAs),
    p("file.follow", Msg::MnFile, Msg::MnFileFollow),
    p("file.run_file", Msg::MnFile, Msg::MnRunFile),
    p("file.large_force", Msg::MnFile, Msg::MnLargeForce),
    p("mem.trim_now", Msg::MnView, Msg::MnMemTrimNow),
    p("file.exit", Msg::MnFile, Msg::MnExit),
    p("edit.cut", Msg::MnEdit, Msg::MnCut),
    p("edit.copy", Msg::MnEdit, Msg::MnCopy),
    p("edit.paste", Msg::MnEdit, Msg::MnPaste),
    p("edit.select_all", Msg::MnEdit, Msg::MnSelectAll),
    p("edit.expand_selection", Msg::MnEdit, Msg::MnExpandSelection),
    p("edit.goto_bracket", Msg::MnEdit, Msg::MnGotoBracket),
    p("edit.bracket_prev", Msg::MnEdit, Msg::MnBracketPrev),
    p("edit.bracket_next", Msg::MnEdit, Msg::MnBracketNext),
    p("edit.bracket_parent", Msg::MnEdit, Msg::MnBracketParent),
    p("edit.bracket_child", Msg::MnEdit, Msg::MnBracketChild),
    p("edit.complete", Msg::MnEdit, Msg::MnComplete),
    p("goto.symbol", Msg::MnEdit, Msg::MnGotoSymbol),
    // Extension Manager(Sublime "Package Control: …" · docs/50 §10) — 관리자 상태에 맞는 명령만(사용자 09-19).
    ext_on(
        "ext.disable_mgr",
        Msg::MnExtensions,
        Msg::MnExtDisableManager,
    ),
    ext_on("view.extensions", Msg::MnView, Msg::MnExtensionsPanel),
    ext_on("ext.install", Msg::MnExtensions, Msg::MnExtInstall),
    ext_on("ext.update", Msg::MnExtensions, Msg::MnExtUpdate),
    PalCmd {
        id: "<ext.dynamic>",
        menu: Msg::MnExtensions,
        item: Msg::MnExtensions,
        kind: PalKind::ExtDynamic,
    },
    ext_on("ext.remove", Msg::MnExtensions, Msg::MnExtRemove),
    ext_on("ext.list", Msg::MnExtensions, Msg::MnExtList),
    ext_on("ext.enable", Msg::MnExtensions, Msg::MnExtEnable),
    ext_on("ext.disable", Msg::MnExtensions, Msg::MnExtDisable),
    ext_on("ext.repo_add", Msg::MnExtensions, Msg::MnExtRepoAdd),
    ext_on("ext.repo_list", Msg::MnExtensions, Msg::MnExtRepoList),
    ext_on("ext.repo_remove", Msg::MnExtensions, Msg::MnExtRepoRemove),
    PalCmd {
        id: "ext.enable_mgr",
        menu: Msg::MnExtensions,
        item: Msg::MnExtEnableManager,
        kind: PalKind::ExtOff,
    },
    p("edit.expand_brackets", Msg::MnEdit, Msg::MnExpandBrackets),
    p("edit.skip_occurrence", Msg::MnEdit, Msg::MnSkipOccurrence),
    p("obj.reveal", Msg::MnEdit, Msg::MnObjLinkReveal),
    p("obj.info", Msg::MnEdit, Msg::MnObjInfoTab),
    p("obj.rows", Msg::MnEdit, Msg::MnObjRows),
    p("view.zoom_in", Msg::MnView, Msg::MnZoomIn),
    p("view.zoom_out", Msg::MnView, Msg::MnZoomOut),
    p("view.zoom_reset", Msg::MnView, Msg::MnZoomReset),
    p("obj.compare", Msg::MnEdit, Msg::MnObjCompare),
    p("compare.next", Msg::MnEdit, Msg::MnCompareNext),
    p("compare.prev", Msg::MnEdit, Msg::MnComparePrev),
    p("obj.compare_server", Msg::MnEdit, Msg::MnObjCompareServer),
    p("obj.menu", Msg::MnExplorer, Msg::MnObjMenu),
    p("obj.select_rows", Msg::MnExplorer, Msg::ExpSelectRows),
    p("obj.source", Msg::MnExplorer, Msg::ExpOpenSource),
    p("obj.copy_name", Msg::MnExplorer, Msg::ExpCopyName),
    p("obj.refresh", Msg::MnExplorer, Msg::ExpRefresh),
    p("obj.refresh_meta", Msg::MnExplorer, Msg::ExpRefreshMeta),
    p("obj.sizes", Msg::MnExplorer, Msg::ExpLoadSizes),
    p("bookmark.toggle", Msg::MnBookmarks, Msg::MnBmToggle),
    p("bookmark.next", Msg::MnBookmarks, Msg::MnBmNext),
    p("bookmark.prev", Msg::MnBookmarks, Msg::MnBmPrev),
    p("bookmark.select_all", Msg::MnBookmarks, Msg::MnBmSelectAll),
    p("bookmark.label", Msg::MnBookmarks, Msg::MnBmLabel),
    p("bookmark.clear_doc", Msg::MnBookmarks, Msg::MnBmClearDoc),
    p(
        "edit.select_all_occurrences",
        Msg::MnEdit,
        Msg::MnSelectAllOccurrences,
    ),
    p("edit.undo", Msg::MnEdit, Msg::MnUndo),
    p("edit.find", Msg::MnEdit, Msg::MnFind),
    p("edit.replace", Msg::MnEdit, Msg::MnReplace),
    p("edit.find_next", Msg::MnEdit, Msg::MnFindNext),
    p("edit.find_prev", Msg::MnEdit, Msg::MnFindPrev),
    p("edit.redo", Msg::MnEdit, Msg::MnRedo),
    p("edit.soft_undo", Msg::MnEdit, Msg::MnSoftUndo),
    p("edit.soft_redo", Msg::MnEdit, Msg::MnSoftRedo),
    // 보기 토글(켜져 있으면 "숨기기" · 꺼져 있으면 "보이기" 한 줄).
    tv("view.explorer", Msg::MnExplorer),
    tv("view.object_details", Msg::MnObjectDetails),
    tv("view.output", Msg::MnOutput),
    tv("view.search", Msg::MnSearchPanel),
    tv("view.project", Msg::MnProjectPanel),
    tv("view.bookmarks", Msg::MnBookmarksPanel),
    tv("view.outline", Msg::MnOutlinePanel),
    tv("view.log", Msg::MnLogWindow),
    tv("view.txlog", Msg::MnTxLogWindow),
    tv("view.sessions", Msg::MnSessManager),
    tv("view.memory", Msg::MnMemoryWindow),
    tv("view.find_objects", Msg::MnFindObjects),
    tv("view.variables", Msg::MnVariables),
    // 드라이버 관리자 = 열기/포커스(토글 아님 · 협업 10-23).
    p("view.drivers", Msg::MnView, Msg::MnDrivers),
    tv("view.on_top", Msg::MnAlwaysOnTop),
    p("view.colors", Msg::MnView, Msg::MnColors),
    p("view.keys", Msg::MnView, Msg::MnKeys),
    p("view.theme", Msg::MnView, Msg::MnTheme),
    p("view.lang", Msg::MnView, Msg::MnLanguage),
    p("view.toolbar_reset", Msg::MnView, Msg::MnResetToolbar),
    p("view.layout_reset", Msg::MnView, Msg::MnResetLayout),
    p("project.new", Msg::MnProject, Msg::MnProjectNew),
    p("project.open", Msg::MnProject, Msg::MnProjectOpen),
    p("project.switch", Msg::MnProject, Msg::MnProjectSwitch),
    p("project.save", Msg::MnProject, Msg::MnProjectSave),
    p("project.save_as", Msg::MnProject, Msg::MnProjectSaveAs),
    p("project.close", Msg::MnProject, Msg::MnProjectClose),
    p(
        "project.add_folder",
        Msg::MnProject,
        Msg::MnProjectAddFolder,
    ),
    p("vars.script", Msg::MnView, Msg::MnVariablesScript),
    p("vars.show", Msg::MnView, Msg::MnShowVariables),
    p("file.close_tab", Msg::MnFile, Msg::MnCloseTab),
    p("tab.next", Msg::MnView, Msg::MnNextTab),
    p("tab.prev", Msg::MnView, Msg::MnPrevTab),
    p("run.statement", Msg::MnRun, Msg::MnRunStatement),
    p(
        "run.statement_new_tab",
        Msg::MnRun,
        Msg::MnRunStatementNewTab,
    ),
    p("result.tab.close", Msg::MnRun, Msg::MnResultCloseTab),
    p("result.tab.next", Msg::MnRun, Msg::MnResultNextTab),
    p("result.tab.prev", Msg::MnRun, Msg::MnResultPrevTab),
    p("run.all", Msg::MnRun, Msg::MnRunAll),
    p("run.explain", Msg::MnRun, Msg::MnExplain),
    p("run.explain_analyze", Msg::MnRun, Msg::MnExplainAnalyze),
    p("run.commit", Msg::MnRun, Msg::MnCommit),
    p("run.rollback", Msg::MnRun, Msg::MnRollback),
    p("conn.toggle", Msg::MnRun, Msg::MnConnect),
    p("conn.disconnect", Msg::MnRun, Msg::MnDisconnect),
    p("session.info", Msg::MnRun, Msg::MnSessInfo),
    p("session.env:none", Msg::MnRun, Msg::MnEnvNone),
    p("session.env:dev", Msg::MnRun, Msg::MnEnvDev),
    p("session.env:test", Msg::MnRun, Msg::MnEnvTest),
    p("session.env:prod", Msg::MnRun, Msg::MnEnvProd),
    p("edit.duplicate_line", Msg::MnEdit, Msg::MnDuplicateLine),
    p("edit.delete_line", Msg::MnEdit, Msg::MnDeleteLine),
    p("edit.join_lines", Msg::MnEdit, Msg::MnJoinLines),
    p("edit.swap_line_up", Msg::MnEdit, Msg::MnSwapLineUp),
    p("edit.swap_line_down", Msg::MnEdit, Msg::MnSwapLineDown),
    p("edit.toggle_comment", Msg::MnEdit, Msg::MnToggleComment),
    p(
        "edit.toggle_block_comment",
        Msg::MnEdit,
        Msg::MnToggleBlockComment,
    ),
    // 시험이 찾아낸 빈자리(10-29): 메뉴·키맵에는 있는데 팔레트에 없던 서식 명령 3개.
    p("edit.format", Msg::MnEdit, Msg::MnFormatSql),
    p("edit.format_with", Msg::MnEdit, Msg::MnFormatWith),
    p("edit.format_preview", Msg::MnEdit, Msg::MnFormatPreview),
    p("intel.refresh", Msg::MnEdit, Msg::MnIntelRefresh),
    p(
        "intel.refresh_server",
        Msg::MnEdit,
        Msg::MnIntelRefreshServer,
    ),
    p("intel.refresh_all", Msg::MnEdit, Msg::MnIntelRefreshAll),
    p("edit.indent", Msg::MnEdit, Msg::MnIndent),
    p("edit.unindent", Msg::MnEdit, Msg::MnUnindent),
    p("edit.select_line", Msg::MnEdit, Msg::MnSelectLine),
    p("edit.split_lines", Msg::MnEdit, Msg::MnSplitLines),
    p("edit.add_caret_up", Msg::MnEdit, Msg::MnAddCaretUp),
    p("edit.add_caret_down", Msg::MnEdit, Msg::MnAddCaretDown),
    p("edit.upper_case", Msg::MnEdit, Msg::MnUpperCase),
    p("edit.lower_case", Msg::MnEdit, Msg::MnLowerCase),
    p("edit.goto_line", Msg::MnEdit, Msg::MnGotoLine),
    p("eol.crlf", Msg::MnEdit, Msg::MnEolCrlf),
    p("eol.lf", Msg::MnEdit, Msg::MnEolLf),
    p("eol.cr", Msg::MnEdit, Msg::MnEolCr),
    p("view.goto_anything", Msg::MnView, Msg::MnGotoAnything),
    p("edit.prefs", Msg::MnEdit, Msg::MnPreferences),
    p("edit.settings_json", Msg::MnEdit, Msg::MnSettingsJson),
    p("settings.export", Msg::MnEdit, Msg::MnSettingsExport),
    p("settings.import", Msg::MnEdit, Msg::MnSettingsImport),
    p("help.license", Msg::MnHelp, Msg::MnLicense),
    p("help.about", Msg::MnHelp, Msg::MnAbout),
];

/// 메뉴에는 있지만 팔레트에 **일부러** 없는 항목(대화상자·동적·메뉴 전용) — 시험의 허용 목록. 새 메뉴 항목은 여기 적든지 [`PALETTE`]에 넣든지.
#[cfg(test)]
pub(crate) const MENU_ONLY: &[&str] = &[
    "edit.insert_sequence",
    "tab.find",
    "help.demo",
    // 팔레트 자신을 여는 명령은 팔레트 안에 둘 이유가 없다.
    "view.palette",
];

/// 메뉴 트리의 항목 id 전부(하위 메뉴 한 단계 포함 · 구분선 제외) — 시험·진단.
#[cfg(test)]
pub(crate) fn menu_ids(menus: &[nexa_ctl::MenuDef]) -> Vec<String> {
    use nexa_ctl::MenuEntry;
    fn walk(entries: &[MenuEntry], out: &mut Vec<String>) {
        for e in entries {
            match e {
                MenuEntry::Item(it) | MenuEntry::Emph(it) | MenuEntry::Disabled(it) => {
                    out.push(it.value.clone());
                }
                MenuEntry::Sub(_, inner) => walk(inner, out),
                MenuEntry::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    for m in menus {
        walk(&m.entries, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn ids() -> Vec<&'static str> {
        PALETTE
            .iter()
            .filter(|c| c.kind != PalKind::ExtDynamic)
            .map(|c| c.id)
            .collect()
    }

    /// 표의 id는 하나씩(종전 팔레트의 view.theme·view.lang·view.bookmarks 중복이 다시 못 들어온다) · 동적 자리 표식은 하나.
    #[test]
    fn palette_ids_are_unique() {
        let all = ids();
        let set: BTreeSet<&str> = all.iter().copied().collect();
        assert_eq!(all.len(), set.len(), "중복 id: {:?}", {
            let mut seen = BTreeSet::new();
            all.iter().filter(|i| !seen.insert(**i)).collect::<Vec<_>>()
        });
        assert_eq!(
            PALETTE
                .iter()
                .filter(|c| c.kind == PalKind::ExtDynamic)
                .count(),
            1
        );
    }

    /// 키맵 표와 같은 id면 **라벨도 같다**(팔레트 "메뉴: 항목"의 항목 = 키 설정 화면의 라벨) — 토글 항목은 패널 이름이라 제외.
    #[test]
    fn palette_labels_match_keymap() {
        let mut bad = Vec::new();
        for c in PALETTE.iter().filter(|c| c.kind == PalKind::Plain) {
            if let Some(k) = crate::keymap::COMMANDS.iter().find(|k| k.id == c.id) {
                if k.label != c.item {
                    bad.push((c.id, k.label, c.item));
                }
            }
        }
        assert!(bad.is_empty(), "키맵 라벨 ≠ 팔레트 라벨: {bad:?}");
    }

    /// 메뉴 트리의 모든 항목은 팔레트에도 있다(허용 목록 제외) — 메뉴에만 추가하고 팔레트를 잊는 일을 막는다. 허용 목록은 실제 메뉴에 있어야 한다.
    #[test]
    fn menu_items_are_in_palette_or_allowlisted() {
        let menus = crate::App::build_menus();
        let menu: BTreeSet<String> = menu_ids(&menus).into_iter().collect();
        let pal: BTreeSet<&str> = ids().into_iter().collect();
        let missing: Vec<&String> = menu
            .iter()
            .filter(|m| !pal.contains(m.as_str()) && !MENU_ONLY.contains(&m.as_str()))
            .collect();
        assert!(missing.is_empty(), "메뉴에만 있는 명령: {missing:?}");
        for m in MENU_ONLY {
            assert!(menu.contains(*m), "허용 목록이 낡음: {m}");
            assert!(!pal.contains(m), "허용 목록인데 팔레트에도 있음: {m}");
        }
    }
}
