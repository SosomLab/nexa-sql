# 83. 객체 탐색기 2차 — DBMS별 트리 · 유효성 표식 · Generate SQL · SQL Preview(2026-09-25 · 100차 mac)

> 사용자 09-25(캡처 5장 = DBeaver의 SQL Server · Oracle · PostgreSQL 항해자 · 프로시저 유효성 아이콘): "DBeaver 소스를 참고해 DBMS 특징별 객체 탐색 구조 · 정리되지 않은 DBMS는 일반 구조 · 목록을 정확하게 조회하는 것부터(조회 UI는 아직) · Valid 여부 = 아이콘 표식 · 우클릭 Generate SQL(테이블 = DML 유형별 · 프로시저 = CALL · 공통 = DDL — 테이블·패키지·프로시저·뷰·인덱스·제약 등등) · 선택 결과 = 모달 SQL Preview(미니맵 없는 편집 모드 · 새로고침/파일로 저장/편집기에서 열기/복사/닫기)". 원천 = [28](28-object-explorer.md) · [77 §1](77-data-workbench-architecture.md)(객체 척추 `ObjectRef → MetaStore → ObjectAction`) · [79](79-metadata-ownership-and-refresh.md) · [41](41-sql-copy-key-rules.md).

## 0. 결론 여섯 줄

1. **트리 = 선언(표) 하나** — `nsql-catalog::tree`: 스키마 아래 폴더 종류 `kinds_for(dialect)`(DBeaver 순서) · DB 수준 폴더 `db_kinds_for(dialect)`(PG Extensions·Event Triggers) · 객체 아래 하위 폴더 `sub_kinds(dialect, kind)`(Columns · Constraints · Foreign Keys · References · Indexes · Triggers · Partitions · Dependencies · Rules · Policies · Extended Properties · Arguments · Attributes · Methods · Procedures · Functions). 탐색기는 표를 읽어 노드를 만들 뿐 방언 분기를 갖지 않는다.
2. **조회 = 카탈로그 함수 셋** — `objects(s, schema, kind)`(종류 추가) · `columns` · ★ `sub_items(s, owner, sub) -> Vec<SubItem{name, detail, icon}>`(방언별 SQL · 없는 것은 빈 목록). 트리 노드 = `Sub{owner, sub}` 폴더 + `Item(SubItem)` 잎.
3. **DBeaver 대조**(plugin.xml `<tree>` · 09-25 원격 읽기) — Oracle: Tables·Views·Materialized Views·Indexes·Sequences·Queues·Types·Packages·Procedures·Functions·Synonyms·Schema Triggers·Table Triggers·Database Links·Java·Jobs·Scheduler(Jobs·Programs) / 테이블 = Columns·Constraints·Foreign Keys·References·Triggers·Indexes·Partitions·Dependencies. SQL Server: Databases→Schemas→ Tables·External Tables·Views·Indexes·Procedures·Sequences·Synonyms·Table Triggers·Data Types / 테이블 = Columns·Unique Keys·Check Constraints·Foreign Keys·Indexes·References·Triggers·Extended Properties. PostgreSQL: Databases→Schemas→ Tables·Foreign Tables·Views·Materialized Views·Indexes·Procedures(+Functions)·Sequences·Data Types·Aggregate Functions / DB = Event Triggers·Extensions·Storage·Roles / 테이블 = Columns·Constraints·Foreign Keys·Indexes·Dependencies·References·Partitions·Triggers·Rules·Policies. 우리 트리는 이 순서·이름을 따르고(§1) **Databases 층은 2차**(§5 D-201 — 지금은 접속한 DB 하나 = 루트 · SQL Server 3부 이름 · PG는 DB마다 접속이라 별도 결정).
4. **유효성 = 아이콘 배지** — `ObjectInfo.status`가 `INVALID`면 아이콘 오른쪽 아래에 빨간 점(⛔ 느낌 · 아이콘 지름의 40 %)을 그린다 · `VALID`/`INVALID` 글자는 행에서 뗀다(그 밖 상태 `DISABLED` 등은 흐린 글자 그대로). 메타 저장소 `ObjStatus`는 이미 있음(완성 후보에도 같은 뜻).
5. **Generate SQL = 우클릭 하위 메뉴** — 종류별: 테이블·뷰·MV = SELECT·INSERT·UPDATE·DELETE·MERGE(방언에 있으면)·DDL / 프로시저·함수 = CALL·DDL / 패키지 = CALL(멤버별 주석)·DDL / 인덱스·제약·트리거·시퀀스·시노님·타입·그 밖 = DDL. 생성 = `nsql-catalog::gen::generate(s, spec)`(컬럼·키·인자 = 카탈로그 · DDL = `source`/`DBMS_METADATA`/`pg_get_*`/sys 구성) → 메타 스레드 → `ExplorerAction::Preview`.
6. **SQL Preview = 모달 창** `sqlprev_win.rs`(입력 창 골격 · nexa-ctl `TextBox` 편집 모드 · SQL 하이라이트 · 미니맵 끔 · 버튼 = 새로고침(다시 생성) · 파일로 저장(`FilePurpose::SqlPreview`) · 편집기에서 열기(새 탭) · 복사 · 닫기(Esc)).

