# STATUS — 지금 상태 한 장

> 시간 역순. 상세는 [journal](journal/), 여기는 요약.

## 2026-09-13 (병합 · win) — origin/main(mac 3·4차) 병합 · win 기록 5~8차 재번호 · 병합본 검증 ✓

원격 mac 3커밋(`NSQL_ORACLE_CLIENT_DIR` · macOS Instant Client 설치) + 로컬 win 5커밋 병합(70709df). 병합본: 71 테스트 · clippy 0 · 19c 실접속 OK · `NSQL_ORACLE_CLIENT_DIR` Windows 동작 확인(잘못된 폴더 → DPI-1047). **미push 7커밋** — push는 사용자 요청 시. → [journal](journal/2026-09-13.md)

## 2026-09-13 (8차 · win) — ★ GUI 창 실기 통과(프로필 접속 → 쿼리 → 그리드) · 무한 재그리기 루프 수정

**요청**(사용자): *"프로그램 실행해줘"*. 창을 띄워 PowerShell로 직접 구동·캡처: 프로필 `biscm` 접속 ✓ · 편집기 입력 ✓ · F5 → 그리드 `BISCM · 22:13:33 · 442` ✓.
**결함 수정**: `about_to_wait`의 무조건 `request_redraw`가 **그리기 무한 루프**(유휴 CPU 100% · 키 입력 3초 지연 · 옛 결과 잔존)를 만들었다 → 타이머 만료 시에만 redraw. 유휴 CPU 13%(전체 창 래스터 65ms/프레임 · 더티 영역은 후속).
**⏳ 사용자 실기**: 한글 IME · 창 둘 프로필 공유. → [journal](journal/2026-09-13.md)

## 2026-09-13 (7차 · win) — ★ 사용자 Oracle 19c 실서버 실기 통과 · 결함 1건 수정 · thin 스파이크

**요청**(사용자): 19c 접속 정보 제공 · *"개발 가능한 부분은 먼저 정리해서 진행"* · Rust 클라이언트 질문 · 다른 DBMS 확인.
**결과**: 프로필 저장 → `conn test` OK 1.24s · `it-oracle.sql` 전부 통과. 결함 `EXEC :V := (SELECT …)`(PLS-00103) 수정(41acc2a). **thin `oracledb` 스파이크: Instant Client 없이 157ms 접속 ✓ — 그러나 OUT 바인드·REF CURSOR API 부재 → 주 드라이버 불가(D-22 교체 조건).** 다른 DBMS는 이 기기에 서버 없음 → MSSQL·PG 접속 정보 대기(PG는 드라이버 M4).
**다음**: T-27 RPC 프로토콜(로컬로 진행 가능) → T-28 → T-29(D-19) → T-31. → [journal](journal/2026-09-13.md)

## 2026-09-13 (6차 · win) — 설계: 드라이버 확장(GitHub 최신 다운로드 · SxS · 관리자) · DBeaver식 접속 대화상자 (DR-23·24)

**요청**(사용자): DBeaver 형태 접속 설정(Golden = 로그인 리스트) · 드라이버는 GitHub에서 최신 다운로드해 확장으로 · DBMS 버전별 하위 호환 확인 → SxS 다중 버전 · 목록·삭제.
**조사 결론**: Oracle은 실제로 필요(23ai ← 19c/21c/23ai · 구형 서버는 19c 클라이언트까지 · ODPI-C 프로세스당 클라이언트 1개) · ODBC 벤더 클라이언트도 필요 · MSSQL은 레거시 TLS 변종 1개 · PG/MySQL/SQLite 불요.
**설계**: 확장 = stdio JSON-RPC 프로세스(내장 순수 Rust 드라이버는 유지) · `drivers/<id>/<ver>/` SxS · 프로필 `driver=<id>@<ver>` · 최신 non-prerelease 기본 · sha256+Ed25519 필수 · 삭제는 참조 프로필/열린 세션 보호. → [22](22-driver-extensions.md) · DR-23·24 · T-27~T-31(순서 고정). **코드 변경 없음.** **다음**: T-27 프로토콜부터(D-15 답과 함께). → [journal](journal/2026-09-13.md)

## 2026-09-13 (5차 · win) — ★ 연결 프로필 저장소 `nsql-vault` · `nsql conn` · GUI Save (DR-22)

