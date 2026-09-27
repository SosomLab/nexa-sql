#!/usr/bin/env python3
"""main.rs의 거대 `impl App`을 기능별 `app/*.rs`로 나눈다(docs/93 §4 · 1회성 구조 이관 도구 · 재실행 = 멱등 아님).

규칙
- rustfmt로 정리된 파일을 전제한다: 메서드 = `    fn …`/`    pub(crate) fn …` 로 시작해 처음 나오는 `    }` 줄에서 끝난다.
- 메서드 앞의 문서 주석·속성·일반 주석은 메서드에 딸려 간다(빈 줄에서 멈춘다).
- 옮긴 메서드는 `pub(crate) fn`이 된다(다른 모듈에서 부르므로). 남은 메서드는 그대로(루트의 비공개는 하위 모듈에서 보인다).
- 각 모듈 = `use crate::*;` + `impl App { … }` — 루트의 `use`·자유 함수·타입이 그대로 보인다.
- `impl ApplicationHandler<Wake> for App` 블록은 통째로 `app/event_loop.rs`로.

사용: python scripts/split-app-impl.py  (작업 트리에서 · 이어서 cargo fmt / cargo check)
"""
import io
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MAIN = ROOT / "crates/nexa-sql/src/main.rs"
APP = ROOT / "crates/nexa-sql/src/app"

