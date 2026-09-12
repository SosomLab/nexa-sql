# STATUS — 지금 상태 한 장

> 시간 역순. 상세는 [journal](journal/), 여기는 요약.

## 2026-09-12 (3차 · mac) — 정리 · 진행사항 최신화 · main push

**요청**(사용자): *"내용 정리 후 진행사항 최신화 수행하고 commit 및 main 병합한 뒤 push"*. 브랜치는 main 하나(병합 대상 없음).
**정리**: 열린 질문 4건을 D-14~D-17로 등재([10 §3](10-decision-record.md)) — 실서버 검증 방식 · 다음 우선순위 · 실기 OS · 캐시 위치. 사용자 답이 오면 DR로 승격하고 그 순서로 착수.
**push**: `SosomLab/nexa-sql`(기존 공개 저장소 · 첫 push) · `SosomLab/nexa-ui`(신규 생성 후 push). 결과는 journal.
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
