# 119a 부록 — 테이블 구조 편집기 조사·자산 대조(협업 · 2026-10-21)

> 개발이 [119](119-table-editor-design.md) §2·§3-1에 합친다. §2 = 도구 7종 조사(공식 문서 우선 · 근거 번호 §D) · §3-1 = 이 저장소 자산 대조(코드 탐색). 표기 = `[Dn]` 공식 URL · `기억 · 확인 필요` = 웹 확인 못 함 · ⚠ = 추정.

## §2 초안 — 도구별 비교(§A)

### A-1. 진입 경로 · 편집 단위

| 도구 | 진입 경로 | 편집 단위 |
|---|---|---|
| **DataGrip** (Modify Table) | 탐색기에서 테이블 우클릭 ▸ **Modify Table** 또는 Ctrl+F6 · 2024.3부터 **모달 아님** [D1] | 컬럼 · 키(PK) · FK · 인덱스 · Check · 가상 컬럼/가상 FK(외부 XML) · 객체마다 Comment [D1] · 트리거·파티션 = 문서에 없음(⚠ 미지원 또는 별도) |
| **DBeaver** (Properties 편집기) | 테이블 더블클릭 → 객체 편집기 **Properties** 탭 [D2][D3] | 탭 = DB마다 다름(Columns · Constraints · Foreign Keys · Indexes · Triggers · Partitions · DDL · Source 등) [D2] · 컬럼 NULL·Unique·Check·기본값 [D3] |
| **Toad for Oracle** | Schema Browser에서 객체 **더블클릭** = Alter 창 [D4] · 데이터 보존 재구성 = **Rebuild Table** 마법사 [D5] | 컬럼 · 제약 · 인덱스(Alter Index Column 탭은 Oracle 제약상 변경 불가) [D4] · 코멘트 [D6(제3자)] · 스토리지·파티션 = 기억 · 확인 필요 |
| **SSMS** (Table Designer) | Tables 아래 테이블 우클릭 ▸ **Design** [D7] | 컬럼(이름·타입·Allow Nulls·속성 창) · 키 · 인덱스 · 관계(FK) · 제약 [D7] · 파일그룹/text·image 위치 [D8] |
| **TablePlus** (Structure 탭) | 데이터 보기 아래 **Structure** 탭(`⌘⌃]`) 또는 사이드바 우클릭 ▸ Open structure [D9] | 컬럼(이름·타입·NULL·기본값·Primary) · 테이블 이름 · **Definition** 버튼 = CREATE 문 보기 [D9] · 인덱스·FK·코멘트 = 기억 · 확인 필요(⚠ 같은 화면 아래 칸) |
| **pgAdmin 4** (Table 대화상자) | 트리 테이블 우클릭 ▸ Properties(새로 = Create ▸ Table) ⚠ 메뉴 이름 기억 [D10] | 탭 General(이름·소유자·스키마·테이블스페이스·코멘트·파티션 여부) · Columns(상속 포함) · Constraints(PK·FK·Check·Unique·Exclude) · Advanced(RLS·fillfactor·TOAST·unlogged·LIKE) · Partition(Range/List/Hash · attach) · Parameter(autovacuum) · Security(권한·라벨) · SQL [D10] |
| **Oracle SQL Developer** (Edit Table) | 테이블 우클릭 ▸ **Edit** [D11][D12] | (Web 판 문서 기준) Columns(NOT NULL·Check) · Primary Key · Unique Keys · Indexes · Foreign Keys · Table Constraints · Comments · Storage · External Table · MView · DDL · Output [D11] · 데스크톱 = Edit 모드에서 테이블 종류 변경·파티션 생성 불가 · 컬럼 순서 변경 불가 [D13(튜토리얼)] |

### A-2. 적용 방식 · 데이터 보존 규칙

