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
| 메모리 회수 | 큰 것을 놓은 1초 뒤 1회 + 유휴 주기 · 보이지 않는 탭의 그리기 캐시 해제 · 워킹셋 트림은 하지 않는다 | [59 §2](59-large-file-handling.md) | glibc `malloc_trim(0)` |
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

- ★ **팝업 배치 규칙**(사용자 09-21 · 전수 조사 89차): 우클릭 메뉴·툴팁·드롭다운은 **그리기 표면 밖으로 나가지 않는다.**

  | 종류 | 쓰는 부품 | 배치 | 안전망 |
  |---|---|---|---|
  | 우클릭 메뉴 · 하위 메뉴(결과 탭 · 편집기 탭 · 편집기 본문 · 그리드 · 탐색기 · 접속 창 · 로그/세션/트랜잭션 로그 창 · 입력란 편집 메뉴 · 파일 대화상자 — 호출 22곳) | nexa-ctl `ContextMenu` | `geom::place_popup` — 정방향(오른쪽·아래) → 반대쪽(끝이 기준점에 닿게) → 가장 가까운 자리로 밀어 넣기 | 첫 paint가 `ctx.surface_size()`를 배워 `geom::nudge_into`로 표면 안에 · 하위 메뉴는 부모가 배운 크기를 물려받아 접는다 |
  | 툴팁(툴바·도크·플로팅 툴바 · 편집기 탭 · 찾기 막대 · 검색 패널 · 접속 창 · 로그 창 — 호출 7곳) | nexa-ctl `draw::draw_tooltip(_in)` | 기준 아래 6px → 아래로 넘치면 **기준 위** → 그래도 안 되면 밀어 넣기 · 가로는 호출자 범위 ∩ 표면 | 같은 함수 안에서(종전 = 늘 아래 · 가로만 맞춤 → 창 아래쪽 도구줄에서 잘렸다) |
  | 드롭다운(설정 창 · 접속 폼 · 위치 드롭다운) | nexa-ctl `Combo` | 아래 → 넘치면 위로(`viewport_bottom`) | 호스트가 하한을 안 줘도 paint가 표면 높이를 배운다(`surface_bottom`) |

  규칙: ① 위치를 직접 계산하지 않는다(위 세 부품 · `place_popup`/`nudge_into`/`popup_host`) ② 글자 상자를 직접 그려 툴팁·메뉴를 흉내 내지 않는다 ③ `host`는 아는 한 창 전체 Rect(끝없는 영역을 넘기는 자리는 주석으로 안전망에 기댄다고 적는다 — 지금 결과 탭 · 편집기 탭 · 접속 창 네 곳) ④ 새 팝업 = 창 모서리 근처에서 연 캡처 1장(짧은 창 = 격리 폴더 `window.main_size` · 기동 명령으로 여는 길을 같이 만든다) ⑤ 창보다 큰 팝업은 스크롤이 있어야 한다. `DrawCtx::surface_size()`를 구현하지 않은 그리기 컨텍스트(테스트 기록기)는 `None` = 종전 동작.

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
- **프로세스 전역 상태(스위치 · 환경 변수 · 현재 폴더)를 만지는 시험은 가드로 직렬화한다** — 시험은 병렬로 돈다. 본보기 = nexa-font `tests::GdiOn`(정적 뮤텍스를 쥔 동안 켬 · Drop에서 끔). 09-22(92차): 가드 없던 두 시험이 CI windows-latest에서만 겹쳐 네 번 실패했고, 로그를 못 본 채 "되돌리니 통과"로 엉뚱한 원인(`file_type()`)을 지목했다 → **CI 실패는 로그를 본 뒤에 고친다**(`gh run view <id> --log-failed` · `gh` 없는 PC면 있는 PC 몫으로 넘긴다) · "되돌리니 통과"는 원인의 증거가 아니다(타이밍도 같이 돌아간다).
- **사용자가 자리에 있을 때 키·마우스 입력을 주입하지 않는다**(`SendKeys` · System Events 키 입력은 사용자의 전경 창으로 간다) · **포커스를 빼앗지 않는다**(`SetForegroundWindow` · `activate` 금지). 앱은 `NSQL_STARTUP_CMD`로 몰고 화면은 창 단위로 찍는다(§3 · §4). 키 입력이 있어야만 볼 수 있는 것(한글 조합 · 드래그)은 **단위 테스트로 덮고, 실기는 사용자 몫으로 보고에 적는다.**
- **사용자의 클립보드를 덮어쓰지 않는다** — 잘라내기·복사 명령을 자동 시험에 넣지 않는다(09-20: 65 MB 잘라내기 대신 줄 삭제 명령으로 시험).
- ★ **확인 요청의 대원칙**(사용자 09-22 92차 — "필수 스크립트에 계속 확인을 요청하는 것은 매우 불필요하고 작업이 Hold되는 원인 · 삭제 안전 확인의 의지는 **프로젝트 외부 리소스를 함부로 수정/삭제하지 않는 안전장치**였다" · 09-21 89차의 정정):

  | | 대상 | 규칙 |
  |---|---|---|
  | **묻지 않고 진행** | ① 진행에 필요한 스크립트·명령 실행(cargo · git 조회/커밋 · python · pwsh `scripts/` · gh 조회 · 캡처·측정 스크립트 · 테스트) ② 프로젝트(nexa-sql · nexa-ui) **내부**에서 생성/관리하는 파일의 수정·삭제 ③ 프로젝트 **외부라도 개발 세션이 만든** 파일·폴더의 수정·삭제 — 세션 스크래치패드 · 격리 `NSQL_HOME` · 비교용 워크트리(`../_cmp/<저장소>`)와 그 `target/`·로그 · `target/` 산출물·`target/capture` · `/tmp`·`$TEMP/claude` ④ 이 저장소 `target/` 아래 `nexa-sql.exe` 인스턴스 종료(아래 줄) ⑤ 실서버의 임시 객체(`pg_temp` · `NSQLT_*` — 세션이 끝나면 사라지거나 시나리오가 지우는 것) | 보고에 한 줄 · 지우기 전에 **내가 만든 것인지 확인**(경로·내용·만든 시각) |
  | **먼저 묻는다** | 프로젝트 **밖의 사용자 리소스**(실제 설정 폴더·프로필·볼트·최근 파일 · 다른 프로젝트 폴더 · 홈의 파일)의 수정/삭제 · 추적 중인 변경을 되돌릴 수 없게 버리는 일(`git reset --hard` · `git clean` · 강제 push · 브랜치 강제 삭제) · 내가 띄우지 않은 **다른 앱**의 프로세스 · 실서버의 영속 객체 · `devcontainer-lock.json` | 무엇을·왜를 말하고 승인을 받는다 |

  정리는 **그 일에 맞는 명령**을 쓴다(워크트리 = `git worktree remove` · 빌드 산출물 = `cargo clean -p …` · 프로세스 = PID 지정) — 범위가 명령 자체에 묶여 있어 엉뚱한 경로를 건드릴 수 없다.
  **확인 창의 출처**: 규칙 문서가 아니라 하네스 권한 설정 `.claude/settings.json`(allow = 명령 **첫 단어** 기준 · `&&`·`;`·`|` 체인은 조각마다 판정 · `ask` = 늘 묻는 것)이다. 09-22 점검: 세션의 명령이 `cd … && …`·변수 대입·`for`/`until`·PowerShell `$x = …; Stop-Process …`처럼 허용 목록에 없는 단어로 시작해 매번 확인/분류기로 갔고, `rm:*`·`Remove-Item:*`이 `ask`라 스크래치·`target/` 정리도 물었다 → allow를 넓히고(`cd`·`sed`·`for`·`until`·`Stop-Process`·`Start-Process`·`$`·스크래치/`target/` 한정 `rm`) `rm`·`Remove-Item`은 `ask`에서 뺀다(외부 삭제 금지는 이 규칙이 지킨다). 이 파일은 하네스 분류기가 **내 편집을 막으므로** 바뀐 내용은 제안 파일(`settings.proposed.json`)로 만들어 사용자가 적용한다. 명령을 쓸 때는 허용된 단어로 시작하고(절대 경로 인자 · 로직은 python 파일), 체인은 짧게.
