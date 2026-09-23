# 77. 데이터 작업대 구조 — 객체 정보·바로가기·그리드 편집·필터·내보내기를 관통하는 설계 (2026-09-23)

> **요청**(사용자 09-23): *"할일 정리 — 1) 개발 중인 기능 점검·완성도 향상 2) 인텔리센스 완성 3) 객체 추가 정보 조회(컬럼 타입 · 테이블/컬럼 정보 바로 확인) 4) 객체 탐색·바로가기(Ctrl+클릭 = 테이블 정보) 5) 그리드 데이터 편집/추가/삭제 6) 그리드 필터 · 조회용 Query 복사 7) Excel 인스턴스/파일로 내보내기 — 프로그램 전체를 관통하는 개념과 구조가 만들어지도록 설계에 반영하고 할 일에 추가."*
>
> 관련: [47 인텔리센스 메타](47-intellisense-metadata.md) · [76 IntelliSense·아웃라인](76-intellisense-and-outline.md) · [43 페치 모델·결과 탭](43-fetch-model-and-result-tabs.md)(DR-33) · [41 결과 → SQL 키 규칙](41-sql-copy-key-rules.md) · [34 트랜잭션 UX](34-transaction-ux.md) · [52 세션 모드](52-session-modes.md)(DR-34) · [28 오브젝트 탐색기](28-object-explorer.md) · [30 아키텍처 패턴](30-architecture-patterns.md) · [39 자원 거버넌스](39-resource-governance.md) · [72 제약 원장](72-size-limits-and-large-file-constraints.md).

## 0. 한 줄 결론

7가지 요구는 **두 개의 척추**로 묶인다. **① 객체 척추** — "편집기의 이름 · 탐색기의 노드 · 결과 그리드의 열"이 모두 같은 **객체 참조(`ObjectRef`)** 로 풀리고, 그 참조 하나로 **메타 저장소(`MetaStore`)** 에서 정보를 꺼내 **같은 동작 목록(`ObjectAction`)** 을 어디서나(완성 팝업 · hover 카드 · Ctrl+클릭 · 우클릭 · 팔레트 · F4) 낸다. **② 데이터 척추** — 결과는 **한 세트(`ResultData`) + 투영(`View`)** (DR-33)이고, 그 위에 **필터 술어**와 **변경 집합(`ChangeSet`)** 을 얹어 "보기 · 거르기 · 고치기 · 문장으로 만들기(`SqlGen`) · 실행(gate) · 내보내기(`Export` 싱크)"가 **한 파이프라인**을 탄다. 새 기능은 이 두 척추의 어느 마디에 붙는지 먼저 정한다(30 §1-2 체크리스트에 추가).

```
객체 척추   이름/노드/열 ──▶ ObjectRef ──▶ MetaStore(단일 원천 · 지연 채움) ──▶ ObjectAction(정보 · DDL · 데이터 · 열 · 탐색기 표시 · 이름 복사)
                                       ▲                                      ▲
                     탐색기 feed · 완성 · 아웃라인 · hover · Ctrl+클릭 · 객체 정보 탭 · 팔레트 — 전부 같은 두 문을 통과
데이터 척추 실행 ──▶ ResultData(세트) ──▶ View(정렬·필터 투영) ──▶ ChangeSet(행 편집 버퍼) ──▶ SqlGen(방언 Caps · 키 규칙 41) ──▶ gate_open 실행(34 · 52)
                                          │                                                  └──▶ 미리보기 SQL(복사 · 편집기로)
                                          └──▶ Export 싱크(클립보드 · CSV/TSV/JSON/SQL/XLSX 파일 · Excel 인스턴스)
                                          └──▶ "조회용 Query 복사" = 결과 출처 문장 + 필터 술어 → WHERE 합성
```

## 1. 객체 척추

### 1-1. `ObjectRef` — 객체를 가리키는 한 값

| 필드 | 뜻 | 원천 |
|---|---|---|
| `conn` | 접속 좌표(방언 · 호스트 · 포트 · DB · 프로필 이름) | `ConnectSpec`(52) · 북마크 `DocKey::Object`와 같은 규칙(69 §2-2) |
| `schema` `kind` `name` | 스키마 · 종류(테이블·뷰·프로시저·패키지·시퀀스·컬럼…) · 이름 | 탐색기 `NodeKind` · nsql-core `ddl_target`(57) |
| `part` | 부분(패키지 body · 인덱스 · 컬럼 이름 · 트리거) | 선택 |

