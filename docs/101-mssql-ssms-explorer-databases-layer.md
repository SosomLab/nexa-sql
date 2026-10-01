# 101 · SQL Server 객체 탐색기 = SSMS 골격(Databases 층 · 현재 DB · 서버 개체/연결된 서버)

> **요청**(사용자 10-01 · 캡처 4장 = SSMS 개체 탐색기 · 우리 서버 헤더 · SSMS 서버 노드 · SSMS DB 노드): *"MSSQL은 보통 전체 데이터베이스 접근이 가능하므로 기본 데이터베이스는 기본 스키마 역할이되 다른 DB 정보도 모두 보이도록 · 기본 골격은 SSMS 형태 · 서버 정보는 SSMS 서버 노드와 유사하게 · `USE`를 써도 보여줄(연결되는) DB가 표시되지 않음 · Linked 서버 정보 확인·수정도 안 됨 · DB 한 개의 구조 = 데이터베이스 다이어그램 · 테이블 · 뷰 · 외부 리소스 · 동의어 · 프로그래밍 기능 · Service Broker · 스토리지 · 보안"*.
> 원천 = [83](83-object-explorer-dbms-trees-and-generate-sql.md)(D-201 · T-204 = "Databases 층 2차"가 바로 이것) · [28](28-object-explorer.md) · [54 §10](54-connection-model-and-disconnect.md)(카탈로그 칸) · [85](85-metadata-layers.md)(메타 3층) · [86](86-object-details-panel.md) · [96](96-object-links.md).
> **상태**: ✅ **1차 T-270 구현(10-01 오후 · [journal §15](journal/2026-09-30.md))** — §2 골격 · §3 현재 DB · 서버 헤더 · 설정 2키 · E2E 14/14. §4의 3부 이름 `_in` 대신 **메타 세션 `USE` 전환**(`switch_db`)으로 단순화(기존 카탈로그 함수 무변경). T-271 ✅(§17). 남음 = T-272.

## 1. 지금 구조와 문제

- 트리 = `Root(연결) → Schema → Folder(종류) → Object → Sub → Item`(DBeaver식). SQL Server는 **접속한 DB 하나**가 루트이고 모든 카탈로그 SQL이 `sys.*`(현재 DB)만 본다 — `db.sys.*` 3부 이름 · DB 매개변수 없음(조사 10-01).
- 서버 헤더 = `호스트:포트 · 연결 N개` · 버전·로그인 없음. 세션의 **현재 DB**(`USE` 뒤) 개념이 없다 — 탐색기·상태줄·`SHOW CONN` 어디에도 안 보인다.
- 연결된 서버(`sys.servers`)는 어디에도 없다.

## 2. 목표 골격(SSMS 대조 · SQL Server에만 적용 · Oracle/PG/MySQL/SQLite는 종전 트리)

```
[서버 헤더]  192.168.0.58:1433 (SQL Server 15.0.4375.4 - BISCM_MS)        연결 1개
 └ 연결 행    BISCM_MS · M4PLAN                             현재 DB: BISCM_MS
    ├ 📁 데이터베이스
    │   ├ 📁 시스템 데이터베이스        master · model · msdb · tempdb
    │   ├ 🛢 BISCM_MS  (현재)          ← 굵게 + 흐린 "(현재)" · 접속 직후 자동 펼침
    │   │   ├ 📁 테이블                 dbo.TB_A · sales.TB_B …(스키마 접두 · 스키마 층 없음)
    │   │   ├ 📁 뷰
    │   │   ├ 📁 외부 리소스 ▸ 외부 테이블
    │   │   ├ 📁 동의어
    │   │   ├ 📁 프로그래밍 기능 ▸ 저장 프로시저 · 함수 · 데이터베이스 트리거 · 형식 · 시퀀스
    │   │   ├ 📁 스토리지 ▸ 파일 그룹 · 파티션 구성표/함수            (2차)
    │   │   └ 📁 보안 ▸ 사용자 · 역할 · 스키마
    │   ├ 🛢 JPN_POC · LOTTE_SNOP · M4PLAN_MS · …
    ├ 📁 보안 ▸ 로그인 · 서버 역할                                  (2차)
    └ 📁 서버 개체 ▸ 연결된 서버 · (백업 장치 · 트리거 = 2차)
```