| 도구 | 적용 방식(즉시/묶음 · DDL 미리보기 · 트랜잭션/되돌리기) | 데이터 보존 규칙(ALTER로 안 되는 변경) |
|---|---|---|
| **DataGrip** | **묶음** — 아래 미리보기 칸 = OK 때 실행할 SQL · **미리보기 SQL 직접 수정 가능** · "Open query in console" = 대화상자 닫고 콘솔로 · Settings = 실행 설정 [D1] · 트랜잭션·undo = 문서 없음 | 문서 없음(기억 · 확인 필요 · ⚠ 방언별 ALTER만 생성 · 재생성 마법사 없음) |
| **DBeaver** | **묶음** — 편집은 DBeaver 안에만 있다가 **Save(Ctrl+S) → Persist Changes 창**에서 반영 · "변경마다 저장 권장" · **Revert** = 마지막 저장 상태로 [D2][D3] · Persist 창의 스크립트 미리보기/복사 = 기억 · 확인 필요 | 기억 · 확인 필요(⚠ 방언 ALTER 생성 · 불가 변경은 서버 오류로 실패) |
| **Toad for Oracle** | **묶음** — Show SQL 버튼/SQL 탭에서 DDL 확인 → 실행 [D4] | **Rebuild Table 마법사** = 컬럼 삭제·이름 변경 포함 재구성 · 생성 ALTER/재구성 스크립트 검토 뒤 실행 [D5] · 비유일 인덱스는 데이터 복사 **뒤**에 삭제(실패 시 원복 쉽게) [D14(포럼·개발자 답)] · Compare Schemas 동기화 = "ALTER로 안 되면 안 함" [D15(포럼)] |
| **SSMS** | **묶음** — 닫기/저장 때 반영 · **Generate Change Script**(미저장 변경 있을 때) · 옵션 "Auto generate change scripts" · 트랜잭션 타임아웃·연결 타임아웃 재정의 · "Warn about tables affected"(영향 테이블 확인) · "Warn about difference detection"(다른 사용자 변경 충돌) · "Warn on null primary keys" [D8][D16] · 변경 스크립트엔 오류 처리 없음(구 문서) [D17] | ★ 옵션 **"Prevent saving changes that require table re-creation"**(기본 켬) — 재생성 필요 = 중간 컬럼 추가 · 컬럼 삭제 · NULL 허용 변경 · 순서 변경 · 타입 변경 [D16] · 끄면 경고 없이 **drop → 재생성**(임시 테이블 복사 ⚠ 기억) · 메타데이터·데이터 손실 위험 · **Change Tracking 정보 삭제** → MS 권고 = 끄지 말고 `ALTER TABLE` [D8] |
| **TablePlus** | **묶음** — 변경은 로컬 보류 → **⌘S**로 서버 반영 [D9] · "Code Review"(SQL 미리보기) · Discard = 기억 · 확인 필요 | 기억 · 확인 필요 |
| **pgAdmin 4** | **묶음** — 편집 중 저장 없음 · **SQL 탭 = 생성 SQL 미리보기(읽기 전용 · 고치려면 다른 탭으로)** · Save / Close(저장 없이) / Reset [D10] · FK Validated 켬 = 저장 때 기존 데이터 검증 [D10] | 상속 컬럼은 부모에서만 변경 [D10] · 타입 변경 = `ALTER COLUMN TYPE`(USING 여부 기억 · 확인 필요) |
| **Oracle SQL Developer** | **묶음** — DDL 칸 = 속성 바꿀 때마다 채워짐 · 기존 테이블 = **UPDATE** 탭에 ALTER · 새 테이블 = CREATE 탭 · DDL 칸은 **직접 수정 불가**(원래 칸으로) · **Apply** → **Output** 칸에 결과·오류 → 칸 고쳐 재실행 [D11][D13] | 컬럼 재정렬 = 드롭·재생성 필요(도구가 하지 않음) [D13] · 타입 변경 처리 = 문서 없음 [D11] |

### A-3. 다중 DBMS 폭 · 특징적 UX

