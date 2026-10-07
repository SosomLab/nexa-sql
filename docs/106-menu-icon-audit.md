# 106 메뉴 아이콘 점검표 (2026-10-07)

**목적** — 아이콘을 띄울 수 있는 모든 메뉴 항목(풀다운 · 우클릭 · 하위 메뉴)을 한 표로 모아, 개발자가 빠진 아이콘·잘못된 아이콘을 바로 그릴 수 있게 한다. (사용자 지적: 결과 우클릭 ▸ 고급 복사 ▸ CSV/텍스트/Markdown이 모두 같은 표 아이콘이고 JSON만 `{}`)

**원천 파일**
- 아이콘 그리기 = `crates/nexa-sql/src/toolicons.rs` (`mi_*` = 메뉴용 `MenuIcon` · 그 밖 = 툴바용 `ToolIcon`)
- 풀다운·팔레트 = `crates/nexa-sql/src/app/menus.rs` (`build_menus_with` · `palette_commands`) · 프로젝트 풀다운 = `app/project.rs`
- 결과 그리드 우클릭·열 머리·보기 모드 = `crates/nexa-sql/src/grid.rs` (`open_menu` · `filter_menu_items` · `cond_menu_items` · `open_header_menu` · `open_view_menu`)
- 결과 탭 = `results.rs` · 편집기 탭 = `editors.rs` · 편집기 본문 = nexa-ui `nexa-ctl/controls/editmenu.rs` + `app/format.rs` + `extensions/mod.rs`
- 객체 탐색기 = `explorer.rs` · `explorers.rs` · 프로젝트 탐색기 = `project_panel.rs` · 북마크 = `bookmarks_panel.rs` · 거터 북마크 = `main.rs` · Ctrl 링크 = `app/objlink.rs`
- 세션·트랜잭션·툴바·상태바 = `app/session.rs` · `app/tx.rs` · `app/toolbar.rs` · `app/menus.rs`(상태줄 팝업) · `app/files.rs`(닫기 확인)
- 보조 창 = `log_win.rs` · `txlog_win.rs` · `sessions_win.rs` · `conn_win.rs`
- 글 = `crates/nsql-i18n/src/lib.rs` (`Msg` 변형마다 `[영어, 한국어]`)

**그리는 길 두 가지(작업 전 확인)**
- 우클릭 메뉴(`nexa-ctl` `ContextMenu`) = `CtxItem::…​.with_icon(Some(toolicons::mi_*()))` — 알파 마스크 · 상태색 틴트. 체크(`with_checked`)·표식(`with_mark`)을 쓰는 항목은 같은 칸을 쓸 수 있으니 아이콘과 겹치는지 확인 필요.
- 풀다운(`nexa-ctl` `pulldown` `MenuEntry` → `ComboItem`) = `with_image(Rc<IconImage>)`(RGBA) 또는 `icon`(글리프 문자열)뿐 — **지금 nexa-sql은 풀다운에 아이콘을 하나도 넣지 않는다**(탭 메뉴의 활성 `✓` 글리프만). 풀다운에 넣으려면 `MenuIcon` 마스크 → `IconImage` 변환(틴트 색 결정) 부품이 먼저 필요하다.
- 툴바용 `ToolIcon`(`new_script` · `open_file` · `save_file` · `save_as` · `run_statement` · `run_all` · `connect` · `disconnect` · `log` · `refresh` · `fetch_stop` · `commit` · `rollback` · `undo` · `redo` · `tx_log` · `sessions`)은 도형 함수 / SVG 경로가 이미 있으므로 `menu_icon(shape_*)` · `material!`로 감싸면 메뉴 아이콘으로 바로 재사용할 수 있다(표의 "재사용" 표기).

## 1. 지금 있는 아이콘

### 1-1. 메뉴용 `mi_*` (`MenuIcon`)

| 함수 | 그리는 모양 | 쓰는 곳 |
|---|---|---|
| `mi_copy` | 겹친 사각형 두 장(외곽선) | 편집기·입력란 우클릭 복사 · 결과 우클릭 복사/머리글과 함께 복사 · `copybtn` 기본 상태 |
| `mi_check` | 체크 ✓ (짧은 획 + 긴 획) | `copybtn` "복사됨" 상태(메뉴 아님) |
| `mi_cut` | 가위(고리 둘 + 날 둘) | 편집기·입력란 우클릭 잘라내기 |
| `mi_paste` | 클립보드(판 + 집게) | 편집기·입력란 우클릭 붙여넣기 |
| `mi_select_all` | 점선 사각형(모서리 + 변 중앙 점) | 편집기·입력란 우클릭 전체 선택 · 결과 우클릭 전체 선택 |
| `mi_table` | 격자(표) | 결과 우클릭 ▸ 고급 복사 ▸ CSV · 텍스트 · Markdown(셋 다!) · 보기 모드 그리드 · TSV · CSV |
| `mi_braces` | 중괄호 `{ }` 한 쌍 | 고급 복사 ▸ JSON · 보기 모드 JSON |
| `mi_db` | 데이터베이스 원통 | 결과 우클릭 SQL로 복사 ▸ · 보기 모드 SQL ▸ 와 그 하위 5개 |
| `mi_files` | 겹친 문서 두 장(VS Code Explorer 느낌) | 활동 막대 객체 탐색기 · **보기 모드 텍스트 · Markdown(뜻 불일치)** |
| `mi_project` | 폴더 + 안의 트리 가지 | 활동 막대 프로젝트 탐색기 |
| `mi_bookmark` | 리본(위 둥근 띠 · 아래 V 파임) | 활동 막대 북마크 |
| `mi_outline` | 들여쓴 목록 세 줄 | 활동 막대 아웃라인 |
| `mi_extensions` | 붙은 네모 셋 + 떨어진 네모 | 활동 막대 확장 |
| `mi_gear` | 톱니(고리 + 이 8개) | 활동 막대 환경 설정 |
| `mi_search` | Material `search`(돋보기) | 활동 막대 파일 검색 |
| `mi_match_case` · `mi_match_word` · `mi_regex` | Material `match_case`(Aa) · `match_word`(ab_) · `regular_expression`(.*) | 찾기 막대 · 필터 틀 · 파일 검색 토글 |
| `mi_visibility` · `mi_dot_file` · `mi_path_match` | Material `visibility`(눈) · 점+획 · `folder_check` | 프로젝트 탐색기 필터 토글 |
| `mi_arrow_up` · `mi_arrow_down` | Material `arrow_upward` · `arrow_downward` | 찾기 막대 이전/다음 |
| `mi_close` · `mi_chevron_right` · `mi_chevron_down` | Material `close` · `chevron_right` · `expand_more` | 찾기 막대 닫기 · 바꾸기 줄 접기/펼치기 |
| `mi_in_selection` | Material `segment`(≡) | 찾기 막대 선택 범위에서 |
| `mi_find_replace` · `mi_replace_all` · `mi_preserve_case` | codicon `replace` · `replace-all` · `preserve-case` | 찾기 막대 바꾸기 줄 |

### 1-2. 툴바용 `ToolIcon` (메뉴에 재사용 가능)

| 함수 | 그리는 모양 | 쓰는 곳 |
|---|---|---|
| `new_script` | ＋ | 툴바 새 편집기 |
| `open_file` | 폴더 외곽선 + 탭 | 툴바 열기 |
| `save_file` · `save_as` | 플로피 · Material `save_as` | 툴바 저장 · 다른 이름으로 저장 |
| `run_statement` · `run_all` | ▷ 외곽선 · ▶ 채움 | 툴바 문장 실행 · 전체 실행 |
| `fetch_stop` | Material `stop_circle` | 툴바 실행 중지 · 결과 도구줄 가져오기 중지 |
| `commit` · `rollback` | Material `check` · `undo` | 툴바 커밋 · 롤백 |
| `undo` · `redo` | Material `undo` · `redo` | 툴바 편집 그룹 |
| `connect` · `disconnect` | Material `power` · `power_off` | 툴바 접속 · 해제 |
| `sessions` · `tx_log` · `log` | Material `dns` · 문서+줄 3 · ≡ | 툴바 세션 · 트랜잭션 로그 · 로그 창 |
| `view_mode` · `refresh` · `fetch_all` | 표 계열 6칸 · 원호+화살촉 · Material `select_all` | 결과 도구줄 |

## 2. 점검표

판정: **추가** = 아이콘이 없는데 있으면 좋음 · **교체** = 지금 아이콘이 글 뜻과 맞지 않음 · **맞음** = 그대로 · **아이콘 불필요** = 값 목록·체크/표식 항목·확인 버튼·제목 등.
제안 = Google Material Symbols 이름 / VS Code codicon 이름(같은 칸 `/`로) · "재사용 X" = 이미 있는 도형을 쓰면 됨.

### 2-1. 풀다운 메뉴