- **SSMS에 있지만 두지 않는 것(1차)**: 데이터베이스 다이어그램 · Service Broker · 데이터베이스 스냅샷 · PolyBase · Always On · 관리 · XEvent — 우리 앱에 뜻이 없거나 2차(D-247).
- 테이블 노드 아래(종전 `sub_kinds` 그대로) = 열 · 키 · 제약 조건 · 인덱스 · 트리거 · 확장 속성. **인덱스 폴더는 SSMS처럼 테이블 아래에만**(DB 직속 "Indexes" 폴더 제거).
- 객체 라벨 = `스키마.이름`(SSMS) · 필터·검색 인덱스는 이름과 스키마 둘 다 · Ctrl 링크·Generate SQL은 `DB.스키마.객체`(현재 DB면 `스키마.객체` · `gen.qualified`).

## 3. 현재 DB(세션 사실) — 표시·추적·기본 스키마 역할

- **정의**: 현재 DB = 그 **세션(`Sess`)** 의 `DB_NAME()`. 접속 직후 = 프로필 Database(비우면 로그인 기본 DB) · 편집기 `USE x` 성공 뒤 = x. 세션에 딸린 상태이므로 **`Sess.current_db`** 에 둔다(DR-34 · App이 아니라 Sess).
- **추적**: 러너가 성공한 항목의 첫 키워드가 `USE`면 **`RunEvent::DbChanged(이름)`**(문장에서 이름 추출 · 왕복 0) · 접속 직후는 워커 `Open` 응답에 `DB_NAME()` 1회. 서버 메시지("데이터베이스 컨텍스트가 … 변경되었습니다")는 그대로 Output에.
- **표시**: ① 탐색기 연결 행 = **로그인 계정**(⑬ · 로그인과 DB는 별개) + 흐린 글 `현재 DB: X` ② DB 노드 `(현재)` ③ **툴바 작업 단위 `sess.db`**(⑫ · 값만 · 드롭다운 = `USE`) ④ `SHOW CONN` "현재 DB" 줄 ⑤ 접속 정보 창.
- **기본 스키마 역할**: 완성·Ctrl 링크·둘러보기·L1 인덱스·L2 워머는 **현재 DB**를 본다. 세션의 현재 DB가 바뀌면 그 연결 칸의 **메타 세션도 `USE`로 따라가고**(`Req::UseDb`) MetaStore를 비우고(`reset`) L1부터 다시 — 기존 "스키마 바뀜 = 메타 갱신"과 같은 길(85 §L1·§L2). 다른 DB의 객체는 **트리에서만**(3부 이름 SQL) 읽고 MetaStore에는 넣지 않는다(1차 · D-249 · 완성 `DB.스키마.` 접두는 2차 T-272).
- 공유 연결 하나에 탭 여러 개 = 현재 DB도 공유(서버 사실). 전용 세션(`CONNECT`)은 각자.

## 4. 데이터 모델·요청 경로(변경 지점)