| 도구 | 다중 DBMS 지원 폭 | 특징적 UX |
|---|---|---|
| **DataGrip** | 넓음(JetBrains 지원 DBMS 전부 · 기억 · 확인 필요) | 왼쪽 위 **구조 트리**(객체 추가/삭제) + 오른쪽 위 **속성 편집기** + 아래 **편집 가능한 SQL 미리보기** 3분할 · 비모달 [D1] |
| **DBeaver** | 매우 넓음(탭 구성이 DB마다 다름) [D2] | 객체 편집기 탭(Properties/Data/ER…) 안 하위 탭 · 아래 도구줄(검색·필터·열 설정·새로 고침·View·Create·Delete·Save·Revert) [D2] · DDL/Source 탭에서 스크립트 보기·수정 [D2] |
| **Toad for Oracle** | Oracle 전용 | Schema Browser 중심 · Alter 창 탭 + Show SQL · 옵션 이름 = Oracle 매개변수 그대로(Oracle 문서가 참조) [D4] · 별도 Rebuild Table 마법사 [D5] |
| **SSMS** | SQL Server 계열(Azure SQL 등 · Synapse Dedicated pool은 디자이너 없음) [D16] | **격자형 컬럼 편집**(이름·타입·Allow Nulls) + 아래 **Column Properties 창** · 키/인덱스/관계는 별도 대화상자(⚠ 기억) · 경고 대화상자 다수 [D7][D16] |
| **TablePlus** | 넓음(MySQL·PG·SQLite·MSSQL·Oracle 등 · 기억 · 확인 필요) | **스프레드시트식 인라인 행 편집**(+ Column · 빈 행 더블클릭) · 데이터 탭과 같은 화면 아래 탭 전환 · Definition 버튼 [D9] · 전 화면 공통 "변경 보류 → ⌘S" |
| **pgAdmin 4** | PostgreSQL 전용(EDB 포함) | 탭형 모달 대화상자 · 하위 객체(제약)마다 **하위 대화상자**(General/Definition/Columns/Action) · 버전별 옵션 노출(PG 10+ 파티션 등) · 마지막 SQL 탭 [D10] |
| **Oracle SQL Developer** | Oracle 중심(타 DB는 마이그레이션·조회 위주 · 기억 · 확인 필요) | 왼쪽 **분류 목록** + 오른쪽 값 · 칸을 순서 없이 방문 · DDL·Output 칸이 대화상자 안 [D11][D18] · 간이 생성 ↔ 고급(Advanced) 전환 = 기억 · 확인 필요 |

## §B 공통 패턴

- **7종 전부 "묶음 변경 → 한 번에 적용"** — 칸마다 즉시 DDL 실행하는 도구 없음. 저장/OK/Apply/⌘S/Persist 한 동작이 경계.
- **생성 DDL 미리보기가 표준** — SQL 탭(pgAdmin)·DDL/UPDATE 칸(SQL Developer)·Show SQL(Toad)·Generate Change Script(SSMS)·미리보기 칸(DataGrip). 미리보기를 **직접 고치게** 하는 것은 DataGrip뿐 · pgAdmin·SQL Developer는 읽기 전용(고치려면 폼으로) = 폼이 단일 원천.
- **미리보기 → 콘솔/편집기로 넘기기** 출구(DataGrip "Open query in console" · SSMS 스크립트 저장 · SQL Developer DDL 저장) — 직접 실행 대신 사용자가 스크립트로 검토·실행.
- **재생성(drop+copy+create) 위험을 명시적으로 막는 것은 SSMS 하나(기본 차단 옵션)** · Toad는 재생성을 **별도 마법사(Rebuild Table)** 로 분리 · 나머지는 ALTER만 생성하고 불가한 변경은 지원하지 않거나 서버 오류에 맡김(⚠).
- **결과·오류를 대화상자 안에 보여 주고 다시 고쳐 실행**(SQL Developer Output 칸) — 일부 문장만 성공한 상태를 사용자가 보게 함. 트랜잭션 묶음·자동 롤백을 문서로 보장하는 도구는 확인 못 함(Oracle·MySQL DDL은 암묵 커밋이라 ⚠ 원천적으로 어려움).
- **DB별 탭 구성 가변**(DBeaver·pgAdmin 버전별 옵션) — 공통 골격(컬럼·제약·인덱스·코멘트·DDL) + 방언별 추가 탭.
- **되돌리기 = 미저장 상태 버리기 수준**(DBeaver Revert · pgAdmin Reset/Close) — 서버 반영 뒤 undo는 없음.
- **안전 경고 대화상자**(SSMS: 영향 테이블 목록 · 동시 변경 충돌 · NULL PK) · 대형 테이블용 타임아웃 재정의.

