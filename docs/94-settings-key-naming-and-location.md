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

- **10-20 3차(D-313)**: Ctrl 관련 7쌍 = `objlink.click`→`ctrl.click` · `editor.desc_inline_ctrl`→`ctrl.desc_inline` · `editor.inline_ctrl_scope`→`ctrl.desc_scope` · `editor.inline_ctrl_full_kb`→`ctrl.full_kb` · `editor.inline_ctrl_off_kb`→`ctrl.off_kb` · `bookmark.inline_label_ctrl`→`ctrl.bookmark_label` · `bookmark.inline_label_ctrl_scope`→`ctrl.bookmark_scope` · 새 분류 Ctrl Key(접두 `ctrl.` ↔ 분류 1:1 · §2 원칙).

- **10-24(138차)** 새 키 = `vars.refresh_on_run`(Script ▸ Variables · Bool · 기본 on · D-355) · `ui.theme_watch`(Appearance · ADVANCED · Linux 테마 감시) · `odbc.cancel`(HIDDEN · DBMS ▸ 드라이버 · auto|on|off · T-414) · `driver.info_clickhouse`(DBMS ▸ 드라이버 정보 행 · T-415) · `driver.info_db2` · `driver.info_access`(10-25 · DBMS ▸ 드라이버 정보 행 · 안내문 · `config list` 제외 = info 키 규칙) · **구현(T-422 · D-357 · 855b9ad6)** = `outline.show_select` · `outline.show_dml` · `outline.show_ddl` · `outline.show_other`(off · 초안 `show_commands`에서 바뀜) · `outline.show_declared`(on) · `outline.expand_depth`(1 · Int 0~8) · `outline.follow_caret`(on) — 분류 Editors ▸ Outline · **`outline.sort`(position|name|kind · 기본 position · 10-25 c9134a63 구현)** · **`outline.grid_columns`(Bool · 기본 on · Editors ▸ Outline · 마지막 포커스가 그리드면 컬럼 목록 · T-423 ② 4d160da9)**.

- **10-26(139차 win)** 새 키 = `ui.icon_prewarm`(Appearance · Bool · 기본 on · HIDDEN · T-430 · 아이콘 마스크 선굽기 스레드 `nsql-icons` 스위치 · 39 §3-3).

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
- ★ **동적 등록(10-27 · 146차 · T-258)** — 확장이 모듈 메타 `settings`(SDK `SettingDef` · 키 = `ext.<확장>.<키>`)로 선언하면 런타임 레지스트리 층에 들어가 설정 창 분류(확장 이름)·`Settings::get`·게스트 설정 JSON에 나온다 · **앱 `REGISTRY`에 정적으로 더하지 않는다**(기존 `ext.sqlfmt_kiros33.*` · `ext.rainbow_pairs.*`는 정적 유지) · 한계 = CLI `nsql config`는 동적 키를 모른다(`UnknownKey`) · 종속 잠금(§8)은 정적 `DEPENDS`만.
- ★ **확장 명령 단축키(10-28 · 149차 · D-370)** — 확장이 메타 `commands[].key`로 기본 키를 내면 호스트가 **`key.<명령 id>` 설정을 그 확장 분류에 동적 등재**한다(라벨 "Shortcut: <명령>" · Text · 기본 "" = 확장 기본 키 · `none` = 없음) → 사용자 지정이 키맵·메뉴 표시에 그대로 반영(`Keymap::from_settings` · `sync_menu_shortcuts`가 동적 명령도 돈다). 앱 정적 명령의 `key.*`(정적 레지스트리)와 같은 이름 규칙.
- ★ **RENAMED의 새 키가 동적 키일 때(10-28)** — 기동 때 새 키가 아직 미등록이면 옛 값을 **새 이름으로 unknown에 보관**(종전 = 폐기) → 확장 등재 직후 `adopt_unknown`이 값으로 올린다. 예 = `editor.seq_default` → `ext.insert_sequence.default`.

### 6-5. ★ 시간 단위 규칙 + 단위 변환 이주(09-30 · 사용자 "10초를 초과하면 초 OK · 그 이하면 ms · 기존 설정 마이그레이션")

