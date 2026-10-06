# 94. 설정 키 이름 체계 — 위치 연계 · 길이 · 1레벨/멀티레벨 · 동의어(2026-09-28 · 102차 끝 3 win · 📐 안 + 부품 구현)

> 사용자 09-28: *"변수 전체 이름(`.`을 포함한 계층 전체)이 설정이 있는 위치와 연결이 잘 되지 않아 찾기 어렵다 · 목적/기능 단위 이름이라면 위치 정보도 연계하거나 체계를 바꿔라 · 두 정보가 상충하면 좋은 방안을 설계해 추천 · 성능·속도·기능에 문제가 없다면 이름이 의미와 직관적으로 연결되고 길이는 최소 · IntelliJ·VS Code·DBeaver·Sublime 체계를 다시 조사해 1레벨(Sublime식)인지 멀티레벨인지 추천 · 동일 의미의 여러 이름이 있는지 검토"*.
> 원천 = `nsql-settings::REGISTRY`(498키 · 46 접두 · 27 카테고리) · [24](24-settings-and-vscode-analysis.md) · [78](78-settings-reorganization.md)(카테고리 재편 안 · T-186) · 설정 창 `prefs_win.rs`.

## 0. 결론 여섯 줄

1. **2레벨 `<카테고리 접두>.<이름>`을 유지·강화한다** — Sublime식 1레벨(flat)은 키 500개에서 충돌·검색 잡음·묶음 불가로 부적합, 3레벨 이상은 길이만 는다(§4).
2. **위치 연계 = "접두 ↔ 설정 창 카테고리 1:1"** 이 원칙이다. 지금은 한 카테고리에 접두가 최대 8개(Session = `session. tx. vars. run. db. script. license. settings.`) 섞여 있어 키만 보고 위치를 못 찾는다(§3). 해법은 두 갈래: ① **키를 옮기지 않고 카테고리를 쪼갠다**(78 §3의 31 카테고리 안 — Session을 세션/트랜잭션 보호/스크립트·변수로) ② 접두가 카테고리와 정말 다른 소수 키만 **이름을 바꾼다**(§6 표 · `RENAMED` 마이그레이션 부품).
3. **의미 직관 > 위치 연계**가 상충 원칙이다(사용자 "2가지 정보가 상충된다면") — 접두는 **사용자의 일**(`tx.` 트랜잭션 · `vars.` 변수)을 말하고, 위치는 카테고리 표가 따라온다. 반대로 위치를 위해 `session.tx_stale_min`처럼 접두를 늘리지 않는다(길이 ↑ · 뜻 ↓).
4. **길이 규칙**: 접두 ≤ 9자 · 이름 ≤ 2낱말(`_` 하나) · 단위 접미(`_ms` `_secs` `_mb` `_kb` `_pct`)는 값의 뜻이므로 남긴다 · `enabled`/`on`류 마스터는 접두만(`intel.enabled` → 유지 · 새 키는 `probe.enabled`처럼).
5. **동의어·중복**(§5): 뜻이 같은 키가 이름만 다른 경우는 없다(전부 대상이 다르다). 다만 **같은 뜻의 낱말이 두 표기**로 쓰이는 곳이 있다(`dblclick`/`double_click` 없음 · `secs`/`sec` 없음 · `max_*`/`*_max` **양쪽** 25/31 · `conn.`/`connect.` **둘 다** 7/4) — 낱말 표기 규칙을 정해 새 키부터 적용하고 기존 키는 §6 2차에서 정리.
6. **구현(이번 세션)**: `RENAMED` 표 + 읽기 마이그레이션 + `canonical_key`(부품) · 1차 적용 4키(§6-1) · 설정 창 = 그룹 → 카테고리 → **접두 묶음** → 등재 순 정렬(`display_order`) + 키 옆 복사 버튼 + 고급 구분(`ADVANCED`) · 큰 파일 카테고리 신설. **2차(전면 rename · §6-2 표)는 D-146 결정 뒤 T-250**.

## 1. 타 제품 조사(재조사 · 09-28)

