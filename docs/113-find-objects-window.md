# 113 · 객체 찾기 창(Find Objects) — 연결 전체 · 컬럼 · 소스 본문 상세 검색

> **요구 원문(사용자 10-15)**: "객체 찾기 별도 모달리스 창: 연결된 전체 연결 대상 · 컬럼과 소스(View·Procedure 등) 본문까지 검색하는 상세 검색 · 검색 조건·결과 표시·간략 미리보기·편집기 탭 연계 · 사용자 친화 UX · 타 클라이언트와 기능·목표·성능·기술·기법·도구 비교 평가해 장점을 아울러서"
> **분담** = §1 타 도구 비교 = 협업 세션(Opus · 128차) · §2~ 설계 = 개발 세션(Fable)
> **짝 문서** = [28 §7](28-object-explorer.md) 탐색기 필터 · [84](84-explorer-search-index.md) 이름 인덱스 · [85](85-metadata-layers.md) 메타 3층 · [96](96-object-links.md) 객체 링크 · [88](88-ui-ux-standards-survey.md) UI/UX 표준 · [108](108-dbms-syntax-and-objects-survey.md) DBMS별 사전 뷰 · [36](36-find-in-files-and-project.md) 파일 검색 탭

## 1. 타 도구 비교

### 1-0. 조사 방법·신뢰도(10-15 · 협업 세션)

이 절은 **기억과 저장소 문서(88 · 108 · 28 · 84 · 36)에 기댄 비교**다. 이번 위임에서는 각 제품의 공식 문서·실물을 다시 열지 않았다. 표의 단축키·옵션 이름은 판마다 바뀌므로 **설계 근거로 쓸 때는 §1-11 신뢰도 표의 "확인 필요" 줄을 실물로 한 번 더 본다.**

| 표기 | 뜻 |
|---|---|
| **확신** | 여러 판에 걸쳐 오래 같은 동작이고 저장소 문서(88·108 등)에도 같은 사실이 있음 |
| **기억** | 써 본 기억·일반 지식 · 세부(옵션 이름·기본값)는 판마다 다를 수 있음 |
| **확인 필요** | 판·에디션(무료/상용)에 따라 있거나 없음 · 단축키·한계 수치 · 성능 기법(내부 구현 추정) |

### 1-1. 한눈 비교(요약)

| 도구 | 진입 | 창 형태 | 범위 | 대상(이름/컬럼/주석/소스/데이터) | 결과 | 미리보기 | 연계 |
|---|---|---|---|---|---|---|---|
| DBeaver | Search ▸ DB Search(Ctrl+H 계열) · Ctrl+Shift+F는 포맷 | 대화상자 → 결과 = **Search 뷰**(도킹 · 모달리스) | 연결 여러 개 · 카탈로그/스키마 선택 트리 | 이름 ○ · 컬럼(옵션) ○ · 주석(옵션) ○ · 소스(옵션 "search in definitions") ○ · 데이터 = 별도 "DB Full-Text" 탭 ○ | 목록/트리(연결 > 스키마 > 종류) | 없음(선택 = 탐색기 이동) | 더블클릭 = 편집기 열기 · Show in navigator |
| DataGrip | Go to Symbol(Ctrl+Alt+Shift+N) · Find in Path(Ctrl+Shift+F) | 팝업(Go to) · Find in Path = 팝업 + Find 도구 창(모달리스) | 인트로스펙션된 모든 데이터 소스 | 이름 ○(로컬 인덱스) · 컬럼 ○ · 주석 △ · 소스 ○(인트로스펙션된 소스 · "Database objects" 범위) · 데이터 = Full-text search(별도) ○ | 목록(일치 줄) · 파일/객체로 그룹 | **있음**(아래 미리보기 창 · 하이라이트 · 줄 번호) | 열기 · 탐색기에서 선택(Alt+F1) · 다음/이전 일치 |
| Oracle SQL Developer | View ▸ Find DB Object | **도킹 패널**(모달리스) | 연결 1 · 스키마 선택 · 종류 체크 | 이름 ○ · 컬럼 ○ · 주석 △ · 소스(ALL_SOURCE) ○ · 데이터 ✕ | **트리**(종류 > 객체 > 일치 줄) | 일치 줄 텍스트(노드 라벨) | 더블클릭 = 객체 편집기 · 소스 줄로 이동 |
| PL/SQL Developer | Tools ▸ Find Database Objects | **모달리스 창** | 연결 1 · 소유자 · 종류 체크 | 이름 ○ · 소스 본문 ○(대소문자 · 낱말 · 정규식 옵션) · 컬럼 △ · 데이터 ✕ | 목록(객체 · 종류) | 없음(대개 줄 위치) | 우클릭 = 객체 메뉴 그대로(편집 · 보기 · DDL) |
| Toad (Oracle) | Database ▸ Object Search | 모달리스 창 | 연결 1 · 스키마 다중 · 종류 다중 | 이름 ○ · 컬럼 ○ · 주석 ○ · 소스 ○ · 데이터 △(별도 기능) | 목록(스키마 · 이름 · 종류 · 일치 위치) | 없음/약함 | 결과 행 = 객체 메뉴 · 편집기 · 스키마 브라우저 |
| SSMS(내장) | Object Explorer Details(F7) 검색 상자 | 문서 창 | 선택한 노드 아래 1 DB | 이름 ○ · 나머지 ✕ | 목록 | 없음 | 탐색기 동기화 |
| Redgate SQL Search(SSMS 추가 기능 · 무료) | 도구 모음 버튼 · 단축키 지정 | **문서 창(모달리스)** | 서버 1 · **DB 전체/선택** · 종류 필터 | 이름 ○ · 컬럼 ○ · 소스 본문 ○ · 작업(Job)·SSIS 일부 ○ · 데이터 ✕ | 목록(종류 아이콘 · 이름 · 스키마 · DB · 일치 개수) | **있음**(아래 소스 창 · 하이라이트 · 일치로 이동) | "Select object in Object Explorer" · 수정 스크립트 열기 |
| TablePlus | Open anything(⌘P / Ctrl+P) | 팝업(퍼지) | 현재 연결 | 이름 ○ · 나머지 ✕ | 목록(퍼지 순위) | 없음 | Enter = 열기 |
| Navicat | 도구 ▸ Find in Database(Premium) · 객체 목록 검색 상자 | 창(모달리스) | 연결 1 · DB/스키마 선택 | 이름 ○ · 소스 ○ · 데이터 ○(검색 대상 선택) · 컬럼 △ | 목록(객체 · 일치) | 약함 | 더블클릭 = 열기 |

