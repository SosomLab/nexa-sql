# 78. 설정 카테고리 재정리 — 유사 기능 묶기 · 선/후행 순서 · 독립/종속 계층 · 강제값(2026-09-24 · 100차 mac · 📐 안)

> 사용자 09-24: "카테고리를 다시 정리해 유사 기능끼리 묶고 관련 기능끼리 붙여 재배치 · 설정 순서에 선/후행 개념 · 독립 설정과 종속 설정의 계층(상위가 꺼지면/켜지면 하위가 설정 · 상위가 설정되면 하위가 특정 값으로)". 원천 = `nsql-settings::REGISTRY` · `CATEGORY_TREE` · `DEPENDS` · `perf::BOOST` · `HIDDEN` · `INFO_KEYS` · 설정 창 `prefs_win.rs`(T-39) · [24](24-settings-and-vscode-analysis.md).

## 0. 결론 여섯 줄

1. **트리 8 그룹 · 24 카테고리 · 434 키**를 **7 그룹 · 31 카테고리**로 다시 묶는다(§3) — 카테고리 = "한 화면에서 같이 결정하는 것" 단위 · 키 접두(`run.` `tx.` `vars.` `db.`)가 아니라 **사용자의 일**(실행 표시 · 트랜잭션 보호 · 변수) 단위.
2. 카테고리 안 **순서 규칙**(§4): ① 마스터 스위치 ② 그 종속 항목을 **바로 아래 들여쓰기** ③ 범위·동작 → 표시 → 한계·예산 ④ 자동 기억 값은 `HIDDEN`. 순서의 단일 원천 = 새 `LAYOUT` 표(레지스트리 자체는 그대로 두어 키 계약·마이그레이션 0).
3. **종속(잠금)** `DEPENDS`를 68 → 약 100으로 보강(§5 · 빠진 마스터 30여) — 조건 종류는 지금 셋(`On` · `NotEmpty` · `Eq`)에 `Gt(0)`(0 = 끔인 정수 키)을 더한다.
4. **강제값** = 새 `FORCES` 표(§6 · `(부모, 조건, 자식, 값)`) — `perf.boost`의 `BOOST`를 여기로 옮겨 일반화 · 유효값(`effective`)이 대체되고 설정 창은 "○○ 때문에 고정" · 저장값은 유지(부모를 되돌리면 복귀).
5. 설정 창: 종속 항목 들여쓰기 + 잠금/강제 사유 문구 · 검색은 그대로 · `nsql config list` 머리글은 `LAYOUT` 순서.
6. 적용은 **안 확인 뒤** T-186으로: ① 부품(`LAYOUT`·`FORCES`·`Gt`) ② 트리·순서 ③ `DEPENDS` 보강 ④ 설정 창 표시 ⑤ 위키·i18n(T-185와 같이).

## 1. 현 상태(09-24)

| 항목 | 값 |
|---|---|
| 그룹 → 카테고리 | General(Log·Session·Performance) · User Interface(Appearance·Input·Keys·Window·Explorer) · Editors(Editor·Intel·Files·Project) · Connections(Connection·CLI) · Data Editor(Grid) · DBMS(Oracle·MSSQL·PG·SQLite) · Bookmarks(3) · Extensions(Manager·Rainbow Pairs) |
| 키 수 | 434(Keys 65 · Editor 53 · Session 46 · Appearance 37 · Files 35 · Grid 24 · Intel 23 · Connection 21 · Log 19 · Explorer 18 · Window 17 · Performance 12 · Oracle 12 · Project 11 · …) |
| 종속 잠금 | `DEPENDS` 68(`On` 54 · `Eq` 9 · `NotEmpty` 5) |
| 강제값 | `perf.boost` → `BOOST` 27키(유일한 강제 메커니즘) · `perf.mode` 프리셋(`PERF` 24키 · 모드별 값) |
| 숨김·정보 | `HIDDEN` 65(자동 기억·구현 값) · `INFO_KEYS`(읽기 전용 파생) |
| 순서 | 카테고리 안 = 레지스트리 등재 순(기능 추가 순서라 흩어짐) |

## 2. 문제(분석)

