# 61 · 핵심 설계와 작업 규칙 — 다른 PC(맥)에서 그대로 이어 가기 (사용자 요청 09-20)

> **왜 이 문서인가**: 세션이 기억하는 규칙 가운데 일부는 **그 PC의 로컬 메모리 폴더**(`~/.claude/projects/…/memory/`)에만 있었다 — 다른 PC에서는 보이지 않는다. 저장소에 들어 있는 것만 따라온다. 이 문서는 ① 로컬에만 있던 규칙을 저장소로 올리고 ② 09-19~20에 굳어진 핵심 설계(편집 버퍼 · 되돌리기 · 큰 파일 · 적재)의 **불변식과 금지 사항**을 한곳에 모으고 ③ Windows와 macOS에서 다르게 해야 하는 것을 가른다.
> **읽는 순서(새 PC · 새 세션)**: [CLAUDE.md](../CLAUDE.md) → 이 문서 → [STATUS](STATUS.md) → [TODO](TODO.md). 형제 저장소 nexa-ui도 같은 순서(`../nexa-ui/CLAUDE.md` §3-1에 요약).
> 규칙의 원문이 따로 있는 것은 링크만 단다(이중 관리 금지) — 여기서 원문인 것은 §1의 "규칙" 줄과 §2-3 · §2-4 · §3이다.

## 1. 핵심 설계 — 불변식과 금지 사항

### 1-1. 편집 버퍼 `TextBuf`(nexa-ctl `edit/textbuf.rs` · [59 §6](59-large-file-handling.md))

- 저장 = UTF-8 갭 버퍼(`Vec<u8>` + 갭 하나) + **줄 시작 표**(줄마다 바이트 시작 · 글자 시작) + **줄 변경 기록**(`LineChange{first, removed, inserted}` · 통째 교체 세대 `epoch`).
- **좌표는 글자(char) 인덱스다.** 캐럿 · 선택 · 되돌리기 기록 · 기록 파일 · 호스트의 찾기/실행 범위/오류 줄이 전부 글자 단위다. 바이트 오프셋은 `textbuf.rs` 밖으로 나가지 않는다.
- 불변식: 갭의 양 끝은 글자 경계 · `lines[0] = (0, 0)` · `lines[k]` = k번째 `'\n'` 바로 뒤 · 논리 바이트열은 늘 온전한 UTF-8 · `unsafe` 0.
- **규칙 ①**: 본문 전체를 글자 배열(`Vec<char>`)이나 문자열(`String`)로 뜨는 코드를 **자주 도는 길**(키 입력 · 그리기 · 틱 · 상태줄)에 새로 넣지 않는다. `buf()`의 줄·구간 조회(`get` · `slice` · `iter_from`/`iter_rev_from` · `line_of`/`line_start`/`line_end`/`line_text` · `find` · `eq_str` · `byte_len` · `content_hash`)를 쓴다. 동작 한 번에 한 번(실행 · 정규식 찾기 · 저장)은 괜찮다. `EditState::chars_vec()`는 드문 명령·테스트 전용이다.
- **규칙 ②**: 줄별 캐시를 새로 만들면 **변경 기록의 소비자**로 만든다 — `(epoch, seq)`를 들고 `changes_since(seq)`로 바뀐 줄만 고치고, `None`(기록이 버려짐)이나 `epoch` 불일치면 전부 다시. 본문 세대(`rev`)만 보고 전 줄을 다시 계산하지 않는다(본보기 = `RowWidthCache` · `LineHlCache`).
- **규칙 ③**: 본문을 통째로 바꾸는 길은 `set_string`/`adopt`뿐이다(세대가 이어서 오른다 — 새 `TextBuf`로 그냥 갈아 끼우면 캐시가 "같은 세대"로 착각한다).
- 아직 본문 전체를 보는 것: 줄 변경 표시(기준선 비교) · 괄호 짝 표 · 접기(wrap) 모드 → T-145.

### 1-2. 되돌리기(nexa-ctl `edit.rs` · [60](60-undo-redo-redesign.md))