**해석기(resolver)** = "지금 커서/클릭/열이 가리키는 것 → `ObjectRef`". 입력 셋: ① 편집기 = nsql-script `intel::context_at`의 낱말·alias 표(76 §3)로 `schema.table.column`까지 · ② 탐색기 노드 = 그대로 · ③ 그리드 열 = 결과 출처 문장(DR-33 `Grid.source`)의 FROM/alias 표 + 드라이버가 준 열 메타(테이블·스키마가 오면 우선). 해석이 애매하면(같은 이름 여럿) 후보 목록을 팝업으로 — 억지로 고르지 않는다.

### 1-2. `MetaStore` — 단일 원천(47 · 98차 §117에서 시작)

지금: 탐색기 응답이 스키마/객체/컬럼을 저장 · `Req::ColumnsMeta` 즉시 채움 · 접속 해제 시 비움 · 64 MB 상한. 더할 것:

| 정보 | 용도 | 채우는 때 |
|---|---|---|
| 컬럼 **타입 · 길이/정밀도 · NULL 허용 · 기본값 · 순서 · 코멘트** | 완성 상세 열 · hover 카드 · 객체 정보 탭 · 그리드 편집의 타입 검사 | 컬럼 요청 때 한 번(드라이버 `columns_of`에 항목 추가) |
| **키**(PK · UK · FK 대상) | 그리드 편집의 행 식별(41 키 규칙) · hover "PK" 표식 · 바로가기(FK → 대상 테이블) | 테이블 정보 요청 때 |
| 테이블 코멘트 · 행 수 추정 · 생성/변경 시각 | 객체 정보 탭 머리 | 요청 때(비싼 것은 버튼으로) |
| 프로시저/함수 **시그니처**(인자 이름·타입·방향) | 완성 시그니처 도움 · hover · 실행 창 | 요청 때 |
| 지연 채움 `Pending` 표식 + 완료 알림 | 팝업/카드가 "불러오는 중…" → 도착하면 갱신(98차 `NeedColumns`와 같은 길) | — |

원칙: **드라이버 포트 하나**(`Session::describe(ObjectRef) -> ObjectInfo` · 4 드라이버 + 확장 ABI 22)로 채우고, UI는 `MetaStore`만 읽는다. 세션 통제(DR-34 `gate_open`)를 지나며, 실행 중이면 큐에 넣고 상태줄에 안내.

### 1-3. `ObjectAction` — 어디서나 같은 동작 목록

| id | 뜻 | 어디서 |
|---|---|---|
| `obj.info` | **객체 정보 탭**(뷰 탭 `ext_view`와 같은 자리 · 열 표(타입·NULL·기본값·키·코멘트) · 인덱스 · 제약 · DDL 미리보기 · "데이터 200행" 버튼) | Ctrl+클릭 · F4 · hover 카드 버튼 · 탐색기 더블클릭 옵션 · 우클릭 · 팔레트 |
| `obj.ddl` | DDL을 편집기 탭으로(북마크 `DocKey::Object` 대상) | 탐색기 · 정보 탭 |
| `obj.data` | `SELECT * … 200`(페치 모델 43) | 탐색기 · 정보 탭 · hover |
| `obj.columns` | 완성 팝업에 열 목록 즉시 | 편집기 |
| `obj.reveal` | 탐색기에서 펼쳐 보이기(project `auto_reveal`과 같은 규칙) | 편집기 · 그리드 열 |
| `obj.copy_name` / `obj.copy_qualified` | 이름 / `schema.name` 복사 | 전부 |
| `obj.goto_fk` | FK 대상 테이블 정보로 | 정보 탭 · 그리드 열 |

레지스트리 = 포트(`ObjectAction` trait) + 목록 + 설정 선택(30 §1). **Ctrl+클릭**은 편집기에서 `resolver → obj.info`(설정 `intel.ctrl_click` = info/reveal/columns 중 선택) · 그리드 열 머리 Ctrl+클릭도 같은 길. **hover 카드**(설정 `intel.hover_ms`)는 `MetaStore` 요약 + 동작 버튼 두어 개(정보 · 데이터 · 탐색기) — nexa-ctl `draw_tooltip` 규칙(팝업 규칙 §2-2) 위에 버튼을 두려면 `HoverCard` 부품(30 §2) 하나를 만든다.

## 2. 데이터 척추

### 2-1. 지금 있는 것(DR-33 · 43 · 41)