- **한 카테고리에 다른 일이 섞임**: Session에 실행 카드(`run.toast*`)·트랜잭션 보호(`tx.*`)·변수(`vars.*`)·페치(`db.*`)·스크립트(`script.strict`) — 다섯 가지 일. Files에 대화상자·큰 파일·외부 변경·파일 검색·OS 아이콘. Appearance에 글꼴·글자 래스터·색·토스트·툴바·애니메이션·클립보드. Performance에 되돌리기 예산 5개(편집기와 떨어짐)·메모리 회수·글리프 캐시. Window에 상태줄 git. Log에 데모 키 2개.
- **선/후행이 뒤집힌 곳**: `editor.rulers`(값)가 `editor.rulers_show`(마스터) 앞 · `file.large_ask_mb`가 L1/L2 뒤 · `grid.result_tabbar_single`(종속)이 `result_tabs_max` 앞 · `intel.delay_ms`가 `auto_activation` 바로 아래지만 `trigger_chars`·`min_chars`는 "활성" 절과 떨어짐 · `bookmark.*` 종속 키 15개가 마스터와 다른 카테고리(Display·Anchor)에.
- **종속이 빠진 마스터**(잠금 없음 · 값을 바꿔도 뜻이 없음): `intel.enabled`(하위 22) · `intel.auto_activation`(delay·trigger·min) · `project.autosave`(secs·backup_days) · `file.external_change`(poll·merge_max·settle·backup_keep) · `grid.result_tabs`(max·per_statement·evict) · `editor.undo_persist`(mb·days) · `editor.highlight_selection`(occurrence_*) · `editor.whitespace_chars`(color·alpha) · `rainbowpair.enabled`(4) · `search.history_max`(rows) · `session.idle_secs`(idle_shared) · `log.dev_mode`(dev_layers ✓ 있음) · `mem.trim_secs`·`mem.trim_on_release` · `probe.enabled`(dns_cache·icmp·stale) · `explorer.visible`(width) · `statusbar.git` ✓ · `ui.toast_secs`(progress·alpha ✓ 일부).
- **강제값이 필요한데 없는 곳**: `intel.enabled=off` → 자동 활성·시그니처·아웃라인 강제 off(지금은 각 기능이 따로 검사) · `file.external_change=off` → merge off · `session.autocommit=on` → `tx.smart_commit` 뜻 없음 · `grid.result_tabs=off` → `result_per_statement` off · `bookmark.enabled=off` → 표시 4 off · `perf.mode=low` → (PERF 프리셋이 이미 함).

## 3. 새 트리(그룹 → 카테고리 → 절) — 안

절(§)은 카드 사이 소제목(설정 창 · 위키 · `config list` 머리글). `▸`는 종속(부모 바로 아래 들여쓰기).

### 3-1. 일반(General)
- **모양(Appearance)** — 언어 `ui.lang` · UI 글꼴 `ui.font_face`·`ui.menu_font_size` · 글자 래스터 절: `ui.text_gdi` ▸ `ui.text_hint` ▸ `ui.text_snap` · `ui.text_contrast`·`ui.text_weight` · 색 절: `ui.hover_color`·`ui.pressed_color`·`ui.color_recent`(HIDDEN) · 메뉴 절: `ui.menu_icons`·`ui.menu_max_width` · 툴바 `toolbar.layout`·`toolbar.hidden`(HIDDEN) · 탭 툴팁 `tabs.tooltip`
- **알림·움직임(Feedback & Motion)** *(신설 · Appearance에서 분리)* — 토스트 절: `ui.toast_secs` ▸ `ui.toast_alpha` ▸ `ui.toast_progress` ▸▸ `ui.toast_fade_to`·`ui.toast_bar_spent`(HIDDEN) · 실행 카드 절: `run.toast` ▸ `run.toast_hide_secs`·`run.toast_tick_ms`·`run.toast_follow`·`run.toast_max` *(Session에서 이동)* · 움직임 절: `ui.animations` ▸ `ui.fade_fast`·`ui.fade_slow`·`ui.fade_out_ms`·`ui.slide_ms`·`ui.hover_intent_ms` · `ui.max_fps` · 복사 확인 `ui.copy_feedback_ms` · 툴팁 `ui.tooltip_delay_ms`
- **입력(Input)** — `input.scroll_natural` · `ui.dblclick_ms` · `ui.ime_hint` · `ui.clipboard_probe` *(Appearance에서 이동)*
- **창(Window)** — `window.monitor` · `window.always_on_top` · 상태줄 절: `statusbar.git` ▸ `statusbar.git_secs` · (`window.*_size/pos` = HIDDEN 그대로)
- **단축키(Keys)** — 그대로(65 · 키맵 화면 T-53) · 순서 = 메뉴 순(File → Edit → View → Run → Tabs → Bookmarks)
- **성능(Performance)** — `perf.mode` · `perf.boost`(강제 표 §6) · 메모리 절: `mem.trim_on_release` · `mem.trim_secs`(0 = 끔) · `mem.release_results_on_disconnect` · 캐시 절: `ui.glyph_cache` · `file.icon_cache`
  *(되돌리기 예산 5개는 편집기 ▸ 되돌리기로 이동)*

