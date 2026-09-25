# 86. 객체 상세 패널 — 탐색기 아래 독립 영역 · 유형별 섹션 · 선택·복사 · 축소/확장(2026-09-25 · 100차 mac · T-223)

> 사용자 09-25(다섯 메시지): "View ▸ 객체 상세 = 탐색기를 가로로 나눠 아래에 선택 객체 상세 뷰 · 표시 + 복사 두 목적 · 유형별 내용은 다른 프로그램 참고해 초안" ·
> "우측 줄이기(▾) = 1줄로 줄여 Description만 + 끝에 복사 버튼 · 클릭 = 설명 복사 · 조합키+클릭 = 유형-이름-설명 · 복사됨 효과" ·
> "기본 = 유형별 내용 · 상하/좌우 스크롤 · 선택·복사/붙여넣기 UI · 헤더 = 펼침 ▾ 축소 / 축소 ▴ 확장" · "탐색기와 분리된 독립 영역(스크롤 겹치지 않게)".
> 원천 = [83](83-object-explorer-dbms-trees-and-generate-sql.md)(하위 폴더 표 · Generate SQL) · [85 §4](85-metadata-layers.md)(L3 즉시 채움·회수) · [77](77-data-workbench-architecture.md)(객체 척추).

## 0. 결론

- **독립 패널** `objdetail.rs`(`DetailPanel`): 탐색기 칸 **아래**, 호스트(`main.rs`)가 배치 — 탐색기 높이를 그만큼 줄이고 사이에 스플리터(`split_d` · 끌면 `explorer.details_h` 기억). 탐색기 스크롤 영역과 겹치지 않는다.
- **머리 줄**(UI 글꼴 · 1줄) = 종류 칩 · **설명**(§209 · 코멘트 없으면 이름) · **▼**(펼침 상태 = 축소) / **▲**(축소 상태 = 확장 · 채운 삼각형) · **복사 버튼**(맨 끝). 축소 = 머리 줄만(설정 `explorer.details_collapsed` 기억).
- **본문**(고정폭 · 편집기 글꼴 · `Editors::preview_box`와 같은 설정 = 글꼴 지표·줄 간격 일치 · §209) = **읽기 전용 텍스트박스**(nexa-ctl `TextBox`) → 선택 · ⌘/Ctrl+C 복사(`edit.copy`) · 상하/좌우 스크롤(줄바꿈 없음) · 유형별 섹션 글.
- **복사 버튼** = 클릭 → 설명(Description · 없으면 이름) · **Shift+클릭** → `종류 - 이름 - 설명` · 복사됨 효과 = `CopyBtn`(1 s 체크 표시 · 다른 복사 버튼과 같은 부품). 조합키 = **Shift**(마우스 사건이 Shift·⌘/Ctrl만 싣고 ⌥/Alt는 없다 · Shift = "더 넓게" 관례).
- **데이터** = `nsql_catalog::object_details(session, &ObjectInfo, GenOpts)` → 섹션 목록(§3) · 탐색기 메타 스레드 `Req::Details`(급한 세션 · 사용자가 보고 있다) → `ExplorerAction::Details` → 패널. CLI `nsql cat detail <객체> [종류]`가 같은 함수(실서버 점검).

## 1. 타 도구 조사(초안 근거 · 기억 기반 요약 · ⚠ 확인 필요)

| 도구 | 위치 | 탭/섹션(테이블) | 그 밖 유형 |
|---|---|---|---|
| **DBeaver** | 편집기 자리 "객체 편집기" 탭 + 네비게이터 아래 **Properties/Description 패널**(옵션) | Properties · Columns · Constraints · Foreign Keys · References · Triggers · Indexes · Partitions · DDL · Data · Virtual | 프로시저 = Properties · Parameters · Source · DDL / 뷰 = Columns · Source · DDL / 스키마 = 객체 수 |
| **Oracle SQL Developer** | 편집기 자리 객체 뷰어 | Columns · Data · Model · Constraints · Grants · Statistics · Triggers · Flashback · Dependencies · Details · Partitions · Indexes · SQL | 패키지 = Code · Errors · Grants · Dependencies · Details / 시퀀스 = Details · Dependencies · SQL |
| **SSMS** | Properties 대화상자 + 객체 탐색기 "Details" 창(⌘/F7 · 목록) | General(속성) · 컬럼·키·인덱스는 **하위 폴더** · Script as | 프로시저 = Properties · Parameters(하위 폴더) · Modify(소스) |
| **DataGrip** | 데이터베이스 탐색기 오른쪽 **Structure/Details 뷰**(빠른 문서) · 편집기 = Data · DDL | Columns(타입·기본·NULL·키) · Keys · Indexes · Foreign keys · DDL | 루틴 = Parameters · Source / 빠른 문서 = 이름·종류·코멘트·DDL 미리보기(**Ctrl+Q**) |
| **Toad(Oracle)** | Schema Browser 오른쪽 패널 | Columns · Indexes · Constraints · Triggers · Data · Script · Grants · Synonyms · Partitions · Subpartitions · Stats/Size · Referential · Used By · Policies | 프로시저 = Source · Errors · Dependencies · Grants |
| **TablePlus** | 하단/편집기 Structure 탭 | Structure(컬럼·인덱스·FK) · DDL | — |
| **pgAdmin** | 오른쪽 Properties/SQL/Statistics/Dependencies/Dependents 탭 | Properties · SQL(DDL) · Statistics · Dependencies | 함수 = Properties · SQL |

