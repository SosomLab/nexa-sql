//! **Project Explorer Menus** — 열린 파일의 우클릭 메뉴를 늘리는 확장(사용자 09-30 · docs/97).
//!
//! 0.2.0(146차 · 10-27 · docs/97 §5 ①~③): ① 메뉴가 **프로젝트 탐색기 파일 행**에도 붙는다(`Menu::project_explorer` · 명령은 그 행의 경로를
//! `Editor::doc_path`로 받는다) ② 파일 동작 = `Files::open`(새 탭) · `Files::reveal`(탐색기에서 드러내기) · `Files::copy_path`(명시적 명령) ③ 설정
//! `ext.project_explorer_menus.log_path`(메타 `settings` 선언 → 호스트 설정 창 "Project Explorer Menus" 분류) · 명령 기본 단축키(`key`).
//!
//! 빌드: `extensions/sdk`에서 `cargo build --release` → `target/wasm32-unknown-unknown/release/project_explorer_menus.wasm`
//! → `pwsh scripts/ext-build.ps1 -Only project-explorer-menus`(패키지 폴더 배치 + sha256) → 확장 패널에서 설치/업데이트.

use nexa_ext_sdk::{
    Command, Editor, Effect, Extension, Files, Label, Menu, Meta, SettingDef, Settings,
};
use std::sync::atomic::{AtomicBool, Ordering};

/// 명령 id(팔레트·메뉴·키맵이 같은 id를 쓴다 · 규칙 = `ext.<확장>.<이름>`).
const CMD_SHOW_PATH: &str = "ext.project_explorer_menus.show_path";
const CMD_OPEN: &str = "ext.project_explorer_menus.open";
const CMD_REVEAL: &str = "ext.project_explorer_menus.reveal";
const CMD_COPY_PATH: &str = "ext.project_explorer_menus.copy_path";
/// 설정 키(접두 = `settings_prefix`).
const KEY_LOG_PATH: &str = "ext.project_explorer_menus.log_path";

/// `log_path` 설정의 마지막 값(호스트 `on_settings`가 바뀔 때마다 준다 · 기본 켬).
static LOG_PATH: AtomicBool = AtomicBool::new(true);

struct ProjectExplorerMenus;

impl Extension for ProjectExplorerMenus {
    fn meta() -> Meta {
        Meta {
            id: "project-explorer-menus".into(),
            name: "Project Explorer Menus".into(),
            settings_prefix: "ext.project_explorer_menus.".into(),
            settings_read: vec![],
            settings_write: vec![],
            stateful: false,
            capabilities: vec![],
            events: vec![],
            prompts: vec![],
            commands: vec![
                Command::new(
                    CMD_SHOW_PATH,
                    Label::new(
                        "Project Explorer Menus: Show file path",
                        "Project Explorer Menus: 파일 경로 보기",
                    ),
                )
                .with_key("ctrl+alt+shift+p"),
                Command::new(
                    CMD_OPEN,
                    Label::new(
                        "Project Explorer Menus: Open in new tab",
                        "Project Explorer Menus: 새 탭으로 열기",
                    ),
                ),
                Command::new(
                    CMD_REVEAL,
                    Label::new(
                        "Project Explorer Menus: Reveal in Project Explorer",
                        "Project Explorer Menus: 프로젝트 탐색기에서 보기",
                    ),
                ),
                Command::new(
                    CMD_COPY_PATH,
                    Label::new(
                        "Project Explorer Menus: Copy file path",
                        "Project Explorer Menus: 파일 경로 복사",
                    ),
                ),
            ],
            menus: vec![
                // 편집기 우클릭(활성 문서 기준).
                Menu::editor(
                    "project-explorer-menus",
                    Label::new("Project Explorer Menus", "Project Explorer Menus"),
                    vec![
                        CMD_SHOW_PATH.into(),
                        CMD_REVEAL.into(),
                        CMD_COPY_PATH.into(),
                    ],
                ),
                // 프로젝트 탐색기 파일 행 우클릭(그 행의 경로 기준 · docs/97 §5 ①).
                Menu::project_explorer(
                    "project-explorer-menus-tree",
                    Label::new("Project Explorer Menus", "Project Explorer Menus"),
                    vec![
                        CMD_OPEN.into(),
                        CMD_REVEAL.into(),
                        CMD_COPY_PATH.into(),
                        CMD_SHOW_PATH.into(),
                    ],
                ),
            ],
            formatter: None,
            // 호스트 설정 창에 "Project Explorer Menus" 분류로 나타난다(T-258 · 146차).
            settings: vec![SettingDef::bool(
                KEY_LOG_PATH,
                Label::new("Log the file path", "파일 경로를 로그에"),
                Label::new(
                    "Show file path writes the path to the log window (developer mode, ext layer)",
                    "'파일 경로 보기'가 경로를 로그 창에 남깁니다(개발자 모드 · ext 층)",
                ),
                true,
            )],
        }
    }