- 실행 중인 다른 프로세스는 사용자의 것일 수 있다 — 끝낼 때는 **내가 띄운 PID**만. **예외 = 이 저장소 `target/{debug,release}/nexa-sql.exe` 인스턴스**: 개발(빌드·테스트·재시작) 사이에는 누가 띄웠든 **강제 종료하고 진행**한다(사용자 09-22 92차 "개발 사이에는 기존 프로세스를 강제 종료하고 진행" — 실행 중인 exe가 링크를 막는다) · 종료한 PID·시작 시각은 보고에 한 줄 · 다른 경로의 exe(설치본)·다른 앱은 그대로.

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

| | Windows | macOS | Linux(09-22 · Ubuntu 26.04 · Wayland) |
|---|---|---|---|
| 셸 | 도구의 Bash = Git Bash. **heredoc이 `\n` · `\0` · 백슬래시 · 따옴표를 망가뜨린다** → 코드 패치는 스크립트 **파일**로 써서 경로로 실행(한글 커밋 메시지 정도는 heredoc으로 된다). `"$VAR\\$1"`은 글자 그대로 `$1`이 된다 — 경로는 미리 변수에 조립한다 | zsh/bash 그대로. heredoc 안전 | bash/zsh 그대로. heredoc 안전. 도구의 비대화 셸은 `~/.zshrc`를 읽지 않으므로 Oracle 환경(`LD_LIBRARY_PATH`)은 스크립트에서 직접 export |
| 화면 캡처 | `PrintWindow(PW_RENDERFULLCONTENT)` — `scripts/win-capture.ps1` · `scripts/win-burst-capture.ps1`(연속 · 입력 주입 없음). `CopyFromScreen` · `FindWindow`는 실패한다 | `screencapture -l <창 id>` 또는 `-R x,y,w,h`. 기존 `scripts/mac-capture.sh`는 **System Events로 키를 보낸다** → 사용자가 자리에 없을 때(위키 캡처)만. 세션 중 검증은 `NSQL_STARTUP_CMD` + `screencapture`만 | Wayland라 `xwininfo`·`xdotool`이 앱 창을 못 본다 → 화면 캡처 대신 **`NSQL_TRACE_FRAMES`의 stderr**(`[startup]` · `[frames]` · `[load]`)와 `/proc`으로 판정 · 캡처가 꼭 필요하면 `gnome-screenshot`/`grim`(미검증) |
| 측정 | `scripts/win-big-probe.ps1`(초 단위 CPU · 상주 · 피크 · 응답) · `scripts/win-latency-probe.ps1` · `scripts/win-leak-cycle.ps1`(같은 동작 N회 → 뒤 절반의 기울기 = 릭 판정) · `scripts/win-startup-probe.ps1`(창이 보이기까지 · 안정까지 CPU) — ★ **빌드 직후 첫 실행은 버린다**(새 exe의 실시간 검사로 CPU가 수백 ms~2초 튄다 · 09-21 89차) · 회귀 판정은 **앞 커밋을 워크트리에 따로 빌드해 같은 시각에 A/B**로 | `ps -o rss,%cpu -p <pid>` · `/usr/bin/time -l`(피크 RSS) · `sample <pid>` | `scripts/linux-startup.sh` · `linux-probe.sh` · `linux-leak.sh`(Windows 셋의 이식 · `/proc/<pid>/{stat,status}` · **RssAnon ≈ Private** — Wayland 화면 버퍼는 `memfd` 공유 매핑이라 익명에 안 잡힌다) · `linux-perf-all.sh`(26 §7-3 전 시나리오 한 번에) · `strace -f -ttt`로 기동 타임라인(`perf record`는 `perf_event_paranoid=4`라 불가) · 벤치 수치는 기기(VM 2코어)가 느린 폭(1.5~2×)을 감안 |
| 빌드 잠김 | 실행 중인 exe가 링크를 막는다 → 빌드 전에 Debug 프로세스 종료 | 없음 | 없음. Release(LTO fat) 워크스페이스 = 2코어에서 6.5분 |
| 3-OS 검사 | nexa-ui = `scripts/check-3os.sh` 통과. nexa-sql = **이 PC에서는 교차 컴파일 불가**(`ring`에 교차 C 컴파일러 필요) → 호스트 fmt + clippy + 테스트만, 나머지는 CI | 맥에서는 `check-3os.sh`가 더 넓게 돈다(과거 맥 세션 기록) — push 전에 돌린다 | Windows PC와 같다 — 교차 타깃은 `ring`이 교차 C 컴파일러를 요구해 실패 → 호스트 fmt + clippy + 테스트만(`scripts/linux-all-tests.sh`), 나머지는 CI |
| 글자 | GDI ClearType 글리프(D-77) — 전진폭이 정수로 스냅 | CoreText 경로 — 전진폭이 소수. **행 폭 고정폭 지름길은 탐침으로 전진폭을 구하므로 양쪽에서 돈다**(디버그 빌드가 실측과 대조). 맥에서 큰 파일을 한 번 열어 `debug_assert`가 조용한지 본다 | nexa-font 고정 경로 후보(Noto Sans CJK KR · DejaVu) + **가족 탐색 = `/usr/share/fonts` 트리 걷기**(D2Coding · 기호 폴백) — 09-22 걷기 1회 캐시 + `file_type()`로 고침(기동 100~700 ms가 이것이었다 · 92차(win)에 3-OS 공통 한 길로 — Windows도 걷기 12.6 → 1.2 ms · 한 번만) · softbuffer 한 길 · **창 백엔드 기본 = X11(XWayland · `gfx.linux_backend`)** — 모달 = `WM_TRANSIENT_FOR`+`_NET_WM_STATE_MODAL`(x11rb · `winfocus::attach_child` · winit 0.30 Wayland는 부모·올리기 API 없음) |
| 메모리 회수 | `HeapSetInformation` + `HeapCompact` | `malloc_zone_pressure_relief`(형 검사만 했고 실기 미확인) | glibc `malloc_trim(0)` |
| IME | 한글 조합 = 캐럿 줄 하나에만 끼움(단위 테스트 통과 · 실기는 사용자) | **T-139 맥 한글 입력**이 열려 있다 — 조합 경로가 84차에 바뀌었으므로(`Rows.over`) 맥에서 먼저 확인 | 미확인 — 기본 백엔드가 X11이라 XIM 경로 · Wayland 네이티브면 winit `Ime`(실기 대기 · T-163) |
| 설정 폴더 | `%APPDATA%\nexa-sql` | `~/Library/Application Support/nexa-sql`(`NSQL_HOME`이 있으면 그것) | `~/.config/nexa-sql`(`NSQL_HOME`이 있으면 그것) · Oracle Instant Client = `~/oracle/instantclient_23_26`(`scripts/install-instantclient-linux.sh`) |