| 제품 | 키 체계 | 저장 | 화면과의 연계 | 사용자에게 키가 보이나 |
|---|---|---|---|---|
| **Sublime Text 4** | **1레벨 flat**(`font_size` · `tab_size` · `word_wrap` · `draw_white_space` · 약 200키) · 문법별 오버라이드는 파일 단위(`SQL.sublime-settings`) · 패키지 설정은 패키지별 파일 | JSON(주석 허용) · Default/User 2열 편집 | 설정 "화면"이 없다 — 파일 자체가 UI(왼쪽 기본값 · 오른쪽 사용자) · 키 = 문서 | 늘 보인다(파일이 UI) |
| **VS Code** | **2~3레벨 점 표기**(`editor.fontSize` · `workbench.colorTheme` · `files.autoSave` · `[sql]` 언어 오버라이드 · 1,000+키) | `settings.json`(사용자·작업 영역·폴더 3층) | 설정 UI의 **목차(TOC) = 접두 → 절**의 표(`Commonly Used · Text Editor ▸ Font · Workbench ▸ Appearance …`) — 접두 하나가 여러 절로 흩어질 수 있어 **검색이 주 경로**(id·제목·설명·별칭 `@id:` `@tag:`) · 카드에 키 id 표시 + "Copy Setting ID/JSON" 톱니 메뉴 | 카드에 id · JSON 편집으로도 |
| **IntelliJ IDEA** | 화면 = **트리**(Appearance & Behavior ▸ Editor ▸ …) · 저장 = XML **컴포넌트별 옵션**(`options/editor.xml` `<option name="LINE_SPACING" value="1.2"/>` — 키가 컴포넌트 이름 + 옵션 이름 2레벨) · 고급은 **Advanced Settings**(검색 가능 · id `ide.tooltip.initialDelay`식 점 표기) · Registry는 개발자용 | XML 파일 다수 | 트리 위치와 XML 키는 **무관**(사용자에게 키를 보이지 않음) · Advanced Settings만 id를 보인다 | 거의 안 보인다 |
| **DBeaver** | Eclipse **preference store** 점 표기(`resultset.maxrows` · `sql.editor.autosave.enabled` · `navigator.show.system.objects` · 플러그인별 파일) | `.metadata/.plugins/org.eclipse.core.runtime/.settings/*.prefs` | Preferences **트리**(General · Editors ▸ SQL Editor · Connections ▸ …) · 키는 화면에 없음 · 검색은 페이지 제목만 | 안 보인다(파일에서만) |

관찰:
- **키를 사용자에게 보이는 제품(Sublime · VS Code)은 키 = 문서**이고, 그래서 이름의 뜻이 중요하다. 우리는 VS Code처럼 카드에 키를 보이고 CLI `nsql config`에서 키를 그대로 쓰므로 이쪽이다.
- VS Code도 "접두 = 화면 절"이 **완전히 1:1은 아니다**(`editor.minimap.*`은 Text Editor ▸ Minimap · `workbench.editor.*`는 Workbench ▸ Editor Management) — 대신 **검색 + id 표시 + 복사**로 보완한다. 우리도 같은 보완을 넣었다(검색은 있었고 · 복사 버튼·정렬은 이번).
- Sublime의 flat은 파일 200줄을 통째로 읽는 사용 방식에서만 통한다. 500키 파일을 flat으로 두면 `max_rows`가 그리드인지 로그인지 이름에 넣어야 하고, 결국 `grid_max_rows`처럼 접두가 `_`로 바뀔 뿐이다.

## 2. 현 상태(09-28 · 스크립트 집계)

- 키 498 · 접두 46 · 카테고리 27(트리 8 그룹).
- 접두 ↔ 카테고리 **1:1인 접두** 34(`key. intel. grid. explorer. log. window. oracle. project. …`).
- **한 카테고리에 접두가 둘 이상**(위치를 키에서 못 읽는 곳 · 11):

| 카테고리 | 접두 | 비고 |
|---|---|---|
| Session | `session.`(8) `tx.`(14) `vars.`(11) `run.`(8) `db.`(3) `script.` `license.` `settings.` | 가장 심함 — 78 §3-4는 세션 / 트랜잭션 보호 / 스크립트·변수 / 알림(실행 카드)으로 쪼갠다 |
| Performance | `mem.` `perf.` `gfx.` `clipboard.` `editor.undo_*`(5) | 되돌리기 5개는 편집기 ▸ 되돌리기로(78) |
| Connection | `conn.`(7 · 로그인 창 UI) `connect.`(4 · 접속 동작) `probe.`(8) `net.` | **`conn.`/`connect.` 두 표기**(§5) · probe = "서버 상태" 카테고리 후보 |
| Explorer | `explorer.`(23) `meta.`(14) `gen.`(5) | meta = 메타 갱신·캐시(같은 화면의 일이라 접두를 남긴다) |
| Files | `file.`(18) `search.`(7) | 파일 검색 카테고리 신설 후보(78) |
| Appearance | `ui.`(33) `toolbar.` `tabs.` | — |
| Log | `log.` `txlog.` `demo.` | `demo.prompted` = HIDDEN |
| Grid | `grid.` `sql.`(1) | `sql.copy_keys`류 1개 → `grid.`로 |
| CLI | `cli.` `bulk.` | bulk = `nsql import/export` 대량 I/O — CLI 카테고리에 두되 접두 유지 |
| Window | `window.` `statusbar.` | — |
| Editor | `editor.` `layout.`(1) | `layout.editor_split_pct` = HIDDEN |

