# 47 · 객체 인텔리센스 — 메타 저장소 한 벌(one datasource) · 점증 수집 · 부분 갱신 · O(1)/이진 탐색

> **상태**: 📐 설계 확정(09-17 · 사용자 요청 "데이터 생성·갱신·메모리 · 적용 범위 · 트리거 · 갱신 주기/범위 · 하나의 데이터로 툴팁까지 · 설명 메타 · 해싱/이진탐색 · 탐색기 직결 · 별도 스레드 · 점증 완성 · 부분 서비스 · 변경분만 갱신") · 구현 = **T-57**(재정의) · 의사결정 **D-79~D-86 ✅ 사용자 확정(09-17 · 전부 권장안)** · ★ **모든 결정은 하드코딩이 아니라 §8 설정 키로**(사용자 09-17 · 카테고리는 DBeaver 기준).
> 선행: [29 §6](29-editor-syntax-palette-statusbar.md)(후보 소스 순위 · alias 탐지 · 시스템 오브젝트 패키지 · 팝업 UI) · [28](28-object-explorer.md)(탐색기 4계층 · 메타 세션 · 갱신 정책) · [30](30-architecture-patterns.md)(포트+레지스트리+설정) · [26 §8](26-performance-architecture.md)(네트워크 부하 규칙) · [39](39-resource-governance.md)(부하원 등재).

## 0. 결론 일곱 줄

1. **데이터는 한 벌**: `nsql-run::meta::MetaStore` — 탐색기 트리 · 자동 완성 · hover 툴팁 · 팔레트 `@객체` · CLI `nsql shell` Tab/`DESC`가 **같은 저장소**를 읽는다. 소비자별 사본 없음(사용자 조건).
2. **원천은 탐색기와 같은 메타 세션**: 카탈로그 질의는 지금의 `nsql-catalog` 포트(방언별 SQL) 그대로 · 탐색기 메타 스레드가 결과를 저장소에 넣고, 트리는 저장소의 **뷰**가 된다. 서버 접속 수는 늘지 않는다(26 §8).
3. **점증 수집**: 접속 → 스키마 목록 + 현재 스키마 객체(0.x초) → 현재 스키마 컬럼(스키마 단위 **벌크 질의** · 청크) → 다른 스키마는 요청(입력·펼침)이 있을 때 · 유휴 때 순차. **부분 상태에서도 서비스**: 있는 것은 즉시 후보, 없는 것은 "불러오는 중…" 한 줄 + 도착하면 팝업 갱신.
4. **갱신은 변경분만**: 버킷(스키마×종류) 단위 **재조회 → 디프 → 바뀐 항목만 교체**(이름·종류·수정시각 해시 비교) · 안 바뀐 테이블의 컬럼은 그대로. 트리거 = 수동(탐색기 새로고침) · 실행한 DDL 감지 · (선택) 워터마크 주기 점검(`MAX(LAST_DDL_TIME)`류 1행 질의).
5. **탐색은 O(1)/O(log n)**: 이름은 **인터닝**(`Sym(u32)`) · 정확 조회 `HashMap<(schema, lower_name) → ObjId>` · 접두 조회는 버킷마다 **소문자 정렬 배열 + `partition_point` 이진 탐색** · 부분/약어 일치는 상한 있는 후처리(후보 ≤ 500 · 예산 30ms). 저장소 읽기는 **락 없는 스냅샷**(`Arc<Snapshot>` 교체 · 버킷마다 `Arc` 구조 공유)이라 UI/워커가 갱신에 막히지 않는다.
6. **스레드 3개 역할**: 메타 스레드(질의 · 저장소 쓰기) · 인텔 워커(문장 파싱 · 후보 계산 · 툴팁 조립) · UI(요청 보내기 · 도착한 것 그리기). 세대 번호로 늦은 응답은 버린다.
7. **설명(코멘트)도 같은 저장소**: 테이블/컬럼 코멘트는 같은 질의에서(Oracle `ALL_TAB_COMMENTS`/`ALL_COL_COMMENTS` · MSSQL `extended_properties` · PG `obj_description`) 받아 후보 오른쪽·툴팁에 표시(설정으로 끔).

## 1. 지금 있는 것 · 없는 것

