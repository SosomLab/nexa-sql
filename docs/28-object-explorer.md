# 28 · 오브젝트 탐색기(Object Explorer) — 지연 1단계 로드 · 노드별 병렬 스레드 · 오류 비확산 · 수동 갱신 기본

> **요청**(사용자 09-14 · DBeaver 캡처): *"DBMS의 Object 탐색 기능. 자동 갱신은 사용자가 직접 설정했을 때만(기본 없음). 조회 시점 = 해당 항목이 확장된 시점에 **단일 레벨(그 수준)만** 메타 정보 조회. 각 구성별 별도 Thread(Tables 확장 진행 중에 Procedures를 확장하면 병렬) · 각 동작의 오류가 escalation 되지 않게."*
> **선행**: [01 아키텍처](01-architecture.md)(4계층 · 어댑터는 `Session` 포트만) · [22](22-driver-extensions.md)(드라이버 확장 — 카탈로그도 같은 경계) · [26 성능](26-performance-architecture.md)(단계 계측 · 경량) · [nexa-ui 21 그리드 계열](../../nexa-ui/docs/21-grid-family.md)(FileGrid와 같은 계층 그리드) · TODO T-20(`Catalog` 포트).
> **상태**: 🚧 **1차 구현 09-15**(`nsql-catalog` + `explorer.rs` + `nsql cat` — 메타 세션 스레드 1개 순차 · 지연 1단계 · 노드별 ⚠ · 소스/SELECT 새 탭). 잔여 = 스레드 풀·취소·툴팁·자동 갱신·nexa-grid. 작업 **T-56**. 결정 **D-46·D-47**(1차 = separate 고정).

---

## 0. 원칙 다섯 줄

1. **확장한 노드의 자식만, 그때만 읽는다.** 트리는 처음에 접속 → 스키마 목록만 안다. 손자는 절대 미리 읽지 않는다(캐시는 "읽은 것"의 기록일 뿐).
2. **요청 하나 = 스레드 하나(상한 있음).** Tables를 펼치는 중에 Procedures를 펼치면 둘이 동시에 간다. 같은 노드의 중복 요청은 합친다.
3. **오류는 그 노드에 머문다.** 실패한 노드는 ⚠ + 사유 + [다시 시도]. 다른 노드·편집기 세션·앱 상태에 영향 0. 로그 창에 한 줄.
4. **메타 조회는 사용자 트랜잭션과 다른 세션으로.** 편집기가 쓰는 세션에 끼어들지 않는다(잠금·트랜잭션·세션 변수 오염 방지).
5. **갱신은 사용자가 시킬 때.** F5/우클릭 "새로 고침"이 기본. 자동 갱신은 설정을 켠 사용자에게만, 펼쳐진 노드만, 유휴 때만.

---

## 1. 화면(DBeaver 캡처 대응 · 기본 UI)

```
┌ Connections ─────────────────── [🔍 filter] [⟳] ┐
│ ▸ 🔴 BISCM@PRD          61.81.244.214:1521      │  ← 프로필(접속 안 됨 = 흐리게 · ● 상태)
│ ▾ 🔴 BISCM_SB@DEV       192.168.0.58:1521       │
│   ▾ 📁 Schemas                                  │  ← 접속 시 1단계(스키마 목록)만
│     ▸ ADMIN                                     │
│     ▾ BISCM_SB                                  │  ← 펼침 = 그 스키마의 "구성" 폴더만
│       ▾ 📁 Tables            (loading… ◌)       │  ← 펼침 = 테이블 목록 1단계 · 스피너
│         ▸ ANTI_KR_ITEM                          │
│         ▸ ANTI_KR_ITEM_NEW                      │
│       ▸ 📁 Views                                │  ← Tables 로딩 중에도 즉시 펼침 가능(별도 스레드)
│       ▸ 📁 Procedures     ⚠ ORA-00942 [retry]   │  ← 실패는 그 노드에만
│       ▸ 📁 Functions · Packages · Sequences · Triggers · Indexes · Synonyms · Types │
└──────────────────────────────────────────────────┘
```