| 위치 | 항목 글(한국어) | Msg/명령 id | 지금 아이콘 | 제안 | 판정 |
|---|---|---|---|---|---|
| 풀다운 ▸ 파일 | 새 편집기 | `MnNew` / `file.new` | 없음 | `note_add` / `new-file` (재사용 `new_script`) | 추가 |
| 풀다운 ▸ 파일 | 열기… | `MnOpen` / `file.open` | 없음 | `folder_open` / `folder-opened` (재사용 `open_file`) | 추가 |
| 풀다운 ▸ 파일 | 저장 | `MnSave` / `file.save` | 없음 | `save` / `save` (재사용 `save_file`) | 추가 |
| 풀다운 ▸ 파일 | 다른 이름으로 저장… | `MnSaveAs` / `file.save_as` | 없음 | `save_as` / `save-as` (재사용 `save_as`) | 추가 |
| 풀다운 ▸ 파일 | SQL 파일 실행… | `MnRunFile` / `file.run_file` | 없음 | `play_circle` / `play-circle` | 추가 |
| 풀다운 ▸ 파일 | 탭 닫기 | `MnCloseTab` / `file.close_tab` | 없음 | `close` / `close` (재사용 `mi_close`) | 추가 |
| 풀다운 ▸ 파일 | 최근 파일(이름 + 경로 · 최대 8) | `file.recent:<n>` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 파일 | 종료 | `MnExit` / `file.exit` | 없음 | `logout` / `sign-out` | 추가 |
| 풀다운 ▸ 프로젝트 | 새 프로젝트 저장… | `MnProjectNew` / `project.new` | 없음 | `create_new_folder` / `new-folder` | 추가 |
| 풀다운 ▸ 프로젝트 | 프로젝트 열기… | `MnProjectOpen` / `project.open` | 없음 | `folder_open` / `folder-opened` | 추가 |
| 풀다운 ▸ 프로젝트 | 프로젝트 전환… | `MnProjectSwitch` / `project.switch` | 없음 | `swap_horiz` / `arrow-swap` | 추가 |
| 풀다운 ▸ 프로젝트 | 프로젝트 저장 | `MnProjectSave` / `project.save` | 없음 | `save` / `save` | 추가 |
| 풀다운 ▸ 프로젝트 | 프로젝트 다른 이름으로 저장… | `MnProjectSaveAs` / `project.save_as` | 없음 | `save_as` / `save-as` | 추가 |
| 풀다운 ▸ 프로젝트 | 프로젝트 닫기 | `MnProjectClose` / `project.close` | 없음 | `close` / `close-all` | 추가 |
| 풀다운 ▸ 프로젝트 | 프로젝트에 폴더 추가… | `MnProjectAddFolder` / `project.add_folder` | 없음 | `create_new_folder` / `new-folder` | 추가 |
| 풀다운 ▸ 편집 | 실행 취소 | `MnUndo` / `edit.undo` | 없음 | `undo` / `discard` (재사용 `undo`) | 추가 |
| 풀다운 ▸ 편집 | 다시 실행 | `MnRedo` / `edit.redo` | 없음 | `redo` / `redo` (재사용 `redo`) | 추가 |
| 풀다운 ▸ 편집 | 선택 되돌리기 ▸ | `MnGrpUndoSelection` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 되돌리기 | 선택 되돌리기(Soft Undo) | `MnSoftUndo` / `edit.soft_undo` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 되돌리기 | 선택 다시 실행(Soft Redo) | `MnSoftRedo` / `edit.soft_redo` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 잘라내기 | `MnCut` / `edit.cut` | 없음 | `content_cut` / 확인 필요 (재사용 `mi_cut`) | 추가 |
| 풀다운 ▸ 편집 | 복사 | `MnCopy` / `edit.copy` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 풀다운 ▸ 편집 | 붙여넣기 | `MnPaste` / `edit.paste` | 없음 | `content_paste` / 확인 필요 (재사용 `mi_paste`) | 추가 |
| 풀다운 ▸ 편집 | 전체 선택 | `MnSelectAll` / `edit.select_all` | 없음 | `select_all` / 확인 필요 (재사용 `mi_select_all`) | 추가 |
| 풀다운 ▸ 편집 | 선택 ▸ | `MnGrpSelection` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 선택 확장(다음 출현) | `MnExpandSelection` / `edit.expand_selection` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 다음 출현 건너뛰기 | `MnSkipOccurrence` / `edit.skip_occurrence` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 모든 출현 선택 | `MnSelectAllOccurrences` / `edit.select_all_occurrences` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 선택을 줄로 확장 | `MnSelectLine` / `edit.select_line` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 선택을 줄마다 나누기 | `MnSplitLines` / `edit.split_lines` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 위 줄에 커서 추가 | `MnAddCaretUp` / `edit.add_caret_up` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 아래 줄에 커서 추가 | `MnAddCaretDown` / `edit.add_caret_down` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 선택 | 괄호 안으로 선택 확장 | `MnExpandBrackets` / `edit.expand_brackets` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 괄호 ▸ | `MnGrpBrackets` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 괄호 | 완성… | `MnComplete` / `edit.complete` | 없음 | 확인 필요 / `symbol-keyword` | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 괄호 | 심볼로 이동… | `MnGotoSymbol` / `goto.symbol` | 없음 | 확인 필요 / `symbol-method` | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 괄호 | 짝 괄호로 이동 | `MnGotoBracket` / `edit.goto_bracket` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 괄호 | 이전 형제 괄호 | `MnBracketPrev` / `edit.bracket_prev` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 괄호 | 다음 형제 괄호 | `MnBracketNext` / `edit.bracket_next` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 괄호 | 상위 괄호 | `MnBracketParent` / `edit.bracket_parent` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 괄호 | 하위 괄호 | `MnBracketChild` / `edit.bracket_child` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 줄 ▸ | `MnGrpLine` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 | 들여쓰기 | `MnIndent` / `edit.indent` | 없음 | `format_indent_increase` / 확인 필요 | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 | 내어쓰기 | `MnUnindent` / `edit.unindent` | 없음 | `format_indent_decrease` / 확인 필요 | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 | 줄 위로 이동 | `MnSwapLineUp` / `edit.swap_line_up` | 없음 | `arrow_upward` / `arrow-up` | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 | 줄 아래로 이동 | `MnSwapLineDown` / `edit.swap_line_down` | 없음 | `arrow_downward` / `arrow-down` | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 | 줄 복제 | `MnDuplicateLine` / `edit.duplicate_line` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 | 줄 삭제 | `MnDeleteLine` / `edit.delete_line` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 | 줄 합치기 | `MnJoinLines` / `edit.join_lines` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 대소문자 변환 ▸ | `MnGrpConvertCase` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 대소문자 변환 | 대문자로 | `MnUpperCase` / `edit.upper_case` | 없음 | `uppercase` / `case-sensitive` | 추가 |
| 풀다운 ▸ 편집 ▸ 대소문자 변환 | 소문자로 | `MnLowerCase` / `edit.lower_case` | 없음 | `lowercase` / 확인 필요 | 추가 |
| 풀다운 ▸ 편집 | 주석 토글 | `MnToggleComment` / `edit.toggle_comment` | 없음 | `comment` / `comment` | 추가 |
| 풀다운 ▸ 편집 | 블록 주석 토글 | `MnToggleBlockComment` / `edit.toggle_block_comment` | 없음 | `code_blocks` / 확인 필요 | 추가 |
| 풀다운 ▸ 편집 | SQL 포맷 | `MnFormatSql` / `edit.format` | 없음 | `format_align_left` / 확인 필요 | 추가 |
| 풀다운 ▸ 편집 | 포맷터 골라 포맷… | `MnFormatWith` / `edit.format_with` | 없음 | `format_align_left` / 확인 필요 | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 포맷 미리보기 | `MnFormatPreview` / `edit.format_preview` | 없음 | `preview` / `open-preview` | 추가 |
| 풀다운 ▸ 편집 | 인텔리센스 캐시 새로 고침 | `MnIntelRefresh` / `intel.refresh` | 없음 | `refresh` / `refresh` (재사용 `refresh`) | 추가 |
| 풀다운 ▸ 편집 | 인텔리센스 캐시 새로 고침(이 서버) | `MnIntelRefreshServer` / `intel.refresh_server` | 없음 | `refresh` / `refresh` | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 인텔리센스 캐시 새로 고침(전 서버) | `MnIntelRefreshAll` / `intel.refresh_all` | 없음 | `sync` / `sync` | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 북마크 ▸ | `MnBookmarks` | 없음 | `bookmark` / `bookmark` (재사용 `mi_bookmark`) | 추가 |
| 풀다운 ▸ 편집 ▸ 북마크 | 북마크 토글 | `MnBmToggle` / `bookmark.toggle` | 없음 | `bookmark_add` / `bookmark` | 추가 |
| 풀다운 ▸ 편집 ▸ 북마크 | 다음 북마크 | `MnBmNext` / `bookmark.next` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 북마크 | 이전 북마크 | `MnBmPrev` / `bookmark.prev` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 북마크 | 북마크 줄 전부 선택(멀티커서) | `MnBmSelectAll` / `bookmark.select_all` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 북마크 | 북마크 이름 편집… | `MnBmLabel` / `bookmark.label` | 없음 | `edit` / `edit` | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 북마크 | 이 문서의 북마크 전부 제거 | `MnBmClearDoc` / `bookmark.clear_doc` | 없음 | `bookmark_remove` / `clear-all` | 추가 |
| 풀다운 ▸ 편집 ▸ 북마크 | 북마크 패널 | `MnBookmarksPanel` / `view.bookmarks` | 없음 | `bookmarks` / `bookmark` (재사용 `mi_bookmark`) | 추가 |
| 풀다운 ▸ 편집 | 찾기 ▸ | `MnGrpFind` | 없음 | `search` / `search` (재사용 `mi_search`) | 추가 |
| 풀다운 ▸ 편집 ▸ 찾기 | 찾기… | `MnFind` / `edit.find` | 없음 | `search` / `search` (재사용 `mi_search`) | 추가 |
| 풀다운 ▸ 편집 ▸ 찾기 | 바꾸기… | `MnReplace` / `edit.replace` | 없음 | `find_replace` / `replace` (재사용 `mi_find_replace`) | 추가 |
| 풀다운 ▸ 편집 ▸ 찾기 | 다음 찾기 | `MnFindNext` / `edit.find_next` | 없음 | `arrow_downward` / `arrow-down` (재사용 `mi_arrow_down`) | 추가 |
| 풀다운 ▸ 편집 ▸ 찾기 | 이전 찾기 | `MnFindPrev` / `edit.find_prev` | 없음 | `arrow_upward` / `arrow-up` (재사용 `mi_arrow_up`) | 추가 |
| 풀다운 ▸ 편집 | 줄로 이동… | `MnGotoLine` / `edit.goto_line` | 없음 | 확인 필요 / `go-to-file` | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 줄 끝 ▸ | `MnGrpLineEndings` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 끝 | Windows 줄끝 (CRLF) | `MnEolCrlf` / `eol.crlf` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 끝 | Unix 줄끝 (LF) | `MnEolLf` / `eol.lf` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 ▸ 줄 끝 | Mac OS 9 줄끝 (CR) | `MnEolCr` / `eol.cr` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 편집 | 환경 설정… | `MnPreferences` / `edit.prefs` | 없음 | `settings` / `gear` (재사용 `mi_gear`) | 추가 |
| 풀다운 ▸ 보기 | 명령 팔레트… | `MnCommandPalette` / `view.palette` | 없음 | `keyboard_command_key` / 확인 필요 | 추가 |
| 풀다운 ▸ 보기 | Goto Anything(탭·파일·명령·심볼)… | `MnGotoAnything` / `view.goto_anything` | 없음 | `search` / `go-to-file` | 추가 |
| 풀다운 ▸ 보기 | 오브젝트 탐색기 | `MnExplorer` / `view.explorer` | 없음 | `account_tree` / `database` (또는 재사용 `mi_files` = 활동 막대와 같게) | 추가 |
| 풀다운 ▸ 보기 | 객체 상세 | `MnObjectDetails` / `view.object_details` | 없음 | `info` / `info` | 추가 |
| 풀다운 ▸ 보기 | 출력(Output) | `MnOutput` / `view.output` | 없음 | `terminal` / `output` | 추가 |
| 풀다운 ▸ 보기 | 파일에서 찾기… | `MnSearchPanel` / `view.search` | 없음 | `search` / `search` (재사용 `mi_search`) | 추가 |
| 풀다운 ▸ 보기 | 프로젝트 탐색기 | `MnProjectPanel` / `view.project` | 없음 | `folder_special` / `folder` (재사용 `mi_project`) | 추가 |
| 풀다운 ▸ 보기 | 로그 창 | `MnLogWindow` / `view.log` | 없음 | `list_alt` / `output` (재사용 `log`) | 추가 |
| 풀다운 ▸ 보기 | 트랜잭션 로그 | `MnTxLogWindow` / `view.txlog` | 없음 | `receipt_long` / `history` (재사용 `tx_log`) | 추가 |
| 풀다운 ▸ 보기 | 세션 관리자 | `MnSessManager` / `view.sessions` | 없음 | `dns` / `server` (재사용 `sessions`) | 추가 |
| 풀다운 ▸ 보기 | 메모리 사용량 | `MnMemoryWindow` / `view.memory` | 없음 | `memory` / 확인 필요 | 추가 |
| 풀다운 ▸ 보기 | 변수 | `MnVariables` / `view.variables` | 없음 | `variables` / `symbol-variable` | 추가 |
| 풀다운 ▸ 보기 | 변수(결과 탭) | `MnShowVariables` / `vars.show` | 없음 | `variables` / `symbol-variable` | 아이콘 불필요 |
| 풀다운 ▸ 보기 | 변수를 스크립트로 | `MnVariablesScript` / `vars.script` | 없음 | `code` / `code` | 아이콘 불필요 |
| 풀다운 ▸ 보기 | 항상 위 | `MnAlwaysOnTop` / `view.on_top` | 없음 | `push_pin` / `pin` | 추가 |
| 풀다운 ▸ 보기 | 툴바 배치 초기화 | `MnResetToolbar` / `view.toolbar_reset` | 없음 | `restart_alt` / `discard` | 아이콘 불필요 |
| 풀다운 ▸ 보기 | 레이아웃 초기화 | `MnResetLayout` / `view.layout_reset` | 없음 | `restart_alt` / `discard` | 아이콘 불필요 |
| 풀다운 ▸ 보기 | 색 설정… | `MnColors` / `view.colors` | 없음 | `palette` / `symbol-color` | 추가 |
| 풀다운 ▸ 보기 | 단축키… | `MnKeys` / `view.keys` | 없음 | `keyboard` / `record-keys` | 추가 |
| 풀다운 ▸ 보기 | 테마 ▸ | `MnThemeMenu` | 없음 | `contrast` / `color-mode` | 추가 |
| 풀다운 ▸ 보기 ▸ 테마 | (테마 이름들 · 현재 ✓) | `view.theme:*` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 보기 | 언어 ▸ | `MnLanguageMenu` | 없음 | `language` / `globe` | 추가 |
| 풀다운 ▸ 보기 ▸ 언어 | (언어 이름들 · 현재 ✓) | `view.lang:*` | 없음 | — | 아이콘 불필요 |
| 풀다운 ▸ 실행 | 문장 실행 | `MnRunStatement` / `run.statement` | 없음 | `play_arrow` / `play` (재사용 `run_statement`) | 추가 |
| 풀다운 ▸ 실행 | 새 결과 탭에 문장 실행 | `MnRunStatementNewTab` / `run.statement_new_tab` | 없음 | `playlist_play` / 확인 필요 | 추가 |
| 풀다운 ▸ 실행 | 전체 실행 | `MnRunAll` / `run.all` | 없음 | `fast_forward` / `run-all` (재사용 `run_all`) | 추가 |
| 풀다운 ▸ 실행 | 실행 계획 | `MnExplain` / `run.explain` | 없음 | `account_tree` / `type-hierarchy` (확인 필요) | 추가 |
| 풀다운 ▸ 실행 | 커밋 | `MnCommit` / `run.commit` | 없음 | `check` / `check` (재사용 `commit`) | 추가 |
| 풀다운 ▸ 실행 | 롤백 | `MnRollback` / `run.rollback` | 없음 | `undo` / `discard` (재사용 `rollback`) | 추가 |
| 풀다운 ▸ 실행 | 접속 | `MnConnect` / `conn.toggle` | 없음 | `power` / `plug` (재사용 `connect`) | 추가 |
| 풀다운 ▸ 실행 | 접속 해제 | `MnDisconnect` / `conn.disconnect` | 없음 | `power_off` / `debug-disconnect` (재사용 `disconnect`) | 추가 |
| 풀다운 ▸ 실행 | 접속 정보 → Output | `MnSessInfo` / `session.info` | 없음 | `info` / `info` | 추가 |
| 풀다운 ▸ 탭 | (열린 탭 이름들 · 활성 ✓ · 미저장 강조) | `tab:<id>` | `✓` 글리프(활성만) | — | 아이콘 불필요 |
| 풀다운 ▸ 탭 | 탭 찾기… | `MnFindTab` / `tab.find` | 없음 | `search` / `search` | 추가 |
| 풀다운 ▸ 도움말 | 샘플 데이터 만들기(Demo 프로필)… | `MnDemoCreate` / `help.demo` | 없음 | `science` / `beaker` | 추가 |
| 풀다운 ▸ 도움말 | 라이선스… | `MnLicense` / `help.license` | 없음 | `license` / `law` | 추가 |
| 풀다운 ▸ 도움말 | Nexa SQL 정보 | `MnAbout` / `help.about` | 없음 | `info` / `info` | 추가 |