- **접두가 카테고리와 어긋난 키**(접두는 A인데 다른 카테고리에 있는 것): `ui.ime_hint`(Appearance) → 09-28 `input.ime_hint`로 · `editor.undo_*` 5(Performance) · `editor.tab_line_scratch`(뜻이 바뀌어 `_unsaved`) · `license.gates_dev`(Session · 뜻 바뀜 `license.gates`).

## 3. 왜 못 찾나(원인 셋)

1. **접두가 "일"을 가리키고 카테고리는 "화면"을 가리킨다** — `tx.stale_min`은 트랜잭션의 일이지만 화면에는 "세션" 카테고리에 있다. 사용자는 키를 보고 "트랜잭션" 카테고리를 찾는데 없다.
2. **카테고리 안 순서가 등재 순**이라 같은 접두가 흩어진다(Session 안에 `session.*`이 세 군데) — 이번에 `display_order`로 접두 묶음 정렬(§6-3).
3. 카드에 키는 있지만 **가져갈 길**(복사)이 없었고, 트리 경로(그룹 › 카테고리)는 검색 결과에서만 보였다.

## 4. 1레벨 vs 멀티레벨 — 추천

| 기준 | 1레벨(Sublime) | **2레벨(권장)** | 3레벨+(VS Code 일부) |
|---|---|---|---|
| 뜻 | `max_rows`만으로는 대상 불명 → `grid_max_rows`로 길어짐 | `grid.max_rows` — 대상 + 값 | `grid.fetch.max_rows` — 절까지 |
| 위치 연계 | 없음(파일 = UI) | **접두 = 카테고리**로 가능(1:1 유지 시) | 절까지 연계되나 절 이동 때마다 키가 깨짐 |
| 길이 | 같은 뜻이면 오히려 김(`_` 접두) | 짧다 | 길다 |
| CLI/검색 | `config set max_rows` 충돌 | `config list grid` 접두 필터가 곧 화면 | 세 토막 타이핑 |
| 마이그레이션 | 전면 | 소수 | 절 재편마다 |

**추천 = 2레벨 고정** + 다음 셋:
- **접두 = 설정 창 카테고리 1:1**을 목표로 카테고리를 쪼갠다(78 §3 안 채택 · 키는 그대로).
- 접두를 못 맞추는 소수(§2 "어긋난 키")만 이름을 바꾼다 — `RENAMED` 표로 옛 파일·CLI 호환.
- 카드에 **경로(그룹 › 카테고리)** 를 검색 결과뿐 아니라 늘 보이고(78 §7 절 머리글과 같이 T-186에서), 키 옆 **복사 버튼**(09-28 구현).

## 5. 동의어·중복 검토(09-28)

"같은 뜻, 다른 이름" 쌍은 **없다** — 비슷해 보이는 것은 전부 대상이 다르다. 대신 **낱말 표기가 둘인 곳**이 있다:

