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
#   h. 절대 경로 알림(T-172 · D-188 · `project.warn_absolute`): 프로젝트 밖 탭이 절대 경로로 남으면 명시 저장마다 로그 한 줄 · 상대뿐이면 0 · 끄면 0
#   i. 등록 폴더 이름(T-172 P2 `folders[].name`): leaf가 같은 두 폴더도 이름을 적으면 `${workspaceFolder:<이름>}` 앵커 · 다시 열면 복원 · 안 적으면 절대(종전)
#   j. 경로 변수(T-172 P3 1단계): `"vars"` + `${var:이름}/…` 탭 = 열림 · 정의 없는 변수 = 자리 탭 · 저장 뒤 vars·version 3·글 그대로
#   k. 변수의 이 PC 자리(T-172 P3 2단계): `project.var_set` → 자리 탭이 진짜 탭으로 · 대응표 `var:이름` · 다시 열면 적용 · 찾아보기 꼬리 일치로 추론
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

# ── i. 등록 폴더 이름(T-172 P2 `folders[].name` · D-187 · 협업 초안 11-22 · h2가 settings를 바꾸므로 h 앞) ───────────────────────────
#   leaf가 같은 두 등록 폴더(x/lib · y/lib) — 이름을 적으면 `${workspaceFolder:<이름>}` 앵커 · 안 적으면 둘 다 절대(종전).
#   h 절과 같은 이유로 HOME을 WK 안 빈 폴더로(%TEMP%가 홈 아래면 대조군이 `${userHome}/…`이 되어 "절대" 판정이 안 선다).
LX="$WK/x/lib"; LY="$WK/y/lib"; IH="$WK/ih"; mkdir -p "$LX" "$LY" "$IH"
printf 'SELECT 7 AS x FROM dual;\n' > "$LX/x.sql"; printf 'SELECT 8 AS y FROM dual;\n' > "$LY/y.sql"
LXM=$(v1_m "$LX"); LYM=$(v1_m "$LY"); IHM=$(v1_m "$IH")
rm -f "$V1_OUT"/pi1-tabs.txt "$V1_OUT"/pi2-tabs.txt
# i1. 이름 적음 → 저장: 이름 보존 · 두 앵커 · 절대 탭 없음
printf '{"version": 2, "folders": [{"path": "."}, {"path": "%s", "name": "lib-x"}, {"path": "%s", "name": "lib-y"}]}\n' "$LXM" "$LYM" > "$P/i1.nsql-project"
HOME="$IHM" v1_gui pi1 "project.load:$PM/i1.nsql-project,@after:1500:open:$LXM/x.sql,@after:2200:open:$LYM/y.sql,@after:3000:project.save,@after:3600:file.exit,@after:4200:toolbar.pick:project.exit_skip" 25
cp "$P/i1.nsql-project" "$V1_OUT/pi1.nsql-project" 2>/dev/null
expect "i1-name-x-kept" "$V1_OUT/pi1.nsql-project" '"name": "lib-x"'
expect "i1-name-y-kept" "$V1_OUT/pi1.nsql-project" '"name": "lib-y"'
expect "i1-anchor-x" "$V1_OUT/pi1.nsql-project" '"${workspaceFolder:lib-x}/x.sql"'
expect "i1-anchor-y" "$V1_OUT/pi1.nsql-project" '"${workspaceFolder:lib-y}/y.sql"'
expect_not "i1-no-abs-x" "$V1_OUT/pi1.nsql-project" "\"path\": \"$LXM/x.sql\""
expect_not "i1-no-abs-y" "$V1_OUT/pi1.nsql-project" "\"path\": \"$LYM/y.sql\""
# i2. 다시 열기 → 두 탭 복원(앵커가 이름으로 풀린다)
HOME="$IHM" v1_gui pi2 "project.load:$PM/i1.nsql-project,@after:1800:tabs.dump:$O/pi2-tabs.txt,@after:2400:file.exit,@after:3000:toolbar.pick:project.exit_skip" 25
expect_re "i2-tab-x-restored" "$V1_OUT/pi2-tabs.txt" '^\*?x\.sql\|.*view=false'
expect_re "i2-tab-y-restored" "$V1_OUT/pi2-tabs.txt" '^\*?y\.sql\|.*view=false'
# i0. 대조 — 이름 없는 같은 구성 = 두 탭 모두 절대 · "name" 키 없음
printf '{"version": 2, "folders": [{"path": "."}, {"path": "%s"}, {"path": "%s"}]}\n' "$LXM" "$LYM" > "$P/i0.nsql-project"
HOME="$IHM" v1_gui pi0 "project.load:$PM/i0.nsql-project,@after:1500:open:$LXM/x.sql,@after:2200:open:$LYM/y.sql,@after:3000:project.save,@after:3600:file.exit,@after:4200:toolbar.pick:project.exit_skip" 25
cp "$P/i0.nsql-project" "$V1_OUT/pi0.nsql-project" 2>/dev/null
expect "i0-abs-x" "$V1_OUT/pi0.nsql-project" "\"path\": \"$LXM/x.sql\""
expect "i0-abs-y" "$V1_OUT/pi0.nsql-project" "\"path\": \"$LYM/y.sql\""
# "name" 키는 북마크 묶음(`{"id": 1, "name": "" …}`)에도 있으므로 폴더 항목으로 좁힌다.
expect_not "i0-no-name-key-x" "$V1_OUT/pi0.nsql-project" "\"path\": \"$LXM\", \"name\""
expect_not "i0-no-name-key-y" "$V1_OUT/pi0.nsql-project" "\"path\": \"$LYM\", \"name\""
expect "i0-folder-x-written" "$V1_OUT/pi0.nsql-project" "\"path\": \"$LXM\""
expect_not "i0-no-folder-anchor" "$V1_OUT/pi0.nsql-project" '${workspaceFolder:lib'