### 2-2. 결과 그리드 우클릭(셀)

| 위치 | 항목 글(한국어) | Msg/명령 id | 지금 아이콘 | 제안 | 판정 |
|---|---|---|---|---|---|
| 우클릭(결과 셀) | 복사 | `MnCopy` / `copy` | `mi_copy` | `content_copy` / `copy` | 맞음 |
| 우클릭(결과 셀) | 머리글과 함께 복사 | `MnCopyWithHeaders` / `copy_h` | `mi_copy` | `content_copy` / `copy` (구별하려면 `copy_all` · 확인 필요) | 맞음 |
| 우클릭(결과 셀) | 고급 복사 ▸ | `MnAdvancedCopy` / `adv` | 없음 | `copy_all` / `files` (확인 필요) | 추가 |
| 우클릭(결과 셀) ▸ 고급 복사 | CSV로 복사 | `MnCopyCsv` / `copy_csv` | `mi_table` | `csv` 또는 `table_chart` / `table` | 교체 |
| 우클릭(결과 셀) ▸ 고급 복사 | 텍스트로 복사 | `MnCopyText` / `copy_txt` | `mi_table` | `notes` 또는 `short_text` / `note` | 교체 |
| 우클릭(결과 셀) ▸ 고급 복사 | Markdown으로 복사 | `MnCopyMarkdown` / `copy_md` | `mi_table` | `markdown`(M↓) / `markdown` | 교체 |
| 우클릭(결과 셀) ▸ 고급 복사 | JSON으로 복사 | `MnCopyJson` / `copy_json` | `mi_braces` | `data_object` / `json` | 맞음 |
| 우클릭(결과 셀) | SQL로 복사 ▸ | `MnCopySql` / `copy_sql` | `mi_db` | `database` 또는 `code` / `database` | 맞음 |
| 우클릭(결과 셀) ▸ SQL로 복사 | SELECT | `MnCopySqlSelect` / `sql_select` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ SQL로 복사 | INSERT | `MnCopySqlInsert` / `sql_insert` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ SQL로 복사 | UPDATE | `MnCopySqlUpdate` / `sql_update` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ SQL로 복사 | DELETE | `MnCopySqlDelete` / `sql_delete` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ SQL로 복사 | MERGE | `MnCopySqlMerge` / `sql_merge` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) | 전체 선택 | `MnSelectAll` / `all` | `mi_select_all` | `select_all` / 확인 필요 | 맞음 |
| 우클릭(결과 셀) | 필터 ▸ | `MnFilter` / `filter` | 없음 | `filter_alt` / `filter` | 추가 |
| 우클릭(결과 셀) ▸ 필터 | 이 값만: {값} | `MnFilterEq` / `filter.eq` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 이 값 제외: {값} | `MnFilterNe` / `filter.ne` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 이 값만: N개 선택 | `MnFilterEqMany` / `filter.eq_sel` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 이 값 제외: N개 선택 | `MnFilterNeMany` / `filter.ne_sel` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 포함하는 글… | `MnFilterContains` / `filter.contains` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | …로 시작… | `MnFilterStarts` / `filter.starts` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 보다 큼… | `MnFilterGt` / `filter.gt` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 보다 작음… | `MnFilterLt` / `filter.lt` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 범위… | `MnFilterBetween` / `filter.between` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 이후(포함)… | `MnFilterAfter` / `filter.ge` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 이전(포함)… | `MnFilterBefore` / `filter.le` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 참만 | `MnFilterTrue` / `filter.true` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 거짓만 | `MnFilterFalse` / `filter.false` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 값 목록… | `MnFilterIn` / `filter.in` | 없음 | `list` / `list-unordered` | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 값 고르기 ▸ (값마다 체크) | `MnFilterPick` / `filter.pick`(`filter.pick:<n>`) | 없음(하위 = 체크) | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 ▸ 값 고르기 | … 값이 더 있음 - 값 목록…으로 | `MnFilterPickMore` / `filter.pick_more` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 값 목록 팝업… | `MnFilterValues` / `filter.values` | 없음 | `checklist` / `checklist` | 추가 |
| 우클릭(결과 셀) ▸ 필터 | 정규식… | `MnFilterRegex` / `filter.regex` | 없음 | `regular_expression` / `regex` (재사용 `mi_regex`) | 추가 |
| 우클릭(결과 셀) ▸ 필터 | NULL만 | `MnFilterNull` / `filter.null` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | NULL 아닌 것만 | `MnFilterNotNull` / `filter.notnull` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 필터 중 하나만 맞아도(OR) | `MnFilterOr` / `filter.or` | 없음(체크) | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 필터 | 이 열 필터 지우기 | `MnFilterClearCol` / `filter.clear_col` | 없음 | `filter_alt_off` / `clear-all` | 추가 |
| 우클릭(결과 셀) ▸ 필터 | 모든 필터 지우기 | `MnFilterClearAll` / `filter.clear` | 없음 | `filter_alt_off` / `clear-all` | 추가 |
| 우클릭(결과 셀) ▸ 필터 | 필터 조회 SQL 복사 | `MnFilterCopyQuery` / `filter.copy_query` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(결과 셀) ▸ 필터 | 필터로 서버 재조회 | `MnFilterRequery` / `filter.requery` | 없음 | `refresh` / `refresh` (재사용 `refresh`) | 추가 |
| 우클릭(결과 셀) | 조건 ▸ | `MnCond` / `cond` | 없음 | `rule` / 확인 필요 | 추가 |
| 우클릭(결과 셀) ▸ 조건 | 이 값: {값} | `MnCondEq` / `cond.eq` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 조건 | 이 값이 아닌: {값} | `MnCondNe` / `cond.ne` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 조건 | {값}(으)로 시작 | `MnCondStarts` / `cond.starts` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 조건 | {값} 포함 | `MnCondContains` / `cond.contains` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 조건 | {값}(으)로 끝남 | `MnCondEnds` / `cond.ends` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 조건 | NULL | `MnCondNull` / `cond.null` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) ▸ 조건 | NULL이 아닌 | `MnCondNotNull` / `cond.notnull` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 셀) | 참조 행 보기 | `MnFollowFk` / `obj.follow_fk` | 없음 | `link` / `link` | 추가 |
| 우클릭(결과 셀) | 값 보기… | `MnGeViewValue` / `grid.edit.view_value` | 없음 | `visibility` / `eye` (재사용 `mi_visibility`) | 추가 |
| 우클릭(결과 셀) | NULL로 | `MnGeSetNull` / `grid.edit.set_null` | 없음 | `format_clear` / 확인 필요 | 아이콘 불필요 |
| 우클릭(결과 셀) | 행 복제 | `MnGeDupRow` / `grid.edit.dup_row` | 없음 | `control_point_duplicate` / `copy` | 추가 |
| 우클릭(결과 셀) | 행 추가 | `MnGeInsertRow` / `grid.edit.insert_row` | 없음 | `add` / `add` | 추가 |
| 우클릭(결과 셀) | 행 삭제 | `MnGeDeleteRow` / `grid.edit.delete_row` | 없음 | `delete` / `trash` | 추가 |
| 우클릭(결과 셀) | 편집 되돌리기 | `MnGeUndo` / `grid.edit.undo` | 없음 | `undo` / `discard` (재사용 `undo`) | 추가 |
| 우클릭(결과 셀) | 편집 다시 하기 | `MnGeRedo` / `grid.edit.redo` | 없음 | `redo` / `redo` (재사용 `redo`) | 추가 |
| 우클릭(결과 셀) | 변경 목록… | `MnGeChanges` / `grid.edit.changes` | 없음 | `list` / `diff` | 추가 |
| 우클릭(결과 셀) | SQL 미리보기… | `MnGePreview` / `grid.edit.preview_sql` | 없음 | `preview` / `open-preview` | 추가 |
| 우클릭(결과 셀) | 변경 적용 | `MnGeApply` / `grid.edit.apply` | 없음 | `check` / `check` (재사용 `commit`) | 추가 |
| 우클릭(결과 셀) | 변경 전부 되돌리기 | `MnGeRevert` / `grid.edit.revert` | 없음 | `undo` / `discard` (재사용 `rollback`) | 추가 |