(○ 있음 · △ 판·에디션·옵션에 따라 · ✕ 없음)

### 1-2. 진입 방식·창 형태

- **모달리스 + 결과가 남는 창**이 객체 검색의 주류다(DBeaver Search 뷰 · SQL Developer Find DB Object · PL/SQL Developer · Toad · Redgate). 결과를 보며 편집기를 오가야 하기 때문이다. TablePlus·DataGrip Go to는 "이름만 빨리"용 팝업이라 목적이 다르다(우리 Ctrl+P = [Goto Anything](wiki/Goto-Anything.md)이 이 자리).
- 진입 단축키는 도구마다 제각각(Ctrl+H · Ctrl+Shift+F · F7 · 지정 없음). **DataGrip만 "이름 = 팝업 / 본문 = 도구 창"을 키로 나눈다** — 같은 검색어로 두 단계를 오가는 흐름이 매끄럽다.

### 1-3. 검색 범위

- 연결 여러 개를 한 번에 = **DBeaver · DataGrip만**. SQL Developer·PL/SQL Developer·Toad·Navicat은 연결 1개.
- SQL Server의 DB 단위 범위 = **Redgate SQL Search가 기준점**(DB 전체 또는 체크한 DB · 시스템 DB 제외 옵션). 내장 SSMS는 선택한 노드 아래 1 DB.
- 범위 선택 UI는 **트리 체크**(DBeaver · Redgate) 또는 **콤보 + 종류 체크박스**(SQL Developer · PL/SQL Developer · Toad)로 갈린다. 종류(테이블·뷰·프로시저·함수·패키지·트리거·시퀀스·시노님) 체크는 모든 상세 검색 도구에 있다.

### 1-4. 검색 대상

| 대상 | 대표 원천(서버 사전) | 가진 도구 |
|---|---|---|
| 객체 이름 | Oracle `ALL_OBJECTS` · PG `pg_class`/`pg_proc` · SQL Server `sys.objects` · MySQL `information_schema.TABLES/ROUTINES` | 전부 |
| 컬럼 이름 | `ALL_TAB_COLUMNS` · `pg_attribute` · `sys.columns` · `information_schema.COLUMNS` | DBeaver(옵션) · DataGrip · SQL Developer · Toad · Redgate |
| 주석(설명) | `ALL_TAB_COMMENTS`/`ALL_COL_COMMENTS` · `obj_description` · `MS_Description` | DBeaver(옵션) · Toad · (우리 127차 = 테이블·뷰 설명 · 28 §7-6) |
| 소스 본문 | Oracle `ALL_SOURCE`(줄 단위 · 4000자 조각) · PG `pg_get_functiondef`/`pg_proc.prosrc` · `pg_views.definition` · SQL Server `sys.sql_modules.definition` · MySQL `ROUTINES.ROUTINE_DEFINITION`/`VIEWS.VIEW_DEFINITION` | DBeaver · DataGrip · SQL Developer · PL/SQL Developer · Toad · Redgate · Navicat |
| 데이터(행 값) | 테이블마다 `LIKE` 질의 | DBeaver(Full-Text 탭) · DataGrip(Full-text) · Navicat — 무겁고 별도 기능으로 분리 |