    fn on_settings(s: &Settings) -> Effect {
        // 설정 하나(`log_path`)만 기억한다 · 편집기 옵션은 건드리지 않는다.
        LOG_PATH.store(
            s.get(KEY_LOG_PATH).is_none_or(|v| v != "off"),
            Ordering::Relaxed,
        );
        Effect::default()
    }

    fn run(cmd: &str, ed: &mut Editor) -> bool {
        // 상황 경로 = 편집기 메뉴면 활성 문서 · 프로젝트 탐색기 메뉴면 그 행(호스트 `run_with(doc_path)` → `nx_host_get`).
        let path = ed.doc_path();
        match cmd {
            CMD_SHOW_PATH => {
                if LOG_PATH.load(Ordering::Relaxed) {
                    match &path {
                        Some(p) => nexa_ext_sdk::log(&format!("file: {p}")),
                        None => nexa_ext_sdk::log("file: (unsaved script)"),
                    }
                }
                true
            }
            CMD_OPEN => path.is_some_and(|p| Files.open(&p)) || true,
            CMD_REVEAL => path.is_some_and(|p| Files.reveal(&p)) || true,
            CMD_COPY_PATH => path.is_some_and(|p| Files.copy_path(&p)) || true,
            _ => false,
        }
    }
}

nexa_ext_sdk::export_extension!(ProjectExplorerMenus);

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_ext_sdk::MenuTarget;

    /// 메타 = id·이름·명령·메뉴·설정이 서로 맞물린다(메뉴 항목 = 등록된 명령 · 설정 키 = 접두 · 탐색기 메뉴 하나).
    #[test]
    fn meta_wires_menus_settings_to_commands() {
        let m = ProjectExplorerMenus::meta();
        assert_eq!(m.id, "project-explorer-menus");
        assert_eq!(m.commands.len(), 4);
        for menu in &m.menus {
            assert!(menu
                .items
                .iter()
                .all(|i| m.commands.iter().any(|c| &c.id == i)));
        }
        assert!(m
            .menus
            .iter()
            .any(|x| x.target == MenuTarget::ProjectExplorer));
        assert!(m
            .commands
            .iter()
            .all(|c| c.id.starts_with(&m.settings_prefix)));
        assert!(m
            .settings
            .iter()
            .all(|s| s.key.starts_with(&m.settings_prefix)));
        assert_eq!(
            m.commands[0].key.as_deref(),
            Some("ctrl+alt+shift+p"),
            "기본 단축키"
        );
    }

    /// 호스트 밖(네이티브 시험)에서는 문서 경로가 없다 → 명령은 처리했다고 답하고 로그만 · 모르는 명령은 거절 · 설정 off는 로그 끔.
    #[test]
    fn run_handles_own_commands_only() {
        let mut ed = Editor;
        for c in [CMD_SHOW_PATH, CMD_OPEN, CMD_REVEAL, CMD_COPY_PATH] {
            assert!(ProjectExplorerMenus::run(c, &mut ed), "{c}");
        }
        assert!(!ProjectExplorerMenus::run("ext.other.cmd", &mut ed));
        let s = Settings(vec![(KEY_LOG_PATH.into(), "off".into())]);
        let _ = ProjectExplorerMenus::on_settings(&s);
        assert!(!LOG_PATH.load(Ordering::Relaxed));
        let s = Settings(vec![(KEY_LOG_PATH.into(), "on".into())]);
        let _ = ProjectExplorerMenus::on_settings(&s);
        assert!(LOG_PATH.load(Ordering::Relaxed));
    }
}
