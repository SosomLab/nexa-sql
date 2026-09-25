# 84. 탐색기 검색 인덱스 — 선별 → 부분 노드 → 순차 완성 · 유휴 선적재(2026-09-25 · 100차 mac)

> 사용자 09-25(세 메시지): ① "MSSQL 접속 후 필터 → 그 뒤 Oracle 접속 = Oracle 객체 조회 안 됨 · 한 번도 직접 조회하지 않은 노드는 검색에 안 잡힘 → 검색 대상에 대해서라도 간략히 인덱싱 · **1) 대상 선별 → 2) 대상 상세 완성 → 3) 나머지 순차 완성**의 점증·순차 방식" ② "사용자가 확인하지 않은 객체도 검색 대상" ③ "검색어가 있으면 그 검색어로 순차 조회하며 **검색 중임이 인식**되게 · DBMS별 검색어 완성(미대상 포함) · 첫 실행 이후 아주 조금씩 느리게 전체 객체를 **1차 인덱싱**해 검색에 활용".
> 원천 = [28 §7](28-object-explorer.md)(검색창 규칙) · [79](79-metadata-ownership-and-refresh.md)(메타 소유·갱신) · [57](57-explorer-refresh-after-ddl.md)(폴더 디프) · [26 §8](26-performance-architecture.md)(네트워크 원칙) · [39 §3](39-resource-governance.md)(부하원).

> ★ 09-25 후속 = [85 메타 3층](85-metadata-layers.md): 인덱스 항목은 `MetaStore`(`Coverage::Names` · L1)에 살고, 검색 판정은 백그라운드 메타 스레드가 한다(85 §6 · `Explorer.index` Vec 폐지 · 프레임 끊김 해소).

## 0. 결론 다섯 줄

1. **검색 대상 = 서버의 모든 객체**(트리에 읽힌 것 + 아직 안 펼친 폴더의 것). 근거 자료 = **이름 인덱스**(`nsql_catalog::name_index` · (스키마, 종류, 이름)만 · 상태·시각 없음).
2. 인덱스는 **스키마 하나씩 순차**로 읽는다(현재 스키마 먼저 · 한 번에 하나 · 우선순위 = 사용자 클릭 다음). 이유 = 실측(§3): Oracle `ALL_OBJECTS`는 비 DBA 계정에서 서버 전체 한 번이 20 s, 스키마 하나는 0.35~3.5 s — 첫 결과가 0.35 s에 나온다.
3. 일치 객체는 도착 즉시 그 (스키마, 종류) 폴더의 **부분 자식**으로 올라온다(`LoadState::Partial` · 개수 `n/?`) → 폴더는 **완성 큐**에 들어가 하나씩 전체 목록으로 바뀐다(디프 · 있던 노드 유지 · `n/전체`).
4. **검색 중 표시** = 서버 헤더 "연결 N개 · 일치 M · **인덱싱 d/T**"(스키마 진행) · 상한에 잘리면 "인덱스 상한".
5. **유휴 선적재** = 접속 뒤 검색이 없어도 백그라운드 메타 세션으로 스키마 하나씩 `explorer.index_idle_ms`(5 s) 간격으로 인덱스를 채운다 → 첫 검색부터 전체가 대상 · 다 읽으면 트래픽 0 · 설정으로 끔.

## 1. 흐름(세 단계 + 선적재)

```
검색어 입력 ─► apply_filter(m)
   │  ① 선별   ensure_index(): 안 읽은 스키마를 큐에(현재 스키마 먼저) · pump_index() = Req::Index(스키마 1) [prio 2]
   │  ② 상세   materialize(): index ∩ m.matches → 폴더 없는 스키마엔 폴더 생성(make_folders) → Object 노드(부분) → 폴더 = Partial → complete_q
   │  refilter(): 부분 폴더도 Loaded처럼 펼침 · 개수 n/?
   └  ③ 완성   pump_complete(): complete_q 앞에서 하나 → soft + Req::Objects(prio 1) → 응답 diff_children → Loaded(n/전체) → 다음
Resp::Index ─► index[스키마] 교체 · index_done += 스키마 · (검색 중이면) ② ③ 다시 · pump_index() 다음 스키마
검색 해제  ─► complete_q·index_q 비움(진행 중 하나는 응답을 그대로 받는다) · 필터가 펼친 폴더 접힘 · 부분 폴더는 Partial 그대로(펼치면 조용히 완성)
유휴      ─► prefetch_step(now): 검색 없음 · 마지막 응답 뒤 idle_ms 경과 · 안 읽은 스키마 하나 → tx_bg Req::Index
```

