# 85. 서버별 객체 정보 3층 — L1 즉시 · L2 백그라운드 캐시 · L3 상세 즉시 채움·빠른 회수(2026-09-25 · 100차 mac)

> 사용자 09-25: "서버별 객체 정보의 완성은 **①** 즉시 서비스(검색·인텔리센스)를 주는 1단계를 최대한 빨리 완성 · **②** 온디맨드 서비스에 최대한 빠르게 대응하도록 백그라운드 캐싱하는 2단계 · **③** 추가 정보는 즉시 채워 서비스하고 불필요하면 최대한 빨리 회수하는 3단계 — 레이어 기법. 타 프로그램의 개념·기술·소스를 수렴해 속도·메모리에 유리하게 설계·구현" · 같은 날 "검색과 애니메이션이 엮이지 않게 상태만 교환하는 별도 스레딩".
> 원천 = [47](47-intellisense-metadata.md)(MetaStore 한 벌) · [76 §9](76-intellisense-and-outline.md)(즉시 채움·회수) · [79](79-metadata-ownership-and-refresh.md)(소유·갱신) · [84](84-explorer-search-index.md)(이름 인덱스·검색) · [26 §8](26-performance-architecture.md) · [39 §3](39-resource-governance.md).

## 0. 결론 다섯 줄

1. **L1 이름 층** = 서버의 모든 객체 (스키마, 종류, 이름). 접속 직후 스키마 하나씩(현재 스키마 먼저 · Oracle DBA→USER→ALL · 스키마당 0.05~0.3 s) 백그라운드 세션으로 채우고 `MetaStore`에 `Coverage::Names`로 산다. **완성·검색은 이름만으로 즉시 서비스**(빈 종류도 `Names{n:0}` = 부정 캐시 → 다시 묻지 않음).
2. **L2 온디맨드 캐시** = 첫 요청이 왕복 없이 나오도록 뒤에서 미리: 현재 스키마 관계 목록 승격(상태·부가) · 그 컬럼(상한 200 · 300 ms 간격) · 사전 뷰. L1이 끝나면 시작 · 한 번에 하나 · 다 채우면 트래픽 0.
3. **L3 상세** = 카드·객체 정보의 제약·인덱스·코멘트 · 현재 스키마 밖 컬럼 · 하위 항목 · DDL. **필요할 때 즉시**(급한 세션) · **TTL + 상한으로 빠르게 회수**(상세 64개/300 s · 밖 컬럼 600 s · 30 s 유휴 점검).
4. **단일 원천** = `MetaStore` 한 벌(탐색기·완성·검색·카드 공용 · 79 §2). 검색 판정용 이름 사본은 백그라운드 메타 스레드가 들고(§6) UI는 일치만 받는다 — 판정·정렬이 UI 프레임을 끊지 않는다.
5. **메모리** = L1 항목당 ≈40 B(심볼 인터닝 · 20k 객체 ≈ 1 MB) · 전역 이름 정렬은 **지연 계산**(스키마 30개 채우는 동안 20k 정렬 30번 → 0번) · L2·L3는 상한·TTL로 되돌아간다.

## 1. 타 도구 수렴(개념·기술 · 기억 기반 요약 · ⚠ = 확인 필요)