- **Oracle `ALL_SOURCE`는 줄 단위**라 "일치 줄 번호"가 공짜로 나온다(SQL Developer 트리의 일치 줄 노드). SQL Server·PG·MySQL은 정의 전체가 한 덩어리 → 줄 번호는 클라이언트가 계산한다(Redgate가 이렇게 함 · 기억).
- 뷰 본문: Oracle `ALL_VIEWS.TEXT`는 LONG이라 서버 `LIKE`가 안 된다 → 클라이언트에서 거르거나 `DBMS_METADATA` · 도구들이 뷰 소스 검색을 옵션으로 빼는 이유(확인 필요).

### 1-5. 검색 조건

| 조건 | DBeaver | DataGrip | SQL Developer | PL/SQL Dev | Toad | Redgate | TablePlus | Navicat |
|---|---|---|---|---|---|---|---|---|
| 대소문자 | ○ | ○ | ○ | ○ | ○ | ✕(대개 무시) | ✕ | ○ |
| 낱말 단위 | △ | ○ | △ | ○ | ○ | ○(Exact match) | ✕ | △ |
| 정규식 | ✕ | ○ | △ | ○ | △ | ✕ | ✕ | ✕ |
| 와일드카드(`%`·`*`) | ○ | ○ | ○ | ○ | ○ | ○ | 퍼지 | ○ |
| 종류 필터 | ○ | ○ | ○ | ○ | ○ | ○ | ✕ | ○ |

- 우리 필터 틀(Aa · ab · (.*) · 설명 포함)은 이미 이 표의 위쪽 행을 다 갖고 있다 → 새 창도 **같은 `filterbar` 부품**을 쓰면 조건 UI가 공짜다(28 §7-1 · 61 §2-2-b).

### 1-6. 결과 표시·미리보기

- **트리형**(SQL Developer = 종류 > 객체 > 일치 줄) vs **목록형**(Redgate · Toad · PL/SQL Developer = 한 줄에 종류 아이콘 · 이름 · 스키마 · DB · 일치 수). 목록형이 정렬·필터·복사에 낫고, 트리형은 한 객체에 일치가 많을 때 낫다.
- **미리보기가 있는 도구는 둘뿐**: DataGrip(아래 미리보기 · 하이라이트 · 줄 번호 · 다음/이전) · Redgate(아래 소스 창 · 모든 일치 하이라이트 · 첫 일치로 스크롤). 사용자 평이 좋은 이유 = "열지 않고 맞는지 본다".
- 일치 위치 표시: SQL Developer = 줄 텍스트 · Redgate = 일치 개수 + 하이라이트 · DataGrip = 줄 조각(앞뒤 문맥 잘라서).

### 1-7. 연계

| 동작 | 가진 도구 | 우리 쪽 부품 |
|---|---|---|
| 편집기 탭으로 열기(소스) | 전부 | 소스 열기 `ObjectOrigin` · 보수적 생성(100 §5) |
| 일치 줄로 캐럿 이동 | DataGrip · SQL Developer · Redgate | 편집기 `goto_line` · 찾기 강조 |
| 탐색기에서 보기 | DBeaver · DataGrip · Redgate · SSMS | `RevealTarget` · `Explorer::step_reveal`(96 §7) |
| 객체 우클릭 메뉴 그대로 | PL/SQL Developer · Toad | 탐색기 메뉴 · `obj.*` 액션(96 §6-7) |
| 이름 복사 · 전체 이름 복사 | 대부분 | `copy_name_of` · `full_name_parts`(Shift) |
| 결과 내보내기(CSV) | Toad · DBeaver | 그리드 내보내기 |

### 1-8. 성능 기법

| 기법 | 쓰는 도구 | 비고 |
|---|---|---|
| 로컬 인덱스(사전 인트로스펙션) | **DataGrip**(이름·소스까지 로컬 · 서버 왕복 0 · 대신 인트로스펙션 지연·낡음) | 우리 L1 이름 인덱스(84) · 디스크 캐시(85 §L1) |
| 사용 시점 서버 질의(종류·스키마당 1회) | SQL Developer · PL/SQL Developer · Toad · Navicat · DBeaver | `ALL_SOURCE`에 `UPPER(TEXT) LIKE` = 큰 스키마에서 수 초~수십 초 |
| 처음 한 번 색인 + 증분 | **Redgate**(DB별 로컬 색인 · 처음 느리고 이후 즉시 · 변경 감지로 증분 · 확인 필요) | 우리 127차 = 검색 시작 때 DB 하나씩 Lazy · 현재 DB 먼저 |
| 백그라운드 + 점증 표시 + 취소 | DBeaver(작업 진행 · 취소) · DataGrip · Redgate | 우리 메타 스레드 `Req::Search` · 혜성 `BusyRing` · `LatestIntent`(99) |
| 결과 상한 | DBeaver("max results") · Toad | `explorer.index_hits_max`(84 §5) |

- 소스 본문 검색은 **서버 사전 질의 비용이 이름 검색의 수십 배**다(본문 전송). 이름·컬럼은 우리 L1/L2 메타로 즉시, 본문은 서버 질의(필요할 때만 · 상한 · 취소)로 나누는 것이 지연 로딩 원칙(61 §1-8)과 맞다.
- **세션·작업 분할 관점(10-15 보강 · 기억 기반)**:

| 관점 | DataGrip | Redgate SQL Search | DBeaver | 시사점(우리) |
|---|---|---|---|---|
| 검색 전용 세션 | 인트로스펙션은 데이터 소스별 별도 연결(기억) | SSMS 연결과 별개 연결로 색인(**확인 필요**) | 메타 읽기 = 별도 "메타 연결" 옵션(Separate connection for metadata · 기억) | 창 전용 세션 = 메인 실행 세션의 문지기·트랜잭션과 섞이지 않음 |
| 작업 단위 | 스키마 단위 인트로스펙션 · 점증(기억) | **DB 단위** 색인(확인 필요) | 연결·스키마 단위 질의(기억) | 서버 × 스키마(DB) × 단계(이름 → 컬럼 → 소스)로 쪼개면 취소·재시도·점증 표시가 자연스러움 |
| 병렬 | 데이터 소스 사이 병렬(기억) | DB 사이 순차 추정(**확인 필요**) | 작업(Job)별 백그라운드 · 병렬 상한 불명(**확인 필요**) | 서버 사이 병렬 · 한 서버 안은 상한(접속·서버 부하 · 26 §8 `connect.max_concurrent`) |
| 취소 | 진행 표시줄에서 취소(확신) | 색인 취소 버튼(기억) | 작업 취소(확신) | 작업마다 독립 취소 · 끝난 작업 결과는 남김 |
| 최신성 | 로컬 인트로스펙션 = 낡을 수 있음(새로 고침 필요 · 확신) | 색인 = 낡을 수 있음 · 증분(확인 필요) | 매번 서버 질의 = 최신(확신) | "항상 최신 = DB 직접 조회" 요구와 맞는 쪽은 DBeaver 방식 · 대신 비용은 분할·병렬·상한으로 |

### 1-9. 한계(도구별 흔한 불만)

- DBeaver: 소스·주석 검색이 옵션이라 기본 결과가 비어 보임 · 미리보기 없음 · 큰 DB에서 느림(서버 질의) — 기억
- DataGrip: 인트로스펙션 안 된 스키마는 안 나옴 · 첫 인트로스펙션이 무거움 — 기억
- SQL Developer: 연결 1개 · Oracle 전용 · 큰 스키마에서 `ALL_SOURCE` 질의 지연 — 확신
- PL/SQL Developer · Toad: Oracle 전용 · 상용 — 확신
- SSMS 내장: 이름만 · 1 DB — 확신 / Redgate: SQL Server 전용 · 첫 색인 시간 — 기억
- TablePlus: 이름만 · 상세 검색 없음 — 기억 / Navicat: 상세 검색은 상위 에디션 — 확인 필요

### 1-10. 우리가 가져갈 장점 7

| # | 장점 | 근거 도구 | 우리 부품·연결 |
|---|---|---|---|
| 1 | **모달리스 창 + 결과가 남음**(편집기와 오가며 봄) | SQL Developer · PL/SQL Dev · Toad · Redgate · DBeaver | 창 = `winhost`(WindowHost) · 메모리·세션 창과 같은 틀 |
| 2 | **연결 전체 · DB 단위 범위 트리 체크**(SQL Server = DB 전체/선택 · 시스템 DB 제외) | DBeaver · Redgate | 127차 `search_scope` all/current · `ExplorerSet` 연결 목록 |
| 3 | **대상 체크 = 이름 · 컬럼 · 설명 · 소스 본문**(데이터 검색은 따로 · 기본 끔) | DBeaver · Toad · Redgate | L1 이름 · L2 컬럼 · 127차 코멘트 사본 · 소스 = 서버 질의 |
| 4 | **아래 미리보기 = 본문 + 모든 일치 하이라이트 + 줄 번호 + 다음/이전** | DataGrip · Redgate | 읽기 전용 `TextBox`(상세 패널 86과 같은 부품) · 찾기 강조 |
| 5 | **목록형 결과 + 종류 아이콘 + 일치 수 + 열 정렬**(한 객체 여러 일치는 미리보기에서) | Redgate · Toad | 그리드 또는 `TreeGrid` · 자연 정렬(`ui.sort_natural`) |
| 6 | **연계 한 길** = 더블클릭 = 소스 탭 + 일치 줄 캐럿 · 우클릭 = 탐색기 메뉴 그대로 · 탐색기에서 보기 · Shift 전체 이름 복사 | DataGrip · PL/SQL Dev · Redgate | `ObjectOrigin` · `RevealTarget` · `obj.*` 액션 |
| 7 | **이름·컬럼 = 로컬 메타 즉시 / 본문 = 서버 질의 백그라운드 · 점증 표시 · 취소 · 상한** | DataGrip(로컬) + Redgate(점증·색인) + DBeaver(취소) | 메타 스레드 · `LatestIntent` · 혜성 · `index_hits_max` |

### 1-11. 신뢰도 표