- 규칙: **기본값이 10초 이하인 시간 설정 = ms(`_ms`)** · 10초 초과 = 초(`_secs`) · 분(`_min`) 그대로 · `0 = 끔`인 타임아웃류(`session.call_timeout_secs` · `tx.*_timeout_secs` · `db.statement_timeout` · `db.cursor_idle_secs`)는 보통 값이 수십 초 이상이라 초 유지.
- 부품 **`nsql_settings::RESCALED`** `(옛 키, 새 키, 배수)` — `migrate_renamed`가 옛 줄을 새 키로 **곱해서** 옮긴다(옛 기본값 × 배수 = 새 기본값이라 줄이 남지 않음 · 새 키가 이미 있으면 옛 값 버림 · 소수 옛 값 허용) · `canonical_key`/`alias_scale` · `Settings::set(옛키)` = 옛 단위 해석(×배수) · `Settings::get_as(옛키)` = 옛 단위로 되돌려 답(CLI `config get`) · `get` = 늘 새 단위.
- 적용(6 + 접미 1): `meta.refresh_idle_secs`→`meta.refresh_idle_ms` · `probe.timeout`→`probe.timeout_ms`(하한 100) · `probe.retry_delay`→`probe.retry_delay_ms`(하한 5000 = 26 §8) · `ui.toast_secs`→`ui.toast_ms` · `run.toast_hide_secs`→`run.toast_hide_ms` · `project.autosave_change_secs`→`project.autosave_change_ms` · `explorer.typeahead_timeout`→`explorer.typeahead_timeout_ms`(값 그대로 · RENAMED).
- 새 시간 키는 처음부터 이 규칙으로 · 단위를 바꾸면 RESCALED 한 줄 + 라벨 `(ms)` + 사용처 `from_millis` + 문서 키 이름 + 시험 `rescaled_keys_migrate_with_unit`(배수 검산).

### 6-6. 분류 "결과 필터" · "고속 스크롤"(10-06 · 사용자 요구)

- **데이터 편집기 ▸ 결과 필터**(새 분류 · 종전 결과 셋 분류에서 분리): `grid.filter_enabled`(사용 여부 · 새 키) · `grid.filter_funnel`(always/hover/none · 종전 on/off → 선택) · `grid.filter_strip` · `grid.filter_pick_max` · `grid.filter_values_max` · `grid.filter_popup_rows`(새 키 · 12) · `grid.filter_values_scope`(others/all · 새 키) · `grid.filter_list_max` · `grid.condition_bar`(인라인 조건 입력란 · 기본 on · 새 키 · 10-06 4차).
- **사용자 인터페이스 ▸ 고속 스크롤**(새 분류 · 종전 입력 분류에서 분리): `scroll.*` 8키(`scroll.fast` · `fast_speed` · `fast_grid_extra` …).
- 키 이름은 그대로(접두 = 일 · 분류만 나눔 · `RENAMED` 없음). 검색 `filter` · `scroll`로 두 분류가 바로 나온다(journal 10-06 §9 ②).

### 6-7. 편집기 줄 간격 2키(10-21 · 134차 win · [118](118-editor-line-spacing.md) · D-315 확정 대기)

- **편집기 ▸ Editor**(분류 `CatEditor` · `editor.font_size` 다음): `editor.line_height_mode`(Choice multiple \| exact \| at_least · 기본 multiple) · `editor.line_height`(Text · 기본 `1.2` · 방식에 따라 배수 또는 px).
- 방식과 값을 두 키로 나눈 이유 = VS Code `editor.lineHeight`처럼 값 크기(< 8 = 배수)로 뜻을 가르면 뜻이 숨는다 · 새 키라 `RENAMED` 없음.


### 6-8. 확장 드라이버 다운로드 키(10-22 · 136차 · T-29 · D-348 · 10-23 = 분류 DBMS ▸ 드라이버)