| 도구 | 층 개념 | 채움 시점 | 회수·갱신 | 우리가 가져온 것 |
|---|---|---|---|---|
| **DataGrip / IntelliJ Database** | ★ **Introspection level 1/2/3** — 1 = 객체 이름만 · 2 = + 컬럼·키(기본) · 3 = + 소스 전체 · 스키마 크기로 자동 선택 · 디스크에 모델 저장(`.idea/dataSources`) · 증분 introspection(`last_ddl_time`) | 접속 때 선택 스키마 전부(백그라운드) | Refresh · 자동 동기화 옵션 | **레벨 = 층**(L1 이름 · L2 컬럼·상태 · L3 소스/상세) · 스키마 단위 진행 · (후속) 디스크 캐시 |
| **DBeaver** | 컨테이너별 지연 캐시(`DBSObjectCache` · `JDBCStructCache` = 자식 일괄 읽기) · "Read table metadata on connect" · "Use column cache" | 트리 펼침 · 완성이 필요할 때 · 접속 때 옵션 | 노드 Refresh = 그 캐시 무효화 | 컨테이너(스키마×종류) 단위 버킷 · 트리 펼침 = 채움 주체 |
| **SSMS** | IntelliSense 로컬 캐시(이름·컬럼) — 오브젝트 탐색기와 별개 | 접속 때 + 필요 때 | Ctrl+Shift+R 새로 고침 | 명시 갱신 명령(79 §4 · 있음) |
| **Azure Data Studio · VS Code mssql** | SqlToolsService 언어 서비스 캐시 · 완료 전엔 "IntelliSense not ready" | 접속 때(타임아웃) | 팔레트 새로 고침 | 준비 상태 표시(헤더 "인덱싱 d/T" · 84 §8) |
| **Oracle SQL Developer** | Code Insight 지연 로드 · 스키마 브라우저 캐시 | 필요 때 | 노드 Refresh | 급한 요청 우선순위(메타 큐) |
| **Toad** ⚠ | Code Insight 캐시(백그라운드 로드 · 파일 저장 옵션) | 접속 뒤 백그라운드 | 옵션 새로 고침 | 백그라운드 세션 분리(있음) |
| **TablePlus · Navicat** | 접속 때 스키마 전체 로드 | 접속 | Reload | 작은 DB에선 한 번에(우리 L1도 스키마 하나는 한 질의) |
| **pgAdmin** | 캐시 없음(키마다 질의) | — | — | 반례(지연·트래픽) |
| **언어 서버(sqls · postgres-language-server)** | 접속 때 스키마 캐시 · DDL 뒤 갱신 | 접속 | DDL 감지 | DDL 뒤 폴더 디프(57 · 있음) |
| **일반 캐시 기법** | 다층(hot/warm/cold) · LRU/TinyLFU · TTL · **부정 캐시** · **단일 비행(single-flight)** · **stale-while-revalidate** · 인터닝/SoA | — | — | 부정 캐시 = `Names{n:0}` · 단일 비행 = 버킷 `Loading`/`upgrading` · SWR = `Stale`(79 §3) · 인터닝 = `Interner`/`Sym` |

## 2. L1 이름 층(즉시)

- 원천 = [84 §2](84-explorer-search-index.md) `nsql_catalog::name_index`(스키마 하나 · 방언별 한 질의 · Oracle §2-1 DBA→USER→ALL).
- 저장 = `MetaStore::load_names(schema, kinds, names, at)` — 스키마의 **모든 종류를 한 편집**으로 `Coverage::Names { at, n, upgrading }` · 이미 `Loaded`/`Stale`/`Loading`인 버킷은 건드리지 않음(내려가지 않음) · 같은 이름은 `ObjId` 유지(컬럼 그대로).
- 타이머 = L1·L2·검색 완성이 남아 있는 동안 호스트 빠른 타이머를 유지(`background_pending` · §202 결함: 유휴면 틱이 멈춰 진행이 섰다) · 검색 중엔 매 틱 큐 감시.
- 스케줄 = `Explorer::l1_step`(접속 직후 · 스키마 목록이 오면 · 검색 없어도 · `explorer.index_prefetch` on · 간격 `explorer.index_idle_ms` 250 ms · 백그라운드 세션 · 한 번에 하나) · 검색 중엔 `pump_index`가 같은 큐를 간격 없이 돈다(둘 다 `tx_bg` = 이름 사본이 한 스레드에).
- 소비 = 완성 `note_coverage(Names)` → 후보를 바로 내고 **청하지 않음** · 검색 = 스레드 사본(§6) · 트리 폴더는 그대로 `Objects`(펼칠 때).
- 승격 = `request_objects`가 `Missing | Stale | Names{upgrading:false}`를 `ObjectsMeta`로 → `Loaded`(상태·부가) · 승격 중에도 `Names` 목록은 산다(`mark_loading` = `upgrading:true` · 깜빡임 0).

## 3. L2 온디맨드 캐시(백그라운드)

- 시작 = L1 완성(`l1_complete`) → `arm_warm` = 현재 스키마 관계 4종(+`intel.from_routines` 함수·패키지·프로시저) `ObjectsMeta` + 사전(`DictMeta`) — 종전 `preload_meta`와 같은 길(접속 직후 몫은 그대로 · L1 뒤 한 번 더는 `Coverage` 검사로 0).
- 컬럼 = 현재 스키마 관계 목록이 오면(`Resp::ObjectsMeta`) 컬럼이 `Unknown`인 것을 이름순으로 `warm_q`에(상한 `meta.warm_columns_max` 200) → `warm_step`이 `meta.warm_idle_ms`(300) 간격 · 한 번에 하나 · `ColumnsMeta{urgent:false}`(백그라운드 세션 · 큐 우선순위 3 = 급한 것 뒤).
- 효과 = 첫 완성의 `alias.` 컬럼 · 카드 컬럼이 왕복 없이 · 실측 데모 SQLite = 목록 응답 뒤 0.9 s 안에 3 테이블 컬럼 완료(로그 `columns-meta … (bg)`).
- 끝 = 큐 소진 → 트래픽 0 · 접속 세대 바뀌면 큐 비움(`reset_tree`).