- 단계 ②의 "상세"는 트리 노드로 올리는 일이다 — 이름 인덱스에는 상태·시각이 없으므로 유효성 배지·변경 시각은 ③ 완성 응답이 채운다(부분 노드는 빈 상태로 그려진다 · 아이콘·펼침은 종류표로 즉시).
- 한 번에 하나(인덱스 1 · 완성 1)라 메타 세션은 사용자 클릭에 늘 비어 있고, 검색어를 바꿔도 큐만 바뀐다(진행 중인 질의는 취소하지 않고 결과를 그대로 쓴다 — 인덱스는 검색어와 무관).

## 2. 이름 인덱스 질의(`nsql_catalog::name_index(s, schemas, max)`)

| 방언 | 질의 | 종류 판정 |
|---|---|---|
| Oracle | ★ **`DBA_OBJECTS` 우선**(접근 불가 = 그 메타 스레드에서 한 번만 시도 · `ORACLE_DBA_OK` thread_local) → 자기 스키마(`SELECT USER`)는 `USER_OBJECTS` → 그 밖 `ALL_OBJECTS` · `object_type IN (트리 종류) AND owner IN (스키마) AND object_name NOT LIKE 'BIN$%' [AND ROWNUM <= max+1]` · §2-1 | `oracle_type(kind)` 역방향 |
| SQL Server | `sys.objects`(U·V·P·PC·FN·IF·TF·AF·FS·FT·TR·TA·SO·SN) ∪ `sys.indexes`(IX) ∪ `sys.types`(TY · user_defined) · `TOP (max+1)` | 타입 코드 표 |
| PostgreSQL | `pg_class`(r·p·f·v·m·S·i·I) ∪ `pg_proc`(proc:p/f/a) ∪ `pg_type`(c·e·d·r) · `LIMIT` | relkind·prokind |
| MySQL | `information_schema.tables ∪ routines ∪ triggers` | 문자열 |
| SQLite | `sqlite_master`(table·view·index·trigger) | 문자열 |
| ODBC | `information_schema.tables` | BASE TABLE·VIEW |

### 2-1. Oracle 사전 뷰 선택(09-25 실측 · T-220 ①)

| 뷰 | BISCM_SB(1,4xx 객체) | 비고 |
|---|---|---|
| `ALL_OBJECTS` | 3.30 s | 행마다 권한 판정(비 DBA 계정) · 30 스키마 이름만 한 번 = **67 s** |
| `DBA_OBJECTS` | **0.089 s** | `SELECT ANY DICTIONARY`/`SELECT_CATALOG_ROLE`이 있는 계정(BISCM 가능) · 권한 없는 객체도 나옴(1441 vs 1422) |
| `USER_OBJECTS` | 0.050 s | 자기 스키마만 · 권한 판정 없음 |

규칙 = DBA → USER(자기 스키마) → ALL 순 폴백. DBA 뷰가 더 보여 주는 객체는 ③ 완성 응답(`objects()` = `ALL_OBJECTS`)의 디프가 걸러내므로 **트리의 권한 반영 규칙은 그대로**(잠깐 부분 노드로 보였다 사라질 수 있음 · 19/1441 규모). 적용 뒤 `nsql cat -c BISCM -s BISCM_SB index` = 3.59 s → **0.27 s** · BISCM_POC 1.66 → 0.27 s(접속 포함 왕복).

- 잘림 = `max+1`행을 요청해 넘치면 `truncated` · 반환 항목은 `kinds_for(dialect)`에 있는 종류만(트리 폴더와 1:1).
- **LIKE를 서버에 보내지 않는다** — 대소문자·단어·정규식 판정은 클라이언트 `Matcher` 하나(검색창 옵션 그대로) · 키 입력마다 서버 왕복 0 · 오프라인에서도 읽어 둔 인덱스로 검색.
- CLI 같은 함수: `nsql cat -c <프로필> index [max]`(범위 = `-s 스키마` 또는 보이는 스키마 목록) · stderr에 "entries · ms · truncated".

## 3. 실측(09-25 · Debug CLI · 첫 접속 포함 안 함)

| 서버 | 범위 | 항목 | 시간 |
|---|---|---|---|
| Oracle BISCM(비 DBA) | 보이는 스키마 30개 한 번 | 4,187 | **20.2 s** |
| Oracle BISCM | 스키마 BISCM 하나 | 937 | 0.35 s |
| Oracle BISCM | BISCM_POC | 975 | 1.6 s |
| Oracle BISCM | BISCM_SB | 1,422 | 3.5 s |
| SQL Server M4PLAN | 전 스키마 | 433 | 63 ms |
| PostgreSQL Repository | 전 스키마 | 358 | 227 ms |

