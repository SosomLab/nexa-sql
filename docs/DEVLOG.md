# DEVLOG — 날짜별 요약

> 시간 역순. 상세는 journal.

- **2026-09-14 (4차 · win)** — D-32~39 확정 → **DR-26** · **`SosomLab/nexa-license` 공개 저장소 생성**(형제 `../nexa-license`) · 골격(format·base32·Product · 12 테스트 · 3-OS CI · **검증 전용 — 발급 코드는 비공개 저장소로**) = T-45 착수 · push. [25 §11](25-license-tiers-and-server.md) 장치 vs 사용자 인증 차이표 · 백업/자동 복원(T-46). 남은 결정 D-23·24·40. → [journal](journal/2026-09-14.md)
- **2026-09-14 (3차 · win)** — 라이선스 모듈 범용성 점검(beep·clip·dir2): 골격 범용 · 전용 가정 7곳 수정안(`Product` 기술자 · 공통 기기 코드 · 경로 주입+자체 파서 · **서명 포트 ed25519/p256 = D-40**(dir2 외부 crate 0) · 열거형 오류 · 다제품 서버). → [25 §10](25-license-tiers-and-server.md) · [journal](journal/2026-09-14.md)
- **2026-09-14 (2차 · win)** — 설계 [25](25-license-tiers-and-server.md): 라이선스 4단(Device 1대 · User 5대 · Team 1~5 파일 묶음 · Org 6+ 서버) 경쟁 8제품 교차 · 사내 인증 서버 `nexa-licensed`(3단 Ed25519 체인 · 리스 TTL · named/device/concurrent) · **DR-25 저장소 분리**(`nexa-license` 공유 라이브러리 + 서버 비공개 저장소) · D-32~39 · T-40~45. 코드 0. → [journal](journal/2026-09-14.md)
- **2026-09-14 (1차 · win)** — ★ 정품 인증 설계 [23](23-license-activation.md)(요청 코드 → 오프라인 Ed25519 라이선스 파일 → `check(Feature)` 게이트 · D-23~31 · T-32~36) · **i18n(기본 영어)·테마(System 기본) 구현**(`nsql-i18n`·`nsql-settings` 신규 · `nsql config` · GUI 단축키 ⇧T/⇧L · 84 테스트) · VS Code 설정 방식·UI 분석 [24](24-settings-and-vscode-analysis.md) · T-39 설정 화면 등재. → [journal](journal/2026-09-14.md)
- **2026-09-13 (9차 · win)** — 정리 · CLAUDE.md 현 단계 갱신 · push(5차~9차 + 병합) · CI green(ci·integration). 다음 = T-27.
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
