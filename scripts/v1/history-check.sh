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
fresh "history.server_check_secs=1" "ui.toast_ms=60000"
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

# ── 독립 검토 후속(11-23 묶음 17) ──
#   r1·r2 자기 컴파일 = 변경 아님: 파일 탭에서 뷰를 지운 뒤 오브젝트 탭에 새 정의를 넣고 F5(run.all) → 서버 본 = 버퍼라 `server` 시점이
#         같은 해시로 생략되는 길 → 확인 = "unchanged"(종전 = 기준이 옛 open으로 떨어져 "changed" · 확인마다 다시) · 두 번 · external 0
#   r3 수동 확인 두 번을 간격 없이 → 둘째는 "still in progress"(대기 중이면 건너뜀) 또는 정상 처리 · 탭 수 불변(중복 탭 0)
#   r4 뷰를 지운 뒤 확인(응답 = 소스 읽기 실패) → 탐색기에서 다른 뷰 v2를 열면 탭이 열린다(남은 요청이 응답을 삼키지 않음)
printf 'DROP VIEW v1;\n' > "$WK/drop.sql"; printf 'CREATE VIEW v1 AS SELECT 2 AS b;' > "$WK/new.sql"
OPENR="${C}open:$W/drop.sql,${C}@after:1500:explorer.expand:main,${C}@after:2500:explorer.expand:Views,${C}@after:3300:explorer.select:v1,${C}@after:3600:explorer.pick:source"
rm -f "$V1_OUT"/hr-*.txt
fresh
v1_gui hr1 "$OPENR,${C}@after:6000:tab.prev,${C}@after:6300:run.all,${C}@after:8500:tab.next,${C}@after:8800:edit.select_all,${C}@after:9000:editor.paste:$W/new.sql,${C}@after:9400:run.all,${C}@after:13000:edit.history_check,${C}@after:15500:status.dump:$O/hr-1.txt,${C}@after:15800:edit.history_check,${C}@after:18300:status.dump:$O/hr-2.txt,${C}@after:18600:tabs.dump:$O/hr-3a.txt,${C}@after:18900:edit.history_check,${C}@after:18950:edit.history_check,${C}@after:19100:status.dump:$O/hr-3.txt,${C}@after:21500:tabs.dump:$O/hr-3b.txt,${C}@after:22000:file.exit,${C}@after:22500:toolbar.pick:project.exit_skip" 40 lite
expect "r1-own-compile-unchanged" "$V1_OUT/hr-1.txt" "main.v1: server source unchanged since"
expect "r2-again-unchanged" "$V1_OUT/hr-2.txt" "main.v1: server source unchanged since"
expect_eq "r1-r2-no-external" "$(ext)" "0"
if grep -qE 'still in progress|unchanged since' "$V1_OUT/hr-3.txt" 2>/dev/null; then v1_pass "r3-second-check-skipped-or-ok"; else v1_failmsg "r3-second-check-skipped-or-ok" "$(head -c 200 "$V1_OUT/hr-3.txt" 2>/dev/null)"; fi
expect_eq "r3-no-duplicate-tab" "$(grep -c '' "$V1_OUT/hr-3b.txt" 2>/dev/null)" "$(grep -c '' "$V1_OUT/hr-3a.txt" 2>/dev/null)"
fresh
v1_cli run -c lite --no-prompt "$(printf 'DROP VIEW IF EXISTS v2;\nCREATE VIEW v2 AS SELECT 9 AS z;\n' > "$WK/v2.sql"; v1_m "$WK/v2.sql")" >/dev/null
v1_gui hr4 "$OPENR,${C}@after:6000:tab.prev,${C}@after:6300:run.all,${C}@after:8500:tab.next,${C}@after:8800:edit.history_check,${C}@after:11500:status.dump:$O/hr-4a.txt,${C}@after:11800:explorer.select:v2,${C}@after:12100:explorer.pick:source,${C}@after:15000:tabs.dump:$O/hr-4.txt,${C}@after:15500:file.exit,${C}@after:16000:toolbar.pick:project.exit_skip" 30 lite
expect_not "r4-check-not-unchanged" "$V1_OUT/hr-4a.txt" "unchanged since"
expect "r4-other-view-opens" "$V1_OUT/hr-4.txt" "v2.sql"
for t in hr1 hr4; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done