## §C 확인 못 한 항목

- DataGrip: 트리거·파티션 편집 지원 · 타입 변경 시 생성 SQL(USING 등) · 트랜잭션 묶음 여부 · 지원 DBMS 목록.
- DBeaver: Persist Changes 창에 스크립트 미리보기/복사 버튼이 있는지(기억상 있음) · 불가 변경 처리.
- Toad: Alter Table 창의 정확한 탭 목록(공식 페이지 본문을 못 읽음 · 검색 요약만) · Rebuild Table이 데이터를 어떻게 복사하는지 공식 확인.
- SSMS: 재생성 시 내부 절차(임시 테이블 `Tmp_` 생성 → INSERT → drop → rename)는 기억 · 키/인덱스 대화상자 구성.
- TablePlus: 인덱스·FK·코멘트 편집 · Code Review(SQL 미리보기)·Discard · 지원 DBMS(공식 Structure 페이지 404).
- pgAdmin: 진입 메뉴 이름 · 기존 테이블 편집 때 SQL 탭이 ALTER 차분만 보이는지 · 타입 변경 USING.
- SQL Developer **데스크톱**: 공식 사용자 가이드 대화상자 절(404) — 위 칸 목록은 **SQL Developer Web** 문서와 3.0/4.0 튜토리얼로 대신함 · Advanced 체크박스.

## §D 근거 URL

- D1 https://www.jetbrains.com/help/datagrip/modify-table-dialog.html
- D2 https://dbeaver.com/docs/dbeaver/Properties-Editor/
- D3 https://dbeaver.com/docs/dbeaver/Creating-columns/
- D4 https://support.quest.com/technical-documents/toad-for-oracle/2025%20r2/getting-started-guide/23 (본문 직접 읽기 실패 · 검색 요약 근거)
- D5 같은 가이드 Rebuild Table 절(검색 요약) · https://support.quest.com/technical-documents/doc1685059
- D6 https://dataedo.com/kb/tools/toad-oracle/how-to-view-and-edit-table-and-column-comments (제3자)
- D7 https://learn.microsoft.com/en-us/ssms/visual-db-tools/design-tables-visual-database-tools
- D8 https://learn.microsoft.com/en-us/troubleshoot/sql/ssms/error-when-you-save-table
- D9 https://docs.tableplus.com/gui-tools/working-with-table/table.md
- D10 https://www.pgadmin.org/docs/pgadmin4/latest/table_dialog.html
- D11 https://docs.oracle.com/en/database/oracle/sql-developer-web/sdwad/editing-tables.html (SQL Developer **Web**)
- D12 https://docs.oracle.com/en/database/oracle/sql-developer-web/19.4/sdweb/table-properties-dialog.html
- D13 https://www.oracle.com/webfolder/technetwork/tutorials/obe/db/sqldev/r40/sqldev4.0_GS/sqldev4.0_GS.html (튜토리얼 · 검색 요약)
- D14 https://forums.toadworld.com/t/rebuild-table-drop-index-part-before-and-after-drop-why/39881
- D15 https://forums.toadworld.com/t/compare-schemas-question-altering-table-instead-of-dropping-and-recreating/10422
- D16 https://learn.microsoft.com/en-us/ssms/menu-help/options-designers-table-and-database-designers-page
- D17 https://learn.microsoft.com/en-us/previous-versions/efhhffxd(v=vs.140)
- D18 https://download.oracle.com/otn_hosted_doc/jdeveloper/1012/db/bdb_p_edit_table.html

## §3-1 초안 — 현재 nexa-sql 자산 대조(협업 · 코드 탐색 · 10-21)

