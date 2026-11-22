#!/usr/bin/env bash
# V1 project-paths — 프로젝트 파일 경로 이식성(T-172 P1·P2 · docs/73): 헤드리스 · 격리 홈 · 입력 주입 0 · 프로젝트 파일·덤프 문자열 대조.
#   a. 등록 폴더(E/lib) + 프로젝트 폴더(P) 파일 두 개를 탭으로 열고 `project.save` → `"version": 2` · `"sql/a.sql"` · `"${workspaceFolder:lib}/b.sql"`
#   b. 원본이 없는 탭(미저장 본문 포함)을 가진 프로젝트를 열고 저장 → 그 탭 항목·본문이 남는다 + 로그 "file not found"(T-172 P1 · 그 전에는 사라짐)
#   c. `"version": 99` 파일 = 열지 않음 + 상태줄·로그 안내("newer than this app supports") · 앱은 살아서 정상 종료 · 파일은 그대로
#   d. 버전 1 옛 파일(상대 경로만) = 그대로 열린다(탭 복원)
#   e. 못 찾은 등록 폴더를 `project.relocate_folder:<순번>:<경로>`로 다시 지정 → 정의·앵커 유지 + `"id"` 발급 + 격리 홈 `project.pc_paths` · e2 = 다시 열면 대응표 적용(T-172 (A)(B))
#   f. 못 찾은 탭의 자리 탭(T-172 (C) · D-189 · D-496): 탭 줄 그 자리에 빈 읽기 전용 뷰 탭 · 저장 때 순서 그대로 · 닫으면 기록에서 뺀다
#      (미저장 본문이 있으면 한 번 더 닫아야) · `project.missing_found:<경로>`로 되살림
#   g. 자리 탭이 보관된 본문을 읽기 전용으로 보여 줌(이중 저장 없음) · 못 찾은 등록 폴더 알림(로그·토스트) · `file.close_tab`(Ctrl+W) 길도 재확인(11-22 묶음 4)
#   종료: 프로젝트가 바뀐 채 `file.exit`면 "프로젝트 저장?" 물음이 뜬다 → 답 = `toolbar.pick:project.exit_skip`(물음 답은 indent_pick 길 · 직접 id는 menu_action이라 무시됨)
#   임시 폴더 = OS 임시 `nsql-v1pp-*`(끝나면 삭제) · 쉼표가 든 경로는 기동 명령 구분자와 겹치므로 쓰지 않는다.
. "$(dirname "$0")/../lib/v1.sh"
expect_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_pass "$1"; else v1_failmsg "$1" "/$3/ 없음 · $(head -c 200 "$2" 2>/dev/null | tr '\n' ' ')"; fi; }
expect_not_re() { if grep -qE -- "$3" "$2" 2>/dev/null; then v1_failmsg "$1" "/$3/ 있음"; else v1_pass "$1"; fi; }
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