| 있음 | 없음(이 설계가 채움) |
|---|---|
| `nsql-catalog`: `schemas` · `current_schema` · `objects(schema, kind)` · `columns(schema, table)` · `keys` · `source` · `compile_errors` · `parse_create_header` | 스키마 단위 **벌크 컬럼** 질의 · **코멘트** 열 · 워터마크 질의 |
| `explorer.rs` 메타 스레드(프로필당 1 세션 · 요청 `Schemas/Objects/Columns/Source/Live` · 세대 번호 · 끊김 즉시 해제) | 결과를 트리 노드 밖에 **저장**하는 곳 · 인덱스 · 다른 소비자 |
| `nsql_script::statement_at`(캐럿 문장) · 토크나이저 · `TxClass`(DDL 판정) | 문맥 판정(FROM 뒤/alias 뒤/스키마 뒤) · alias 표 · 후보 랭킹 · 팝업 컨트롤 · hover 카드 |
| docs/29 §6 원칙(끄면 비용 0 · 워커 · 디바운스·예산 · 후보 순위 · 시스템 오브젝트 패키지) | 저장소·갱신·메모리 설계(이 문서) |

## 2. 아키텍처

```
                 ┌──────────── UI 스레드 ────────────┐
 편집기 입력 ──▶ IntelReq{gen, doc_hash, caret, kind} ──▶ ┐
 hover 대기 ──▶ HoverReq{gen, word, ctx}              ──▶ │  intel 워커(우선순위 낮음)
 탐색기 펼침 ──▶ MetaReq::Ensure(bucket)  ─────────────┐ │   · statement_at → 문맥/alias 표(문장 해시 캐시)
                                                       │ │   · store.snapshot() → 후보/툴팁 조립(예산 30ms)
                                                       ▼ ▼   · 빠진 버킷은 MetaReq::Ensure를 메타 스레드에
                     메타 스레드(nsql-explorer · 프로필당 1 세션)   ◀─┘
                       큐 2단: [사용자/탐색기 요청] > [채움 계획(유휴)]
                       nsql-catalog 질의 → 디프 → Snapshot 교체(버킷 Arc만 새로)
                                    │
                     MetaStore(Arc<Snapshot>) ◀── 읽기: 탐색기 트리 · 팝업 · 툴팁 · 팔레트 · CLI
```

- **왜 메타 스레드 하나**: 서버 접속을 늘리지 않고(26 §8 · 프로브도 상한) 카탈로그 질의를 **순차**로 보내면 서버 부하가 예측 가능하다. 사용자 요청(펼침 · `.` 뒤 컬럼)이 채움 계획보다 **항상 먼저**(큐 2단 · 진행 중 청크는 끝까지 · 청크는 ≤ 200ms 크기).
- **인텔 워커는 서버를 모른다**: 스냅샷만 읽는다. 빠진 데이터는 "필요하다"고 메타 스레드에 알리고 **지금 있는 것으로 답**한다.

## 3. 저장소(`nsql-run::meta`) — 자료구조

```rust
pub struct Sym(u32);                       // 인터닝된 이름(원문 보존 · 소문자 키는 인터너가 함께 가짐)
pub struct ObjId(u32);                     // 스냅샷 안 객체 인덱스
pub struct ObjEntry { schema: Sym, name: Sym, kind: ObjectKind, modified: u64 /*epoch s · 0=모름*/,
                      status: u8, comment: Option<Sym>, cols: ColState }
pub enum ColState { Unknown, Loading, Loaded { at: u64, range: Range<u32> } /* cols 배열 구간 */, Error }
pub struct ColEntry { name: Sym, data_type: Sym, nullable: bool, position: u16, default: Option<Sym>, comment: Option<Sym>,
                      key: KeyFlags /*PK·FK·UQ 비트*/ }
pub struct Bucket { schema: Sym, kind: ObjectKind, state: Coverage, objs: Vec<ObjId> /*소문자 이름 정렬*/, hash: u64 }
pub enum Coverage { Missing, Loading{since}, Loaded{at, n}, Error{msg, at} }
pub struct Snapshot {
    dialect: Dialect, current_schema: Sym,
    schemas: Arc<Vec<Sym>>,                             // 정렬
    buckets: HashMap<(Sym, ObjectKind), Arc<Bucket>>,   // 스키마×종류
    objs: Arc<Vec<ObjEntry>>, cols: Arc<Vec<ColEntry>>,  // 세대별 append-only 조각(§5 디프)
    exact: Arc<HashMap<(Sym, Sym /*lower*/), ObjId>>,    // O(1) 정확 조회(스키마 없이 찾을 때는 (current, name) → (synonym) → 전 스키마 순)
    by_name: Arc<Vec<(Sym /*lower*/, ObjId)>>,           // 전 스키마 이름 정렬(스키마 모를 때 접두 이진 탐색)
    watermark: HashMap<Sym, Watermark>,                  // 스키마별 변경 감지 값
    stamp: u64,                                          // 스냅샷 세대
}
```