## 1. 트리 표(1차 구현 범위)

| 방언 | 스키마 아래(순서) | DB 수준(루트 아래) |
|---|---|---|
| Oracle | Tables · Views · Materialized Views · Indexes · Sequences · Queues · Types · Packages · Procedures · Functions · Synonyms · Schema Triggers · Table Triggers · Database Links · Java · Jobs · Scheduler Jobs · Scheduler Programs | — |
| SQL Server | Tables · External Tables · Views · Indexes · Procedures · Functions · Sequences · Synonyms · Table Triggers · Data Types | — (Database Triggers = 2차) |
| PostgreSQL | Tables · Foreign Tables · Views · Materialized Views · Indexes · Procedures · Functions · Aggregate Functions · Sequences · Data Types | Extensions · Event Triggers |
| MySQL(일반) | Tables · Views · Procedures · Functions · Triggers | — |
| SQLite(일반) | Tables · Views · Indexes · Triggers | — |
| ODBC/확장(일반) | Tables · Views | — |

Package Bodies 폴더는 뗀다(DBeaver와 같이 패키지 노드 아래 멤버 · 본문은 소스/DDL) — 종류 자체(`PackageBody`)는 소스·DDL·CLI `cat`에 남는다.

| 객체 | Oracle | SQL Server | PostgreSQL | 일반 |
|---|---|---|---|---|
| Table | Columns · Constraints · Foreign Keys · References · Triggers · Indexes · Partitions · Dependencies | Columns · Unique Keys · Check Constraints · Foreign Keys · Indexes · References · Triggers · Extended Properties | Columns · Constraints · Foreign Keys · Indexes · Dependencies · References · Partitions · Triggers · Rules · Policies | Columns · Constraints · Foreign Keys · Indexes · Triggers |
| View | Columns · Constraints · Triggers · Dependencies | Columns · Triggers · Extended Properties | Columns · Dependencies · Triggers · Rules | Columns |
| Materialized View | Columns · Constraints · Indexes · Dependencies | — | Columns · Indexes · Dependencies | — |
| External/Foreign Table | — | Columns | Columns · Constraints | — |
| Index | Columns | Columns | Columns | Columns(SQLite) |
| Type | Attributes · Methods | — | Attributes | — |
| Package | Procedures · Functions · Dependencies | — | — | — |
| Procedure · Function | Arguments · Dependencies | Arguments | Arguments | Arguments(MySQL) |

조회 SQL은 `nsql-catalog::sub_items`에 방언별로(전부 읽기 전용 사전 뷰 · 한 노드 = 한 질의 · 메타 세션 · 우선순위 = 트리 1). Dependencies는 Oracle `ALL_DEPENDENCIES`만 1차(PG `pg_depend`는 2차).