# ── e. 폴더 다시 지정(T-172 (A)(B) · D-494 세부 = 결과는 이 PC 대응표에만) ─────────────
#   등록 폴더 `gone/lib`는 없고 실제 파일은 `moved/lib2/b.sql` — `project.relocate_folder:<순번>:<경로>`로 다시 지정 →
#   프로젝트 파일은 정의(gone/lib · 앵커 lib)를 그대로 두고 고유 번호 `"id"`만 생긴다 · 대응표는 격리 홈 settings.conf `project.pc_paths`.
#   e2 = 같은 홈으로 다시 열기만 해도 대응표가 적용돼 탭이 바로 열린다.
G="$WK/gone/lib"; M2="$WK/moved/lib2"
mkdir -p "$M2"
printf 'SELECT 3 AS e FROM dual;\n' > "$M2/b.sql"
GM=$(v1_m "$G"); M2M=$(v1_m "$M2")
printf '{"version": 2, "folders": [{"path": "."}, {"path": "%s"}], "active": 1, "tabs": [{"path": "${workspaceFolder:lib}/b.sql", "title": "b.sql", "id": 7}]}\n' "$GM" > "$P/e.nsql-project"
v1_gui pe "project.load:$PM/e.nsql-project,@after:1500:tabs.dump:$O/pe-tabs0.txt,@after:2000:project.relocate_folder:1:$M2M,@after:2400:status.dump:$O/pe-status.txt,@after:2700:tabs.dump:$O/pe-tabs.txt,@after:3200:project.save,@after:3600:log.dump:$O/pe-log.txt,@after:4200:file.exit,@after:4800:toolbar.pick:project.exit_skip" 25
cp "$P/e.nsql-project" "$V1_OUT/pe.nsql-project" 2>/dev/null
cp "$V1_HOME/settings.conf" "$V1_OUT/pe-settings.conf" 2>/dev/null
# T-172 (C) 뒤: 못 찾은 탭은 탭 줄에 **자리 탭**(뷰 · 읽기 전용)으로 보인다(종전 기대 = "b.sql 탭 없음") · 다시 지정하면 진짜 탭이 열리고 자리 탭은 닫힌다.
expect_re "e-placeholder-before" "$V1_OUT/pe-tabs0.txt" '^\*?b\.sql\|.*view=true'
expect "e-status-relocated" "$V1_OUT/pe-status.txt" "Folder relocated on this PC"
EID=$(grep -oE '"id": "[0-9a-f]{16}"' "$V1_OUT/pe.nsql-project" 2>/dev/null | head -1 | sed 's/.*: "\(.*\)"/\1/')
if [ -n "$EID" ]; then v1_pass "e-id-issued"; else v1_failmsg "e-id-issued" "'\"id\": \"<16진 16자>\"' 없음 · $(head -c 200 "$V1_OUT/pe.nsql-project" 2>/dev/null | tr '\n' ' ')"; fi
expect "e-def-kept" "$V1_OUT/pe.nsql-project" "gone/lib"
expect_not "e-def-no-moved" "$V1_OUT/pe.nsql-project" "moved"
expect_not "e-def-no-lib2" "$V1_OUT/pe.nsql-project" "lib2"
expect "e-anchor-kept" "$V1_OUT/pe.nsql-project" '"${workspaceFolder:lib}/b.sql"'
PCL=$(grep '^project.pc_paths=' "$V1_OUT/pe-settings.conf" 2>/dev/null)
if [ -n "$PCL" ] && [ -n "$EID" ] && printf '%s' "$PCL" | grep -qF "$EID" && printf '%s' "$PCL" | grep -qF "lib2"; then v1_pass "e-pc-table"
else v1_failmsg "e-pc-table" "id='$EID' · 줄='$(printf '%s' "$PCL" | head -c 200)'"; fi
expect_re "e-tab-opened" "$V1_OUT/pe-tabs.txt" '^\*?b\.sql\|.*view=false'
expect_not_re "e-placeholder-closed" "$V1_OUT/pe-tabs.txt" '^\*?b\.sql\|.*view=true'
# e2. 다시 열기만(다시 지정 명령 없음) → 대응표 적용
v1_gui pe2 "project.load:$PM/e.nsql-project,@after:1500:tabs.dump:$O/pe2-tabs.txt,@after:1800:log.dump:$O/pe2-log.txt,@after:2500:project.save,@after:3100:file.exit,@after:3700:toolbar.pick:project.exit_skip" 25
cp "$P/e.nsql-project" "$V1_OUT/pe2.nsql-project" 2>/dev/null
expect "e2-table-applied" "$V1_OUT/pe2-tabs.txt" "b.sql"
expect_not "e2-no-not-found" "$V1_OUT/pe2-log.txt" "not found"
expect "e2-def-still-kept" "$V1_OUT/pe2.nsql-project" "gone/lib"
expect_not "e2-def-no-moved" "$V1_OUT/pe2.nsql-project" "moved"
expect_not "e2-def-no-lib2" "$V1_OUT/pe2.nsql-project" "lib2"