- 기록 = 연산 `Op{pos, remove_n, insert, insert_n}` · 적용하면 역연산이 나온다(되돌리기와 다시 실행이 한 함수) · **저장하는 것은 지운 글자뿐** · 묶음 안의 연산은 **뒤에서 앞으로** 적용.
- **규칙 ④ — 본문을 바꾸는 길은 셋뿐**: `splice_rec(_str)`(한 곳) · `replace_many_inner`(여러 곳) · `apply_ops`(되돌리기). 새 편집 기능은 이 셋 위에 짓는다 — `buf.splice`를 직접 부르면 세대(`rev`) · 기록 · 읽기 전용 · 거대 편집 확인이 전부 빠진다(83차에 실제로 있던 결함).
- 저장 지점 = 상태 id(`mark_saved`/`is_saved` · O(1)). 호스트는 본문 비교 전에 이것부터 본다. 저장하면 열린 타이핑 묶음을 끊는다.
- 메모리 = 바이트 예산(`editor.undo_budget_mb`) + 개수 상한 보조 · 최신 1단계는 늘 남긴다.
- 묶음 규칙 = 인접 타이핑·Backspace·Delete 합치기 + 단어 경계 + (보조) 긴 정지(`editor.undo_group_ms`). 시간 단독 기준은 쓰지 않는다.
- **거대 편집 확인**(`editor.undo_giant_mb`): 확인 수단은 **지우는 동작의 되풀이**(Delete · Backspace · 잘라내기 · 편집 명령 · 모두 바꾸기)뿐이다. **타이핑·붙여넣기·IME는 확인이 되지 않는다** — 새 편집 진입점을 만들면 `giant_refused(bytes, confirmable)`를 먼저 부르고 끝에 `giant_done()`.
- **기록 파일**(`undofile.rs` · `export_history`/`import_history`): 저장할 때 쓰고 · 열 때 본문 길이·해시가 같으면 들이고 · 하나라도 어긋나면 **통째로 버린다**(부분 복구 없음). 형식의 좌표 단위가 바뀌면 `HISTORY_VERSION`을 올린다. 기록에는 지운 글자가 평문으로 들어간다 — 설정 폴더 밖에 두지 않는다.

### 1-3. 큰 파일 · 적재(nexa-sql `editors.rs` · `fileload.rs` · [59 §5](59-large-file-handling.md))

- 큰 파일 모드 L1/L2 = 기능을 **끈다**(미니맵 · 선택어 강조 · 기준선 / + 구문 강조). 큰 탭은 **저장본 사본을 들지 않는다** → 더러움 = 저장 지점, 외부 변경 = 자동 병합 없이 띠로 묻는다, 기록 파일도 쓰지 않는다. 새 기능이 `saved[i]`(저장본 사본)에 기대면 큰 탭에서 빈 문자열을 본다는 것을 기억한다.
- **적재는 탭 안에 가둔다**: 8 MB 이상 = 자리 탭(`begin_load_tab` — 비어 있음 · 읽기 전용 · **경로 없음**) → 작업 스레드(읽기 → 글자 풀이 → `PreparedText`) → `fill_loaded`(활성 탭을 **빼앗지 않는다**). 진행 막은 그 탭이 활성일 때만 · 0.3초 넘게 걸릴 때만 · 100% 프레임 뒤에 편집 화면.
- **규칙 ⑤**: 오래 걸리는 일 때문에 **창 전체의 입력을 막지 않는다**(사용자 09-20 — 처음 요구를 같은 날 뒤집음). 막는 범위 = 그 일의 대상 하나(탭). 막는 명령의 판정은 순수 함수 + 테스트(`fileload::blocked_while_loading`).
- **규칙 ⑥**: 뒤에서 끝난 일이 실행으로 이어질 때는 **시작할 때의 탭(세션)** 을 확인한다 — "열지 않고 실행"은 읽는 사이 활성 탭이 바뀌면 실행하지 않는다(다른 접속 = 운영일 수도 있다).
- **규칙 ⑦**: 무거운 준비는 스레드에서, UI 스레드는 **옮겨 받기만**(`PreparedText` · `set_prepared` · 0 ms). 읽기 경로에 사본을 만들지 않는다(`decode_owned` · `detect_owned` — 온전한 UTF-8 · LF 파일이면 읽은 버퍼가 그대로 본문).
- 뷰 탭(본문 없는 탭 · 확장 상세 `ext_view.rs`): 편집 입력·찾기·실행이 가지 않는다 · 닫을 때 저장을 묻지 않는다. 읽을거리 "문서"(안내 탭)와 구분한다 — 화면이면 뷰 탭, 글이면 Plain Text + No connection 안내 탭.

### 1-4. 그 밖에 09-19~20에 굳은 것(원문 링크)