- 컨트롤 = **nexa-grid 계층 그리드**(nexa-ui 21 · FileGrid와 같은 `RowSource` + depth 글리프) · `CatalogSource` 하나 추가. 컬럼 = 이름 · (선택) 종류/행 수/수정 시각(펼친 뒤 채움).
- 위치 = 접속 패널 아래(왼쪽 사이드바 · 접기 가능) · 폭 조절. 더블클릭 = 테이블이면 편집기에 `SELECT * FROM …` 템플릿 · 프로시저면 `DESC`/소스(후속).
- 탐색기 노드도 [nexa-ui 21 §3-3](../../nexa-ui/docs/21-grid-family.md)와 같은 **툴팁 카드**(종류 · 스키마 · 소유자 · 행 수 · 마지막 조회 시각 · 설정 `tabs.tooltip`과 같은 토글 `explorer.tooltip`).

---

### 1-1. 키보드(DBeaver 내비게이터 · SWT/Windows 트리 관례 · 단축키 제외 · 사용자 09-19)

| 키 | 동작 |
|---|---|
| ↑ / ↓ | 위/아래 행 |
| → | 접힌 노드 = 펼침 · 펼쳐진 노드 = **첫 자식으로** |
| ← | 펼쳐진 노드 = 접기 · 잎/접힌 노드 = **상위로**(루트는 그대로) |
| Backspace | 상위로 |
| Home / End | 첫 / 마지막 행 |
| PageUp / PageDown | 보이는 행 수만큼 |
| Enter | 기본 동작(테이블 = SELECT 템플릿 탭 · 소스 있는 객체 = 소스 · 컬럼 = 이름 복사 · 그 밖 = 펼침/접힘) |
| `+` / `-` | 펼침 / 접힘 |
| `*` | 선택 노드 아래 **읽어 둔** 하위 전부 펼침(안 읽은 폴더는 요청만 · 다음 `*`로 이어서) |
| 글자 | **타입어헤드**(nexa-beep 이식 · nexa-ctl 부품 · 설정 `explorer.typeahead*`): 접두 누적(타임아웃 2000ms · 설정) · 한글은 앱이 직접 조합(IME 탈피 · Windows는 한/영 키로 모드 전환 · 상태줄 고지) · 공백·특수문자 포함 여부 설정 · **↑/↓ = 접두 매치 안에서 순환**(순환 중 타임아웃 유지) · Backspace = 접두 축소(자모 단위) · Esc = 초기화 · HUD(3×3 위치 설정) · 대소문자 무시 |
| Backspace · `+` `-` `*` | 접두가 **비어 있을 때만** 위 표의 트리 명령 · 접두 입력 중엔 글자/축소로 |
| Esc | 열린 메뉴 닫기 |

nexa-ctl `TreeControl`(파일 대화상자·설정 창 트리)도 ←/→는 같은 규칙.

- **F5** = 선택 노드 하위를 조용히 다시 읽기(읽어 둔 것만 · 펼침·선택·스크롤 보존) · **Shift+F5** = 캐시를 버리고 새로 · 자동 갱신 규칙은 [57](57-explorer-refresh-after-ddl.md)(실행한 DDL · 커밋 시점 · 객체 없음 신호 · 유휴 워터마크).

### 1-2. 우클릭 ▸ 새로 고침 — 계층형(사용자 09-19 · journal 83차)

모든 노드의 우클릭 메뉴 끝에 **Refresh**(구분선 아래 · F5와 같은 함수 `refresh_selected`). 범위는 **누른 자리가 정한다**:

| 누른 곳 | 다시 읽는 것 |
|---|---|
| 서버(루트) | 스키마 목록 + **읽어 둔** 폴더·객체 전부 |
| 스키마 | 그 아래 읽어 둔 폴더·객체 전부 |
| 종류 폴더(Tables 등) | 그 목록 + 그 아래 읽어 둔 객체(컬럼 등) 전부 |
| 객체(테이블 등) | **그 객체만**(컬럼) — 형제는 건드리지 않는다 |
| 컬럼·잎 | 속한 객체만 |
| 아직 안 읽은 폴더 | 새로 읽기(Loading) |

읽지 않은 가지는 펼치지 않는다(비확산 원칙 §3) · 조용한 디프 갱신([57](57-explorer-refresh-after-ddl.md))이라 펼침·선택·스크롤이 유지된다 · Shift+F5 = 캐시를 버리고 새로.


## 2. 모델 — 4계층 경계

