#!/usr/bin/env bash
# V1 rclose — 결과 탭 닫기 = 미적용 편집 확인(T-507 · 87 §16): 편집 있음 → 상태 안내 · cancel = 유지 · discard = 닫힘 / 편집 없음 = 바로 닫힘.
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_cli conn add lite "sqlite:$(v1_m "$V1_HOME")/d.sqlite" -d sqlite --no-prompt >/dev/null
printf "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);\nINSERT INTO t VALUES (1,'a');\nINSERT INTO t VALUES (2,'b');\n" > "$V1_OUT/mk.sql"
v1_cli run -c lite --no-prompt "$(v1_m "$V1_OUT/mk.sql")" >/dev/null
printf 'SELECT * FROM t\n' > "$V1_OUT/q.sql"
W=$(v1_m "$V1_OUT")
v1_gui dirty "@connected:open:$W/q.sql,@connected:@after:1500:run.all,@connected:@after:4000:grid.edit.set:0;1;zz,@connected:@after:4500:grid.dump:$W/a.grid,@connected:@after:5000:result.tab.close,@connected:@after:5500:status.dump:$W/b.st,@connected:@after:6200:rclose.cancel,@connected:@after:6700:grid.dump:$W/c.grid,@connected:@after:7200:result.tab.close,@connected:@after:7700:rclose.discard,@connected:@after:8500:grid.dump:$W/d.grid,@connected:@after:9000:file.exit" 25 lite
expect "edit-dirty" "$V1_OUT/a.grid" "dirty=true"
expect "ask-on-close" "$V1_OUT/b.st" "have unapplied edits"
expect "cancel-keeps" "$V1_OUT/c.grid" "dirty=true"
expect "discard-closes" "$V1_OUT/d.grid" "rows=0"
v1_gui clean "@connected:open:$W/q.sql,@connected:@after:1500:run.all,@connected:@after:4000:result.tab.close,@connected:@after:4500:status.dump:$W/e.st,@connected:@after:4800:grid.dump:$W/e.grid,@connected:@after:5500:file.exit" 20 lite
expect_not "clean-no-ask" "$V1_OUT/e.st" "unapplied edits"
expect "clean-closes" "$V1_OUT/e.grid" "rows=0"
v1_done