| 항목 | 사례 | 판정 · 규칙 |
|---|---|---|
| `max_*` vs `*_max` | `grid.max_rows` `log.max_lines` `search.max_file_kb`(25) ↔ `explorer.index_max` `intel.max_items`·`run.toast_max`·`editor.undo_max`(31) | 둘 다 쓰인다. **규칙(새 키)**: 상한은 `max_<대상>`(`max_rows`) · 대상이 접두에 이미 있으면 `_max`(`explorer.index_max`) — 즉 **뒤에 오는 낱말이 대상**일 때만 `max_` 접두 |
| `conn.` vs `connect.` | `conn.*` 7 = 로그인 창 UI(폭·확인 시간) · `connect.*` 4 = 접속 동작(동시 수·재접속) | 대상이 다르다(창 vs 동작) → `conn.` → **`login.`** 으로 바꾸면 뜻이 선다(§6-2 후보) |
| `secs`/`ms`/`min` | `tx.stale_min` `session.idle_secs` `ui.fade_fast`(ms 접미 없음) `ui.copy_feedback_ms` | 단위 접미는 **늘 붙인다** — `ui.fade_fast/slow`·`ui.slide_ms`처럼 빠진 3개는 §6-2 |
| 색 | `*_color` 통일(`editor.tab_accent`만 예외) | `editor.tab_accent` → `editor.tab_accent_color` 후보 |
| 켜기/끄기 | `*.enabled` · `*.visible` · 접두만(`editor.minimap`) | 셋 다 뜻이 다르다(기능 켬 / 보임 / 표시 요소) → 유지 |
| 글꼴 | `ui.font_face`·`editor.font_face`·`grid.font_face`·`explorer.font_size` | 대상별 · 중복 아님 |
| 상한 KB | `search.max_file_kb` `ext.rainbow_pairs.max_kb` `intel.max_doc_kb` `file.external_merge_max_kb` `log.file_max_kb` | 대상별 · [72](72-size-limits-and-large-file-constraints.md) 원장 |
| 스레드 | `search.threads` `project.scan_threads` | 대상별 |
| 더블클릭 | `ui.dblclick_ms` 하나 | — |
| IME | `ui.ime_hint` ↔ `ui.ime_hint_watch`(부모·자식) | 09-28 `input.`으로 · 종속 등재 |

## 6. 이행

### 6-1. 1차(이번 세션 · 구현)
- 부품: `nsql_settings::RENAMED`(옛→새) · `Settings::migrate_renamed`(읽기 때 옮김 · 새 키가 있으면 옛 값 버림 · 옛 줄은 다음 저장 때 사라짐) · `canonical_key`.
- 적용: `ui.ime_hint`→`input.ime_hint` · `ui.ime_hint_watch`→`input.ime_hint_watch`(둘 다 Input 카테고리 · 종속) · `editor.tab_line_scratch`→`editor.tab_line_unsaved`(뜻이 "미저장 탭"으로) · `license.gates_dev`→`license.gates`(D-145).
- 설정 창: `display_order` 정렬 · 키 복사 버튼 · `ADVANCED`/`is_advanced` · 큰 파일 카테고리(`CatLargeFiles`).

### 6-2. 2차 후보(D-146 결정 뒤 · T-250) — ✅ 09-29 적용(아래 표 그대로 · `RENAMED` 11쌍 · 새 분류 Server status / Transaction safety / Script & variables / Run cards / Undo · 옛 키는 읽기 마이그레이션·CLI `config`에서 그대로 통함)
| 옛 | 새 | 이유 |
|---|---|---|
| `conn.*`(7) | `login.*` | 로그인 창 UI임을 이름이 말하게 · `connect.`와 표기 충돌 해소 |
| `editor.undo_*`(5 · Performance) | 그대로 · 카테고리만 편집기 ▸ 되돌리기(78) | 접두는 맞다 |
| `sql.copy_keys`류 `sql.`(1) | `grid.` | Grid 카테고리 |
| `ui.fade_fast` `ui.fade_slow` `ui.slide_ms`(있음) | `ui.fade_fast_ms` `ui.fade_slow_ms` | 단위 접미 |
| `editor.tab_accent` | `editor.tab_accent_color` | 색 접미 |
| `tx.*` `vars.*` `run.toast*` | 그대로 · 카테고리 쪼갬(78 §3-4) | 접두 = 일 |
| `demo.prompted` `settings.*` `layout.*` | HIDDEN 유지 | 화면에 없음 |

### 6-4. 확장 설정 키 규칙(09-30 · 사용자 "확장은 `ext.` 구분을 앞에")

- **확장이 소유한 설정 키 = `ext.<확장>.<키>`** — `<확장>` = 확장을 식별하는 짧은 이름(snake_case · 예 `sqlfmt_kiros33` · `rainbow_pairs` · `hello`) · 매니페스트 `settings_prefix` = `ext.<확장>.` · 호스트는 접두로 그 확장의 설정만 넘긴다.
- 3차 이름 바꿈(`RENAMED` 13쌍 · 옛 줄은 읽기 이주 · CLI `config`는 옛 키도 통함): `sqlfmt.*` → `ext.sqlfmt_kiros33.*` · `rainbowpair.*` → `ext.rainbow_pairs.*` · 샘플 `hello.say` → `ext.hello.say`.
- 새 확장은 처음부터 이 규칙으로(30 §2 확장점 규칙 · 68 SDK 문서).

