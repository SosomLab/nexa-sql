# 43 · 페치 모델(상한·더 가져오기·전체·건수) + 결과 탭(Ctrl+Enter / Ctrl+\) — 조사·설계 (사용자 요청 09-16)

> **요구**(사용자 09-16 원문 요약): ① 다른 CLI들의 **조회 상한** 조사 → 설정·적용 방식 정리. ② 상한은 **CLI 대화형(interpreter)에서만** · 파일/파이프 같은 **직접 출력은 제한 없음**. ③ GUI 기본 **200행**(DBeaver식) + **스크롤로 추가 페치** · CLI도 **추가 페치를 요청**할 수 있게. ④ DBeaver 결과 창 하단(행 추가/삭제 · 한 번에 조회할 행수 · 전체 페치 · Count) 조사 → **구동 속도·과도한 메모리(쓴 뒤 해제되면 허용)** 를 지키며 적용. ⑤ 기본 조회 건수 = **전역 설정** → 각 결과 창에서 **개별 수정 · 탭 생명주기 동안 유지**. ⑥ **Ctrl+Enter** = 현재 결과 그리드에 조회 · **Ctrl+\\** = **결과 탭을 추가**하며 조회(조회 개수 등 독립) — 성능 제약·메모리 최소화로.
> **위치**: [26 §6](26-performance-architecture.md) D-42·D-43의 답 · T-48의 설계 본문 · [39 §3](39-resource-governance.md) 부하원(서버 커서 · 결과 메모리) 등재 · [34](34-transaction-ux.md) 트랜잭션과 커서 수명.

## 0. 요약(결정)

| # | 결정 | 근거 |
|---|---|---|
| **D-68** | 상한 적용 범위 = **대화형 표시 경로만**: GUI 결과 그리드 · `nsql shell`(터미널). **`nsql run`·`export`·파이프/`-o` 파일 = 무제한**(`--max-rows`를 주면 그 값) | 사용자 ② · psql/mysql도 표시 경로만 제한(§1) |
| **D-69** | 기본 상한 **200행**(DBeaver `resultset.fetch.size` 동일) — D-42의 1,000 대신. 전역 `grid.max_rows`(GUI) · `cli.max_rows`(shell) · 결과 탭마다 **개별 값**(탭 생명주기 · 저장 안 함) | 사용자 ③⑤ · 200은 첫 화면 한 장 + 왕복 1회 |
| **D-70** | 추가 페치 = **서버 커서 유지 + OFFSET 재질의 폴백**(D-43 확정). 세션당 열린 커서 **1개** — 새 실행·커밋·롤백·다른 탭 실행이 앞 커서를 닫고, 닫힌 탭의 "더"는 OFFSET 재질의(정렬 없는 질의는 경고) | 왕복·서버 부담 최소 · 트랜잭션 규칙과 충돌 0([34](34-transaction-ux.md)) |
| **D-71** | 결과 탭 = **편집기 탭 ↔ 결과 패널(탭 여러 개)**. Ctrl+Enter = 활성 결과 탭 **교체** · Ctrl+\\ = **새 결과 탭**(자기 상한·커서·상태). 탭마다 rows는 자기 것 · 그리기는 활성 탭만 · 닫으면 즉시 해제 | 사용자 ⑥ · 메모리 = 탭 수 × 행 수 · `grid_stash`(09-16 3차) 확장 |
| **D-72** | 메모리 예산 = **행 바이트 상한** `grid.memory_budget_mb`(기본 1024 · **탭마다 독립** — 09-17 사용자 "탭은 서로 영향 없음" · 합계 검사 폐기). "전체 페치"·자동 페치가 예산에 닿으면 멈추고 상태줄 안내(계속하려면 예산 상향 · Export 권유). 결과·탭·커서는 **쓴 뒤 해제**: 탭 닫기 · 재실행 · 접속 해제 = `Vec` drop + 커서 close | 사용자 ④ "쓴 뒤 해제되면 허용" · [39 S-1·S-2](39-resource-governance.md) |

## 1. 조사 — 다른 CLI의 조회 상한