### 3-1. 유니버설 메타 모델(사용자 09-17 "DBMS가 달라도 전체 집합 구조 · 못 채우는 값은 기본값/공백 · 논리 뷰 동일 · 인터페이스처럼 추상화")

원칙: **모델은 모든 DBMS 메타의 상위집합(superset)** 하나이고, 어댑터는 그 모델을 **채울 수 있는 만큼만** 채운다. 소비자(완성·툴팁·트리·CLI)는 방언을 묻지 않고 같은 필드를 읽는다. "없음"의 뜻을 세 가지로 구분해 논리 뷰가 흔들리지 않게 한다.

| 값 상태 | 표현 | 뜻 · 표시 |
|---|---|---|
| **채움** | `Some(v)` / 비어 있지 않은 문자열 | 서버가 준 값 |
| **빈 값** | `""` / `Some(0)` | 서버가 "없다"고 답함(코멘트 없음 · 기본값 없음) — 툴팁에 빈칸 |
| **해당 없음** | `None` + `MetaCaps` 비트 off | 그 DBMS에 개념이 없음(SQLite 스키마 · PG 수정시각 · MSSQL 시노님 상태) — 툴팁·열 자체를 **숨김** · 완성 랭킹에서 중립 |
| **아직 모름** | `Coverage::Missing/Loading` | 수집 전 — 부분 서비스(§4) |

```rust
/// 방언별 "채울 수 있는 것" — 어댑터가 접속 때 한 번 신고 · 소비자는 이것으로 열/행을 숨긴다(값이 None인 이유를 구분).
pub struct MetaCaps { bits: u64 }   // SCHEMAS · CATALOGS(DB) · SYNONYMS · PACKAGES · MATVIEWS · SEQUENCES · TYPES · TRIGGERS
                                     // · OBJ_MODIFIED · OBJ_STATUS · OBJ_COMMENT · COL_COMMENT · COL_DEFAULT · COL_IDENTITY
                                     // · KEYS_FK · INDEXES · PARAMS · SOURCE · WATERMARK · CURRENT_SCHEMA · CASE_INSENSITIVE_NAMES
pub struct SchemaEntry { name: Sym, catalog: Option<Sym> /*MSSQL/PG DB*/, is_current: bool, is_system: bool }
pub struct ObjEntry   { schema: Sym, name: Sym, kind: ObjectKind /*17종 상위집합*/, status: ObjStatus /*Valid|Invalid|Unknown*/,
                        modified: Option<u64>, created: Option<u64>, comment: Option<Sym>, extra: Option<Sym> /*PG 오버로드 서명 등*/,
                        target: Option<(Sym, Sym)> /*시노님·뷰 대상*/, cols: ColState }
pub struct ColEntry   { name: Sym, data_type: Sym /*원문*/, canon: CanonType /*Text|Int|Float|Decimal|Date|Time|DateTime|Bool|Binary|Json|Other*/,
                        length: Option<u32>, precision: Option<u16>, scale: Option<u16>, nullable: Option<bool>, position: u16,
                        default: Option<Sym>, comment: Option<Sym>, identity: Option<bool>, key: KeyFlags /*PK·FK·UQ·IDX 비트*/,
                        fk_target: Option<(Sym, Sym, Sym)> }
pub struct ParamEntry { name: Sym, data_type: Sym, mode: ParamMode /*In|Out|InOut|Return|Unknown*/, position: u16, default: Option<Sym> }
```

- **어댑터 인터페이스**(`nsql-catalog` 확장 · 방언별 구현 · 기본 구현은 "해당 없음"):
  ```rust
  pub trait MetaSource {
      fn caps(&self) -> MetaCaps;
      fn schemas(&mut self, s: &mut dyn Session) -> Result<Vec<SchemaEntry>, DbError>;
      fn objects(&mut self, s: &mut dyn Session, schema: &str, kind: ObjectKind) -> Result<Vec<ObjEntry>, DbError>;
      fn columns_bulk(&mut self, s: &mut dyn Session, schema: &str, after: Option<&str>, limit: usize) -> Result<Vec<(Sym /*table*/, ColEntry)>, DbError>;
      fn columns(&mut self, s: &mut dyn Session, schema: &str, table: &str) -> Result<Vec<ColEntry>, DbError>;
      fn params(&mut self, ...) -> Result<Vec<ParamEntry>, DbError> { Ok(vec![]) }       // 기본 = 해당 없음
      fn watermark(&mut self, s: &mut dyn Session, schema: &str) -> Result<Watermark, DbError> { Ok(Watermark::None) }
  }
  ```
  `caps()`에 없는 메서드는 호출하지 않는다(질의 0). 정적 패키지(시스템 오브젝트 · 29 §6-4)도 같은 트레이트로 읽기 전용 구현.