# ── f. 자리 탭(T-172 (C) · D-189 · D-496) ───────────────────────────────────────
#   탭 순서 = a.sql(있음) · gone1.sql(없음 · 본문 없음) · gone2.sql(없음 · 미저장 본문) · c.sql(있음) — 못 찾은 탭이 가운데.
#   `"active"` = 탭 순번(자리 탭 포함)으로 자리 탭을 활성으로 연다.
printf 'SELECT 4 AS c FROM dual;\n' > "$P/sql/c.sql"
mkdir -p "$P/sql/sub"; printf 'SELECT 5 AS f FROM dual;\n' > "$P/sql/sub/gone1.sql"
mkf() { printf '{"version": 2, "folders": [{"path": "."}], "active": %s, "tabs": [{"path": "sql/a.sql", "title": "a.sql", "id": 11}, {"path": "sql/gone1.sql", "title": "gone1.sql", "id": 12}, {"path": "sql/gone2.sql", "title": "gone2.sql", "id": 13, "text": "-- v1pp f unsaved keep"}, {"path": "sql/c.sql", "title": "c.sql", "id": 14}]}\n' "$1" > "$P/$2"; }
porder() { grep -oE '"path": "sql/[a-z0-9/]+\.sql"' "$1" 2>/dev/null | sed 's/"path": "//; s/"$//' | tr '\n' ' ' | sed 's/ $//'; }
torder() { sed 's/^\*//; s/|.*//' "$1" 2>/dev/null | grep '\.sql$' | tr '\n' ' ' | sed 's/ $//'; }
# f1. 열기 → 저장: 자리 탭이 그 자리에 · 저장 순서 그대로 · 미저장 본문 그대로
mkf 0 f1.nsql-project
v1_gui pf1 "project.load:$PM/f1.nsql-project,@after:1500:tabs.dump:$O/pf1-tabs.txt,@after:2300:project.save,@after:2900:file.exit,@after:3500:toolbar.pick:project.exit_skip" 25
cp "$P/f1.nsql-project" "$V1_OUT/pf1.nsql-project" 2>/dev/null
expect_re "f-placeholder-shown" "$V1_OUT/pf1-tabs.txt" '^\*?gone2\.sql\|.*view=true'
expect_eq "f-placeholder-order" "$(torder "$V1_OUT/pf1-tabs.txt")" "a.sql gone1.sql gone2.sql c.sql"
expect_eq "f-order-kept" "$(porder "$V1_OUT/pf1.nsql-project")" "sql/a.sql sql/gone1.sql sql/gone2.sql sql/c.sql"
expect "f-body-kept" "$V1_OUT/pf1.nsql-project" "-- v1pp f unsaved keep"
# f2. 본문 없는 자리 탭(gone1) 닫기 = 바로 닫힘 → 기록에서도 빠진다
mkf 1 f2.nsql-project
v1_gui pf2 "project.load:$PM/f2.nsql-project,@after:1500:tabs.dump:$O/pf2-tabs0.txt,@after:1900:project.missing_close,@after:2300:tabs.dump:$O/pf2-tabs.txt,@after:2700:project.save,@after:3300:file.exit,@after:3900:toolbar.pick:project.exit_skip" 25
cp "$P/f2.nsql-project" "$V1_OUT/pf2.nsql-project" 2>/dev/null
expect_re "f2-active-placeholder" "$V1_OUT/pf2-tabs0.txt" '^\*gone1\.sql\|.*view=true'
expect_not "f2-tab-gone" "$V1_OUT/pf2-tabs.txt" "gone1.sql"
expect_not "f2-closed-removed" "$V1_OUT/pf2.nsql-project" "sql/gone1.sql"
expect "f2-other-kept" "$V1_OUT/pf2.nsql-project" '"path": "sql/gone2.sql"'
expect "f2-other-body-kept" "$V1_OUT/pf2.nsql-project" "-- v1pp f unsaved keep"
# f3. 미저장 본문 든 자리 탭(gone2) — 첫 닫기는 막힘(안내) · 두 번째에 닫힘
mkf 2 f3.nsql-project
v1_gui pf3a "project.load:$PM/f3.nsql-project,@after:1500:project.missing_close,@after:1800:status.dump:$O/pf3a-status.txt,@after:2000:tabs.dump:$O/pf3a-tabs.txt,@after:2500:project.save,@after:3100:file.exit,@after:3700:toolbar.pick:project.exit_skip" 25
cp "$P/f3.nsql-project" "$V1_OUT/pf3a.nsql-project" 2>/dev/null
expect "f3-first-close-blocked" "$V1_OUT/pf3a.nsql-project" '"path": "sql/gone2.sql"'
expect "f3-first-body-kept" "$V1_OUT/pf3a.nsql-project" "-- v1pp f unsaved keep"
expect "f3-first-close-notice" "$V1_OUT/pf3a-status.txt" "close it again to confirm"
expect_re "f3-first-tab-still" "$V1_OUT/pf3a-tabs.txt" '^\*gone2\.sql\|.*view=true'
mkf 2 f3.nsql-project
v1_gui pf3b "project.load:$PM/f3.nsql-project,@after:1500:project.missing_close,@after:1900:project.missing_close,@after:2300:tabs.dump:$O/pf3b-tabs.txt,@after:2700:project.save,@after:3300:file.exit,@after:3900:toolbar.pick:project.exit_skip" 25
cp "$P/f3.nsql-project" "$V1_OUT/pf3b.nsql-project" 2>/dev/null
expect_not "f3-second-tab-gone" "$V1_OUT/pf3b-tabs.txt" "gone2.sql"
expect_not "f3-second-close-removed" "$V1_OUT/pf3b.nsql-project" "sql/gone2.sql"
expect_not "f3-second-body-gone" "$V1_OUT/pf3b.nsql-project" "-- v1pp f unsaved keep"
# f4. 찾아보기 = `project.missing_found:<경로>`(활성 자리 탭 gone1 → sql/sub/gone1.sql) → 진짜 탭 · 자리 탭 없음 · 기록 경로 갱신
mkf 1 f4.nsql-project
v1_gui pf4 "project.load:$PM/f4.nsql-project,@after:1500:project.missing_found:$PM/sql/sub/gone1.sql,@after:2100:tabs.dump:$O/pf4-tabs.txt,@after:2600:project.save,@after:3200:file.exit,@after:3800:toolbar.pick:project.exit_skip" 25
cp "$P/f4.nsql-project" "$V1_OUT/pf4.nsql-project" 2>/dev/null
expect_re "f4-found-reopened" "$V1_OUT/pf4-tabs.txt" '^\*?gone1\.sql\|.*view=false'
expect_not_re "f4-no-placeholder" "$V1_OUT/pf4-tabs.txt" '^\*?gone1\.sql\|.*view=true'
expect "f4-path-updated" "$V1_OUT/pf4.nsql-project" '"path": "sql/sub/gone1.sql"'
expect_not "f4-old-path-gone" "$V1_OUT/pf4.nsql-project" '"path": "sql/gone1.sql"'
expect "f4-other-kept" "$V1_OUT/pf4.nsql-project" "-- v1pp f unsaved keep"