| CLI | 상한/페치 설정 | 적용 방식 | 우리에게 |
|---|---|---|---|
| **psql** | `\set FETCH_COUNT 100` | 기본 **무제한**(전체를 메모리에 받은 뒤 표 정렬). FETCH_COUNT를 주면 **서버 커서(DECLARE … CURSOR)로 N행씩** 받아 **스트리밍 출력**(메모리 상한 · 컬럼 폭은 첫 묶음 기준). 표시는 `PAGER`(less)가 페이지 | 커서 배치 페치의 원형 · "표시 = 페이저" |
| **SQL\*Plus / SQLcl** | `SET ARRAYSIZE 15`(1~5000) · `SET PAGESIZE` · `SET PAUSE ON` | 행 상한 없음 · ARRAYSIZE = **왕복당 행수**(성능) · PAUSE = 화면마다 멈춤(대화형 페이징). 스풀(`SPOOL`)은 무제한 | "왕복 배치"와 "행 상한"은 다른 축 — 우리도 둘을 분리(`fetch_size` vs `max_rows`) |
| **mysql** | `--safe-updates`(`--i-am-a-dummy`) → `select_limit=1000` · `--quick` · `--pager` | safe-updates면 **LIMIT 없는 SELECT에 LIMIT 1000을 붙여** 실행(서버가 자름). `--quick` = 버퍼 없이 행마다 출력(메모리 0 · 대신 서버 잠금 오래). 기본은 무제한 | "질의 재작성으로 상한"의 예 — 우리는 재작성 대신 커서 조기 중단(방언 무관) |
| **pgcli / mycli / litecli** | `row_limit = 1000`(설정 파일) | LIMIT 없는 질의가 row_limit을 넘으면 **"N행 넘음 · 계속?" 프롬프트** → 예면 전부 · 아니면 중단. `\pager` | 대화형 확인의 예 — 우리는 프롬프트 대신 **잘라 보여주고 `more`** |
| **sqlcmd / sqlite3** | 없음 | 무제한 · 출력 파일(`-o`)로 | 직접 출력 = 무제한(D-68과 같음) |
| **DuckDB CLI** | `.maxrows 40`(duckbox) | **표시** 상한 — 위·아래 N행 + `…` 생략 표시(실행은 전부) | 표시 상한 ≠ 페치 상한 · 우리는 페치 자체를 줄여 서버·메모리 모두 절약 |
| **ClickHouse client** | `output_format_pretty_max_rows = 10000` | Pretty 형식만 상한("Showed first 10000.") · 파일/TSV 무제한 | 형식(대화형 표)에만 상한 — D-68과 같은 원칙 |
| **BigQuery `bq`** | `--max_rows 100` | 대화형 **페이지** 100행 · `--start_row`로 다음 페이지(서버 페이지 토큰) | "다음 페이지 = 서버 토큰" — Oracle/PG 커서에 해당 |
| **DBeaver**(GUI) | `resultset.fetch.size = 200` · 자동 다음 세그먼트 · Fetch all · Count | §2 | 사용자가 고른 기준 |

**공통 결론**: ① 상한은 **대화형 표시**에만 두고 파일·파이프는 그대로(psql·mysql·ClickHouse·bq 모두). ② 상한 넘김은 **"처음 N행 · 더 있음"** 을 알리고 **다음 묶음을 요청하는 명령**을 둔다(bq `--start_row` · psql 커서). ③ 왕복당 행수(`fetch_size`/ARRAYSIZE)와 표시 상한(`max_rows`)은 다른 설정이다.

## 2. 조사 — DBeaver 결과 패널(사용자 1·2번 이미지)

| 요소 | DBeaver 동작 | 설정 키(DBeaver) |
|---|---|---|
| **결과 행수 상자**(하단 `200`) | 이 결과 탭의 **세그먼트 크기**. 전역 기본 = Preferences ▸ ResultSets ▸ "Result set fetch size"(200). 탭에서 바꾸면 그 탭만·재실행에 유지 | `resultset.fetch.size` |
| **Fetch next page**(`200+`) | 다음 세그먼트를 **이어 붙임**(rows 누적). 구현 = 대개 **재질의 + offset**(방언이 LIMIT/OFFSET을 지원하면 SQL에 붙임 · 아니면 서버에서 앞 행을 건너뜀) · JDBC `setMaxRows` | `resultset.maxrows.sql`("Use SQL to limit fetch size") |
| **Fetch all** | 상한 없이 끝까지(경고 없음 · Java heap 이슈의 원인 · [26 §1](26-performance-architecture.md)) | — |
| **자동 다음 세그먼트** | 그리드 끝까지 스크롤하면 다음 세그먼트 자동 요청 | `resultset.fetch.auto`("Auto-fetch next segment") |
| **Count**(`…` 메뉴 · Calculate total row count) | `SELECT COUNT(*) FROM (질의) x`를 **별도로** 실행해 상태줄에 "총 N행" | — |
| **Refresh** | 같은 질의 재실행(첫 세그먼트) · 필터/정렬 유지 | — |
| **Ctrl+Enter** | 문장 실행 → **현재 결과 탭** 교체 | `org.jkiss.dbeaver.core.sql.editor.execute` |
| **Ctrl+\\** | 문장 실행 → **새 결과 탭**(탭마다 세그먼트 크기·필터·정렬·고정(pin) 독립) | `…execute.new` |
| 행 추가/삭제/복제 · Save/Cancel | **데이터 편집기**(그리드 편집 → 키로 UPDATE/INSERT/DELETE 생성 · [41](41-sql-copy-key-rules.md) 키 규칙 그대로) | — |
| 상태 문구 | `200 row(s) fetched - 0.052s (0.047s fetch), on 2026-09-16 at 15:20:52` | — |