> ★ **10-23(542250c) = 새 분류 DBMS ▸ 드라이버**(`CatDbmsDrivers` · DBMS 그룹 맨 앞 · 고급 토글 대상)로 모음 = `driver.auto_install`(접속 → 이동) · `driver.repo` · `driver.http_timeout_secs`(CatInternal HIDDEN → **노출**) + 읽기 전용 info 3(`driver.info_folder` · `driver.info_index` · `driver.info_installed`(+ [드라이버 관리자…] 버튼)) · DBMS별 드라이버 행 `mysql.info_driver` 신설(pg/mssql/sqlite/oracle info 키와 같은 꼴 · 내장/확장 설치됨/미설치 3상태). 아래 줄의 분류 표기(CatInternal · 접속)는 그 전 기록.

- 분류 = **CatInternal**(설정 창에 안 보임 · HIDDEN) · `driver.repo`(Text · 기본 **`SosomLab/nexa-sql`**(D-349 · 10-22 · 별도 드라이버 저장소 없음) · 색인 `drivers/index.json`(drivers.yml 봇이 갱신)·자산(Releases 태그 **`drivers-v<ver>`** · pre-release · `nsql-driver-<id>-<ver>-<os>-<arch>.zip`/`.sig`/`.sha256` · 10-29 T-454 정정 · 종전 표기 "`driver-*`")을 받을 GitHub 저장소) · `driver.http_timeout_secs`(Int 5~600 · 기본 30 · 10 s 초과라 `_secs` · §6-5).
- 토큰은 설정 키로 두지 않는다 = 환경 변수 `NSQL_GITHUB_TOKEN`만(설정 파일에 비밀을 남기지 않음) · 자동 갱신 확인 키 `driver.check_updates`는 GUI 배지와 함께 2차.
- ★ **`driver.auto_install`**(10-23 · D-350 C단계) = Bool · 기본 **on** · 분류 **접속**(`CatConnection` · 설정 창에 보임) · 라벨 "없는 드라이버 자동 설치" · 끄면 접속 시 -1350 + 설치 안내만.
- ★ **`driver.check_updates`**(10-23 · fbe5001) = Bool · 기본 **on** · 분류 **DBMS ▸ 드라이버** · 라벨 "드라이버 갱신 하루 1회 확인" · 색인 캐시 24시간 초과일 때만 GET 1 · 새 버전 = 상태줄 안내 + 로그(자동 교체 없음) · ★ **10-27 설명 갱신(D-364)** = 영어 "Once a day at startup, re-read the driver index; when installed drivers have newer versions, show a notice with an [Update] button (status bar and log too). Nothing is replaced until you click it" · 한국어 "하루 1회 기동 때 드라이버 색인을 다시 읽어 설치된 드라이버에 새 버전이 있으면 [갱신] 버튼이 있는 안내를 띄웁니다(상태줄·로그에도). 누르기 전에는 바꾸지 않습니다"(`Msg::DescDriverCheckUpdates`).

### 6-9. SSH 터널 키(10-28 · 159차 · T-3 · D-388)

- `ssh.command` = Text · 기본 `ssh` · 분류 **접속**(`CatConnection`) · **고급**(ADVANCED) · 빈 값 = 터널 비활성("SSH: tunnels are disabled …") · 경로를 넣으면 그 실행 파일.
- `login.window_h` 기본값 614 → **648**(10-28 · 160차 · 접속 창 SSH 줄 · D-389 · 저장한 값은 유지).
- `ssh.connect_timeout_secs` = Int 1..=120 · 기본 15 · 분류 접속 · 라벨 "SSH 터널 대기 상한(초)" · 포워딩 포트가 열릴 때까지 + ssh `ConnectTimeout` · 10초 초과라 `_secs`(§6-5).

### 6-10. 텍스트 셰이핑 스위치(10-29 · 166차 · T-11 · D-383)

- `ui.text_shaping` = Bool · 기본 **off** · 분류 **모양**(`CatAppearance`) · 라벨 `LblTextShaping` · UI 텍스트(라벨·메뉴·대화상자 · 비례 글꼴)만 rustybuzz 배치로 그린다(편집기·그리드 고정폭 제외) · 켜면 OS 래스터(GDI/CoreText) 대신 글리프 래스터 · 스파이크 A/B 측정용 · 변경 즉시 반영.

### 6-11. OFFSET 재질의 겹침 검사(10-29 · 171차 · T-116)

