# BRANCHES — 브랜치 이력

> 시간 역순. 생성/병합/삭제/커밋수/요약.

| 브랜치 | 생성 | 병합 | 커밋 | 요약 |
|---|---|---|---|---|
| feat/win-95-run-cards-menus-skip-next | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 93차 후반~95차(win): ★ 실행 Facade(43 §11 · `Requery` · `fetch_card_policy`) · 그리드 선택 표시 5건(`grid.row_focus[_color]`) · 다중 열기 중앙 모달·기본 항목 · placeholder 제거 · 실행 카드 시계 ms·`run.toast_tick_ms` · 헤더 위선 1px · ★ 실행 카드 스택(앱 공유 · 최신 위 · 아래 고정 · 픽셀 휠 · `run.toast_follow/max` · 중지 버튼 색) · 툴팁 카드 왼쪽 · 편집 메뉴 팝업 층 · ★ 팝업 배타 게이트 + 바깥 우클릭 통과 · 좀비 훅 정리 스크립트 · ★ Ctrl+K,Ctrl+D Quick Skip Next(맥 ⌘K,⌘D) · ★ Edit 메뉴 6그룹(하위 메뉴) · journal §43~60 · STATUS · DEVLOG · TODO(U-28~36) · 30 §2 · 43 §11 · CLAUDE.md |
| docs/ci-result-93b | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 93차 후반 push CI 결과(nexa-ui 7ec2462 ✓ · nexa-sql c979106 ci 3-OS ✓ · integration ✓) · journal §42 |
| feat/win-93b-file-dialog-select-ime-ellipsis | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 93차 후반(win): IME 안내 = 입력란 아래·언어 바뀌면 즉시(`ui.ime_hint`) · 미리보기 탭 더블클릭 승격 · 파일 창 Ctrl 수식키/Ctrl+Space/Ctrl+A · 트리 키보드 스크롤 추종 · 다중 선택 강조 = 노드 경로 · 긴 경로 가운데 축약 + Alt 전체(`ui.menu_max_width`) · Ctrl+드래그 스윕 · 러버밴드(빈 공간·행) · 71 §C-2 회수 시험 R1~R6 · 다른 세션 94차(71 성능 점검 프로세스 · 26/45/65 · `win-inventory`/`win-perf-all`) 포함 — nexa-ui 56·57차와 짝 |
| docs/ci-result-93 | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 93차 push CI 결과(nexa-ui f23243d ✓ · nexa-sql f452617 ci 3-OS ✓ · integration ✓ · CI 1.98.1 린트 교훈) · journal §33 · STATUS · DEVLOG |
| fix/ci-linux-imestate-dead-code | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | CI ubuntu `check` — `imestate.rs` `describe`/`classify`가 Linux 빌드에서 미사용(입력 소스 조회 없음) → `cfg_attr(not(windows/macos/test), allow(dead_code))` · macOS·Windows는 3b656be에서 통과 |
| fix/ci-clippy-as-chunks | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | CI `check` 3-OS 실패 원인 = MSRV 1.89 뒤 새 clippy 린트(`chunks_exact(2)` 상수 → `as_chunks::<2>()` · nsql-search decode.rs · 로컬 1.97.1에는 없는 린트) |
| fix/clippy-all-targets-93 | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | check-3os 호스트 clippy(`--all-targets -D warnings`) — 테스트에서만 쓰는 `is_preview`/`has_project` = `#[cfg(test)]` · `reveal` = 배선 전 `allow(dead_code)`(T-165 P4) · 교차 타깃 3건은 이 PC 한계(CI) |
| feat/win-93-project-explorer-multi-open | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | ★ 93차(win): 프로젝트 탐색기 + 미리보기 탭(67 §4-1 · `project.rs`·`project_panel.rs`) · 파일 열기 다중 선택 + 순차 적재(`file.open_max`) · 다중 인스턴스 시작 모드(`instance.lock` · MSRV 1.89) · 동시 편집 탭 상단 줄 · 토스트 진행/IME 안내/프로필 해시 파일/가로 스크롤/클릭 안정(§14~§27) · 📐 66·67·67a·68·69(다른 세션)·70 · 규칙 점검(61 §2-4·§3-1) — nexa-ui 55차와 짝 |
| docs/kill-app-process-rule | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 규칙 — 개발 중 `target/` 아래 `nexa-sql.exe`는 누가 띄웠든 강제 종료하고 진행(61 §2-4 · CLAUDE.md §3 · journal §15) · Release 재기동 |
| docs/ci-result-92 | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 92차 push CI 결과(nexa-ui e5433cb 3-OS ✓ — Windows 포함 = 시험 경주 원인 확인 · nexa-sql bcb4123 ci 3-OS ✓) |
| fix/win-92-font-race-docs | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 92차(win) — 최신화·분석(mac 90차 + Linux 91차 · Windows 412 · 회귀 없음) · nexa-ui 53차 Windows CI 실패 원인 정정(nexa-font 시험 `set_text_gdi` 경주 → nexa-ui 54차 가드 + 걷기 캐시 3-OS 복귀) · Windows `[startup]` 첫 기록 · 문서(journal §14 · DEVLOG · STATUS · MILESTONES · TODO T-163 · 61 · CLAUDE.md) |
| docs/ci-result-90 | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | mac 90차 push(ae4cd81)의 CI 결과 기록(ci 3-OS · integration 초록 · check-3os 교차 타깃은 CC 없음) |
| docs/linux-password-window-check | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 확인 대상 기록 — Linux 비밀번호 창(T-163 ⑦ · 시작 인자 경로는 정상) |
| docs/ci-result-91 | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | 91차 push CI 결과(cf4585e · nexa-ui b3d8e8b 3-OS 초록 · Windows 원인 = `file_type()`) |
| feat/linux-modal-x11 | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | ★ Linux 모달 = 메인에 붙은 창(창 백엔드 기본 X11 `gfx.linux_backend` · `WM_TRANSIENT_FOR`+`_NET_WM_STATE_MODAL` · x11rb · `xprop` 확인) · 문서(journal §12 · 61 §3 · 26 · 10 원장 · TODO T-163) |
| fix/linux-app-icon-ci | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | Linux 앱 아이콘(`app_id` · `scripts/install-desktop-linux.sh`) · 91차 CI 결과(1f0d4d0 초록 · nexa-ui Windows ✗ 기록) |
| perf/linux-91-full-check | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | Linux 91차 — Oracle Instant Client 스크립트 · 성능 전수(26 §7-7) · Linux 기동 병목 둘(nexa-font 걷기 캐시 · 아이콘 `memo`/지연) · `[startup]` 계측 · `scripts/linux-*.sh` 6종 · 전체 테스트 자동화 · OS별 차이표 |
| fix/mac-90-login-explorer-modal-logs | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | mac 90차 — 🔧 시작 직후 Details 팝업(`ConnectPanel::new` 저장본) · 🔧 탐색기 키보드 서버 간 이동(`explorers::cross_pane` · PgUp/PgDn 페이지) · 🔧 맥 탭 메뉴 배율 · 🔧 Σ 초기 활성 · ★ 비밀번호 창 최상위 모달 · 🔧 `EXEC` 블록 현재 문 실행(원문 `span` · 왕복 테스트) · 🔧 `PRINT` 값·`VARIABLE` 이름+타입·`Begin.server` · 사용자 샘플 mssql.sql · 기록(journal 90차 · DEVLOG · STATUS · TODO · MILESTONES · 63 · CLAUDE.md) |
| docs/variables-samples-connect | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 사용자 수정 — 변수 샘플(`examples/variables/mssql.sql` · `oracle.sql`) 끝에 `CONNECT user@host`(비밀번호 없는 꼴 = 입력 창/프롬프트로 묻는다) + 바인드만 있는 `SELECT`(결과 머리줄 = 변수 이름) 예 |
| docs/ci-result-89-24 | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 추가 24 push의 CI 결과 기록(nexa-ui ci · nexa-sql ci·integration 초록) |
| feat/remaining-89-24 | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 89차 추가 24 — 성능 재점검(회귀 없음 · `lone_select_items` 한 번 훑기) · T-132 ③ `--password-stdin` · T-162 ② FROM 없는 `SELECT … INTO :V`(T-SQL) ③ 비밀 이름 메아리 가림 · T-158 드래그 자동 스크롤 틱 연결 · "미사용 확장" · 측정 스크립트 `NSQL_NO_ACTIVATE` |
| docs/ci-result-89-23 | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 추가 21~23 push의 CI 결과 기록(ci · integration 초록) |
| feat/bind-headers-save-on-close | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 89차 추가 21~23 — 바인드만 있는 SELECT 항목에 변수 표기를 열 이름으로(SQL Server·PG `lone_select_items`) · mssql 읽기 전용 숫자 바인드 타입 · CLI `opener` 비밀번호 없으면 서버에 가지 않음 · 저장하지 않은 탭 닫기 = 탭 옆 메뉴(저장하고 닫기 → 저장 창 · `editor.close_unsaved`) |
| fix/ci-win-test-and-pg-wait | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 89차 push 뒤 CI 둘: 워커 비밀번호 시험이 드라이버의 실패 시점에 기대던 것(Windows CI) · integration `POSTGRES_HOST` 누락(wait-for-db 변수 이름 수정으로 드러남) |
| fix/ci-clippy-first-mut | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 89차 push 뒤 CI clippy 오류 하나(`iter_mut().next()` → `first_mut()` · 로컬 clippy가 CI보다 낡아 못 본 검사) |
| feat/win-89-vars-explorer-secret | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 89차(win) 전체 — 종합 점검(회귀 없음) · T-150 PG refcursor · T-151 실서버(Oracle DATE 바인드 · SQL Server OUTPUT) · ★ T-152 `Caps` 포트 · T-153(`ACCEPT` · `COLUMN NEW_VALUE` · 시스템 변수 · `${v:형식}` · `${env:…}`) · 결과 탭 번호 이름·이름 바꾸기·실행 쿼리 복사 · 팝업 배치 규칙 · 큰 파일 기능 제한 단계 · 설정 ▸ DBMS(Oracle 클라이언트 · docs 64) · 🔧 탐색기 우클릭 결함 둘 · ★ 일회성 비밀번호 + 세션 자격 금고(`Secret` · nsql-vault `session`) · 같은 서버 `CONNECT` 규칙 · 입력 창 Enter 누수 · 자체 시험 `ui.*`/`NSQL_NO_ACTIVATE` |
| feat/variables-samples | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 89차 추가 6(win) — DBMS별 변수 사용법 샘플 `examples/variables/`(oracle·mssql·pg·sqlite + README · 실행 검증) · SQL Server `EXEC :V := 서버 식` 빈 값 결함 수정(`needs_declare_prepend`) · 63 §3-2 · TODO T-162 — **이 세션의 변경 조각만**(89차의 나머지 미커밋 작업은 작업 트리에 그대로) |
| docs/handoff-to-windows-88 | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 88차 인계(mac → win) — docs 61 §1-5·§3·§6 · TODO T-150~154 · CLAUDE.md 현 단계·§4 |
| docs/vars-wrapup-88 | 2026-09-21 | 2026-09-21 → main(삭제) | 1 | 88차 마감 — 최종 CI 결과 · MILESTONES M2 09-20~21 묶음 · 남긴 질문 셋 |
| feat/session-vars-2 | 2026-09-21 | 2026-09-21 → main(삭제) | 4 | 88차 후반(mac) — 변수 관리 V5 일부(`captures` · D-139 · SQL Server `@@ROWCOUNT`) · V7 `bench_vars` · V2 변수 창 · 실서버 통합 9/9 |
| feat/session-vars | 2026-09-21 | 2026-09-21 → main(삭제) | 9 | 87차 후반~88차(mac) — ★ T-146 수동 커밋 결함 · ★ 변수 관리(docs 63·63a · REF CURSOR 자동 표시 · 서명 추론 · 딸린/문장별 결과 탭 · 계층형 변수 표 · 입력 창 · 파일별 보존) — 중간 push(사용자 09-21) |
| fix/mac-hangul-app-compose | 2026-09-20 | 2026-09-21 → main(삭제) | 5 | 86~87차(mac) — ★ T-139 한글 앱 조합 · 분할기 방언 결함(`split_script_in`) · 62 맥 점검 · T-148 보완 다섯(`SHOW ERRORS` 등) · T-147 IOSurface 화면 내보내기(`present.rs` · `gfx.mac_present`) · 결정 번호 중복 정리 |
| docs/portable-rules | 2026-09-20 | 2026-09-20 → main(삭제) | 1 | 85차 — docs 61(핵심 설계·작업 규칙·OS별 차이 — 다른 PC에서 이어 가기) · CLAUDE.md 세션 공통 규칙 · 검증 스크립트 둘 |
| docs/status-refresh-84 | 2026-09-20 | 2026-09-20 → main(삭제) | 1 | 84차 마감 정리 — CLAUDE.md "현 단계" · MILESTONES M2 09-19~20 묶음 · CI 실패 표시(T-146) |
| feat/textbuf-undo-followups | 2026-09-20 | 2026-09-20 → main(삭제) | 1 | ★ 83~84차 — 되돌리기 재설계(연산 기록) · 대용량 파일(큰 파일 모드 · 열기 선택 · 탭 격리 적재) · **편집 버퍼 교체 T-142**(`TextBuf`) · 되돌리기 후속 넷(D-129~132 · `undofile.rs`) · 확장 뷰 탭 · 탐색기 계층형 새로 고침 · 로그인 표식 · docs 59 §5·§6 · 60 |
| feat/conn-veil | 2026-09-19 | 2026-09-19 → main(삭제) | 2 | 접속 창 접속 중 막(veil · 66차) · 문서 |
| feat/liveness-disconnect-model-perf | 2026-09-19 | 2026-09-19 → main(삭제) | 3 | 접속 생존 관리([53](53-connection-liveness.md)) · 세션 창 · 접속 UX·Disconnect 연결 모델([54](54-connection-model-and-disconnect.md)) · T-131/T-133 폼 · 툴바/그리드 드래그 고스트 · Σ 규정 · 편집기 입력 지연 1차([55](55-editor-input-latency.md)) · 임시 dev.start_demo(58~64차) |
| feat/explorer-tree-login-grid | 2026-09-18 | 2026-09-18 → main(삭제) | 4 | 탐색기 서버 루트 이어 붙이기(공용 스크롤) · 로그인 목록 열 조절/자동 맞춤 · 기본 크기 · 미니맵 기본 on · REF CURSOR 예제(57차) |
| feat/session-modes | 2026-09-18 | 2026-09-18 → main(삭제) | 5 | ★ DR-34 세션 컨텍스트([52](52-session-modes.md)) — 공유 N · 전용 CONNECT · 개별 모드 · 통제 단일화 · 서버별 탐색기 · 세션 상태 추적 · CONNECT 파서 · 로그인 창 확장 결함(56차) |
| main | 2026-09-12 | — | 1 | 착수(조사·골격·엔진) |