| 항목 | 신뢰도 | 근거·비고 |
|---|---|---|
| 각 도구의 "모달리스 결과 창" 여부 | 확신(SQL Developer · PL/SQL Dev · Redgate) / 기억(Toad · Navicat · DBeaver Search 뷰) | |
| DBeaver 메뉴 경로·단축키(DB Search · Ctrl+H) | **확인 필요** | 판(22~24)마다 메뉴 위치가 바뀜 · Ctrl+Shift+F는 SQL 포맷 |
| DataGrip 단축키(Go to Symbol · Find in Path) | 기억 | IntelliJ 공통 키맵 기준 |
| SQL Developer Find DB Object 트리·`ALL_SOURCE` | 확신 | 108 §2 사전 뷰와 일치 |
| PL/SQL Developer 정규식 옵션 | 기억 | 판(13~15) 차이 확인 필요 |
| Toad 주석 검색 | 확인 필요 | 에디션 차이 |
| Redgate SQL Search 미리보기·하이라이트 · DB 선택 | 확신 | 무료 · 널리 쓰임 |
| Redgate 로컬 색인·증분 | **확인 필요** | 내부 구현 추정 |
| TablePlus Open anything = 이름만 | 기억 | |
| Navicat Find in Database 에디션 · 데이터 검색 | **확인 필요** | |
| §1-8 성능 수치("수 초~수십 초") | 확인 필요 | 실측 없음 · 우리 실서버(BISCM · M4PLAN)로 재야 함 |
| 서버 사전 뷰 이름(§1-4 표) | 확신 | [108](108-dbms-syntax-and-objects-survey.md) §2~§6 |

## 2. 정의(사용자 10-15) · 목표

- **객체 찾기(Find Objects) 창** = 별도 **모달리스** 창(메인 소유 · 메모리·세션 창과 같은 틀) · 대상 = **연결된 모든 서버**(객체 탐색기 칸 = 카탈로그 단위) · **이름 + 설명(코멘트) + 컬럼 + 소스 본문**(뷰 정의 · 프로시저/함수/패키지/트리거 본문) 검색 · 조건 · 결과 · 미리보기 · 편집기/탐색기 연계.
- 목표 다섯: ① 한 번의 검색어로 "어느 서버 어느 객체 어디에 있나"를 한 화면에 ② 이름·설명·컬럼은 **즉시**(메타 층) · 본문은 **서버 질의 1회/서버/단계 · 백그라운드 · 점증 표시 · 취소 · 상한**(61 §1-8 지연 로딩 · 39 부하원) ③ 결과에서 **한 번의 동작**으로 소스 탭(일치 줄 캐럿) · 탐색기 위치 · 이름 복사 · SELECT ④ 탐색기 필터·파일 검색과 **같은 조건 어휘**(Aa · 낱말 · 정규식 · 이력) ⑤ 종류 아이콘·자연 정렬·칩 등 기존 부품 재사용(30 §2).
- 범위 밖(후속 T) = 데이터 값 검색(행 내용) · 저장된 검색 · 정규식의 서버측 본문 검색 · SSMS 다른 DB 본문(v1 = 현재 DB).

## 3. UI · 워크플로

```
┌ Find Objects ───────────────────────────────────────────────────────────┐
│ [검색어 ____________________▾] [Aa] [ab] [(.*)]   [검색 ▶ / 중지 ■]        │ ← 조건 1행
│ 서버 [전체 ▾]  스키마 [________] (비면 전부 · 쉼표)                          │ ← 범위
│ 대상 ☑이름 ☑설명 ☑컬럼 ☑소스    종류 ☑테이블·뷰 ☑루틴·패키지 ☑트리거 ☑그 밖   │
├ 결과 ──────────────────────────────────────────────────────────────────┤
│  ⌂ 이름            스키마     서버              일치                     │
│  ▦ SALES_CUSTOMER  BISCM      61.81.244.214     이름 · 컬럼 2 · 소스 3    │
│  ƒ PRC_RUN_ALL     BISCM_SB   61.81.244.214     소스 12: … CUSTOMER …     │
├ 미리보기(스플리터) ─────────────────────────────────────────────────────┤
│  BISCM_SB.PRC_RUN_ALL (PROCEDURE · 61.81.244.214)                        │
│   12 │   SELECT c.cust_id FROM sales_customer c                           │ ← 일치 강조
│   47 │   -- customer 재계산                                               │
├ 진행(v2) ───────────────────────────────────────────────────────────────┤
│  검색 중 · 서버 1/3 · 스키마 12 · 작업 17/41 · 조사 8,230행 · 객체 37 · 일치 52   │ ← 혜성 링 = 검색어 상자 둘레
│  시작 17:44:36 · 종료 - · 경과 1.2 s · BISCM · HR · Source · 412 ms       │
│ 거르기 [________________________]   ☑이름 ☑설명 ☑컬럼 ☑소스              │ ← 결과 좁히기(뷰 투영)
│                                           [편집기에서 열기] [탐색기에서 보기] [닫기] │
└─────────────────────────────────────────────────────────────────────────┘
```

