# 41 · 결과 → SQL 문 생성의 키 규칙 — 조사 · 설계 · 결정 (사용자 요청 09-16)

> **요구**(사용자 09-16): CLI에서도 결과를 SQL 5종(select · insert · update · delete · merge)으로 출력 · **키 = PK → 없으면 첫 인덱스 → 없으면 앞 3개 컬럼** · 대체했으면 **요청당 1회** 경고(로그/결과 맨 아래) · GUI도 같은 규칙 · 설정으로 유일성 기준을 **① PK 또는 첫 유니크 인덱스 ② 전체 컬럼** 중 선택(기본 ①) · 다른 클라이언트 조사 후 설계·결정.
> **구현**: nsql-io `sqlgen`(GUI·CLI 공용 생성기) · nsql-catalog `keys()`(4 방언) · 설정 `sql.key_mode` · CLI `-f sql:<kind>`/셸 `set format sql:<kind>`/`copy sql:<kind>` · GUI Copy SQL(워커 키 조회 · 캐시).

---

## 1. 다른 클라이언트는 어떻게 하나

| 클라이언트 | UPDATE/DELETE 생성의 키 | 키가 없으면 | 사용자 조정 |
|---|---|---|---|
| **DBeaver** | 엔티티 식별자 = PK → 유니크 제약/인덱스 → **가상 키**(사용자 지정) | 편집 시 "가상 키 정의" 대화상자 · 설정 *Use all columns as key when no unique key* | 가상 키 · 설정 |
| **DataGrip** | PK → 유니크 | **전체 컬럼**을 WHERE에(경고 없음 · "no key" 배지) | 없음(수동 편집) |
| **HeidiSQL** | PK → 유니크 키 | 전체 컬럼 | 없음 |
| **Navicat** | PK | 전체 컬럼(Copy as Update Statement) | 없음 |
| **TablePlus** | PK(편집 자체가 PK 필요) | 편집 불가 · 복사는 첫 컬럼 | 없음 |
| **Toad** | 사용자 선택(Export ▸ Update · 키 컬럼 체크) | 사용자 선택 | 대화상자 |
| **SQL Developer** | PK(Export ▸ Update) | 전체 컬럼 | 없음 |
| **SSMS / sqlcmd / psql / sqlplus** | (기능 없음 — INSERT 스크립트만 또는 없음) | — | — |

**공통점**: PK → 유니크 → (없으면) **전체 컬럼**이 다수. 전체 컬럼 WHERE는 "안전하지만 길고, NULL·부동소수·CLOB 컬럼에서 한 행도 못 맞히는" 문제가 있어 DBeaver는 가상 키를 둔다. **앞 3개 컬럼**은 어느 도구도 쓰지 않는 규칙이라 **경고를 반드시 붙인다**(사용자가 지정한 규칙 · 여러 행이 맞을 수 있음).

## 2. 결정

| # | 결정 | 근거 |
|---|---|---|
| D-62 ✅ | **키 규칙(기본 `pk`)**: PK → 첫 유니크 제약/인덱스 → **앞 3개 컬럼** + 경고 1회. 키 컬럼은 결과에 **모두 있어야** 채택(없으면 다음 후보) | 사용자 지정 · 다수 도구와 같은 앞 두 단계 · 3컬럼 대체는 경고로 표시 |
| D-63 ✅ | **설정 `sql.key_mode`** = `pk`(기본) \| `all`(전체 컬럼 = DataGrip/HeidiSQL/Navicat 방식) — GUI Copy SQL · CLI `-f sql:*` · 셸 공통 | 사용자 요구 ①/② |
| D-64 ✅ | **경고는 요청당 1회**: CLI = 결과 맨 아래 `-- WARNING: …` SQL 주석(출력이 그대로 유효한 SQL) · GUI = 상태줄 + 로그 창 1줄 | 사용자 요구 · 문장 사이에 끼우면 붙여 넣기 방해 |
| D-65 ✅ | 테이블을 실행문에서 추정 못 하면 `T` + 경고(`-- WARNING: … T was used`) · INSERT는 키 경고 없음(키를 안 씀) | 종전 GUI 동작 유지 |
| D-66 ✅ | 키 조회는 **필요할 때 1회**(테이블당 캐시 · 접속마다 초기화) — 조회마다 프리페치하지 않는다 | [39 S-6](39-resource-governance.md) 추가 왕복 상한 |
| D-67 ⏳ | DBeaver식 **가상 키**(테이블별 사용자 지정 키 · 프로필에 저장) — 3컬럼 경고가 자주 뜨면 도입 | 후속(T-92) |