| 주제 | 핵심 | 원문 |
|---|---|---|
| 수동 커밋 잠금 방지 | 읽기 트랜잭션 자동 종료 · 유휴 경고 → 카운트다운 롤백 · 막힘 감지 · 분기 = `tx_guard_step` 순수 함수 + MC/DC | [56](56-manual-commit-lock-prevention.md) |
| 객체 탐색기 갱신 | 실행한 DDL → 그 폴더만 디프 · 커밋 시점 · 유휴 워터마크 · 우클릭 Refresh = 누른 자리가 범위(서버 = 전부 · 폴더 = 그 아래 · 객체 = 그것만) | [57](57-explorer-refresh-after-ddl.md) · [28 §1-2](28-object-explorer.md) |
| 외부 파일 변경 | 조용한 재로드 · 겹침 0이면 병합 · 비모달 띠 · 저장 2단 확인 · 감지 = 활성화·탭 전환·저장 직전 + 보이는 탭 폴링 · 재로드 = 최소 줄 편집 | [58](58-external-change-policy.md) · [60 §7](60-undo-redo-redesign.md) |
| 접속 유형 | 개발/시험/운영 · 운영 = 실행 2단 확인 + 짧은 유휴 기준(값·UI는 가정 — 사용자 확인 필요) | [56 §9](56-manual-commit-lock-prevention.md) |
| 메모리 회수 | 큰 것을 놓은 1초 뒤 1회 + 유휴 주기 · 보이지 않는 탭의 그리기 캐시 해제 · 워킹셋 트림은 하지 않는다 | [59 §2](59-large-file-handling.md) |
| 확장 | 관리자 끔 = 전 확장 정지 · 활동 막대 아이콘 = 관리자 켜짐일 때만 · 상세 = 뷰 탭 | [50 §14](50-extension-system.md) |

## 2. 작업 규칙(OS 공통)

### 2-1. 사용자와의 약속

- **답은 한글로**(진행 안내 한 줄까지 · 코드 식별자·명령어는 그대로 · 사용자 09-16).
- **진행 보고 규칙**(CLAUDE.md §3 맨 위): 지시를 받으면 즉시 진행 중 목록에 넣어 보여 주고 · 항목이 끝날 때마다 중간 보고 · 최종 보고에 **"충돌·조정"** 절(무엇과 무엇이 부딪쳤나 · 어느 쪽을 따랐나 = 기본은 나중 지시 · 그래서 무엇을 되돌렸나).
- **push는 사용자가 말할 때만.** "commit · main 병합 · push" = 작업 브랜치(`feat/…` · `fix/…` · `docs/…`) → 커밋 → `main`에 `--ff-only` 병합 → 로컬 브랜치 삭제 → `docs/BRANCHES.md`에 한 줄 → push(형제 저장소를 함께 고쳤으면 **nexa-ui 먼저**) → CI 결과를 journal에. 원문 = [16](16-doc-git-conventions.md).
- 스테이징은 `git add <파일>`만(`-A` · `.` 금지). 커밋 메시지 끝에는 그 세션이 안내하는 `Co-Authored-By:` 줄을 그대로 붙인다(모델 이름은 세션마다 다르다 — 지어내지 않는다).
- 문서 = 한 작업 한 트랜잭션(journal → DEVLOG → STATUS → MILESTONES/TODO → BRANCHES). CLAUDE.md의 "현 단계" 줄도 같이 본다(09-20에 52차에서 멈춰 있던 것을 발견).
- 테스트 실행 = Debug · 그 전에 **Release도 빌드**(사용자가 `target/release`로 병행 시험) · 끝나면 Debug 창을 **깨끗한 환경 변수로** 다시 띄워 둔다.

### 2-2. 코드 규칙(원문은 CLAUDE.md §3 · [30](30-architecture-patterns.md) · [39 §3](39-resource-governance.md) · [26 §8](26-performance-architecture.md))

사용자 문자열 = `nsql-i18n::Msg` · 설정 키 = `nsql-settings::REGISTRY`(구현 상수도 · 자주 안 바꾸면 HIDDEN) · 스레드·디스크·캐시 = 부하원 원장에 등재 + 끄거나 상한을 둘 설정 · 네트워크 = 26 §8 체크리스트 · DB 진입점 = `gate_open()` · 분기 조건 2개 이상 = 순수 함수 + MC/DC · 포커스 링 ≤ 1 · 마우스는 커서 아래 컨트롤에만 · hover = `IntentFade` · 팝업은 맨 마지막 층.

