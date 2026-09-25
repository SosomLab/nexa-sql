# 79. 메타 소유·공유·갱신 설계 — 연결 단위 공용 · 읽은 쪽이 채움 · 명시 갱신(2026-09-24 · 100차 mac)

> 사용자 09-24: "객체 정보의 소유는 DBMS 연결에 공용 · 읽은 쪽이 채워 전달 · 다른 탭은 캐시를 즉시 · 탐색기 갱신은 하위 객체까지 · 완성 정보 갱신은 별도 메뉴/명령 팔레트로 직접 요청 — 타 클라이언트 조사 후 설계". 원천 = [47](47-intellisense-metadata.md) · [57](57-explorer-refresh-after-ddl.md) · [76 §9](76-intellisense-and-outline.md) · [77 §1-2](77-data-workbench-architecture.md).

> 09-25 후속 = [85 메타 3층](85-metadata-layers.md)(L1 이름 층 `Coverage::Names` · L2 워머 · L3 회수 TTL/상한).

## 0. 결론 다섯 줄

1. **소유 = 서버(연결 스펙) 하나에 `MetaStore` 하나**(`ExplorerSet` 칸) — 이미 그렇다. 그 서버에 붙은 모든 탭·세션(공유·전용 `CONNECT`)이 같은 스냅샷을 읽는다(`meta_view(spec)`). 새로 할 것 = 소유 규칙을 문서·시험으로 고정하고, 채우는 주체를 넓힌다(§2).
2. **채움 = 읽은 쪽이 넣는다**: 탐색기 트리(펼침) · 완성(즉시 채움·미리 읽기 · 09-23~24) · (예정) hover 카드·객체 정보 탭(T-179) · `DESC`/실행 결과의 열 메타(§2-3). 소비자는 스냅샷만 읽고 `Coverage`로 "없음/읽는 중/있음/오류"를 안다.
3. **다른 탭 = 캐시 즉시**: 이미 그렇다(Arc 스냅샷 · 복사 0). 빠진 것 = 탭이 다른 서버로 옮겨 붙을 때(`CONNECT`)의 스냅샷 전환은 자동(스펙으로 칸을 찾음).
4. **탐색기 갱신 = 하위까지**: 노드 새로 고침(F5·우클릭)이 그 하위 폴더를 다시 읽는 것은 있지만(57 T3), **컬럼·완성 버킷은 그대로**다 → 갱신 범위 규칙(§3)을 정해 `Coverage::Stale`로 표시하고 다음 요청 때 다시 읽게 한다(목록은 그대로 보여 깜빡임 0).
5. **완성 캐시 갱신 = 명시 명령**(§4): 팔레트/메뉴 `Refresh IntelliSense cache`(SSMS·ADS·VS Code mssql과 같은 이름 · 단축키 Ctrl+Shift+R) — 범위 = 현재 스키마 / 이 서버 / 전 서버 · 탐색기 루트 메뉴에도 같은 항목 · 자동 갱신(DDL 뒤 · 유휴 워터마크)은 지금 것을 유지.

## 1. 타 클라이언트 조사(기억 기반 요약 · 확인 필요 표시 ⚠)