## 4. L3 상세(즉시 채움 · 빠른 회수)

| 정보 | 채움(즉시) | 회수 |
|---|---|---|
| 테이블 상세(제약·인덱스·코멘트 = 카드) | `request_detail` → `DetailMeta`(백그라운드 · 1.3 s 실측) | `meta.detail_max` 64(오래된 것부터) · `meta.detail_ttl_secs` 300 |
| 현재 스키마 밖 컬럼 | 완성·카드가 `ColumnsMeta{urgent:true}` | `meta.cols_ttl_secs` 600(현재 스키마는 제외) · 바이트 상한 `evict_if_needed`(있음) |
| `스키마.` 버킷(다른 스키마 목록) | 완성 즉시 채움(76 §9) | 미사용 즉시 `drop_bucket`(있음 · `reclaim_intel_buckets`) |
| 하위 항목(제약·인덱스 잎 · 83 §1) · DDL 미리보기 | 트리 펼침 · Generate SQL | 트리 노드 = 접으면 유지(가벼움) · 미리보기 = 창 닫힘 |

- 회수 훅 = `ExplorerSet::idle_tick`(30 s) → `Explorer::reclaim_meta` → `MetaStore::reclaim(now, keep=현재 스키마, detail_max, detail_ttl, cols_ttl)` · 목록은 유지(다음 요청 때 다시 · 깜빡임 0).

## 5. 메모리·속도(설계 근거)

| 항목 | 값 | 근거 |
|---|---|---|
| L1 항목 | ≈40 B(`ObjEntry`) + 인터너 문자열 1회 | BISCM 30 스키마 ≈ 20k 객체 → ≈1 MB(스키마 이름·종류는 심볼) |
| 전역 이름 정렬(`by_name`) | **지연**(`OnceLock` · 첫 `prefix_any`/스키마 없는 조회 때 1회) | 종전 = 버킷 갱신마다 O(N log N) → L1 30 스키마 = 30번 정렬 20k(Debug 수십 ms씩) = 프레임 끊김의 한 축 |
| 스냅숏 편집 | 편집마다 `Snapshot` 복제(객체 20k × 40 B ≈ 0.8 MB memcpy) | 스키마당 1회(`load_names`는 종류 전부 한 편집) — 종전 종류마다 편집 대비 1/18 |
| L2 컬럼 | 컬럼당 32 B · 200 테이블 × 20 컬럼 ≈ 128 KB | 상한 설정 |
| L3 상세 | 항목당 수백 B · 64개 | 상한·TTL |
| 메모리 창 | `MetaStore::layer_bytes()` = (L1, L2, L3) | 80 §(메모리 맵 · 카테고리 Meta 합) |

## 6. 검색 ↔ 애니메이션 분리(스레드 · 사용자 09-25 "상태만 IPC로 교환")

- 종전 = 인덱스 응답마다 UI 스레드가 ① `MetaStore` 편집 + 전역 정렬 ② 인덱스 **전체 스캔** 매칭 ③ 재필터 → Oracle 30 스키마 = 30번 끊김(사용자 "프레임 건너뛰기").
- 지금 = **백그라운드 메타 스레드**가 자기가 읽은 L1 이름 사본(`l1_names` · 스키마별 `Vec<NameEntry>`)을 들고 판정한다.
  - UI → 스레드: `Req::Search{rev, matcher, limit}`(검색어마다 · 우선순위 1 · 질의 없음) · `Req::SearchStop` · `Req::NamesUpdate{schema, kind, names}`(트리가 전체 목록을 읽었을 때 사본 정정 · 84 §6).
  - 스레드 → UI: `Resp::Hits{rev, hits}` — 검색어를 받으면 사본 전부에서 한 번 · 그 뒤 인덱스 스키마가 도착할 때마다 **그 스키마 분만**(점증).
  - UI = `rev`가 지금 검색어와 같을 때만 `materialize_hits`(일치 ≤ `hits_max` 삽입) + `refilter` → 스캔 0 · 매처 실행 0 · 정렬 0.
  - 애니메이션(84 §8-1)은 `tick`에서 상태만 읽어 그린다(`SearchState`) — 검색 작업과 공유 데이터 없음(메시지만).
