//! **Project Explorer Menus** — 열린 파일의 우클릭 메뉴를 늘리는 확장의 **기본 틀**(사용자 09-30 · docs/97).
//!
//! 지금 하는 일(1단계): 편집기 우클릭 메뉴에 "Project Explorer Menus" 그룹 + 명령 하나 — 활성 문서의 파일 경로를 호스트에서 받아
//! (`Editor::doc_path` ← import `nx_host_get(DOC_PATH)`) 앱 로그에 남긴다. 다음 단계(호스트 확장 · docs/97 §5)는 프로젝트 탐색기
//! OPEN FILES 행 메뉴에 붙이기(`menus[].target = "project_explorer"`)와 파일 조작 op.
//!
//! 빌드: `extensions/sdk`에서 `cargo build --release` → `target/wasm32-unknown-unknown/release/project_explorer_menus.wasm`
//! → `pwsh scripts/ext-build.ps1 -Only project-explorer-menus`(패키지 폴더 배치 + sha256) → 확장 패널 "설치 가능"에서 설치.

use nexa_ext_sdk::{Command, Editor, Effect, Extension, Label, Menu, Meta, Settings};

/// 명령 id(팔레트·메뉴·키맵이 같은 id를 쓴다 · 규칙 = `ext.<확장>.<이름>`).
const CMD_SHOW_PATH: &str = "ext.project_explorer_menus.show_path";

struct ProjectExplorerMenus;

impl Extension for ProjectExplorerMenus {
    fn meta() -> Meta {
        Meta {
            id: "project-explorer-menus".into(),
            name: "Project Explorer Menus".into(),
            // 이 확장의 설정 키 접두(아직 키 없음 · 추가하면 호스트 레지스트리에도 등록 — docs/94 §6-4).
            settings_prefix: "ext.project_explorer_menus.".into(),
            commands: vec![Command {
                id: CMD_SHOW_PATH.into(),
                label: Label::new(
                    "Project Explorer Menus: Show file path",
                    "Project Explorer Menus: 파일 경로 보기",
                ),
            }],
            // 편집기 우클릭 메뉴의 서브메뉴(1단계 자리) — 항목 = 위 명령 id.
            menus: vec![Menu {
                id: "project-explorer-menus".into(),
                label: Label::new("Project Explorer Menus", "Project Explorer Menus"),
                items: vec![CMD_SHOW_PATH.into()],
            }],
            formatter: None,
        }
    }

    fn on_settings(_s: &Settings) -> Effect {
        // 편집기 옵션은 건드리지 않는다.
        Effect::default()
    }

    fn run(cmd: &str, ed: &mut Editor) -> bool {
        match cmd {
            CMD_SHOW_PATH => {
                // ★ 호스트 함수 호출의 끝단: 호스트 `run_with(doc_path)` → HostCtx → import `nx_host_get` → SDK `Editor::doc_path`.
                match ed.doc_path() {
                    Some(p) => nexa_ext_sdk::log(&format!("file: {p}")),
                    None => nexa_ext_sdk::log("file: (unsaved script)"),
                }
                true
            }
            _ => false,
        }
    }
}

nexa_ext_sdk::export_extension!(ProjectExplorerMenus);

#[cfg(test)]
mod tests {
    use super::*;

    /// 메타 = id·이름·명령·메뉴가 서로 맞물린다(메뉴 항목 = 등록된 명령).
    #[test]
    fn meta_wires_menu_to_command() {
        let m = ProjectExplorerMenus::meta();
        assert_eq!(m.id, "project-explorer-menus");
        assert_eq!(m.name, "Project Explorer Menus");
        assert!(m.menus[0]
            .items
            .iter()
            .all(|i| m.commands.iter().any(|c| &c.id == i)));
        assert!(m
            .commands
            .iter()
            .all(|c| c.id.starts_with(&m.settings_prefix)));
    }

    /// 호스트 밖(네이티브 시험)에서는 문서 경로가 없다 → 명령은 처리했다고 답하고 로그만.
    #[test]
    fn run_handles_own_command_only() {
        let mut ed = Editor;
        assert!(ProjectExplorerMenus::run(CMD_SHOW_PATH, &mut ed));
        assert!(!ProjectExplorerMenus::run("ext.other.cmd", &mut ed));
    }
}
