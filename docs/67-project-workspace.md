# 67 · 프로젝트(워크스페이스) 관리 — 조사 · 설계 · 결정

> **요청**(사용자 09-22): *"Sublime · VS Code · DataGrip · DBeaver의 프로젝트 관리 조사. Sublime처럼 프로젝트에 미저장 탭까지 보존해 종료 뒤 다시 쓰고, 파일은 참조로 열어 다음에 최신을, 닫을 때는 전체/개별 저장·취소 확인. 강제 종료엔 주기 자동 저장(되돌리기 추적 · 별도 파일 · 복원 UI · 진짜 저장 등 최신 방식 조사·추천). 프로젝트 파일은 임의 폴더 · 폴더 Add/Remove(Sublime 구조). 탐색기 = Open files(자동 높이) + Directories(통합 스크롤 검토). 활동 막대 아이콘. **편집기 워크스페이스 개념 — DB 연결·설정과 분리.** 메뉴 = 저장·새로·열기·다른 이름·닫기·전환 + Command Palette."*
> **선행**: [18 세션·프로젝트](18-session-and-projects.md) · [36 파일 검색·프로젝트](36-find-in-files-and-project.md)(§3 project.json 초안 · D-55~57) · [58 외부 변경 정책](58-external-change-policy.md) · [59 큰 파일](59-large-file-handling.md) · [60 되돌리기 기록 파일](60-undo-redo-redesign.md). 조사 원문 = [67a](67a-project-research.md).

---

## 0. 결론 여섯 줄

1. **파일 둘**(Sublime 구조): `<이름>.nsql-project`(폴더 목록 · 프로젝트 설정 · **어디에나** · VCS에 올림) + `<이름>.nsql-workspace`(열린 탭 · 활성 탭 · 접힘 상태 · 검색 히스토리 — 프로젝트 **옆** · 개인용). 프로젝트 없는 상태 = **기본 워크스페이스**(사용자 데이터 폴더 `workspaces/default.nsql-workspace`) — 지금의 이름 없는 탭도 여기서 복원(hot exit · D-57).
2. **미저장 본문은 프로젝트/워크스페이스 파일에 넣지 않는다** — 사용자 데이터 폴더 `backups/<워크스페이스 id>/<탭 id>`에 **탭마다 스냅숏 파일**(VS Code · Zed · Notepad++ 방식). 워크스페이스는 그 파일을 가리키기만. 근거 = Sublime의 "창 닫기 → 워크스페이스 / 종료 → 세션" 이중 경로가 유실의 원인(포럼 59074 · 65806) · 프로젝트 파일을 옮기거나 동기화해도 안전.
3. **주기 저장 = 입력 디바운스 1초 + 최대 대기 5초 상한**(VS Code 1초 · Zed 100ms 스로틀 · Notepad++ 7초 · Kate 15초 중 절충) · 임시 파일 → `rename`(원자적) · dirty가 풀리면(저장·되돌리기·닫기) 스냅숏 **즉시 폐기** · 큰 버퍼(`editor.undo_giant_mb` 계열 상한)는 스냅숏 대신 저장 프롬프트(Sublime 4151).
4. **복원 = 묵시**(VS Code · Zed · Sublime · Notepad++ 공통): 탭이 dirty 표식 그대로 돌아오고 상태줄/토스트 "복원된 미저장 탭 n개"만. 참조 파일이 디스크에서 바뀌었으면 탭 상단 띠(유지 / 디스크에서 다시 읽기 / 비교 — `extfile.rs`·`merge3` 재사용). 복원 못 한 스냅숏은 지우지 않고 둔다.
5. **탐색기 = 한 컨테이너에 패널 둘**(VS Code식): 위 OPEN FILES(항목 수 × 행 높이로 자동 · 상한 `project.open_files_max` 9 · 넘치면 섹션 안 스크롤) + 아래 FOLDERS(나머지 · 폴더별 트리) · 경계 sash · 헤더 접기. **통합 스크롤 하나**는 Sublime이 택했다가 사용자 불만(헤더 유실 · 28631)을 남긴 방식이라 **채택하지 않는다**(사용자 "문제가 있다면 설계 변경" 조항 적용) — 절충 = OPEN FILES 헤더 sticky.
6. **DB 연결과 분리**: 프로젝트 파일에는 접속 정보가 없다. 탭의 접속 컨텍스트(프로필 **이름** · 전용 세션 여부)는 워크스페이스에 **표식으로만** 저장하고 복원 때 자동 재접속하지 않는다(26 §8 · "No connection + 이전 프로필 표식"). 필요하면 나중에 프로젝트 설정에 `connection` 키를 **선택**으로.

---

## 1. 조사 요약(원문 [67a](67a-project-research.md))