DBeaver의 약점(우리가 피할 것): ① Fetch all에 예산이 없어 heap 폭주 ② "다음 세그먼트"가 재질의라 **정렬 없는 질의는 페이지가 겹치거나 빠질 수 있음**(경고 없음) ③ 탭을 닫아도 JDBC 결과·캐시가 늦게 풀림.

## 3. 설계 — 페치 모델

### 3-1. 한 결과의 수명(서버 커서 기준)

```
Execute(문장, max_rows = N)
  → 드라이버: 서버 커서 열고 N+1행까지만 받음(조기 중단 · Oracle/SQLite ✅ · PG/MSSQL = §3-3) → ResultSet{rows: N, more: true} + CursorHandle
  → 호스트: 그리드/표에 N행 · 상태줄 "처음 N행 · 더 있음"
FetchNext(handle, N)      → 커서에서 다음 N행(이어 붙임)  · 커서가 닫혔으면 OFFSET 재질의(§3-4)
FetchAll(offset=든 행 수)  → **나머지를 이어 받아 붙인다**(09-17 · 위치·정렬·텍스트 스크롤 유지): 같은 문장의 커서가 그 위치면 커서 fetch_all ·
                            커서가 없으면 원문을 커서로 재실행해 앞 offset행은 버리고(메모리 0) 이어 받기 · 커서 없는 드라이버 = OFFSET 재질의 ·
                            배치(`db.fetch_all_size`)마다 진행률 · 예산(D-72 · 남은 예산)·취소에 멈춤 · 자동 페치가 나가 있으면 큐 · 더 없으면 버튼 흐림
Count(문장)               → 메타 세션에서 SELECT COUNT(*) FROM (문장) x  · 커서와 무관
Close(handle)             → 새 실행 · 커밋/롤백 · 탭 닫기 · 접속 해제 · 세션당 1개(새로 열면 앞 것 닫힘)
```

- **상한 두 축**: `max_rows`(표시 세그먼트 · D-69) vs `fetch_size`(왕복당 행수 · 세션 옵션 `fetch_size` · Oracle ARRAYSIZE 격 · 기본 200 · 전체 페치 때 `db.fetch_all_size` 기본 **5000**으로 상향 — 실측 09-17 WAN 155k행: 200 = 34.3s · 5000 = 4.6s).
- **Timeline**: FetchNext/FetchAll은 `Stage::Navigate` 스팬으로 덧붙인다([26 §2](26-performance-architecture.md) 표 "Navigate" ☐ → 여기서 채움).

### 3-2. 포트(nsql-core) — 확장점 세 조각([30 §1-2](30-architecture-patterns.md))

```rust
/// 열린 서버 커서(세션당 1개 · 새 실행이 닫는다).
pub struct CursorHandle(pub u32);
pub trait Session {
    // 기존 execute는 그대로(뒤에 남는 커서가 있으면 ExecResult::pending에 실어 준다)
    fn fetch_next(&mut self, h: CursorHandle, max: usize) -> Result<(ResultSet, bool /*more*/), DbError>;
    fn close_cursor(&mut self, h: CursorHandle) -> Result<(), DbError> { Ok(()) }
    fn cursor_supported(&self) -> bool { false }   // false = 호스트가 OFFSET 폴백
}
pub struct ExecResult { …, pub pending: Option<CursorHandle> }   // 마지막 결과 집합이 잘렸을 때만
```

- 레지스트리 = 드라이버(기존) · 설정 선택 = `grid.max_rows`/`cli.max_rows`/`grid.fetch_mode`(cursor|offset|off).
- 러너(`nsql-run`)는 `RunEvent::ResultSet{ rs, more, cursor: Option<CursorHandle> }` · 새 `Runner::fetch_next(h, n)`/`fetch_all(h, budget, progress)` · `count(sql)`.

### 3-3. 드라이버별 구현(커서 유지)