- 남은 UI 몫 = `load_names`(스키마당 1 편집) · 노드 삽입 · 재필터 — §203: 재필터는 부모 표 O(n) + 소문자 라벨 캐시(`lower_labels`) + `Matcher::matches_cached`로 2,230 노드 92 → <20 ms · 아이콘 마스크는 시작 때 선굽기(`nsql-icons`).
- 시험 = `search_thread_tests::search_hits_matches_across_schemas_with_limit`(순수 함수) · 탐색기 `index_materializes_hits_into_unloaded_folders_and_completes`(응답 주입) · GUI 격리 덤프(§10).

## 7. 설정(카테고리 Explorer)

| 키 | 기본 | 층 | 뜻 |
|---|---|---|---|
| `explorer.search_index` | on | L1 | 끔 = 트리에 읽힌 것만 검색 |
| `explorer.index_prefetch` | on | L1 | **접속 직후** 이름 층 채움(종전 "유휴 선적재") |
| `explorer.index_idle_ms` | **250**(종전 5000 · D-209) | L1 | 스키마 사이 간격 |
| `explorer.index_max` · `explorer.index_hits_max` | 200000 · 2000 | L1 | 84 §5 |
| `meta.warm_columns_max` · `meta.warm_idle_ms` | 200 · 300 | L2 | 현재 스키마 컬럼 미리 읽기 상한·간격 |
| `meta.detail_max` · `meta.detail_ttl_secs` · `meta.cols_ttl_secs` | 64 · 300 · 600 | L3 | 상세 상한·TTL · 밖 컬럼 TTL |
| `meta.disk_cache` | on | L1 | 이름 층 디스크 캐시(§9 · `NSQL_HOME/meta/`) |
| `intel.preload` · `meta.max_bytes` 등 | (기존) | L2/L3 | 76 §9 · 39 §3-6 |

**D-209(한 줄 고지)**: `explorer.index_idle_ms` 기본 5000 → 250 · `explorer.index_prefetch` 뜻 = "유휴 선적재" → "접속 직후 L1 채움"(사용자 ① "1단계를 최대한 빠르게").

## 8. 부하·회수 원장 갱신

- 26 §8: L1 = 접속 직후 스키마당 1 질의(250 ms 간격 · 다 읽으면 0 · 접속한 적 없는 서버 0) · L2 = 현재 스키마 관계 컬럼 ≤200 질의(300 ms 간격) · 검색 판정 = 질의 0.
- 39 §3-2: 끄는 키 = `explorer.index_prefetch` · `meta.warm_columns_max=0` · 상한 키 위 표.
- 스레드 원장(39 §3-7): 새 스레드 0 — 백그라운드 메타 스레드가 판정까지(질의 사이 ms 단위).

## 9. 남은 것(T-222)

- ~~디스크 캐시~~ ✅ §201 = `metacache.rs`(`NSQL_HOME/meta/<cred_id 해시>.names` · 텍스트 · 저장 = L1 완성 때 별 스레드 · 심기 = 스키마 목록이 오면 지금 스키마만 Names + 스레드 사본 seed · 서버 L1이 곧 덮어씀 · `meta.disk_cache` on) · 남음 = (Oracle) `last_ddl_time` 증분 · 캐시 나이 표시.
- 스키마 순서 학습(최근 일치·사용 많은 스키마 먼저) · 컬럼 이름 인덱스(옵션) · L2에 현재 스키마 외 "최근 쓴 스키마" 포함 · ~~메모리 창 세 줄~~ ✅ §201(`Cat::Meta/MetaCols/MetaDetail`).

## 10. 자체 점검(09-25 · 키 주입 0)

- 단위 = `cargo test -p nsql-run layer_tests`(Names → 승격 id 유지 · 빈 종류 부정 캐시 · 회수 TTL/상한/현재 스키마 제외) · `cargo test -p nexa-sql explorer`(부분 → 완성 · 스레드 판정 순수 함수) · 전체 224·44·19·10 통과.
- GUI(격리 · 데모 SQLite): `@after:2000:explorer.filter:ept,@after:2600:explorer.dump:f1,@after:6000:explorer.dump:f2` → f1 = `Tables (1/3) Loaded / dept`(스레드 일치 → 부분 → 완성) · 로그 `columns-meta main.dept|emp|sales (bg)` = L2 컬럼 워머.
- 실서버 = `nsql cat -c BISCM -s <스키마> index` 0.27~0.3 s(L1 스키마당) · 실기 U-159(Oracle 검색 애니메이션이 끊기지 않는지 · 접속 뒤 8 s 안 전 스키마 인덱싱 뒤 첫 검색 즉시).
