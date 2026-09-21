# BRANCHES — 브랜치 이력

> 시간 역순. 생성/병합/삭제/커밋수/요약.

| 브랜치 | 생성 | 병합 | 커밋 | 요약 |
|---|---|---|---|---|
| docs/ci-result-90 | 2026-09-22 | 2026-09-22 → main(삭제) | 1 | mac 90차 push(ae4cd81)의 CI 결과 기록(ci 3-OS · integration 초록 · check-3os 교차 타깃은 CC 없음) |
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