### 6-5. ★ 시간 단위 규칙 + 단위 변환 이주(09-30 · 사용자 "10초를 초과하면 초 OK · 그 이하면 ms · 기존 설정 마이그레이션")

- 규칙: **기본값이 10초 이하인 시간 설정 = ms(`_ms`)** · 10초 초과 = 초(`_secs`) · 분(`_min`) 그대로 · `0 = 끔`인 타임아웃류(`session.call_timeout_secs` · `tx.*_timeout_secs` · `db.statement_timeout` · `db.cursor_idle_secs`)는 보통 값이 수십 초 이상이라 초 유지.
- 부품 **`nsql_settings::RESCALED`** `(옛 키, 새 키, 배수)` — `migrate_renamed`가 옛 줄을 새 키로 **곱해서** 옮긴다(옛 기본값 × 배수 = 새 기본값이라 줄이 남지 않음 · 새 키가 이미 있으면 옛 값 버림 · 소수 옛 값 허용) · `canonical_key`/`alias_scale` · `Settings::set(옛키)` = 옛 단위 해석(×배수) · `Settings::get_as(옛키)` = 옛 단위로 되돌려 답(CLI `config get`) · `get` = 늘 새 단위.
- 적용(6 + 접미 1): `meta.refresh_idle_secs`→`meta.refresh_idle_ms` · `probe.timeout`→`probe.timeout_ms`(하한 100) · `probe.retry_delay`→`probe.retry_delay_ms`(하한 5000 = 26 §8) · `ui.toast_secs`→`ui.toast_ms` · `run.toast_hide_secs`→`run.toast_hide_ms` · `project.autosave_change_secs`→`project.autosave_change_ms` · `explorer.typeahead_timeout`→`explorer.typeahead_timeout_ms`(값 그대로 · RENAMED).
- 새 시간 키는 처음부터 이 규칙으로 · 단위를 바꾸면 RESCALED 한 줄 + 라벨 `(ms)` + 사용처 `from_millis` + 문서 키 이름 + 시험 `rescaled_keys_migrate_with_unit`(배수 검산).

### 6-6. 분류 "결과 필터" · "고속 스크롤"(10-06 · 사용자 요구)

- **데이터 편집기 ▸ 결과 필터**(새 분류 · 종전 결과 셋 분류에서 분리): `grid.filter_enabled`(사용 여부 · 새 키) · `grid.filter_funnel`(always/hover/none · 종전 on/off → 선택) · `grid.filter_strip` · `grid.filter_pick_max` · `grid.filter_values_max` · `grid.filter_popup_rows`(새 키 · 12) · `grid.filter_values_scope`(others/all · 새 키) · `grid.filter_list_max` · `grid.condition_bar`(인라인 조건 입력란 · 기본 on · 새 키 · 10-06 4차).
- **사용자 인터페이스 ▸ 고속 스크롤**(새 분류 · 종전 입력 분류에서 분리): `scroll.*` 8키(`scroll.fast` · `fast_speed` · `fast_grid_extra` …).
- 키 이름은 그대로(접두 = 일 · 분류만 나눔 · `RENAMED` 없음). 검색 `filter` · `scroll`로 두 분류가 바로 나온다(journal 10-06 §9 ②).

### 6-3. 설정 창 규칙(구현)
- 카드 순서 = **그룹 → 카테고리 → 접두 묶음(그 접두가 처음 등재된 자리 순) → 등재 순**(`display_order`) — 검색 결과도 같은 순.
- 키 이름 오른쪽 **복사 버튼**(글꼴 높이 · 클릭 = 키 복사 → ✓ → `ui.copy_feedback_ms` 뒤 원복 · Shift/Ctrl(⌘)+클릭 = 보이는 설정 전부 `# 카테고리 › 라벨` + `키=값` 형식으로).
- **고급**(`is_advanced` = HIDDEN ∪ DBMS 그룹 ∪ `ADVANCED` 표) — Advanced 스위치(라벨 오른쪽) 꺼짐 = 숨김 + 목록 위 "고급 설정 N개 숨김" 한 줄 · 켜짐 = 키 이름 강조색.

## 7. 성능·기능 영향
- 정렬은 카드 재구성 때 한 번(500키 × 튜플 비교 · μs 단위) · 그리기 비용 0 추가(복사 버튼은 카드당 아이콘 하나).
- 마이그레이션은 파일 읽기 때 `unknown` 목록만 훑는다(옛 키가 없으면 0).
- 기능: 키 계약은 `RENAMED`로 보존(CLI `config get/set 옛키` = 새 키로 통함 · `canonical_key`).