**요청**(사용자): 저장소 최신화·변경 분석 → *"사용자 폴더에 접속 정보를 암호화해서 저장 · 연결 시 재사용"* → *"몇 개의 Instance든 저장된 암호를 함께 사용"*. nexa-ui는 `git@kiros33.github.com:SosomLab/nexa-ui.git`로 clone(이 기기에 없었음 → 빌드 복구).
**산출**: `nsql-vault`(비밀번호만 ChaCha20-Poly1305 봉투 · 도메인 = 프로필 이름 · 기기 키 Windows DPAPI/그 외 0600 · 동시 첫 실행에도 단일 키) · `nsql conn list|add|show|rm|test|path` · `-c <이름>` · 스크립트 `CONNECT <이름>`(Runner Resolver) · GUI 이름 칸 + Save · 숨김 비밀번호 입력. **71 테스트 green**(+14) · clippy 0 · CLI 실기 ✓(저장 파일에 평문 없음).
**결정**: **DR-22**(파일 저장소 + 봉투 · D-2 닫힘) · D-18(mac Keychain·Linux Secret Service 후속). 원장 +3(clip 동일 판).
**⏳ 사용자 실기**: GUI Save → 이름으로 Connect · 창 둘 공유. **다음**: 사용자 후속 방향(DBeaver식 접속 대화상자 · GitHub 최신 드라이버 다운로드 · SxS 다중 버전·삭제) 설계 → [22](22-driver-extensions.md). → [journal](journal/2026-09-13.md)

## 2026-09-13 (4차 · mac) — Oracle Instant Client macOS 설치 실기 · 설치 스크립트 · 정리·push

**요청**(사용자): mac 설치 방법 → 전 패키지 한 폴더(CLI 포함) → 폴더 위치·이름 추천 → 정리·push.
**산출**: `scripts/install-instantclient-mac.sh`(아키텍처 감지 · Intel DMG/ARM64 ZIP · 6패키지 · 격리 해제 · `~/lib`·버전 링크 · `--rc`) · [20 §5](20-testing-codespaces.md) 전면 개편(폴더 추천 · URL 표 · PATH/TNS_ADMIN/NLS_LANG · 연결 3형식 · 한글) · 어댑터 `NSQL_ORACLE_CLIENT_DIR`·DPI-1047 안내.
**실측**: 이 Mac에 실제 설치 — 255MB · sqlplus 19.16 · `nsql run`이 ORA-12541까지 도달(클라이언트 로드 ✓). **폴더 규칙 확정**: `~/Oracle/instantclient_<major>_<minor>` + 버전 없는 링크 `~/Oracle/instantclient`.
**⏳ 남음**: 사내 Oracle 실접속(사용자 네트워크) · GUI 창 실기 · D-15~D-17 답.
→ [journal](journal/2026-09-13.md)

## 2026-09-13 (3차 · mac) — Instant Client macOS 안내(docs/20 §5) · `NSQL_ORACLE_CLIENT_DIR` · DPI-1047 힌트

**요청**(사용자): Instant Client mac 설치·설정 방법. Intel Mac → 19.16 · `~/lib` 심볼릭 링크 권장 · 어댑터에 lib dir 환경변수 + 로드 실패 안내 추가. → [journal](journal/2026-09-13.md)

## 2026-09-13 (2차 · mac) — 정리 · 진행사항 최신화 · push

**요청**(사용자): 정리 → commit → main 병합 → push. 브랜치는 main 하나. journal 09-13 파일 분리. 원격 `SosomLab/nexa-sql`·`nexa-ui` 모두 최신 · CI green. **다음**: D-15~D-17 답 대기 → 기본안 = nexa-edit E1(로프 버퍼) 착수([17](17-editor-incremental-plan.md)). → [journal](journal/2026-09-13.md)

## 2026-09-13 (1차 · mac) — Codespaces 최소 사양(2코어·8GB)

**요청**(사용자): *"codespaces 서버는 최소 사양으로 · 느려도 괜찮다"* → `hostRequirements` 2/8GB/32GB · DB 메모리 상한(oracle 2g · mssql 1.5g) · 빌드 병렬 1 · DB 대기 비차단. [20](20-testing-codespaces.md) 갱신. → [journal](journal/2026-09-13.md)

## 2026-09-12 (5차 · mac) — ★ Oracle · SQL Server 실서버 검증 통과 (integration 워크플로)

**결과**: `integration` run 34700220848 — 통합 테스트 **5/5 green**(Oracle: 세션 변수 왕복 · REF CURSOR PRINT · DBMS_OUTPUT · PL/SQL 블록 OUT · DML / MSSQL: SELECT INTO 재작성·OUT 회수 · `#temp`·`GO` 배치·한글 NVARCHAR / SQLite). `nsql run examples/it-oracle.sql`이 실서버에서 블록 EXEC→PRINT→REFCURSOR→DBMS_OUTPUT까지 그대로 동작.
**실서버가 잡아낸 결함 3건(수정)**: ① tiberius의 `query/execute`는 항상 `sp_executesql`이라 그 안의 `CREATE TABLE #t`가 소멸 → 파라미터 없는 DDL·세션 문장은 SQL 배치(`simple_query`)로 라우팅 ② `SELECT TOP 1 … INTO :V` 재작성이 `TOP`을 대입 뒤로 보냄 → 접두(TOP/DISTINCT) 보존 ③ CLI가 URL의 `%40`을 디코드하지 않아 sa 로그인 실패 → `scheme://` 형식만 퍼센트 디코드.
**닫힘**: T-4·T-5(실서버 검증) · D-14. **남은 미검증**: GUI 창 실기(사용자) · Instant Client 배포 방식(D-1).
→ [journal/2026-09-12](journal/2026-09-12.md) · [20](20-testing-codespaces.md)