### 2-3. 결과 열 머리 우클릭 · 보기 모드 메뉴 · 결과 탭

| 위치 | 항목 글(한국어) | Msg/명령 id | 지금 아이콘 | 제안 | 판정 |
|---|---|---|---|---|---|
| 우클릭(열 머리) | 필터 항목(값 항목 없이 · 2-2 필터 ▸ 하위와 같음) | `filter.*` | 2-2와 같음 | 2-2와 같음 | 아이콘 불필요 |
| 우클릭(열 머리) | 오름차순 정렬 | `MnSortAsc` / `sort.asc` | 없음(체크) | `arrow_upward` / `arrow-up` (체크 칸과 겹침 확인 필요) | 아이콘 불필요 |
| 우클릭(열 머리) | 내림차순 정렬 | `MnSortDesc` / `sort.desc` | 없음(체크) | `arrow_downward` / `arrow-down` (체크 칸과 겹침 확인 필요) | 아이콘 불필요 |
| 우클릭(열 머리) | 정렬 해제 | `MnSortClear` / `sort.clear` | 없음 | — | 아이콘 불필요 |
| 우클릭(열 머리) | 객체 탐색기에서 보기 | `MnObjLinkReveal` / `obj.reveal` | 없음 | `account_tree` / `go-to-file` (또는 재사용 `mi_files`) | 추가 |
| 보기 모드 메뉴(결과 도구줄 ▦) | 그리드 | `MnViewGrid` / `view:grid` | `mi_table` | `grid_on` / `table` | 맞음 |
| 보기 모드 메뉴 | 텍스트 | `MnViewText` / `view:text` | `mi_files` | `notes` 또는 `short_text` / `note` | 교체 |
| 보기 모드 메뉴 | Markdown | `MnViewMarkdown` / `view:markdown` | `mi_files` | `markdown`(M↓) / `markdown` | 교체 |
| 보기 모드 메뉴 | JSON | `MnViewJson` / `view:json` | `mi_braces` | `data_object` / `json` | 맞음 |
| 보기 모드 메뉴 | TSV | `MnViewTsv` / `view:tsv` | `mi_table` | `table_rows` / 확인 필요 (그리드·CSV와 구별) | 교체 |
| 보기 모드 메뉴 | CSV | `MnViewCsv` / `view:csv` | `mi_table` | `csv` 또는 `table_chart` / `table` | 교체 |
| 보기 모드 메뉴 | SQL ▸ | `MnViewSql` / `view:sql` | `mi_db` | `database` / `database` | 맞음 |
| 보기 모드 메뉴 ▸ SQL | SELECT · INSERT · UPDATE · DELETE · MERGE (5개) | `MnCopySql*` / `view:sql_*` | `mi_db`(5개 모두 같은 원통) | — (부모와 중복 → 하위는 빼기 권장) | 아이콘 불필요 |
| 우클릭(결과 탭) | 이름 바꾸기… | `MnResultRename` / `rename` | 없음 | `edit` / `edit` | 추가 |
| 우클릭(결과 탭) | 실행 쿼리 복사 | `MnResultCopySql` / `copy_sql` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(결과 탭) | 결과 탭 닫기 | `MnResultCloseTab` / `close` | 없음 | `close` / `close` (재사용 `mi_close`) | 추가 |
| 우클릭(결과 탭) | 다른 탭 닫기 | `MnResultCloseOthers` / `close_others` | 없음 | `tab_close` / `close-all` | 아이콘 불필요 |
| 우클릭(결과 탭) | 오른쪽 탭 닫기 | `MnResultCloseRight` / `close_right` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 탭) | 탭 고정 / 탭 고정 해제 | `MnResultPin` · `MnResultUnpin` / `pin` | 없음 | `push_pin` / `pin` · `pinned` | 추가 |
| 우클릭(결과 탭) | 맨 앞으로 | `MnResultMoveFirst` / `first` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 탭) | 맨 뒤로 | `MnResultMoveLast` / `last` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 탭) | 첫 탭으로 | `MnResultFirstTab` / `go_first` | 없음 | — | 아이콘 불필요 |
| 우클릭(결과 탭) | 마지막 탭으로 | `MnResultLastTab` / `go_last` | 없음 | — | 아이콘 불필요 |