- **방언 매핑 표**(채움 규칙 · 빈칸 = 해당 없음):

| 필드 | Oracle | SQL Server | PostgreSQL | SQLite | MySQL |
|---|---|---|---|---|---|
| 스키마 | `ALL_USERS` | `sys.schemas`(+DB) | `pg_namespace`(+DB) | — (`main`/attached만 · `is_current`) | DB = 스키마 |
| 객체 수정시각 | `LAST_DDL_TIME` | `modify_date` | — (`None` · 워터마크는 수·oid) | — | `UPDATE_TIME`(테이블만) |
| 객체 상태 | `STATUS`(VALID/INVALID) | — | — | — | — |
| 코멘트 | `ALL_TAB/COL_COMMENTS` | `extended_properties(MS_Description)` | `obj/col_description` | — | `TABLE_COMMENT`/`COLUMN_COMMENT` |
| 시노님 대상 | `ALL_SYNONYMS` | `sys.synonyms.base_object_name` | — | — | — |
| 컬럼 기본값 | `DATA_DEFAULT`(LONG → 문자열) | `default_constraints` | `pg_attrdef` | `pragma_table_info.dflt_value` | `COLUMN_DEFAULT` |
| identity | `IDENTITY_COLUMN`(12c+) | `is_identity` | `attidentity` | `AUTOINCREMENT`(sqlite_master 파싱) | `EXTRA=auto_increment` |
| 파라미터 | `ALL_ARGUMENTS` | `sys.parameters` | `pg_proc.proargnames` | — | `information_schema.PARAMETERS` |
| 이름 대소문자 | 대문자 정규화 · 따옴표 보존 | 기본 무시(collation) | 소문자 정규화 · 따옴표 보존 | 무시 | OS 의존 → `CASE_INSENSITIVE_NAMES` 비트 |

- **정규화 규칙**(어댑터 안에서 · 저장소는 결과만 받음): 타입 원문은 그대로 두고 `CanonType`만 매핑(툴팁·랭킹·데이터 편집기 공용) · 시각은 epoch 초 · 대소문자는 인터너가 소문자 키를 함께 갖되 **표시 원문 유지** · 따옴표 식별자는 원문 키도 등록(§3 표).
- 소비자 규칙: 필드를 읽기 전에 방언을 `match`하지 않는다 — `caps()`와 `Option`만 본다. 방언별 예외가 필요해지면 **필드나 `MetaCaps` 비트를 추가**해 모델을 넓힌다(소비자에 방언 분기 금지 · 30 §1 확장점 규칙).

| 연산 | 비용 | 방법 |
|---|---|---|
| `lookup(schema?, name)` | O(1) | `exact` 해시(소문자 · 따옴표 식별자는 원문 키도 함께 등록) |
| `prefix(schema, kind, "cus", limit)` | O(log n + k) | 버킷 `objs`(소문자 정렬)에서 `partition_point` 2회 → 구간 |
| `prefix_any("cus", limit)` | O(log N + k) | `by_name` 전 스키마 정렬 배열 |
| `columns(obj)` | O(1) | `cols` 구간 슬라이스(정렬은 position · 접두는 소문자 보조 정렬 인덱스 `col_sorted` 구간) |
| 부분/약어 일치(`s_c` → `SALES_CUSTOMER`) | O(n·m) 상한 | 접두 결과가 `limit` 미만일 때만 · 버킷 크기 ≤ 20k · 예산 초과 시 중단(부분 결과 표시) |
| 스냅샷 읽기 | 락 0 | `ArcSwap`류(`Mutex<Arc<Snapshot>>`에서 `Arc` 복제만 · 그리는 동안 락 없음) |

- **메모리 추정**: 객체 10k + 컬럼 200k 기준 — 인터너 문자열 ≈ 6MB · `ObjEntry` 32B×10k · `ColEntry` 24B×200k ≈ 5MB · 인덱스 ≈ 4MB → **≈ 15MB**. 상한 `meta.max_mb`(64) 초과 시 **가장 오래 안 쓴 스키마의 컬럼부터** 내림(객체 목록은 유지 · 다시 필요하면 재조회). 부하원 원장 39 §3에 등재(스레드 0 추가 · 메모리 · 네트워크 = 메타 세션 순차).
- **디스크 캐시(선택 · D-85)**: `<config>/meta/<profile-id>.nmeta`(버전 · 방언 · 스냅샷 직렬화 · 워터마크). 접속 직후 읽어 **즉시 서비스** → 백그라운드에서 워터마크 비교 → 다른 버킷만 재조회. 비밀 없음(이름·타입·코멘트만) · 크기 상한 같음.