| 자리 | 변경 |
|---|---|
| `explorer.rs` `NodeKind` | `+ Database(String)` · `+ Group{db: Option<String>, group: GroupKind}`(Databases · SystemDbs · ExternalResources · Programmability · Storage · Security · ServerSecurity · ServerObjects) · `Folder{schema, kind}` → `Folder{db: Option<String>, schema, kind}`(schema `""` = 모든 스키마) · `Object(ObjectInfo)`의 DB = 조상 Database 노드(`db_of(i)` · `schema_of`와 같은 걷기) |
| `node_key` | `d:{db}` · `g:{db}:{group:?}` · `f:{db}:{schema}:{kind:?}` · `o:{db}:…` — DB가 달라도 `dbo`가 충돌하지 않게 |
| `Req`/`Resp` | `Databases{gen,node}` · `Objects{…, db: Option<String>}` · `Columns/SubItems/Source{…, db}` · `UseDb{db}` · `Opened`에 `(dialect, desc, version, login, current_db)` · `LinkedServers{gen,node}` · `LinkedDetail{name}` |
| `nsql-catalog` | **`Scope { db: Option<String> }`** 를 받는 `_in` 짝(`objects_in` · `columns_in` · `source_in` · `sub_items_in` · `name_index_in`) — 기존 함수는 `db: None`으로 위임(다른 방언 무영향). SQL Server SQL은 `{p}sys.objects`(`p` = `[db].` 또는 빈 문자열) · `OBJECT_ID('[db].[s].[n]')`(3부 지원) · `OBJECT_DEFINITION`은 현재 DB 전용이라 **`[db].sys.sql_modules`** 로 교체 · `SCHEMA_NAME()`·`DB_NAME()`은 현재 DB 질의에만 |
| `nsql-catalog` 새 조회 | `databases(s)` = `sys.databases`(name · database_id · state_desc · is_system = `database_id <= 4` · owner · recovery · compatibility) · `server_info(s)` = `SERVERPROPERTY('ProductVersion'/'Edition'/'MachineName')` + `SUSER_SNAME()` + `DB_NAME()` · `linked_servers(s)` = `sys.servers WHERE is_linked = 1` · `linked_server_detail(s, name)` = sys.servers 열 전부 + `sys.linked_logins`(+ `server_principals`) · `db_users/db_roles/db_schemas(s, db)` · `filegroups/partitions`(2차) |
| `ObjectKind` | `+ LinkedServer` · `+ DbUser` · `+ DbRole`(1차는 목록·상세만) |
| `nsql-run` | `RunEvent::DbChanged(String)` · `Session`에 `current_database()`는 두지 않는다(러너 파싱 + Open 1회로 충분 · 포트 최소) |
| `Sess` | `current_db: Option<String>` · `ExplorerSet::set_current_db(spec, db)` → 칸의 표시 갱신 + `Req::UseDb` + 메타 리셋 |
| 헤더 | `endpoint` 뒤 `(SQL Server {version} - {login})`(로그인이 여럿 = `- 로그인 N`) · i18n `ExpServerSql` |
| 상세 패널(86) | Database 노드 = 상태 · 소유자 · 복구 모델 · 호환성 · 생성일 · 용량(2차) · 연결된 서버 노드 = 제품 · 공급자 · 데이터 원본 · 카탈로그 · 옵션(rpc/rpc_out/data access/collation/…) · 로그인 매핑 표 |

## 5. 연결된 서버(Linked Servers) — 확인·수정·삭제

- **목록**: 서버 개체 ▸ 연결된 서버 ▸ `[이름]`(제품 흐림 글). 펼침 = **카탈로그**(`EXEC sp_catalogs @server` · 실패 = 빈 폴더 + ⚠ · 2차 = 카탈로그 ▸ 테이블 4부 이름).
- **확인**: 선택 = 상세 패널(§4 표) · 우클릭 ▸ 속성 → Output.
- **수정** = 다른 객체와 같은 흐름(소스 열기 → 고쳐서 F5): 우클릭 ▸ **Open script** = 새 탭에 재생성 스크립트
  ```sql
  EXEC master.dbo.sp_dropserver @server = N'LNK', @droplogins = 'droplogins';
  EXEC master.dbo.sp_addlinkedserver @server = N'LNK', @srvproduct = N'…', @provider = N'SQLNCLI', @datasrc = N'…', @catalog = N'…';
  EXEC master.dbo.sp_addlinkedsrvlogin @rmtsrvname = N'LNK', @useself = N'False', @locallogin = NULL, @rmtuser = N'…', @rmtpassword = N'********';
  EXEC master.dbo.sp_serveroption @server = N'LNK', @optname = N'rpc out', @optvalue = N'true';  -- 옵션마다 한 줄
  ```
  비밀번호는 서버가 주지 않으므로 `********` 자리표시 + 주석 안내(보수적 생성 원칙 100 §5 = 그대로 실행하면 결과가 달라질 수 있는 자리는 명시). Generate SQL ▸ DDL = 같은 본문(DROP 줄 없이).
