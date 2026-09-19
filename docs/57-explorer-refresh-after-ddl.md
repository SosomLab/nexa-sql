# 57 · 객체 탐색기 갱신 — DDL 뒤 · 주기 · 범위 (다른 도구 조사 · 설계) (사용자 요청 09-19)

> **요구**: "create, drop 등 객체 정보가 바뀌어도 객체 탐색기에 갱신이 안 된다 — 갱신 시점·주기·범위를 다른 앱의 설정과 기능으로 조사하고, 우리 구현 방식을 설계해 확인받기".
> **상태**: 📐 설계 확정(09-19 · D-106~D-110 전부 권장안) → 개발 T-138. 관련: [28 객체 탐색기](28-object-explorer.md) · [47 §5 메타 갱신(D-82 ✅ 수동+DDL 감지+워터마크)](47-intellisense-metadata.md) · [26 §8 네트워크 부하 규칙](26-performance-architecture.md).

## 0. 결론 다섯 줄

1. 지금 탐색기는 **수동 새로 고침(우클릭·F5)뿐**이다. 설정 `explorer.auto_refresh`·`explorer.refresh_secs`는 레지스트리에만 있고 **배선되지 않았다**. 47에서 정한 갱신 정책(D-82: 수동 + 실행한 DDL 감지 + 워터마크 주기)은 메타 저장소(T-57) 설계에만 있고 탐색기 트리에는 아직 없다.
2. 도구들은 셋으로 갈린다: **수동만**(SSMS · SQL Developer · pgAdmin · DBeaver 편집기 DDL) · **실행한 문장을 분석해 그 객체만**(DataGrip "smart refresh") · **증분/주기**(DataGrip incremental · DBeaver 데이터 편집기의 auto-refresh). 사용자가 가장 불편해하는 것은 첫째(DBeaver #859·#21734, SSMS IntelliSense Ctrl+Shift+R).
3. 권장 = **DataGrip식 "실행한 DDL만큼만"을 기본**으로: 실행이 성공한 DDL 문장에서 (종류·스키마·이름)을 뽑아 **그 폴더(버킷) 하나만** 다시 읽고, 펼침·선택·스크롤을 보존한 채 **디프로 반영**한다. 거기에 **유휴 워터마크 주기**(다른 세션·다른 사람의 변경)와 **수동 범위 새로 고침**을 얹는다.
4. 시점의 함정 둘: **트랜잭션 DDL 방언(PostgreSQL·SQL Server·SQLite)의 수동 커밋** — 메타 세션은 커밋 전 변경을 못 본다 → **커밋 시점에 갱신** · **스크립트 실행** — 문장마다가 아니라 **실행이 끝난 뒤 버킷별 1회**(디바운스).
5. 트래픽은 "내가 DDL을 실행했을 때 1질의" + "유휴 주기 1행 질의"뿐이고 전부 설정으로 끈다(26 §8 · 39 §3 등재).

## 1. 다른 도구

| 도구 | 편집기에서 DDL 실행 뒤 | 주기·자동 | 수동 | 범위 | 비고 |
|---|---|---|---|---|---|
| **DataGrip** | **Smart refresh** — "문장이 바꿀 수 있는 객체를 분석해 그 집합만 새로 읽는다"(Auto sync 기본 켬) | **증분 인트로스펙션** — 앞선 조회 뒤 바뀐 객체만(서버의 수정 시각·xmin 등) | Refresh(Ctrl+F5) · "Forget Cached Schemas" | 선택한 노드/스키마(fragment) · 전체(full) · 인트로스펙션 **수준 1~3**(이름만 / 소스 제외 / 전부 — 객체 수로 자동 선택) | 대형 스키마 대응이 1급 |
| **DBeaver** | 자동 갱신 없음 — 내비게이터 F5 필요(메타 캐시). 오래된 개선 요청 #859 · "객체를 못 찾으면 캐시를 다시 읽자" #21734 | 데이터 편집기만 auto-refresh(초) · 내비게이터는 없음 | F5(선택 노드 하위) · 접속 Invalidate/Reconnect | 선택 노드 | 자체 대화상자로 만든 객체는 트리에 즉시 |
| **SSMS** | 없음 — Object Explorer는 F5 · IntelliSense 캐시는 Ctrl+Shift+R | 없음 | F5 · Refresh Local Cache | 선택 노드 | "새 테이블에 빨간 밑줄"이 대표 불만 |
| **Oracle SQL Developer** | 없음(자체 대화상자 DDL만 반영) | 없음 | 접속/노드 Refresh | 선택 노드 | — |
| **pgAdmin** | 없음(Query Tool) | 없음 | 노드 Refresh | 선택 노드 | 자체 대화상자는 즉시 |
| **TablePlus** | 구조 편집기 저장 시 반영 · 질의 창 DDL은 ⌘R | 없음 | ⌘R(전체 reload) | 접속 전체 | 작은 스키마 전제 |
| **Toad / PL/SQL Developer** | 옵션으로 "DDL 뒤 Schema Browser 갱신"(알려진 동작 · 미검증) | 없음 | F5 | 객체 종류 탭 | — |

**읽을 점**: ① 대부분 수동이고 그게 불만의 원천 ② 자동으로 하는 DataGrip도 **전체가 아니라 문장이 건드린 객체만** ③ 주기 갱신은 "바뀐 것만"(증분) ④ 범위는 늘 **선택 노드 하위**가 단위 ⑤ 큰 스키마는 읽는 깊이(수준)를 줄인다 — 우리는 이미 "직계 자식만 · 펼칠 때 로드"(28 §3)라 수준 1에 해당.

## 2. 설계

### 2-1. 갱신 트리거 넷

| # | 트리거 | 시점 | 범위 | 비용 |
|---|---|---|---|---|
| T1 | **내가 실행한 DDL** | 문장 성공 직후 — 단, **트랜잭션 DDL 방언 + 수동 커밋**이면 그 트랜잭션이 **커밋될 때**(롤백이면 버림) · 스크립트/여러 문장은 **실행이 끝난 뒤 버킷별 1회** | 문장에서 뽑은 (스키마, 종류) **폴더 하나** — `CREATE/DROP/RENAME` = 그 종류 폴더 목록 · `ALTER TABLE`/`COMMENT` = 그 테이블이 펼쳐져 있으면 컬럼만 · `CREATE SCHEMA/USER`·`DROP SCHEMA` = 스키마 목록 | 폴더당 1질의(이미 있는 `children` 질의) · 폴더가 **한 번도 안 펼쳐졌으면 0**(다음에 펼칠 때 어차피 새로 읽음) |
| T2 | **유휴 워터마크 주기** | `meta.refresh_secs`(기본 300초 · 0 = 끔) · 앱이 유휴(입력 없음 5초+)이고 그 서버 메타 세션이 온라인일 때만 | **펼쳐 본 스키마**마다 1행 질의로 "마지막 DDL 시각·객체 수"를 비교 → 바뀐 스키마의 **펼쳐진 폴더만** 다시 읽음(Oracle `MAX(LAST_DDL_TIME),COUNT(*)` · SQL Server `MAX(modify_date),COUNT(*)` · PostgreSQL `COUNT(*)+MAX(oid)`/`pg_stat` · SQLite `PRAGMA schema_version`) | 스키마당 1행 |
| T3 | **수동** | F5 / 우클릭 "새로 고침" / 탐색기 머리 ⟳ | 선택 노드 하위(종전) · **루트에서 = 그 서버 전체(펼쳐진 것만)** · Shift+F5 = 캐시를 버리고 전부 | 펼쳐진 폴더 수 |
| T4 | **못 찾음 신호** | 실행 오류가 "객체 없음"(ORA-00942 · 42P01 · 208 …)이고 그 이름이 트리에 **있을 때**(= 트리가 낡았다) 또는 조회가 성공했는데 트리에 **없을 때** | 그 (스키마, 종류) 폴더 | 1질의 · 같은 폴더는 60초에 1회 |

### 2-2. 반영 방식 — 다시 만들지 않고 **디프**

- 지금 `refresh(i)`는 자식을 전부 떼고 다시 읽는다 → 펼쳐 둔 테이블의 컬럼·선택·스크롤이 사라진다.
- 새 방식: 새 목록과 옛 자식을 **이름(+종류)으로 맞춰** 추가·삭제·유지로 나눈다 — 유지되는 노드는 **그대로**(펼침·자식·선택 보존) · 새 노드는 정렬 위치에 삽입 · 사라진 노드만 제거(선택돼 있었으면 부모로). 스크롤은 첫 보이는 노드를 기준으로 유지.
- 새로 생긴 객체는 2초간 옅은 강조(선택은 옮기지 않는다 — 입력 중 방해 금지).
- 로딩 표시는 폴더 옆 작은 점(…)만 — 트리 전체를 "불러오는 중"으로 바꾸지 않는다.

### 2-3. 문장 → 대상(파서)

- 이미 있는 것: `TxClass::is_ddl` · `first_keyword` · 47 §5의 `parse_create_header`(설계). 추가: `nsql_core::ddl_target(sql, dialect) -> Option<DdlTarget { verb, kind, schema, name }>` — `CREATE [OR REPLACE] [GLOBAL TEMPORARY|UNIQUE|MATERIALIZED …] <KIND> [IF NOT EXISTS] [schema.]name` · `DROP <KIND> [IF EXISTS] …` · `ALTER <KIND> …` · `RENAME a TO b` · `COMMENT ON` · `TRUNCATE`(목록 불변 → 무시) · 인용 식별자 · 방언별 대소문자 접기(Oracle 대문자 · PG 소문자).
- 못 읽은 DDL(동적 SQL · PL/SQL 블록 안 `EXECUTE IMMEDIATE`)은 T1이 아니라 T2/T4가 잡는다 — 추측으로 전체를 다시 읽지 않는다.
- 스키마가 생략되면 그 세션의 현재 스키마(접속 계정 · `ALTER SESSION SET CURRENT_SCHEMA`/`search_path`/`USE`를 러너가 본 값).

### 2-4. 여러 세션·여러 탭

- 같은 서버에 붙은 **모든 세션의 DDL**이 그 서버 탐색기 칸 하나를 갱신한다(탐색기는 서버당 하나 · 52).
- 다른 프로그램·다른 사람의 변경 = T2(주기)와 T4(못 찾음) 몫.
- 인텔리센스 메타 저장소(T-57 ②)가 들어오면 **같은 트리거가 저장소를 갱신하고 탐색기는 그 저장소를 본다**(47 §5와 한 벌 · 설정 키도 `meta.*` 하나).

### 2-5. 설정(47 §8의 `meta.*`를 그대로 · 탐색기 분류에 노출)

| 키 | 기본 | 뜻 |
|---|---|---|
| `meta.refresh_on_ddl` | on | T1 — 실행한 DDL 뒤 그 폴더만 갱신 |
| `meta.refresh_on_commit` | on | T1 — 트랜잭션 DDL 방언의 수동 커밋: 커밋 때 갱신(off = 문장 직후 시도) |
| `meta.refresh_secs` | 300 | T2 — 유휴 워터마크 주기(0 = 끔 · 옛 `explorer.refresh_secs` 값을 옮김) |
| `meta.refresh_idle_secs` | 5 · HIDDEN | T2 — 유휴 판정 |
| `meta.refresh_scope` | changed | T2 — 바뀐 스키마만 / 펼쳐진 전부 |
| `meta.refresh_on_missing` | on | T4 — 못 찾음 신호로 그 폴더 갱신 |
| `meta.refresh_highlight_ms` | 2000 · HIDDEN | 새 객체 강조 시간(0 = 없음) |

옛 키 `explorer.auto_refresh`/`explorer.refresh_secs`는 `meta.refresh_secs`로 통합(마이그레이션: auto_refresh=on이면 refresh_secs 값을 옮기고 off면 0).
성능 향상 모드: T2만 끈다(T1은 사용자 동작에 딸린 1질의라 유지).

## 3. 결정(사용자 확인)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| D-106 ✅ ①(09-19) | DDL 실행 뒤 자동 갱신 기본 | ① 켬(그 폴더만 · 디프) ② 끔(수동만) ③ 켬 + 서버 전체 | **①** |
| D-107 ✅ ①(09-19) | 트랜잭션 DDL 방언의 수동 커밋 | ① 커밋 때 갱신 ② 문장 직후(메타 세션이 못 볼 수 있음) ③ 둘 다 | **①** |
| D-108 ✅ ①(09-19) | 유휴 주기 갱신 기본 | ① 300초 · 바뀐 스키마만 ② 끔 ③ 60초 | **①** |
| D-109 ✅ ①(09-19) | "못 찾음" 신호 갱신 | ① 켬 ② 끔 | **①** |
| D-110 ✅ ①(09-19) | 새 객체 표시 | ① 2초 옅은 강조(선택 이동 없음) ② 강조 + 그 객체로 스크롤 ③ 표시 없음 | **①** |

## 4. 단계

① `ddl_target` 파서 + 테스트(방언 4) ② 탐색기 디프 갱신(`refresh_diff`) ③ T1 배선(실행 완료·커밋 시점 · 버킷 디바운스 · 전 세션) ④ T3 범위(루트·Shift+F5) ⑤ T4 ⑥ T2 워터마크(방언별 1행 질의) ⑦ 설정·마이그레이션·문서(28 §4 · 26 §8 · 39 §3).

## 출처

- DataGrip: [Metadata and introspection(smart refresh · incremental)](https://www.jetbrains.com/help/datagrip/introspection.html) · [Introspection levels](https://www.jetbrains.com/help/datagrip/introspection-levels.html) · [Cannot find a database object in Database Explorer](https://www.jetbrains.com/help/datagrip/cannot-find-a-database-object-in-the-database-tree-view.html)
- DBeaver: [#859 Refresh cached metadata/navigator tree after manual DDL](https://github.com/dbeaver/dbeaver/issues/859) · [#21734 Reload metadata cache if object not found](https://github.com/dbeaver/dbeaver/issues/21734) · [Data Refresh(auto-refresh)](https://dbeaver.com/docs/dbeaver/Data-Refresh/7.3/)
- SSMS: [Refresh IntelliSense cache(Ctrl+Shift+R)](https://blog.sqlauthority.com/2013/07/04/sql-server-how-to-refresh-ssms-intellisense-cache-to-update-schema-changes/) · [MS Learn — refreshing the IntelliSense cache](https://learn.microsoft.com/en-us/archive/blogs/dtjones/refreshing-the-intellisense-cache)
- Oracle SQL Developer: [Concepts and Usage(Refresh)](https://docs.oracle.com/en/database/oracle/sql-developer/20.2/rptug/sql-developer-concepts-usage.html)