결론 = 지금 있는 것은 **읽기**(메타 수집 · CREATE DDL 생성)와 **안전장치**(백업 · 미리보기 · 비교 · 탐색기 새로 고침 · 트랜잭션 판정)다. 구조 편집의 핵심인 **"현재 정의 → 목표 정의" 차이를 ALTER로 바꾸는 엔진은 없다.** 경로 = `crates/` 기준(nexa-ctl만 `../nexa-ui/crates/nexa-ctl/src/`).

| 자산 | 위치(파일 · 핵심 이름) | 재사용 판단 한 줄 | 모자란 점 |
|---|---|---|---|
| Generate SQL 7종(83) | `nsql-catalog/src/gen.rs` · `GenWhat`(Select/Insert/Update/Delete/Merge/Call/Ddl) · `generate` · `object_ddl` · `oracle_get_ddl` · `mssql_table_ddl`/`oracle_table_ddl`/`pg_table_ddl` · `table_with_keys` · `constraint_line` · `mssql_index_ddl` · `sub_ddl` · `ident`/`qual`/`sql_str` | **CREATE 쪽 조각 생성기로 재사용** — 이미 `ALTER TABLE … ADD CONSTRAINT`(FK 분리) · `CREATE INDEX` · `COMMENT ON` · `sp_addextendedproperty`를 만든다 · "새 테이블 만들기" 미리보기와 제약·인덱스 단건 DDL에 바로 | 결과가 완성 문자열(구조화 모델 없음) · 입력 = 늘 라이브 세션의 현재 객체 · **변경문 없음**(ALTER COLUMN · DROP CONSTRAINT/INDEX · RENAME · 컬럼 추가/삭제 · 코멘트 변경/삭제) · 헬퍼 대부분 비공개 `fn` |
| 하위 종류 트리 | `nsql-catalog/src/tree.rs` · `SubKind`(18) · `sub_kinds(dialect, kind)` · `sub_items` · `SubItem` · `index_columns` · `attributes` | **편집기 탭 구성(DBMS별 어떤 탭)의 근거로 그대로** — DBeaver 대조표 기준 · Oracle/MSSQL/PG/그 밖 분기 | `SubItem` = 표시용(이름·detail·아이콘) · 편집 속성 없음 · MySQL·SQLite는 공통 분기 `(_, Table)` |
| 테이블 상세·코멘트 읽기(85·86) | `nsql-catalog/src/lib.rs` · `columns()`→`ColumnInfo{name,data_type,nullable,position,default}` · `keys()` · `table_detail()`→`TableDetail{keys:Vec<KeyDef>,indexes:Vec<IndexDef>,comment,col_comments}` · `KeyDef{name,kind P/U/R/C,cols,ref_table,ref_cols}` · `IndexDef{name,unique,cols}` · `comments_raw` · `schema_comments_raw` · `comment_sqls` · `fmt_type` · `detail.rs` `object_details`/`column_details` | **편집기 "원본(before)" 모델의 뼈대** — 5방언 컬럼·키·인덱스·코멘트 질의가 이미 있다 | `data_type` = 표시 문자열 하나(길이·정밀도·스케일 미분리) · identity/auto-increment · collation · generated · CHECK 식 · FK ON DELETE/UPDATE·deferrable · 인덱스 방식·정렬·INCLUDE·조건 · 테이블스페이스 · 파티션 없음 · SQLite 코멘트 없음 |
| 그리드 데이터 편집(87) | nexa-ctl `gridedit/changeset.rs` `ChangeSet`(set_cell/insert_row/toggle_delete/undo/redo/group) · `RowRef`/`RowStatus` · `gridedit/spec.rs` `CellSpec`/`CellKind` · `nexa-sql/src/gridedit_sql.rs` SqlGen 어댑터(`ColMeta`·`Sink`·`literal`) · `nsql-run` `Runner::apply_changes(&[ApplyStmt])→ApplyReport`(COUNT 사전 검사 → 영향 행 = 1 → `savepoint_sql` 되돌림 · `ApplyLogItem`) | **UI(ChangeSet + 편집 그리드 + undo)는 "컬럼 목록 편집 표"에 그대로** · `apply_changes`의 문장 단위 실행 · `ApplyError`(단계·index) 보고 · 트랜잭션 로그 흐름도 | 3겹 보호가 **DML 전제**(영향 행 = 1 · COUNT 가드) · DDL은 영향 행 개념 없음 · Oracle/MySQL DDL = 암묵 커밋이라 세이브포인트 무효 · ChangeSet 값 = `Option<String>` 셀(컬럼 정의 의미 모름) |
| 소스 열기 · DDL 생성(100) | 100 §1·§5 보수적 생성 원칙 · `gen.rs` `object_ddl`/`oracle_get_ddl`(`DBMS_METADATA.GET_DDL` + 폴백) · PG `pg_get_*` · MSSQL `OBJECT_DEFINITION`+sys · SQLite `sqlite_master.sql` · `lib.rs` `source`/`source_with`/`SourceOpts` · E2E `scripts/dbms-source-e2e.sh` | **"현재 DDL 보기" + 변경 후 비교의 기준선으로 그대로** | 텍스트뿐(파싱해 모델로 되돌리지 않음) · MySQL 실서버 왕복 검증 남음(T-268) |
| 변경 전 객체 백업(114 §7-2 · T-357) | `nexa-sql/src/app/objbackup.rs` `BkAction{Drop,Replace,Alter,Rename,Truncate}` · `backup_targets` · `App::objbk_plan`/`objbk_take`/`objbk_finish` · `looks_missing` · 진입 = `app/run.rs` `run_text_in` 게이트(`objbk_skip_once` · `prod_confirmed_once`) · 설정 `backup.object_*` | **그대로** — 편집기가 만든 ALTER/DROP을 같은 실행 경로로 보내면 `ddl_target`으로 대상을 뽑아 현재 DDL을 파일로 백업 → 실행 · 운영 접속 백업 실패 = 막음 | 실행 경로가 편집기 텍스트 기반 → 구조 편집기가 **별도 경로로 실행하면 게이트를 안 거친다**(같은 경로로 이어야 함) |
| 비교(19) | `nexa-sql/src/app/compare.rs` `compare_target`·`normalize_ddl`·`canon_line`·`normalize_pairs`·`diff_count`·`obj_compare_stmt` · `diff_view.rs` 2칸 뷰어 · nexa-ctl `diff.rs` `align`/`Hunk`/`Row` | **"적용 전/후 DDL 텍스트 비교" 화면으로 바로**(생성 DDL vs 서버 DDL · 정규화 포함) | 줄 단위 텍스트 diff(구조 의미 diff 아님) → ALTER 생성에는 못 씀 |
| SQL Preview 모달(83 §4) | `nexa-sql/src/sqlprev_win.rs` `SqlPrevWin`(`open`/`open_plain`/`open_value`/`set_result`/`set_note`) · `SqlPrevAction` · 버튼 = 새로 고침·파일로 저장·편집기에서 열기·복사·닫기 | **"생성될 ALTER 스크립트 확인 → 편집기에서 열기/복사/저장" 단계에 그대로**(`open_plain` = 임의 텍스트) | "실행(적용)" 버튼·경고 칸(데이터 손실·암묵 커밋) 없음 · 새로 고침이 `GenSpec`에 묶임 |
| DDL 대상 파서(57) | `nsql-core/src/ddl.rs` `ddl_target(sql, dialect)→Option<DdlTarget>` · `DdlVerb{Create,Drop,Alter,Rename,Comment}.changes_list()` · `DdlKind` · 소비 = `app/meta.rs` `meta_on_done`/`meta_flush` · `objbackup.rs` · `compare.rs` · `tabobj.rs` | **그대로** — 적용 뒤 탐색기 그 폴더만 새로 고침 · 백업 대상 판정 | 머리말만 읽는 얕은 파서 · `sp_rename`·동적 SQL = None |
| 능력표·방언 판정(61 §1-6) | `nsql-core/src/caps.rs` `Caps`(`tx_begin`·`server_autocommit`·`ident_fold`/`fold_ident`·`marker` …) · `nsql-core/src/lib.rs` 트랜잭션 DDL 판정(PG·MSSQL·SQLite = 롤백 가능 · Oracle·MySQL = 암묵 커밋) · `implicit_commit(sql)` | **"원자적으로 적용 가능한가" 판정과 경고에 그대로** | 구조 편집 능력 칸 없음(ALTER COLUMN 문법 · 온라인 DDL · SQLite 재생성 필요 · 컬럼 RENAME 지원 버전) |
| DROP 문 · 삭제 창(28 §9) | `nsql-catalog` `drop_sql(dialect, &ObjectInfo)` · `app/drop.rs` · `drop_win.rs`(백업 Preview 가로채기) | 인덱스·제약 삭제 문 생성 + 2단 확인 창 패턴으로 | 객체 단위만(컬럼 삭제 없음) |
| 설정·i18n·확장 | `nsql-i18n` `Msg`/`t`/`tf` · `nsql-settings` · `extensions/mod.rs` `Registry`/`Extension`(in-process + WASM) · 22 §0·§2(드라이버 = in-process cdylib · stdio JSON-RPC 프로세스는 격리 선택) | 문구·설정 키는 기존 방식 · 명령·메뉴 기여 = 확장 레지스트리 | 22는 **드라이버 전송** 격리라 편집기 기능 자체의 격리에는 맞지 않음(효용 낮음 · §5 격리안은 따로 검토) |
| 객체 상세 패널(86) | `nexa-sql/src/objdetail.rs`(읽기 전용 섹션 뷰) · `app/meta.rs` `sync_detail_target`/`feed_comments` | 편집기 진입점(우클릭·상세 패널) + 코멘트 비동기 채움 패턴 | 읽기 전용 |

