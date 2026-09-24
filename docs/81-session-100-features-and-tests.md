# 81. 100차(맥 · 2026-09-23~24) 기능 목록과 테스트 방법 — 사용자 확인용

> 이 세션에서 들어간 기능을 **써 보는 순서**로 정리했다. 자동 시험(단위·CI)은 통과했고, 키·마우스가 있어야 보이는 것은 아래 "확인 방법"이 사용자 몫이다(TODO의 U-100~U-134와 같다). 상세 설계 = [76](76-intellisense-and-outline.md) · [80](80-memory-monitor.md) · [79](79-metadata-ownership-and-refresh.md) · journal §130~184.

## 1. 코드 완성(인텔리센스)

| # | 기능 | 확인 방법 | 관련 설정 |
|---|---|---|---|
| 1 | **⌃Space**(맥) / Ctrl+Space 완성 호출 · 자동 팝업 | 접속 뒤 새 탭 `SELECT * FROM ` 뒤 ⌃Space | `intel.enabled` · `intel.auto_activation` · `intel.delay_ms` |
| 2 | 현재 스키마 객체 즉시 채움 · `스키마.` 시점 캐싱 · 권한 반영 사전(`ALL_`/`DBA_`/`V$` · `sys.` · `pg_catalog.`) · 미사용 즉시 회수 | `FROM ` → 테이블 목록 · `FROM BISCM.` → 그 스키마 · `FROM ALL_T` → `ALL_TABLES` | `intel.preload` |
| 3 | 팝업 자리 = 접두 시작 · 입력 중 고정 · 폭 상한 + 가로 스크롤(←/→ · Shift+휠) · 10행 고정 | `M4S_`를 이어 치며 팝업이 안 움직이는지 · 긴 이름에서 ←/→ | `intel.popup_rows` · `intel.popup_max_width` |
| 4 | 상세 카드(오른쪽 · 불투명) — 컬럼/테이블/함수/스키마/조각 · Description · PK/FK/UQ/인덱스 `#i/n` · 머문 대상만(빠른 스크롤 중 유지) | 항목에 머물기 → 카드 · ↓를 누르고 있기 → 카드가 멈춘 뒤 마지막으로 | `intel.detail_card` · `intel.detail_bg_alpha`(0) · `intel.detail_text_alpha`(0) · `intel.card_settle_ms`(150) |
| 5 | 클릭 = 선택(카드) · 더블클릭/Enter/Tab = 확정 · PgUp/PgDn/Home/End · 휠(팝업 위에서만 · 밖은 편집기) | 마우스로 훑기 · 팝업 밖에서 휠 → 팝업 닫히고 편집기 스크롤 | `intel.key_passthrough` |
| 6 | 정렬 = 정확 > 접두 > 단어 경계 > 포함 > 약어(`MI40`) · 같은 등급 = 최근 확정 → 출처 → 순번 → 짧은 이름 · **빈 접두 = 이름순** · 전체 일치 굵은 파랑 · 부분 일치 글자 강조 | `FROM MI40` → `M4S_I002040` 위 · 빈 접두 = 알파벳 순 | `intel.match` · `intel.recent_boost` |
| 7 | **페이지 로딩** — 200개 + "N개 더" → 끝까지 스크롤/End = 다음 200 · End = 전부 | `FROM ` 빈 접두에서 End 반복 · 뷰 `VM4S_I002040`이 뒤 페이지에 | `intel.max_items`(200) · `intel.max_total`(5000) |
| 8 | FROM 자리 = 테이블·뷰·MV·시노님 한 층 → 테이블 함수 → 함수·패키지 → 사전 뷰 · **프로시저·SQL Server 스칼라 함수 제외** · 종류 아이콘 | `FROM BISCM.` 뷰가 테이블 사이에 · M4PLAN `FROM dbo.`에 프로시저 없음 · `FROM DU` → `DUAL` | `intel.from_routines` · `intel.icons` |
| 9 | `FROM 스키마.테이블.` = 팝업 없음 · `FROM 스키마.패키지.` = 테이블 함수·함수 · `ㅁ.` = 없음 | `FROM ORDDATA.ORDDCM_DOCS_USR.` · `FROM BISCM.SP_MPS_PEGGING_PKG.`(프로시저만이라 빔) · `SELECT BISCM.SP_MPS_PEGGING_PKG.` = 멤버 2 | — |
| 10 | 접두 없는 컬럼 = 전 alias 컬럼 · 확정 = `A.컬럼` · Alt = 이름만 | `SELECT | FROM EMP A, DEPT B` → `A.EMPNO`·`B.DEPTNO` · Alt+Enter | `intel.qualify_columns` |
| 11 | **`*`/`A.*` → "모든 컬럼 (N)"** 하나만 · 카드에 목록 · 한 줄(기본)/여러 줄(줄 앞 쉼표) · 쉼표 뒤 Space/Tab | `SELECT A.*` 바로 뒤 ⌃Space → Enter | `intel.star_layout` · `intel.star_comma_space` |
| 12 | `INSERT INTO t (` 컬럼 목록 조각 · 시그니처 도움(상태줄) · 내장 함수·`DBMS_*`·방언 키워드(`TOP`·`ROWNUM`·`ILIKE`·`PRAGMA`) | `INSERT INTO M4S_I002040 (` · `NVL(` · 문장 시작에서 `TO` | `intel.insert_columns` · `intel.functions` · `intel.keywords` |
| 13 | **캐시 새로 고침** ⌘⇧R(현재 스키마) · Edit ▸ 이 서버/전 서버 · 탐색기 루트·스키마 우클릭 "메타 새로 고침" · 탐색기 "새로 고침"도 함께 낡음 표시 | 다른 도구로 테이블 만든 뒤 ⌘⇧R → 상태줄 "버킷 N · 객체 M" → `FROM ` 에 새 테이블 | — |