### 3-2. 편집기(Editors)
- **편집기(Editor)** — 글꼴 절: `editor.font_face`·`editor.font_size` · 표시 절: `editor.line_numbers` · `editor.text_pad_left` · `editor.caret_blink` · `editor.diff_marks` · `editor.whitespace_chars` ▸ `editor.whitespace_color`·`editor.whitespace_alpha` · 안내선 절: `editor.rulers_show` ▸ `editor.rulers`·`editor.ruler_color`·`editor.ruler_alpha` · 선택 강조 절: `editor.highlight_selection` ▸ `editor.occurrence_line_color`·`_line_width`·`_fill_color` · `editor.max_occurrences` · 편집 절: `editor.tab_size` · `editor.auto_indent` ▸ `editor.smart_indent` ▸▸ `editor.indent_rules` · ▸ `editor.indent_to_bracket`·`editor.trim_auto_whitespace` · `editor.auto_close_pairs` ▸ `editor.pair_in_strings` · 복사 절: `editor.copy_rich` · `editor.copy_confirm_mb` · 탭 절: `editor.tab_accent`·`editor.tab_line_scratch/file/preview` · `editor.split_max` · `layout.editor_split_pct`(HIDDEN)
- **되돌리기(Undo)** *(신설)* — `editor.undo_max` · `editor.undo_group_ms` · `editor.undo_budget_mb` · `editor.undo_giant_mb` · `editor.undo_persist` ▸ `editor.undo_persist_mb`·`editor.undo_persist_days`
- **미니맵(Minimap)** *(신설)* — `editor.minimap` ▸ `editor.minimap_width`·`_box_color`·`_border`·`_viewport`·`_click`·`_find`·`_errors` · `bookmark.minimap`(북마크 표시와 중복 등재 — 링크)
- **코드 완성(Code completion)** — `intel.enabled` ▸ 활성 절: `intel.auto_activation` ▸▸ `intel.delay_ms`·`intel.trigger_chars`·`intel.min_chars` · ▸ 후보 절: `intel.match`·`intel.keywords`·`intel.document_words`·`intel.functions`·`intel.preload`·`intel.recent_boost`·`intel.max_items`·`intel.budget_ms` · ▸ 팝업 절: `intel.popup_rows`·`intel.popup_max_width`·`intel.show_types`·`intel.key_passthrough` · ▸ 삽입 절: `intel.insert_case`·`intel.insert_parens`·`intel.insert_alias`·`intel.insert_space`·`intel.insert_columns` · ▸ `intel.signature_help`
- **북마크(Bookmarks)** *(그룹 → 카테고리 셋을 이 그룹 아래로)* — 동작·저장 / 표시 / 위치 추적·정리 그대로 · 전부 `bookmark.enabled` ▸
- **확장(Extensions)** — 관리자 / Rainbow Pairs(`rainbowpair.enabled` ▸ 4)