- `grid.overlap_check` = Bool · 기본 **켬** · 분류 = 결과 그리드 · OFFSET 재질의로 다음 페이지를 받을 때 직전 페이지 꼬리 1행을 겹쳐 받아 대조(다르면 오류 · 같으면 꼬리 떼고 이어 붙임) · 끄면 종전 동작 · 페이지당 한 행 추가 전송([43 §9-3](43-fetch-model-and-result-tabs.md)).

### 6-12. CLI 셸 기록(10-29 · 175차 · T-453)

- `cli.history_max` = Int 0~100000 · 기본 **500** · 분류 CLI(`CatCli`) · `nsql shell` 대화형(터미널)일 때 ↑/↓ 기록에 남기는 줄 수 · 파일 = 설정 폴더 `shell_history` · **0 = 기록 안 함** · 파이프·배치(비TTY)는 늘 기록 안 함.

### 6-13. 서버 호출 상한 라벨(10-29 · 176차 · T-128)

- `session.call_timeout_secs` 라벨 "Oracle 호출 상한(초)" → **"서버 호출 상한(초)"**(`LblCallTimeout`) · 설명 = Oracle·SQL Server 공용(Oracle = 넘겨도 세션 유지 · SQL Server = 소켓 닫고 다음 실행에 재접속) · 키·단위·기본(0 = 끔) 그대로.

### 6-14. 자동 커밋 · 자동완성 설정의 CLI 범위(10-29 · 178차 · T-453)

- `session.autocommit`(기본 on) = GUI + **`nsql shell`** · `nsql run`·export·import 등 = 이 설정과 무관하게 수동 + 끝날 때 커밋(SQL*Plus 관례 · 스크립트 호환 · 사용자 결정 ⑤b) · 설명(`DescAutocommit`)에 CLI 범위 명시 · 셸·스크립트 `SET AUTOCOMMIT`이 우선.
- `intel.insert_alias` · `intel.alias_style` · `intel.alias_upper` · `intel.insert_case` = GUI 코드 완성 + **CLI 셸 Tab**(`CompleteCfg::from_settings` · 셸 기동 때 읽음) · `intel.insert_alias` 기본 off 그대로(사용자 10-30 "현행 유지").

### 6-15. 문자열 안 `&` 치환(10-30 · 179차 · T-149 · D-141)

| 키 | 종류 | 기본 | 분류 | 뜻 |
|---|---|---|---|---|
| `script.define_in_strings` | Bool | **on** | 스크립트·변수(`CatScriptVars`) | on = SQL*Plus 그대로(`'…'` 안 `&이름`도 치환) · off = 문자열 리터럴 안은 글자 그대로·묻지 않음(`'R&D'` 보존) · 주석 안은 늘 치환 없음 · GUI·CLI(러너 5곳) 공용 |

- 상태줄 `&` 세그먼트(블록 `define` · `statusbar.layout`)의 auto/on/off는 이 키가 아니라 `script.define`을 바꾼다(모든 세션에 즉시).

### 6-16. 들여쓰기 감지(10-30 · 180차 · T-69)

| 키 | 종류 | 기본 | 분류 | 뜻 |
|---|---|---|---|---|
| `editor.detect_indent` | Bool | **on** | 편집기 | 파일을 열 때 앞 2000줄에서 탭/공백·폭을 감지해 **그 탭에만** 적용(전역·확장자 설정 불변) · 끄면 종전 계층 그대로 · 상태줄 팝업 "버퍼에서 감지"는 이 키와 무관하게 언제든 |

### 6-17. 미니맵 자동 숨김(10-30 · 180차 · T-110 2-5)

| 키 | 종류 | 기본 | 분류 | 뜻 |
|---|---|---|---|---|
| `editor.minimap_min_cols` | Int(0~300) | **60** | 편집기 ▸ 미니맵 | 본문 폭이 이 열 수보다 좁으면 미니맵을 그리지 않음(`editor.minimap` 켬 설정은 유지 · 넓히면 복귀) · 0 = 늘 표시 · 변경 즉시 반영 |

### 6-18. 미니맵 저장 뒤 수정 줄(10-30 · 180차 5차 · T-110 2-4)