| 도구 | 프로젝트 파일 위치 | 다중 폴더 | 미저장 버퍼 | 저장 주기 | 충돌 복원 | 탐색기 열린 파일 |
|---|---|---|---|---|---|---|
| Sublime Text 4 | 어디든(상대 경로 = 프로젝트 파일 기준) · 워크스페이스는 옆 | `folders[]` | 창 닫기 → 워크스페이스 `buffers[].contents` · 종료 → `Local/Session` · 작업 중 → `Auto Save Session` | 주기(간격 비공개) | 묵시 · 거대 버퍼는 프롬프트 | OPEN FILES + FOLDERS **한 목록**(섹션이 밀려 내려감) |
| VS Code | `.code-workspace` 어디든 · untitled = `<userData>/Workspaces/<id>` | 멀티루트 | `<userData>/Backups/<id>/<scheme>/<hash>`(프리앰블+본문) | 디바운스 1s(자동 저장 짧으면 2s) · dirty 해제 시 폐기 | 묵시 · 백업 실패 시만 프롬프트 | OPEN EDITORS = 독립 ViewPane(22px × ≤9 · 동적) + sash |
| DataGrip | `.idea/` 디렉터리 | Attach Directory | 없음(자동 저장이 곧 저장) + Local History | 포커스 이탈 · 유휴 N초 | 자동 저장 + Local History | 파일 창 + Scratches/Consoles(열린 파일 섹션 없음) |
| DBeaver | Eclipse 워크스페이스 안(외부 링크 가능) | 프로젝트 여러 + Link Folder | 없음(닫을 때 자동 저장 기본 24.1.1+) | 닫기/실행 · Eclipse Autosave 60s(기본 끔) | 없음(#22058 미해결) | Project Explorer(열린 파일 섹션 없음) · 스크립트–연결 결합 강함 |
| Zed | 폴더 = 워크스페이스(SQLite) | ○ | SQLite `editors.contents` | 100ms 스로틀 | 묵시 · 빈 창 미지원 | 프로젝트 패널만 |
| Notepad++ / Kate / Vim | 세션 파일 | ○ | `backup/` 7s · `.kate-swp` 15s(차이) · `.swp` 4s/200자 | 주기 | 묵시 / Recover 띠 / ATTENTION 프롬프트 | — |

**배울 것**: 스냅숏은 사용자 폴더 한 곳(VS Code·Zed·Notepad++) · 디바운스 + 원자적 쓰기 · 묵시 복원 · 열린 파일 섹션은 패널 분리(VS Code). **피할 것**: Sublime의 이중 저장 경로 · DBeaver의 스크립트–연결 결합 · Zed의 "프로젝트 없는 빈 창 미지원".

---

## 2. 모델

### 2-1. 파일

```jsonc
// my.nsql-project  (어디에나 · VCS)
{ "version": 1,
  "folders": [ { "path": "D:/Projects/acme/<프로젝트>/이슈관리", "name": "이슈관리", "exclude": ["target", ".git"] },
               { "path": "../Oracle" } ],                     // 상대 = 이 파일 기준(Sublime·VS Code 규약)
  "settings": { "editor.tab_size": 4 } }                      // 선택 · 이 프로젝트에서만 덮어씀(24 계층 · 2차)

// my.nsql-workspace  (프로젝트 옆 · 개인용 · .gitignore 권장)
{ "version": 1, "project": "my.nsql-project",
  "tabs": [ { "id": 17, "path": "이슈관리/2026-09-01.sql", "caret": 120, "scroll": 3, "enc": "utf8", "conn": "APPDB" },
            { "id": 18, "title": "Script_3", "backup": true, "caret": 0 } ],   // 메모리 탭 = 본문은 backups/에
  "active": 18, "expanded": ["이슈관리/"], "open_files_h": 5,
  "search": { "where": ["<project>"], "queries": ["1433"] } }
```

- 프로젝트 없는 상태 = `NSQL_HOME/workspaces/default.nsql-workspace`(`project` 없음) — 지금까지의 "이름 없는 탭" 세션이 여기로 옮겨진다(36 §3 D-57 ✅ 채택).
- 워크스페이스 id = 프로젝트 파일 경로의 해시(기본 = `default`) → `NSQL_HOME/backups/<id>/<탭 id>.txt`(첫 줄 프리앰블 JSON: 원본 경로 · 원본 해시 · 인코딩 · 접속 표식 · 시각 · 상한 10 KB) + 본문.

### 2-2. 상태 · 명령(메뉴 **Project** + Command Palette `Project: …`)

| 명령 | 메뉴/팔레트 | 동작 |
|---|---|---|
| New Project… | `project.new` | 저장 위치를 물어(`.nsql-project` 저장 대화상자) 빈 프로젝트 → 현재 탭들은 새 워크스페이스로 **이어진다**(Sublime "Save Project As"와 같은 감각) |
| Open Project… | `project.open` | 파일 대화상자(`.nsql-project`) → 현재 워크스페이스 저장·닫기(§3 확인) → 새 프로젝트+워크스페이스 열기 |
| Recent Projects ▸ / Switch Project… | `project.recent:<i>` · `project.switch` | 최근 목록(`project.recent` HIDDEN · 10개) + **"(프로젝트 없음)"** 항목 = 기본 워크스페이스로 |
| Save Project | `project.save` | 프로젝트 파일 + 워크스페이스 즉시 저장(자동 저장과 같은 경로) |
| Save Project As… | `project.save_as` | 새 경로로 프로젝트 파일 저장(상대 경로 재계산 · VS Code) · 워크스페이스도 옆으로 |
| Close Project | `project.close` | §3 확인 → 기본 워크스페이스로 |
| Add Folder to Project… | `project.add_folder` | 폴더 대화상자(자체 · `PickerMode::Folder`) · 끌어놓기(winit `DroppedFile`) · 프로젝트가 없으면 **untitled 프로젝트** 생성(VS Code) |
| Remove Folder from Project | `project.remove_folder:<i>` · 탐색기 폴더 우클릭 | 목록에서만 제거(디스크 불변) |
| Refresh / Reveal in Explorer / Open Containing Folder | 탐색기 우클릭 | |

### 2-3. 저장 시점(hot exit · 자동 저장)

| 시점 | 무엇 |
|---|---|
| 타이핑 | 아무것도 쓰지 않음 · dirty + 세대 번호 |
| 변경 뒤 **1초 유휴** 또는 **5초 상한** | 바뀐 메모리 탭·dirty 파일 탭의 스냅숏(`backups/<id>/<tab>.txt.tmp` → rename) — **탭 단위 격리**(61 §2 · 작업 스레드 · UI는 `buf()` 스냅숏만) |
| 탭 열기/닫기/이동 · 활성 탭 바뀜 · 폴더 추가/제거 · 접힘 | 워크스페이스 JSON(디바운스 2초) |
| 저장 · 되돌려서 clean · 탭 닫기(버림) | 그 탭 스냅숏 **즉시 삭제** |
| 프로젝트 닫기 · 전환 · 앱 종료 | §3 확인 뒤 워크스페이스 + 스냅숏 flush |
| 큰 버퍼(> `project.backup_max_mb` 기본 8) | 스냅숏 대신 "저장하세요" 프롬프트(Sublime 4151) · 큰 파일 모드 L1/L2(59)와 연동 |

설정: `project.hot_exit`(`always` 기본 · `only_on_quit` · `off`) · `project.backup_debounce_ms`(1000 · HIDDEN) · `project.backup_max_wait_ms`(5000 · HIDDEN) · `project.backup_max_mb`(8) · `project.open_files_max`(9) · `project.recent`(HIDDEN) · `project.last`(HIDDEN · 마지막 프로젝트 = 다음 기동에 다시 연다).

### 2-4. 복원

1. 기동(★ 09-22 사용자 · journal §31): **기본 = 파일 모드**(프로젝트 없음). 인자 `.nsql-project` = 그 프로젝트 · `project.restore_last`(기본 끔)가 켜져 있고 **첫 인스턴스**(`instance.lock`)이고 파일 인자가 없을 때만 `project.last` · 이후 인스턴스는 인자 없으면 파일 모드. (종전 안: `project.last`가 있으면 늘 복원 — 폐기.)
2. 파일 탭: 디스크를 **다시 읽는다**(참조 = 최신 · 사용자 요구) → 스냅숏이 있으면(dirty였음) 스냅숏 본문을 올리고 프리앰블의 원본 해시와 디스크 해시가 다르면 탭 상단 띠 **유지 / 디스크에서 다시 읽기 / 비교**(58 · `extfile.rs`) · 파일이 사라졌으면 "(삭제됨)" 표식 + 스냅숏 본문.
3. 메모리 탭: 스냅숏 본문 그대로 · 제목 · dirty 표식 · 캐럿.
4. 접속 표식: 프로필 이름만 표시(자동 재접속 없음).
5. 상태줄/토스트 "복원된 미저장 탭 n개"(클릭 = 목록). 손상된 워크스페이스 파일은 `.bak`으로 시도 → 둘 다 실패면 빈 창(조용히 죽지 않는다 · 18 §3).

---

## 3. 닫을 때 묻기(프로젝트 닫기 · 전환 · 창/앱 종료)

- `project.hot_exit = always`(기본): **묻지 않는다** — 스냅숏이 있으므로(VS Code·Sublime). 단 스냅숏을 못 쓴 탭(거대 · 쓰기 실패)이 있으면 그 탭만 묻는다.
- `off`: 사용자 요구대로 **탭별 목록 + [모두 저장] [개별…] [저장 안 함] [취소]** — 개별 = 탭마다 저장/버림/취소(기존 `editor.close_unsaved` 3택과 같은 부품 · 저장 창 타임아웃 버튼 규칙).
- 미커밋 트랜잭션이 있는 탭은 34 §2-4의 트랜잭션 확인이 **먼저**(파일 저장과 별개의 질문).

---

## 4. 탐색기(활동 막대 `view.project` · 아이콘 = 폴더 트리)

```
▾ OPEN FILES (3)                          ← 자동 높이(3 × 행) · 상한 9 · 넘치면 섹션 스크롤 · 헤더 클릭 = 접기
    ● Script_3                (메모리 · dirty)
      2026-09-01.sql          이슈관리/
      a.sql                   D:/tmp/          ← 프로젝트 밖 파일도 열려 있으면 여기 보임
══════ sash(끌어 높이 조절 · 워크스페이스에 저장) ══════
▾ FOLDERS
  ▾ 이슈관리                  D:/Projects/…/이슈관리
      2026-09-01.sql
    ▸ 보관
  ▸ Oracle
```

- 부품: OPEN FILES = nexa-ctl `TreeView`(평면 · 열 = 제목·경로 흐림) · FOLDERS = `TreeView` 루트 = 폴더 · **지연 열거**(펼칠 때 `nexa_fs::list_opts` · 09-16 무시 규칙 · `.gitignore` 옵션 D-55) · 폴더 감시 = `nexa-fs watch`(58과 같은 부품 · 펼친 폴더만) · 두 트리는 `ExplorerSet`의 공용 뷰포트 규칙(`set_clip` · 키 경계 이동 `cross_pane`)을 그대로 재사용하되 sash로 나뉜 **두 뷰포트**.
- 행 동작: 클릭 = 그 탭/파일 열기 · 더블클릭 = 고정 탭 · 우클릭 = 새 파일 · 새 폴더 · 이름 바꾸기 · 삭제(휴지통) · 경로 복사 · 탐색기에서 보기 · Remove Folder(루트만) · 파일 검색(36 Where에 이 폴더).
- 활성 탭 ↔ 트리 선택 동기(`project.reveal_active` 기본 켬).
- 부하원(39 §3): 폴더 감시 핸들 = 펼친 폴더 수 · 트리 캐시 = 열거한 항목 · 스냅숏 스레드 1 · 전부 설정 상한.

### 4-1. 구현 상태(09-22 · [journal §28](journal/2026-09-22.md))

- ✅ FOLDERS 트리(`project_panel.rs` · 자체 노드 목록 · 지연 열거 · 루트 하나면 자동 펼침) · **깊이 무관 필터**(펼치지 않은 폴더도 `project.scan_max`까지 열거) · 클릭 = 미리보기 탭 · 더블클릭/Enter = 정식 탭 · Space = 미리보기 · 경로 툴팁 · 프로젝트 없음 = 안내 + Open/New 링크 행 · 저장 뒤 `refresh`.
- ✅ **미리보기 탭**(Sublime · 사용자 09-22): 편집기 `preview` 하나 — 클릭마다 같은 탭에 바꿔 넣음(앞 버퍼 드롭) · 편집 = 승격(`◦` 제거) · **탭 본체 더블클릭 = 승격**(사용자 09-22) · 탐색기 더블클릭/Enter = 처음부터 정식 · 다음 클릭 = 새 미리보기 · 설정 `project.preview_tab`.
- ✅ 메뉴 Project · 팔레트 · `.nsql-project` 대화상자(Open/Save/Folder) · 최근 · Switch · `project.last` 기동.
- ✅ 09-22 후반(journal §76~78): 셰브론 = 객체 탐색기 부품 · OS 파일/폴더 아이콘 `project.icons`(향상 모드 off) · 루트 우클릭 메뉴(Remove Folder/Add Folder) · 프로젝트 파일 `"selected"` = 마지막 선택 복원 · 탭 메뉴 "Reveal in Project Explorer"(프로젝트 폴더 안 파일만) · 활성 탭 동기 `project.auto_reveal`(기본 off = 표시만 · on = 펼침+스크롤).
- ✅ 09-23(journal §89): 작업 환경 = 프로젝트 파일(`tabs` 경로/스크립트 본문/캐럿 앵커 · `active` · `bookmarks` 내장) · 자동 저장 `project.autosave`/`autosave_secs`(30) · 종료 흐름(프로젝트 물음 → 파일 탭 물음) · 복원 = 다시 읽기 + fuzzy 캐럿 · **OPEN FILES 섹션**.
- ⏳ sash · 폴더 감시 · 워크스페이스 파일·hot exit(P3) · 파일 검색 연동(P5).

---

## 5. 구현 순서(T-165)

| 단계 | 내용 | 부품 |
|---|---|---|
| P1 🚧(09-22 모델·JSON = GUI `project.rs`) | `nsql-project` 크레이트(의존 0 · JSON = `nsql-settings::json` 재사용 여부 검토 → 자체 미니 직렬화): `Project`·`Workspace` 모델 · 읽기/쓰기(원자적 · `.bak`) · 상대 경로 · 스냅숏 저장소 `Backups`(프리앰블 · 폐기 · 상한) · 순수 판정 `close_plan`(hot_exit × dirty × 거대 · MC/DC) · 테스트 | 새 크레이트 |
| P2 ✅(09-22) | GUI 상태 `App.project`(현재 프로젝트 · 워크스페이스 · 저장 시계) · 메뉴 **Project** + 팔레트 `Project: …` · 파일 대화상자(`.nsql-project` 필터 · 폴더 모드) · 최근 목록 · Switch(팔레트 목록 + "(프로젝트 없음)") | `main.rs` · `project_cmds.rs` |
| P3 | hot exit: 탭 스냅숏 디바운스/상한 · 워크스페이스 디바운스 · 기동 복원(묵시 + 안내 토스트) · 닫기 확인 3택/개별 · 기본 워크스페이스로 기존 세션 대체 | `hotexit.rs` · `editors.rs` |
| P4 🚧(09-22 트리·필터·미리보기 ✅) | 탐색기 패널 `project_panel.rs`(OPEN FILES + FOLDERS · sash · 지연 열거 · 감시 · 우클릭) · 활동 막대 아이콘 · 활성 탭 동기 | 활동 막대 · nexa-fs |
| P5 | 파일 검색 Where `<project>` 연동(36) · 프로젝트 설정 덮어쓰기(24 계층) · 캡처 4장(모서리 규칙) · 문서 | |

---

## 6. 결정(권장안 · 사용자 확인)

| # | 질문 | 권장 |
|---|---|---|
| **D-144** | 미저장 본문 위치: ① 워크스페이스 파일 안(Sublime) ② 사용자 폴더 `backups/` 스냅숏(VS Code) | **②** — 프로젝트 파일 이동·동기화·다른 PC에서 안전 · 사용자 요구 "프로젝트 파일에 일괄 저장"의 목적(종료 뒤 복원)은 그대로 충족 |
| **D-145** | 주기 저장 방식: ① 본문 스냅숏 ② 되돌리기 연산 기록(60 `undofile`)만 ③ 하이브리드(작으면 스냅숏 · 크면 저장 해시+연산 기록) | **③** — 1차는 ①만 구현 · 거대 버퍼는 프롬프트 |
| **D-146** | 복원 UX: ① 묵시 + 안내 ② 시작 때 "복원할까요?" 대화상자 | **①** |
| **D-147** | 탐색기 배치: ① VS Code식 두 패널 + sash ② Sublime식 한 목록 | **①**(사용자 "통합 스크롤에 문제가 있으면 설계 변경" — Sublime 사용자 불만이 그 문제) |
| **D-148** | 프로젝트 없는 상태: ① 기본 워크스페이스로 hot exit 유지(D-57) ② 아무것도 보존 안 함 | **①** |
| **D-149** | 접속 정보: ① 프로젝트 파일에 없음(탭 표식만) ② 선택 키 `connection` | **①** 지금 · ②는 요구 생기면 |

## 6. 작업 모드 셋 — 파일 · 폴더 · 프로젝트(09-23 · 사용자 확정)

| 모드 | 진입 | 로컬 상태의 자리(`project::WorkMode::local_dir`) | 지금 나누는 것 | 앞으로 같은 자리에서 나눌 것 |
|---|---|---|---|---|
| **파일 모드** | 인자 없음(또는 파일 인자만) | 전역 설정 폴더 `%APPDATA%/nexa-sql`(`NSQL_HOME`) | 북마크 = `workspaces/default.nsql-workspace` | (기본 = 전역 설정 그대로) |
| **폴더 모드** | `nexa-sql .` · `nexa-sql c:\users\user\project`(첫 폴더 인자 · 절대화) | **`<폴더>/.nsql/`** | 북마크 = `.nsql/workspaces/default.nsql-workspace` · `${workspaceFolder}` = 그 폴더 · 파일 대화상자 시작 폴더 | 설정 오버라이드(`.nsql/settings.conf` → 전역 위에 겹침) · 최근 파일 · 되돌리기 기록 · 미저장 스냅숏 — **설계만** 반영(코드 = `WorkMode::local_dir` 한 자리) |
| **프로젝트 모드** | `.nsql-project` 인자 · 열기 · 전환 | **프로젝트 파일 안**(`local_dir = None`) · 기기별 오버라이드는 73 §4-2 `.local.json` | 탭·캐럿·패널·북마크·접속 표식(§4·70 §6-1) | 프로젝트 설정 절(`"settings": {…}` → 전역 위에 겹침 · 73 D-186~190 뒤) |

규칙: ① 우선순위 = 프로젝트 > 폴더 > 파일(`WorkMode::of`) ② 프로젝트를 닫으면 **직전 모드**(폴더 인자로 켰으면 폴더 · 아니면 파일)로 돌아가고 편집기는 처음 실행 상태(journal §98) ③ 파일 모드에서 새 프로젝트를 만들면 로컬 북마크는 프로젝트로 **이관**(로컬 파일 비움 · 기존 프로젝트를 열 때는 이관 없음) ④ `.nsql/`은 첫 저장 때 만든다(빈 저장소를 위해 파일을 만들지 않는 기존 규칙 그대로) ⑤ 새 로컬 상태를 추가할 때는 자기 경로를 계산하지 말고 `WorkMode::local_dir()` 아래에 둔다(설정·기능별 분리를 한 자리에서).

파일 열기/저장 창 **시작 폴더**(10-22 · 135차) = 기동 뒤 **첫 열기** → 프로젝트 모드면 프로젝트 파일이 있는 폴더 · 폴더 모드면 그 폴더 · 둘 다 아니면 활성 탭 파일 폴더 → `file.last_dir` / **그 뒤** → 마지막 위치(`file.last_dir` = 창을 닫을 때 기록) → 활성 탭 폴더 → 홈 · 설정 창의 폴더 고르기는 이 규칙 밖 · 순수 함수 `app/files.rs::file_dialog_start`.

## 7. 프로젝트 파일 형식 v2 — JSON 헤더 + 탭별 payload 블록(09-23 · 사용자 "메타는 일반 텍스트 · payload는 바이너리 · CDATA식 탭별")

```text
{ "version": 1, "folders": [...], "tabs": [ { "path": "a.sql", "id": 12, "hash": "…", "blob": true, ... } ], "bookmarks": {...} }
%%NSQL-BLOBS%%
tab=12 len=1834
<a.sql의 미저장 본문 1834바이트 그대로>
tab=15 len=27
<Script_2 본문>
```

- **헤더** = 종전 JSON 그대로(폴더 · 탭 메타 · 패널 · 북마크) — 사람이 읽고 diff한다. 본문 `text`는 **헤더에 넣지 않는다**(`"blob": true` 표식만).
- **블록** = 마커 줄 `%%NSQL-BLOBS%%` 아래 탭마다 `tab=<id> len=<n>`(일반 텍스트 메타) + 줄바꿈 + **payload n 바이트**(이스케이프 없음 · 어떤 바이트든) + 줄바꿈. 길이로 자르므로 payload 안의 마커·따옴표·줄바꿈이 안전하고 JSON 재파싱 비용이 없다.
- 부품 = [`nsql-settings::projfile`](../crates/nsql-settings/src/projfile.rs) `split(bytes) → (헤더, 블록들)` · `join(헤더, 블록들) → bytes` — GUI(`Project::load/to_document`)와 CLI(`nsql bookmark --project`: 헤더만 고치고 블록은 되붙임)가 같이 쓴다. 옛 파일(마커 없음 · `text`가 JSON 안)은 그대로 읽힌다.
- payload는 탭 `id`로 되붙는다 → 본문을 담는 탭은 반드시 `id`가 있다(캡처가 늘 넣는다). 바뀜 비교(`project_last_json`)는 문서 전체 바이트.
- 탭 필드 `profile`(10-30 · 182차 · T-121 ⑤ · D-407 프로젝트 파일 하나만) = 그 탭이 묶인 공유 세션의 **프로필 이름**(접속 중일 때만 · 전용/인라인 접속 = 없음 · 비밀번호·주소 없음 = §0-6 "표식만") → 복원 때 그 프로필 세션이 있으면 바로 묶고 없으면 대기 → 사용자가 그 프로필로 접속하면 묶음(자동 재접속 없음).
- 탭 필드 `ro`(10-13) = 읽기 전용으로 연 탭(`"ro": true` · 없으면 false) → 복원 때 `LoadMode::ReadOnly` · 이진 안내 탭은 `path` = 원본 파일 + `ro` (+ 미리보기면 `preview`)로 저장되고 복원 때 다시 이진 판정으로 안내 탭이 된다 · "(앞부분)"·디스크 보기 탭은 담지 않는다([70 §6-1](70-autosave-and-restore.md)).

> ★ **10-26 T-153 ③** — `TabState.vars`(무제 탭 변수 = `vars_to_script` 글 · 프로젝트 모드에서만 보관 · 복원 때 새 탭 id로) · 파일로 저장하면 경로 해시 변수 파일로 인수인계.

## 8. 프로젝트 설정 절 `settings`(10-30 · T-39b · T-69 프로젝트 층 · D-406·D-407·D-408 · 설계 = 협업 · 구현 전)

> 사용자 규칙 **"프로젝트 파일 하나만"**(D-407): 프로젝트별 설정을 `.nexa/settings.conf`(24 · 36 · 종전 T-39b 안)나 폴더 모드 `.nsql/settings.conf`(§6)로 두지 않고 **프로젝트 파일 헤더의 `settings` 절**에 둔다. 종전 안 폐기.

### 8-1. 형식

```jsonc
{ "version": 1, "folders": [ … ], "tabs": [ … ],
  "settings": {
    "editor.tab_size": 2,                 // 이 프로젝트의 모든 탭
    "editor.indent_spaces": true,
    "format.keyword_case": "upper",
    "[sql]":  { "editor.tab_size": 4 },  // 문법 한정(VS Code `[언어]` 관례 · 이름 = 구문 정의 이름 소문자)
    "[json]": { "editor.tab_size": 2, "editor.indent_spaces": true }
  } }
```

- 값 = 설정 레지스트리(`nsql-settings::REGISTRY`)의 키와 같은 이름·같은 값 꼴(JSON 숫자·불·문자열) · 모르는 키 = 읽을 때 무시 + 로그 1회(지우지 않는다 = 새 판에서 쓴 파일을 옛 판이 열어도 보존).
- `[문법]` 묶음 = 그 구문의 탭에만 · 한 단계만(중첩 없음).

### 8-2. 허용 키 — `Entry.scope`

프로젝트 파일은 VCS로 **다른 사람에게서 받는 파일**이다. 받은 프로젝트를 여는 것만으로 위험한 동작이 켜지면 안 된다(VS Code "Restricted Mode" · 신뢰되지 않은 워크스페이스의 실행 경로 설정 차단과 같은 이유).

| 범위 | 예 | 프로젝트 절 |
|---|---|---|
| **Project 허용**(레지스트리 `Entry`에 표시 · 기본 = 불허) | `editor.*`(들여쓰기·줄 끝·글꼴 크기 제외 검토) · `format.*` · `script.define*` · `intel.*` 일부 · `grid.null_text` | ✅ |
| Global 전용 | 드라이버 경로·자동 설치 · `ssh.command` · 확장 · 라이선스 · `perf.*` · 창·글꼴·테마 · `session.*` · 네트워크 상한 · 비밀 | ❌ 무시 + 로그 |

- 구현 = `Entry`에 `project: bool`(또는 `scope`) 칸 하나 · 기본 false · 허용 목록은 키를 추가할 때 함께 정한다(30 §1-2 체크리스트에 한 줄).

### 8-3. 해석 순서(가까운 층이 이긴다 · 해석기 = 층 목록 · 30 §1)

```
기본값(REGISTRY) → 전역 settings.conf → 문법 Packages/<pkg>/<Syntax>.nexa-settings(T-69) → 프로젝트 settings → 프로젝트 settings["[문법]"] → 현재 탭(메모리 · 감지 D-403·팝업)
```

- 프로젝트 없음(파일 모드) = 프로젝트 두 층이 빈 층. 폴더 모드 = 프로젝트 층 없음(§6의 `.nsql/settings.conf` 안은 D-407로 만들지 않음 · 필요해지면 사용자에게 묻는다).
- 들여쓰기 감지(D-403)는 지금처럼 **탭 층만** 채운다 → 감지가 안 되는 파일(새 파일·빈 파일)에서 문법·프로젝트 층이 기본을 정한다.

### 8-4. 쓰기 · 화면

- ★ **구현(10-31 · 186차 13차)** = 탭이 아니라 **하단 스위치 "프로젝트"**(기존 "고급"·"변경" 스위치와 같은 자리 · 고급 묶음 · 프로젝트가 열려 있을 때만 보임) — 아래 설계(머리 범위 탭)를 이렇게 바꿈. 설계 원문: 설정 창 머리에 **범위 탭 `사용자 | 프로젝트`**(프로젝트 모드일 때만 · VS Code User/Workspace) — 프로젝트 탭 = Project 허용 키만 보이고 값을 바꾸면 헤더 `settings` 절에 쓴다(기존 저장 경로 `project_save_to` · 자동 저장 감시 세대에 포함 · 70 §7-1).
- 카드에 **출처 표시**("프로젝트에서 덮어씀" · 되돌리기 = 그 키를 절에서 지움) · 상태줄 들여쓰기 툴팁 = 값의 출처 층(31 §2).
- 상태줄 들여쓰기 팝업 = "이 문법의 기본으로 저장"(전역 문법 층) · "프로젝트에 저장"(프로젝트 모드일 때 · `[문법]` 묶음) · "전역 기본으로 저장".
- 프로젝트 파일이 밖에서 바뀌면(git pull) 58 정책대로 다시 읽고 설정 층을 갈아 끼운다(열린 탭에 즉시 반영 · `apply_setting` 경로).

### 8-6. 구현 1차(10-31 · 186차 12차 · 개발) — 설계와 다른 점

- **허용 키 = 함수** `nsql_settings::project_allowed(key)`(§8-2의 `Entry.scope` 칸 대신 · 허용 = `editor.*`(글꼴·미니맵 폭 제외) · `format.*` · `script.define*` · `grid.null_text` · `intel.insert_*`) — 새 키를 추가할 때 이 함수의 허용 여부를 함께 정한다(30 §1-2 체크리스트 9).
- **읽기/쓰기** = project.rs 헤더 `settings` 절(평키 + `[문법]` 한 단계 · JSON 불/정수/문자열 ↔ 레지스트리 원시 글자 · **모르는 키 보존** · 비면 키를 쓰지 않음) · 순수 `json_raw`/`setting_json`/`syntax_group` · 시험 `settings_section_round_trip_preserves_unknown_keys`.
- **적용** = `apply_project_settings()`(프로젝트 열기·전환 + 기동 복원 길) = `Settings::clear_overlay("project")` → 허용 키만 `set_overlay(key, raw, "project")` → 걷힌·넣은 키를 `apply_setting`으로 즉시 반영 · **overlay라 settings.conf에 쓰지 않는다**(프로젝트를 닫거나 바꾸면 자동 원복) · `[문법]` 묶음은 1차에서 `editor.tab_size`·`editor.indent_spaces`만(`Editors::set_syntax_indent`) · 불허·잘못된 값 = 로그 1줄 `Project settings ignored …: <키>`.
- **들여쓰기 층**(editors.rs) = 전역 → 문법(`syntax.rs` `indent_defaults()` = `Packages/<pkg>/<이름>.nexa-settings` 스캔 · Sublime `<Syntax>.sublime-settings` 차용 · JSON 객체) → 프로젝트 `[문법]` → 탭 재정의(감지 D-403·팝업) · `indent_for_syntax`/`default_indent_for` · 시험 `syntax_indent_layers_resolve_nearest_first`(MC/DC 6단) · `syntax_settings_layer_indent_defaults`.
- **V1(협업 · 격리 홈 · temp 프로젝트 인자)** = ① `{"editor.tab_size":2,"[sql]":{"editor.indent_spaces":false},"driver.auto_install":false}` → `edit.indent` = 탭 · 로그 "무시 … driver.auto_install" 1줄 ② `[sql]` 공백 true → `··select`(공백 2칸 = 프로젝트 tab_size + [sql] 둘 다 적용) ✓.
- **13차(10-31 · 186차 후속) 설정 창 범위** = prefs_win 하단 스위치 **"프로젝트"**(호스트 `set_project_scope(Option<Vec<(키, 값)>>)` · 프로젝트 없음 = 숨김) — 켜면 카드 = `project_allowed` 키만 · 값 = 프로젝트 절 값(없으면 실효값) · 강조선 = 절에 있는 키 · 안내줄 `PrefsProjectScopeNote` · 값 변경/초기화 = `PrefsAction::ProjectChanged`/`ProjectReset` → `project_setting_set(key, Some(값)|None)`(허용 키·정규화 통과 값만 → 헤더 절 평키 갱신 → `apply_project_settings` → `project_changed(false)` = 자동 저장 감시) · **카드 덧말**(범위와 무관) = 절에 있는 키의 카드에 "프로젝트에서 덮어씀: 값"(호스트 덧말과 ` · `로 합침 · 별도 `project_notes`라 들여쓰기 등 다른 덧말을 지우지 않음) · 동기화 `prefs_sync_project_scope()` = `apply_project_settings` 끝 + 설정 창 열 때 · 덤프 `project=-|off|on` · 순수 `view_of` · 시험 `project_scope_filters_allowed_keys_and_shows_project_values` · nexa-sql 520/0.
- **남음(다음 묶음)** = 상태줄 들여쓰기 팝업 "프로젝트에 저장" · 프로젝트 파일 외부 변경(git pull) 시 재적용 · `[문법]`의 그 밖 키 · 설정 창에서 `[문법]` 묶음 편집.

### 8-5. 시험

- 순수 해석기: 층 조합 MC/DC(전역만 · +문법 · +프로젝트 · +프로젝트[문법] · +탭) · 모르는 키 보존 왕복 · Global 전용 키 무시.
- 저장 왕복: 프로젝트 탭에서 바꾼 값 → 헤더 절 → 다시 열기 = 같은 값 · 다른 프로젝트로 전환 = 원래 값.
- 자동 시험은 격리 `NSQL_HOME` + `temp_dir()` 프로젝트 파일만.