`ResultData`(Arc 세그먼트) → `View`(행·열 인덱스 투영 · 정렬) → 그리드 · 복사(41 키 규칙) · 텍스트 보기 · CLI 렌더러 하나. 결과 탭마다 **출처 문장**(`Grid.source` · Σ 건수의 근거). 실행 통제 = `Sess::blocked`/`gate_open`(52).

### 2-2. 필터 술어 — `View`의 두 번째 투영

| 항목 | 설계 |
|---|---|
| 모델 | `Predicate { col, op, value }`의 AND 목록(OR는 2차) · op = `=` `≠` `<` `≤` `>` `≥` `contains` `starts` `ends` `regex` `is null` `is not null` `in(…)` |
| 적용 | 클라이언트 측 `View` 재계산(세트는 그대로 · DR-33 복사 0) · 페치가 더 오면 다시 적용 · 상한(72)은 세트 크기 규칙 그대로 |
| UI | 열 머리 깔때기 아이콘 → 값 목록(distinct 상위 N + 검색 상자 = `FilterBar` 부품 재사용 · 한글 자모) · 필터 줄(그리드 위 한 줄 · 칩 · × 지우기) · 상태줄 "n / N 행" |
| **조회용 Query 복사** | `source` 문장을 서브쿼리로 감싸 술어를 방언 문법으로 합성: `SELECT * FROM ( <source> ) q WHERE q."COL" = '…'`(리터럴 인용 = 41 §키 규칙 · 바인드 대신 리터럴) · "필터를 서버에 다시 조회"(`Requery` 43 §11)도 같은 합성 |

### 2-3. `ChangeSet` — 그리드 편집 버퍼(데이터 직접 관리)

| 항목 | 설계 |
|---|---|
| 편집 가능 판정 | 출처 문장이 **단일 테이블 SELECT**(JOIN·GROUP·DISTINCT·집합 연산 없음 · nsql-script 분해)이고 **행 식별 키**가 있을 때: PK/UK(MetaStore) → 없으면 Oracle `ROWID`/PG `ctid`/SQLite `rowid`/MSSQL `%%physloc%%`(자동으로 숨은 열 추가해 재조회) → 그것도 없으면 "전체 열 = 값"으로 갱신하되 1행 확인 · 읽기 전용 이유를 상태줄에(뷰 · 식별 불가 · PROD 연결 `tx.prod_*`) |
| 모델 | `ChangeSet { inserts: Vec<Row>, updates: Map<RowKey, Map<Col, (old, new)>>, deletes: Set<RowKey> }` — 세트를 고치지 않고 **덧그린다**(셀 색 = 바뀜/새 행/삭제 취소선 · Esc = 그 셀 되돌림 · Ctrl+Z = 편집 되돌리기 층은 nexa-ctl `Op` 기록과 별개인 그리드 자체 기록) |
| 입력 | 셀 더블클릭/F2/타이핑 = 인라인 `TextBox`(타입에 따라 숫자·날짜 검증 · NULL = Ctrl+0 · 큰 텍스트/CLOB = 값 편집 창) · 행 추가 = `+`(툴바 있음) · 삭제 = `−` · 복제 |
| **`SqlGen`** | `Caps`(89차 · 방언 능력표) + 41 키 규칙으로 `INSERT`/`UPDATE … WHERE 키`/`DELETE` 생성 · 값 리터럴/바인드 · 날짜는 진짜 바인드(T-151) · **미리보기 창**(생성 SQL 전문 · 편집기로 보내기 · 복사) |
| 적용 | ✓(툴바 있음) = 한 트랜잭션으로 순서(delete → update → insert) 실행 · `gate_open` · 자동 커밋 모드면 끝에 커밋(T-155 규칙) · 수동이면 커밋/롤백 UX(34) · 결과 = 갱신 행 수 · 실패 시 그 문장부터 중단 + 셀 표시 · 성공 뒤 **재조회 또는 로컬 반영**(설정 `grid.edit_refresh`) |
| 안전 | PROD 연결 확인(`run.prod_confirm`) · 영향 행 수 0/2+ 이면 중단(`grid.edit_strict`) · 트랜잭션 잠금 방지(56) 규칙 그대로 |

### 2-4. `Export` — 내보내기 싱크