## 4. 수집 계획(점증) — 접속부터

| 단계 | 언제 | 무엇 | 크기/예산 |
|---|---|---|---|
| S0 | 접속 직후 | `schemas` · `current_schema` · (디스크 캐시 있으면 적재) | 1~2 질의 |
| S1 | S0 뒤 즉시 | 현재 스키마의 **모든 종류 객체**(`objects` 종류별 · 종류 순서 = Table → View → Synonym → Function/Procedure/Package → 나머지) | 종류당 1 질의 · 결과 ≤ 20k |
| S2 | S1 뒤 유휴 | 현재 스키마 **컬럼 벌크**: 새 카탈로그 함수 `columns_bulk(schema, after_table, limit)` — 테이블 이름 순 페이지(≤ 2,000행/청크 · 청크 사이에 사용자 요청 양보) | 청크 ≤ 200ms |
| S3 | 요청 시 | `SCHEMA.` 입력 · 탐색기 펼침 · 문장에 다른 스키마 등장 → 그 스키마 S1(+S2 그 테이블만) | 즉시(큐 앞) |
| S4 | 유휴(설정 D-81) | 다른 스키마를 접근 빈도/이름 순으로 순차 S1 → S2 · `meta.prefetch=all`일 때만 · 앱 유휴 5s 뒤 · 실행 중이면 중단 | 스키마당 |
| 즉시 채움 | `alias.` 뒤 컬럼이 `Unknown` | 그 테이블 `columns` 1건(큐 앞) → 도착하면 열린 팝업 갱신 | 1 질의 |

- **부분 서비스 규칙**: 버킷 `Loading`이면 팝업 첫 줄 "`SCHEMA` 불러오는 중… (n/m)" · 있는 후보는 정상 표시 · 도착하면 같은 세대 팝업만 갱신. `Error`면 흐린 한 줄 + 로그 창 1줄 · 재시도는 사용자 요청(새로고침)에서만(자동 재시도 금지 · 26 §8).
- **취소**: 접속 해제/세대 변경 → 진행 중 청크 결과 버림 · 큐 비움. 실행 중(`busy`)에는 S4 정지(서버 부하 양보).

## 5. 갱신 — 변경분만

| 트리거 | 범위 | 방법 |
|---|---|---|
| 탐색기 새로고침(노드/하위 전체/루트 ⟳) | 그 노드 버킷(하위 = 버킷들) | 재조회 → **디프** |
| 실행한 DDL(`TxClass::is_ddl` · `parse_create_header` → 종류·스키마·이름) | 그 객체(+종류 버킷 1개) | `CREATE/ALTER/DROP` → 그 버킷 재조회 · `ALTER TABLE` → 그 테이블 컬럼 `Unknown`으로 |
| 워터마크 주기(D-82 · `meta.refresh_secs` · 유휴 때만) | 스키마별 | 1행 질의로 값 비교: Oracle `MAX(LAST_DDL_TIME), COUNT(*)`(ALL_OBJECTS) · MSSQL `MAX(modify_date), COUNT(*)`(sys.objects) · PG `COUNT(*), MAX(oid)` + `pg_class.relfilenode` 합(수정시각 없음 → 수 변화·xmin 합) · SQLite `PRAGMA schema_version` → 바뀐 스키마만 S1 재조회 |
| 접속 재개(재접속·다른 프로필) | 전체 | 캐시 있으면 워터마크 → 다른 버킷만 |

**디프 알고리즘**(버킷 단위): 새 목록을 `(lower_name) → (kind, modified, status, comment)` 해시로 만들고 옛 버킷과 비교 → 추가·삭제·변경 세 집합. 변경 없음 = 옛 `Arc<Bucket>` 그대로(스냅샷 교체 비용 = 해시맵 항목 1개). 변경 있음 = 새 `Bucket` · 안 바뀐 `ObjEntry`는 **같은 ObjId 재사용**(컬럼 구간 유지) · `modified`가 바뀐 테이블만 `cols = Unknown`. 스냅샷 `objs/cols`는 세대별 조각 append + 삭제 표시(tombstone) · 삭제 비율 > 30%면 유휴 때 컴팩션(한 번의 재배치 · 스냅샷 교체).

## 6. 소비자