| 도구 | 메타 소유 | 채움 | 탐색기 갱신 | 완성 캐시 갱신(명시) |
|---|---|---|---|---|
| **DBeaver** | 연결(DataSource)당 `DBNModel` 캐시 — 탐색기·SQL 편집기·데이터 편집기 공용 | 트리 펼침 · 완성이 필요한 것을 지연 로드 | 노드 **Refresh(F5)** = 그 노드 하위 캐시 무효화 → 다시 읽음(자식 전부) | 별도 명령 없음 — 트리 Refresh가 곧 완성 캐시 갱신(같은 캐시) · 설정 "Enable auto-refresh" |
| **DataGrip / IntelliJ** | 데이터 소스당 스키마 모델(`.idea` 디스크 캐시) | 접속 시 introspection(선택한 스키마) · 필요 시 부분 | 데이터 소스/스키마 **Refresh**(Ctrl+Alt+Y) = 전체 재동기화 · "Forget Cached Schemas" | 같은 명령(Refresh) · 자동 = DDL 실행 뒤 자동 동기화 옵션 |
| **SSMS** | 연결당 IntelliSense 로컬 캐시(편집기 세션) | 접속 시 + 필요 시 | 오브젝트 탐색기 Refresh는 **캐시와 별개** | **Edit ▸ IntelliSense ▸ Refresh Local Cache(Ctrl+Shift+R)** — 탐색기와 캐시가 따로라 이 명령이 필요 |
| **Azure Data Studio · VS Code mssql** | 연결당 언어 서비스 캐시 | 접속 시 | — | 명령 팔레트 **"Refresh IntelliSense Cache"** |
| **Oracle SQL Developer** | 연결당 카탈로그 캐시 | 트리 펼침 · Completion Insight가 지연 로드 | 노드 Refresh = 하위 다시 읽기 | 별도 없음(트리 Refresh) · 설정 "Code Insight" 지연/개수 |
| **Toad** ⚠ | 연결당 Code Insight 캐시 | 접속 시 백그라운드 로드 | Schema Browser Refresh | "Refresh Code Insight cache"(옵션 화면) |
| **TablePlus / Navicat** | 연결당 | 접속 시 스키마 로드 | Reload(⌘R) | 같은 Reload |
| **pgAdmin** | 캐시 없음 — 키 입력마다 서버 질의 | — | Refresh | 해당 없음 |

공통점: **소유는 연결 단위** · 트리와 완성이 **같은 캐시**를 쓰는 도구(DBeaver·DataGrip·SQL Developer)는 트리 Refresh가 곧 완성 갱신 · **따로인 도구(SSMS·ADS)** 만 별도 명령. 우리는 같은 캐시(`MetaStore`)이므로 DBeaver 방식이 기본이고, 사용자가 원한 **명시 명령**은 "갱신 범위를 크게(서버/전체) 한 번에" 잡는 데 쓴다.

## 2. 소유·채움 규칙(확정안)

### 2-1. 소유
- `ExplorerSet` 칸 = 서버(연결 스펙 · 호스트·포트·DB·계정) 하나 = `Explorer` 하나 = `MetaStore` 하나 · 세션 수와 무관(공유 N · 전용 `CONNECT` 포함 · 52 §2-2). 접속이 0이 되면 접속만 닫고 트리·메타는 유지(56차) · 칸이 닫히면 비움.
- 소비자 API = `meta_view(spec) -> (&Interner, Arc<Snapshot>)` 하나(완성·툴팁·hover·객체 정보 탭·그리드 열 메타). **새 소비자는 이 API만 쓴다**(30 §1-2 체크리스트에 추가).

### 2-2. 채움(누가 · 언제)
| 채우는 쪽 | 무엇 | 세션 | 우선순위 |
|---|---|---|---|
| 탐색기 트리 펼침 | (스키마, 종류) 버킷 · 테이블 컬럼 | 급한 메타 세션 | 1 |
| 완성 즉시 채움 | alias 컬럼(급) · `스키마.` 버킷 · 사전 | 급한/백그라운드 | 1 / 3~5 |
| 접속 직후 미리 읽기 | 현재 스키마 관계 종류 + 사전 | 백그라운드 | 4~5 |
| (예정) hover 카드·객체 정보 탭(T-179) | 컬럼 상세·키·코멘트·시그니처 | 급한 | 1 |
| ★ (제안) 실행 결과의 열 메타 | `SELECT *` 결과의 열 이름·타입 → 그 테이블 컬럼(단일 테이블 문장일 때) | 없음(결과에서) | — |
| ★ (제안) `DESC`/`DESCRIBE` 결과 | 컬럼 표 → 메타 | 없음 | — |

원칙: 채운 쪽이 누구든 **같은 `load_bucket`/`set_columns`** 로 들어가고 `Coverage`/`ColState`가 상태의 단일 원천. 두 쪽이 같은 것을 동시에 요청해도 `Loading` 표시로 중복 요청 0.

