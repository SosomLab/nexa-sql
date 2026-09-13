# DEVLOG — 날짜별 요약

> 시간 역순. 상세는 journal.

- **2026-09-13 (9차 · win)** — 정리 · CLAUDE.md 현 단계 갱신 · push(5차~9차 + 병합). 다음 = T-27.
- **2026-09-13 (병합 · win)** — origin/main(mac 3·4차) 병합 · 차수 재번호(mac 1~4 · win 5~8) · 병합본 71 테스트·실서버 ✓ · `NSQL_ORACLE_CLIENT_DIR` Windows 확인.
- **2026-09-13 (8차 · win)** — GUI 창 실기(자동 구동): 프로필 접속 → F5 → 그리드 ✓. `about_to_wait` 무한 재그리기 루프 수정(유휴 CPU 100%→13% · 입력 지연 해소). → [journal](journal/2026-09-13.md)
- **2026-09-13 (7차 · win)** — ★ 사용자 Oracle 19c 실서버: 프로필 접속 OK · `it-oracle.sql` 통과 · `:V := (SELECT…)` PLS-00103 수정. thin `oracledb` 스파이크 157ms 접속 ✓ but OUT 바인드 없음(D-22). → [journal](journal/2026-09-13.md)
- **2026-09-13 (6차 · win)** — 설계 [22](22-driver-extensions.md): 드라이버 확장(GitHub 최신 · stdio RPC 프로세스 · SxS `drivers/<id>/<ver>/` · 관리자 목록/삭제) · DBeaver식 접속 대화상자 · DR-23·24 · T-27~31. Oracle 호환 매트릭스 조사. → [journal](journal/2026-09-13.md)
- **2026-09-13 (5차 · win)** — ★ 연결 프로필 저장소 `nsql-vault`(비밀번호 봉투 · Windows DPAPI 기기 키 · 여러 인스턴스 단일 키) · `nsql conn` · `-c <이름>`/`CONNECT <이름>` · GUI Save · DR-22 · 71 테스트. nexa-ui SSH 별칭 clone. → [journal](journal/2026-09-13.md)
- **2026-09-13 (4차 · mac)** — Oracle Instant Client macOS 설치 실기(255MB · sqlplus 19.16 · 클라이언트 로드 ✓) · 설치 스크립트 · docs/20 §5. (3차 · mac: `NSQL_ORACLE_CLIENT_DIR` · DPI-1047 안내)
- **2026-09-13** — Codespaces 최소 사양(2코어·8GB · DB mem_limit · 빌드 병렬 1) · 정리·push. → [journal](journal/2026-09-13.md)
- **2026-09-12 (5차)** — ★ Oracle·MSSQL 실서버 통합 테스트 5/5 green · 실서버 결함 3건 수정(#temp 배치 라우팅 · TOP 재작성 · URL 디코드).
- **2026-09-12 (4차)** — Codespaces devcontainer · DBMS별 Docker compose · 통합 테스트 5건 · integration.yml(DR-21).
- **2026-09-12 (3차)** — 정리 · D-14~17 등재 · main push(nexa-sql 첫 push · nexa-ui 저장소 생성).
- **2026-09-12 (2차)** — DP→DR 승격 · 드라이버 3종(sqlite 실기 · oracle/mssql 컴파일) · `nsql run/shell/export` · 최소 GUI 창 · `nexa-font` · 설계 문서 12/14/15/17/18/19 · 분리기 결함 4건 수정. → [journal](journal/2026-09-12.md)
- **2026-09-12** — 프로젝트 착수. 조사 5건(03~07) · `../nexa-ui` 추출(189 테스트) · `nsql-core`+`nsql-script` 세션 변수 엔진(37 테스트) · `nsql plan` · 설계 문서(00·01·02·08·09·11·13) · 결정 DR-1~9/DP-1~10. → [journal](journal/2026-09-12.md)