### 2-4. 편집기 본문 · 편집기 탭 · Ctrl 링크 · 거터

| 위치 | 항목 글(한국어) | Msg/명령 id | 지금 아이콘 | 제안 | 판정 |
|---|---|---|---|---|---|
| 우클릭(편집기 본문 · 입력란 공통) | 복사 | nexa-ctl `CtxCopy` / `copy` | `mi_copy` | `content_copy` / `copy` | 맞음 |
| 우클릭(편집기 본문 · 입력란 공통) | 잘라내기 | nexa-ctl `CtxCut` / `cut` | `mi_cut` | `content_cut` / 확인 필요 | 맞음 |
| 우클릭(편집기 본문 · 입력란 공통) | 붙여넣기 | nexa-ctl `CtxPaste` / `paste` | `mi_paste` | `content_paste` / 확인 필요 | 맞음 |
| 우클릭(편집기 본문 · 입력란 공통) | 전체 선택 | nexa-ctl `CtxSelectAll` / `select_all` | `mi_select_all` | `select_all` / 확인 필요 | 맞음 |
| 우클릭(편집기 본문 · SQL 탭) | 포맷 ▸ | `MnFormatGroup` / `format.group` | 없음 | `format_align_left` / 확인 필요 | 추가 |
| 우클릭(편집기 본문) ▸ 포맷 | SQL Format ({포맷터}) (기본 + 나머지) | `MnFormatWithName` / `format.use:<id>` | 없음 | — | 아이콘 불필요 |
| 우클릭(편집기 본문) ▸ 포맷 | 대문자로 | `MnUpperCase` / `edit.upper_case` | 없음 | `uppercase` / `case-sensitive` | 추가 |
| 우클릭(편집기 본문) ▸ 포맷 | 소문자로 | `MnLowerCase` / `edit.lower_case` | 없음 | `lowercase` / 확인 필요 | 추가 |
| 우클릭(편집기 본문) | 확장이 넣은 메뉴 ▸ 와 항목 | 확장 매니페스트 `menus` | 없음 | 확장이 정함(확인 필요) | 아이콘 불필요 |
| 우클릭(편집기 탭) | 계속 열어 두기 | `MnTabKeepOpen` / `keep_open` | 없음 | `push_pin` / `pin` | 추가 |
| 우클릭(편집기 탭) | 탭 이름 바꾸기… | `MnTabRename` / `rename` | 없음 | `edit` / `edit` | 추가 |
| 우클릭(편집기 탭) | 탭 닫기 | `MnCloseTab` / `close` | 없음 | `close` / `close` (재사용 `mi_close`) | 추가 |
| 우클릭(편집기 탭) | 왼쪽 탭 닫기 | `MnTabCloseLeft` / `close_left` | 없음 | — | 아이콘 불필요 |
| 우클릭(편집기 탭) | 오른쪽 탭 닫기 | `MnTabCloseRight` / `close_right` | 없음 | — | 아이콘 불필요 |
| 우클릭(편집기 탭) | 탭 모두 닫기 | `MnTabCloseAll` / `close_all` | 없음 | `tab_close` / `close-all` | 추가 |
| 우클릭(편집기 탭) | 파일 이름 복사 | `MnCopyFileName` / `copy_name` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(편집기 탭) | 파일 경로 복사 | `MnCopyFilePath` / `copy_path` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(편집기 탭) | 파일 위치 열기 | `MnTabReveal` / `reveal` | 없음 | `folder_open` / `folder-opened` | 추가 |
| 우클릭(편집기 탭) | 프로젝트 탐색기에서 보기 | `MnTabRevealProject` / `reveal_project` | 없음 | `folder_special` / `folder` (재사용 `mi_project`) | 추가 |
| 우클릭(Ctrl 객체 링크) | 설명 복사 | `MnObjLinkCopyDesc` / `objlink.copy_desc` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(Ctrl 객체 링크) | 이름 - 설명 복사 | `MnObjLinkCopyNameDesc` / `objlink.copy_name_desc` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(Ctrl 객체 링크) | 객체 탐색기에서 보기 | `MnObjLinkReveal` / `objlink.reveal` | 없음 | `account_tree` / `go-to-file` (또는 재사용 `mi_files`) | 추가 |
| 우클릭(Ctrl 객체 링크) | 다른 스키마의 같은 이름 ▸ (후보들) | `MnObjLinkCandidates` / `objlink.cands` | 없음 | — | 아이콘 불필요 |
| 우클릭(거터 북마크 영역) | 북마크 패널 | `MnBookmarksPanel` / `bmg.view` | 없음 | `bookmarks` / `bookmark` (재사용 `mi_bookmark`) | 추가 |
| 우클릭(거터 북마크 영역) | 북마크 토글(체크) | `MnBmToggle` / `bmg.toggle` | 없음(체크) | — | 아이콘 불필요 |
| 우클릭(거터 북마크 영역) | 니모닉 ▸ (0~9 체크) | `MnBmMnemonic` / `bmg.mn` | 없음 | — | 아이콘 불필요 |
| 우클릭(거터) ▸ 니모닉 | 니모닉 지우기 | `MnBmMnemonicClear` / `bmg.mn:clear` | 없음 | — | 아이콘 불필요 |