## 2026-09-12 (4차 · mac) — ★ 실서버 테스트 구성: Codespaces devcontainer · DBMS별 Docker · Actions 통합 워크플로 (DR-21)

**요청**(사용자): *"codespaces를 사용한 테스트 구성"* · *"각 DBMS별 docker 방식으로"* → D-14 해소.
**산출**: `.devcontainer/`(compose: oracle · mssql · postgres · mysql 각 컨테이너 · Rust+Instant Client+한글 폰트 이미지 · nexa-ui 자동 clone) · `scripts/it.sh`·`wait-for-db.sh` · **통합 테스트 5건**(`nsql-drivers/tests/integration.rs` · 환경변수 게이트 · REFCURSOR·DBMS_OUTPUT·PL/SQL OUT·MSSQL SELECT INTO·GO 배치) · `examples/it-oracle.sql`·`it-mssql.sql` · `.github/workflows/integration.yml`(서비스 컨테이너 + Instant Client 설치 + `nsql run` 실기). `VarType::Auto`의 T-SQL 선언을 `NVARCHAR(4000)`로(SQL_VARIANT 회수 불안정).
**실측(로컬)**: 워크스페이스 56 테스트 green · 통합 테스트는 환경변수 없어 [skip]. **원격 검증**: push 후 `integration` 워크플로 결과를 journal에 기록(첫 실행은 이미지 pull·Oracle 기동으로 10분 내외 예상).
→ [journal/2026-09-12](journal/2026-09-12.md) · [20](20-testing-codespaces.md)

## 2026-09-12 (3차 · mac) — 정리 · 진행사항 최신화 · main push

**요청**(사용자): *"내용 정리 후 진행사항 최신화 수행하고 commit 및 main 병합한 뒤 push"*. 브랜치는 main 하나(병합 대상 없음).
**정리**: 열린 질문 4건을 D-14~D-17로 등재([10 §3](10-decision-record.md)) — 실서버 검증 방식 · 다음 우선순위 · 실기 OS · 캐시 위치. 사용자 답이 오면 DR로 승격하고 그 순서로 착수.
**push**: `SosomLab/nexa-sql`(첫 push) · `SosomLab/nexa-ui`(신규 생성) — CI 첫 실행 빨강 2건 수정 후 **양쪽 3-OS green**.
**다음 후보(답 전 기본안)**: D-15 권장 순서 = nexa-edit E1(로프 버퍼·Transaction·History) → 세션 hot exit → 비교 뷰. D-14 권장 = Docker 로컬 서버.
→ [journal/2026-09-12](journal/2026-09-12.md)

## 2026-09-12 (2차 · mac) — ★ M1+M2 병행 착수: 드라이버 3종 · CLI run/shell/export · 최소 GUI · 폰트 · 설계 문서 6건