→ Oracle은 `ALL_OBJECTS`의 행별 권한 판정이 비용이라 **스키마 단위 순차**가 맞다(첫 결과 0.35 s · 나머지는 뒤에서). SQL Server·PG는 한 번에 끝나도 같은 길을 쓴다(구조 하나).

## 4. 데이터 구조(`Explorer`)

| 필드 | 뜻 |
|---|---|
| `index: Vec<NameEntry>` | 스키마별로 교체되는 항목(수천~수만 · 항목당 이름 셋 = 수십 B) |
| `index_done: HashSet<String>` | 항목이 다 있는 스키마 |
| `index_q: VecDeque<String>` · `index_inflight: Option<String>` | 읽을 순서(현재 스키마 먼저) · 진행 중 하나 |
| `index_truncated` · `index_last_at` | 상한 안내 · 선적재 간격 기준 |
| `LoadState::Partial` | 인덱스가 올린 자식만 있는 폴더(개수 `n/?`) |
| `complete_q: VecDeque<usize>` · `completing: Option<usize>` | 완성 순서(넣은 순 = 보이는 순) · 진행 중 하나 |
| `batching` | 여러 노드를 만드는 동안 `refilter` 미루기(끝에 한 번) |

메타 워커 큐 우선순위: `Open/Prepare` 0 · 사용자 클릭·완성 `Objects` 1 · **`Index` 2** · 백그라운드 컬럼 3 · `ObjectsMeta` 4 · 사전 5.

## 5. 설정(`nsql-settings` · 카테고리 Explorer)

| 키 | 기본 | 뜻 |
|---|---|---|
| `explorer.search_index` | on | 끔 = 트리에 읽힌 것만 거름(종전) |
| `explorer.index_max` | 200000 | 스키마당 인덱스 상한(0 = 무제한) · 걸리면 헤더 "인덱스 상한" |
| `explorer.index_hits_max` | 2000 | 한 필터가 서버마다 트리에 올리는 일치 상한('A' 같은 넓은 검색어 보호) |
| `explorer.index_prefetch` | on | 유휴 선적재(§7) |
| `explorer.index_idle_ms` | 5000 | 선적재 간격(스키마 하나마다) |

## 6. 일관성(인덱스와 트리가 어긋나지 않게)

- **전체 목록 응답(`Resp::Objects`)** = 그 (스키마, 종류) 버킷의 인덱스를 새 목록으로 교체 — 트리 갱신(DDL 뒤 57 T1 · 워터마크 T2 · 사용자 새로 고침)이 곧 인덱스 갱신.
- **스키마 목록이 새로 오면**(접속·재접속·스키마 옵션 변경) 인덱스 전부 무효화 → 검색 중이면 바로 다시 채움(사용자 1번 이미지 = 필터 중 새 접속의 노드가 안 보이던 결함 해결).
- **명시 메타 갱신**(`intel.refresh*` · 79 §4) = 무효화.
- 부분 폴더를 사용자가 직접 펼치면 조용히 완성(디프) — 있던 부분 노드·선택은 그대로.
- 실패한 스키마는 `index_done`에 넣어 재시도 폭주를 막는다(다음 무효화 때 다시).

## 7. 유휴 선적재(사용자 ③ "첫 실행 이후 아주 조금씩 느리게")

- 주기 = 호스트의 유휴 틱(`App::idle_tick` 자리 · `ExplorerSet::prefetch_tick`) → 칸마다 `Explorer::prefetch_step(now)`.
- 조건 = 켬 · 온라인 · 스키마 목록 있음 · **검색 중 아님** · 진행 중 인덱스 없음 · 마지막 응답 뒤 `idle_ms` 경과 → 안 읽은 스키마 하나를 **백그라운드 메타 세션(`tx_bg`)**으로 읽는다(사용자 클릭 세션을 막지 않는다 · 세션은 첫 요청 때 열린다 = 26 §8의 "미리 읽기 세션"과 같은 것).
- 끝 = 모든 스키마 `index_done` → 요청 0. 접속한 적 없는 서버·오프라인 칸 = 0(26 §8 원칙).
- 비용 = Oracle 30 스키마 × 평균 0.7 s = 서버당 총 20 s의 질의가 2.5분에 걸쳐 흩어짐 · 메모리 = 항목 수 × ~60 B(BISCM 4,187 → ≈250 KB).

## 8. 표시