| 층 | 무엇 | 위치 |
|---|---|---|
| **Core 포트** | `trait Catalog { fn kinds(&self) -> &[NodeKind]; fn children(&mut self, parent: &NodePath) -> Result<Vec<CatalogNode>, DbError>; }` · `NodeKind { Database, Schema, Folder(ObjectType), Table, View, MaterializedView, Procedure, Function, Package, Sequence, Trigger, Index, Synonym, Type, Column, Parameter }` · `CatalogNode { kind, name, path, has_children: Option<bool>, meta: Vec<(String, String)> }` | `nsql-core`(의존 0 · 타입만) |
| **어댑터** | 방언별 구현 — 메타 조회 SQL을 어댑터가 안다(앱은 모른다): Oracle `ALL_USERS/ALL_OBJECTS/ALL_TAB_COLUMNS/ALL_ARGUMENTS` · MSSQL `sys.databases/schemas/objects/columns/parameters` · SQLite `sqlite_master`/`pragma_table_info` · PG `information_schema`/`pg_catalog` · ODBC `SQLTables/SQLColumns` · 드라이버 확장은 RPC 메서드 `catalog.children`([22](22-driver-extensions.md)) | `nsql-driver-*` |
| **Run** | `Explorer` 서비스 — 메타 전용 세션(프로필당 1개 · 지연 생성) · **요청 큐 + 스레드 풀(상한 N=4/프로필)** · 노드별 상태 머신 · 캐시(경로 → 자식 + 읽은 시각) · 취소 토큰 | `nsql-run::explorer` |
| **호스트** | 트리 그리드 · 스피너/⚠ · 이벤트 수신 · CLI `nsql cat` | GUI · CLI |

**CLI 동등**([27](27-cli-conventions.md) 규약): `nsql cat schemas` · `nsql cat tables [schema]` · `nsql cat columns <table>` · `nsql cat procs [schema]` — 같은 `Catalog` 포트 · 셸 `DESC`(T-52)도 이것을 쓴다.

---

## 3. 로딩 — 단일 레벨 · 병렬 · 비확산

```
사용자: 노드 펼침 ─▶ Explorer::expand(path)
   ├─ 캐시 있음 & 만료 아님 ──▶ 즉시 표시(재조회 없음 — 수동 갱신 전까지)
   ├─ 같은 path 진행 중 ────────▶ 합류(중복 요청 0)
   └─ 새 요청 ──▶ 스레드 풀(프로필당 ≤ 4 · 초과분은 큐) ──▶ 메타 세션 `Catalog::children(path)` ──▶ 이벤트
                                 ├─ Ok(children)  → 노드 = Loaded{n, at} · 자식 삽입(1단계만 · has_children은 "폴더면 true · 객체면 컬럼/파라미터 = lazy")
                                 ├─ Err(e)        → 노드 = Error{message} · 로그 창 1줄(Error) · **다른 노드·세션 무영향**
                                 ├─ 취소(접힘)     → 결과 버림 · 노드 = Idle
                                 └─ 패닉/스레드 죽음 → catch_unwind · 노드 = Error("internal") · 풀은 재보충
```

| 규칙 | 내용 |
|---|---|
| 단일 레벨 | `children(path)`는 **직계 자식만**. 폴더(Tables)의 자식 = 테이블 이름 목록. 테이블의 자식(컬럼)은 테이블을 펼칠 때. 행 수·크기 같은 무거운 메타는 **툴팁/속성 패널에서 요청 시** |
| 병렬 | 노드 종류별이 아니라 **요청별** 스레드(상한). Tables·Procedures를 연달아 펼치면 2 스레드. 상한 초과는 큐(FIFO) · 접히면 큐에서 제거 |
| 메타 세션 | 프로필당 별도 접속 1개(지연) · 편집기 세션과 분리(트랜잭션·`SET` 오염 0) · 접속 실패 = 탐색기 루트에 ⚠(편집기는 계속 됨) · 접속 해제 시 함께 닫음 · SQLite `:memory:`는 세션 공유 불가 → 편집기 세션에 **짧은 읽기만**(D-47) |
| 오류 비확산 | 어댑터 오류 · 타임아웃(기본 15s · `explorer.timeout`) · 패닉 전부 그 노드의 상태로만. 요청 스레드는 `catch_unwind` · 결과 채널 `try_send` · UI는 이벤트 드레인만(로그 허브와 같은 격리 원칙 · [26 §3](26-performance-architecture.md)) |
| 취소 | 노드를 접으면 취소 토큰 set → 어댑터가 지원하면 `cancel`(T-3) · 아니면 결과를 버림 |
| 계측 | 각 로드 = `Timeline`(Send/Execute/Fetch · note "catalog") → 로그 창 · 툴팁에 "마지막 조회 123ms" |
| 정렬·필터 | 이름 자연 정렬 · 필터 상자 = 로드된 노드 이름 부분 일치(서버 재조회 없음) · 대량(테이블 1만+)은 그리드 가상화가 감당 |
| 캐시 | 프로필·경로 키 · 메모리만(세션 동안) · 접속 해제 시 비움 · 수동 갱신 = 그 노드 하위 캐시 무효화 |

