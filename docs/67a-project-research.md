# 67a · 프로젝트/워크스페이스 관리 조사 원문(하위 에이전트 보고 · 2026-09-22 · 설계 = [67](67-project-workspace.md))

> 원문 그대로(출처 URL 포함 · 확인하지 못한 항목은 **(확인 필요)**). 요약·결정은 67에.

## 1. Sublime Text 4

### 1-1. `.sublime-project` vs `.sublime-workspace`
| 항목 | 내용 | 출처 |
|---|---|---|
| 두 파일의 역할 | `.sublime-project` = 프로젝트 정의(VCS에 커밋) · `.sublime-workspace` = "user specific data, such as the open files and the modifications to each"(커밋 안 함) | [S1] |
| `folders[]` 키 | 필수 `path` — **"may be relative to the project directory, or a fully qualified path"**(상대 경로 기준 = 프로젝트 파일이 있는 디렉터리) · 선택 `name`, `file_include/exclude_patterns`, `folder_include/exclude_patterns`, `binary_file_patterns`, `index_include/exclude_patterns`, `follow_symlinks`(bool) | [S1] |
| `settings` | Editor Settings 범주만 프로젝트에서 덮어씀 | [S1] |
| `build_systems` | 인라인 빌드 시스템 배열(`name` 필수) | [S1] |
| 프로젝트 파일 위치 | 폴더 안에 있어야 한다는 제약은 문서에 **없음** — 절대 경로를 쓰면 어디든 둘 수 있음(상대 경로만 프로젝트 파일 기준) | [S1] |
| 워크스페이스 파일 위치 | "Wherever there's a `.sublime-project` file, you will find an ancillary `.sublime-workspace` file too" — 프로젝트 파일 **옆**에 같은 이름으로 자동 생성 · 한 프로젝트에 워크스페이스 여러 개 가능(Project ▸ New Workspace for Project · Save Workspace As…) | [S2] |
| 워크스페이스 내용 | "opened files, pane layout, find history and more" · 미저장 버퍼는 `buffers[]` 안에 `contents` 문자열로 **인라인** 저장(세션 파일과 같은 구조) · build 4121부터 "Undo history is preserved in the session"(→ `undo_stack`) · 4213부터 파일이 디스크에서 바뀌었으면 undo 기록 복원 안 함 | [S2][S5][S6][S7] |
| Project 메뉴 명령 | Close Project · Save Project As… · Open Recent · Switch Project… · Quick Switch Project… · New Workspace for Project · Save Workspace As… · 사이드바 폴더 우클릭 "Remove Folder from Project" · Project ▸ "Add Folder to Project…"(우클릭/메뉴 명령 이름은 커뮤니티 문서 기준 **(확인 필요 — 공식 문서에 명령 목록 없음)**) | [S2] |

### 1-2. `hot_exit` — 값과 동작
- 기본 설정 주석: **"Exiting the application with hot_exit enabled will cause it to close immediately without prompting. Unsaved modifications and open files will be preserved and restored when next starting."** [S8][S9]
- ST4 값 셋(OdatNurd 인용, Build 4107 "Added options to hot_exit setting to control behavior when the last window is closed") [S8][S6]:
  - `"always"` — "Always perform a hot exit when the application exits. This includes when the last window is closed on relevant platforms."
  - `"only_on_quit"` — "Only perform a hot exit when the application is asked to exit, not when the last window is closed. This setting is only used on Windows and Linux."
  - `"disabled"` — "Disable hot exit."(ST3의 `false`에 해당 · 이때 미저장 탭은 종료 시 저장 여부를 묻고, 4121부터 레이아웃도 기억 안 함 → `remember_layout`으로 별도 제어)
- **어디에 쓰이는가(핵심 함정)** — 포럼 보고: "Closing a project window causes unsaved changes to be saved in the workspace… Closing the entire application (Quit/Exit) causes unsaved changes to be saved to Session.sublime_session, but **not** to the workspace." 즉 프로젝트 창을 **닫으면** 워크스페이스에, 앱을 **종료하면** 전역 세션에 들어감 → 다른 PC/마운트 안 된 드라이브에서 열면 유실 사례 [S10]. 프로젝트/워크스페이스 파일을 옮기면 버퍼가 사라진 사례도 있음 [S11]. Build 4151: "Fixed very large unsaved files being lost on hot exit; a prompt is now shown to save them"(거대 미저장 버퍼는 세션에 안 넣고 저장 프롬프트) [S6].
- Switch Project 시: 현재 프로젝트의 워크스페이스에 버퍼가 기록되고 다음 프로젝트 워크스페이스가 열림(위 "프로젝트 창 닫기" 경로와 같음 — 포럼 설명 기준 **(확인 필요)**).