### 2-5. 객체 탐색기 · 프로젝트 탐색기 · 북마크 패널

| 위치 | 항목 글(한국어) | Msg/명령 id | 지금 아이콘 | 제안 | 판정 |
|---|---|---|---|---|---|
| 우클릭(객체 탐색기 · 객체) | 행 조회 | `ExpSelectRows` / `select` | 없음 | `table_view` / `table` | 추가 |
| 우클릭(객체 탐색기 · 테이블) | 데이터 가져오기… | `MnImportData` / `import` | 없음 | `upload_file` / 확인 필요 | 추가 |
| 우클릭(객체 탐색기 · 객체) | 소스 열기 | `ExpOpenSource` / `source` | 없음 | `code` / `code` | 추가 |
| 우클릭(객체 탐색기 · 패키지) | 본문 열기 | `ExpOpenBody` / `body` | 없음 | `code` / `code` (확인 필요) | 추가 |
| 우클릭(객체 탐색기 · 객체/잎) | SQL 생성 ▸ | `MnGenSql` / `gen` | 없음 | `database` / `database` (재사용 `mi_db`) | 추가 |
| 우클릭(객체 탐색기) ▸ SQL 생성 | SELECT · INSERT · UPDATE · DELETE · MERGE · CALL · DDL | `GenWhat::label` / `gen:<code>` | 없음 | — | 아이콘 불필요 |
| 우클릭(객체 탐색기 · 객체/잎/스키마/DB) | 이름 복사 | `ExpCopyName` / `copy` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(객체 탐색기 · 객체) | 삭제… | `ExpDropObject` / `drop` | 없음 | `delete` / `trash` | 추가 |
| 우클릭(객체 탐색기 · 전 노드) | 새로 고침 | `ExpRefresh` / `refresh` | 없음 | `refresh` / `refresh` (재사용 `refresh`) | 추가 |
| 우클릭(객체 탐색기 · 스키마/루트) | 메타 새로 고침(인텔리센스) | `ExpRefreshMeta` / `refresh_meta` | 없음 | `sync` / `sync` | 추가 |
| 우클릭(객체 탐색기 · 스키마/DB/묶음) | 용량 확인 | `ExpLoadSizes` / `sizes` | 없음 | `data_usage` / 확인 필요 | 추가 |
| 우클릭(객체 탐색기 · 오프라인 루트) | 연결 | `ExpConnectServer` / `connect` | 없음 | `power` / `plug` (재사용 `connect`) | 추가 |
| 우클릭(객체 탐색기 · 오프라인 루트) | 탐색기에서 제거 | `ExpRemoveServer` / `remove` | 없음 | `remove_circle_outline` / `remove` | 추가 |
| 우클릭(객체 탐색기 · 루트) | 이 연결로 새 탭 | `ExpNewTabHere` / `newtab` | 없음 | `add` / `add` (재사용 `new_script`) | 추가 |
| 우클릭(객체 탐색기 · 루트) | 이 연결 해제 / 연결 해제(이 서버의 모든 세션) | `ExpDisconnectConn` · `ExpDisconnectServer` / `disconnect` | 없음 | `power_off` / `debug-disconnect` (재사용 `disconnect`) | 추가 |
| 해제 메뉴(탐색기 서버 헤더) | {연결} 연결 해제 | `ExpDisconnectOne` / `disc:*` | 없음 | `power_off` / `debug-disconnect` | 추가 |
| 해제 메뉴(탐색기 서버 헤더) | 모두 해제({n}) | `ExpDisconnectAllN` / `disc:all` | 없음 | `power_off` / `debug-disconnect` | 추가 |
| 해제 메뉴(탐색기 서버 헤더) | 연결 해제 ▸ (연결별) | `ExpDisconnectPick` / `disc` | 없음 | `power_off` / `debug-disconnect` | 추가 |
| 우클릭(프로젝트 탐색기) | 이름 복사 | `MnCopyName` / `copy_name` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(프로젝트 탐색기) | 경로 복사 | `MnCopyPath` / `copy_path` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(프로젝트 탐색기) | 파일 위치 열기 | `MnRevealFile` / `reveal` | 없음 | `folder_open` / `folder-opened` | 추가 |
| 우클릭(프로젝트 탐색기 · 루트) | 프로젝트에서 폴더 제거 | `MnProjectRemoveFolder` / `remove_folder` | 없음 | `folder_delete` / `remove` | 추가 |
| 우클릭(북마크 패널 · 북마크) | 열기 | `MnBmOpen` / `open` | 없음 | `open_in_new` / `go-to-file` | 추가 |
| 우클릭(북마크 패널 · 북마크) | 북마크 이름 편집… | `MnBmLabel` / `rename` | 없음 | `edit` / `edit` | 추가 |
| 우클릭(북마크 패널 · 북마크) | 니모닉 ▸ (0~9 · 지우기) | `MnBmMnemonic` / `mn` | 없음 | — | 아이콘 불필요 |
| 우클릭(북마크 패널 · 북마크) | 그룹으로 이동 ▸ (그룹들 · 새 그룹) | `MnBmMoveGroup` / `grp` | 없음 | `drive_file_move` / 확인 필요 | 아이콘 불필요 |
| 우클릭(북마크 패널 · 북마크) | 제거 | `MnBmRemove` / `remove` | 없음 | `bookmark_remove` / `trash` | 추가 |
| 우클릭(북마크 패널 · 문서) | 이 문서 열기 | `MnBmDocOpen` / `doc.open` | 없음 | `open_in_new` / `go-to-file` | 추가 |
| 우클릭(북마크 패널 · 문서) | 이 문서의 북마크 전부 멀티커서로 | `MnBmDocSelectAll` / `doc.select` | 없음 | — | 아이콘 불필요 |
| 우클릭(북마크 패널 · 문서) | 이 문서의 북마크 전부 제거 | `MnBmDocRemoveAll` / `doc.remove` | 없음 | `delete` / `trash` | 추가 |
| 우클릭(북마크 패널 · 그룹) | 그룹 이름 바꾸기 | `MnBmGroupRename` / `grp.rename` | 없음 | `edit` / `edit` | 추가 |
| 우클릭(북마크 패널 · 그룹) | 기본 그룹으로 지정 | `MnBmGroupDefault` / `grp.default` | 없음 | — | 아이콘 불필요 |
| 우클릭(북마크 패널 · 그룹) | 그룹 켜기 / 그룹 끄기 | `MnBmGroupEnable` · `MnBmGroupDisable` / `grp.toggle` | 없음 | `toggle_on` · `toggle_off` / 확인 필요 | 아이콘 불필요 |
| 우클릭(북마크 패널 · 그룹/빈 곳) | 새 그룹 | `MnBmNewGroup` / `new_group` | 없음 | `create_new_folder` / `new-folder` | 추가 |
| 우클릭(북마크 패널 · 그룹) | 그룹 삭제(항목은 기본 그룹으로) | `MnBmGroupDelete` / `grp.delete` | 없음 | `delete` / `trash` | 추가 |
| 우클릭(북마크 패널 · 그룹) | 그룹과 항목까지 삭제 | `MnBmGroupDeleteAll` / `grp.delete_all` | 없음 | `delete_forever` / `trash` | 추가 |
| 우클릭(북마크 패널 · 빈 곳) | 무효 북마크 전부 지우기 | `MnBmRemoveInvalid` / `remove_invalid` | 없음 | `delete_sweep` / `clear-all` | 추가 |
| 우클릭(북마크 패널 · 빈 곳) | 북마크 설정 열기… | `MnBmSettings` / `settings` | 없음 | `settings` / `gear` (재사용 `mi_gear`) | 추가 |

### 2-6. 세션 · 트랜잭션 · 툴바 · 상태줄 팝업 · 확인 팝업