> **v2(10-15 저녁 · 사용자 지시 3)**: ① 늘 **최신 내용 기반 = DB 직접 조회**(사본·캐시 없음 · D-306) ② **작업을 잘게**(서버 × 스키마 × 단계) · 창 **전용 엔진**(메인 창·탐색기 메타 스레드와 분리) · 작업마다 독립 세션 ③ 결과는 **합친 뒤** 거르기·정렬·미리보기·열기·바로가기 · **진행 패널**(혜성 + 범위·대상·결과·시작·종료·경과)로 끝날 때까지 동작이 보인다.

- **진입**: View ▸ Find Objects…(Ctrl+Alt+F · 팔레트 `view.find_objects`) · 객체 탐색기 필터 틀에 글이 있으면 그 글을 검색어로 받아 연다(후속 = 틀 오른쪽 버튼).
- **조건 1행**: 검색어(이력 = `search_history` 부품 · 상자 `findobj` = 후속) · Aa(대소문자) · ab(낱말) · (.*)(정규식 — **이름·설명**에 적용(클라이언트) · 컬럼·본문은 메타 문자 없는 글이면 그대로, 있으면 그 단계 건너뜀 + 안내) · Enter = 검색 · Esc = 검색 중이면 중지 · 아니면 창 닫기 · Tab = 검색어 → 스키마 → 거르기 → 목록.
- **범위**: 서버 콤보(전체 | 연결된 서버 각각 · 라벨 = 탐색기 헤더와 같음) · 스키마 글 상자(비면 전부 · 쉼표 여러 개 · 탐색기 필터와 같은 대소문자 무시) · SSMS는 **현재 DB**(v1 · 후속 = DB 체크 트리 1-10 ②).
- **대상 4**(이름 · 설명 · 컬럼 · 소스) · **종류 4 묶음**(테이블·뷰·MV · 프로시저·함수·패키지 · 트리거 · 그 밖 = 시퀀스·시노님·타입…) — 기본 전부 켬 · 설정에 기억.
- **결과 목록**(평면 · 자연 정렬 서버 → 스키마 → 이름 · 종류 아이콘 = 탐색기와 같은 `exp_icons`): 한 객체 = 한 행 · "일치" 열 = 무엇이 걸렸는지 요약(이름 · 설명 · 컬럼 N · 소스 N줄) · 선택 = 미리보기 · 더블클릭/Enter = 소스 탭(소스 있는 종류) 또는 탐색기에서 보기(그 밖) · 우클릭 = 탐색기 메뉴의 부분집합(소스 열기 · 탐색기에서 보기 · 이름 복사 · Shift 전체 이름 복사 · Generate SQL ▸ SELECT).
- **미리보기**(아래 · 스플리터 · 기본 1/3): 머리 = `스키마.이름 (종류 · 서버)` · 본문 = 일치 줄 목록 `줄번호 │ 글`(소스) · 컬럼 표(`컬럼 · 자료형 · 설명`) · 이름/설명 일치는 설명 글 · 일치 구간 강조(찾기 강조색) · 더블클릭 줄 = 그 줄로 소스 탭.
- **진행 패널(v2)**: 두 줄 = ① 상태(검색 중/완료) · 서버 d/T · 스키마 수 · 작업 d/T · 조사한 행 · 객체 수 · 일치 수 ② 시작 시각 · 종료 시각 · 경과(0.1 s) · 보임 n/m(거르기 중) · 안내(상한·정규식 건너뜀·오류) 또는 **마지막으로 끝난 작업**(서버 · 스키마 · 단계 · ms). 혜성 링(nexa-ctl `BusyRing` · 검색어 상자 둘레)은 **종료 전까지** 돌고 끝나면 완료 깜빡임 · 작업이 끝날 때마다 결과 행이 바로 늘어난다(합치기 = 객체 열쇠 `(서버, DB, 스키마, 이름, 종류)`).
- **결과 거르기(v2)**: 글 상자(이름·스키마·서버·요약·컬럼 이름 부분 일치 · 입력마다 즉시) + **일치 종류 토글 4**(이름·설명·컬럼·소스 중 하나라도) → 마스터 결과는 그대로 두고 **보기(`view`)만 투영**(DR-33과 같은 모양 · 선택 행 유지) · 열 머리 클릭 = 정렬(이름·스키마·서버·일치 수 · 재클릭 = 역순).
- **창**: 크기·위치 기억(`window.findobj_size`/`_pos` · 다른 창과 같은 `wingeom::Memo`) · 메인이 닫히면 함께 · 메인 활성/비활성과 무관하게 남음(모달리스) · 결과는 다음 검색까지 유지.

## 4. 엔진(v2 · 창 전용 · 서버 × 스키마 × 단계 작업 · 병렬 · 실시간 직접 조회)