MODULES = {
    "find": ("찾기·바꾸기 · 파일 검색(docs/36)", r"find_.*|start_search|open_search_result|sync_find_marks"),
    "toolbar": ("툴바 그룹 도크·플로팅·표시 항목(docs/30 ToolDock)",
                r"open_toolbar_menu|hidden_toolbar_ids|apply_toolbar_visibility|apply_tool_layout_setting|save_tool_layout|"
                r"drain_dock_actions|open_float|dock_group|reset_layout|reset_toolbar|build_tool_dock"),
    "extensions": ("확장(관리자·패널·명령 · docs/50·75)", r"ext_.*|apply_extensions|run_extension_cmd"),
    "project": ("프로젝트·작업 환경·다중 열기(docs/67·70)",
                r"project_.*|multi_.*|reset_workspace_to_initial|restore_caret|flush_on_exit|sync_open_files|"
                r"ask_project_exit|reveal_in_project|sync_project_panel_opts"),
    "session": ("세션 컨텍스트 · 접속/해제 · 실행 통제 문지기(docs/52·54 · DR-34)",
                r"session_mode|login_place|fail_login_attempt|all_sess|sess_by_id|sess_id_for_tab|bound_tabs|new_shared|"
                r"activate_shared|disconnect_.*|with_sess|sync_sess|make_unconnected|on_new_tab|new_private|connect_quietly|"
                r"drain_all|reap_sessions|explorer_attach|sync_sess_ui|sync_gate|gate|gate_open|idle_tick|wake_if_idle|"
                r"open_badge_menu|badge_pick|spawn_worker|log_disconnect|mark_disc|open_disc_guard|disc_pick|connect_server|"
                r"new_tab_on|apply_net_options|on_conn_disconnected|sync_disconnect_btn|session_rows|abandon_worker|stop_run"),
    "grid_results": ("결과 그리드·결과 탭·페치·그리드 편집 적용(docs/43·87)",
                     r"after_grid_event|grid_edit_.*|cell_load_.*|goto_statement|grid_for|sleeping_grids.*|activate_result|"
                     r"new_result_tab|child_result_tab|prune_child_results|result_title|title_result_as|retitle_result|panel_action|"
                     r"move_result_tab|close_result_tab|send_fetch|fetch_card_start|refresh_result|begin_view_sql|finish_view_sql|"
                     r"begin_sql_copy|finish_sql_copy|freeze_tab_results|freeze_results_of|set_dialect_for_sess_grids|all_grids|"
                     r"run_grid|sync_grid_tab|apply_grid_.*|apply_result_tab_opts|apply_click_policy"),
    "tx": ("트랜잭션 UX · 수동 커밋 잠금 방지(docs/34·56)",
           r"tx_.*|sync_tx_button|sync_tx_ui|open_tx_menu|open_tx_guard|set_autocommit.*|run_tx_after"),
    "memory": ("메모리 회수·메모리 창(docs/80)", r"mem_.*|open_mem_window|toggle_mem_window"),
    "completion": ("코드 완성·시그니처·아웃라인·이동(docs/76)",
                   r"intel_.*|open_goto_symbol|outline_.*|goto_byte|open_goto_anything"),
    "files": ("파일 열기/저장·인코딩·탭 닫기·종료 흐름(T-74 · docs/59)",
              r"recent_files|push_recent|remember_file_dialog|open_file_window|open_file|decode_bytes|encode_text|open_file_enc|"
              r"load_file|file_loads_poll|file_load_cancel_active|paint_file_load|file_loaded|save_to|vars_persist_.*|"
              r"undo_persist_.*|close_tab_guarded|finish_exit|request_exit|dirty_tabs|save_all_step|ask_save_close|close_pick|"
              r"finish_close_after_save"),
    "connwin": ("접속 창(로그인 목록·시험·프로필 관리 · docs/22)",
                r"open_conn_window|start_test|dispatch_attempts|attempt_done|test_profile|login_profile|set_profile_env|"
                r"duplicate_profile|delete_profile|handle_conn_win_action"),
    "windows": ("보조 창 열기·모달·창 목록(로그·트랜잭션 로그·변수·세션)",
                r"toggle_log_window|open_txlog_window|open_vars_window|vars_win_context|open_sessions_window|modal_window|"
                r"modal_open|sync_modal|all_windows|aux_window|on_window_focused|persist_window_sizes|apply_window_sizes|apply_on_top"),
    "vars": ("변수(내장·DEFINE·변수 창 적용 · docs/63)",
             r"run_intrinsic|intrinsic_context|run_defines|vars_rows|vars_apply|global_vars_changed|run_vars"),
    "license": ("라이선스 게이트·창·배지(docs/23·25 · T-36)",
                r"license_.*|about_lines|gates_on|entitled|lic_gate|cap|feature_texts|tab_room|open_license_window"),
    "demo": ("데모 프로필·샘플 데이터(docs/21 §5)", r"demo_.*|open_demo_prompt|start_demo_create|finish_demo"),
    "live": ("실시간 재조회(live)", r"live_.*"),
    "meta": ("메타·객체 상세·탐색기 동작(docs/85·86)",
             r"meta_.*|sync_detail_target|feed_comments|detail_actions|explorer_actions"),
    "bookmarks": ("북마크(docs/69 · T-167)", r"bm_.*|bookmark_cmd|open_bm_gutter_menu"),
    "menus": ("메뉴·팔레트·명령 분배(풀다운·우클릭·키 명령)",
              r"menu_action|build_menus|build_menus_with|sync_tabs_menu|rebuild_menus|apply_menu_decor|tab_menu_request|"
              r"open_indent_menu|open_eol_menu|open_enc_menu|indent_pick|open_status_popup.*|open_palette|clip_action|"
              r"copy_confirm_pending|open_menus|close_menu_bits|close_context_menus|key_command|key_mode|editor_cmd"),
    "startup_cmd": ("기동 명령(`NSQL_STARTUP_CMD` · 자체 시험·자동 점검 경로 · docs/61 §4)", r"startup_cmd"),
    "settings": ("설정 반영·테마·언어(설정 키 → 화면 · docs/24)",
                 r"apply_setting|apply_.*|prefs_sync|edit_settings_json|json_tick|persist_settings|cycle_theme|toggle_lang|"
                 r"occurrence_cap|relabel|rebuild_log_hub"),
    "events": ("백그라운드 사건 소화(워커·접속·가져오기·실행 카드)",
               r"drain_conn|drain_events|import_start|import_cancel|import_done|run_toast_start|handle_panel_action|"
               r"panel_op_name|set_panel_result|panel_state_for"),
    "paint": ("그리기(프레임 합성)", r"paint|column_rule"),
    "input": ("입력 경로(키·마우스 라우팅 · 포커스 · IME · docs/61 §2-2)",
              r"route|route_dispatch|route_inner|route_splitters|ctl_event|regions_cap_notice|giant_notice|set_focus|"
              r"focused_textbox|ime_refresh|sync_hangul_mode"),
    "run": ("SQL 실행·설명 계획·입력 응답(docs/43)",
            r"run_sql|run_text|run_explain|place_run|password_reply|input_reply|sync_run_stmt_button"),
}
ORDER = list(MODULES)
COMPILED = [(m, re.compile(r"(?:%s)$" % MODULES[m][1])) for m in ORDER]


def classify(name: str) -> str | None:
    for m, rx in COMPILED:
        if rx.match(name):
            return m
    return None