# ── j. 경로 변수(T-172 P3 1단계 · `"vars"` + `${var:이름}/…` 앵커 · c9355995 · 협업 11-22) ───────────────────────────
#   프로젝트 파일에 직접 적은 `"version": 3` · `"vars": {"DATA": <임시 폴더>}` · 탭 `${var:DATA}/v.sql`(있음) · `${var:NOPE}/x.sql`(정의 없음)
#   → 열면 v.sql = 진짜 탭 · x.sql = 자리 탭 → 저장 뒤 vars 보존 · version 3 · 두 탭 글 그대로(미정의 변수 = 글자 그대로 보존).
#   h·i 절과 같은 이유로 HOME을 WK 안 빈 폴더로(홈 앵커와 겹치지 않게).
DD="$WK/data"; mkdir -p "$DD"; printf 'SELECT 9 AS v FROM dual;\n' > "$DD/v.sql"; DDM=$(v1_m "$DD")
rm -f "$V1_OUT"/pj-tabs.txt
printf '{"version": 3, "vars": {"DATA": "%s"}, "folders": [{"path": "."}], "active": 0, "tabs": [{"path": "${var:DATA}/v.sql", "title": "v.sql", "id": 41}, {"path": "${var:NOPE}/x.sql", "title": "x.sql", "id": 42}]}\n' "$DDM" > "$P/j.nsql-project"
HOME="$IHM" v1_gui pj "project.load:$PM/j.nsql-project,@after:1800:tabs.dump:$O/pj-tabs.txt,@after:2300:project.save,@after:2900:file.exit,@after:3500:toolbar.pick:project.exit_skip" 25
cp "$P/j.nsql-project" "$V1_OUT/pj.nsql-project" 2>/dev/null
expect_re "j-var-tab-opened" "$V1_OUT/pj-tabs.txt" '^\*?v\.sql\|.*view=false'
expect_re "j-undefined-var-placeholder" "$V1_OUT/pj-tabs.txt" '^\*?x\.sql\|.*view=true'
expect "j-version3" "$V1_OUT/pj.nsql-project" '"version": 3'
expect "j-vars-kept" "$V1_OUT/pj.nsql-project" "\"DATA\": \"$DDM\""
expect "j-var-anchor-kept" "$V1_OUT/pj.nsql-project" '"${var:DATA}/v.sql"'
expect "j-undefined-var-kept" "$V1_OUT/pj.nsql-project" '"${var:NOPE}/x.sql"'
expect_not "j-no-abs-tab" "$V1_OUT/pj.nsql-project" "\"path\": \"$DDM/v.sql\""