| 드라이버 | 커서 유지 방법 | 조기 중단 | 비고 |
|---|---|---|---|
| **Oracle**(`oracle` crate) | `Statement::into_result_set()`로 **소유 ResultSet**(문장 수명 = 커서 수명)을 세션 구조체에 보관 · `fetch_next` = 이터레이터 N행 | ✅(09-15) | `fetch_array_size` = `fetch_size`. 커밋해도 Oracle 커서는 살아 있으나 **ORA-01002 방지**를 위해 우리 규칙(커밋/롤백 = 닫기) 적용 |
| **SQLite** | `Statement` + `Rows`를 보관(연결과 수명 결합 → `Box<dyn …>` + unsafe 없이 `rusqlite::Statement`를 `OwnedStatement` 부품으로) | ✅ | 단일 스레드 · 트랜잭션 무관 |
| **PostgreSQL**(tokio-postgres) | **Portal**: `transaction.bind(stmt) → Portal` · `query_portal(&portal, max_rows)` = 서버가 N행만 보냄 · 트랜잭션 안에서만 살아 있음 | ✅(Portal) | 자동 커밋 모드에선 결과를 다 읽기 전까지 **암묵 트랜잭션 유지** → [34](34-transaction-ux.md) 탭 배지에 "커서 열림" 표시 · `close_cursor`가 커밋 |
| **SQL Server**(tiberius) | `QueryStream`은 클라이언트를 빌려 쓴다 → 워커 **비동기 태스크 안에 스트림을 잡고** `fetch_next` 요청을 채널로 받아 N행 넘김(태스크 = 세션 소유자 · 요청 없을 땐 대기 · 서버는 TDS 흐름 제어로 멈춤) | ✅(스트림) | 1차는 **OFFSET 폴백**(`cursor_supported=false`)으로 출발 · 스트림 유지는 T-48c |

### 3-4. OFFSET 재질의 폴백(커서가 없거나 닫혔을 때)

- 방언별 래핑: PG/SQLite/MySQL `SELECT * FROM (q) x LIMIT n OFFSET k` · Oracle 12c+ `OFFSET k ROWS FETCH NEXT n ROWS ONLY` · MSSQL `… ORDER BY (SELECT NULL) OFFSET k ROWS FETCH NEXT n ROWS ONLY`(ORDER BY 필수).
- **정렬(ORDER BY) 없는 질의**는 페이지가 겹칠 수 있다 → 상태줄·로그 `perf` 1줄 경고("정렬 없는 재질의 — 결과가 겹칠 수 있음 · 커서 유지 권장") · 설정 `grid.offset_warn`(기본 켬).
- 세션 변수(DR-8)·임시 테이블에 기대는 질의는 **같은 세션**에서 재질의(메타 세션 X).

### 3-5. 메모리·해제 규칙(D-72 · [39](39-resource-governance.md))

| 항목 | 규칙 | 설정 |
|---|---|---|
| 세그먼트 크기 | 200(전역) · 탭별 상자(1~100,000) | `grid.max_rows` · 탭 로컬 |
| 예산 | **탭별 독립**(09-17): 이 탭의 결과 데이터(`ResultData` 세그먼트 누적) **+ 텍스트 보기 파생 캐시**(`text_bytes` · 그리드로 돌아오면 0) ≤ 예산 → 넘으면 Fetch next/자동 페치 **거부 + 안내** · 전체 조회는 받으면서 예산에서 멈춤 · 뷰 전환은 복사 0이라 봉우리 없음(DR-33) | `grid.memory_budget_mb` 1024 |
| 전체 페치 | 배치 스트리밍(`fetch_size` 2000) · 진행률(행·MB·초) · **취소 버튼** · 예산·`grid.fetch_all_max`(0 = 예산까지)에서 멈춤 | `grid.fetch_all_max` |
| 자동 페치 | 스크롤이 마지막 행 − 1화면에 닿으면 다음 세그먼트 1회(연속 요청 금지 · 진행 중이면 무시) | `grid.auto_fetch` 켬 |
| 해제 | 탭 닫기·재실행·접속 해제 → rows `Vec` drop + `shrink_to_fit` 없는 새 Vec · 커서 close · `grid_stash`에서 제거. 비활성 탭은 **그리기 0 · 폭 캐시만** 유지 | — |
| 부하원 원장 | [39 §3](39-resource-governance.md)에 두 줄: "서버 커서(세션당 1)" · "결과 메모리(예산)" — 끄는 키 `grid.fetch_mode=off`(= 종전 단발 페치) | `grid.fetch_mode` |
| 측정 | S-2 방식: 200행 ×(Fetch next 10회 → 탭 닫기) 5주기 뒤 Private 증가 ≤ 100 KB · 커서 핸들 0 | `scripts/memcycle.ps1` 시나리오 추가 |

## 4. 설계 — GUI 결과 패널·탭

### 4-1. 구조(`grid_stash` 확장)

```
App.results: HashMap<EditorId, ResultPanel>        // 편집기 탭 ↔ 결과 패널(09-16 3차 쌍 규칙 유지)
ResultPanel { tabs: Vec<ResultTab>, active: usize, tabbar: TabBar }
ResultTab   { title, sql, grid: Grid, max_rows: usize /*탭 로컬*/, cursor: Option<CursorHandle>,
              more: bool, total: Option<u64> /*Count*/, fetched_at, elapsed, pinned: bool, bytes: usize }
```

