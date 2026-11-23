#!/usr/bin/env bash
# V1 tx-pending — 미커밋 대기 수(T-107b · journal 11-22 §17 8장면을 시나리오로 올림 · 11-24): `tx.dump`의 pending= · txlog_updates= ·
#   종료/탭 닫기 확인 팝업(`open_tx_guard` · 상태줄 팝업 = toolbar.menudump). SQLite 격리 홈 · 헤드리스 · 입력 주입 0.
#   기대값 = 11-22 §17 표 그대로 + s8은 D-500(그리드 적용 = 행마다 1건 · a8c653b8) 뒤 값 2/2.
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1tx-XXXX"); W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
rm -f "$V1_OUT"/tx-*.txt
pend() { tr -d '\r' < "$1" 2>/dev/null | sed -n 's/^pending=\([0-9]*\)|.*txlog_updates=\([0-9]*\).*/\1\/\2/p' | head -1; }
mk() { # [session.autocommit=on] — 새 격리 홈 + Local(SQLite) + t_h 3행
  v1_home en; local ac=off a; for a in "$@"; do case $a in session.autocommit=*) ac=${a#*=};; esac; done
  v1_set "session.autocommit=$ac"   # 같은 키가 두 줄이면 앞 줄이 이긴다 → 한 줄만
  "$V1_NSQL" conn add Local "sqlite:$(v1_m "$V1_HOME")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  printf "CREATE TABLE t_h (id INTEGER PRIMARY KEY, v TEXT);\nINSERT INTO t_h VALUES (1,'o1');\nINSERT INTO t_h VALUES (2,'o2');\nINSERT INTO t_h VALUES (3,'o3');\n" > "$WK/mk.sql"
  "$V1_NSQL" run -c Local --no-prompt "$W/mk.sql" >/dev/null 2>&1
}
val() { printf 'SELECT v FROM t_h WHERE id = %s;\n' "$1" > "$WK/q.sql"; "$V1_NSQL" run -c Local --no-prompt -f csv "$W/q.sql" 2>&1 | tail -1 | tr -d '\r"'; }
fin() { rm -rf "$V1_HOME"; V1_HOME=""; }
# <태그> <sql 본문> <실행 뒤 명령(쉼표 · 없으면 빈 글)> [추가 설정] — 실행 2.5 s 뒤 덤프 1 · 그 명령 · 3 s 뒤 덤프 2 · 롤백하고 종료
scene() { local tag=$1 sql=$2 mid=$3; shift 3
  mk "$@"; printf '%b' "$sql" > "$WK/$tag.sql"
  local c="@connected:open:$W/$tag.sql,@connected:@after:1500:run.all,@connected:@after:4000:tx.dump:$O/tx-$tag-1.txt"
  [ -n "$mid" ] && c="$c,@connected:@after:4300:$mid"
  c="$c,@connected:@after:7000:tx.dump:$O/tx-$tag-2.txt,@connected:@after:7300:run.rollback,@connected:@after:8000:file.exit,@connected:@after:8600:toolbar.pick:tx.rollback_then"
  v1_gui "tx-$tag" "$c" 25 Local; fin
}
scene s1 "UPDATE t_h SET v = 'x1' WHERE id = 1;\n" ""
scene s2 "UPDATE t_h SET v = 'x1' WHERE id = 1;\nUPDATE t_h SET v = 'x2' WHERE id = 2;\n" ""
scene s3 "CREATE TABLE t_new (a INTEGER);\n" ""
scene s4 "UPDATE t_h SET v = 'x9' WHERE id = 99;\n" ""
scene s5 "UPDATE t_h SET v = 'x1' WHERE id = 1;\n" "run.commit"
scene s6 "UPDATE t_h SET v = 'x1' WHERE id = 1;\n" "run.rollback"
scene s7 "UPDATE t_h SET v = 'x1' WHERE id = 1;\n" "" "session.autocommit=on"
for s in s1:1/1:1/1 s2:2/2:2/2 s3:1/1:1/1 s4:0/0:0/0 s5:1/1:0/0 s6:1/1:0/0 s7:0/0:0/0; do
  IFS=: read -r t a b <<< "$s"
  expect_eq "$t-pending-txlog-1" "$(pend "$V1_OUT/tx-$t-1.txt")" "$a"
  expect_eq "$t-pending-txlog-2" "$(pend "$V1_OUT/tx-$t-2.txt")" "$b"