# ── g. 자리 탭 본문 · 폴더 알림 · 닫기 길(11-22 묶음 4 · D-189) ──────────────────────
#   g1. 미저장 본문 든 자리 탭(gone2)을 활성으로 열면 보관된 본문을 **읽기 전용**으로 보여 준다(`editor.dump` 머리 3번째 칸 = 읽기 전용) ·
#       저장해도 그 본문이 탭 본문으로 이중 저장되지 않는다(gone2 항목 1개 · 본문 1번).
mkf 2 g1.nsql-project
v1_gui pg1 "project.load:$PM/g1.nsql-project,@after:1500:tabs.dump:$O/pg1-tabs.txt,@after:1800:editor.dump:$O/pg1-editor.txt,@after:2400:project.save,@after:3000:file.exit,@after:3600:toolbar.pick:project.exit_skip" 25
cp "$P/g1.nsql-project" "$V1_OUT/pg1.nsql-project" 2>/dev/null
expect_re "g-active-placeholder" "$V1_OUT/pg1-tabs.txt" '^\*gone2\.sql\|.*view=true'
expect "g-body-shown" "$V1_OUT/pg1-editor.txt" "-- v1pp f unsaved keep"
expect_re "g-body-readonly" "$V1_OUT/pg1-editor.txt" '^gone2\.sql\|[^|]*\|true\|'
expect_re "g-tab-ro" "$V1_OUT/pg1-tabs.txt" '^\*gone2\.sql\|.*ro=true'
expect_eq "g-body-still-kept" "$(grep -oF -- '-- v1pp f unsaved keep' "$V1_OUT/pg1.nsql-project" 2>/dev/null | wc -l | tr -d ' ')" "1"
expect_eq "g-no-double-tab" "$(grep -oF '"path": "sql/gone2.sql"' "$V1_OUT/pg1.nsql-project" 2>/dev/null | wc -l | tr -d ' ')" "1"
#   g2. 등록 폴더 하나가 없는 프로젝트 → 로그·알림 "project folder(s) not found on this PC" · 폴더가 모두 있으면(b 실행 로그) 그 줄 없음
printf '{"version": 2, "folders": [{"path": "."}, {"path": "%s"}], "tabs": [{"path": "sql/a.sql", "title": "a.sql", "id": 21}]}\n' "$(v1_m "$WK/nofolder/lib")" > "$P/g2.nsql-project"
v1_gui pg2 "project.load:$PM/g2.nsql-project,@after:1500:log.dump:$O/pg2-log.txt,@after:1700:toast.dump:$O/pg2-toast.txt,@after:2300:file.exit,@after:2900:toolbar.pick:project.exit_skip" 25
expect "g-folders-missing-logged" "$V1_OUT/pg2-log.txt" "project folder(s) not found on this PC"
expect "g-folders-missing-toast" "$V1_OUT/pg2-toast.txt" "project folder(s) not found on this PC"
expect_not "g-no-false-notice" "$V1_OUT/pb-log.txt" "project folder(s) not found"
#   g3. 닫기 길(`file.close_tab` = Ctrl+W → close_tab_guarded): 미저장 본문 든 자리 탭 첫 닫기 = 안 닫힘 + 안내 · 상태줄 글 끝 공백 없음
mkf 2 g3.nsql-project
v1_gui pg3 "project.load:$PM/g3.nsql-project,@after:1500:file.close_tab,@after:1800:status.dump:$O/pg3-status.txt,@after:2000:tabs.dump:$O/pg3-tabs.txt,@after:2500:project.save,@after:3100:file.exit,@after:3700:toolbar.pick:project.exit_skip" 25
cp "$P/g3.nsql-project" "$V1_OUT/pg3.nsql-project" 2>/dev/null
expect "g-close-tab-notice" "$V1_OUT/pg3-status.txt" "close it again to confirm"
expect_re "g-close-tab-still" "$V1_OUT/pg3-tabs.txt" '^\*gone2\.sql\|.*view=true'
expect "g-close-tab-body-kept" "$V1_OUT/pg3.nsql-project" "-- v1pp f unsaved keep"
LASTC=$(tail -c 1 "$V1_OUT/pg3-status.txt" 2>/dev/null | od -An -c | tr -d ' ')
if [ -s "$V1_OUT/pg3-status.txt" ] && [ "$LASTC" != "" ] && [ "$LASTC" != '\n' ]; then v1_pass "g-notice-no-trailing-space"
else v1_failmsg "g-notice-no-trailing-space" "끝 글자='$LASTC' · $(head -c 200 "$V1_OUT/pg3-status.txt" 2>/dev/null)"; fi

# 공통 — 모든 기동 panic 없음
for t in pa pb pb2 pc pd pe pe2 pf1 pf2 pf3a pf3b pf4 pg1 pg2 pg3; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