| 소비자 | 읽는 것 | 비고 |
|---|---|---|
| **자동 완성**(편집기) | 문맥 → 버킷 접두/컬럼 | 문맥: `FROM/JOIN/UPDATE/INTO/DELETE FROM` 뒤 = 관계(D-80 범위) · `alias.`/`table.` = 컬럼 · `SCHEMA.` = 그 스키마 객체 · `SELECT …`/`WHERE`/`ON` = 문장 테이블 컬럼(alias 표) + 함수 · 그 밖 = 키워드+문서 단어. 랭킹 = 정확 > 접두 > 단어 경계(`_` 뒤) > 약어/부분 · 같은 등급은 MRU(최근 확정 20개) > 이름. 상한 200 · 팝업 12행 가상화 |
| **hover 툴팁**(D-84) | `lookup` + `columns` + 코멘트 + 키 | 캐럿 아닌 **마우스 위치 단어**(alias면 표로 풀어) · 300ms 정지 · 카드 = 종류 아이콘 · `SCHEMA.NAME` · 코멘트 · 컬럼 표(≤ 20행 · PK 굵게 · 타입 · NULL) · "탐색기에서 보기 · SELECT 템플릿" 링크 · 별도 데이터 조립 없음(스냅샷 참조만) |
| 탐색기 트리 | 버킷 · 컬럼 | 노드 = `(bucket, ObjId)` 참조 · 상태 스피너 = `Coverage` |
| 팔레트 `@` | `prefix_any` | Goto object → 탐색기 위치·SELECT 템플릿 |
| CLI `nsql shell` Tab · `DESC` | 같은 API(동기) | 스레드 대신 그 자리에서 · 예산 동일 |
| 시스템 오브젝트·내장 함수(29 §6-4) | 정적 패키지 `Packages/Intel/<dialect>` | 저장소와 **같은 인터페이스**(`MetaSource` 트레이트 · 읽기 전용 스냅샷)로 합쳐 보인다 — 데이터는 따로지만 소비자는 하나의 API |

## 7. 의사결정(✅ 사용자 확정 09-17 — 전부 권장안 · 값은 §8 설정의 **기본값**이 된다)

| # | 질문 | 후보 | 권장 |
|---|---|---|---|
| **D-79** 트리거 | 팝업이 언제 뜨는가 | ① 입력 즉시 자동 ② 자동 + 지연(250ms) ③ Ctrl+Space만 ④ ②+Ctrl+Space 즉시 | ✅ **④** — `.` 뒤는 즉시 · 식별자 2자+지연 · 수동키 항상 → `intel.auto_activation`/`intel.delay_ms`/`intel.trigger_chars`/`intel.activate_on_typing` |
| **D-80** FROM 뒤 범위 | 스키마 없이 무엇을 보여주나 | ① 현재 스키마 관계만 ② 스키마 이름만(스키마 입력 뒤 객체) ③ 현재 스키마 관계 + 스키마 이름 + 시노님 ④ 접근 가능한 전 스키마 관계 | ✅ **③** → `intel.from_scope`(current_and_schemas) · ④ = `all` · ① = `current` · ② = `schemas_first` |
| **D-81** 선반입 범위 | 접속 뒤 무엇을 미리 채우나 | ① 현재 스키마만(나머지 요청 시) ② ① + 탐색기에서 펼친/문장에 나온 스키마 ③ ② + 유휴 때 전 스키마 순차 | ✅ **②** → `meta.prefetch`(current_and_used) · ① = `current` · ③ = `all` |
| **D-82** 갱신 방식 | 변경을 어떻게 아나 | ① 수동 새로고침만 ② ① + 실행한 DDL 감지 ③ ② + 워터마크 주기(기본 300s · 유휴) | ✅ **③** → `meta.refresh_on_ddl`(on) · `meta.refresh_secs`(300 · 0 = 끔) · `meta.refresh_idle_secs`(5) |
| **D-83** 코멘트 | 테이블/컬럼 설명을 언제 받나 | ① 객체·컬럼 질의에 함께(항상) ② 툴팁 열 때만 지연 ③ 안 받음 | ✅ **①** → `meta.comments`(always) · ② = `on_demand` · ③ = `off` · 표시는 `intel.show_comments` |
| **D-84** hover 툴팁 | 무엇에 띄우나 | ① 테이블/뷰만 ② 테이블/뷰/컬럼(alias 포함)/프로시저·함수 ③ 끔 | ✅ **②** → `intel.hover`(on) · `intel.hover_scope`(all) · ① = `relations` · `intel.hover_delay_ms`(300) |
| **D-85** 디스크 캐시 | 재시작 뒤 즉시 서비스 | ① 메모리만 ② 프로필별 디스크 캐시 + 워터마크 검증 | ✅ **②** → `meta.disk_cache`(on) · `meta.disk_cache_max_mb`(64) · 프로필 삭제 시 파일도 삭제 |
| **D-86** 매칭 | 후보 일치 규칙 | ① 접두만 ② 접두 + 단어 경계 부분 ③ ② + 약어 퍼지(`s_c`) | ✅ **③** → `intel.match`(fuzzy) · ② = `contains` · ① = `prefix` · `intel.match_case`(insensitive) |