## 2. 유효성 표식

- 원천 = `ObjectInfo.status`(Oracle `ALL_OBJECTS.STATUS` · SQL Server·PG = 없음(빈) · 트리거 `ENABLED/DISABLED`는 부가 글자).
- 그리기 = 아이콘 위 배지(빨간 원 · 아이콘 오른쪽 아래 · 흰 테두리 1px) — 아이콘 캐시는 그대로(배지는 그 자리에서 `fill_circle`) · 아이콘 끔(`explorer.icons=off`)이면 색 칩을 빨강으로.
- 행 글자에서 `VALID`/`INVALID`를 뗀다(깔끔 · DBeaver 동일) · 툴팁·상세 카드에는 유지.

## 3. Generate SQL(2단계)

| 종류 | 항목 | 생성 원천 |
|---|---|---|
| Table · External/Foreign Table | SELECT · INSERT · UPDATE · DELETE · MERGE(Oracle·SQL Server·PG 15+ · SQLite = `ON CONFLICT`) · DDL | 컬럼 = `columns` · 키 = `table_detail.keys`(P→U→첫 열 · [41](41-sql-copy-key-rules.md) 규칙) · 바인드 = `:컬럼`(엔진이 방언으로 고쳐 씀 DR-8) · DDL = Oracle `DBMS_METADATA.GET_DDL('TABLE')` · SQL Server/PG = `table_ddl`(sys/pg 구성) · SQLite `sqlite_master.sql` |
| View · MView | SELECT · DDL | `source` |
| Procedure · Function | CALL · DDL | 인자 = `routine_args` → Oracle `EXEC s.p(:a, :b)` + OUT 주석 · SQL Server `EXEC s.p @a = :a` · PG `CALL s.p(:a)` / `SELECT * FROM s.f(:a)` · DDL = `source` |
| Package | CALL(멤버마다 한 줄 · 주석) · DDL(spec + body) | `package_members` · `source` |
| Index · Constraint(Item) | DDL | Oracle `GET_DDL('INDEX'/'CONSTRAINT'/'REF_CONSTRAINT')` · PG `pg_get_indexdef`/`ALTER TABLE … ADD CONSTRAINT … pg_get_constraintdef` · SQL Server = sys 구성 · SQLite `sqlite_master` |
| Trigger · Sequence · Synonym · Type · 그 밖 | DDL | `source` · Oracle `GET_DDL` · PG 구성 · SQL Server `OBJECT_DEFINITION` |

메뉴 id = `gen:<what>` · 요청 = `Req::GenSql{spec}`(메타 스레드 · 우선순위 1) · 응답 = `ExplorerAction::Preview{title, text, spec}` → 호스트가 SQL Preview를 연다(이미 열려 있으면 본문 교체 · "새로고침" = 같은 `spec` 재요청).

### 3-1. 생성 옵션(DBeaver "Settings" · 사용자 09-25 · 구현 ✅ §193)

| 옵션(설정 키) | 기본 | 뜻 |
|---|---|---|
| 정규화 이름 사용 `gen.qualified` | 켬 | `스키마.객체`(SQL Server = `DB.스키마.객체` · 확장 속성 `DB.sys.sp_addextendedproperty`) · 끄면 객체 이름만 |
| 간결한 SQL `gen.compact` | 끔 | 빈 줄·주석 줄 제거 · 들여쓰기 = 탭 하나(Oracle = `PRETTY false`) |
| 전체 DDL `gen.full_ddl` | 끔 | 인덱스까지 · Oracle은 저장 절(SEGMENT_ATTRIBUTES · STORAGE · TABLESPACE)까지 |
| 외래 키 분리 `gen.separate_fk` | 켬 | FK = `ALTER TABLE … ADD CONSTRAINT`로 따로(끄면 CREATE TABLE 안 인라인 · Oracle `REF_CONSTRAINTS`) |