# ── 셋째 독립 검토 후속(11-23 묶음 20) ──
#   r5 A: 컴파일 성공 뒤 서버 본 = 직전 버퍼 시점이면 그 시점 파일이 `server`로 다시 표시된다(같은 해시의 `compile` 파일이 남지 않음)
#         → 그 위에 실패한 컴파일 시점이 하나 더 쌓여도 확인 = "unchanged"(종전 = 기준이 옛 open으로 떨어져 "changed") · external 0
#   r6 F(오프라인이면 대기를 세우지 않음) = 미판정(아래 이유)
#   C(컴파일 뒤 저장 요청이 대기 중 확인을 덮음) = 미판정 — 두 요청을 같은 순간에 겹치게 할 기동 명령 타이밍을 잡을 수 없음(단위 시험이 덮음)
printf 'CREATE VIEW v1 AS SELEC 2 AS b;' > "$WK/bad.sql"
fresh
v1_gui hr5 "$OPENR,${C}@after:6000:tab.prev,${C}@after:6300:run.all,${C}@after:8500:tab.next,${C}@after:8800:edit.select_all,${C}@after:9000:editor.paste:$W/new.sql,${C}@after:9400:run.all,${C}@after:13000:edit.select_all,${C}@after:13200:editor.paste:$W/bad.sql,${C}@after:13600:run.all,${C}@after:16000:edit.history_check,${C}@after:18500:status.dump:$O/hr-5.txt,${C}@after:19000:file.exit,${C}@after:19500:toolbar.pick:project.exit_skip" 35 lite
SRV=$(find "$V1_HOME/history" -name '*_server_*.sql' 2>/dev/null | head -1); SH=$(basename "$SRV" .sql 2>/dev/null | sed 's/.*_server_//')
echo "r5 history: $(find "$V1_HOME/history" -name '*.sql' 2>/dev/null | xargs -n1 basename 2>/dev/null | sed 's/^[0-9]*_//' | tr '\n' ' ')"
expect_eq "r5-server-snapshot-exists" "$([ -n "$SRV" ] && echo y)" "y"
expect_eq "r5-no-compile-with-server-hash" "$(find "$V1_HOME/history" -name "*_compile_${SH}.sql" 2>/dev/null | wc -l | tr -d ' ')" "0"
expect "r5-server-has-new-body" "$SRV" "SELECT 2 AS b"
expect "r5-unchanged-after-failed-compile" "$V1_OUT/hr-5.txt" "main.v1: server source unchanged since"
expect_eq "r5-no-external" "$(ext)" "0"
# r6(F) = 미판정 — 툴바 `conn.disconnect`는 편집기 탭 세션만 끊고 오브젝트 소스를 읽는 탐색기 연결은 남아(11-23 첫 판 = 끊은 뒤에도 "unchanged") 오프라인
#   조건을 기동 명령으로 만들 수 없다 · 단위 시험(`source_ready_for`)이 덮는다.
# r7(넷째 독립 검토 후속 · 11-23 묶음 21): 확인 직후 그 탭을 닫고 같은 객체를 탐색기에서 다시 열면 탭이 열린다
#    (대기가 일반 "소스 열기" 응답을 삼키지 않음 · 대기 칸 하나로 가로채는 방식의 약점 = 재설계 TODO · 이 판정은 지금 고친 범위만)
fresh
v1_gui hr7 "$OPEN,${C}@after:6000:edit.history_check,${C}@after:6100:file.close_tab,${C}@after:6400:tabs.dump:$O/hr-7a.txt,${C}@after:6600:explorer.select:v1,${C}@after:6800:explorer.pick:source,${C}@after:10000:tabs.dump:$O/hr-7b.txt,${C}@after:10300:file.exit,${C}@after:10800:toolbar.pick:project.exit_skip" 25 lite
expect_not "r7-tab-closed-first" "$V1_OUT/hr-7a.txt" "v1.sql"
expect "r7-reopened-tab" "$V1_OUT/hr-7b.txt" "v1.sql"
expect_eq "r7-one-v1-tab" "$(grep -c 'v1\.sql' "$V1_OUT/hr-7b.txt" 2>/dev/null)" "1"
for t in hr5 hr7; do expect_not "$t-no-panic" "$V1_OUT/$t.stderr" "panicked"; done
rm -rf "$WK"
v1_done