### 2-3. 검증 규칙(이번 주에 효과가 있었던 방법)

- **측정 먼저.** 느리다는 곳을 고치기 전에 재서 병목을 확인한다 — "읽기가 느릴 것"이라던 큰 파일 열기의 실제 병목은 UI 스레드의 행 폭 측정(3.8초)이었다. 도구 = §4.
- **자료 구조를 바꿀 때는 단순 모델과 난수로 대조한다**(자체 xorshift · 외부 crate 0): `TextBuf` ↔ `Vec<char>`(30 시드 × 150 편집) · 되돌리기 ↔ 상태 전체 스냅샷(40 × 120). 한글 · 이모지 · 개행 · 빈 본문 · 끝 개행을 알파벳에 넣는다.
- **교체는 호환 경로로 먼저 전체 테스트를 통과시킨 뒤** 자주 도는 길부터 새 API로 옮긴다(T-142: 임시 `chars_vec()`로 290개 통과 확인 → 이관).
- 새 구현이 옛 정의와 **같은 답**을 내는지 테스트로 남긴다(`eol::detect` 한 번 훑기 · `enc::decode_owned` · 줄 단위 찾기 · `TextBuf::find` = `str::find`).
- 결과는 Release로 잰다. Debug의 "멈춤"은 최적화 없는 빌드의 느림일 수 있다(65 MB).
- 보고는 정직하게: 하지 않은 것 · 시험하지 못한 것(키 입력이 필요한 것) · 기존 실패(CI)를 따로 적는다.

### 2-4. 격리 · 안전 규칙

- **앱을 띄워 시험할 때는 `NSQL_HOME`을 임시 폴더로 돌린다** — 사용자의 실제 설정 · 프로필 · 최근 파일 · 되돌리기 기록에 닿지 않게. 그리고 **정말 적용됐는지 확인한다**: 09-20에 셸 인자 실수로 격리 폴더가 엉뚱한 새 폴더로 잡혔고, 화면에 "Not connected"가 나온 것이 단서였다. 확인 방법 = 격리 폴더의 `settings.conf` 수정 시각 · 실제 설정 파일의 수정 시각이 그대로인지 · 임시 영역에 떠돌이 폴더가 생기지 않았는지.
- **단위 테스트는 실제 설정 폴더에 쓰지 않는다.** 파일을 다루는 모듈은 폴더를 인자로 받는다(`undofile::store_in(dir, …)` 패턴) · 테스트는 `std::env::temp_dir()` 아래에서만.
- **사용자가 자리에 있을 때 키·마우스 입력을 주입하지 않는다**(`SendKeys` · System Events 키 입력은 사용자의 전경 창으로 간다) · **포커스를 빼앗지 않는다**(`SetForegroundWindow` · `activate` 금지). 앱은 `NSQL_STARTUP_CMD`로 몰고 화면은 창 단위로 찍는다(§3 · §4). 키 입력이 있어야만 볼 수 있는 것(한글 조합 · 드래그)은 **단위 테스트로 덮고, 실기는 사용자 몫으로 보고에 적는다.**
- **사용자의 클립보드를 덮어쓰지 않는다** — 잘라내기·복사 명령을 자동 시험에 넣지 않는다(09-20: 65 MB 잘라내기 대신 줄 삭제 명령으로 시험).
- 실행 중인 다른 `nexa-sql` 프로세스가 사용자의 것일 수 있다 — 끝낼 때는 **내가 띄운 PID**만(빌드를 위해 Debug exe를 끝내는 것은 사용자가 허락한 예외).

### 1-5. 09-20~21(맥 86~88차)에 굳은 것 — 변수 · 트랜잭션 · 화면 내보내기