### 1-3. 세션 파일 위치·기록 시점
- 데이터 디렉터리: Windows `%APPDATA%\Sublime Text` · macOS `~/Library/Application Support/Sublime Text` · Linux `~/.config/sublime-text`(ST3는 뒤에 ` 3`) [S3]. 세션은 그 아래 **`Local/`**: `Local/Session.sublime_session`(파일·폴더 이력 + 종료 시 버퍼) · `Local/Auto Save Session.sublime_session`(작업 중 주기적으로 쓰는 미저장 버퍼 스냅숏) · `Session.sublime_session-backup`도 관찰됨 [S12][S13].
- JSON 구조: 최상위 `windows[]` → `buffers[]` 각 항목에 `contents`(미저장 본문), `settings`, `file`(있으면), `undo_stack`; `expanded_folders`, `file_history`, `select_project` 등 [S12][S13][S14].
- **기록 주기는 공식 문서에 없음** — 커뮤니티 자료는 "quietly writes temporary copies … in the background"(Auto Save Session = 주기적) · 정확한 간격은 **(확인 필요)**. 정상 종료 시 `Session.sublime_session`을 덮어쓰고 Auto Save Session은 다음 시작 때 소비됨 [S12].

### 1-4. 사이드바 OPEN FILES / FOLDERS
- 토글: View ▸ Side Bar ▸ Show Open Files(명령 `toggle_show_open_files`, 설정 `show_open_files` — 이름은 포럼 기준 **(확인 필요)**) [S15].
- 배치: **한 스크롤 목록**에 OPEN FILES가 위, FOLDERS가 아래. FichteFoll: "You can not make the sidebar segment with the open files permanent, because it shares the space with the folder display." → 트리를 스크롤하면 Open Files가 위로 사라짐. wbond의 설계 근거: 파일이 많아지면 "it isn't possible to see all of the open files, nor the folder tree. Which then leads to multiple scroll areas." — 즉 Sublime은 **분리 스크롤 영역을 의도적으로 피하고** 열린 파일 수만큼 섹션이 그대로 늘어난 뒤 트리를 밀어 내림 [S16].

### 1-5. 충돌 복구
- 다음 시작 때 `Auto Save Session`의 버퍼를 **묻지 않고** 복원(커뮤니티 자료는 "prompt to recover"라고도 함 — 버전별 차이 **(확인 필요)**). 한계: 세션이 다른 기기로 옮겨지면 빈 창으로 열리고 세션이 덮어써져 버퍼가 사라짐 [S12]; ST3는 OS 재시작 시 강제 종료되어 워크스페이스가 안 써졌음("This has been fixed in ST4") [S11]; 4107 "Fixed a possible case where an update loses the current session", `close_deleted_files` 추가 [S6].

## 2. VS Code

### 2-1. 워크스페이스 파일·저장 위치
- `.code-workspace` JSON: `folders[{path, name}]`("absolute or relative paths" · "Relative paths are better when you want to share") · `settings` · `extensions` · `launch` · `tasks`. "Save Workspace As" = 새 위치 기준으로 상대 경로 재계산 · Add Folder to Workspace / Remove Folder from Workspace 명령 · 두 번째 폴더를 추가하면 자동으로 *untitled* 워크스페이스 생성, 창 닫을 때 삭제 확인 [V1][V2].
- 저장 위치(소스): `untitledWorkspacesHome = <userData>/Workspaces` · `appSettingsHome = <userData>/User` · `workspaceStorageHome = <userData>/User/workspaceStorage`(창별 UI 상태·확장 상태 SQLite) · `localHistoryHome = <userData>/User/History` · `backupHome = <userData>/Backups`. untitled 워크스페이스 id = `Date.now()+random`, 폴더는 `Workspaces/<id>/workspace.json`, 삭제 시 `workspaceStorage/<id>/obsolete` 표식 파일을 쓴 뒤 정리 [V3][V4][V5].

