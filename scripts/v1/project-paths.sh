#!/usr/bin/env bash
# V1 project-paths — 프로젝트 파일 경로 이식성(T-172 P1·P2 · docs/73): 헤드리스 · 격리 홈 · 입력 주입 0 · 프로젝트 파일·덤프 문자열 대조.
#   a. 등록 폴더(E/lib) + 프로젝트 폴더(P) 파일 두 개를 탭으로 열고 `project.save` → `"version": 2` · `"sql/a.sql"` · `"${workspaceFolder:lib}/b.sql"`
#   b. 원본이 없는 탭(미저장 본문 포함)을 가진 프로젝트를 열고 저장 → 그 탭 항목·본문이 남는다 + 로그 "file not found"(T-172 P1 · 그 전에는 사라짐)
#   c. `"version": 99` 파일 = 열지 않음 + 상태줄·로그 안내("newer than this app supports") · 앱은 살아서 정상 종료 · 파일은 그대로
#   d. 버전 1 옛 파일(상대 경로만) = 그대로 열린다(탭 복원)
#   종료: 프로젝트가 바뀐 채 `file.exit`면 "프로젝트 저장?" 물음이 뜬다 → 답 = `toolbar.pick:project.exit_skip`(물음 답은 indent_pick 길 · 직접 id는 menu_action이라 무시됨)
#   임시 폴더 = OS 임시 `nsql-v1pp-*`(끝나면 삭제) · 쉼표가 든 경로는 기동 명령 구분자와 겹치므로 쓰지 않는다.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_set "project.autosave=off"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1pp-XXXX")
P="$WK/proj"; E="$WK/ext/lib"
mkdir -p "$P/sql" "$E"
printf 'SELECT 1 AS a FROM dual;\n' > "$P/sql/a.sql"
printf 'SELECT 2 AS b FROM dual;\n' > "$E/b.sql"
PM=$(v1_m "$P"); EM=$(v1_m "$E"); O=$(v1_m "$V1_OUT")

# ── a. 앵커로 저장 ─────────────────────────────────────────────────────────────
printf '{"version": 1, "folders": [{"path": "."}, {"path": "%s"}]}\n' "$EM" > "$P/a.nsql-project"
v1_gui pa "project.load:$PM/a.nsql-project,@after:1500:open:$PM/sql/a.sql,@after:2200:open:$EM/b.sql,@after:3200:project.save,@after:3600:status.dump:$O/pa-status.txt,@after:4200:file.exit" 25
cp "$P/a.nsql-project" "$V1_OUT/pa.nsql-project" 2>/dev/null
expect "a-version2" "$V1_OUT/pa.nsql-project" '"version": 2'
expect "a-rel-path" "$V1_OUT/pa.nsql-project" '"path": "sql/a.sql"'
expect "a-folder-anchor" "$V1_OUT/pa.nsql-project" '"${workspaceFolder:lib}/b.sql"'
expect_not "a-no-abs-tab" "$V1_OUT/pa.nsql-project" "$EM/b.sql"