| 주제 | 핵심(불변식) | 원문 |
|---|---|---|
| **수동 커밋 = 진짜 트랜잭션**(T-146) | PG·SQLite·SQL Server·MySQL은 서버가 문장마다 커밋한다 → **수동 모드의 첫 문장 앞에서 러너가 연다**(`nsql-run` `manual_begin_sql` 순수 함수 + MC/DC · `tx_open`). 커밋·롤백·읽기 종료·접속에서 `tx_open`을 내린다 — **세션을 직접 커밋하는 새 경로를 만들면 `note_tx_ended()`를 같이 부른다**(안 부르면 다음 문장의 `BEGIN`이 "이미 트랜잭션 안"으로 실패) | [journal 09-21](journal/2026-09-21.md) 87차 후반 |
| **변수 표 = 세 층**(D-135) | 탭(App이 주인 `App.tab_vars` — 실행마다 `Cmd::Run.vars`로 넘기고 `RunEvent::Vars`로 돌려받는다) → 연결 공유(러너 = 세션 · `VAR x SHARE` · 호스트 사본 `Sess.shared_vars` · 고치면 `Cmd::SharedVars`) → 프로필(읽기 전용 · 아직 출처 없음). **변수를 `Runner`/`Sess`에 새로 두지 않는다**(탭이 주인) · 바인드(`VarStore`)와 치환(`Engine.defines`)은 섞지 않는다 | [63 §3](63-variable-management.md) |
| **결과가 여럿이면 탭도 여럿** | 같은 문장의 두 번째 이후 결과(REF CURSOR 여러 개 · 암묵 결과 · 다중 결과 집합) = 늘 딸린 탭(`ResultTab.child_of`) · 다른 문장의 결과 = 설정 `grid.result_per_statement`(D-138) · 판정 = `sessions::extra_result_slot` · 실행 끝에 안 쓰인 딸린 탭을 걷는다(`run_tracking` — **결과 새로고침은 걷지 않는다**) · `RunEvent::ResultSet.label` = 커서 변수 이름 | [63 §5](63-variable-management.md) |
| **1행 결과를 추측해 변수로 빨아들이지 않는다** | OUT 바인드가 없는 방언은 요청이 받는 쪽을 명시(`Prepared.captures` · 자리 순서) · Oracle/SQL Server는 결과 집합을 절대 흡수하지 않는다 · 0행·여러 행 = Oracle식 오류(D-139 · SQL Server는 서버 쪽 `@@ROWCOUNT` 검사) | [63 §3-1](63-variable-management.md) |
| **선언 없는 바인드의 타입 = 루틴 서명** | `nsql-script::call_shape` + `nsql-catalog::routine_args`(지금 Oracle `ALL_ARGUMENTS`만) · 루틴당 1회 캐시 · DDL·접속에서 비움 · 저장 코드 DDL(`CREATE … PROCEDURE/TRIGGER …`)은 바인드하지 않는다(`Prepared::verbatim` — `:NEW`/`:OLD`) | [63 §3](63-variable-management.md) |
| **실행 전에 한 번 묻는다**(D-137) | 워커가 `missing_inputs`로 찾고 `RunEvent::InputNeeded`를 낸 뒤 `InputReply`를 **기다린다**(세션 = 바쁨 · ■ = 입력 취소) · 창은 이벤트 루프가 있을 때(`about_to_wait`) 만든다 — **메뉴·기동 명령이 부탁한 창 열기 깃발은 `about_to_wait`에서도 본다**(`window_event` 끝에서만 보면 이벤트가 없을 때 안 열린다) | [63 §5](63-variable-management.md) |
| **화면 내보내기는 `present.rs` 한 곳** | 모든 창은 `present::Presenter`로 픽셀을 낸다(`softbuffer::Context/Surface`를 창 코드에 직접 두지 않는다) · macOS IOSurface 경로는 **기본 끔**(`gfx.mac_present`) — 화면·입력 실험은 기본 끔 + 실패 시 조용한 폴백 · 사용자가 병행 테스트하는 `target/` 빌드를 실험 상태로 두지 않는다 | [62 §2-1](62-macos-input-and-present.md) |
| 맥 한글 입력 | 한글 입력 소스일 때만 앱 조합(`input.hangul_compose=auto` · nexa-ctl `TextBox` 전역 스위치) — **새 창을 만들면 `set_ime_allowed(input::system_ime())` + `sync_hangul_mode`의 창 목록에 등록** | [62 §1](62-macos-input-and-present.md) |

## 3. OS별로 다른 것