**새로 만들어야 하는 것**
- **구조화된 테이블 정의 모델 + 방언별 읽기** — `ColumnInfo`/`KeyDef`/`IndexDef` 확장(타입 분해 · identity · collation · generated · CHECK 식 · FK 동작 · 인덱스 옵션 · 코멘트 NULL/빈 문자열 구분).
- **차이 계산 → ALTER 생성 엔진** — before/after 의미 diff → 방언별 ADD/ALTER/DROP/RENAME COLUMN · 제약·인덱스 DROP+재생성 · 코멘트 · 문장 순서(FK 먼저 DROP · 나중 ADD) · SQLite = "새 테이블 → 복사 → 교체" 재생성 전략 · MSSQL = DEFAULT 제약 이름 처리 · `gen.rs` CREATE 조각 함수 공개 또는 이전.
- **DDL 적용기** — `apply_changes`의 영향 행 = 1 보호는 DDL에 안 맞음 · PG·MSSQL·SQLite = 단일 트랜잭션 전체 롤백 · Oracle·MySQL = 문장마다 암묵 커밋 → "중간 실패 = 일부 적용" 보고 + 복구 안내 · 데이터 손실 경고(컬럼 삭제 · 타입 축소 · NOT NULL 추가 전 NULL 행 수 사전 검사) · 실행 경로 = 백업 게이트(`run_text_in`) + `ddl_target` 새로 고침을 지나게 통일.
- **편집 UI** — 컬럼·제약·인덱스 탭 · 타입 선택기 · 방언별 타입 목록 · ChangeSet·편집 그리드 재사용 + 정의 행용 `CellSpec` 검증 확장 · SQL Preview에 "적용" 동작 + 경고 칸.
- **방언 능력표 확장** — 컬럼 rename 지원 버전 · 컬럼 순서 변경(MySQL `AFTER`만) · 온라인 DDL · 코멘트 방식(COMMENT ON / 확장 속성 / MySQL 인라인 / SQLite 없음).
- 참고 = 별도 MySQL 드라이버 크레이트는 안 보임(`Dialect::Mysql`만) → 실서버 검증 경로 먼저 확인.