def main() -> int:
    text = io.open(MAIN, encoding="utf-8").read()
    lines = text.splitlines(keepends=True)
    impl0 = next(i for i, l in enumerate(lines) if l.startswith("impl App {"))
    impl1 = next(i for i in range(impl0 + 1, len(lines)) if lines[i].rstrip("\r\n") == "}")
    fn_rx = re.compile(r"^    (?:pub\(crate\) )?(?:const )?fn ([a-z_0-9]+)")
    spans = []  # (start, end_inclusive, name)
    i = impl0 + 1
    while i < impl1:
        m = fn_rx.match(lines[i])
        if not m:
            i += 1
            continue
        start = i
        while start - 1 > impl0:
            prev = lines[start - 1]
            s = prev.strip()
            if prev.startswith("    ") and (s.startswith("//") or s.startswith("#[")):
                start -= 1
            else:
                break
        end = i
        while lines[end].rstrip("\r\n") != "    }":
            end += 1
        spans.append((start, end, m.group(1)))
        i = end + 1
    moved: dict[str, list[str]] = {m: [] for m in ORDER}
    take = set()
    for s, e, name in spans:
        mod = classify(name)
        if not mod:
            continue
        chunk = lines[s : e + 1]
        for k, l in enumerate(chunk):
            if fn_rx.match(l):
                chunk[k] = re.sub(r"^    (?:pub\(crate\) )?", "    pub(crate) ", l, count=1)
                break
        moved[mod].extend(chunk + ["\n"])
        take.update(range(s, e + 1))
    # impl ApplicationHandler 블록
    h0 = next(i for i, l in enumerate(lines) if l.startswith("impl ApplicationHandler<Wake> for App {"))
    h_start = h0
    while h_start - 1 >= 0 and lines[h_start - 1].strip().startswith(("//", "#[")):
        h_start -= 1
    h1 = next(i for i in range(h0 + 1, len(lines)) if lines[i].rstrip("\r\n") == "}")
    handler = lines[h_start : h1 + 1]
    take.update(range(h_start, h1 + 1))

    out = [l for k, l in enumerate(lines) if k not in take]
    # 연속 빈 줄 정리
    clean = []
    for l in out:
        if l.strip() == "" and clean and clean[-1].strip() == "":
            continue
        clean.append(l)
    # `mod app;` = dlog! 매크로 뒤(매크로는 선언 순서대로 보인다)
    body = "".join(clean)
    anchor = body.index("macro_rules! dlog {")
    end_macro = body.index("\n}\n", anchor) + 3
    body = body[:end_macro] + "\n/// `App`의 기능별 `impl` 조각(docs/93 §4 — main.rs 거대 객체 분할).\nmod app;\n" + body[end_macro:]
    io.open(MAIN, "w", encoding="utf-8", newline="\n").write(body)

    APP.mkdir(exist_ok=True)
    mods = []
    for m in ORDER:
        if not moved[m]:
            continue
        mods.append(m)
        title = MODULES[m][0]
        src = f"//! App — {title}.\n//!\n//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.\n\nuse crate::*;\n\nimpl App {{\n"
        src += "".join(moved[m]).rstrip("\n") + "\n}\n"
        io.open(APP / f"{m}.rs", "w", encoding="utf-8", newline="\n").write(src)
    ev = "//! App — winit 사건 처리기(`ApplicationHandler` · 창 사건·유휴 틱·재개).\n//!\n//! main.rs에서 옮김(docs/93 §4).\n\nuse crate::*;\n\n" + "".join(handler)
    io.open(APP / "event_loop.rs", "w", encoding="utf-8", newline="\n").write(ev)
    mods.append("event_loop")
    modrs = "//! `App` 기능 모듈 — 상태(`App` 구조체)는 main.rs 한 곳, 동작은 기능별 파일(docs/93 §4).\n//!\n//! 새 동작은 해당 기능 파일에 둔다. 두 기능에 걸치면 호출하는 쪽(상위 흐름) 파일에.\n\n"
    modrs += "".join(f"mod {m};\n" for m in sorted(mods))
    io.open(APP / "mod.rs", "w", encoding="utf-8", newline="\n").write(modrs)
    kept = sum(1 for s, e, n in spans if not classify(n))
    print(f"moved {len(spans) - kept} methods into {len(mods)} modules · kept {kept} in main.rs")
    for s, e, n in spans:
        if not classify(n):
            print("  keep", n, e - s + 1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
