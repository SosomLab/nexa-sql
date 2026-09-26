#!/usr/bin/env bash
# linux-mem-reclaim.sh — 성능 C-2 **메모리 회수 시험**(docs/71 §C-2 R1~R6 · win-mem-reclaim.ps1 이식 · 사용자 09-26).
#   E(누수 주기)가 *되풀이의 기울기*를 본다면 여기는 **놓은 뒤 기준선으로 돌아오는가**를 본다.
#   한 프로세스 안에서 ① 기준선 ② 올림 ③ 놓음 ④ 회수(유휴 memtrim 뒤) 네 시점의 RSS/RssAnon을 잰다.
# 사용: scripts/linux-mem-reclaim.sh -H <격리홈> -D <자료> -o <출력> [-p Local]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; H=""; D=""; OUT=""; PROF=Local
while getopts "H:D:o:p:" o; do case $o in H) H=$OPTARG;; D) D=$OPTARG;; o) OUT=$OPTARG;; p) PROF=$OPTARG;; esac; done
[ -n "$H" ] && [ -n "$D" ] && [ -n "$OUT" ] || { echo "usage: -H <home> -D <data> -o <out> [-p profile]"; exit 2; }
mkdir -p "$H" "$D" "$OUT"; R="$OUT/mem-reclaim.txt"; : > "$R"
APP="$ROOT/target/release/nexa-sql"; NSQL="$ROOT/target/release/nsql"
say() { echo "$@" | tee -a "$R"; }
rss()  { awk '/^VmRSS:/{print int($2/1024)}'   /proc/$1/status 2>/dev/null; }
anon() { awk '/^RssAnon:/{print int($2/1024)}' /proc/$1/status 2>/dev/null; }

# 시험 파일
[ -f "$D/mid2m.sql" ] || python3 -c "
open('$D/mid2m.sql','w').write('SELECT a, b FROM t WHERE x = 1 AND y = 2 ORDER BY a;\n' * 40000)"
[ -f "$D/big65m.sql" ] || python3 -c "
open('$D/big65m.sql','w').write('SELECT col_a, col_b, col_c FROM some_table WHERE id > 100 GROUP BY col_a;\n' * 900000)"
[ -f "$D/rows100k.sql" ] || cat > "$D/rows100k.sql" <<'SQL'
WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<100000)
SELECT x AS id, 'name_'||x AS name, x*1.5 AS amount, 'status'||(x%7) AS status, 'memo text for row '||x AS memo FROM n;
SQL
for i in 1 2 3 4 5 6 7 8; do
  [ -f "$D/r$i.sql" ] || cat > "$D/r$i.sql" <<SQL
WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10000)
SELECT x AS id, 'n${i}_'||x AS name, x*1.5 AS amount FROM n;
SQL
done
NSQL_HOME="$H" "$NSQL" conn add "$PROF" "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
NSQL_HOME="$H" "$NSQL" config set grid.max_rows 100000 >/dev/null 2>&1

# case <id> <설명> <올림 명령(;구분)> <올림 안정초> <놓음 명령(;구분)> <놓음 안정초> <허용치MB>
#   타임라인은 **앱 시작 기준 절대 시각**으로 맞춘다(앞선 판은 sleep 누적이 어긋나 기준선을 올림 뒤에 쟀다 — 09-26 수정):
#     t=BASE(4 s) 기준선 측정 → t=BASE+1.5 s부터 올림 명령 → +up_s 올림 측정 → 놓음 명령 → +down_s 놓음 측정 → +12 s 회수 측정
BASE_MS=4000
case_run() {
  local id=$1 desc=$2 up=$3 up_s=$4 down=$5 down_s=$6 limit=$7
  # 명령 타임라인(ms)
  local t=$((BASE_MS + 1500)) cmds="" c
  local IFS_SAVE=$IFS; IFS=';'
  for c in $up;   do cmds="$cmds,@after:$t:$c"; t=$((t+1400)); done
  local up_at=$((t + up_s*1000))
  t=$up_at
  for c in $down; do cmds="$cmds,@after:$t:$c"; t=$((t+1400)); done
  local down_at=$((t + down_s*1000))
  IFS=$IFS_SAVE
  local start=$(date +%s%N)
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="${cmds#,}" "$APP" "$PROF" >/dev/null 2>"$OUT/$id.stderr" &
  local p=$!
  at() { local target=$1 now d; now=$(( ($(date +%s%N) - start) / 1000000 )); d=$(( target - now ));
         [ "$d" -gt 0 ] && sleep "$(awk "BEGIN{printf \"%.3f\", $d/1000}")"; }
  at $BASE_MS;              local b_rss=$(rss $p) b_anon=$(anon $p)
  at $up_at;                local u_rss=$(rss $p) u_anon=$(anon $p)
  at $down_at;              local d_rss=$(rss $p) d_anon=$(anon $p)
  at $((down_at + 12000));  local f_rss=$(rss $p) f_anon=$(anon $p)
  kill $p 2>/dev/null; wait $p 2>/dev/null
  [ -z "$b_anon" ] && { say "$(printf '%-4s %-38s (측정 실패 — 프로세스 조기 종료)' "$id" "$desc")"; return; }
  local back=$((f_anon - b_anon)) grew=$((u_anon - b_anon))
  local mark="OK"; [ "$back" -gt "$limit" ] && mark="OVER(+${back}MB > ${limit}MB)"
  [ "$grew" -le 0 ] && mark="$mark ⚠올림이 기준선보다 크지 않음(시나리오 확인)"
  say "$(printf '%-4s %-34s 기준 %4s → 올림 %4s(+%d) → 놓음 %4s → 회수 %4s MB anon | RSS %4s→%4s→%4s→%4s | 잔여 %+d MB %s' \
        "$id" "$desc" "$b_anon" "$u_anon" "$grew" "$d_anon" "$f_anon" "$b_rss" "$u_rss" "$d_rss" "$f_rss" "$back" "$mark")"
}

say "== linux-mem-reclaim $(date '+%F %T')  commit=$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null)"
say "   기준선 = 기동+접속 · 허용치 = docs/71 §C-2"
say ""
case_run R1 "편집기 탭 2 MB → 탭 닫기"            "open:$D/mid2m.sql"   6 "file.close_tab"   5 2
case_run R2 "결과 10만 행 → 결과 탭 닫기"          "open:$D/rows100k.sql;run.all"   9 "result.tab.close"   5 3
case_run R3 "대용량 65 MB → 탭 닫기"               "open:$D/big65m.sql;bigfile.open"   14 "file.close_tab"   6 4
case_run R4 "결과 탭 8개 → 탭째 전부 닫기"              "open:$D/r1.sql;run.all;open:$D/r2.sql;run.all;open:$D/r3.sql;run.all;open:$D/r4.sql;run.all;open:$D/r5.sql;run.all;open:$D/r6.sql;run.all;open:$D/r7.sql;run.all;open:$D/r8.sql;run.all" 10 "file.close_tab;file.close_tab;file.close_tab;file.close_tab;file.close_tab;file.close_tab;file.close_tab;file.close_tab" 8 4
case_run R6 "편집기+결과 번갈아 → 전부 닫기"       "open:$D/mid2m.sql;open:$D/rows100k.sql;run.all;open:$D/mid2m.sql" 8 "result.tab.close;file.close_tab;file.close_tab;file.close_tab" 8 4
say ""
say "   (R5 커서 결과는 Oracle/PG 실서버 필요 — 별도 실행)"
say "== done $(date '+%T')"