| | Windows | macOS |
|---|---|---|
| 셸 | 도구의 Bash = Git Bash. **heredoc이 `\n` · `\0` · 백슬래시 · 따옴표를 망가뜨린다** → 코드 패치는 스크립트 **파일**로 써서 경로로 실행(한글 커밋 메시지 정도는 heredoc으로 된다). `"$VAR\\$1"`은 글자 그대로 `$1`이 된다 — 경로는 미리 변수에 조립한다 | zsh/bash 그대로. heredoc 안전 |
| 화면 캡처 | `PrintWindow(PW_RENDERFULLCONTENT)` — `scripts/win-capture.ps1` · `scripts/win-burst-capture.ps1`(연속 · 입력 주입 없음). `CopyFromScreen` · `FindWindow`는 실패한다 | `screencapture -l <창 id>` 또는 `-R x,y,w,h`. 기존 `scripts/mac-capture.sh`는 **System Events로 키를 보낸다** → 사용자가 자리에 없을 때(위키 캡처)만. 세션 중 검증은 `NSQL_STARTUP_CMD` + `screencapture`만 |
| 측정 | `scripts/win-big-probe.ps1`(초 단위 CPU · 상주 · 피크 · 응답) · `scripts/win-latency-probe.ps1` | `ps -o rss,%cpu -p <pid>` · `/usr/bin/time -l`(피크 RSS) · `sample <pid>` |
| 빌드 잠김 | 실행 중인 exe가 링크를 막는다 → 빌드 전에 Debug 프로세스 종료 | 없음 |
| 3-OS 검사 | nexa-ui = `scripts/check-3os.sh` 통과. nexa-sql = **이 PC에서는 교차 컴파일 불가**(`ring`에 교차 C 컴파일러 필요) → 호스트 fmt + clippy + 테스트만, 나머지는 CI | 맥에서는 `check-3os.sh`가 더 넓게 돈다(과거 맥 세션 기록) — push 전에 돌린다 |
| 글자 | GDI ClearType 글리프(D-77) — 전진폭이 정수로 스냅 | CoreText 경로 — 전진폭이 소수. **행 폭 고정폭 지름길은 탐침으로 전진폭을 구하므로 양쪽에서 돈다**(디버그 빌드가 실측과 대조). 맥에서 큰 파일을 한 번 열어 `debug_assert`가 조용한지 본다 |
| 메모리 회수 | `HeapSetInformation` + `HeapCompact` | `malloc_zone_pressure_relief`(형 검사만 했고 실기 미확인) |
| IME | 한글 조합 = 캐럿 줄 하나에만 끼움(단위 테스트 통과 · 실기는 사용자) | **T-139 맥 한글 입력**이 열려 있다 — 조합 경로가 84차에 바뀌었으므로(`Rows.over`) 맥에서 먼저 확인 |
| 설정 폴더 | `%APPDATA%\nexa-sql` | `~/Library/Application Support/nexa-sql`(`NSQL_HOME`이 있으면 그것) |

**윈도우에서 처음 할 일(맥 88차 뒤 · 09-21)**: ① 두 저장소 pull(**nexa-ui 먼저** — nexa-sys `layer_present` 추가분 · Windows에서는 늘 `None`인 빈 구현) → `cargo test --workspace` 양쪽(nexa-ui 352 · nexa-sql 353) ② Debug 빌드로 **창 11곳이 전부 열리고 그려지는지**(메인 · 로그인 · 파일 · 설정 · 색 · 단축키 · 로그 · 트랜잭션 로그 · 세션 · 툴바 플로팅 + 새 창 둘 — `present.rs`로 전부 바꿨다 · Windows 경로는 종전 softbuffer지만 손댄 곳이 넓다) ③ **새 창 둘의 실기**(맥에서만 확인했다): 변수 입력 창(`SELECT :X, '&y' FROM dual` 실행 → 격자 · Tab/Enter/Esc/Skip · ■ = 취소 · **Windows 한글 IME**로 값 입력) · 변수 창(View ▸ Variables — 줄 선택 · Set · NULL · Share/Local · Delete · `이름 = 값` · 탭 전환 따라감) — 포커스 링 ≤ 1 · 클릭이 영역 밖 컨트롤에 닿지 않는지(CLAUDE.md §3 포커스·마우스 규칙 ①~⑤) ④ **수동 커밋 실기**(T-146 — SQL Server·PG): Auto-commit 끔 → INSERT → 다른 세션에서 안 보임 → Rollback 버튼 → 사라짐 · Commit → 남음 · 상태줄 ●·툴바 배지 ⑤ 결과 탭: 조회 3개 스크립트 = 탭 3개 · 다시 실행 = 같은 자리 · 첫 탭 ↻ 새로고침에 다른 탭이 닫히지 않는지 · Oracle `examples/oracle-refcursor-pkg.sql` = 커서가 바로 결과로 ⑥ 그다음 [TODO](TODO.md) **T-149 잔여 → T-150~T-153**(아래 §6).