T-57 작업 순서 = ⓪ §8 설정 키 전부 REGISTRY 등록(라벨·설명 = Msg · 종속 잠금) ① `nsql-run::meta` 저장소+인덱스+디프(테스트) ② 카탈로그 벌크 컬럼·코멘트·워터마크 질의(방언 4) ③ 탐색기를 저장소 뷰로 ④ 인텔 워커 + 문맥/alias ⑤ 팝업 컨트롤(nexa-ctl) ⑥ hover 카드 ⑦ 디스크 캐시 ⑧ CLI Tab.

## 8. 설정 — 결정 전부를 키로(사용자 09-17 "하드코딩 아닌 설정 · 카테고리는 DBeaver 기준")

DBeaver 환경설정의 두 페이지를 그대로 카테고리로 쓴다: **편집기 ▸ SQL 편집기 ▸ 코드 완성**(`Editors ▸ SQL Editor ▸ Code Completion` = `intel.*`)과 **접속 ▸ 메타데이터**(`Connection types/Metadata` = `meta.*`). 설정 창 카드 순서 = 아래 표 순서(그룹 제목 = DBeaver 그룹). 값은 `nsql config list all`에 전부 보이고, 자주 안 바꾸는 것은 `HIDDEN`.

### 8-1. 코드 완성(`intel.*` · `CatIntel` · DBeaver "Code Completion")