# ── k. 변수의 이 PC 자리(T-172 P3 2단계 · `project.var_set:이름=폴더` · `project.pc_paths`의 `var:이름` · 1cb7443f · 협업 11-23) ─────────
#   k1. vars 없는 `${var:NOPE}/sub/x.sql` · `${var:NOPE}/y.sql` → 자리 탭 2 → var_set → 진짜 탭 2 · 상태줄 · 대응표 `var:NOPE` · 파일엔 글 그대로
#   k2. 같은 홈으로 다시 열기 → 자리 탭 0(대응표 적용)
#   k3. 다른 변수 Q의 자리 탭에서 `project.missing_found:<폴더>/sub/x.sql` → 꼬리 일치로 자리 추론 · 같은 변수의 다른 탭도 열림
KD="$WK/kd"; QD="$WK/qd"; mkdir -p "$KD/sub" "$QD/sub"
for b in "$KD" "$QD"; do printf 'SELECT 11 AS x FROM dual;\n' > "$b/sub/x.sql"; printf 'SELECT 12 AS y FROM dual;\n' > "$b/y.sql"; done
KDM=$(v1_m "$KD"); QDM=$(v1_m "$QD")
mkk() { printf '{"version": 3, "folders": [{"path": "."}], "active": 0, "tabs": [{"path": "${var:%s}/sub/x.sql", "title": "x.sql", "id": 51}, {"path": "${var:%s}/y.sql", "title": "y.sql", "id": 52}]}\n' "$2" "$2" > "$P/$1"; }
rm -f "$V1_OUT"/pk*-tabs.txt "$V1_OUT"/pk1-status.txt
mkk k.nsql-project NOPE
HOME="$IHM" v1_gui pk1 "project.load:$PM/k.nsql-project,@after:1800:tabs.dump:$O/pk0-tabs.txt,@after:1950:log.dump:$O/pk0-log.txt,@after:2100:project.var_set:NOPE=$KDM,@after:2900:status.dump:$O/pk1-status.txt,@after:3200:tabs.dump:$O/pk1-tabs.txt,@after:3500:project.save,@after:4100:file.exit,@after:4700:toolbar.pick:project.exit_skip" 25
cp "$P/k.nsql-project" "$V1_OUT/pk1.nsql-project" 2>/dev/null
expect_re "k1-before-placeholder-x" "$V1_OUT/pk0-tabs.txt" '^\*?x\.sql\|.*view=true'
expect_re "k1-before-placeholder-y" "$V1_OUT/pk0-tabs.txt" '^\*?y\.sql\|.*view=true'
expect "k1-vars-missing-notice" "$V1_OUT/pk0-log.txt" "path variables without a folder on this PC - NOPE (2)"
expect "k1-status" "$V1_OUT/pk1-status.txt" "Path variable NOPE on this PC"
expect_re "k1-tab-x-real" "$V1_OUT/pk1-tabs.txt" '^\*?x\.sql\|.*view=false'
expect_re "k1-tab-y-real" "$V1_OUT/pk1-tabs.txt" '^\*?y\.sql\|.*view=false'
expect "k1-pc-paths-var" "$V1_HOME/settings.conf" "var:NOPE"
expect "k1-file-keeps-var-x" "$V1_OUT/pk1.nsql-project" '"${var:NOPE}/sub/x.sql"'
expect "k1-file-keeps-var-y" "$V1_OUT/pk1.nsql-project" '"${var:NOPE}/y.sql"'
expect_not "k1-file-no-pc-path" "$V1_OUT/pk1.nsql-project" "$KDM"
HOME="$IHM" v1_gui pk2 "project.load:$PM/k.nsql-project,@after:1800:tabs.dump:$O/pk2-tabs.txt,@after:2300:file.exit,@after:2900:toolbar.pick:project.exit_skip" 25
expect_re "k2-reopen-x-real" "$V1_OUT/pk2-tabs.txt" '^\*?x\.sql\|.*view=false'
expect_re "k2-reopen-y-real" "$V1_OUT/pk2-tabs.txt" '^\*?y\.sql\|.*view=false'
expect_not_re "k2-no-placeholder" "$V1_OUT/pk2-tabs.txt" 'view=true'
mkk q.nsql-project Q
HOME="$IHM" v1_gui pk3 "project.load:$PM/q.nsql-project,@after:1800:tabs.dump:$O/pk3a-tabs.txt,@after:2100:project.missing_found:$QDM/sub/x.sql,@after:2900:tabs.dump:$O/pk3-tabs.txt,@after:3300:file.exit,@after:3900:toolbar.pick:project.exit_skip" 25
expect_re "k3-before-placeholder" "$V1_OUT/pk3a-tabs.txt" '^\*?x\.sql\|.*view=true'
expect_re "k3-found-x-real" "$V1_OUT/pk3-tabs.txt" '^\*?x\.sql\|.*view=false'
expect_re "k3-inferred-y-real" "$V1_OUT/pk3-tabs.txt" '^\*?y\.sql\|.*view=false'
expect "k3-pc-paths-var-q" "$V1_HOME/settings.conf" "var:Q"