- **삭제** = 28 §9 객체 삭제 모달 그대로(백업 = 위 스크립트 파일 · `sp_dropserver … 'droplogins'` · 운영 = 백업 실패 시 금지).
- **새로 만들기** = Open script 틀(빈 값)로 — 대화상자 없음(1차).

## 6. 결정(권장안으로 진행 · 사용자 확인 대기)

| # | 결정 | 권장 | 비고 |
|---|---|---|---|
| **D-246** | SQL Server 트리 골격 = SSMS(스키마 층 없음 · `스키마.이름` 라벨 · 프로그래밍 기능 묶음) vs DBeaver(DB → 스키마 → 종류) | **SSMS**(사용자 명시) · 설정 `explorer.mssql_tree = ssms|schema`로 DBeaver식 복귀 가능 | Oracle/PG는 종전 |
| **D-247** | SSMS 폴더 중 생략 = 다이어그램 · Service Broker · 스냅샷 · PolyBase · Always On · 관리 · XEvent | **생략**(빈 폴더를 두지 않는다) | 2차 후보 = 스냅샷 |
| **D-248** | 현재 DB 추적 = 러너 `USE` 파싱(왕복 0) + 접속 1회 `DB_NAME()` | **그 방식** | `DB_NAME()` 매 실행 재질의는 부하 |
| **D-249** | 다른 DB 객체의 완성(MetaStore) | **1차 = 현재 DB만** · 다른 DB = 트리 열람만 · 3부 접두 완성 2차 | 메타 열쇠 `(db, 스키마)` 확장은 2차 |
| **D-250** | 현재 DB가 바뀌면 메타 = 비우고 다시(L1 → L2) | **그 방식**(= 스키마 바뀜과 같은 길) | 캐시 L1 디스크 키에 DB 포함 |
| **D-251** | 시스템 DB(master · model · msdb · tempdb) = 하위 폴더로 분리(SSMS) | **분리** · `explorer.mssql_system_dbs` 숨김 가능 | |
| **D-252** | 연결된 서버 수정 = Open script(재생성 스크립트) · 대화상자 없음 | **스크립트** | 비밀번호 자리표시 |
| **D-253** | 서버 헤더 = `호스트:포트 (SQL Server 버전 - 로그인)` · 다른 방언도 같은 틀(`Oracle 19c - BISCM` · `PostgreSQL 16 - postgres`) | **전 방언**(버전 질의 1회 · `Resp::Opened`) | 2차는 Oracle/PG 버전 |

## 7. 할 일

| ID | 항목 | 크기 | 상태 |
|---|---|---|---|
| **T-270** | 1차 — Databases 층 + SSMS 골격(§2) · 현재 DB 추적·표시·메타 추종(§3) · 메타 세션 `USE` 전환(§4 단순화) · 서버 헤더 버전·로그인 · i18n · 순수 판정 시험 · 실서버 E2E `win-mssql-explorer-e2e.sh` 14/14 | 대 | ✅ 10-01(상세 패널 DB 절 = T-272) |
| **T-271** | 연결된 서버(§5) — 목록 · 상세(속성 + 로그인 매핑) · Open source = 재생성 스크립트 · Generate SQL ▸ DDL · 삭제 모달(`sp_dropserver droplogins`) · CLI `cat source/detail linked_server` | 중 | ✅ 10-01([journal §17](journal/2026-09-30.md) · 실서버 권한 없어 E2E는 폴더 읽기만 · 카탈로그 펼침 = T-272) |
| **T-272** | 2차 — 3부 접두 완성(`DB.스키마.`) · 스토리지(파일 그룹·파티션) · 서버 보안(로그인·역할) · 연결된 서버 카탈로그 ▸ 테이블 · 스냅샷 · DB 용량 | 중 | 대기 |