### 3-3. 파일·프로젝트(Files & Project)
- **파일(Files)** — 대화상자 절: `file.show_hidden`·`file.show_dot`·`file.open_max`·`file.os_icons` ▸ `file.probe_chevrons` · 저장 절: `file.eol_new`·`file.eol_save`·`file.overwrite_confirm_ms` · (`file.last_dir`·`file.recent` HIDDEN)
- **큰 파일(Large files)** *(신설)* — `file.large_ask_mb` · `file.large_l1_mb`·`file.large_l1_lines` · `file.large_l2_mb`·`file.large_l2_lines` · `file.large_ext_level`·`file.large_syntax_level` · `file.large_head_mb` · `file.async_load_mb` ▸ `file.load_progress_ms` · `file.run_file`
- **외부 변경(External changes)** *(신설)* — `file.external_change` ▸ `file.external_poll_ms`·`file.external_settle_ms` ▸ `file.external_merge`(=auto일 때) ▸▸ `file.external_merge_max_kb`·`file.external_backup_keep`
- **파일 검색(Find in Files)** *(신설)* — `search.threads`·`search.max_file_kb`·`search.gitignore`·`search.excludes` · 이력 절: `search.history_max`(0 = 끔) ▸ `search.history_view`·`search.history_rows`
- **프로젝트(Project)** — 시작 절: `project.restore_last` · 탐색기 절: `project.preview_tab`·`project.icons`·`project.auto_reveal` · 필터 절: `project.scan_max`·`project.scan_threads` · 저장 절: `project.autosave` ▸ `project.autosave_secs`·`project.backup_days` · (`project.last`·`project.recent` HIDDEN)

### 3-4. 접속·세션(Connections)
- **접속(Connection)** — `connect.max_concurrent` · `connect.auto_reconnect` · `connect.reconnect_same` · `connect.remember_session_password` · 로그인 창 절: `conn.close_after_connect_ms`·`conn.delete_confirm_ms` · (`conn.window_*`·`conn.panel_w`·`conn.port_w`·`conn.button_scale_pct` = HIDDEN 후보)
- **서버 상태(Server status)** *(신설)* — `probe.enabled` ▸ `probe.interval`·`probe.timeout`·`probe.max_retries`·`probe.retry_delay`·`probe.max_inflight`·`probe.stale_secs`·`probe.dns_cache_secs`·`probe.icmp` · `net.keepalive_secs`
- **세션(Session)** — `session.autocommit` · `session.private_connect` · `session.max_shared`·`session.max_private` · 유휴 절: `session.idle_secs`(0 = 끔) ▸ `session.idle_shared` · `session.call_timeout_secs` · 페치 절: `db.fetch_size`·`db.fetch_all_size`·`db.cursor_idle_secs` *(Grid와 중복 등재 — 링크)*
- **트랜잭션 보호(Transaction safety)** *(신설 · [56](56-manual-commit-lock-prevention.md))* — `tx.stale_min`·`tx.remind_min` · `tx.idle_limit_min` ▸ `tx.idle_countdown_secs` · `tx.smart_commit` · `tx.block_poll_secs`(0 = 끔) · 서버 절: `tx.server_idle_timeout_secs`·`tx.lock_wait_timeout_secs` · 운영 절: `run.prod_confirm` · `tx.prod_stale_min`·`tx.prod_idle_limit_min`
- **스크립트·변수(Script & variables)** *(신설)* — `script.strict` · `run.cursor_autoshow` · `vars.signature_lookup` · 치환 절: `vars.brace_subst` ▸ `vars.env_subst` · `vars.intrinsic` · `vars.expand_at` · `vars.max_value_kb` · 보존 절: `vars.persist` ▸ `vars.persist_days`
- **CLI** — 그대로 7

### 3-5. 데이터(Data)
- **결과 셋(Result sets)** — 페치 절: `grid.max_rows` · `grid.auto_fetch` ▸ `grid.offset_warn` · `grid.memory_budget_mb` · 탭 절: `grid.result_tabs` ▸ `grid.result_tabbar_single`·`grid.result_tabs_max`·`grid.result_per_statement`·`grid.result_tab_evict` · `grid.result_tab_title` · 표시 절: `grid.font_face`·`grid.font_size`·`grid.row_height_pct`·`grid.row_numbers`·`grid.null_text` · `grid.row_focus` ▸ `grid.row_focus_color` · 열 폭 절: `grid.col_min_width` · `grid.col_max_mode` ▸ `grid.col_max_chars`(=manual)