done
# s8 결과 그리드 2행 편집 → row.save = 행마다 1건(D-500) → 2/2
mk; printf 'SELECT id, v FROM t_h ORDER BY id;\n' > "$WK/s8.sql"
v1_gui tx-s8 "@connected:open:$W/s8.sql,@connected:@after:1500:run.all,@connected:@after:4500:grid.edit.set:0;1;g1,@connected:@after:5000:grid.edit.set:1;1;g2,@connected:@after:5500:grid.edit.cmd:row.save,@connected:@after:9500:tx.dump:$O/tx-s8-1.txt,@connected:@after:12500:tx.dump:$O/tx-s8-2.txt,@connected:@after:12800:run.rollback,@connected:@after:13500:file.exit,@connected:@after:14100:toolbar.pick:tx.rollback_then" 30 Local
expect_eq "s8-grid-2rows-1" "$(pend "$V1_OUT/tx-s8-1.txt")" "2/2"
expect_eq "s8-grid-2rows-2" "$(pend "$V1_OUT/tx-s8-2.txt")" "2/2"
fin
# s9 미커밋 1건에서 종료 → 확인 팝업(Commit and exit / Rollback and exit / Cancel) · 상태줄 안내 → 롤백하고 종료 → 서버 값 원래대로
mk; printf "UPDATE t_h SET v = 'x1' WHERE id = 1;\n" > "$WK/s9.sql"
v1_gui tx-s9 "@connected:open:$W/s9.sql,@connected:@after:1500:run.all,@connected:@after:4000:file.exit,@connected:@after:4500:toolbar.menudump:$O/tx-s9-m.txt,@connected:@after:4600:status.dump:$O/tx-s9-st.txt,@connected:@after:4800:toolbar.pick:tx.rollback_then" 20 Local
expect "s9-exit-guard-commit" "$V1_OUT/tx-s9-m.txt" "tx.commit_then"
expect "s9-exit-guard-rollback" "$V1_OUT/tx-s9-m.txt" "tx.rollback_then"
expect "s9-exit-guard-status" "$V1_OUT/tx-s9-st.txt" "1 uncommitted statement(s)"
expect_eq "s9-exited" "$(cat "$V1_OUT/tx-s9.exit" 2>/dev/null)" "0"
expect_eq "s9-server-rolled-back" "$(val 1)" "o1"
fin
# s10 미커밋 1건에서 탭 닫기 → 확인 팝업 · 상태줄 → 롤백하고 닫기 → 대기 0
mk; printf "UPDATE t_h SET v = 'x1' WHERE id = 1;\n" > "$WK/s10.sql"
v1_gui tx-s10 "@connected:open:$W/s10.sql,@connected:@after:1500:run.all,@connected:@after:4000:file.close_tab,@connected:@after:4500:toolbar.menudump:$O/tx-s10-m.txt,@connected:@after:4600:status.dump:$O/tx-s10-st.txt,@connected:@after:4800:toolbar.pick:tx.rollback_then,@connected:@after:6500:tx.dump:$O/tx-s10-2.txt,@connected:@after:7000:file.exit,@connected:@after:7600:toolbar.pick:tx.rollback_then" 20 Local
expect "s10-close-guard-rollback" "$V1_OUT/tx-s10-m.txt" "tx.rollback_then"
expect "s10-close-guard-status" "$V1_OUT/tx-s10-st.txt" "1 uncommitted statement(s)"
expect_eq "s10-after-rollback-0" "$(pend "$V1_OUT/tx-s10-2.txt")" "0/0"
fin
for t in s1 s2 s3 s4 s5 s6 s7 s8 s9 s10; do expect_not "$t-no-panic" "$V1_OUT/tx-$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