# ── l. 변수 자리가 멀쩡할 때 찾아보기 = 그 탭만(독립 검토 후속 · `var_missing_here` · 11-23 묶음 17) ─────────────
#   `vars: {"DATA": <실재 폴더>}` + `${var:DATA}/a.sql`(없음) + `${var:DATA}/b.sql`(있음) → 자리 탭 1 → 다른 폴더의 a.sql로 찾아보기 →
#   a.sql만 그 파일로 열림 · 대응표에 `var:DATA` 없음 · b.sql 경로 불변 · "Path variable DATA on this PC" 없음(종전 = 변수 전체가 옮겨짐)
LD="$WK/ld"; OD="$WK/od"; mkdir -p "$LD" "$OD"; printf 'SELECT 21 AS b FROM dual;\n' > "$LD/b.sql"; printf 'SELECT 22 AS a FROM dual;\n' > "$OD/a.sql"
LDM=$(v1_m "$LD"); ODM=$(v1_m "$OD")
printf '{"version": 3, "vars": {"DATA": "%s"}, "folders": [{"path": "."}], "active": 0, "tabs": [{"path": "${var:DATA}/a.sql", "title": "a.sql", "id": 61}, {"path": "${var:DATA}/b.sql", "title": "b.sql", "id": 62}]}\n' "$LDM" > "$P/l.nsql-project"
rm -f "$V1_OUT"/pl-*.txt
HOME="$IHM" v1_gui pl "project.load:$PM/l.nsql-project,@after:1800:tabs.dump:$O/pl-0.txt,@after:2100:project.missing_found:$ODM/a.sql,@after:2900:tabs.dump:$O/pl-1.txt,@after:3100:log.dump:$O/pl-log.txt,@after:3400:project.save,@after:4000:file.exit,@after:4600:toolbar.pick:project.exit_skip" 25
cp "$P/l.nsql-project" "$V1_OUT/pl.nsql-project" 2>/dev/null
expect_re "l-before-one-placeholder" "$V1_OUT/pl-0.txt" '^\*?a\.sql\|.*view=true'
expect_re "l-b-real-before" "$V1_OUT/pl-0.txt" '^\*?b\.sql\|.*view=false'
expect_re "l-a-opened" "$V1_OUT/pl-1.txt" '^\*?a\.sql\|.*view=false'
expect_not "l-no-pc-var" "$V1_HOME/settings.conf" "var:DATA"
expect_not "l-no-var-notice" "$V1_OUT/pl-log.txt" "Path variable DATA on this PC"
expect "l-b-path-unchanged" "$V1_OUT/pl.nsql-project" '"${var:DATA}/b.sql"'
expect "l-vars-kept" "$V1_OUT/pl.nsql-project" "\"DATA\": \"$LDM\""

# ── m. 변수 아래 등록 폴더의 이름이 이 PC 자리 지정 뒤에도 남음(둘째 독립 검토 후속 · `set_var_pc` · 11-23 묶음 18) ─────────
#   vars DATA = 이 PC에 없는 자리 + 등록 폴더 `{"path": "${var:DATA}/lib", "name": "lib-x"}` → `project.var_set:DATA=<실재 폴더>` → 저장 →
#   파일에 `"name": "lib-x"` 그대로 · 정의 경로 `${var:DATA}/lib` 그대로(종전 = 이 PC 자리를 옮기면 폴더 이름이 저장에서 빠졌다)
MD="$WK/md"; mkdir -p "$MD/lib"; printf 'SELECT 31 AS m FROM dual;\n' > "$MD/lib/m.sql"; MDM=$(v1_m "$MD")
printf '{"version": 3, "vars": {"DATA": "%s"}, "folders": [{"path": "."}, {"path": "${var:DATA}/lib", "name": "lib-x"}], "active": 0, "tabs": [{"path": "sql/a.sql", "title": "a.sql", "id": 71}]}\n' "$(v1_m "$WK/nowhere")" > "$P/m.nsql-project"
HOME="$IHM" v1_gui pm "project.load:$PM/m.nsql-project,@after:1800:project.var_set:DATA=$MDM,@after:2600:project.save,@after:3200:file.exit,@after:3800:toolbar.pick:project.exit_skip" 25
cp "$P/m.nsql-project" "$V1_OUT/pm.nsql-project" 2>/dev/null
expect "m-folder-name-kept" "$V1_OUT/pm.nsql-project" '"name": "lib-x"'
expect "m-folder-path-kept" "$V1_OUT/pm.nsql-project" '"${var:DATA}/lib"'
expect "m-pc-var-recorded" "$V1_HOME/settings.conf" "var:DATA"
expect_not "m-no-abs-folder" "$V1_OUT/pm.nsql-project" "\"path\": \"$MDM/lib\""