### 3-6. 오브젝트 탐색기(Object explorer)
- 표시 절: `explorer.visible` ▸ `explorer.width` · `explorer.icons` · `explorer.tooltip` · 타입어헤드 절: `explorer.typeahead` ▸ 4 · 갱신 절: `meta.refresh_on_ddl` ▸ `meta.refresh_on_commit` · `meta.refresh_on_missing` · `meta.refresh_secs`(0 = 끔) ▸ `meta.refresh_idle_secs` · `meta.refresh_highlight_ms` · `explorer.timeout` · `meta.cache_mb`

### 3-7. 로그·진단(Log & diagnostics)
- 로그 창 절: `log.open_at_start` · `log.max_lines` · `log.newest_first`·`log.wrap`·`log.autoscroll`·`log.always_on_top`·`log.switch_scale` · `log.kinds`·`log.columns`(HIDDEN 후보) · 형식 절: `log.format` ▸ `log.template`(=template) · 파일 절: `log.file`(비어 있으면 끔) ▸ `log.file_format`·`log.file_max_kb` · 개발자 절: `log.dev_mode` ▸ `log.dev_layers` · 트랜잭션 로그 `txlog.max_entries` · (`dev.start_demo`·`demo.prompted` → HIDDEN)

### 3-8. DBMS — 그대로(Oracle 12 · SQL Server 3 · PostgreSQL 2 · SQLite 1)

## 4. 순서 규칙(카테고리 안 · `LAYOUT`의 정렬 근거)

1. **마스터 → 종속**: 스위치/모드 키가 먼저, 그 종속 키는 바로 아래 들여쓰기(깊이 2까지 · `▸▸`).
2. **선행 결정 → 후행 조정**: 범위·모드(무엇을) → 동작(어떻게) → 표시(어떻게 보이나) → 한계·예산(얼마나).
3. **자주 바꾸는 것 위**, 드문 구현 값은 아래 또는 `HIDDEN`.
4. 같은 절 안은 논리 흐름(예: L1 → L2 → 묻기 크기 → 부분 보기).
5. 한 키가 두 카테고리에 뜻이 있으면 **한 곳에만 등재 + 다른 곳은 링크 행**(예: `db.fetch_*` Session ↔ Result sets).

## 5. 종속(잠금) 보강 — `DEPENDS` 추가 안(약 32)

| 자식 | 부모 | 조건 |
|---|---|---|
| `intel.auto_activation`·`intel.match`·`intel.keywords`·`intel.document_words`·`intel.functions`·`intel.preload`·`intel.recent_boost`·`intel.max_items`·`intel.budget_ms`·`intel.popup_rows`·`intel.popup_max_width`·`intel.show_types`·`intel.key_passthrough`·`intel.insert_*`·`intel.signature_help` | `intel.enabled` | On |
| `intel.delay_ms`·`intel.trigger_chars`·`intel.min_chars` | `intel.auto_activation` | On |
| `project.autosave_secs`·`project.backup_days` | `project.autosave` | On |
| `file.external_poll_ms`·`file.external_settle_ms`·`file.external_merge` | `file.external_change` | ≠ off(새 조건 `Not("off")` 또는 `Eq` 둘) |
| `file.external_merge_max_kb`·`file.external_backup_keep` | `file.external_merge` | On |
| `file.load_progress_ms` | `file.async_load_mb` | Gt(0) |
| `grid.result_tabs_max`·`grid.result_per_statement`·`grid.result_tab_evict` | `grid.result_tabs` | On |
| `grid.offset_warn` | `grid.auto_fetch` | On |
| `editor.undo_persist_mb`·`editor.undo_persist_days` | `editor.undo_persist` | On |
| `editor.occurrence_line_color`·`_line_width`·`_fill_color` | `editor.highlight_selection` | On |
| `editor.whitespace_color`·`editor.whitespace_alpha` | `editor.whitespace_chars` | NotEmpty |
| `rainbowpair.unmatched`·`colors`·`contrast_order`·`max_kb` | `rainbowpair.enabled` | On |
| `search.history_view`·`search.history_rows` | `search.history_max` | Gt(0) |
| `session.idle_shared` | `session.idle_secs` | Gt(0) |
| `tx.idle_countdown_secs` | `tx.idle_limit_min` | Gt(0) |
| `probe.dns_cache_secs`·`probe.icmp`·`probe.stale_secs` | `probe.enabled` | On |
| `explorer.width` | `explorer.visible` | On |
| `meta.refresh_idle_secs` | `meta.refresh_secs` | Gt(0) |
| `file.probe_chevrons` | `file.os_icons` | On |
| `ui.toast_progress`·`ui.toast_alpha` | `ui.toast_secs` | Gt(0) |