### 8-1. 검색 진행 애니메이션(사용자 09-25 "진행 중인지 직관적으로 · 테두리 위를 도는 짧은 밝은 선 · 완료 = 두 번 깜빡임 · 완료 테두리는 다르게" · ✅ §198)
- 상태 = `filterbar::SearchState { Idle, Running, Done }` — `ExplorerSet::sync_search_state`가 `apply_filter`와 매 `tick`에 계산: 검색어 없음 = Idle · 어느 칸이든 `Explorer::search_busy()`(인덱스 진행/큐 · 완성 진행/큐) = Running · 그 밖 = Done.
- Running = 둥근 테두리 둘레(`round_rect_path` · 모서리 호 6분할)를 따라 **둘레의 18 %** 길이 선이 **1.6 s에 한 바퀴**(꼬리 = 옅고 굵게 3px · 머리 = `th.accent` 2px · `path_window`로 한 바퀴 넘는 구간은 둘로) · 매 tick 다시 그림(≈30 ms 타이머 = `is_animating`).
- Running → Done = **두 번 깜빡임**(130 ms × 4 위상 · 켬 = `th.accent` 2px) → 그 뒤 **완료 테두리 `th.ok`** 1px 유지 · Idle로 가면 취소 · Idle → Done(진행 없이 끝난 검색) = 깜빡임 없이 완료 테두리만.
- §199 강화 = 혜성 꼬리 8단계(배경 → 강조색 · 1.5 → 3px) + 진행 중 테두리 전체 강조색 50 % · 1.2 s · 22 % · 완료 시그니처 원복 = 상자 재포커스·글 변경·초기화(`dismiss_done`) · 검색어가 있는 채로 서버 추가 = 새 칸도 즉시 검색 모드(`new_pane`).
- 비용 = Running/깜빡임 동안만 타이머(끝나면 정적) · 시험 = `search_anim_tests`(둘레 길이 ≈ 사각 + 2πr · 창 wrap 둘 · 상태 전환·깜빡임 창).

- 서버 헤더: `연결 N개 · 일치 M` + 검색 중이고 인덱스가 덜 읽혔으면 `· 인덱싱 d/T`(그룹 합) + 잘렸으면 `· 인덱스 상한`.
- 폴더 개수: 부분 = `n/?` · 완성 = `n/전체` · 필터 없음 = `전체`(부분 폴더는 접힌 채 `n/?` — 펼치면 완성).
- 부분 객체 노드 = 아이콘·펼침 화살표는 종류표로 즉시 · 상태 배지·시각은 완성 뒤.

## 9. 부하·회수

- 26 §8 표 등재(검색 인덱스 · 유휴 선적재) · 39 §3-2 부하원(끄는 키 = `explorer.search_index` · `explorer.index_prefetch`).
- 회수: 검색 해제 = 큐 비움(진행 중 1개만 완주) · 칸 닫힘 = 인덱스 비움(`reset_tree`) · 오프라인 = 요청 0.

## 10. 자체 점검(09-25 · 키 주입 0)

- 단위: `cargo test -p nexa-sql filterbar`(`search_anim_tests` 둘 · §8-1) · `cargo test -p nexa-sql explorer` — `index_materializes_hits_into_unloaded_folders_and_completes`(안 읽은 폴더 → Partial `1/?` → 디프 완성 `2/3` → 해제 접힘·큐 비움) · 검색 이력 드롭다운 `dropdown_lists_filters_picks_and_leaves`(행 클릭 = 고르기 · 우클릭 = 즉시 감춤).
- 실서버 CLI: `nsql cat -c <프로필> index` / `-s <스키마> index`(§3 표).
- GUI(격리 `NSQL_HOME` + 데모 SQLite): `NSQL_STARTUP_CMD="@after:1800:explorer.dump:d0,@after:2000:explorer.filter:ept,@after:4500:explorer.dump:d1"` → d0 = `schema main Idle`(아무것도 안 펼침) · d1 = `Tables (1/3) Loaded / object dept` — 펼치지 않은 폴더의 객체가 검색으로 올라옴(§197).

## 11. 남은 것(T-220)

- ~~Oracle `USER_OBJECTS`~~ ✅ §198(§2-1 · DBA → USER → ALL) · 스키마 순서 = 최근 검색이 많이 걸린 스키마 먼저(학습).
- 컬럼 이름 인덱스(`ALL_TAB_COLUMNS` · 옵션 · 무게 큼) · 인덱스 디스크 캐시(다음 실행에 첫 검색 즉시 · 79 §2 소유 규칙 안에서).
- ~~진행 표시~~ ✅ §8-1(테두리 애니메이션) · 부분 폴더 `n/?` 옆 점 · 헤더 진행에 스키마 이름.
