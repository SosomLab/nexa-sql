#!/usr/bin/env bash
# V1 tab-close-rdirty — 편집기 탭 닫기 = 딸린 결과 탭의 미적용 편집 확인(T-507 잔여 · 11-22 묶음 4 · 87 §16):
#   편집 있음 → 첫 `file.close_tab`(Ctrl+W 길 = close_tab_guarded)은 막고 상태줄 안내 · 같은 탭을 한 번 더 닫으면 닫힘 /
#   편집 없음 = 한 번에 닫힘. 미적용 편집 만들기는 rclose.sh와 같은 길(SQLite 파일 DB · `grid.edit.set`) · 키·마우스 주입 0 · 격리 홈.
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
for t in dirty clean; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
v1_done