"Format SQL"은 포매터가 아직 없어 뺐다(T-215). 미리보기 창 체크박스를 바꾸면 설정에 남고 즉시 재생성 · CLI = `gen … qualified=0 compact=1 full=1 fk=0`. 테이블 DDL 모양 = 사용자 샘플(헤더 주석 · `-- DROP TABLE` · COLLATE · NULL 명시 · 인라인 CONSTRAINT · 확장 속성/`COMMENT ON`).

## 4. SQL Preview 창(2단계)

모달(`App::sync_modal` 목록 · 입력 창과 같은 골격) · 제목 "SQL Preview — {객체} · {종류}" · 본문 = `TextBox` 다중 줄 · SQL 하이라이트 · 미니맵 끔 · 편집 가능 · 하단 버튼 왼→오 = 새로고침 · 파일로 저장… · 편집기에서 열기 · 복사 · 닫기. 키 = Esc 닫기 · ⌘/Ctrl+C = 선택/전체 복사(TextBox). 파일로 저장 = 파일 창 `PickerMode::Save` + `FilePurpose::SqlPreview(text)` · 기본 이름 = `{객체}_{what}.sql`. 편집기에서 열기 = 새 탭(제목 같음) + 창 닫기. 창 크기 = 720×480 기본 · `window.sqlprev_size` 기억(2차).

## 5. 결정 · 할 일

| # | 결정 | 권장 |
|---|---|---|
| **D-201** | Databases 층(SQL Server = `db.sys.*` 3부 이름으로 다른 DB 열람 · PG = DB마다 접속이라 "접속한 DB만 펼침 + 나머지 이름만") | 2차(T-204) · 1차는 접속 DB 하나 |
| **D-202** | Package Bodies 폴더 제거(DBeaver 동일) · 본문은 패키지 노드 소스/DDL | 제거 |
| **D-203** | Generate SQL 바인드 표기 = `:컬럼`(엔진 재작성 DR-8) vs 방언 원표기(`@c` · `$1`) | `:컬럼` |

| ID | 항목 | 크기 | 상태 |
|---|---|---|---|
| **T-203** | 1단계 — 트리 표 + `sub_items` + 새 종류 조회 + 유효성 배지 + 아이콘 8종 + i18n | 중~대 | ✅ §190 |
| **T-204** | Databases 층(D-201) · SQL Server Database Triggers · PG Storage/Roles | 중 | 대기 |
| **T-205** | 2단계 — Generate SQL(`gen.rs`) + SQL Preview 모달 + CLI `cat gen` | 중 | ✅ §190 |
| **T-206** | Dependencies PG(`pg_depend`) · Oracle Recycle Bin · 권한 폴더 | 소 | 대기 |

## 6. 자체 점검 방법(09-25 · 키 주입 0)

- 단위: `cargo test -p nsql-catalog`(tree·gen) · `cargo test -p nexa-sql explorer`(계층 새로 고침 = Sub 모델) · `exp_icons` 마스크 24.
- 실서버 CLI(GUI와 같은 함수): `nsql cat -c <프로필> <종류>`(새 종류 코드는 `nsql cat kinds`) · `nsql cat -c <프로필> sub <객체> <하위> [주인 종류]` · `nsql cat -c <프로필> gen <what> <객체> [종류] [하위 이름]`.
- GUI(격리 `NSQL_HOME` + 데모 SQLite · `NSQL_NO_ACTIVATE=1`): 기동 명령 `@after:1500:explorer.expand1,@after:2500:explorer.expand2,@after:3800:explorer.dump:<파일>,@after:4000:explorer.menu4,@after:4400:explorer.pick:gen:merge,@after:6000:sqlprev.dump:<파일>,…` → 트리 덤프(`깊이|종류|라벨|부가|상태`)와 미리보기 본문을 파일로 대조(§190 결과).