> v1(10-15 낮) = 탐색기 서버별 메타 스레드에 `Req::Find` 3단계를 실어 보냈고 ① 이름·설명은 **L1 사본**으로 판정했다. 사용자 지시(10-15 저녁) "항상 변경된 최신 내용 기반 · 실시간으로 DB를 직접 조회 · 작업을 최대한 잘게 · 창 자체도 별도로 동작 · 각 작업도 별도로 동작"으로 **v2 = 전용 엔진 `findobj_engine.rs`**로 바꿨다(v1 경로는 제거).

| 조각 | 무엇 | 비고 |
|---|---|---|
| `Engine`(`App.findobj_engine`) | `start(rev, q, servers, workers_per_server)` → 서버마다 **계획 스레드** `nsql-findobj-{ix}` · `cancel()` = 살아 있는 `rev`를 0으로(워커는 작업 경계에서 멈춤) · `try_recv()` = 메시지 펌프(`about_to_wait` `findobj_pump`) | 메인 창·탐색기 메타 스레드와 **공유하는 것 없음**(깨우기 `wake`만) |
| 계획(`plan_and_run`) | `explorer::open_meta`(세션 자격 금고로 비밀번호) → SSMS = `set_option("schema", db)` → 스키마 = 조건의 스키마 또는 `schemas_opt`(시스템 스키마 숨김 · 빈 스키마 숨김 · SQLite = main) → **작업 목록** = 스키마마다 `Names` · `Columns` · `Source × 종류 묶음`(루틴·패키지·타입 / 뷰·MV / 트리거) → `Planned{schemas, tasks}` | 작업 = (단계, 스키마, 종류 묶음) · 큐 `VecDeque` + `Mutex` |
| 워커 | 서버마다 `findobj.workers`(기본 2 · 1~8)개 = 계획 세션 + 추가 세션(각자 `open_meta`) · `work()` = 큐에서 하나씩 → `run_task()` → `Task{stage, schema, hits, scanned, note, ms}` · 마지막 워커가 `ServerDone{err}` | 세션 하나 = 한 번에 한 질의(DR-34 통제 단위와 같은 모양) |
| ① 이름·설명 | `find::names(s, schemas, pat|None, with_comment, max)` = **서버 질의 1회**(Oracle `ALL_OBJECTS` ⟕ `ALL_TAB_COMMENTS` · PG `pg_class`/`pg_proc`/`pg_trigger` UNION · SQL Server `sys.objects` ⟕ `extended_properties MS_Description` · SQLite `sqlite_master` · MySQL `tables`/`routines`/`triggers`) → 정규식·낱말·종류는 클라이언트 `filter_names` | **L1 사본은 쓰지 않는다**(D-305) |
| ② 컬럼 | `find::columns(s, schemas, pat, max)` 질의 1회(사전 뷰 LIKE · 방언별) | 정규식(메타 문자) = 건너뜀 + 안내 |
| ③ 소스 | `find::source(s, schemas, kinds, pat, max, lines_per_obj)` 질의 1~2회 · 종류 묶음마다 작업 하나 | Oracle `ALL_SOURCE` 줄 단위 · 다른 DBMS = 덩어리 → `match_lines` |
| 창(`push_msg`) | `rev`가 다르면 버림 · `Planned` = 서버/스키마/작업 합산(서버마다 한 번) · `Task` = 합치기(`merge` · 객체 열쇠) + 조사 행 + 마지막 작업 글 → `apply_view` · `ServerDone` = 서버마다 한 번 · 전부 끝 = `finish`(종료 시각 · 혜성 Done) | 결과·보기·미리보기 = 창 안 순수 함수 |

- 취소 = 새 검색(`rev` 올림) · ■ · Esc · 창 닫기 → `Engine::cancel()` · 진행 중 질의는 호출 상한(`meta.call_timeout_secs`)까지.
- 상한 = `findobj.max_hits`(서버·작업마다 · 상태 안내 "상한") · 한 객체의 소스 일치 줄 = `findobj.lines_per_object`.
- 방언별 LIKE: 대소문자 무시 = Oracle `UPPER(x) LIKE UPPER(p)` · PG `ILIKE` · SQL Server `COLLATE Latin1_General_CI_AS`(구분 = `_CS_AS`) · SQLite `LIKE`(ASCII 무시 · 구분 = `instr`) · MySQL `LIKE`(구분 = `BINARY`) · 와일드카드 `%_\`는 `ESCAPE '\'`로 이스케이프 · 낱말 경계는 클라이언트 후처리.
- 원천 표(108 §2~§6): Oracle `ALL_OBJECTS`·`ALL_TAB_COMMENTS`·`ALL_TAB_COLUMNS`·`ALL_SOURCE`(+ 뷰 = `ALL_VIEWS.TEXT_VC` 12c+) / PG `pg_class`·`pg_proc`·`pg_description`·`pg_attribute`·`prosrc`·`pg_views`·`pg_matviews`·`pg_get_triggerdef` / SQL Server `sys.objects`·`sys.extended_properties`·`sys.columns`·`sys.sql_modules.definition` / SQLite `sqlite_master`·`pragma_table_info` / MySQL `information_schema.tables/routines/triggers/columns/views`.
- 부하원 등재(39 §3): **스레드 = 검색 중 서버마다 1 + (workers−1)**(끝나면 전부 종료 · 상주 0) · 세션 = 서버마다 workers개(끝나면 닫음) · 네트워크 = 사용자 동작당 서버마다 (스키마 수 × 2~5) 질의(자동 재시도 없음 · 26 §8 ✓) · 메모리 = 결과 상한 × 행 · 끌 수 있는 손잡이 = `findobj.workers` 1 · `findobj.max_hits`.