공통 = **속성(General) → 컬럼 → 제약/인덱스/트리거 → DDL·소스** 순 · 루틴은 **인자 + 소스** · 아래·오른콽에 붙는 **읽기 전용 텍스트/표**. 우리 초안은 이 순서를 따르고, 유형별 하위 폴더 표(83 §1 = 트리와 같은 원천)를 그대로 섹션으로 쓴다(재발명 0).

## 2. 배치·상호작용

```
┌ 활동 막대 │ 객체 탐색기(검색창 + 트리)                         │ 편집기 …
│           │ …                                                │
│           ├──────── split_d(끌기 = explorer.details_h) ────────┤
│           │ [TABLE] BISCM.M4E_I301030  Description…   ▾  ⧉    │  ← 머리 줄(UI 글꼴 · 26 px)
│           │ == Properties (5) ==                              │  ← 본문(고정폭 · 읽기 전용 · 선택/복사/스크롤)
│           │ Property  Value                                   │
│           │ …                                                │
```
- 켜기/끄기 = View ▸ Object Details(팔레트 같음) · 설정 `explorer.details`(off 기본) · 탐색기가 닫혀 있으면 함께 연다.
- 축소(▾) = 머리 줄 하나 = 설명 + 복사 · 확장(▴) = 본문 복원 · `explorer.details_collapsed` 기억.
- 선택 추적 = 탐색기 선택이 바뀌면(`ExplorerSet::selected_target` · 마지막으로 누른 칸) 대상 교체 → 객체 = "상세를 읽는 중…" 뒤 섹션 · 컬럼/잎/스키마 = 즉시 속성만(서버 왕복 0).
- 포커스 = `Focus::Details`(패널 클릭) · 키(이동·선택)는 텍스트박스로 · ⌘/Ctrl+C = 선택 복사 · 편집 키는 읽기 전용이라 무시.
- 마우스 = 패널 안 사건은 탐색기보다 먼저(독립 영역) · 휠 = 본문 스크롤.

## 3. 유형별 섹션(§209 확정 = **인텔리센스·탐색기 범위 안** · `object_details`/`column_details` · 비는 섹션은 뺀다)

> 09-25 사용자 정정: 소스·DDL·컬럼 목록은 넣지 않는다(트리·Generate SQL 몫). 아래 표의 원안 가운데 굵은 항목만 남았다.

| 대상 | 섹션(순서) | 근거 |
|---|---|---|
| **테이블**(외부/외래 포함) | Properties(**이름 · 설명=코멘트**) → **Primary Key**(이름 · 컬럼) → **Indexes**(이름 · 컬럼 · UNIQUE) — `table_detail` 질의 2 | DBeaver·SQL Developer·DataGrip 공통 순서 |
| **뷰·MV** | Properties(이름 · 설명) → **사용 테이블**(Dependencies 하위 표 · 방언이 지원할 때) | SQL Developer Dependencies |
| 인덱스 | Properties → Columns(인덱스 컬럼) → DDL | Toad Indexes 탭 |
| **프로시저·함수·패키지** | Properties(이름) → **Arguments**(이름 · 타입/방향/기본값) · 패키지 = Procedures/Functions | SQL Developer · DataGrip Parameters |
| 시퀀스·시노님·기타 | Properties → 하위 폴더 표 → DDL(있으면) | pgAdmin SQL |
| **컬럼**(트리에서 컬럼 선택) | Properties(**이름 · 설명=컬럼 코멘트 · 타입 · NOT NULL · 기본값** · 테이블) — 탐색기 값을 즉시 그리고 코멘트만 질의 1(`column_details`) | DataGrip 빠른 문서 |
| **잎**(제약·인덱스·트리거·인자 …) | Properties(주인 · 종류 · 이름 · 부가 · 상태) — 왕복 0 | — |
| 스키마 | Properties(이름) → **Count**(종류별 객체 수 · 메타 L1/L2에서 · 왕복 0 · §208) | DBeaver 스키마 |