| 위치 | 항목 글(한국어) | Msg/명령 id | 지금 아이콘 | 제안 | 판정 |
|---|---|---|---|---|---|
| 세션 배지 메뉴(탭 표식) | 연결 없음(표식) | `MnSessNoConnection` / `sess.none` | 없음(표식) | — | 아이콘 불필요 |
| 세션 배지 메뉴 | (공유 연결 목록 · 표식) | `sess.use:<id>` | 없음(표식) | — | 아이콘 불필요 |
| 세션 배지 메뉴 | (전용 세션 · 표식) | `sess.private` | 없음(표식) | — | 아이콘 불필요 |
| 세션 배지 메뉴 | 서버 유형 - 이 세션만(임시) (머리글) | `MnEnvSessionHdr` / `sess.env.hdr` | 없음 | — | 아이콘 불필요 |
| 세션 배지 메뉴 | 서버 유형: 없음/개발/테스트/운영 (표식) | `MnEnvNone`·`MnEnvDev`·`MnEnvTest`·`MnEnvProd` / `sess.env:*` | 없음(표식) | — | 아이콘 불필요 |
| 세션 배지 메뉴 | 접속 정보 → Output | `MnSessInfo` / `sess.info` | 없음 | `info` / `info` | 추가 |
| 툴바 DB 목록(▾) | (데이터베이스 이름들 · 표식) / (목록 없음…) | `sess.db:*` · `MnSessUseNoList` | 없음 | — | 아이콘 불필요 |
| 툴바 Disconnect ▾ | {n}개 탭 모두 연결 해제 | `MnDiscAll` / `disc.all` | 없음 | `power_off` / `debug-disconnect` (재사용 `disconnect`) | 추가 |
| 툴바 Disconnect ▾ | 이 탭만 떼기(No connection) | `MnDiscDetach` / `disc.detach` | 없음 | `link_off` / 확인 필요 | 추가 |
| 툴바 Disconnect ▾ | 취소 | `MnTxCancel` / `disc.cancel` | 없음 | — | 아이콘 불필요 |
| 상태줄 트랜잭션 메뉴 | 자동 커밋(표식) | `MnTxAuto` / `tx.auto` | 없음(표식) | — | 아이콘 불필요 |
| 상태줄 트랜잭션 메뉴 | 수동 커밋(표식) | `MnTxManual` / `tx.manual` | 없음(표식) | — | 아이콘 불필요 |
| 상태줄 트랜잭션 메뉴 | 커밋 ({n}) | `MnTxCommitN` / `tx.commit` | 없음 | `check` / `check` (재사용 `commit`) | 추가 |
| 상태줄 트랜잭션 메뉴 | 롤백 ({n}) | `MnTxRollbackN` / `tx.rollback` | 없음 | `undo` / `discard` (재사용 `rollback`) | 추가 |
| 상태줄 트랜잭션 메뉴 | (대기 문장 목록) | `tx.noop` | 없음 | — | 아이콘 불필요 |
| 트랜잭션 확인 팝업 | 커밋하고 탭 닫기/접속 해제/종료/자동 커밋으로 | `MnTxCommitClose`·`…Disconnect`·`…Exit`·`…Switch` / `tx.commit_then` | 없음 | `check` / `check` (재사용 `commit`) | 추가 |
| 트랜잭션 확인 팝업 | 롤백하고 탭 닫기/접속 해제/종료/자동 커밋으로 | `MnTxRollbackClose`·`…Disconnect`·`…Exit`·`…Switch` / `tx.rollback_then` | 없음 | `undo` / `discard` (재사용 `rollback`) | 추가 |
| 트랜잭션 확인 팝업 | 취소 | `MnTxCancel` / `tx.cancel` | 없음 | — | 아이콘 불필요 |
| 우클릭(툴바) | (누른 버튼의 공통 명령) | `tbcmd:<cmd>` | 없음 | 명령마다 다름 · 확인 필요 | 아이콘 불필요 |
| 우클릭(툴바) | (누른 버튼 표시 · 체크) | `tb:<id>` | 없음(체크) | — | 아이콘 불필요 |
| 우클릭(툴바) | {그룹} - 띄우기 / 붙이기 | `MnFloatGroup` · `MnDockGroup` / `tbg:<gid>` | 없음 | `open_in_new` · `dock_to_bottom` / 확인 필요 | 아이콘 불필요 |
| 우클릭(툴바) | 툴바 설정… | `MnToolbarSettings` / `tb.settings` | 없음 | `settings` / `gear` (재사용 `mi_gear`) | 추가 |
| 우클릭(툴바) | 툴바 배치 초기화 | `MnResetToolbar` / `tb.reset` | 없음 | `restart_alt` / `discard` | 추가 |
| 우클릭(상태바) | "{칸}" 숨기기 | `MnStatusHide` / `sb.hide:<id>` | 없음 | `visibility_off` / `eye-closed` | 아이콘 불필요 |
| 우클릭(상태바) | 상태바 설정… | `MnStatusSettings` / `sb.settings` | 없음 | `settings` / `gear` (재사용 `mi_gear`) | 추가 |
| 우클릭(상태바) | 상태바 항목 기본값 | `MnStatusReset` / `sb.reset` | 없음 | `restart_alt` / `discard` | 추가 |
| 상태줄 들여쓰기 팝업 | 공백으로/탭으로 들여쓰기 · 탭 폭: n (표식) | `MnIndentSpaces`·`MnIndentTabs`·`MnTabWidth` / `indent.*` | 없음(표식) | — | 아이콘 불필요 |
| 상태줄 들여쓰기 팝업 | 들여쓰기를 공백으로/탭으로 변환 | `MnConvertToSpaces`·`MnConvertToTabs` / `indent.to_*` | 없음 | — | 아이콘 불필요 |
| 상태줄 줄끝 팝업 | Windows/Unix/Mac OS 9 줄끝 (표식) | `MnEolCrlf`·`MnEolLf`·`MnEolCr` / `eol.*` | 없음(표식) | — | 아이콘 불필요 |
| 상태줄 인코딩 팝업 | (인코딩 이름들) · 다른 인코딩으로 다시 열기 ▸ | `enc.*` · `MnReopenEnc` / `enc.reopen` | 없음 | — | 아이콘 불필요 |
| 상태줄 자동 저장 팝업 | 프로젝트 폴더 열기 | `MnAutosaveProject` / `autosave.project` | 없음 | `folder_open` / `folder-opened` | 추가 |
| 상태줄 자동 저장 팝업 | 일반 파일 자동 저장 위치 열기 | `MnAutosaveBackups` / `autosave.backups` | 없음 | `folder_open` / `folder-opened` | 추가 |
| 상태줄 자동 저장 팝업 | 자동 저장 설정… | `MnAutosaveSettings` / `autosave.settings` | 없음 | `settings` / `gear` (재사용 `mi_gear`) | 추가 |
| 상태줄 자동 저장 팝업 | 프로젝트 저장 | `MnProjectSave` / `autosave.save_project` | 없음 | `save` / `save` (재사용 `save_file`) | 추가 |
| 탭 닫기 확인 팝업 | 저장하고 닫기 | `MnCloseSave` / `close.save` | 없음 | `save` / `save` | 추가 |
| 탭 닫기 확인 팝업 | 저장하지 않고 닫기 | `MnCloseDiscard` / `close.discard` | 없음 | — | 아이콘 불필요 |
| 탭 닫기 확인 팝업 | 모두 저장({n}개 탭) | `MnCloseSaveAll` / `close.save_all` | 없음 | `save` / `save-all` | 추가 |
| 탭 닫기 확인 팝업 | 모두 취소({n}개 탭 · 변경 버리고 닫기) | `MnCloseDiscardAll` / `close.discard_all` | 없음 | — | 아이콘 불필요 |
| 탭 닫기·종료 확인 팝업 | 취소 | `MnCloseCancel` / `close.cancel` | 없음 | — | 아이콘 불필요 |
| 종료 확인 팝업(프로젝트) | 프로젝트 저장 후 종료 | `MnProjectExitSave` / `project.exit_save` | 없음 | `save` / `save` | 추가 |
| 종료 확인 팝업(프로젝트) | 프로젝트 저장 없이 종료 | `MnProjectExitSkip` / `project.exit_skip` | 없음 | — | 아이콘 불필요 |
| 여러 파일 열기 팝업 | 파일 {n}개 열기 | `MultiOpenGo` / `multi.open` | 없음 | `folder_open` / `folder-opened` | 아이콘 불필요 |
| 여러 파일 열기 팝업 | 취소 | `BtnCancel` / `multi.cancel` | 없음 | — | 아이콘 불필요 |
| 데모 안내 팝업 | 로컬 SQLite 데모를 만들까요?… / 지금 만들기 / 나중에 | `DemoAsk`·`DemoYes`·`DemoLater` / `demo.*` | 없음 | — | 아이콘 불필요 |

### 2-7. 보조 창(로그 · 트랜잭션 로그 · 세션 · 접속)