| 그룹(DBeaver) | 키 | 종류 · 기본 | DBeaver 대응 | 결정 |
|---|---|---|---|---|
| 자동 활성화 | `intel.enabled` | Bool · **on** | (페이지 전체) | 끄면 훅 0(29 §6-1) |
| | `intel.auto_activation` | Bool · **on** | Enable auto activation | D-79 |
| | `intel.delay_ms` | Int 0~2000 · **250** | Auto activation delay | D-79 |
| | `intel.activate_on_typing` | Bool · **on** | Activate on typing | D-79 · off = 트리거 문자·수동키만 |
| | `intel.trigger_chars` | Text · **`.`** | Activation triggers | D-79 · `.`·`(`·`,` 등 |
| | `intel.min_chars` | Int 1~5 · **2** | — | 식별자 n자부터 자동 |
| | `key.edit.complete` | 키 · **Ctrl+Space** | (키 바인딩) | D-79 · 키맵 프리셋에 |
| 제안 | `intel.match` | Choice prefix/contains/**fuzzy** | Match by contains(확장) | D-86 |
| | `intel.match_case` | Choice **insensitive**/sensitive | Case sensitive names | D-86 |
| | `intel.sort` | Choice **relevance**/alpha | Sort alphabetically | 랭킹(§6) vs 이름순 |
| | `intel.max_items` | Int 50~2000 · **200** | — | 후보 상한 |
| | `intel.recent_boost` | Bool · **on** | — | MRU 20개 우선 |
| | `intel.hide_duplicates` | Bool · **on** | Hide duplicate names | 시노님=테이블 중복 |
| | `intel.global_search` | Bool · **off** | Enable global search | 스키마 없이 전 스키마 `prefix_any` |
| 범위 | `intel.from_scope` | Choice current/schemas_first/**current_and_schemas**/all | Use short names(확장) | D-80 |
| | `intel.system_objects` | Bool · **on** | Show server-side objects | 29 §6-4 정적 패키지 |
| | `intel.keywords` | Bool · **on** | Show keywords | 후보 순위 5 |
| | `intel.document_words` | Bool · **on** | — | 후보 순위 6 |
| 삽입 | `intel.insert_case` | Choice **default**/upper/lower/match | Proposal insert case | 확정 시 대소문자 |
| | `intel.insert_space` | Bool · **on** | Insert space after table/column | |
| | `intel.insert_alias` | Bool · **off** | Insert table alias | FROM 확정 시 `A` 붙임 |
| | `intel.qualify` | Choice **short**/schema | Use long names | 확정 시 `SCHEMA.` 포함 여부 |
| 표시 | `intel.show_types` | Bool · **on** | Show column data types | |
| | `intel.show_comments` | Bool · **on** | — | D-83 표시 |
| | `intel.popup_rows` | Int 6~30 · **12** | — | 팝업 가시 행 |
| | `intel.signature_help` | Bool · **on** | — | 함수 인자 힌트(29 §6-4) |
| 부가 정보(hover) | `intel.hover` | Bool · **on** | Show hover(DBeaver: 편집기 hover) | D-84 |
| | `intel.hover_scope` | Choice relations/**all** | — | D-84 |
| | `intel.hover_delay_ms` | Int 100~2000 · **300** | — | D-84 |
| | `intel.hover_columns` | Int 0~100 · **20** | — | 카드 컬럼 표 행 수 |
| 자원 | `intel.budget_ms` | Int 5~200 · **30** · HIDDEN | — | 후보 계산 예산(29 §6-1) |

### 8-2. 메타데이터(`meta.*` · `CatMeta` · DBeaver "Metadata")

| 그룹(DBeaver) | 키 | 종류 · 기본 | DBeaver 대응 | 결정 |
|---|---|---|---|---|
| 읽기 | `meta.enabled` | Bool · **on** | Read metadata on connect | off = 탐색기 펼침만 · 인텔 저장소 안 채움 |
| | `meta.prefetch` | Choice current/**current_and_used**/all | Read all objects on connect(확장) | D-81 |
| | `meta.prefetch_columns` | Bool · **on** | — | S2 벌크 컬럼 |
| | `meta.comments` | Choice **always**/on_demand/off | Read table/column comments | D-83 |
| | `meta.keys` | Bool · **on** | Read keys | PK/FK 비트(툴팁·랭킹) |
| | `meta.separate_connection` | Bool · **on** | Open separate connection for metadata | 지금의 메타 세션(26 §8 상한 안) |
| 갱신 | `meta.refresh_on_ddl` | Bool · **on** | — | D-82 |
| | `meta.refresh_secs` | Int 0~3600 · **300** | (탐색기 `explorer.refresh_secs`와 통합 · 0 = 끔) | D-82 워터마크 |
| | `meta.refresh_idle_secs` | Int 1~60 · **5** · HIDDEN | — | 유휴 판정 |
| | `meta.refresh_scope` | Choice **changed**/all | — | 워터마크가 바뀐 스키마만 / 전부 |
| 캐시 | `meta.disk_cache` | Bool · **on** | Cache metadata | D-85 |
| | `meta.disk_cache_max_mb` | Int 8~512 · **64** · HIDDEN | — | 프로필당 |
| | `meta.max_mb` | Int 16~1024 · **64** | — | 메모리 상한(초과 = 컬럼 LRU 내림) |
| 부하 | `meta.chunk_rows` | Int 200~10000 · **2000** · HIDDEN | — | 벌크 청크 |
| | `meta.chunk_ms` | Int 50~1000 · **200** · HIDDEN | — | 청크 시간 상한 |
| | `meta.timeout` | Int 5~120 · **15** | (탐색기 `explorer.timeout` 통합) | |
| | `meta.pause_while_running` | Bool · **on** · HIDDEN | — | 실행 중 S4 정지 |

- 종속 잠금(설정 창 D-52 규칙): `intel.enabled=off` → `intel.*` 전부 잠김 · `intel.auto_activation=off` → `delay_ms/activate_on_typing/trigger_chars/min_chars` 잠김 · `intel.hover=off` → `hover_*` 잠김 · `meta.enabled=off` → `meta.*` 잠김 · `meta.refresh_secs=0` → `refresh_idle_secs/refresh_scope` 잠김 · `meta.disk_cache=off` → `disk_cache_max_mb` 잠김.
- 성능 모드(39 §4 `perf.mode`/`perf.boost`)와의 관계: `perf.boost`는 UI 전용 키만 강제하므로 `intel.*`/`meta.*`는 건드리지 않는다. 대신 `perf.mode=low`(저사양 프리셋)에서 `meta.prefetch=current` · `meta.prefetch_columns=off` · `intel.match=prefix`로 프리셋 값을 낮춘다(사용자가 개별 값을 두면 그 값이 우선 · 39 §4 우선순위).
- 기존 `explorer.auto_refresh/refresh_secs/timeout`은 `meta.*`로 **통합**(탐색기와 인텔이 같은 저장소를 쓰므로 갱신 정책도 하나 · 마이그레이션 = 옛 키 값이 있으면 새 키로 옮기고 제거).
- CLI: 같은 키가 `nsql shell` Tab 완성과 `DESC`에도 적용(스레드 없이 동기).
