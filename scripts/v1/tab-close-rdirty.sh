#!/usr/bin/env bash
# V1 tab-close-rdirty — 편집기 탭 닫기 = 딸린 결과 탭의 미적용 편집 확인(T-507 잔여 · 11-22 묶음 4 · 87 §16):
#   편집 있음 → 첫 `file.close_tab`(Ctrl+W 길 = close_tab_guarded)은 막고 상태줄 안내 · 같은 탭을 한 번 더 닫으면 닫힘 /
#   편집 없음 = 한 번에 닫힘. 미적용 편집 만들기는 rclose.sh와 같은 길(SQLite 파일 DB · `grid.edit.set`) · 키·마우스 주입 0 · 격리 홈.
#   x = 종료 · 프로젝트 닫기 · 다른 프로젝트 열기 앞의 확인(11-22 묶음 5 · 아래 x 절).
#   마지막 탭은 닫히지 않고 비워지므로 채움 탭(q2.sql)을 먼저 열어 둔다(q.sql = 활성 · 실행 탭).
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_cli conn add lite "sqlite:$(v1_m "$V1_HOME")/d.sqlite" -d sqlite --no-prompt >/dev/null
printf "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);\nINSERT INTO t VALUES (1,'a');\nINSERT INTO t VALUES (2,'b');\n" > "$V1_OUT/mk.sql"
v1_cli run -c lite --no-prompt "$(v1_m "$V1_OUT/mk.sql")" >/dev/null
printf 'SELECT * FROM t\n' > "$V1_OUT/q.sql"
printf -- '-- filler\n' > "$V1_OUT/q2.sql"
W=$(v1_m "$V1_OUT")
# r1·r2. 미적용 편집 → 첫 닫기 막힘 → 두 번째 닫힘
v1_gui dirty "@connected:open:$W/q2.sql,@connected:@after:800:open:$W/q.sql,@connected:@after:1800:run.all,@connected:@after:4300:grid.edit.set:0;1;zz,@connected:@after:4800:grid.dump:$W/a.grid,@connected:@after:5200:tabs.dump:$W/a.tabs,@connected:@after:5500:file.close_tab,@connected:@after:5900:status.dump:$W/b.st,@connected:@after:6100:tabs.dump:$W/b.tabs,@connected:@after:6600:file.close_tab,@connected:@after:7100:tabs.dump:$W/c.tabs,@connected:@after:7600:status.dump:$W/c.st,@connected:@after:8200:file.exit" 25 lite
expect "r0-edit-dirty" "$V1_OUT/a.grid" "dirty=true"
expect "r0-q-active" "$V1_OUT/a.tabs" "*q.sql|"
expect "r1-first-close-blocked" "$V1_OUT/b.tabs" "q.sql|"
expect "r1-notice" "$V1_OUT/b.st" "unapplied edits - close the tab again"
expect_not "r2-second-close-closed" "$V1_OUT/c.tabs" "q.sql|"
expect "r2-filler-kept" "$V1_OUT/c.tabs" "q2.sql|"
# r3. 대조 — 미적용 편집 없음 = 한 번에 닫힘 · 안내 없음
v1_gui clean "@connected:open:$W/q2.sql,@connected:@after:800:open:$W/q.sql,@connected:@after:1800:run.all,@connected:@after:4300:tabs.dump:$W/e.tabs,@connected:@after:4600:file.close_tab,@connected:@after:5000:status.dump:$W/f.st,@connected:@after:5200:tabs.dump:$W/f.tabs,@connected:@after:5800:file.exit" 20 lite
expect "r3-q-open-before" "$V1_OUT/e.tabs" "*q.sql|"
expect_not "r3-clean-closes-once" "$V1_OUT/f.tabs" "q.sql|"
expect_not "r3-no-notice" "$V1_OUT/f.st" "unapplied edits"
# x. 탭이 통째로 사라지는 동작 앞의 확인(T-507 마무리 · 11-22 묶음 5): 종료 · 프로젝트 닫기 · 다른 프로젝트 열기 →
#    상태 팝업("N result tab(s) have unapplied edits - continuing discards them.") · `close.rdirty_go` = 미뤄 둔 동작을 잇는다 · `close.cancel` = 아무 일 없음.
#    팝업 항목은 기동 명령 `close.*`(startup_cmd 자동 시험 접두 → indent_pick 길) · 살아 있음 = 뒤이은 `fs.write` 표식.
# x1. 종료: 막힘 → 취소 = 살아 있음 · 편집 그대로 → 다시 종료 → 버리고 계속 = 종료 코드 0
v1_gui xexit "@connected:open:$W/q.sql,@connected:@after:1500:run.all,@connected:@after:4000:grid.edit.set:0;1;zz,@connected:@after:4500:grid.dump:$W/x1a.grid,@connected:@after:5000:file.exit,@connected:@after:5500:status.dump:$W/x1b.st,@connected:@after:5800:fs.write:$W/x1-alive1.txt|alive,@connected:@after:6300:close.cancel,@connected:@after:6800:grid.dump:$W/x1c.grid,@connected:@after:7000:tabs.dump:$W/x1c.tabs,@connected:@after:7300:fs.write:$W/x1-alive2.txt|alive,@connected:@after:7800:file.exit,@connected:@after:8300:close.rdirty_go,@connected:@after:9800:fs.write:$W/x1-alive3.txt|alive" 25 lite
expect "x1-edit-dirty" "$V1_OUT/x1a.grid" "dirty=true"
expect "x1-exit-blocked" "$V1_OUT/x1-alive1.txt" "alive"
expect "x1-notice" "$V1_OUT/x1b.st" "unapplied edits - continuing discards them"
expect "x1-cancel-keeps" "$V1_OUT/x1c.grid" "dirty=true"
expect "x1-cancel-tab-kept" "$V1_OUT/x1c.tabs" "q.sql|"
expect "x1-alive-after-cancel" "$V1_OUT/x1-alive2.txt" "alive"
expect_eq "x1-go-exits" "$(cat "$V1_OUT/xexit.exit")" "0"
if [ -e "$V1_OUT/x1-alive3.txt" ]; then v1_failmsg "x1-go-gone" "종료 뒤 표식이 써짐"; else v1_pass "x1-go-gone"; fi
# x2. 대조 — 미적용 편집 없음 = `file.exit` 한 번에 종료(물음 없음)
v1_gui xclean "@connected:open:$W/q.sql,@connected:@after:1500:run.all,@connected:@after:4000:file.exit,@connected:@after:5500:fs.write:$W/x2-alive.txt|alive" 20 lite
expect_eq "x2-clean-exits-once" "$(cat "$V1_OUT/xclean.exit")" "0"
if [ -e "$V1_OUT/x2-alive.txt" ]; then v1_failmsg "x2-no-ask" "종료가 막혀 표식이 써짐"; else v1_pass "x2-no-ask"; fi
# x3. 프로젝트 닫기: 막힘(안내) → 버리고 계속 = 닫힘("Project closed")
mkdir -p "$V1_OUT/pa" "$V1_OUT/pb"
cp "$V1_OUT/q.sql" "$V1_OUT/pa/q.sql"; printf -- '-- b
' > "$V1_OUT/pb/b.sql"
printf '{"version": 2, "folders": [{"path": "."}]}
' > "$V1_OUT/pa/a.nsql-project"
printf '{"version": 2, "folders": [{"path": "."}], "active": 0, "tabs": [{"path": "b.sql", "title": "b.sql", "id": 1}]}
' > "$V1_OUT/pb/b.nsql-project"
v1_set "project.autosave=off"
v1_gui xclose "@connected:project.load:$W/pa/a.nsql-project,@connected:@after:1000:open:$W/pa/q.sql,@connected:@after:2000:run.all,@connected:@after:4500:grid.edit.set:0;1;zz,@connected:@after:5000:grid.dump:$W/x3a.grid,@connected:@after:5500:project.close,@connected:@after:6000:status.dump:$W/x3b.st,@connected:@after:6200:tabs.dump:$W/x3b.tabs,@connected:@after:6700:close.rdirty_go,@connected:@after:7300:status.dump:$W/x3c.st,@connected:@after:7500:tabs.dump:$W/x3c.tabs,@connected:@after:8000:file.exit,@connected:@after:8600:toolbar.pick:project.exit_skip" 25 lite
expect "x3-edit-dirty" "$V1_OUT/x3a.grid" "dirty=true"
expect "x3-close-blocked" "$V1_OUT/x3b.st" "unapplied edits - continuing discards them"
expect "x3-close-blocked-tab" "$V1_OUT/x3b.tabs" "q.sql|"
expect "x3-go-closes" "$V1_OUT/x3c.st" "Project closed"
expect_eq "x3-exit-0" "$(cat "$V1_OUT/xclose.exit")" "0"
# x4. 다른 프로젝트 열기: 막힘(안내 · B의 탭 없음) → 버리고 계속 = B로 바뀜(b.sql 탭)
v1_gui xload "@connected:project.load:$W/pa/a.nsql-project,@connected:@after:1000:open:$W/pa/q.sql,@connected:@after:2000:run.all,@connected:@after:4500:grid.edit.set:0;1;zz,@connected:@after:5000:grid.dump:$W/x4a.grid,@connected:@after:5500:project.load:$W/pb/b.nsql-project,@connected:@after:6000:status.dump:$W/x4b.st,@connected:@after:6200:tabs.dump:$W/x4b.tabs,@connected:@after:6700:close.rdirty_go,@connected:@after:7700:tabs.dump:$W/x4c.tabs,@connected:@after:8200:file.exit,@connected:@after:8800:toolbar.pick:project.exit_skip" 25 lite
expect "x4-edit-dirty" "$V1_OUT/x4a.grid" "dirty=true"
expect "x4-load-blocked" "$V1_OUT/x4b.st" "unapplied edits - continuing discards them"
expect_not "x4-load-blocked-tab" "$V1_OUT/x4b.tabs" "b.sql|"
expect "x4-go-loads" "$V1_OUT/x4c.tabs" "b.sql|"
expect_eq "x4-exit-0" "$(cat "$V1_OUT/xload.exit")" "0"
for t in dirty clean xexit xclean xclose xload; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
v1_done