## 5. 연계(한 길)

- 소스 탭 = 탐색기 `open_source`와 **같은 길**(`Req::Source` · `ObjectOrigin{server}` · 작업 단위 = 객체 DB/스키마 · 100 §5 보수적 생성) + 열린 뒤 일치 줄로 캐럿(`goto_line`).
- 탐색기에서 보기 = `ExplorerSet::reveal(server, RevealTarget)`(96 §7 · 단계별 펼침) · 이름 복사 = 탐색기 `name_of` 규칙 · Shift = `full_name_parts`(28 §11) · SELECT = `GenSpec{Select}` → SQL Preview.
- 창 ↔ 메인 포커스: 창에서 연 소스 탭은 메인을 앞으로(`winfocus::focus`) · 창은 남는다.

## 6. 설정 · 키 · i18n

| 키 | 기본 | 뜻 |
|---|---|---|
| `findobj.max_hits` | 2000 | 서버·단계마다 일치 상한(상태줄 "이상") |
| `findobj.lines_per_object` | 10 | 한 객체의 소스 일치 줄 상한(미리보기) |
| `findobj.targets` | name,comment,column,source | 기억된 대상 체크 |
| `findobj.kinds` | relations,routines,triggers,others | 기억된 종류 묶음 |
| `findobj.workers` | 2 | 서버당 동시 작업(세션) 수 · 1~8(v2) |
| `window.findobj_size` / `_pos` | 980×680 | 창 크기·위치 |
| 키맵 `view.find_objects` | Ctrl+Alt+F(mac ⌥⌘F) | 열기/앞으로 |

## 7. 자체 시험(키 주입 0)

- 기동 명령 `findobj.open` · `findobj.query:[cwr:]<글>`(조건 반영 + 검색 실행) · `findobj.filter:<글>`(결과 거르기 · v2) · `findobj.dump:<파일>`(첫 줄 = `running|rows|view|servers d/T|schemas|tasks d/T|scanned|hits|notes|errors` · 둘째·셋째 줄 = 진행 패널 두 줄 · 결과 행 `종류|스키마|이름|서버|일치`(보기 순서) · 선택 행 미리보기 `pre|줄|글|강조수`) · `findobj.pick:<n>`(보기 n행 더블클릭 = 소스 탭) · E2E `scripts/win-findobj-e2e.sh` **19/0**(SQLite 격리 홈 · ① 이름·컬럼·소스 각 행 ② Aa ③ 낱말 ④ pick = 소스 탭 ⑤ 상한 안내 ⑥ **진행 패널**(작업 d/T 전부 끝 · 서버/스키마/작업/조사/객체/일치 · 시작·종료·경과) ⑦ **결과 거르기**(`orders` → 보기 2/4 · showing 2 of 4)).
- 단위 = `find::columns_sql/source_sqls/names_sql`(방언별 SQL 모양 · 이스케이프) · `find::match_lines`(줄 가르기 · 낱말 경계 · 상한) · 엔진 `source_groups`/`filter_names`/`regex_meta` · 창 상태 순수 함수(합치기 · 거르기 · 정렬 · 요약 글).

## 8. 단계 · 결정

- **1단계(10-15 낮)** = §3 창 + §4 엔진 ①②③(v1 · 탐색기 스레드) + §5 연계 + §6 + §7 · **v2(10-15 저녁 · 사용자 지시 3)** = 전용 엔진 · 작업 분할 · 실시간 직접 조회(D-306) · 진행 패널 · 결과 거르기 · 정렬 · **2단계(후속 T-354)** = 정규식 서버측(Oracle `REGEXP_LIKE` · PG `~*`) · SSMS DB 체크 트리 · 데이터 값 검색(별도 대상 · 기본 끔 · 1-10 ③) · 저장된 검색 · 결과 내보내기 · 탐색기 필터 틀 "상세 검색" 버튼.
- 결정 번호(10 §3): **D-292** 창 = 모달리스 · 메인 소유 · 최상위 아님 · **D-293** 본문 검색은 서버 LIKE(정규식은 이름·설명·컬럼만 · 본문은 메타 문자 없을 때만) · **D-294** SSMS 본문·컬럼 범위 = 현재 DB(v1) · **D-306** 객체 찾기 = **늘 DB 직접 조회**(L1·코멘트 사본 금지 · 이름·설명도 서버 질의 1회) · 창 전용 엔진 · 서버 × 스키마 × 단계 작업 · 서버당 `findobj.workers` 세션.