**윈도우에서 처음 할 일(맥 88차 뒤 · 09-21)**: ① 두 저장소 pull(**nexa-ui 먼저** — nexa-sys `layer_present` 추가분 · Windows에서는 늘 `None`인 빈 구현) → `cargo test --workspace` 양쪽(nexa-ui 352 · nexa-sql 353) ② Debug 빌드로 **창 11곳이 전부 열리고 그려지는지**(메인 · 로그인 · 파일 · 설정 · 색 · 단축키 · 로그 · 트랜잭션 로그 · 세션 · 툴바 플로팅 + 새 창 둘 — `present.rs`로 전부 바꿨다 · Windows 경로는 종전 softbuffer지만 손댄 곳이 넓다) ③ **새 창 둘의 실기**(맥에서만 확인했다): 변수 입력 창(`SELECT :X, '&y' FROM dual` 실행 → 격자 · Tab/Enter/Esc/Skip · ■ = 취소 · **Windows 한글 IME**로 값 입력) · 변수 창(View ▸ Variables — 줄 선택 · Set · NULL · Share/Local · Delete · `이름 = 값` · 탭 전환 따라감) — 포커스 링 ≤ 1 · 클릭이 영역 밖 컨트롤에 닿지 않는지(CLAUDE.md §3 포커스·마우스 규칙 ①~⑤) ④ **수동 커밋 실기**(T-146 — SQL Server·PG): Auto-commit 끔 → INSERT → 다른 세션에서 안 보임 → Rollback 버튼 → 사라짐 · Commit → 남음 · 상태줄 ●·툴바 배지 ⑤ 결과 탭: 조회 3개 스크립트 = 탭 3개 · 다시 실행 = 같은 자리 · 첫 탭 ↻ 새로고침에 다른 탭이 닫히지 않는지 · Oracle `examples/oracle-refcursor-pkg.sql` = 커서가 바로 결과로 ⑥ 그다음 [TODO](TODO.md) **T-149 잔여 → T-150~T-153**(아래 §6).