### 2-3. 다른 탭에서
스냅샷은 Arc라 읽기 비용 0 · 탭이 다른 서버 세션으로 바뀌면(`CONNECT`) `spec`이 바뀌어 다른 칸의 스냅샷을 읽는다. 없는 서버(접속 문자열로 붙은 전용 세션)도 칸이 생기므로 같다.

## 3. 갱신 범위 규칙(탐색기 새로 고침 = 하위까지)

| 새로 고침 대상 | 다시 읽는 것 | 컬럼·완성 버킷 |
|---|---|---|
| 서버 루트 | 스키마 목록 + **열린** 폴더(57 T3 그대로) | 그 서버 전 버킷 `Stale` · 전 컬럼 `Unknown`(목록은 유지 · 다음 요청 때 다시) · 사전 `Stale` |
| 스키마 노드 | 그 스키마의 열린 폴더 | 그 스키마 전 종류 버킷 `Stale` · 그 스키마 테이블 컬럼 `Unknown` |
| 종류 폴더 | 그 폴더 | 그 버킷 다시(디프) · 그 종류가 관계면 그 테이블들 컬럼 `Unknown` |
| 객체 노드 | 그 객체 하위(컬럼 폴더) | 그 테이블 컬럼 다시 |

구현: `Coverage::Stale { at, n }`(목록은 살아 있고 요청이 오면 다시 읽음 — 완성은 `Loaded`처럼 후보를 내고 동시에 `NeedObjects`를 낸다 = 깜빡임 0 · "갱신 중…" 표식) · `MetaStore::mark_stale(schema|all)` · `mark_columns_unknown(schema|table)` · 탐색기 `refresh_selected`가 위 표대로 호출. 워터마크(57 T2)·DDL 뒤 갱신(57 T1)은 지금처럼 폴더 단위 디프.

## 4. 명시 갱신 명령(완성 캐시)

- 명령 id `intel.refresh`(팔레트 "Refresh IntelliSense cache" · Edit ▸ IntelliSense ▸ Refresh · 탐색기 루트 우클릭 "Refresh metadata" · 단축키 Ctrl+Shift+R(SSMS와 같음 · 맥 ⌘⇧R) · 범위 = 하위 메뉴: **현재 스키마**(기본) / 이 서버 / 전 서버.
- 동작 = §3의 `Stale`/`Unknown` 표시 + 현재 스키마·사전은 **바로 다시 읽기**(백그라운드 세션 · 미리 읽기와 같은 길) · 상태줄 "IntelliSense cache refreshed(스키마 N · 객체 M)" · 트리는 열린 폴더만 조용히 디프.
- 자동 갱신은 그대로: DDL 뒤(57 T1) · 유휴 워터마크(T2 · `meta.refresh_secs`) · 못 찾음 신호(T4). 명시 명령은 "밖에서 바뀐 것을 지금 반영"용.

## 5. 할 일(제안 순서 · TODO 등재)

1. **T-187 갱신 범위 규칙**(§3 · `Coverage::Stale` · `mark_stale`/`mark_columns_unknown` · 탐색기 새로 고침 배선 · 시험) — ✅ 09-24 §177(루트 = 전부 · 스키마·폴더·객체 = 그 스키마 단위 · 완성은 Stale 버킷을 `Loaded`처럼 쓰고 재요청 · `request_objects`는 Stale도 다시 읽음).
2. **T-188 명시 갱신 명령**(§4 · `intel.refresh` + 범위 · 팔레트/메뉴/루트 메뉴/키 · 상태줄) — ✅ 09-24 §177(`intel.refresh`/`_server`/`_all` · Edit 메뉴 · 팔레트 · 루트·스키마 우클릭 "메타 새로 고침" · ⌘⇧R/Ctrl+Shift+R · 상태줄 "버킷 N · 객체 M").
3. **T-189 채움 주체 확장**(§2-2 제안 둘: 실행 결과 열 메타 · `DESC` 결과) — 소~중 · 77 §1-2와 함께.
4. 소유 규칙 시험: 같은 서버 두 세션(공유+전용)이 같은 스냅샷을 보는지 · `CONNECT`로 바뀐 탭이 다른 칸을 읽는지(MC/DC).