## 3. 규칙(공용 · `nsql_io::sqlgen`)

```
choose_key(mode, catalog_keys, result_columns):
  all → cols = 전체
  pk  → PK 컬럼이 전부 결과에 있으면 PK
      → 유니크(제약·인덱스 순서대로) 중 컬럼이 전부 결과에 있는 첫 것
      → 앞 min(3, n)개 컬럼 · source = FirstN(경고)
generate(dialect, table, names, rows, kind, key):
  SELECT cols FROM t WHERE k1 = v1 AND k2 = v2            (NULL은 IS NULL)
  INSERT INTO t (cols) VALUES (…)
  UPDATE t SET 비키 = … WHERE 키…                            (전 컬럼이 키면 SET 전체 = 같은 값)
  DELETE FROM t WHERE 키…
  MERGE INTO t t USING (SELECT … [FROM DUAL]) s ON (t.k = s.k AND …) WHEN MATCHED THEN UPDATE SET 비키 WHEN NOT MATCHED THEN INSERT
    (MSSQL: MERGE t AS t USING (…) AS s ON … · SQLite는 MERGE가 없어 Oracle 형태로 낸다)
```

키 원천(`nsql_catalog::keys`): Oracle `ALL_CONSTRAINTS(P/U)` + `ALL_INDEXES(UNIQUE)` · SQL Server `sys.indexes(is_primary_key/is_unique)` · PostgreSQL `pg_index(indisprimary/indisunique)` · MySQL `information_schema.table_constraints` · SQLite `PRAGMA table_info/index_list/index_info`. 스키마는 실행문의 접두(`s.t`) 또는 현재 스키마 · 따옴표 없는 이름은 방언 기본 대소문자(Oracle 대문자 · PG 소문자).

## 4. 사용법

| 곳 | 방법 |
|---|---|
| CLI 1회성 | `nsql run -c prod -f sql:update q.sql` · `-f sql:merge` · `sql`만 쓰면 insert |
| CLI 기본 | `nsql config set cli.format sql:insert` |
| 셸 | `set format sql:delete` · `copy sql:merge`(마지막 결과 → 클립보드 · 경고 포함) |
| GUI | 결과 그리드 우클릭 ▸ Advanced Copy ▸ Copy SQL ▸ SELECT/INSERT/UPDATE/DELETE/MERGE |
| 유일성 기준 | `nsql config set sql.key_mode all` · GUI 설정 창 ▸ Result Sets ▸ Key columns for generated SQL |

## 5. 검증(09-16 · SQLite · `scratch/sqlkinds.sql`)

| 테이블 | 키 | 결과 |
|---|---|---|
| `emp(id PK)` | PK | `WHERE id = 1` · MERGE `ON (t.id = s.id)` |
| `ux` + `UNIQUE INDEX (a, b)` | 유니크 | `WHERE a = 'a1' AND b = 'b1'` |
| `nokey(p,q,r,s,t)` | 앞 3 | `WHERE p = '1' AND q = '2' AND r = '3'` + `-- WARNING …(p, q, r)` 1줄 |
| `SELECT 1 AS n, 'k' AS k`(테이블 없음) | 앞 2 | `T` + 경고 2줄(테이블 · 키) |

5종 전부 문장 생성 · INSERT는 경고 없음 · 셸 `copy sql:delete` 클립보드 확인. 단위 테스트 nsql-io `sqlgen::tests` 4(키 선택 · 복합 키 5종 · NULL 키 · 테이블 추정/분리) · nexa-sql `sql_copy_all_kinds_produce_statements`.