**리눅스에서 처음 할 일(09-22 91차)**: ① 두 저장소 pull(nexa-ui 먼저) → `scripts/linux-all-tests.sh -o /tmp/nsql-tests`(fmt·clippy·test 양쪽 + 3-OS 호스트 + 실서버 통합 = `NSQL_*_PROFILE`) ② Oracle = `scripts/install-instantclient-linux.sh --rc`(새 셸) ③ 성능 = `scripts/linux-perf-all.sh -H <격리 홈> -D <데이터> -o <결과>`(격리 홈에 `Local` SQLite 프로필 먼저 · 26 §7-7과 비교 · 창까지 시간은 `[startup]`으로 구간 확인) ④ 실기 = TODO T-163(Wayland IME · 모달 · 캡처).

**맥에서 처음 할 일**: ① 두 저장소 pull(nexa-ui 먼저) → `cargo test --workspace` 양쪽 ② `scripts/check-3os.sh` ③ 큰 파일(수십 MB) 열기 · 한글 조합 입력 · 저장 → 재시작 → 되돌리기 ④ T-139 ⑤ **하네스 권한 점검**(아래 §3-1 — 09-23 맥·Linux 몫을 한 번에 적용 · 새 확인 창이 뜨면 그 첫 단어를 §3-1 목록에 더해 다시 제안한다).

### 3-1. 하네스(Claude Code) 권한 설정 — 3-OS 공통 규칙과 OS별 추가분(사용자 09-22 "동일 권한 관리를 mac에서도 다시 요청할 수 있게")

- **원칙** = §2-4 대원칙: 확인은 **프로젝트 외부 리소스의 수정/삭제**에만. 진행에 필요한 스크립트 실행 · 프로젝트 내부 파일 · 세션이 만든 파일/폴더 · `target/`의 앱 프로세스는 묻지 않는다.
- **어디에**: 저장소의 `.claude/settings.json`(커밋됨 · 3-OS가 같은 파일을 읽는다 · `defaultMode = acceptEdits` · `allow` = 명령 **첫 단어** 기준 · `&&`·`;`·`|` 체인은 조각마다 판정 · `ask` = `sudo` · `git reset --hard` · `git clean` · 강제 push만). 이 파일은 하네스의 자동 모드 분류기가 **에이전트의 편집을 막는다**("자기 수정") → 바꿀 것이 생기면 스크래치패드에 완성본 `settings.proposed.json`을 만들고 **사용자가 복사**한다(09-22 Windows에서 그렇게 적용).
- **Windows(09-22 적용됨)**: Bash `cd sed cut sort uniq tr fold paste printf xargs diff stat du dirname basename jq curl sleep timeout test true env export tasklist taskkill` · `for /until /while /if /[` · `bash scripts/` · `rm`은 **스크래치(`"$TEMP/claude/` · `C:/Users/<user>/AppData/Local/Temp/claude/`) · `/tmp/` · `target/` · `../_cmp/` · `../nexa-ui/target/` 한정** / PowerShell `cd Stop-Process Start-Process Start-Sleep New-Item Set-Content Add-Content Add-Type Copy-Item Move-Item Rename-Item Remove-Item Env:` · 스크래치/`target/` 한정 `Remove-Item -Recurse -Force` · `Out-String Out-Null Where-Object ForEach-Object Sort-Object Get-Date Join-Path Split-Path foreach/if/try` · `$…` · `"…"` · `[…]` · `& "$root\target\…"`.
- **macOS · Linux(09-23 mac 100차 · 제안 파일로 적용)**: 맥 세션도 분류기에 막혀(`[Self-Modification]`) 스크래치패드에 완성본 `settings.proposed.json`을 만들고 사용자가 복사했다. **한 파일을 3-OS가 읽으므로 맥·Linux 몫을 한 번에 넣었다** — Linux 세션은 `main`을 pull하면 그대로 받는다(추가 확인 창이 뜨면 아래 Linux 목록에 더하고 다시 제안). 항목 = Unix 공통 + macOS + Linux(기존 항목 전부 유지 · `ask`에 `pkexec`·`apt`·`apt-get` 추가).

  ```json
  // Unix 공통(macOS · Linux) — 스크래치 = 세션이 알려 주는 `/private/tmp/claude-<uid>/…`(맥) · `$TMPDIR/claude/` · `$XDG_RUNTIME_DIR/claude/`(Linux) · `/tmp/`(기존)
  "Bash(rm -rf /private/tmp/claude-:*)", "Bash(rm -f /private/tmp/claude-:*)",
  "Bash(rm -rf \"$TMPDIR/claude/:*)", "Bash(rm -f \"$TMPDIR/claude/:*)",
  "Bash(rm -rf \"$XDG_RUNTIME_DIR/claude/:*)", "Bash(rm -f \"$XDG_RUNTIME_DIR/claude/:*)",
  "Bash(pkill -f target/:*)", "Bash(kill:*)", "Bash(ps:*)", "Bash(pgrep:*)", "Bash(lsof:*)",
  "Bash(nohup target/:*)", "Bash(nohup ./target/:*)", "Bash(./target/:*)", "Bash(../nexa-ui/target/:*)",
  "Bash(sh scripts/:*)", "Bash(zsh:*)", "Bash(/usr/bin/time:*)", "Bash(uname:*)", "Bash(nproc:*)", "Bash(getconf:*)",
  "Bash(tee:*)", "Bash(seq:*)", "Bash(bc:*)", "Bash(id:*)", "Bash(ldd:*)", "Bash(size:*)", "Bash(readelf:*)",
  // macOS
  "Bash(rm -rf /var/folders/:*)", "Bash(rm -f /var/folders/:*)",
  "Bash(screencapture:*)", "Bash(open -a:*)", "Bash(open target/:*)", "Bash(osascript -e:*)",
  "Bash(xattr:*)", "Bash(codesign:*)", "Bash(otool:*)", "Bash(nm:*)",
  "Bash(sw_vers:*)", "Bash(sysctl:*)", "Bash(vm_stat:*)", "Bash(sample:*)",
  // Linux(scripts/linux-*.sh · install-*-linux.sh가 쓰는 첫 단어 + 창 조회·캡처)
  "Bash(grim:*)", "Bash(gnome-screenshot:*)", "Bash(xdg-open target/:*)",
  "Bash(xprop:*)", "Bash(xwininfo:*)", "Bash(xdotool search:*)", "Bash(xdotool getwindow:*)",
  "Bash(strace:*)", "Bash(ltrace:*)", "Bash(perf:*)", "Bash(free:*)", "Bash(lscpu:*)",
  // ask에 추가
  "Bash(pkexec:*)", "Bash(apt:*)", "Bash(apt-get:*)"
  ```
  (`pkill`은 `target/` 아래 nexa-sql만 — 다른 앱은 여전히 묻는다 · `kill`은 PID 지정이라 허용 · `xdotool`은 조회 동사(`search`·`getwindow*`)만 — 키 주입 `key`/`type`은 허용하지 않는다(61 §2 입력 주입 금지) · `sudo`·`pkexec`·패키지 설치는 묻는다 · PowerShell 항목은 Unix에서 무해하게 무시된다 · `osascript -e`는 창 이름 조회 같은 읽기용 — System Events 키 주입은 규칙으로 금지.)