---

## 4. 갱신 정책(설정)

| 키 | 기본 | 뜻 |
|---|---|---|
| `explorer.auto_refresh` | **off** | on이면 **펼쳐진 노드**만 주기 재조회(접힌 노드·자식은 안 함) |
| `explorer.refresh_secs` | 300 | 자동 갱신 주기(60~3600) · 앱이 유휴(입력·실행 없음 5s)일 때만 · 실행 중 세션과 분리돼 있어도 서버 부하를 위해 순차(동시 1) |
| `explorer.timeout` | 15 | 노드 로드 타임아웃(초) |
| `explorer.tooltip` | on | 노드 툴팁 카드 |
| 수동 | — | 노드 F5 · 우클릭 "새로 고침(이 노드)" / "하위 전체 새로 고침" · 루트 ⟳ = 스키마 목록 |

---

## 5. 방언별 노드 종류(1차)

> ★ 09-25 갱신: 2차 트리(DBeaver 대조 · 폴더 순서 · 객체 아래 하위 폴더 18종 · DB 수준 폴더 · 유효성 배지 · Generate SQL)는 **[83](83-object-explorer-dbms-trees-and-generate-sql.md) §1~§2가 원장**이다 — 아래 표는 1차 기록.

| 방언 | 루트 아래 | 스키마 폴더 |
|---|---|---|
| Oracle | Schemas(`ALL_USERS`) | Tables · Views · Materialized Views · Sequences · Procedures · Functions · Packages · Triggers · Indexes · Synonyms · Types · DB Links |
| SQL Server | Databases → Schemas | Tables · Views · Procedures · Functions · Triggers · Indexes · Types · Synonyms |
| PostgreSQL | Databases → Schemas | Tables · Views · Materialized Views · Sequences · Functions · Types · Extensions |
| MySQL | Databases(=Schemas) | Tables · Views · Procedures · Functions · Triggers · Events |
| SQLite | (파일) | Tables · Views · Indexes · Triggers |
| ODBC/확장 | 어댑터가 `kinds()`로 신고 | 어댑터가 정한다(manifest) |

객체 아래: Table → Columns · Indexes · Constraints · Triggers(각각 지연) · Procedure → Parameters · Source(요청 시).

---

## 6. 결정 · 작업

| # | 결정 | 권장 |
|---|---|---|
| **D-46** | 메타 전용 세션 — 프로필당 별도 접속(권장 · 트랜잭션 분리) / 편집기 세션 공유(접속 수 1 · DBA 계정 제한 환경) — 설정 `explorer.session = separate\|shared`로 둘 다 두되 기본은? | `separate` 기본 · 공유는 설정 |
| **D-47** | 세션 공유가 불가피한 방언(SQLite `:memory:`)·설정에서 **편집기 실행 중 메타 요청은 대기**(끼어들지 않음) / 거부 | 대기(큐) |

| ID | 항목 | 의존 |
|---|---|---|
| **T-56** | `Catalog` 포트(core) · Oracle/MSSQL/SQLite 어댑터 `children` · `nsql-run::explorer`(메타 세션 · 스레드 풀 · 상태 머신 · 캐시 · 취소 · 계측) · `nsql cat` · 트리 그리드(nexa-grid `CatalogSource` · 스피너 · ⚠ retry · 툴팁) · 설정 4키 · 자동 갱신 | nexa-ui G-1~G-3 · T-3(취소) |

## 7. 검색창(객체 필터 · 사용자 09-25 "객체가 많아 고르기 어렵다 — 상단 검색창? 서버별? 전체?")