# ── b. 못 찾은 탭 보존 ─────────────────────────────────────────────────────────
printf '%s\n' '{"version": 2, "folders": [{"path": "."}], "active": 1, "tabs": [{"path": "sql/gone.sql", "title": "gone.sql", "id": 5, "text": "-- v1pp unsaved keep"}, {"path": "sql/a.sql", "title": "a.sql", "id": 6}]}' > "$P/b.nsql-project"
v1_gui pb "project.load:$PM/b.nsql-project,@after:1500:tabs.dump:$O/pb-tabs.txt,@after:2500:project.save,@after:3000:log.dump:$O/pb-log.txt,@after:3600:file.exit,@after:4200:toolbar.pick:project.exit_skip" 25
cp "$P/b.nsql-project" "$V1_OUT/pb.nsql-project" 2>/dev/null
expect "b-opened-other-tab" "$V1_OUT/pb-tabs.txt" "a.sql"
expect "b-missing-tab-kept" "$V1_OUT/pb.nsql-project" '"path": "sql/gone.sql"'
expect "b-missing-body-kept" "$V1_OUT/pb.nsql-project" "-- v1pp unsaved keep"
expect "b-log-not-found" "$V1_OUT/pb-log.txt" "file not found"
expect "b-saved-v2" "$V1_OUT/pb.nsql-project" '"version": 2'
# b2. 명시 저장 뒤 **두 번째 저장**(종료 물음 "저장")에도 남아야 한다 — 11-22 발견: `project_save_to` → `project_set`이 `proj_missing`을
#     비워 두 번째 저장에서 탭이 사라지고, 그 차이 때문에 저장 직후에도 종료 물음이 뜬다(개발 수정 전까지 FAIL = 결함 감시).
printf '%s\n' '{"version": 2, "folders": [{"path": "."}], "active": 1, "tabs": [{"path": "sql/gone.sql", "title": "gone.sql", "id": 5, "text": "-- v1pp unsaved keep"}, {"path": "sql/a.sql", "title": "a.sql", "id": 6}]}' > "$P/b2.nsql-project"
v1_gui pb2 "project.load:$PM/b2.nsql-project,@after:2500:project.save,@after:3600:file.exit,@after:4300:toolbar.menudump:$O/pb2-menu.txt,@after:4600:toolbar.pick:project.exit_save" 25
cp "$P/b2.nsql-project" "$V1_OUT/pb2.nsql-project" 2>/dev/null
expect_not "b2-no-exit-prompt-after-save" "$V1_OUT/pb2-menu.txt" "project.exit_save"
expect "b2-missing-tab-kept-2nd-save" "$V1_OUT/pb2.nsql-project" '"path": "sql/gone.sql"'

# ── c. 더 새 판(99)은 열지 않음 ────────────────────────────────────────────────
printf '%s\n' '{"version": 99, "folders": [{"path": "."}], "tabs": [{"path": "sql/a.sql", "title": "a.sql"}]}' > "$P/c.nsql-project"
C0=$(md5sum < "$P/c.nsql-project")
v1_gui pc "project.load:$PM/c.nsql-project,@after:1500:status.dump:$O/pc-status.txt,@after:1700:log.dump:$O/pc-log.txt,@after:1900:tabs.dump:$O/pc-tabs.txt,@after:2600:file.exit" 25
expect "c-status-newer" "$V1_OUT/pc-status.txt" "newer than this app supports"
expect "c-log-newer" "$V1_OUT/pc-log.txt" "newer than this app supports"
expect_not "c-no-tab-restored" "$V1_OUT/pc-tabs.txt" "a.sql"
expect_eq "c-exit-0" "$(cat "$V1_OUT/pc.exit")" "0"
expect_eq "c-file-untouched" "$(md5sum < "$P/c.nsql-project")" "$C0"

# ── d. 버전 1 옛 파일 ──────────────────────────────────────────────────────────
printf '%s\n' '{"version": 1, "folders": [{"path": "sql"}], "tabs": [{"path": "sql/a.sql", "title": "a.sql"}]}' > "$P/d.nsql-project"
v1_gui pd "project.load:$PM/d.nsql-project,@after:1500:tabs.dump:$O/pd-tabs.txt,@after:1700:status.dump:$O/pd-status.txt,@after:2400:file.exit,@after:3000:toolbar.pick:project.exit_skip" 25
expect_eq "d-exit-0" "$(cat "$V1_OUT/pd.exit")" "0"
expect "d-v1-tab-restored" "$V1_OUT/pd-tabs.txt" "a.sql"
expect_not "d-no-error" "$V1_OUT/pd-status.txt" "Project file:"

# 공통 — 네 기동 모두 panic 없음
for t in pa pb pb2 pc pd; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