- **적용 절차(3-OS 공통)**: ① 세션이 스크래치패드에 `settings.proposed.json`(현재 파일 + 추가분 · 기존 항목 전부 유지)을 만들고 `comm`으로 손실 0을 확인 ② 사용자가 복사 — 맥/Linux `cp <스크래치>/settings.proposed.json .claude/settings.json` · Windows `Copy-Item` ③ `jq -e '.permissions.allow|length' .claude/settings.json`으로 유효한 JSON인지 ④ 커밋(3-OS가 같은 파일을 받는다).
- **점검법**: 세션 초반에 확인 창이 두 번 이상 뜨면 그 명령의 첫 단어를 적어 두고 한 번에 제안 파일로 모아 부탁한다(하나씩 묻지 않는다). 제안은 **기존 항목 전부 유지 + 추가**만(병합) · `ask`의 되돌릴 수 없는 git 항목은 지우지 않는다.

## 4. 자체 검증 도구

> ★ **"성능 평가해줘"가 오면** — 도구를 고르기 전에 [71 성능 종합 점검 프로세스](71-performance-review-process.md)를 편다: 규정 대상 12차원 · 순서 A(인벤토리) → B(기동) → C(시나리오) → D(향상 모드 A/B) → E(누수) → F(벤치) → G(병목 추적) · 판정선 · 생략할 때는 **왜 생략했는지**를 보고에 적는다. 실행기 = Windows `scripts/win-perf-all.ps1` · Linux `scripts/linux-perf-all.sh`.


