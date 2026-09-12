# 12 · 사용자 Sublime Text 구성 학습 — 선호 프로필

> 사용자 요청(09-12): *"이 PC의 Sublime Text 구성 및 Package 설치와 구성을 확인해서 내가 선호하는 구성을 학습해줘."*
> 원천: `~/Library/Application Support/Sublime Text/`(iCloud `Configuration/macOS/Sublime Text`로 심볼릭 링크 — 기기 간 동기화). 읽은 것: `Packages/User/Preferences.sublime-settings` · `Package Control.sublime-settings` · `Installed Packages/` · `Packages/`. 사용자 키맵 파일은 없음(기본 키맵 사용). Sublime Merge도 설치됨.

## 1. Preferences — nexa-sql 기본값으로 삼는 항목

| 설정 | 값 | nexa-sql 반영 |
|---|---|---|
| `font_face` / `font_size` | **D2Coding · 14** | ★ 편집기·결과 기본 = D2Coding 14(있으면) → `nexa-font` 한글 고정폭 1순위와 일치 |
| `rulers` | `[20, 80, 120]` | 편집기 룰러 기본값 그대로(주석: 협업 코드 폭 가이드) |
| `auto_match_enabled` | **false** | 괄호·따옴표 자동 쌍 **기본 끔** — 체크리스트 P0 항목이지만 기본값은 OFF |
| `draw_white_space` | `["selection"]` | 공백 표시 = 선택 영역에서만 |
| `show_encoding` · `show_line_endings` | true | 상태줄에 인코딩·EOL 상시 표시 |
| `trim_trailing_white_space_on_save` · `ensure_newline_at_eof_on_save` | true | 저장 시 꼬리 공백 제거 · EOF 개행 |
| `trim_automatic_white_space` | false | 자동 들여쓰기 공백은 지우지 않음 |
| `auto_complete` · `index_files` | true | 자동완성 자동 팝업 · 심볼 인덱스(Goto Definition) |
| `ignored_packages` | Vintage · ActionScript | Vim 모드 안 씀 → P2 유지 |
| 컬러 스킴 | 기본(`Color Scheme - Default`) | 기본 스킴 = Mariana 계열 다크 추정(확인 필요) |

## 2. 설치 패키지 22종 → 시사점

| 패키지 | 무엇 | nexa-sql에서의 대응 |
|---|---|---|
| **Origami** | 창 분할(pane) 키보드 조작 | 분할 패널·탭 그룹(P1)을 키보드 우선으로 |
| **Alignment** | `=`·`:` 정렬 | ★ SQL 정렬(컬럼 목록·`=` 정렬) — 사용자 예시 스크립트가 탭 정렬 스타일 → 포매터 옵션 |
| **Table Editor** | 마크다운 표 편집 | 결과 그리드 → 텍스트 표 복사(마크다운/고정폭) |
| **Insert Nums** · **Text Pastry** | 연번·목록 삽입(멀티커서) | 멀티커서 P0 확인 · 연번 삽입 명령(Notepad++ Column Editor 대응) |
| **Compare Side-By-Side** | 두 버퍼 비교 | 결과셋 비교·스크립트 diff(P2) |
| **GitGutter** · **SublimeGit** | git 표시·명령 | 스크립트 파일 git gutter(P2) |
| **Pretty JSON** | JSON 정렬/축소 | JSON 셀 뷰어·포매터 |
| **HexViewer** | 16진 보기 | BLOB/RAW 셀 16진 뷰어 |
| **ConvertToUTF8** | EUC-KR 등 인코딩 변환 | ★ 한국 현장 인코딩(EUC-KR/CP949) 스크립트 열기·저장 — P0 인코딩 감지에 CP949 포함 |
| **Color Highlight** | 색 코드 표시 | (해당 없음) |
| AdvancedNewFile · AutoFileName · SideBarEnhancements · FileIcons | 파일 다루기 | 스크립트 폴더 사이드바(P1) |
| Emmet · MarkdownPreview · LiveReload · VBScript · Random Everything | 웹/기타 | 해당 없음 |

## 3. 프로필 요약 (한 줄)

**고정폭 한글 폰트(D2Coding 14) · 룰러 20/80/120 · 자동 괄호 끔 · 저장 시 정리 · 키보드 분할(Origami) · 정렬·연번·표 편집 같은 "텍스트 가공" 패키지 선호 · 인코딩 표시·변환 필수 · Vim 안 씀.**
→ nexa-sql `Default` 패키지의 `Preferences.sublime-settings` 기본값을 이 표대로 둔다([09 §4](09-editor-and-packages.md)).