### 7-1. 결정 · 근거
- **패널 맨 위 필터 상자 하나**(프로젝트·북마크·확장 패널과 같은 `filterbar` 부품 = Aa · ab · (.*) · × · 검색어 이력 `filter.explorer`) — 다른 층과 같은 자리·같은 손놀림.
- 범위 = **전 서버**(기본 · 설정 `explorer.filter_scope=all`) — 이어 붙인 한 트리에서 상자 하나가 자연스럽고, 서버 헤더가 "연결 N개 · 일치 M"으로 서버별 결과를 보여 주므로 서버별 상자를 따로 둘 이유가 없다. 키보드 대상 서버만 거르고 싶으면 `shown`(마지막으로 누른 서버).
- 서버별 편의 = 헤더의 일치 수 + Enter/⇧Enter로 일치 사이 이동(키보드 대상 서버 안) + 필터 중에도 다른 서버 헤더는 그대로.

### 7-2. 규칙(`Explorer::apply_filter` · 구현 ✅ §192)
- 대상 = 객체·컬럼·잎·스키마 이름(폴더·하위 폴더 라벨은 대상 아님 — "Tables"가 `b`에 걸리지 않게). 매칭 = 부품 규칙(옵션 없음 = 공백으로 나눈 낱말 전부 포함 · 대소문자 무시 · 한글 자모 · 옵션 켜면 정규식).
- 남는 노드 = 일치 **또는** 일치 자손이 있음(조상) **또는** 아직 안 읽은 컨테이너(스키마·폴더·하위 폴더 — 내용을 모르므로 남기고, 펼치면 그 안을 거른다). 객체는 안 읽었어도 이름으로 판정.
- 일치 자손을 품은 읽어 둔 노드는 **펼친다**(일치가 보이게) · 일치 글자는 옅은 강조 띠 · 헤더에 일치 수.
- 키: ⌘F/Ctrl+F(탐색기 포커스) = 상자로 · Enter = 다음 일치 선택 · ⇧Enter = 이전 · Esc = 지우고 트리로 · 이력 끝 ↓/Tab = 트리로.
- 트리가 새로 읽히면(펼침·갱신) 같은 필터를 다시 적용(`ExplorerSet::drain`).

### 7-3. 2차(T-209)
- 안 읽은 객체까지 찾기 = 메타 저장소 버킷(`MetaStore` · 접속 직후 미리 읽기)에서 이름 일치를 세어 폴더에 "일치 N(안 읽음)"을 붙이고 클릭 시 읽기 · 서버별 범위 칩(틀 오른쪽 토글) · 일치 없는 서버 헤더 접기.

## 8. 객체 상세 패널(✅ §207 · [86](86-object-details-panel.md))
- 탐색기 아래 독립 영역 · 유형별 섹션(속성·컬럼·제약/인덱스/트리거/인자·소스/DDL) · 읽기 전용 고정폭 본문(선택·복사·스크롤) · 머리 줄 ▾/▴ 축소·확장 · 복사 = 설명 / Shift = 종류-이름-설명 · View ▸ Object Details · `explorer.details*`.

### 7-5. 검색 대상 = 서버 전체(✅ §197 · [84](84-explorer-search-index.md))
- 트리에 읽힌 것만 거르던 한계 제거 — 이름 인덱스(스키마 순차) → 일치를 부분 폴더(`n/?`)로 즉시 → 순차 완성(`n/전체`) · 헤더 `인덱싱 d/T` · 유휴 선적재 · 설정 `explorer.search_index*`.

### 7-4. 규칙 v2(사용자 09-25 · 구현 ✅ §193)
- 남는 노드 = **자기 일치** 또는 **자손 일치**(조상) 또는 일치 노드의 자손 — 그 밖(미일치 · 아직 안 읽은 폴더 포함)은 모두 숨김. 최소 표시 레벨 = 서버 헤더 + 연결 행(루트) · 일치 0이면 헤더에 "일치 0".
- 개수 = `일치/전체`(`Tables (3/449)`) · 필터가 대신 펼친 노드는 해제 때 다시 접힘(사용자가 직접 누른 노드는 그대로) · 판정기는 탐색기가 보관하고 자식이 바뀔 때마다 스스로 다시 거른다(새 연결·펼침 포함).