- **기동 명령** `NSQL_STARTUP_CMD`(쉼표로 구분 · 3-OS 공통): `open:<경로>` · 명령 id(`edit.duplicate_line` · `file.save` · `view.log` …) · `@connected:<명령>`(첫 접속 뒤) · `@after:<ms>:<명령>`(시차) · `conn.edit:<프로필>` · `bigfile.open|readonly|head|run`(큰 파일 열기 선택) · `file.load_cancel`. 실행 인자로 프로필 이름(`Local`)을 주면 그 프로필로 접속한다.
- **앱 안 마우스 사건**(09-21): `ui.move:x/y` · `ui.click:x/y` · `ui.rclick:x/y`(창 좌표 · 장치 픽셀 · 쉼표는 명령 구분자라 `/`) — OS 입력 주입이 아니라 앱이 스스로 `InputEvent`를 만들어 **실제 라우팅 경로(`route`)** 에 넣는다. 컨트롤을 직접 부르는 캡처 명령(`explorer.menu` …)은 라우팅 결함을 못 본다(탐색기 우클릭이 첫 커밋부터 닿지 않던 것을 이것으로 찾았다). Debug 첫 기동은 수 초 — `@after:`는 기동 뒤 기준이니 캡처 대기를 12초 이상.
- 환경 변수: `NSQL_NO_ACTIVATE=1`(★ 자체 시험 인스턴스는 **반드시** — 창을 활성화하지 않고 띄운다 · 09-21에 캡처용 창이 전경을 가져가 사용자가 치던 글자를 받았다) · `NSQL_HOME`(격리) · `NSQL_TRACE_FRAMES=1`(프레임 구간 · `[load] fill … ms`) · `NSQL_TRACE_MEM=1`.
- 벤치(nexa-ui): `cargo run --release -p nexa-ctl --example bench_editor <줄 수> <기능>`(`hl,ln,base,mm,occ,br` 또는 `all` · `BENCH_ASCII=1` · `BENCH_PREPARED=1`) · `--example bench_undo`.
- 스크립트(맥 · 09-24): **`scripts/mac-perf-all.sh`**(성능 전수 실행기 = `linux-perf-all.sh` 이식 · `mac-startup.sh`/`mac-probe.sh`/`mac-leak.sh` + `mac-common.sh`(`ps -o time/rss` · `ps -M` 스레드 · `vmmap --summary` footprint · `lsof` fd) · 실서버 CLI 타이밍은 `NSQL_PERF_ORACLE/MSSQL/PG=<프로필>`)
- 스크립트(3-OS · 09-24): **`scripts/func-block-comment.sh`**(기능 점검 자동화의 맥/Linux 첫 예 — 격리 홈 + 기동 명령 + 결과 파일 비교 · 키 주입 0 · 새 편집 명령을 넣으면 같은 틀로 한 경우를 더한다)
- 스크립트: **`scripts/win-perf-all.ps1`(성능 전수 실행기 · `-Stages`/`-Only`)** · **`scripts/win-inventory.ps1`(용량·정적/동적 라이브러리·구성 파일·설정 키)** · **`scripts/win-mem-reclaim.ps1`(71 §C-2 회수 시험 R1~R6)** · ★ **`scripts/win-func-check.ps1`(기능 점검 자동화 — 시나리오 표 = 격리 홈 + 기동 명령 + 전 창 캡처 + 생존/패닉 자동 판정 · 새 기능을 넣으면 시나리오 한 줄을 더한다 · 09-22 §62 · S01~S35 = 09-22 요청 전부 + soft undo·상한·북마크)** · `scripts/win-capture.ps1` · `scripts/win-burst-capture.ps1` · `scripts/win-big-probe.ps1` · `scripts/win-leak-cycle.ps1` · `scripts/win-startup-probe.ps1` · `scripts/win-latency-probe.ps1` · `scripts/win-badge-probe.ps1` · `scripts/mac-capture.sh` · `scripts/check-3os.sh` · **Linux(09-22)**: `scripts/linux-startup.sh` · `linux-probe.sh` · `linux-leak.sh` · `linux-perf-all.sh`(전 시나리오 + 릭 + CLI) · `linux-all-tests.sh`(두 저장소 게이트 + 3-OS + 실서버 통합 + CLI 기능 · 결과 `summary.txt`) · `install-instantclient-linux.sh`.
- 기동 구간: `NSQL_TRACE_FRAMES=1` → `[startup] settings · fonts · event_loop · … · app`(누적 ms) + `[frames] … first paint … at +N ms`(09-22 · 39 §2 S-14).
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

### 1-6. 09-21(Windows 89차)에 굳은 것 — 능력표 · PG 커서 · 입력