- **Ctrl+Enter**(`run.statement`): 활성 결과 탭에 **교체**(rows drop → 새 결과 · 탭 로컬 max_rows 유지). 결과 탭이 없으면 하나 만든다.
- **Ctrl+\\**(`run.statement_new_tab` · mac `cmd+\\`): 새 결과 탭 추가 → 활성. 탭 제목 = 문장에서 추정한 테이블/첫 단어 + 순번(`M4S_I002040 2`) · 더블클릭 이름 바꾸기 · 고정(pin)은 자동 정리에서 제외.
- 탭 상한 `grid.result_tabs_max`(기본 8 · 편집기당) — 넘으면 **가장 오래된 비고정 탭** 닫기(rows 해제) + 상태줄 안내.
- F5(스크립트 전체)는 종전대로 마지막 결과 집합을 활성 탭에(여러 결과 집합 = D-5와 함께 탭으로 펼치는 옵션 `grid.script_results_tabs`).
- 그리기: 활성 탭만 `Grid::paint` · 탭바는 편집기 탭바 부품(`TabBar`) 재사용 · 우클릭 = 닫기/다른 탭 닫기/고정/이름 바꾸기/Export.

### 4-2. 하단 결과 도구줄(1번 이미지 대응 · 좌→우)

| 요소 | 동작 | 1차 |
|---|---|---|
| Refresh | 탭의 `sql`을 같은 세션에서 재실행(첫 세그먼트 · 탭 max_rows) | ✅ |
| 행 추가/삭제/복제 · Save/Cancel | 데이터 편집기(T-94 · [41](41-sql-copy-key-rules.md) 키 규칙으로 문장 생성 · [34](34-transaction-ux.md) 트랜잭션) | 후속 |
| `|<  <  >  >|` | 선택 행 이동(첫/이전/다음/끝) — 키보드와 같음 | ✅ |
| **세그먼트 상자** `200` | 탭 로컬 max_rows(편집 → Enter → 다음 요청부터) · 초기값 = `grid.max_rows` | ✅ |
| **`200+`**(Fetch next) | `fetch_next(cursor, max_rows)` → 이어 붙임 · 커서 없으면 OFFSET | ✅ |
| **Fetch all** | §3-5 · 진행률 + 취소 | ✅ |
| **Count** | 메타 세션 `SELECT COUNT(*) FROM (sql) x` → 상태 "N / 총 M" · 실패(방언·DDL)면 조용히 비활성 | ✅ |
| Export… | 기존 Export(파일 · [40](40-cli-usage.md) CLI와 같은 형식) — **탭 rows가 아니라 재질의로 무제한**(D-68) | ✅(기존) |
| 상태 문구 | `200행 · 더 있음 · 0.052s(fetch 0.047s) · 15:20:52` · Count 뒤 `200 / 총 12,345` · 예산 근접 시 색 | ✅ |

### 4-3a. 결과 탭 비용 검토 · 켜기/끄기(사용자 09-16 · D-73~75)

**질문**: 다중 탭이 성능·메모리를 무겁게 하지 않는가 · 끄는 설정이 필요한가 · 탭이 1개면 탭 바를 숨길 수 있는가 · 탭 이동 수단.

| 비용 축 | 실제 | 판정 |
|---|---|---|
| 메모리 | 잠든 탭 = `rows: Vec<Vec<Value>>` + 폭 캐시 + 구조체 ~1 KB. 기본 세그먼트 200행 × 20컬럼 ≈ 300 KB/탭 → 8탭 ≈ 2.4 MB. 커지는 경우는 **전체 조회·큰 세그먼트**뿐이고 이는 예산 D-72(`grid.memory_budget_mb` · 탭 합계)가 막는다 | 가볍다(예산이 상한) |
| CPU/그리기 | **활성 탭만** 페인트 · 잠든 탭은 0 · 탭 바 한 줄 ≈ 0.1 ms(단일 행 · 넘칠 때만 ◀ ▶) | 무시 가능 |
| 서버 | OFFSET 폴백은 커서를 열어 두지 않는다 · 커서 유지(T-48a) 뒤에도 세션당 1개 규칙 | 0 |
| 코드 | `grid_stash`(편집기 탭 ↔ 그리드)를 `ResultPanel{tabs}`로 한 단계 넓힘 · TabBar 부품 재사용(단일 행 · ◀ ▶ · 휠 · 드래그 · 우클릭 · 핀이 이미 있음) | 중(부품은 있음) |

**결론: 설계 자체가 무거운 요소는 없다 → 도입한다.** 다만 사용자의 두 이유(메모리 상한을 강제로 낮추기 · 탭 바 한 줄의 공간)는 정당하므로 **끄기 설정을 둔다** — 끄면 데이터 구조·그리기 경로가 지금(탭 1개)과 완전히 같아 비용 0.