## 2. 편집기·창

| # | 기능 | 확인 방법 | 설정 |
|---|---|---|---|
| 14 | 블록 주석 ⌘⌥/(맥 · ⌘⇧?는 macOS 도움말이 선점) · Windows Ctrl+Shift+? · 줄 주석 ⌘/ | 선택 뒤 ⌘⌥/ · 다시 = 해제 | 단축키 설정 |
| 15 | 보조 창을 클릭하면 메인·다른 창도 함께 앞으로(고른 창 맨 위) | 메모리 창 클릭 → 메인이 뒤에 남지 않음 | `window.focus` |
| 16 | **메모리 모니터** — 상태줄 총량(클릭 효과) · 모델리스 창(Top 스위치 · 1초 갱신 · 데이터/시스템 표 · 스파크라인 · **힙 정리** 버튼) · 창 닫힘 = 비용 0 | 상태줄 `NN MB` 클릭 · View ▸ Memory Usage · Fetch all 중 "Result data" 증가 · 200행 재조회 뒤 감소 · 힙 정리 → 상태줄 "전 → 후" | `mem.statusbar` · `mem.refresh_ms` · `mem.always_on_top` |

## 3. 성능(맥)

| # | 내용 | 확인 방법 |
|---|---|---|
| 17 | 화면 내보내기 기본 **IOSurface**(D-133) — 프레임 Release 43 → 9.8 ms · Debug 48 → 13.9 ms | `NSQL_TRACE_FRAMES=1`로 띄우면 stderr `[frames] present backend = iosurface` · 화면이 비면 설정 `gfx.mac_present=softbuffer` |
| 18 | 개발 빌드 의존 최적화(`profile.dev.package."*"`) — Debug 프레임 94 → 48/14 ms | `cargo build` 첫 빌드 ≈ 7분 · 이후 증분 |
| 19 | 비활성 앱 캐럿 깜빡임 정지(유휴 CPU 19 % → 0) | 다른 앱을 앞에 두고 활성 상태 보기 |
| 20 | CLI 접속 2 s = Instant Client 적재 1.3 s + 로그온 0.4 s(앱 여지 없음) | `nsql run --timing` · `NSQL_TRACE_CONNECT=1` |

## 4. 자동 시험(이미 통과) · 도구
- 단위: nexa-sql `cargo test -p nexa-sql -- intel`(12) · nsql-script `intel`(11) · nsql-run `meta`(5) · nexa-ctl `ctxmenu`(26) · memstat(2) · 전체 `cargo test --workspace` 두 저장소 · `scripts/check-3os.sh --quick`.
- 실측 도구: `scripts/mac-perf-all.sh`(전수) · `scripts/mac-restart-debug.sh`(빌드·재기동) · 격리 계측 = `NSQL_HOME=<격리> NSQL_NO_ACTIVATE=1 NSQL_TRACE_FRAMES=1 NSQL_STARTUP_CMD='@after:1500:view.memory' target/debug/nexa-sql`.
- 4-DBMS 카탈로그 확인: `nsql cat -c <프로필> tables|views|funcs|packages` · `columns <표>`(76 §14).
