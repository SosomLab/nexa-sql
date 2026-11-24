#!/usr/bin/env bash
# V1 vars-saveas — 다른 이름으로 저장 = 변수 표를 새 경로 기록으로 복사(D-195 · 74 §4-4 · 11-25).
#   헤드리스 · 격리 홈(SQLite Local) · 입력 주입 0 · 임시 파일만.
#   v1 a.sql에서 변수 v를 만들고 실행 → 격리 홈 vars/ 에 기록 1개(a.sql 것 · 값 abc)
#   v2 `file.save_as:b.sql` → 기록 2개(옛 경로 것은 그대로 = 복사) · 새 기록에도 abc      (종전 = 1개 그대로 — 다음 실행 때에야 생김)
#   v3 `vars.persist=off`면 기록을 만들지 않는다(0개)
. "$(dirname "$0")/../lib/v1.sh"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1vs-XXXX"); W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
rm -f "$V1_OUT"/vs-*.txt
mk() { v1_home en; [ -n "$1" ] && v1_set "$1"
  "$V1_NSQL" conn add Local "sqlite:$(v1_m "$V1_HOME")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1; }
nrec() { ls "$V1_HOME/vars" 2>/dev/null | grep -c '\.sql$'; }
has() { grep -lF -- "$1" "$V1_HOME"/vars/*.sql 2>/dev/null | wc -l | tr -d ' '; }
eqn() { if [ "$2" = "$3" ]; then v1_pass "$1"; else v1_failmsg "$1" "기대 $3 · 실제 $2 · $(ls "$V1_HOME/vars" 2>/dev/null | tr '\n' ' ')"; fi; }
printf "VARIABLE v VARCHAR2(10)\nEXEC :v := 'abc'\nSELECT :v AS x;\n" > "$WK/a.sql"
# v1 — 실행만
mk ""; v1_gui vs1 "@connected:open:$W/a.sql,@connected:@after:1500:run.all,@connected:@after:4500:file.exit" 15 Local
eqn "v1-one-record" "$(nrec)" 1
eqn "v1-record-has-value" "$(has abc)" 1
rm -rf "$V1_HOME"; V1_HOME=""
# v2 — 실행 뒤 다른 이름으로 저장
rm -f "$WK/b.sql"
mk ""; v1_gui vs2 "@connected:open:$W/a.sql,@connected:@after:1500:run.all,@connected:@after:4500:file.save_as:$W/b.sql,@connected:@after:5500:tabs.dump:$O/vs-2t.txt,@connected:@after:6000:file.exit" 15 Local
eqn "v2-two-records" "$(nrec)" 2
eqn "v2-both-have-value" "$(has abc)" 2
expect "v2-tab-is-b" "$V1_OUT/vs-2t.txt" "b.sql"
if [ -f "$WK/b.sql" ]; then v1_pass "v2-file-written"; else v1_failmsg "v2-file-written" "b.sql 없음"; fi
rm -rf "$V1_HOME"; V1_HOME=""
# v3 — 보존 끔
rm -f "$WK/b.sql"
mk "vars.persist=off"; v1_gui vs3 "@connected:open:$W/a.sql,@connected:@after:1500:run.all,@connected:@after:4500:file.save_as:$W/b.sql,@connected:@after:6000:file.exit" 15 Local
eqn "v3-persist-off-no-record" "$(nrec)" 0
rm -rf "$V1_HOME"; V1_HOME=""
for t in vs1 vs2 vs3; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
