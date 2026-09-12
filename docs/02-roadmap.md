# 02 · 로드맵 — 수직 슬라이스 순서

> 원칙(계열): 얇은 끝단을 먼저 관통하고 넓힌다. 각 M은 "사용자가 실기로 확인할 수 있는 것"으로 끝난다.

| M | 목표 | 산출 | 상태 |
|---|---|---|---|
| **M0** | 조사·결정·골격 | 조사 5건(03~07) · 설계(01·08·09·11) · `nexa-ui` 추출 · `nsql-script` 엔진 + `nsql plan` | ✅ 09-12 |
| **M1** | **CLI 관통** — Oracle·MSSQL 실접속 | 드라이버 3종(sqlite ✅ · oracle/mssql 구현 · ⏳ 실서버) · `nsql run/shell/export` ✅ · 세션 변수 실기(SQLite ✅ · Oracle/MSSQL ⏳) | 🚧 09-12 |
| **M2** | **GUI 관통** — 창 하나에 편집기·그리드 | 최소 창 ✅(TextBox·자체 그리드·워커) · `nexa-edit` E1~E5([17](17-editor-incremental-plan.md)) · 세션 hot exit([18](18-session-and-projects.md)) · `nexa-grid` · dock/tab · 연결 프로필·키체인 | 🚧 09-12 |
| **M3** | 편집기 P0 완성 + 패키지 1차 | syntect `.sublime-syntax` · 컬러스킴 · 스니펫·완성 · Command Palette · `Default` 패키지 · 오브젝트 브라우저(`Catalog`) | ☐ |
| **M4** | 데이터 작업 | `nsql-io` import/bulk 6방언 · 데이터 편집기(변경 SQL 미리보기 → 커밋 · TablePlus식) · PG/MySQL/SQLite 드라이버 · ODBC 폴백(Tibero) | ☐ |
| **M5** | Oracle·MSSQL 깊이 | PL/SQL 컴파일·`SHOW ERRORS`·DBMS_OUTPUT · 실행계획 · 세션 모니터 · T-SQL 실행계획·XEvent 최소 | ☐ |
| **M6** | 확장·배포 | WASM 플러그인 API(TextCommand/EventListener) · 드라이버 RPC 플러그인 · 3-OS 릴리스 파이프라인(clip 계승) · 서명(D-6) · 라이선스 키(13) | ☐ |