- 표 → 글 = `render_table`(열 폭 = 글자 수 최대 · 두 칸 띄움 · 머리글 밑줄) — CLI와 같은 모양이라 복사해 붙여도 정렬이 산다.
- 섹션 제목 = `== 라벨 (n) ==`(i18n · 하위 폴더 라벨은 탐색기와 같은 `sub_msg`).

## 4. 복사 규칙

| 동작 | 결과 | 예 |
|---|---|---|
| 복사 버튼 클릭 | 설명(Description) · 없으면 `스키마.이름` | `PIPELINED` / `BISCM.M4E_I301030` |
| **Shift**+클릭 | `종류 - 스키마.이름 - 설명` | `TABLE - BISCM.M4E_I301030 - …` |
| 본문 선택 + ⌘/Ctrl+C | 선택 글 | 표 일부 · DDL 일부 |
| 효과 | `CopyBtn` 체크 표시 1 s(다른 복사 버튼과 같음) · 실패 = 상태줄 | |

설명(Description)의 정의(§209·§211 확정) = 테이블·객체 = **코멘트, 없으면 이름** · 스키마 = 이름 · **컬럼 = 코멘트를 DBMS가 준 그대로**(공백 = 공백 · **NULL = 흐린 회색 NULL**) · 잎 = 부가, 없으면 이름. 머리 줄에는 종류 칩 옆에 **설명만** 보인다. 컬럼 코멘트는 **테이블 단위 캐시**(`Req::Comments` → `comments_raw` · 테이블을 고를 때 미리 · 컬럼을 처음 고를 때 한 번)라 같은 테이블의 컬럼은 이름 → 설명 교체 없이 즉시.

## 5. 층·부하(85 §4 L3)

- 선택마다 카탈로그 질의 = 속성 0 · 컬럼 1 · 하위 폴더 종류 수(비면 0행 응답) · 소스/DDL 1 → 테이블 ≈ 6~9 질의(각 수십 ms · 급한 메타 세션 · 26 §8 등재). 같은 객체를 다시 골라도 다시 읽는다(캐시 없음 = 최신 · 후속 = 짧은 LRU).
- 메모리 = 섹션 글 하나(수 KB~수백 KB DDL) · 패널을 끄면 대상 비움.

## 6. 설정

| 키 | 기본 | 뜻 |
|---|---|---|
| `explorer.details` | off | 패널 켬(View ▸ Object Details) |
| `explorer.details_h` | 240 | 높이(px · 스플리터 끌기 기억 · 80~1200) |
| `explorer.details_collapsed` | off | 축소 상태 기억 |
| `gen.*` | (83 §3-1) | DDL 섹션의 생성 옵션 |

## 7. 자체 점검(09-25 · 키 주입 0)

- 단위 = `cargo test -p nsql-catalog detail`(`render_table` 정렬 · 소스 종류) · 실서버 CLI = `nsql cat -c BISCM -s BISCM detail M4E_I301030`(Properties · Columns 9 · Constraints · Indexes · DDL) · `nsql cat -c M4PLAN detail <proc> procedure`.
- GUI(격리 · 데모 SQLite · `explorer.details=on`) = `@after:1500:explorer.expand1,@after:2500:explorer.expand2,@after:3500:explorer.select3,@after:5000:details.dump:<파일>` → `== Properties (4) == … == Columns (3) == … == Constraints (1) ==`(dept) · 기동 명령 `explorer.select<row>` 신설.
- 실기 U-166 = View ▸ 객체 상세 → 테이블 클릭 = 섹션 · ▾/▴ · 복사 · Shift+복사 · 본문 선택 ⌘C · 스플리터 끌기 · 탐색기 스크롤과 겹치지 않음.

## 8. 남은 것(T-225)

- ~~스키마 객체 수 · 코멘트 = 설명 · 더블클릭 = 편집기 · ▾/▴ 도형~~ ✅ §208 · 남음 = 섹션 접기/탭 · 짧은 LRU · 컬럼 선택 시 주인 테이블 요약 · 속성 표 컬럼 코멘트 열은 코멘트가 있을 때만.