**맥에서 처음 할 일**: ① 두 저장소 pull(nexa-ui 먼저) → `cargo test --workspace` 양쪽 ② `scripts/check-3os.sh` ③ 큰 파일(수십 MB) 열기 · 한글 조합 입력 · 저장 → 재시작 → 되돌리기 ④ T-139.

## 4. 자체 검증 도구

- **기동 명령** `NSQL_STARTUP_CMD`(쉼표로 구분 · 3-OS 공통): `open:<경로>` · 명령 id(`edit.duplicate_line` · `file.save` · `view.log` …) · `@connected:<명령>`(첫 접속 뒤) · `@after:<ms>:<명령>`(시차) · `conn.edit:<프로필>` · `bigfile.open|readonly|head|run`(큰 파일 열기 선택) · `file.load_cancel`. 실행 인자로 프로필 이름(`Local`)을 주면 그 프로필로 접속한다.
- 환경 변수: `NSQL_HOME`(격리) · `NSQL_TRACE_FRAMES=1`(프레임 구간 · `[load] fill … ms`) · `NSQL_TRACE_MEM=1`.
- 벤치(nexa-ui): `cargo run --release -p nexa-ctl --example bench_editor <줄 수> <기능>`(`hl,ln,base,mm,occ,br` 또는 `all` · `BENCH_ASCII=1` · `BENCH_PREPARED=1`) · `--example bench_undo`.
- 스크립트: `scripts/win-capture.ps1` · `scripts/win-burst-capture.ps1` · `scripts/win-big-probe.ps1` · `scripts/win-latency-probe.ps1` · `scripts/win-badge-probe.ps1` · `scripts/mac-capture.sh` · `scripts/check-3os.sh`.
- 기준 수치(Release · 09-20 · 이 Windows PC): 65 MB 파일 상주 87 MB · 피크 106 MB · 70만 줄 입력 3~4 ms · 그리기 2 ms · 4만 줄 전 기능 입력 8 ms. 맥에서 크게 다르면 원인을 본다.

## 5. 로컬 메모리에서 저장소로 올린 것(이 PC의 `memory/` → 여기)

| 로컬 메모리 | 이제 있는 곳 |
|---|---|
| 답은 한글로 | §2-1 · CLAUDE.md §3 |
| Debug 실행 + Release 빌드 | §2-1 · CLAUDE.md §3(기존) |
| 포커스 링 규칙 · 마우스 라우팅 규칙 | CLAUDE.md §3(기존) |
| 형제 저장소 SSH 별칭 | CLAUDE.md §1(기존) |
| GUI 자체 캡처 방법 | §3 · §4 · `scripts/` |
| (메모리에 없던 것) 입력 주입 금지 · 포커스 · 클립보드 · 격리 확인 · 테스트의 실제 설정 폴더 금지 · heredoc · 브랜치 흐름 | §2-1 · §2-4 · §3 |

## 6. 이어 받을 일 — 맥 88차(09-21) 끝의 상태와 다음 순서

> 상태: nexa-sql `main` = 88차 마감 · CI `ci` ✅ · `integration` ✅ 9/9(Oracle·SQL Server·PostgreSQL 컨테이너) · nexa-ui `main` = 50차. 설계 SSOT = [63 변수 관리](63-variable-management.md)(단계표 §5 · 결정 §7) · 기록 = [journal 09-21](journal/2026-09-21.md).