# ── h. 절대 경로 알림(T-172 · D-188 · project.warn_absolute · 협업 초안 11-22) ───────────
#   OS 임시 폴더가 홈 아래(Windows %TEMP% = 사용자 폴더 · 맥 /var/folders는 홈 밖)라 그대로면 탭이 `${userHome}/…`으로 적혀 세지 않는다
#   → 이 절의 기동만 HOME을 WK 안 빈 폴더로 바꿔 홈 앵커를 피한다(home_dir_str = HOME → USERPROFILE).
FAR="$WK/far"; HH="$WK/h"; mkdir -p "$FAR" "$HH"
printf 'SELECT 6 AS h FROM dual;
' > "$FAR/h.sql"
FM=$(v1_m "$FAR"); HM=$(v1_m "$HH")
mkh() { printf '%s
' '{"version": 2, "folders": [{"path": "."}], "active": 0, "tabs": [{"path": "sql/a.sql", "title": "a.sql", "id": 31}]}' > "$P/$1"; }
ABSW="path(s) in the project file are absolute"
rm -f "$V1_OUT"/ph1-log.txt "$V1_OUT"/ph0-log.txt "$V1_OUT"/ph2-log.txt
#   h1. 켬(기본) — 프로젝트 밖 탭 1개 + 저장 2번(명시 저장은 매번) → 줄 2개 · 수 = 1
mkh h1.nsql-project
HOME="$HM" v1_gui ph1 "project.load:$PM/h1.nsql-project,@after:1500:open:$FM/h.sql,@after:2300:project.save,@after:2700:project.save,@after:3100:log.dump:$O/ph1-log.txt,@after:3700:file.exit,@after:4300:toolbar.pick:project.exit_skip" 25
cp "$P/h1.nsql-project" "$V1_OUT/ph1.nsql-project" 2>/dev/null
expect "h1-tab-abs-in-file" "$V1_OUT/ph1.nsql-project" "$FM/h.sql"
expect "h1-warn-count-1" "$V1_OUT/ph1-log.txt" "1 $ABSW"
expect_eq "h1-warn-each-explicit-save" "$(grep -cF "$ABSW" "$V1_OUT/ph1-log.txt" 2>/dev/null)" "2"
#   h0. 상대 경로뿐 → 줄 0(부정 판정 전제 = 덤프가 실제로 돌았다 — 로그가 비면 0바이트 파일)
mkh h0.nsql-project
HOME="$HM" v1_gui ph0 "project.load:$PM/h0.nsql-project,@after:1500:project.save,@after:1900:log.dump:$O/ph0-log.txt,@after:2500:file.exit,@after:3100:toolbar.pick:project.exit_skip" 25
expect_eq "h0-dump-ran" "$([ -f "$V1_OUT/ph0-log.txt" ] && echo y)" "y"
expect_not "h0-no-warn-relative" "$V1_OUT/ph0-log.txt" "$ABSW"
#   h2. 끔 → 줄 0(settings를 바꾸므로 시나리오 맨 끝)
v1_set "project.warn_absolute=off"
mkh h2.nsql-project
HOME="$HM" v1_gui ph2 "project.load:$PM/h2.nsql-project,@after:1500:open:$FM/h.sql,@after:2300:project.save,@after:2700:log.dump:$O/ph2-log.txt,@after:3300:file.exit,@after:3900:toolbar.pick:project.exit_skip" 25
expect "h2-tab-abs-in-file" "$P/h2.nsql-project" "$FM/h.sql"
expect_eq "h2-dump-ran" "$([ -f "$V1_OUT/ph2-log.txt" ] && echo y)" "y"
expect_not "h2-off-no-warn" "$V1_OUT/ph2-log.txt" "$ABSW"

# 공통 — 모든 기동 panic 없음
for t in pa pb pb2 pc pd pe pe2 pf1 pf2 pf3a pf3b pf4 pg1 pg2 pg3 pi1 pi2 pi0 pj pk1 pk2 pk3 pl pm ph1 ph0 ph2; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