- **엔진·러너는 방언을 묻지 않는다 — `Caps`를 묻는다**(`nsql-core/caps.rs` · T-152): 새 분기가 필요하면 `Dialect` `match`를 쓰지 말고 능력표에 칸을 더한다(전수 테스트 `table_matches_the_former_dialect_branches`에 한 줄) · 세션이 붙으면 `Engine::set_caps(방언, session.caps())` · 남겨 둔 방언 분기 = 값 인용(`to_sql_literal`)과 카탈로그 질의뿐.
- **자동 커밋 모드의 `COMMIT`은 `caps.autocommit_needs_commit(tx_open, tx_user)`가 참일 때만**(T-155) · CLI 종료 = `Runner::commit_at_exit`.
- **PG refcursor는 드라이버가 푼다**(T-150): 결과의 글자 값이 열린 커서 이름이면 그 자리에서 `FETCH … IN` → 결과 집합 + `result_labels` · 자동 커밋이면 호출과 FETCH를 한 트랜잭션으로 · 러너·GUI는 모른다(Oracle 핸들 커서와 같은 "결과 + 라벨" 모양).
- **PG 호출의 OUT 값은 서명이 말한 자리의 바인드가 받는다**(T-151 · `Engine.call_captures` — 한 번 쓰이고 비워진다).
- **`ACCEPT`는 실행할 때마다 묻는다**: GUI = 실행 전 입력 창이 한 번에(`InputNeed{prompt,default,hide}`) → `apply_inputs`가 `Engine.accepted`에 표시 → 러너는 그 `ACCEPT`를 지나간다 · CLI = 그 자리에서 묻는다.
- **실서버 통합 테스트는 프로필로**: `NSQL_PG_PROFILE=<볼트 프로필 이름>`(비밀번호를 환경 변수·명령줄에 쓰지 않는다) · 서버에는 `pg_temp` 임시 객체만 만든다.
- **날짜·불리언은 진짜 타입으로 바인드한다**(Oracle · T-151): 값이 ISO 꼴(`YYYY-MM-DD[ HH:MM[:SS[.f]]]`)이면 `OracleType::Date`/`Timestamp`, 아니면 종전처럼 글자(서버의 암묵 변환). 변수 값의 표시도 ISO 꼴 — 결과 그리드의 DATE 열과 같다.
- **T-SQL 호출의 `OUTPUT`은 서명이 보충한다**(`Caps::call_signature = OutputMarks` · `Engine.call_outputs` — 한 번 쓰이고 비워진다) · **선언 없이 생긴 글자 변수는 값이 돌아오는 바인드에서만 4000으로 넓힌다**(`dialect.rs bind_type` — 선언한 길이는 사용자의 뜻).
- **미정의 바인드 경고는 읽기만 하는 보통 SQL에서만** — 블록·`EXEC`의 바인드는 받는 쪽일 수 있다.
- **시스템 변수는 `Engine.sysvars`에**(사용자 `DEFINE`과 섞지 않는다 · 이름 = `SYSTEM_VARS`) · `${이름:형식}`은 정의된 이름 + 아는 형식만 바꾼다(모르면 글자 그대로 · 묻지 않는다).
- **새 부하원은 스위치와 함께**: 결과에 영향을 주면 개별 설정(`vars.signature_lookup` · `pg.refcursor_expand`), 영향이 없으면 `perf::BOOST` 후보(39 §4-6). 실서버 시험은 프로필 환경 변수 셋(`NSQL_ORACLE_PROFILE` · `NSQL_MSSQL_PROFILE` · `NSQL_PG_PROFILE`) — Oracle은 객체를 만들지 않고(익명 블록 · SYS 패키지), SQL Server는 세션 임시 프로시저(`#…`)만.
- **큰 파일 탭은 상시 비용이 드는 기능을 단계별로 끈다 — 새 기능(특히 확장 효과·플러그인 훅)을 넣을 때 `editors.rs enforce_large`에 한 줄**(`feature_limited(level, 기준)` · 기준은 설정 · 편집 코어는 끄지 않는다 · 단계가 내려가면 `restore_large_features`로 되돌린다).
- **설정의 종속은 `DEPENDS`로**(부모가 조건을 못 채우면 설정 창에서 잠김 + 흐림) · 키를 바꾸면 옛 값은 `migrate_*`로 옮긴다(`grid.result_tabbar` → `grid.result_tabbar_single`).
- 자체 캡처: 설정 창은 `NSQL_STARTUP_CMD=edit.prefs:<검색어>`로 그 카드만 보이게 연다 · 격리 폴더의 `settings.conf`는 **`키=값`(공백 없음)** — 덧붙인 뒤 `nsql config get <키>`(같은 `NSQL_HOME`)로 읽히는지 확인한다.
- **결과 탭 제목 = 번호 규칙이 기본**(`결과N` · 가장 큰 번호 + 1 · `results.rs numbered_index`가 두 언어의 틀을 읽는다) · 사용자가 붙인 이름(`named`)과 커서 라벨은 덮지 않는다.
- **DBMS별 종속 설정은 "자동 탐지 / 직접 지정 / 읽기 전용 정보" 틀로**([64 §2](64-dbms-clients-and-driver-packaging.md)): 계산 값은 `nsql_settings::INFO_KEYS`(저장 안 함 · `set` 거부 · 설정 창에서 늘 잠김) · 탐지는 파일 시스템만 읽는다 · **직접 지정한 것은 그것만 쓴다**(조용히 다른 것으로 넘어가지 않는다 — 실패가 낫다) · 드라이버는 값만 돌려주고 문구는 호스트가 `Msg`로.
- **`${…:q}`** = 작은따옴표로 감싸고 안의 작은따옴표는 두 번 · SQL Server `N'…'` · MySQL 역슬래시 두 번(`engine.rs sql_text_literal`).
- **마우스 이동·키 처리 경로에서 `edit.text()`·`chars_vec()`를 부르지 않는다** — 줄·구간 조회는 `edit.buf()`의 줄 표로(`line_of` · `line_start` · `line_end`). 89차 추가: 드래그 처리가 사건마다 본문 전체를 문자열로 만들어 20 MB에서 12.7 ms/이동이었다 → 입력 경로를 고치면 `bench_editor`에 그 시나리오를 더한다.
- **드래그 선택은 포인터가 컨트롤 밖에 있어도 세로 위치를 따라간다**(옆 밖 = 그 줄의 처음/끝 · 한 글자씩 미는 가로 자동 스크롤은 같은 줄에 가려진 글이 있을 때만).
- **"잠금"은 사건 전달을 막는 것으로 끝나지 않는다** — 포커스 지정 · 키/IME/붙여넣기 경로(`focused_textbox`) · 값 수거 · 잠금 재계산 시점 넷을 다 본다. 글자 칸은 nexa-ctl 읽기 전용(`set_read_only` — 복사는 되고 편집은 편집 코어가 거부)으로, 나머지는 포커스를 주지 않는다(`prefs_win.rs apply_deps` · 89차 추가 7).
- **창은 만들 때 모니터 안으로**(`wingeom::keep_on_screen`) — 새 창을 추가하면 생성 직후 한 줄.

## 6. 이어 받을 일 — 맥 88차(09-21) 끝의 상태와 다음 순서

> 상태: nexa-sql `main` = 88차 마감 · CI `ci` ✅ · `integration` ✅ 9/9(Oracle·SQL Server·PostgreSQL 컨테이너) · nexa-ui `main` = 50차. 설계 SSOT = [63 변수 관리](63-variable-management.md)(단계표 §5 · 결정 §7) · 기록 = [journal 09-21](journal/2026-09-21.md).