| 순서 | 할 일 | 어디를 고치나 · 어떻게 확인하나 |
|---|---|---|
| 1 | **Windows 실기**(위 "윈도우에서 처음 할 일" ②~⑤) | 결과는 journal에 실기표로 · 결함은 그 자리에서 |
| 2 | **T-150 PostgreSQL refcursor** — `SELECT f()`가 돌려준 커서 이름을 `FETCH ALL IN "이름"`으로 받아 결과 탭으로 | `nsql-driver-pg`: 단순 질의 경로에는 **열 타입이 없다**(`Column.type_name` 빈 글) → 커서 경로(`execute_cursor` — 드라이버가 `BEGIN`을 쥔다)나 `prepare`로 타입(`refcursor`)을 얻은 뒤, **같은 트랜잭션 안에서** FETCH → `ExecResult.result_sets`에 덧붙이고 `CLOSE` · 자동 커밋이면 드라이버/러너가 트랜잭션으로 감싼다(T-146 `tx_open`과 충돌하지 않게) · 검증 = `integration.rs`에 PG 함수(`RETURNS refcursor` · `RETURNS SETOF refcursor`) 테스트 → 작업 브랜치에서 `gh workflow run integration.yml --ref <브랜치>`(main을 건드리지 않고 실서버 확인 — 09-21에 쓴 방법) |
| 3 | **T-151 서명 추론 넓히기 + 타입** | `nsql-catalog::routine_args`에 PG(`pg_proc`/`pg_get_function_arguments`) · SQL Server(`sys.parameters`) · MySQL(`information_schema.PARAMETERS`) · `VarType`에 DATE/TIMESTAMP 실제 바인드(Oracle 드라이버는 지금 스칼라 OUT을 전부 VARCHAR2(4000)로 묶는다) · BOOLEAN · `Direction::Out`을 실제로 만든다(지금은 전부 InOut) |
| 4 | **T-152 `Caps` 포트**([63 §3](63-variable-management.md)) | `nsql-script/dialect.rs`·`engine.rs`의 방언 `match`(`wrap_exec` · `prepare` · `exec_targets`의 `Oracle | Mssql` 판정 · 러너의 `absorb` 방언 게이트)를 드라이버 능력 질의로 — 포트(trait) + 레지스트리 + 설정([30](30-architecture-patterns.md)) · MySQL/MariaDB·NoSQL 어댑터가 능력표만 채우면 붙게 |
| 5 | **T-153 변수 UX 잔여** | `ACCEPT [HIDE] [DEFAULT]` · `COLUMN … NEW_VALUE`(`nsql-script/command.rs`) · 입력 창에 타입 열·미리보기(`input_win.rs`) · **MacroStore 분리 + 시스템 변수**(`_USER` `_DATE` `_ROW_COUNT` `_SQLCODE` `_ELAPSED_MS`) · `${v:형식}` · CLI `-v name=value` · 편집기 hover에 변수 값 · 미정의 변수 경고(`Diagnostic::ImplicitVariable`은 만들어지기만 하고 **소비자가 없다**) · 값 크기 상한 `vars.max_value_kb` · 프로필 층의 출처(접속 프로필 필드) · 이름 없는 탭 보존(S-1 hot exit와 함께) |
| 6 | 그 밖에 열려 있던 것 | **T-145**(줄 변경 표시·괄호 표의 줄 단위 갱신) · T-147 잔여(`nexa-gfx::Surface` stride → 중간 버퍼 제거 · D-133 기본 전환) · T-148 잔여(SQLite 열기 실패 14가 "Connection lost"로 분류됨 · MySQL `DELIMITER`) · T-136 · T-103 위키 · T-105 · T-106 |

**알아 둘 흠(09-21에 봤지만 고치지 않은 것)**: ① PG 수동 커밋에서 문장 하나가 실패하면 트랜잭션이 *aborted* 상태로 남아 Rollback 전까지 전부 실패한다(PG의 정상 동작 · psql `ON_ERROR_ROLLBACK`처럼 문장마다 SAVEPOINT를 두는 선택지를 [56](56-manual-commit-lock-prevention.md)에 검토로 남길 것) ② `VAR X NUMBER`는 이름을 대문자로 만든다(표기 보존 D-142는 `EXEC :x := …` 경로만) ③ `VAR X`(타입 없음)는 선언하지 않고 조회만 한다 ④ `vars.show`(SHOW VARIABLES)는 접속이 있어야 돈다(`gate_open`) — 변수 창은 접속 없이도 열린다 ⑤ `open_sessions`·`open_txlog` 같은 창 열기 깃발은 아직 `window_event` 끝에서만 본다(메뉴 클릭은 문제없고 `NSQL_STARTUP_CMD`로 열 때만 늦다 — `open_vars`처럼 `about_to_wait`에도 두면 된다) ⑥ `.devcontainer/devcontainer.json`은 `NSQL_POSTGRES_URL`인데 통합 테스트는 `NSQL_PG_URL`을 본다(Codespaces에서 PG 테스트가 조용히 건너뛰어진다 — 이름을 맞출 것) ⑦ `.devcontainer/devcontainer-lock.json`은 사용자 파일이라 추적하지 않았다.

**결정 대기**: D-133(맥 화면 내보내기 기본값 → `iosurface`?) · D-134(`input.hangul_compose` 기본 auto 확인) · D-87~90 · D-115~118 · Codespaces = 사용자 `gh auth refresh -h github.com -s codespace` 1회 뒤 가능.