**요청**(사용자): 서명은 별도 요청 시 · **나머지 DP 전부 확정 → 한꺼번에 개발** · GUI 최소 기능을 CLI와 병행 · 한글·고정폭 폰트 · 영역별 폰트 · 구문강조/포매터 등 패키지식 모듈 · Sublime 구성 학습 · 외부 파일 변경 처리 조사 · 편집기 점증 계획 · 미저장 버퍼 복원(hot exit)·프로젝트 · 성능 원칙(요청 병합·인덱스·미니맵) · 비교/git/오브젝트 시점 캐시.
**결정**: DP-1~9 → **DR-10~18 승격** · DR-19 한글·고정폭 1급 · DR-20 서명 보류([10](10-decision-record.md)).
**코드**(전부 fmt·clippy `-D warnings`·테스트 green — 워크스페이스 51 테스트):
- `nsql-io` export 작성기(grid/csv/tsv/json/jsonl/insert · CJK 폭 정렬) + CSV 파서.
- `nsql-driver-sqlite`(rusqlite bundled) · ★ `nsql-driver-oracle`(kubo · 이름 바인드 IN/InOut · REF CURSOR 핸들 · DBMS_OUTPUT 폴링 · 컴파일만 — 실서버 미검증) · ★ `nsql-driver-mssql`(tiberius · `DECLARE @v 타입 = @Pn` + 트레일러 SELECT로 OUT 회수 · 렌더 테스트 2 · 실서버 미검증).
- `nsql-run`(엔진 ↔ 세션 오케스트레이션 · OUT 흡수 규약 · 이벤트) · `nsql-drivers` 레지스트리(feature: sqlite/oracle/mssql) · `Session::set_option`(SERVEROUTPUT·fetch_size).
- `nsql` CLI: `plan` · **`run`** · **`shell`** · **`export`**. ★ 실기: `examples/sqlite-session-vars.sql`로 세션 변수 왕복(`EXEC :V_MAX := (SELECT MAX…)` → `PRINT` → 후속 조회 바인드) · export json/insert/csv 파일 ✓.
- ★ **GUI 최소 창**(`nexa-sql`): 접속 필드+Connect · 고정폭 SQL 편집기(TextBox 다중행 · IME) · 자체 가상화 그리드 · 상태줄 · 워커 스레드 · `⌘/Ctrl+Enter`/`F5`. `--smoke`로 폰트 체인·드라이버 로드 확인(UI = Apple SD Gothic Neo · 고정폭 = **D2Coding** → 한글 폴백). 창 실기는 사용자 몫(⏳).
- `../nexa-ui`: `nexa-font` 신설 · `Rect::intersection`(커밋 `1f75cc8`).
**분리기 결함 수정**: 한 줄 다중 문장 유실 · `;` 뒤 명령 · 블록 EXEC 오프셋 · `?` 방언 EXEC 래핑.
**문서**: [12 Sublime 프로필](12-user-sublime-profile.md) · [14 폰트·모듈](14-fonts-and-feature-modules.md) · [15 외부 변경](15-external-file-changes.md) · [17 편집기 점증 계획](17-editor-incremental-plan.md) · [18 세션·프로젝트](18-session-and-projects.md) · [19 비교·git·오브젝트 캐시](19-compare-git-and-object-history.md).
**⏳ 사용자 실기**: `cargo run -p nexa-sql` 창 열기 · 한글 입력 · `sqlite::memory:` 접속 후 실행 · Oracle/MSSQL 실서버 접속(`nsql run -c oracle://…`).
→ [journal/2026-09-12](journal/2026-09-12.md)

## 2026-09-12 (1차 · mac) — ★ 프로젝트 착수: 조사 5건 · nexa-ui 추출 · 세션 변수 엔진 · 기반 보고서

**요청**(사용자, 순차 7건): DBeaver급 크로스플랫폼 SQL 클라이언트 · Rust 평가 · 영리만 유료 라이선스 · 언어/플랫폼/아키텍처/범위 결정 · 경쟁 앱 조사(Golden·PLEdit·Orange·SSMS 등) · **sqlplus 변수/CONNECT를 SQL Server에서도** · 컨트롤 라이브러리 선분리 · 편집기 = Sublime 차용 + 패키지 확장 · 편집기 기능 목록(VS Code·ST·IntelliJ+α) · CLI(export/import/bulk) 포함 · 4계층 독립 아키텍처 · 전체 보고서.
**조사**: 에이전트 5건(크로스플랫폼 · Oracle · MSSQL · Rust 생태계 · 편집기) → [03](03-competitive-landscape.md)~[07](07-editor-research.md). 핵심 발견: Oracle 공식 순수 Rust thin 드라이버 베타(2026-08) · MS `mssql-tds` 0.1.0(09-10) · Orange 아직 판매 중 · ADS 은퇴 · T-SQL 변수는 배치에서 죽으므로 `sp_executesql` OUTPUT이 1차 우회.
**산출**: ① `../nexa-ui` 저장소(clip gfx/ctl/conf 이관 · **189 테스트** · 커밋 `d182eee`) ② `nsql-core` + `nsql-script`(분리·명령·바인드·방언 재작성·엔진·CONNECT · **37 테스트** · clippy 0) ③ `nsql plan` dry-run — 사용자 Golden 예시가 Oracle/MSSQL 양쪽으로 계획됨 ④ 문서 13건 + [00 보고서](00-foundation-report.md) ⑤ PolyForm NC 라이선스(양 저장소).
**결정**: DR-1~9 확정(사용자 발언 근거) · **DP-1~10 확인 대기**([10 §2](10-decision-record.md)) — 특히 DP-9(CLI 먼저 관통) · DP-6(셰이핑 크레이트) · DP-10(서명·법인).
**다음**: M1 — `nsql-net` · Oracle/MSSQL 드라이버 · `nsql run/shell` 실접속. → [journal/2026-09-12](journal/2026-09-12.md)
