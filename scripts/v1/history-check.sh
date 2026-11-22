#!/usr/bin/env bash
# V1 history-check — (O-1 · 서버 변경 확인 `edit.history_check` · docs/19 §3): 헤드리스 · 격리 홈 · SQLite 임시 DB · 입력 주입 0.
#   한 GUI 실행 안에서: 파일 탭 c.sql(뷰를 다른 본문으로 다시 만드는 DDL) + 탐색기에서 뷰 v1을 오브젝트 탭으로 연다.
#   ① 오브젝트 탭에서 확인 → "unchanged since" · external 0
#   ② c.sql 탭으로 가서 run.all(DROP + CREATE · 자동 커밋 켬) → 오브젝트 탭으로 돌아와 확인 → "changed since" · external 1
#   ③ 한 번 더 확인 → "unchanged since"(같은 변경을 두 번 알리지 않음) · external 그대로 1
#   ④ 파일 탭에서 확인 → "works on an object source tab"
#   ⑤ panic 없음
. "$(dirname "$0")/../lib/v1.sh"
v1_home en
v1_set "session.autocommit=on"
WK=$(mktemp -d "${TEMP:-/tmp}/nsql-v1hc-XXXX")
v1_cli conn add lite "sqlite:$(v1_m "$WK")/d.sqlite" -d sqlite --no-prompt >/dev/null
printf 'CREATE TABLE t (a INTEGER);\nCREATE VIEW v1 AS SELECT 1 AS a;\n' > "$WK/mk.sql"
v1_cli run -c lite --no-prompt "$(v1_m "$WK/mk.sql")" >/dev/null
printf 'DROP VIEW v1;\nCREATE VIEW v1 AS SELECT 2 AS b;\n' > "$WK/c.sql"
W=$(v1_m "$WK"); O=$(v1_m "$V1_OUT")
ext() { find "$V1_HOME/history" -name '*_external_*.sql' 2>/dev/null | wc -l | tr -d ' '; }
rm -f "$V1_OUT"/hc-*.txt
C="@connected:"
v1_gui hc "${C}open:$W/c.sql,${C}@after:1500:explorer.expand:main,${C}@after:2500:explorer.expand:Views,${C}@after:3300:explorer.select:v1,${C}@after:3600:explorer.pick:source,${C}@after:6000:tabs.dump:$O/hc-tabs.txt,${C}@after:6300:edit.history_check,${C}@after:8500:status.dump:$O/hc-1.txt,${C}@after:8800:tab.prev,${C}@after:9100:run.all,${C}@after:11500:tab.next,${C}@after:11800:edit.history_check,${C}@after:14000:status.dump:$O/hc-2.txt,${C}@after:14300:log.dump:$O/hc-2log.txt,${C}@after:14600:edit.history_check,${C}@after:16800:status.dump:$O/hc-3.txt,${C}@after:17100:tab.prev,${C}@after:17400:edit.history_check,${C}@after:17800:status.dump:$O/hc-4.txt,${C}@after:18200:file.exit,${C}@after:18700:toolbar.pick:project.exit_skip" 35 lite
N1=$(find "$V1_HOME/history" -name '*.sql' 2>/dev/null | wc -l | tr -d ' ')
expect "0-object-tab-opened" "$V1_OUT/hc-tabs.txt" "v1.sql"
expect "1-unchanged" "$V1_OUT/hc-1.txt" "main.v1: server source unchanged since"
expect "2-changed" "$V1_OUT/hc-2.txt" "main.v1: server source changed since"
expect "2-log-line" "$V1_OUT/hc-2log.txt" "server source changed since"
expect_eq "2-3-one-external" "$(ext)" "1"
expect "3-unchanged-again" "$V1_OUT/hc-3.txt" "main.v1: server source unchanged since"
expect "4-file-tab-guard" "$V1_OUT/hc-4.txt" "works on an object source tab"
EXT=$(find "$V1_HOME/history" -name '*_external_*.sql' 2>/dev/null | head -1)
expect "2-external-has-new-body" "$EXT" "SELECT 2 AS b"
expect_not "5-no-panic" "$V1_OUT/hc.stderr" "panicked"
echo "history files: $N1 · external: $(ext)"