| # | 결정 | 내용 |
|---|---|---|
| **D-73** | `grid.result_tabs` 켬(기본) / 끔 | 끔 = Ctrl+\가 Ctrl+Enter처럼 동작 · 탭 바 없음 · 켬→끔 전환 시 **활성 탭 외 전부 즉시 해제**(rows drop · 커서 close) — "강제로 메모리 줄이기" |
| **D-74** | 탭 바 표시 `grid.result_tabbar` = auto(기본) / always | auto = **탭이 2개 이상일 때만** 한 줄 표시(1개면 그리드가 그 높이를 쓴다) · always = 항상(고정 레이아웃 선호자) |
| **D-75** | 결과 탭 바 = **단일 행 고정**(`set_multiline(false)`) | 넘치면 오른쪽 끝 ◀ ▶(TabBar 내장) · 휠/가로 휠 · 드래그 재정렬 · **우클릭 메뉴**: 닫기 · 다른 탭 닫기 · 오른쪽 탭 닫기 · 고정/해제 · 이름 바꾸기 · **맨 앞으로/맨 뒤로 이동** · **첫/마지막 탭 활성**(한 번에 이동) · 활성 탭이 바뀌면 보이도록 자동 스크롤(내장) |

**속도 여지(켜기/끄기 목록)**: `grid.result_tabs`(구조 자체) · `grid.result_tabbar`(그리기 한 줄) · `grid.result_tabs_max`(8 · 넘치면 가장 오래된 비고정 탭 자동 닫기 `grid.result_tab_evict` 켬) · `grid.memory_budget_mb`(합계) · `grid.auto_fetch` · 탭 전환 시 폭 캐시 유지(재측정 0) · 탭 제목 측정은 제목이 바뀔 때만.

**구현 규칙(성능)**: ① 이벤트·페인트는 활성 `ResultTab`만 만진다 ② 워커 결과는 `(편집기 탭 id, 결과 탭 id)` 키로 잠든 탭에도 배달(그리기는 안 함) ③ 탭 닫기 = `Vec` drop + 커서 close(즉시) ④ 예산 초과 시 새 탭 생성 대신 "가장 오래된 비고정 탭 닫기 or 예산 상향" 안내 ⑤ 탭 바는 `grid.result_tabbar=auto`에서 탭 수가 1↔2를 넘을 때만 레이아웃 재계산.

### 4-3. 키·설정·i18n

- 키맵(`keymap.rs`): `run.statement_new_tab` = `ctrl+\\` / `cmd+\\` · `result.fetch_next` = `ctrl+alt+pagedown` · `result.fetch_all` = `ctrl+alt+shift+pagedown` · `result.tab.close` = `ctrl+shift+w` · `result.tab.next/prev` = `ctrl+alt+right/left`(Sublime·DBeaver 충돌 없음 · [24](24-settings-and-vscode-analysis.md) 캡처 창에서 바꿀 수 있음).
- 설정(REGISTRY · 라벨 = Msg): `grid.result_tabs`(on) · `grid.result_tabbar`(auto|always) · `grid.result_tab_evict`(on) · `grid.max_rows`(있음 · 200) · `grid.fetch_mode`(cursor|offset|off) · `grid.auto_fetch`(on) · `grid.fetch_all_max`(0) · `grid.memory_budget_mb`(256) · `grid.result_tabs_max`(8) · `grid.offset_warn`(on) · `grid.script_results_tabs`(off) · `db.fetch_size`(200 · HIDDEN) · `cli.max_rows`(200) · `cli.auto_more`(off).
- 문구: 상태 `StRowsMore` 갱신(`처음 {0}행 · 더 있음 · {1}s`) · 예산 안내 · 정렬 없는 재질의 경고 · 탭 이름 등 — 전부 `Msg`.

## 5. 설계 — CLI(`nsql shell`)

- **shell만 상한**(D-68): `cli.max_rows`(200) · `set max_rows N` · `show`에 표시. `nsql run`/`export`/`-o`/파이프는 무제한(`--max-rows`를 주면 그 값 — 종전).
- 잘리면 표 뒤 한 줄: `-- first 200 rows · more: \more · \all · \count` (형식이 grid/markdown일 때만 · csv/tsv/json은 shell에서도 잘리지 않음 — 복사·저장용 출력이므로 D-68).
- 명령: `\more [n]`(= `more`) = 다음 세그먼트(커서 · 폴백 OFFSET) · `\all` = 전체(예산 `cli.memory_budget_mb`? → **없음**: 터미널은 스트리밍 출력이라 메모리 상한 불필요 · 대신 Ctrl+C 취소) · `\count` = COUNT(*) · `\pager on|off`(psql식 · `PAGER`/`less`) — 표시가 길면 페이저.
- `cli.auto_more`(off): 켜면 psql `FETCH_COUNT`처럼 **묻지 않고 세그먼트를 이어 출력**(스트리밍 · 메모리 상한) — 배치가 아닌 대화형에서 큰 표를 훑을 때.
- 도움말(`help.rs`) 옵션 원장에 `set max_rows` · `\more/\all/\count` 추가 · 기본값은 설정에서 읽음(22차 규칙).