| 순서 | 할 일 | 어디를 고치나 · 어떻게 확인하나 |
|---|---|---|
| 1 | 🚧 89차(win) 자동화 가능한 몫 ✅ · 키 입력 실기 = **TODO T-154 확인표 U-1~U-8**(사용자 · 방법은 요청 시 안내) — **Windows 실기**(위 "윈도우에서 처음 할 일" ②~⑤) | 결과는 journal에 실기표로 · 결함은 그 자리에서 |
| 2 | ✅ 89차(win) **T-150 PostgreSQL refcursor** — `SELECT f()`가 돌려준 커서 이름을 `FETCH ALL IN "이름"`으로 받아 결과 탭으로 | `nsql-driver-pg`: 단순 질의 경로에는 **열 타입이 없다**(`Column.type_name` 빈 글) → 커서 경로(`execute_cursor` — 드라이버가 `BEGIN`을 쥔다)나 `prepare`로 타입(`refcursor`)을 얻은 뒤, **같은 트랜잭션 안에서** FETCH → `ExecResult.result_sets`에 덧붙이고 `CLOSE` · 자동 커밋이면 드라이버/러너가 트랜잭션으로 감싼다(T-146 `tx_open`과 충돌하지 않게) · 검증 = `integration.rs`에 PG 함수(`RETURNS refcursor` · `RETURNS SETOF refcursor`) 테스트 → 작업 브랜치에서 `gh workflow run integration.yml --ref <브랜치>`(main을 건드리지 않고 실서버 확인 — 09-21에 쓴 방법) |
| 3 | ✅ 89차 후반(win · 실서버 Oracle·SQL Server) — MySQL만 드라이버 도입 때 · **T-151 서명 추론 넓히기 + 타입** | `nsql-catalog::routine_args`에 PG(`pg_proc`/`pg_get_function_arguments`) · SQL Server(`sys.parameters`) · MySQL(`information_schema.PARAMETERS`) · `VarType`에 DATE/TIMESTAMP 실제 바인드(Oracle 드라이버는 지금 스칼라 OUT을 전부 VARCHAR2(4000)로 묶는다) · BOOLEAN · `Direction::Out`을 실제로 만든다(지금은 전부 InOut) |
| 4 | ✅ 89차(win) **T-152 `Caps` 포트**([63 §3](63-variable-management.md)) | `nsql-script/dialect.rs`·`engine.rs`의 방언 `match`(`wrap_exec` · `prepare` · `exec_targets`의 `Oracle | Mssql` 판정 · 러너의 `absorb` 방언 게이트)를 드라이버 능력 질의로 — 포트(trait) + 레지스트리 + 설정([30](30-architecture-patterns.md)) · MySQL/MariaDB·NoSQL 어댑터가 능력표만 채우면 붙게 |
| 5 | 🚧 89차(win) `ACCEPT`·CLI `-v`·미정의 경고 · 후반 `COLUMN NEW_VALUE`·시스템 변수·`${v:형식}`·값 상한 ✅ · 남음 = hover 값·입력 창 타입 열·프로필 층 출처 — **T-153 변수 UX 잔여** | `ACCEPT [HIDE] [DEFAULT]` · `COLUMN … NEW_VALUE`(`nsql-script/command.rs`) · 입력 창에 타입 열·미리보기(`input_win.rs`) · **MacroStore 분리 + 시스템 변수**(`_USER` `_DATE` `_ROW_COUNT` `_SQLCODE` `_ELAPSED_MS`) · `${v:형식}` · CLI `-v name=value` · 편집기 hover에 변수 값 · 미정의 변수 경고(`Diagnostic::ImplicitVariable`은 만들어지기만 하고 **소비자가 없다**) · 값 크기 상한 `vars.max_value_kb` · 프로필 층의 출처(접속 프로필 필드) · 이름 없는 탭 보존(S-1 hot exit와 함께) |
| 6 | 그 밖에 열려 있던 것 | **T-145**(줄 변경 표시·괄호 표의 줄 단위 갱신) · T-147 잔여(`nexa-gfx::Surface` stride → 중간 버퍼 제거 · D-133 기본 전환) · T-148 잔여(SQLite 열기 실패 14가 "Connection lost"로 분류됨 · MySQL `DELIMITER`) · T-136 · T-103 위키 · T-105 · T-106 |

**89차(win)에 고친 흠**: ②(`VAR X NUMBER` 표기 보존) · ⑤(`about_to_wait`에서도 창 깃발) · ⑥(`NSQL_PG_URL`) — 아래 목록의 나머지(① ③ ④ ⑦)는 그대로.

**알아 둘 흠(09-21에 봤지만 고치지 않은 것)**: ① PG 수동 커밋에서 문장 하나가 실패하면 트랜잭션이 *aborted* 상태로 남아 Rollback 전까지 전부 실패한다(PG의 정상 동작 · psql `ON_ERROR_ROLLBACK`처럼 문장마다 SAVEPOINT를 두는 선택지를 [56](56-manual-commit-lock-prevention.md)에 검토로 남길 것) ② `VAR X NUMBER`는 이름을 대문자로 만든다(표기 보존 D-142는 `EXEC :x := …` 경로만) ③ `VAR X`(타입 없음)는 선언하지 않고 조회만 한다 ④ `vars.show`(SHOW VARIABLES)는 접속이 있어야 돈다(`gate_open`) — 변수 창은 접속 없이도 열린다 ⑤ `open_sessions`·`open_txlog` 같은 창 열기 깃발은 아직 `window_event` 끝에서만 본다(메뉴 클릭은 문제없고 `NSQL_STARTUP_CMD`로 열 때만 늦다 — `open_vars`처럼 `about_to_wait`에도 두면 된다) ⑥ `.devcontainer/devcontainer.json`은 `NSQL_POSTGRES_URL`인데 통합 테스트는 `NSQL_PG_URL`을 본다(Codespaces에서 PG 테스트가 조용히 건너뛰어진다 — 이름을 맞출 것) ⑦ `.devcontainer/devcontainer-lock.json`은 사용자 파일이라 추적하지 않았다.

**결정 대기**: D-133(맥 화면 내보내기 기본값 → `iosurface`?) · D-134(`input.hangul_compose` 기본 auto 확인) · D-87~90 · D-115~118 · Codespaces = 사용자 `gh auth refresh -h github.com -s codespace` 1회 뒤 가능.