| 키 | 종류 | 기본 | 분류 | 뜻 |
|---|---|---|---|---|
| `editor.minimap_dirty` | Bool | **off** | 편집기 ▸ 미니맵 | 저장본 대비 추가·수정·삭제 줄을 미니맵 왼쪽 가장자리에 표시(거터 표시 `editor.diff_marks`와 별개) · 변경 즉시 반영 · 기준 = 열 때·저장 때의 본문 |

### 6-19. 미니맵 git 변경 줄(10-30 · 181차 8차 · T-110 2-3)

| 키 | 종류 | 기본 | 분류 | 뜻 |
|---|---|---|---|---|
| `editor.minimap_git` | Bool | **on** | 편집기 ▸ 미니맵 | git 저장소 안 파일이면 HEAD 대비 추가·수정·삭제 줄을 미니맵 왼쪽 **안쪽 열**에 표시(바깥 열 = `editor.minimap_dirty`) · 끄면 기준선 비움 · 변경 즉시 · **`statusbar.git`이 꺼져 있으면 HEAD를 읽지 않으므로 표시 없음** · 갱신 주기 = `statusbar.git_secs` |

### 6-20. 오프라인 메타 상한(10-30 · 182차 9차 · T-121 ②)

| 키 | 종류 | 기본 | 분류 | 뜻 |
|---|---|---|---|---|
| `meta.cache_mb` | Int(0~4096) | **256** | 성능 | 세션이 모두 끊긴(오프라인) 서버 탐색기들이 들고 있는 메타의 합계 상한(MB) · 넘으면 오래 안 쓴 서버부터 컬럼 층 → 전체 순으로 비움 · 0 = 상한 없음 · 점검 주기 = 세션 유휴 틱(`session.idle_secs` = 0이면 점검 안 함) |

### 6-3. 설정 창 규칙(구현)
- 카드 순서 = **그룹 → 카테고리 → 접두 묶음(그 접두가 처음 등재된 자리 순) → 등재 순**(`display_order`) — 검색 결과도 같은 순.
- 키 이름 오른쪽 **복사 버튼**(글꼴 높이 · 클릭 = 키 복사 → ✓ → `ui.copy_feedback_ms` 뒤 원복 · Shift/Ctrl(⌘)+클릭 = 보이는 설정 전부 `# 카테고리 › 라벨` + `키=값` 형식으로).
- **고급**(`is_advanced` = HIDDEN ∪ DBMS 그룹 ∪ `ADVANCED` 표) — Advanced 스위치(라벨 오른쪽) 꺼짐 = 숨김 + 목록 위 "고급 설정 N개 숨김" 한 줄 · 켜짐 = 키 이름 강조색.

## 7. 성능·기능 영향
- 정렬은 카드 재구성 때 한 번(500키 × 튜플 비교 · μs 단위) · 그리기 비용 0 추가(복사 버튼은 카드당 아이콘 하나).
- 마이그레이션은 파일 읽기 때 `unknown` 목록만 훑는다(옛 키가 없으면 0).
- 기능: 키 계약은 `RENAMED`로 보존(CLI `config get/set 옛키` = 새 키로 통함 · `canonical_key`).

## 8. 설정 종속(잠금) 규칙(10-27 · 145차 win · T-443 · D-367)