## 6. 부하 점검([26 §8](26-performance-architecture.md) · [39 §6](39-resource-governance.md))

| 항목 | 답 |
|---|---|
| 새 트래픽 | Fetch next/all/Count = **사용자 동작 1회당 1요청**(자동 페치도 스크롤 끝 1회 · 진행 중 무시). 자동 재시도 없음 |
| 서버 자원 | 세션당 커서 1개 · 커밋/롤백/새 실행이 닫음 · 접속 해제 시 close · 유휴 커서 상한 `db.cursor_idle_secs`(0 = 없음 · 켜면 초과 시 닫고 폴백) |
| 메모리 | 예산 합계(D-72) · 해제 시점 명시(§3-5) · S-2 5주기 측정 |
| CPU/그리기 | 활성 탭만 그림 · 이어 붙일 때 폭 캐시는 새 행만 측정(T-50 컬럼 저장소 전까지 `Vec<Vec<Value>>`) |
| 끄는 키 | `grid.fetch_mode=off`(단발 페치 · 종전 동작) · `grid.auto_fetch=off` · `cli.auto_more=off` |

## 8. 실행 대기열(Run Queue) — 설계만(사용자 09-17 · 구현 = **T-112 · 우선순위 최하위**)

> **지금 규칙(✅ 구현됨)**: 편집기 하나에서 **실행은 한 번에 하나** — 실행 중(`busy`)이면 Ctrl+Enter/F5/Ctrl+\는 막히고 상태줄에 "실행 중"(`run_sql` 가드). 이전 요청이 끝난 뒤에만 다시 실행된다.
> **나중(T-112)**: 막는 대신 **대기열에 넣고 순차 실행**. 아래는 그때의 설계.

### 8-1. 동작

| 규칙 | 내용 |
|---|---|
| 자리 | 편집기 탭마다 대기열 하나(`RunQueue { running: Option<RunJob>, waiting: VecDeque<RunJob> }`) · 접속 세션은 하나이므로 실제 실행은 **앱 전체에서 한 번에 하나**(탭 간 = FIFO · 세션 단위 직렬) |
| 상한 | 실행 중 1 + 대기 2 = **카드 최대 3장**(실행 상태 카드 [runtoast](../crates/nexa-sql/src/runtoast.rs) 위로 쌓임) · 3장이 찼을 때 실행 요청 = 상태줄 "대기열이 가득 찼습니다(3)" + 토스트 · 설정 `run.queue_max`(2 · 0 = 지금처럼 막기) |
| 대기 카드 | 문장 한 줄 · "대기 n번째" · 넣은 시각 · **취소 ×**(대기 중인 것만) — 실행 중인 카드의 ■는 지금처럼 사용자가 직접(취소 포트 T-108) |
| 취소 | 대기 카드 × → 목록에서 제거 · 뒤 카드가 앞으로 당겨져 **토스트 자리(대기 1번째)** 에 보임 · 실행 중인 것은 일괄 취소 대상 아님 |
| 일괄 취소 | 툴바 ■ 길게/우클릭 "대기열 전부 취소" · 팔레트 `run.queue_clear` · 상태줄 대기열 세그먼트 클릭 메뉴 — **실행 중 제외** · 개수 확인 토스트 |
| 다음 실행 | 실행 중 작업이 끝나면(`worker.done` · 오류 포함) 대기열 앞 작업을 **즉시** 시작 · 사용자가 그 사이 편집기를 바꿨어도 작업에 담긴 **문장 스냅샷**으로 실행(캐럿 문장을 다시 읽지 않는다) |
| 결과 자리 | 앞 결과를 덮어쓰지 않도록 대기열에서 시작하는 실행은 **항상 새 결과 탭(Ctrl+\ 경로 · `run.statement_new_tab`)** — 첫 작업만 사용자가 누른 키대로(Ctrl+Enter = 교체) · 결과 탭 상한(`grid.result_tabs_max`)에 걸리면 고정 안 된 가장 오래된 탭 정리(43 §4 규칙) |
| 트랜잭션 | 수동 커밋 중이면 대기 작업도 같은 트랜잭션에 이어진다(세션 하나) — 대기 카드에 Tx 배지 · 일괄 취소는 커밋/롤백을 건드리지 않는다 |
| 접속 끊김 | 실행 중 작업 오류 + 대기열 **전부 취소**(카드에 "접속 끊김" 사유) |
| 대기열 관리 | ★ **관리 기능 필요(기록)**: 상태줄 세그먼트 "⏳ 실행 1 · 대기 2" → 클릭 = 목록 팝업(순서 바꾸기 ↑↓ · 개별 취소 · 전부 취소 · 문장 보기) · 트랜잭션 로그 창에 대기 작업도 "대기" 행으로(44) |