| 위치 | 항목 글(한국어) | Msg/명령 id | 지금 아이콘 | 제안 | 판정 |
|---|---|---|---|---|---|
| 우클릭(로그 창) | 보이는 종류 ▸ (종류마다 체크) | `MnLogKinds` / `kinds` | 없음 | — | 아이콘 불필요 |
| 우클릭(로그 창) | 컬럼 ▸ (컬럼마다 체크) | `MnLogColumns` / `cols` | 없음 | — | 아이콘 불필요 |
| 우클릭(로그 창) | 개발자 레이어 ▸ (레이어마다 체크) | `MnLogDevLayers` / `layers` | 없음 | — | 아이콘 불필요 |
| 우클릭(로그 창) | 선택 복사 | `MnLogCopySel` / `copy_sel` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(로그 창) | 보이는 줄 복사 | `MnLogCopyAll` / `copy` | 없음 | `copy_all` / `copy` | 추가 |
| 우클릭(로그 창) | 로그를 파일로 저장… | `MnLogSaveAs` / `save` | 없음 | `save_as` / `save-as` (재사용 `save_as`) | 추가 |
| 우클릭(로그 창) | 지우기 | `MnLogClear` / `clear` | 없음 | `delete_sweep` / `clear-all` | 추가 |
| 우클릭(트랜잭션 로그 창) | 문장 복사 | `MnTxCopySql` / `copy` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(트랜잭션 로그 창) | 새 편집기 탭으로 | `MnTxOpenSql` / `open` | 없음 | `open_in_new` / `go-to-file` | 추가 |
| 우클릭(트랜잭션 로그 창) | 값 적용 문장 복사 | `MnTxCopyBoundSql` / `copy_bound` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(트랜잭션 로그 창) | 값 적용 문장을 새 탭으로 | `MnTxOpenBoundSql` / `open_bound` | 없음 | `open_in_new` / `go-to-file` | 추가 |
| 우클릭(세션 창) | 활성 연결로 | `MnSessActivate` / `use` | 없음 | `radio_button_checked` / 확인 필요 | 아이콘 불필요 |
| 우클릭(세션 창) | 탭으로 | `MnSessGoTab` / `tab` | 없음 | `tab` / 확인 필요 | 아이콘 불필요 |
| 우클릭(세션 창) | 다시 접속 | `MnSessReconnect` / `again` | 없음 | `refresh` / `debug-restart` | 추가 |
| 우클릭(세션 창) | 접속 해제 | `MnDisconnect` / `drop` | 없음 | `power_off` / `debug-disconnect` (재사용 `disconnect`) | 추가 |
| 우클릭(세션 창) | 모두 해제 | `MnSessDisconnectAll` / `drop_all` | 없음 | `power_off` / `debug-disconnect` (재사용 `disconnect`) | 추가 |
| 접속 창 폼 변경 확인 팝업 | 폼에 저장하지 않은 변경이 있습니다(제목) | `MnGuardTitle` / `guard_title` | 없음 | — | 아이콘 불필요 |
| 접속 창 폼 변경 확인 팝업 | 변경 저장 | `MnGuardSave` / `guard_save` | 없음 | `save` / `save` | 아이콘 불필요 |
| 접속 창 폼 변경 확인 팝업 | 변경 버리기 · 취소 | `MnGuardDiscard` · `MnGuardCancel` / `guard_*` | 없음 | — | 아이콘 불필요 |
| 우클릭(접속 창 콤보) | 선택 항목 복사 | `MnCopyItem` / `combo_item` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(접속 창 콤보) | 목록 복사 | `MnCopyList` / `combo_list` | 없음 | `copy_all` / `copy` | 추가 |
| 우클릭(접속 창 목록 빈 곳) | 새로 만들기 | `BtnNew` / `new` | 없음 | `add` / `add` | 추가 |
| 우클릭(접속 창 프로필) | 복제 | `MnDuplicate` / `dup` | 없음 | `control_point_duplicate` / `copy` | 추가 |
| 우클릭(접속 창 프로필) | 접속 문자열 복사 | `MnCopyConnString` / `copy_cs` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(접속 창 프로필) | 서버 유형 - 프로필에 저장(머리글) · 없음/개발/테스트/운영(표식) | `MnEnvProfileHdr` · `MnEnv*` / `env:*` | 없음(표식) | — | 아이콘 불필요 |
| 우클릭(접속 창 프로필) | 설정 파일 이름 복사 | `MnCopyProfileFile` / `copy_file` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |
| 우클릭(접속 창 프로필) | 설정 파일 경로 복사 | `MnCopyProfilePath` / `copy_path` | 없음 | `content_copy` / `copy` (재사용 `mi_copy`) | 추가 |

### 2-8. 표에서 뺀 것(참고)

- **명령 팔레트**(`palette_commands`)는 글 목록뿐이라 아이콘 칸이 없다. 팔레트에만 있는 명령(`file.follow` · `file.large_force` · `mem.trim_now` · `edit.settings_json` · `view.outline` · `view.extensions` · `ext.*` 10개 · `obj.*` 10개 · `view.zoom_*` · `tab.next/prev` · `result.tab.*` · `session.env:*` · `syntax.set:*` · 포맷터 고르기/미리보기/기본 지정)은 이 점검 대상이 아니다. 풀다운으로 올리면 2-1 기준을 따른다.
- **인텔리센스 완성 팝업**(`intel.rs`)은 후보 종류 아이콘(`IconKind` → `menu_icon`)이 따로 있고 별도 체계라 제외.
- **조건 바 완성 · 검색어 이력 드롭다운**(`condbar.rs` · `search_history.rs`)은 값 목록이라 아이콘 불필요.
- **확장 메뉴**(`extensions/mod.rs`)는 확장 매니페스트가 항목을 정한다 — 아이콘 필드가 생기면 다시 본다.

## 3. 요약

**판정별 개수**(표 2-1~2-7 · 335행 · 묶음 행은 1행으로 셈)

| 판정 | 행 수 |
|---|---|
| 추가 | 175 |
| 교체 | 7 |
| 맞음 | 12 |
| 아이콘 불필요 | 141 |

**우선순위(위에서부터)**
1. 결과 우클릭 ▸ 고급 복사 ▸ **CSV로 복사** — `mi_table` → Material `csv`/`table_chart` · codicon `table` (교체)
2. 고급 복사 ▸ **텍스트로 복사** — `mi_table` → `notes`/`short_text` · codicon `note` (교체)
3. 고급 복사 ▸ **Markdown으로 복사** — `mi_table` → `markdown`(M↓) · codicon `markdown` (교체)
4. 고급 복사 ▸ **JSON으로 복사** — `mi_braces` 유지(`data_object` · `json`) · 위 셋을 바꾼 뒤 한 벌로 맞춰 보기 (맞음)
5. 결과 우클릭 **SQL로 복사 ▸** — `mi_db` 유지 · 결과 우클릭 **고급 복사 ▸** 부모에 아이콘 추가(`copy_all` · 확인 필요)
6. 보기 모드 메뉴 **텍스트 · Markdown** — `mi_files`(탐색기 아이콘) → 1~3과 같은 도형 공유 (교체)
7. 보기 모드 메뉴 **CSV · TSV** — 그리드와 같은 `mi_table` → CSV = `csv` · TSV = `table_rows`(확인 필요) (교체) · 보기 모드 SQL 하위 5개의 중복 `mi_db` 빼기
8. 결과 우클릭 **필터 ▸** 부모(`filter_alt` · `filter`) + 필터 지우기 둘(`filter_alt_off`) + 정규식(`mi_regex` 재사용) + 서버 재조회(`refresh` 재사용) (추가)
9. 결과 우클릭 편집 무리 — 행 추가/복제/삭제 · 편집 되돌리기/다시 하기 · 변경 적용/전부 되돌리기 · 값 보기 · 참조 행 보기 (추가 · 대부분 툴바 도형 재사용)
10. 모든 "복사" 계열 우클릭(이름/경로/설명/문장/접속 문자열 · 약 20곳) = `mi_copy` 재사용 · 이어서 풀다운 파일/편집/실행 핵심(새 편집기 · 열기 · 저장 · 실행 취소/다시 · 잘라내기/복사/붙여넣기 · 문장/전체 실행 · 커밋/롤백 · 접속/해제)은 **풀다운 아이콘 부품(마스크 → `IconImage`)** 이 먼저 필요

**새로 그려야 하는 도형(재사용으로 안 되는 것)** — `csv`(또는 `table_chart`) · `notes` · `markdown` · `table_rows` · `filter_alt` · `filter_alt_off` · `checklist` · `delete`(휴지통) · `add` · `edit`(연필) · `push_pin` · `folder_open`(메뉴 크기) · `open_in_new` · `link` · `info` · `code` · `restart_alt` · `delete_sweep` · `sync` · `power_off`(메뉴용 감싸기) 등. 툴바 도형(`new_script` · `save_file` · `run_*` · `commit` · `rollback` · `undo` · `redo` · `connect` · `disconnect` · `refresh` · `sessions` · `tx_log` · `log`)은 `menu_icon(shape_*)` / `material!`로 감싸 메뉴용 `mi_*`를 만든다.

## 4. 라이선스 메모

Material Symbols = Apache-2.0 · Codicons = CC-BY-4.0 → 이름·모양만 참고하고 벡터 도형으로 직접 그린다(이미지·글꼴 파일은 넣지 않는다). 기존 `material!`/`codicon!` 매크로처럼 SVG 경로를 옮겨 쓰는 경우는 출처 주석과 고지(THIRD-PARTY-NOTICES)를 함께 확인한다.