- **원천** = `nsql_settings::DEPENDS: &[(자식, 부모, Dep)]` + `Dep`(`On` · `NotEmpty` · `Eq(값)` · `Ne(값)`) + `locked_by_chain` — 설정 창은 잠긴 카드를 흐리게 그리고 편집을 막는다(`prefs_win.rs`). **CLI `nsql config set`은 잠김과 무관하게 쓴다**(값을 미리 넣어 두는 길 · 종전 규약).
- **등재 기준** = 마스터가 꺼지거나 특정 값이 아니면 **아무 효과도 없는 키**만 등재한다 — 기능 스위치(`*.enabled` · `*_enabled` · `*.on`) 아래 세부 키 · 모드 값에서만 쓰이는 키(`=값` · `≠값`) · "0 = 끔"인 수치 아래 키(`Ne("0")`).
- **공용 키는 등재하지 않는다** — 다른 기능도 읽는 키를 한 기능의 스위치에 걸면 그 기능을 쓸 수 없게 된다. 예 = `intel.alias_style`·`alias_upper`(Generate SQL·CLI 공용 · D-367) · `intel.min_chars`·`key_passthrough`(결과 조건 바 완성 공용) · `editor.pair_*`(강조·이동 공용 코어) · `probe.stale_secs`(실행 전 생존 판정) · `connect.reconnect_same`(명시적 접속 규칙) · `vars.cli_global`(CLI가 따로 읽고 씀) · `perf.mode` 아래(개별 값이 프리셋보다 우선 · 강제 잠금은 boost뿐).
- **부모는 실제로 막는 키** — 같은 접두의 상위가 아니라 그 키를 읽는 코드 분기의 조건이 부모다(예 `objlink.card_buttons`·`hover_ms` ← `objlink.tooltip` · `file.external_poll_ms` ← `file.external_check = focus_poll`) · 확인 = 사용처 grep(`settings.get/flag("키")`가 어느 `if` 안에 있나).
- **사슬** = 조상 중 하나라도 조건을 못 채우면 잠김(깊이 8) · ★ **부정 조건(`Eq("off")` · `Ne`)은 부모 잠김을 물려받지 않는다** — "부모가 꺼졌을 때 쓰이는" 키가 조부모 때문에 잠기는 모순 방지(예 `tabs.close_select` ← `tabs.close_use_history = off`: `tabs.history`를 꺼도 열림).
- **값 목록 조건 확인** — `Eq`/`Ne`의 값이 그 키의 선택지(`*_OPTS`)에 실제로 있어야 한다(없으면 절대 안 잠기거나 늘 잠김 · 10-27 `explorer.mssql_system_dbs` ← `Ne("off")` 사례) · Int 부모에 `NotEmpty`는 의미 없음(`Ne("0")`).
- **표 한계(보류)** = 부모 1개뿐(부모 둘 중 하나 = `editor.desc_inline_*` · `ext.sqlfmt_kiros33.outlier_chars`) · 값 목록 조건 없음(`mysql.tls_ca` ∈ {verify_ca, verify_identity}) — 필요해지면 `Dep::Any(&[..])`·부모 목록으로 확장.
- **시험** = `depends_table_is_sound_and_locks_alias_style`(모든 자식·부모가 레지스트리에 있음 · 자기 참조·순환 없음 · 사슬 · 부정 조건 · alias_* 열림) — 새 키를 등재하면 이 시험이 같이 지킨다.
- **새 설정 키를 만들 때** = 마스터 아래 세부 키면 같은 커밋에 `DEPENDS` 한 줄 · 공용이면 등재하지 않고 설명(`Desc*`)에 "~에도 적용"을 적는다.

## 9. 런타임 overlay 층(10-27 · 148차 win · D-369)

- **무엇** = 확장이 기본 설정을 덮어쓸 때만 쓰는 메모리 층(nsql-settings `overlay`) — `get`/`effective`가 사용자 값보다 **먼저** 본다 · 파일에 저장하지 않는다(`save`·`is_modified` 영향 0) · 설정 창·`settings.conf`의 사용자 값은 그대로 남는다.
- **들어가는 길은 하나** = `Settings::set_overlay(owner, key, val)` — 레지스트리에 있는 키 · 형식 검증 · 읽기 전용 거절 · 확장 메타 `settings_write` 선언(자기 접두 `ext.<id>.`는 늘 허용) · 호출당 16개 · 재귀 상한 3.
- **나가는 길** = 확장이 꺼지거나 내려가면 `clear_overlay(owner)` + 그 키 재적용 — 사용자 값으로 즉시 원복("overlay cleared" 로그).
- **동적 키 승격** = 확장 메타 `settings`로 등재되는 키는 기동 때 아직 레지스트리에 없어 `unknown`에 들어간다 → 등재 직후 `adopt_unknown()`이 값으로 올린다(검증 · 로그 "adopted"). 등재 뒤 메모리로 바꾼 거울 키(`ext.<id>.use`)는 **바로 저장**한다(10-28 결함 B).
- **금지** = overlay로 파일 값을 바꾸는 우회 · 확장이 레지스트리 밖 키를 쓰는 것 · 사용자 값을 지우는 것.