### 8-2. 구조

```
run_sql(all) ─▶ RunJob{ id, editor, text(스냅샷), mode: Replace|NewTab, enqueued_at, tx_hint }
   ├─ running 없음 ──▶ start(job)                     (지금 경로 · 카드 = 실행 중)
   ├─ waiting.len() < queue_max ──▶ waiting.push_back  (카드 = 대기 n)
   └─ 가득 ──▶ 거절(상태줄 · 토스트)
worker.done ─▶ running = None ─▶ waiting.pop_front() ─▶ mode = NewTab 강제 ─▶ start
cancel(id)   ─▶ waiting.retain(≠ id) ─▶ 카드 재배치
cancel_all() ─▶ waiting.clear()(running 제외)
```

- `RunJob.text`는 실행 시점이 아니라 **넣는 시점**의 문장(사용자가 편집해도 대기 작업은 그대로 · 카드 툴팁에 원문).
- 카드 = `RunToast`를 `Vec`으로(실행 중 1 + 대기 ≤ 2 · 대기 카드는 2행(문장 · "대기 n · hh:mm:ss") · ×). 그리기 순서 = 실행 중 맨 아래 → 대기 1 → 대기 2 → 일반 토스트.
- 설정: `run.queue_max`(2 · 0 = 막기) · `run.queue_new_tab`(on · off = 교체 허용 — 결과 덮어씀 경고) · `run.queue_toast`(on).
- 부하: 큐는 메모리 문장 ≤ 3 · 스레드 0 추가(워커 하나) · 26 §8 무관.

## 7. 작업 순서(T-48 갱신 · [TODO](TODO.md))

> **진행(09-16 25차)**: T-48a의 OFFSET 폴백 경로(`paging.rs` · `query_once` · 워커 `FetchPage/Count`)와 T-48b 도구줄(세그먼트 상자 · 전체/건수 · 상태 · 보기 모드 · 자동 페치)이 1차로 들어갔다. 서버 커서 유지(Oracle/SQLite/PG Portal)·예산·결과 탭(T-93)·데이터 편집기(T-94)는 잔여.

| 단계 | 내용 | 산출 |
|---|---|---|
| **T-48a** 포트 ✅ 09-16 49차 | `Session::fetch_next/close_cursor/cursor_supported` · `ExecResult.pending` · 러너 `fetch_next/fetch_all/count/fetch_page` · Navigate 스팬 · Oracle·SQLite 커서 유지 · **PG = DECLARE CURSOR**(Portal은 동기 크레이트 수명 문제 · 실서버 미검증) · MSSQL 폴백 · OFFSET 래퍼 · `RunEvent` 모양 불변(러너가 커서 보관) · 커서 열린 동안 자동 커밋 유예 | core/run/driver |
| **T-48b** GUI ✅ 09-17 51차 | 결과 도구줄(세그먼트 상자 · Fetch next · Fetch all **스트리밍(배치 진행률 · ■/Esc 취소 · 예산 수신 중 판정)** · Count · Refresh · 이동) · 탭 로컬 max_rows · 예산(탭별) · 상태 문구 · 설정 | `grid.rs`/`main.rs`/`worker.rs` · 러너 `query_stream` |
| **T-93** 결과 탭 ✅ 09-16 49차 | `ResultPanel`/`ResultTab` · Ctrl+\\ · 탭바(TabBar 재사용) · 우클릭 메뉴 · 상한·고정·해제 · 편집기 탭 쌍 유지 · 메모리 예산(D-72) | GUI `results.rs` |
| **T-48c** 자동 페치·MSSQL 스트림 | 스크롤 끝 자동 페치 · tiberius 스트림 유지 · `db.cursor_idle_secs` | GUI/driver |
| **T-48d** CLI ✅ 09-16 49차 | `cli.max_rows` · `\more/\all/\count` · `\pager` · `cli.auto_more` · 도움말 · Oracle e2e(`\more` 25~31ms · count 62,449) | CLI |
| **T-94** 데이터 편집기 | 행 추가/삭제/복제 · 셀 편집 · Save = [41](41-sql-copy-key-rules.md) 키 규칙으로 INSERT/UPDATE/DELETE · [34](34-transaction-ux.md) | 별도 설계 |

**검증 게이트**(39 §6): Oracle 사내 실서버 100만 행 테이블 — 첫 세그먼트 ≤ 0.3s · Fetch next 10회 연속 왕복 ≤ 0.2s/회 · Fetch all 예산 256 MB에서 멈춤 + 안내 · 탭 닫기 뒤 Private 복귀 · 커서 유휴 시 V$OPEN_CURSOR 1개.