# ── 자동 확인(D-499 · `history.server_check_secs` · 오브젝트 탭이 활성화된 순간 + 간격이 지났을 때만 · 기본 0 = 끔) ──
#   a1. 간격 1초 · 변경 없음 왕복(파일 탭 → 1.5초 뒤 오브젝트 탭) → 상태줄에 "unchanged" 없음(자동은 조용히) · external 0
#   a2. 같은 홈 · 파일 탭에서 뷰를 다시 만든 뒤 오브젝트 탭으로 → "changed since" + external 1(수동과 같은 안내)
#   a0. 대조 = 설정 없음(기본 0) · 같은 변경 왕복 → external 0 · 로그에 "server source changed" 없음
fresh() { # [설정…] — 새 격리 홈 + 같은 DB 파일을 처음 뷰(SELECT 1 AS a)로 되돌림
  [ -n "$V1_HOME" ] && rm -rf "$V1_HOME"; v1_home en; v1_set "session.autocommit=on"; for kv in "$@"; do v1_set "$kv"; done
  v1_cli conn add lite "sqlite:$(v1_m "$WK")/d.sqlite" -d sqlite --no-prompt >/dev/null
  printf 'DROP VIEW IF EXISTS v1;\nCREATE VIEW v1 AS SELECT 1 AS a;\n' > "$WK/reset.sql"
  v1_cli run -c lite --no-prompt "$(v1_m "$WK/reset.sql")" >/dev/null
}
OPEN="${C}open:$W/c.sql,${C}@after:1500:explorer.expand:main,${C}@after:2500:explorer.expand:Views,${C}@after:3300:explorer.select:v1,${C}@after:3600:explorer.pick:source"
rm -f "$V1_OUT"/ha-*.txt
fresh "history.server_check_secs=1"
v1_gui ha1 "$OPEN,${C}@after:6000:tab.prev,${C}@after:7500:tab.next,${C}@after:10000:status.dump:$O/ha-1.txt,${C}@after:10300:log.dump:$O/ha-1log.txt,${C}@after:10600:file.exit,${C}@after:11100:toolbar.pick:project.exit_skip" 30 lite
expect_eq "a1-dump-ran" "$([ -f "$V1_OUT/ha-1.txt" ] && [ -f "$V1_OUT/ha-1log.txt" ] && echo y)" "y"
expect_not "a1-auto-quiet-when-same" "$V1_OUT/ha-1.txt" "unchanged since"
expect_not "a1-no-change-notice" "$V1_OUT/ha-1log.txt" "server source changed since"
expect_eq "a1-no-external" "$(ext)" "0"
v1_gui ha2 "$OPEN,${C}@after:6000:tab.prev,${C}@after:6300:run.all,${C}@after:9000:tab.next,${C}@after:11500:status.dump:$O/ha-2.txt,${C}@after:11800:log.dump:$O/ha-2log.txt,${C}@after:12000:toast.dump:$O/ha-2toast.txt,${C}@after:12300:toast.click:0,${C}@after:13800:tabs.dump:$O/ha-2tabs.txt,${C}@after:14200:file.exit,${C}@after:14700:toolbar.pick:project.exit_skip" 30 lite
expect "a2-auto-changed" "$V1_OUT/ha-2.txt" "main.v1: server source changed since"
expect "a2-log-line" "$V1_OUT/ha-2log.txt" "server source changed since"
expect_eq "a2-one-external" "$(ext)" "1"
expect "a2-toast-object" "$V1_OUT/ha-2toast.txt" "main.v1"
expect "a2-toast-click-hint" "$V1_OUT/ha-2toast.txt" "Click to compare"
expect "a2-toast-opens-compare" "$V1_OUT/ha-2tabs.txt" "⇄"
fresh
v1_gui ha0 "$OPEN,${C}@after:6000:tab.prev,${C}@after:6300:run.all,${C}@after:9000:tab.next,${C}@after:11500:status.dump:$O/ha-0.txt,${C}@after:11800:log.dump:$O/ha-0log.txt,${C}@after:12100:file.exit,${C}@after:12600:toolbar.pick:project.exit_skip" 30 lite
expect_eq "a0-dump-ran" "$([ -f "$V1_OUT/ha-0log.txt" ] && echo y)" "y"
expect_not "a0-off-no-notice" "$V1_OUT/ha-0log.txt" "server source changed since"
expect_eq "a0-off-no-external" "$(ext)" "0"
for t in ha1 ha2 ha0; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