새 조건 종류: `Gt(i64)`(정수 키 · "0 = 끔") · `Not(&str)`.

## 6. 강제값 — 새 `FORCES` 표(안)

`(부모, 조건, 자식, 강제값)` · 조건이 참이면 `effective(자식)` = 강제값 · 설정 창은 잠금 + "**○○ 때문에 고정**" · 저장값은 그대로(부모를 되돌리면 복귀) — `perf.boost`의 `BOOST`가 첫 행들.

| 부모 | 조건 | 자식 → 값 | 이유 |
|---|---|---|---|
| `perf.boost` | On | `BOOST` 27행 그대로 | 지금 동작 이관 |
| `intel.enabled` | Off | `intel.auto_activation`=off · `intel.signature_help`=off | 하위 기능이 따로 검사하지 않게 |
| `file.external_change` | Eq(off) | `file.external_merge`=off | 감지가 없으면 병합도 없다 |
| `grid.result_tabs` | Off | `grid.result_per_statement`=off · `grid.result_tabbar_single`=off | 탭이 없으면 뜻 없음 |
| `session.autocommit` | On | `tx.smart_commit`=off | 수동 커밋 전용 |
| `bookmark.enabled` | Off | `bookmark.gutter`·`minimap`·`inline_label`·`statusbar`=off | 표시 0 |
| `editor.minimap` | Off | `bookmark.minimap`=off · `editor.minimap_find`=off | 미니맵이 없다 |
| `log.dev_mode` | Off | `log.dev_layers`="" | 계층 선택 무의미 |
| `perf.mode` | Eq(low) | (`PERF` 프리셋 값 — 이미 `effective`가 함) | 표로 통합 |

`DEPENDS`(잠금 · 값 유지)와 `FORCES`(강제 · 유효값 대체)의 구분: **하위가 그 자체로 값을 가질 이유가 있으면 잠금**(예: 미니맵 폭) · **상위 때문에 값이 정해져야 하면 강제**(예: 완성 끔 → 자동 활성 끔).

## 7. 구현 부품(적용 때)

- `nsql-settings`: `LAYOUT: &[(Msg /*cat*/, &[LayoutItem])]`(`Key(&str)` · `Section(Msg)` · `Link(&str)`) — 카테고리 안 순서·절·링크의 단일 원천 · 등재 안 된 키는 카테고리 끝(레지스트리 순) · 무결성 시험(전 키 1회 등재 · 존재하는 키만 · 종속 키는 부모 뒤).
- `Dep::Gt(i64)` · `Dep::Not(&str)` · `FORCES: &[(&str, Dep, &str, &str)]` · `Settings::effective(key)`가 `FORCES`를 먼저 보고 → `forced_by(key) -> Option<&str>` · `BOOST`는 `FORCES`로 흡수(`boost_locked`는 `forced_by`로).
- 설정 창: `LAYOUT` 순 · 절 머리글 · 종속 깊이 들여쓰기 · 잠금 문구 "부모 ○○을 먼저" / 강제 문구 "○○ 때문에 고정(값)" · 검색 결과도 같은 순.
- `nsql config list`: 카테고리 머리글 + 절 · `--forced`로 강제 출처 표시.
- 위키 `Settings` 페이지 = 트리 그대로(T-185 i18n 간략화와 같이).

## 8. 적용 단계(T-186)

① 부품(`LAYOUT`·`FORCES`·`Gt`/`Not` · 시험) → ② `CATEGORY_TREE` 7 그룹 31 카테고리 + i18n(신설 카테고리 라벨 8) → ③ `LAYOUT` 전 키 등재(§3 그대로) → ④ `DEPENDS` 보강(§5) · `FORCES`(§6 · `BOOST` 이관) → ⑤ 설정 창 표시(들여쓰기·절·사유) → ⑥ `config list`·위키 → ⑦ 실기(잠금·강제·검색) — 예상 규모: 코드 중(설정 창 표시가 큼) · 문서 중.
