# BRANCHES — 브랜치 이력

> 시간 역순. 생성/병합/삭제/커밋수/요약.

| 브랜치 | 생성 | 병합 | 커밋 | 요약 |
|---|---|---|---|---|
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