### 2-2. Hot Exit — 설정과 백업 메커니즘
- 문서: "By default, VS Code remembers unsaved changes to files when you exit." `files.hotExit` = `off` / `onExit`(마지막 창 닫힘(Win/Linux) 또는 `workbench.action.quit` 때 · "All windows without folders opened will be restored upon next launch") / `onExitAndWindowClose`(+ 폴더 창은 마지막이 아니어도) · 폴더 창까지 복원하려면 `window.restoreWindows: all`. 백업 폴더: `%APPDATA%\Code\Backups` · `~/Library/Application Support/Code/Backups` · `~/.config/Code/Backups` [V6].
- **백업 쓰기 시점**(`workingCopyBackupTracker.ts`): `onDidChangeContent` → 내용 버전 증가 후 `scheduleBackup`; `onDidChangeDirty` → dirty가 되면 예약, clean이 되면 `discardBackup`. 지연 = `DEFAULT_BACKUP_SCHEDULE_DELAYS { default: 1000, delayed: 2000 }` ms — 자동 저장 지연이 짧을 때는 2000 ms(저장과 백업의 경주 회피), untitled는 항상 1000 ms. 즉 **타이핑 디바운스 1 s(간격 타이머 아님)** [V7].
- **디스크 배치**(`workingCopyBackupService.ts`): `Backups/<workspace backup folder>/<scheme>/<hash(identifier)>` · 첫 줄 프리앰블 = `"<resource URI> <JSON meta>\n"`(최대 10,000 바이트 · 초과 시 메타 폐기) 다음에 본문 · 자원별 IO 큐로 직렬화 · 파일 서비스로 **atomic write** · 메모리 `ResourceMap` 모델로 `hasBackupSync` 판정 [V8]. 워크스페이스 폴더 이름: 워크스페이스는 `workspace.id`, 단일 폴더는 `md5(path)`(Linux는 대소문자 유지, 그 외 소문자화), 빈 창은 생성 id; 메타는 `stateService`(storage.json)에 · 비어 있거나 위치가 사라진 백업 폴더는 `deleteStaleBackup`으로 이동-삭제 [V9].
- **종료 흐름**(electron-browser 트래커): 대기 중 백업 취소 → `shouldBackupBeforeShutdown`: hot exit off면 백업 없음; `QUIT`/`RELOAD`는 항상 백업("backup because next start we restore all backups"); `CLOSE`는 폴더/워크스페이스가 열려 있고 `onExitAndWindowClose`이거나 Win/Linux 마지막 창일 때만; `LOAD`(워크스페이스 전환)는 `onExitAndWindowClose`일 때만. 백업이 됐으면 **프롬프트 없이 종료**, 안 됐거나 실패했을 때만 `confirmBeforeShutdown`(Save/Don't Save/Cancel). 브라우저판은 동기 검사만: dirty가 있는데 백업이 없으면 "Unload veto: pending backups" [V10][V11].
- **복원 UX**: 다음 시작 때 dirty 상태 그대로 **묵시 복원**(프롬프트 없음). `onExitAndWindowClose`에서는 "Only non-folder windows are restored on main process launch"(폴더 창은 그 폴더를 다시 열 때 백업이 붙음) · 복원되지 않은 백업은 폐기하지 않음("never want to discard backups that we know were not restored") [V9][V10].

### 2-3. Auto Save
- `files.autoSave` = `off` / `afterDelay`(`files.autoSaveDelay` 기본 1000 ms) / `onFocusChange` / `onWindowChange`; 기본값 = 네이티브 `off`, 웹 `afterDelay`; 부속 `files.autoSaveWorkspaceFilesOnly`, `files.autoSaveWhenNoErrors`(기본 false) [V6][V12].

### 2-4. Explorer 배치 — OPEN EDITORS
- `explorer.openEditors.visible`(기본 9, 최소 1 · "initial maximum number of editors shown") · `explorer.openEditors.minVisible`(기본 0 · 미리 확보하는 슬롯 수) · `explorer.openEditors.sortOrder` = `editorOrder`(기본)/`alphabetical`/`fullPath` [V12].
- `OpenEditorsView extends ViewPane`(id `workbench.explorer.openEditorsView`) — Explorer 뷰 컨테이너 안의 **독립 패널**. 항목 높이 22 px; `minimumBodySize = min(visible 설정, 항목 수) × 22`(세로 방향), `maximumBodySize = max(항목 수, minVisible) × 22`; 항목 수/설정이 바뀌면 `updateSize()`로 **동적 갱신** → 열린 편집기가 늘면 최대 `visible`개 높이까지 자라고 그 이상은 섹션 안에서 스크롤 [V13].
- 컨테이너 = `ViewPaneContainer` → `PaneView`(SplitView 기반) — 패널 사이 **드래그 가능한 sash**, 패널마다 자기 스크롤, 접기/펼치기, 초기 크기는 `weight` 비례, 순서 바꾸기 드래그 지원 · Outline·Timeline도 같은 컨테이너의 섹션이며 `...` 메뉴로 표시/숨김·드래그 정렬 [V14][V15].

## 3. JetBrains DataGrip(IntelliJ 플랫폼)

### 3-1. 프로젝트 개념
- 프로젝트 = "a combination of your data sources, query consoles and, attached directories with files … and associated settings"; 새 프로젝트는 DataGrip projects 디렉터리에 폴더 + `.idea/`(`dataSources.xml`, `db-forest-config.xml` 등) [J1].
- **Attach Directory to Project**(Files 도구창 도구 막대/우클릭) — "attached directories become associated with the project, their location on the disk remains unchanged" · 여러 디렉터리 가능 · "Detach Directory from Project"로 해제(디스크는 그대로) [J1][J2].
- Files 도구창의 **Scratches and Consoles** 노드: `Database Consoles`(데이터 소스별 하위 폴더 · 데이터 소스 생성 시 콘솔 하나 자동 생성·부착) + 스크래치. 콘솔 파일은 "stored in the **consoles** subdirectory of the IDE configuration directory"(프로젝트 밖) — 설정 디렉터리 = Windows `%APPDATA%\JetBrains\<product><version>`, macOS `~/Library/Application Support/JetBrains/…`, Linux `~/.config/JetBrains/…`; 스크래치는 같은 곳의 `scratches`(프로젝트 무관 · 버퍼 5개 순환) [J2][J3][J4][J5]. 실제 파일명 관례(`consoles/db/<uuid>/console.sql`)는 **(확인 필요)**.
- 2025.3에서 콘솔을 "query files"(`/queries` · 타 IDE는 `.idea/queries`)로 대체했다가 2025.3.1에서 **되돌림**(전역 데이터 소스 사용자에게 문제) [J6].

### 3-2. 미저장 개념 없음 — 자동 저장
- "DataGrip automatically saves changes"; 트리거 = 컴파일/실행/디버그/VCS 조작/파일·프로젝트 닫기/IDE 종료(고정, 설정 불가) + System Settings의 두 옵션: **"Save files when switching to a different application or a built-in terminal"** · **"Save files if the IDE is idle for N seconds"**(기본 N = 15 초로 알려짐 **(확인 필요 — 문서에 기본값 미기재)**) [J7][J8].
- 탭에 "unsaved" 표식은 선택(Editor Tabs ▸ Mark modified = 파란 점) — 기본 UX는 저장 개념을 숨김 [J7].
- **Safe write**: 이전 이름 "Use safe write (save changes to a temporary file first)" → 현재 System Settings **"Back up files before saving"**("creates a backup before saving, restoring from backup if the save fails … called 'safe write'") · 임시 파일에 쓴 뒤 교체 → 핫 리로드 도구와 충돌 사례 [J8][J9].
- **Local History** = 복구 수단: "automatically records your project's state as you edit code" · 저장 위치 = 시스템 디렉터리(`%LOCALAPPDATA%\JetBrains\<product>\LocalHistory`, macOS `~/Library/Caches/JetBrains/…`) 바이너리 · 기본 보존 **최근 5 작업일**(`localHistory.daysToKeep`) · 새 버전 설치 시 초기화 · "not guaranteed to persist" [J10][J4].
- 충돌 복구 = 자동 저장 + Local History 조합(별도 hot-exit 백업 계층 없음). 문서상 "changes will not be lost" — 단 마지막 자동 저장 이후 입력은 유실 가능 **(확인 필요)** [J7].

## 4. DBeaver

### 4-1. 프로젝트·워크스페이스
- Eclipse 워크스페이스 `workspace6/` 아래 `.metadata/` + 프로젝트 폴더(기본 **General**). 프로젝트 메타 = `<project>/.dbeaver/`: `data-sources.json`(연결 · `data-sources*.json` 패턴 모두 로드) · `credentials-config.json`(암호화 자격) · `tasks.json` · `project-settings.json` · `project-metadata.json`(리소스 속성 — 스크립트별 기본 데이터 소스가 여기에 · 이슈에서 "datasources should be identified by name, as the uid seems to change") [D1][D2][D3]. `project-metadata.json`의 정확한 스키마(리소스 경로 키 → `default-datasource`)는 **(확인 필요 — 소스 파일 경로가 바뀌어 원문 확인 실패)**.
- 프로젝트 여러 개 · 생성 시 "custom storage location" 지정 가능 · CLI로 외부 폴더를 프로젝트로 link/unlink · 활성 프로젝트는 이름 변경/삭제 불가 · 같은 이름으로 재생성하면 연결을 되살림 [D4].
- **Scripts 폴더** + 리소스 핸들러(Bookmarks · Diagrams · Scripts 세 폴더 · Project Explorer의 Configure로 저장 폴더 변경) · **Create ▸ Link Folder / Link File**로 외부 폴더·파일을 프로젝트에 연결(외부 스크립트 열기 · 기본 저장 폴더 지정) [D5][D6].

### 4-2. 자동 저장 설정(소스 확정)
| 설정(라벨) | 키 | 기본값 |
|---|---|---|
| Auto-save after any modifications | `SQLEditor.autoSaveOnChange` | false |
| **Auto-save editor on close** | `SQLEditor.autoSaveOnClose` | **true**(24.1.1부터 · PR #34256, 이슈 #29925 "crash로 스크립트 유실" 요청) |
| Save editor on query execute | `SQLEditor.autoSaveOnExecute` | false |
| Save/restore active schema | `SQLEditor.autoSaveActiveSchema` | true |
| Delete empty scripts on editor close | `script.delete.empty` | `DELETE_NEW` |
| Attach connections to scripts | `script.auto.connection.attach` | true |
| Create script folders(연결 폴더 구조대로) | `script.auto.connection.folders` | false |
| Read/Write connection binding from/in script header | `SQLEditor.script.bind.embedded.read/write` | true / false(`commentType` = NAME) |
[D7][D8][D9][D10]
- "Auto-save on focus lost"라는 DBeaver 고유 설정은 **없음**(질문의 명칭은 확인 안 됨) · 타이머 기반 자동 저장은 Eclipse 플랫폼 **General ▸ Editors ▸ Autosave**("Enable autosave for dirty editors" · 기본 끔 · 비활동 60 s · Eclipse 4.6 Neon부터 · 실제 파일에 직접 저장) [D11].
- 이슈 #39368(25.0.1): 닫을 때 프롬프트 없이 자동 저장되고 끌 수 없다는 불만 → "Closed as not planned" [D12].

### 4-3. 충돌 시 미저장 내용
- Eclipse 편집기 상태 복원은 **열린 편집기 목록만** 복원하고 dirty 본문은 보존하지 않음; DBeaver도 hot-exit 계층 없음 → 이슈 #22058 "restore unsaved sql queries after restart"(탭에 붙는 임시 파일 요청) 미해결 [D13]. SQL Console(파일 없음)은 "closing the console discards its contents" [D5]. 복구 수단 = Eclipse **Local History**(`.metadata/.plugins/org.eclipse.core.resources/.history` · 저장 때마다 상태 기록 · 설정 Days to keep files / Maximum entries per file / Maximum file size(MB)) — 저장한 적 있는 내용만 [D14].

### 4-4. 스크립트–연결 결합도
- 매우 긴밀: 스크립트마다 활성 연결이 리소스 속성으로 저장(`project-metadata.json`) + 선택적으로 스크립트 헤더 주석에 연결 이름 기록(`bind.embedded.*`) + 연결 폴더별 스크립트 폴더 자동 생성 옵션 + 연결 삭제 시 스크립트 연결 정리 · 편집기에서 연결을 바꿔도 "retaining the SQL text" [D5][D7][D8].

## 5. 기타 편집기·DB 클라이언트(요약)

| 도구 | 미저장 내용 보존 방식 | 복구 UX | 출처 |
|---|---|---|---|
| **Zed** | 워크스페이스 직렬화 SQLite(`…/Zed/db/0-stable/db.sqlite`)의 `editors.contents`에 dirty 버퍼 본문 저장 · "throttled to a 100ms" 이벤트(변경·저장) 기반 · 종료 때 직렬화 완료까지 대기 · `session.restore_unsaved_buffers`(기본 on) + `restore_on_startup`(`last_session`/`last_workspace`/`empty_tab`/`launchpad`) · 자동 저장 `autosave` = off/after_delay{ms}/on_focus_change/on_window_change | 묵시 복원(프롬프트 없음) · 한계: undo 기록 미보존 · **프로젝트 없는 빈 창은 미지원**(#17051 not planned) · 회귀 이슈 다수(#16336 · #52038) | [Z1][Z2][Z3] |
| **TablePlus** | "Reopen closed workspaces at tabs"로 탭 복원 · 본문은 "Automatically save queries while editing"이 켜져 있을 때만 유지 → 끄면 익명 탭이 **빈 채로** 복원(#2961) · 실행/저장 전 닫으면 유실(#3626 2025-07 미답) | 프롬프트 없음 · 사실상 미보존 | [T1][T2] |
| **Beekeeper Studio** | 연결·저장 쿼리·열린 탭 = 설정 폴더 `app.db`(SQLite) · 탭 복원 동작하나 워크스페이스 1개만 기억(#2666) · 대형 쿼리 본문 전체 저장이 부담이라는 논의(#2548) | 묵시 복원 | [B1][B2] |
| **Azure Data Studio** | VS Code 포크 → `files.hotExit`·Backups 구조 동일 **(확인 필요 — 별도 확인 안 함)** | — | — |
| **Neovim/Vim** | 스왑 파일(`.swp` · 파일 옆 또는 `'directory'`) — "updated after typing 200 characters or when you have not typed anything for four seconds"(`updatecount`/`updatetime`) · 변경분 블록 저장(원본 + 스왑으로 재구성) | 파일 열 때 **ATTENTION 프롬프트**(Open Read-Only/Edit/Recover/Delete/Quit/Abort) · `:recover`/`vim -r` · 복구 뒤 diff 확인 권고 · "???LINES MISSING" 가능 | [N1] |
| **Notepad++** | "Enable session snapshot and periodic backup" — 편집됐지만 저장 안 된 파일만 `%AppData%\Notepad++\backup\`에 **기본 7 초마다** 스냅숏 · `session.xml`이 미저장 탭 → 백업 파일 경로를 가리킴 · 수동 Save Session…에는 미저장 문서 미포함 · 매뉴얼: "unsaved changes … should always be considered as volatile" | 다음 시작 때 미저장 탭 그대로(빨간 아이콘) 묵시 복원 | [P1][P2][P3] |
| **Kate** | `.<file>.kate-swp`(파일 옆 · 원본과의 **차이(편집 로그)** · **15 초**마다 동기) — 한 번 저장된 파일만 · 21.04부터 세션 "stash"(미저장·Untitled 문서를 세션에 보관 · `katestashmanager`) — 정상 종료용이며 충돌·Plasma 세션 복원에서 유실 버그(#447384 · #449229 · #462112) | 파일 열 때 상단 띠 **Recover / Discard / View diff** | [K1][K2] |
| **DBeaver** | 위 §4 — 닫을 때 자동 저장이 기본 · 충돌 대비는 Local History만 | 없음 | — |

## 6. 비교표

| 도구 | 프로젝트 파일 위치 제약 | 다중 폴더 | 미저장 버퍼 저장 위치 | 저장 주기 | 충돌 복원 UX | 탐색기 "열린 파일" 섹션 |
|---|---|---|---|---|---|---|
| Sublime Text 4 | 어디든(상대 경로는 프로젝트 파일 기준) · 워크스페이스는 프로젝트 옆 고정 | ○ `folders[]` | 프로젝트 창 닫기 → `.sublime-workspace` `buffers[].contents` / 앱 종료 → `Local/Session.sublime_session` / 작업 중 → `Local/Auto Save Session.sublime_session` | 주기(간격 비공개) + 종료 이벤트 | 묵시 복원 · 거대 버퍼는 저장 프롬프트 | OPEN FILES 위 + FOLDERS, **한 스크롤 목록**(섹션이 파일 수만큼 늘어남 · 고정 불가) |
| VS Code | `.code-workspace` 어디든(상대/절대) · untitled = `<userData>/Workspaces/<id>/workspace.json` | ○ 멀티루트 | `<userData>/Backups/<md5(folder) 또는 workspace.id>/<scheme>/<hash>` 파일 하나씩(프리앰블+본문) | 타이핑 디바운스 1 s(자동 저장 짧으면 2 s) · dirty 해제 시 즉시 폐기 | 묵시 복원(hot exit) · 백업 실패 시만 저장 프롬프트 | OPEN EDITORS = 독립 ViewPane(22 px × ≤`visible` 9 · 동적) + sash로 조절 · 패널별 스크롤 |
| DataGrip | 프로젝트 = `.idea/` 디렉터리 · 어디든 생성 | ○ Attach Directory | 없음(자동 저장이 곧 저장) + LocalHistory(시스템 디렉터리) | 포커스 이탈 · 유휴 N초 · 실행/VCS/닫기 | 자동 저장 + Local History(5 작업일) · 프롬프트 없음 | Files 도구창: 프로젝트/부착 디렉터리 + Scratches and Consoles 노드(열린 파일 섹션 없음) |
| DBeaver | Eclipse 워크스페이스 안 프로젝트(외부 위치 링크 가능) | 프로젝트 여러 개 + Link Folder | 없음(닫을 때 자동 저장 기본 · Console은 폐기) | 닫기/실행 시 + Eclipse Autosave(60 s · 기본 끔) | 없음(Local History만 · #22058 미해결) | Project Explorer(Bookmarks/Diagrams/Scripts) · 열린 파일 섹션 없음 |
| Zed | 폴더 열기 = 워크스페이스(SQLite 직렬화) | ○ | SQLite `editors.contents` | 이벤트 + 100 ms 스로틀 | 묵시 복원 · 빈 창 미지원 | 프로젝트 패널만 |
| Notepad++ | 세션/워크스페이스 XML 어디든 | ○(가상 프로젝트) | `backup/` 폴더 파일 + `session.xml` 참조 | 7 s 주기 | 묵시 복원 | 없음(탭만) |
| Kate | 세션 | ○ 프로젝트 플러그인 | `.kate-swp`(차이 로그) + 세션 stash | 15 s 주기 | Recover/Discard/Diff 띠 | 문서 목록 플러그인(열린 파일) 별도 도구뷰 |
| Vim | — | — | `.swp` 변경 블록 | 200자 또는 4 s 유휴 | ATTENTION 프롬프트 | — |

## 7. 권장안(Nexa SQL 관점)

### (a) 미저장 버퍼 주기 보존
1. **모델 = VS Code 방식(버퍼당 스냅숏 파일 + 원자적 교체)을 기본으로**, Zed처럼 이벤트 스로틀. 근거: VS Code는 내용 변경 **디바운스 1 s**(untitled 항상 1 s), Zed는 100 ms 스로틀, Notepad++ 7 s, Kate 15 s, Vim 4 s/200자. 권장 = **입력 디바운스 1 s + 최대 대기(예 5 s) 상한**(계속 타이핑 중이어도 5 s마다 한 번은 씀 — VS Code는 상한이 없고 `onDidChangeContent`마다 재예약하므로 연속 타이핑 중 유실 창이 길어질 수 있음).
2. **파일 배치**: `NSQL_HOME/backups/<워크스페이스 해시>/<탭 id 또는 경로 해시>` · 첫 줄 프리앰블(원본 경로 · 접속 프로필/세션 컨텍스트 · 버전 · 커서) + 본문. 프리앰블 길이 상한(VS Code 10 KB) · dirty 해제(저장·되돌리기·탭 닫기)에 **즉시 폐기**. 쓰기 = 임시 파일 → rename(Sublime `atomic_save` · JetBrains safe write · VS Code atomic).
3. **크기 상한**: Sublime 4151처럼 거대 버퍼는 스냅숏 대신 **저장 프롬프트**로 전환(예 `editor.undo_giant_mb`와 같은 계열 설정 · 큰 파일 모드 L1/L2와 연동). Zed 실측: 50만 줄 직렬화 ≈ 200 ms(백그라운드 스레드) → 본문 통째 쓰기는 백그라운드 + 탭 단위 격리(61 §2 불변식과 일치).
4. **대안 = 차이 로그(Kate/Vim)**: 파일 크기와 무관하게 쓰기 비용이 작지만 원본이 바뀌면 재생 불가(Kate는 한 번 저장된 파일만 · Vim "LINES MISSING"). Nexa는 `TextBuf` 연산 기록(60)이 이미 있으니 **하이브리드**가 가능: 작은 버퍼 = 본문 스냅숏, 큰 파일 = 저장 시점 해시 + 연산 기록 파일(`undofile.rs`)을 스냅숏으로 승격. 단, 원본 해시 불일치 시 폐기 규칙(Sublime 4213 "undo not restored when file changed on disk")을 따를 것.
5. **위치 규칙 함정 회피**: Sublime의 "창 닫기 → 워크스페이스 / 앱 종료 → 전역 세션" 이중 경로가 유실의 원인. **한 곳**(사용자 데이터 폴더)만 쓰고 프로젝트 파일에는 미저장 본문을 넣지 않는다(VS Code·Zed·Notepad++ 모두 사용자 폴더). 프로젝트/워크스페이스 파일 이동·다른 PC로 동기화되어도 안전.
6. 세션 파일(열린 탭·레이아웃·활성 탭·접속)은 **별도 JSON**으로 종료·주기 저장하고, 백업 파일은 그 세션이 가리키기만(Notepad++ `session.xml` → `backup/`).

### (b) 충돌 복원 UX
- 현대 편집기(VS Code · Zed · Sublime · Notepad++)는 **묵시 복원**이 표준: 탭이 dirty 표식 그대로 돌아오고 프롬프트 없음. 프롬프트형(Vim ATTENTION · Kate 띠)은 파일을 **다시 열 때** 스왑을 발견하는 파일 중심 모델에서 필요.
- 권장: **기본 = 묵시 복원 + 비침입 안내**(상태줄/토스트 "복원된 미저장 탭 N개" · 클릭하면 목록) · 원본이 디스크에서 바뀐 경우만 탭 상단 띠로 **Keep restored / Reload from disk / Diff**(Kate식 · 이미 있는 `extfile.rs`/`merge3` 재사용). hot exit 설정은 VS Code 3값(`off`/`onExit`/`onExitAndWindowClose`)이 Sublime 3값과 등가 — `editor.hot_exit`로 한 키. 복원 못한 백업은 **삭제하지 않고** 보관(VS Code "never discard … not restored").
- DB 클라이언트 특수성: 복원된 탭의 **접속 컨텍스트**(프로필·전용 세션 CONNECT 여부·트랜잭션 상태)는 재접속하지 않고 "No connection + 이전 프로필 이름 표식"으로 복원(자동 재접속 금지 규칙 · 26 §8) — DBeaver의 스크립트–연결 바인딩(리소스 속성 + 헤더 주석)은 참고, 단 결합은 프로필 **이름**으로(uid 변동 이슈 #21567).

### (c) 열린 파일 + 폴더 탐색기 배치
- 두 계보: **Sublime = 한 스크롤 목록**(단순 · 열린 파일이 많으면 트리가 밀려 내려가고 헤더가 사라짐 → 사용자 불만 #28631) vs **VS Code = 독립 패널 + sash**(OPEN EDITORS 높이 = 항목 수 × 22 px, `visible`(9) 상한까지 자라고 넘치면 자체 스크롤 · `minVisible`로 최소 슬롯 · 사용자가 sash로 조절 · `...`로 숨김/순서 변경).
- 권장 = **VS Code식 두 패널을 한 컨테이너에**: 위 "OPEN TABS"(항목 수에 따라 자동 높이 · 상한 `explorer.open_files_max`(기본 9) · 넘치면 섹션 내부 스크롤 · 정렬 editorOrder/alphabetical) + 아래 FOLDERS/서버 트리(나머지 공간) · 경계 sash 드래그 · 헤더 클릭 접기 · 접힌 상태·높이는 워크스페이스 상태에 저장. 이미 있는 부품 재사용: nexa-ctl `DockLayout`/`ToolDock`(툴바 도크의 분할 규칙) · 탐색기 `ExplorerSet` 공용 스크롤(57차) — "통합 스크롤"이 꼭 필요하면 Sublime식으로 두 목록을 한 `Rows`에 이어 붙이되 **OPEN TABS 헤더를 sticky**로 고정해 Sublime의 단점(헤더 유실)을 피하는 절충안이 가능.

## Sources

**Sublime Text**
- [S1] https://www.sublimetext.com/docs/projects.html
- [S2] https://docs.sublimetext.io/guide/usage/file-management/projects.html
- [S3] https://www.sublimetext.com/docs/revert.html
- [S5] https://www.sublimetext.com/docs/settings.html
- [S6] https://www.sublimetext.com/download (ST4 changelog · builds 4107/4121/4151/4213)
- [S7] https://forum.sublimetext.com/t/new-api-buffer-clear-undo-stack/60561
- [S8] https://forum.sublimetext.com/t/hot-exit-not-working/62422
- [S9] https://docs.sublimetext.io/reference/settings.html · https://github.com/randy3k/sublime-default/blob/master/Preferences.sublime-settings
- [S10] https://forum.sublimetext.com/t/workspace-is-not-saved-when-exiting-sublime-text-hot-exit-quirk/59074
- [S11] https://forum.sublimetext.com/t/unsaved-file-buffers-wiped-when-project-workspace-file-moved/65806
- [S12] https://forum.sublimetext.com/t/restoring-files-saved-in-auto-save-session-sublime-session/25654
- [S13] https://softhints.com/recover-unsaved-files-sublime-linux-mac/ · https://www.codestudy.net/blog/where-does-sublime-store-unsaved-files/
- [S14] https://fileinfo.com/extension/sublime-workspace
- [S15] https://forum.sublimetext.com/t/open-files-in-left-side-bar-missing/5247
- [S16] https://forum.sublimetext.com/t/keep-sidebar-open-files-always-visible-horizontal-tabs-feature/28631
- 참고 이슈: https://github.com/sublimehq/sublime_text/issues/4912 · https://github.com/sublimehq/sublime_text/issues/3027

**VS Code**
- [V1] https://code.visualstudio.com/docs/editing/workspaces/workspaces
- [V2] https://code.visualstudio.com/docs/editing/workspaces/multi-root-workspaces
- [V3] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/platform/environment/common/environmentService.ts
- [V4] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/platform/environment/electron-main/environmentMainService.ts
- [V5] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/platform/workspaces/electron-main/workspacesManagementMainService.ts
- [V6] https://code.visualstudio.com/docs/editing/codebasics (Save/Auto Save · Hot Exit)
- [V7] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/workingCopy/common/workingCopyBackupTracker.ts
- [V8] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/workingCopy/common/workingCopyBackupService.ts
- [V9] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/platform/backup/electron-main/backupMainService.ts
- [V10] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/workingCopy/electron-browser/workingCopyBackupTracker.ts
- [V11] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/services/workingCopy/browser/workingCopyBackupTracker.ts
- [V12] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/contrib/files/browser/files.contribution.ts
- [V13] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/contrib/files/browser/views/openEditorsView.ts
- [V14] https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/browser/parts/views/viewPaneContainer.ts
- [V15] https://code.visualstudio.com/docs/getstarted/userinterface
- 참고 이슈: https://github.com/microsoft/vscode/issues/157668 · https://github.com/microsoft/vscode/issues/9509

**JetBrains DataGrip / IntelliJ**
- [J1] https://www.jetbrains.com/help/datagrip/working-with-projects.html
- [J2] https://www.jetbrains.com/help/datagrip/files-tool-window.html
- [J3] https://www.jetbrains.com/help/datagrip/query-consoles.html
- [J4] https://www.jetbrains.com/help/datagrip/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html
- [J5] https://www.jetbrains.com/help/datagrip/scratches.html
- [J6] https://blog.jetbrains.com/datagrip/2025/12/18/query-consoles-are-coming-back/ · https://blog.jetbrains.com/datagrip/2025/09/16/a-farewell-to-consoles/
- [J7] https://www.jetbrains.com/help/datagrip/saving-and-reverting-changes.html · https://www.jetbrains.com/help/idea/saving-and-reverting-changes.html
- [J8] https://www.jetbrains.com/help/datagrip/system-settings.html · https://www.jetbrains.com/help/idea/system-settings.html
- [J9] https://intellij-support.jetbrains.com/hc/en-us/community/posts/360002667619-Where-is-the-safe-write-setting-stored
- [J10] https://www.jetbrains.com/help/datagrip/local-history.html

**DBeaver / Eclipse**
- [D1] https://dbeaver.com/docs/dbeaver/Configuration-files-in-DBeaver/
- [D2] https://dbeaver.com/docs/dbeaver/Admin-Manage-Connections/
- [D3] https://github.com/dbeaver/dbeaver/issues/21567
- [D4] https://dbeaver.com/docs/dbeaver/Projects/
- [D5] https://dbeaver.com/docs/dbeaver/Script-Management/
- [D6] https://dbeaver.com/docs/dbeaver/Project-Explorer/ · https://dbeaver.com/docs/dbeaver/Projects-View/
- [D7] https://raw.githubusercontent.com/dbeaver/dbeaver/devel/plugins/org.jkiss.dbeaver.ui.editors.sql/src/org/jkiss/dbeaver/ui/editors/sql/SQLPreferenceConstants.java
- [D8] https://raw.githubusercontent.com/dbeaver/dbeaver/devel/plugins/org.jkiss.dbeaver.ui.editors.sql/src/org/jkiss/dbeaver/ui/editors/sql/internal/SQLEditorMessages.properties
- [D9] https://raw.githubusercontent.com/dbeaver/dbeaver/devel/plugins/org.jkiss.dbeaver.ui.editors.sql/src/org/jkiss/dbeaver/ui/editors/sql/internal/SQLEditorPreferencesInitializer.java · …/preferences/PrefPageSQLEditor.java
- [D10] https://github.com/dbeaver/dbeaver/issues/29925
- [D11] https://bugs.eclipse.org/bugs/show_bug.cgi?id=486644
- [D12] https://github.com/dbeaver/dbeaver/issues/39368
- [D13] https://github.com/dbeaver/dbeaver/issues/22058
- [D14] https://help.eclipse.org/latest/topic/org.eclipse.platform.doc.user/tasks/tasks-88.htm · https://help.eclipse.org/latest/topic/org.eclipse.platform.doc.user/reference/ref-14.htm

**기타**
- [Z1] https://github.com/zed-industries/zed/pull/13546 · [Z2] https://zed.dev/docs/configuring-zed · [Z3] https://github.com/zed-industries/zed/issues/17051 · https://github.com/zed-industries/zed/issues/16336 · https://github.com/zed-industries/zed/issues/52038
- [T1] https://github.com/TablePlus/TablePlus/issues/2961 · [T2] https://github.com/tableplus/tableplus/issues/3626
- [B1] https://docs.beekeeperstudio.io/support/data-location/ · [B2] https://docs.beekeeperstudio.io/user_guide/sql_editor/saving_queries/ · https://github.com/beekeeper-studio/beekeeper-studio/issues/2666 · https://github.com/beekeeper-studio/beekeeper-studio/issues/2548
- [N1] https://vimhelp.org/recover.txt.html
- [P1] https://npp-user-manual.org/docs/preferences/ · [P2] https://npp-user-manual.org/docs/session/ · [P3] https://community.notepad-plus-plus.org/topic/21782/faq-periodic-backup-vs-autosave-plugin
- [K1] https://www.ctrl.blog/entry/kate-swp.html · [K2] https://bugs.kde.org/show_bug.cgi?id=353654 · https://bugs.kde.org/show_bug.cgi?id=449229