| 싱크 | 설계 |
|---|---|
| 클립보드 | 있음(41) · 탭 구분/CSV/SQL INSERT 선택 |
| 파일 | CSV/TSV/JSON/SQL INSERT/**XLSX**(자체 writer — zip + XML · 의존 0 지향 DR-3 · 셀 타입 숫자/날짜/문자 · 시트 이름 = 결과 탭 이름 · 상한 1,048,576행 안내) · 선택 행/열만 · 헤더 포함 · 인코딩(CJK `encoding_rs`) |
| **Excel 인스턴스**(Windows) | COM 자동화 `Excel.Application`(late binding `IDispatch` · `windows` crate = 드라이버 급 명시 예외로 원장 등재 · Excel 없으면 안내 + 파일 저장으로 폴백) · 새 통합 문서에 붙여 넣기(클립보드 경유 아님 · `Range.Value2` 2차원 배열 · 10만 행은 조각 전송) · 옵션: 헤더 굵게 · 열 폭 자동 · 표(ListObject)로 |
| 공통 | 진행 카드(실행 카드 스택 재사용) · 취소 · 큰 세트는 스트리밍(세그먼트 단위) · 설정 그룹 `export.*` |

## 3. 관통 규칙(30 §1-2 체크리스트에 더한다)

1. 객체를 다루는 새 기능은 **`ObjectRef`로 받고 `MetaStore`에서 읽고 `ObjectAction`으로 낸다** — 자기 조회·자기 메뉴를 만들지 않는다.
2. 결과를 다루는 새 기능은 **`View` 투영 또는 `ChangeSet` 위**에 놓고, 서버에 보내는 문장은 **`SqlGen` 한 곳**에서 만든다(41 키 규칙 · `Caps`).
3. 서버로 가는 길은 전부 `gate_open`(52) · PROD 확인 · 트랜잭션 UX(34) · 로그(48) · 상한(72) · 부하원(39) 등재.
4. 명령 id 규약: `obj.*` `grid.*` `export.*` · 팔레트/메뉴/키/우클릭이 같은 id · 설정 키는 `intel.*` `grid.*` `export.*` 그룹.
5. 실기 확인표(U-*)와 자동 점검(`win-func-check`) 시나리오를 기능마다 하나씩.

## 4. 할 일(순서 · TODO 등재)

| # | 항목 | 척추 | 선행 | 규모 |
|---|---|---|---|---|
| **T-177** | 개발 중 기능 점검·완성도(99차 U-81~U-96 실기 결과 반영 · 검색/이력/필터/아웃라인 마감) | — | — | 중 |
| **T-178** | **인텔리센스 완성**(76 §7 2차 = 시스템 객체 패키지 · 시그니처 · alias 자동 · JOIN/INSERT 생성 · 팝업 스크롤(71차 부품) · 디스크 캐시 · 워커) + `MetaStore` 확장(§1-2 타입/키/코멘트/시그니처) | 객체 | — | 대 |
| **T-179** | 객체 정보 조회(`Session::describe` 포트 · `ObjectInfo` · **객체 정보 탭** · hover 카드 부품) | 객체 | T-178 메타 | 중 |
| **T-180** | 객체 탐색·바로가기(`resolver` · **Ctrl+클릭** · F4 · `obj.*` 액션 레지스트리 · 그리드 열 머리 → 객체 · FK 따라가기) | 객체 | T-179 | 중 |
| **T-181** | 그리드 필터 + 조회용 Query 복사(`Predicate` 투영 · 열 머리 깔때기 · 필터 줄 · `Requery` 합성) | 데이터 | — | 중 |
| **T-182** | 그리드 데이터 편집/추가/삭제(`ChangeSet` · 편집 가능 판정 · 인라인 편집 · `SqlGen` 미리보기 · 트랜잭션 적용 · 되돌리기) | 데이터 | T-179 키 메타 · T-181 | 대 |
| **T-183** | 내보내기(파일 CSV/TSV/JSON/SQL/**XLSX** 자체 writer · **Excel 인스턴스** COM · 진행 카드 · 설정 `export.*`) | 데이터 | — | 중~대 |

결정 대기: **D-197** `ObjectRef`에 접속 좌표를 넣는 방식(북마크 69 §2-2 재사용 vs 세션 id) · **D-198** 편집 가능 판정에서 키 없는 테이블의 "전체 열 = 값" 허용 여부(기본 = 1행 확인 뒤 허용) · **D-199** Excel COM을 위한 `windows` crate 도입(명시 예외 원장 · Windows 전용 feature · 없으면 파일 폴백) · **D-200** XLSX writer 자체 구현(zip deflate 포함 · 의존 0) vs 외부 crate.
