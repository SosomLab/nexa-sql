#!/usr/bin/env bash
# win-stop-latency-e2e.sh — 실행 중지(■) 지연 측정 E2E(10-09 · 사용자 "프로시저 실행 중 중지가 5~10초 뒤에야 걸린다" · docs/44 §6).
#   저장된 Oracle 연결(기본 BISCM)로 오래 도는 문장 셋을 돌리고, 기동 명령으로 N초 뒤 ■(`run.stop`)를 눌러 **중지 요청 → 중지 확정**
#   사이의 시간을 로그 시각(`log.dump_t`)으로 잰다 · 키 주입 0 · 영속 객체 0(블록·조회뿐).
#   ① `DBMS_SESSION.SLEEP`(18c+ · 없으면 `DBMS_LOCK.SLEEP`) = 대기 중 취소 ② PL/SQL CPU 루프 = 루프 안 취소 ③ 큰 조인 SELECT = SQL 실행 중 취소
#   판정선(권장): 각 ≤ 2000 ms. OOB(TCP urgent)가 막힌 망에서는 서버의 다음 읽기까지 지연될 수 있다(44 §6 Oracle 행).
# 사용: scripts/win-stop-latency-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-P <실제 설정 폴더>] [-O BISCM] [-w 3000(실행 뒤 ■까지 ms)] [-l 2000(판정선 ms)]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"; ORA="BISCM"; WAIT=3000; LIMIT=2000
while getopts "o:g:P:O:w:l:" o; do case $o in
  o) OUT=$OPTARG;; g) EXE=$OPTARG;; P) PROF=$OPTARG;; O) ORA=$OPTARG;; w) WAIT=$OPTARG;; l) LIMIT=$OPTARG;;
esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-P real-config-dir] [-O oracle-profile] [-w wait-ms] [-l limit-ms]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/stop-latency-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

# run_stop <이름> <본문> → 시각 덤프 경로. 접속 뒤 열기 → 3 s 뒤 run.all → WAIT ms 뒤 run.stop → 25 s 뒤 로그(시각) 덤프.
run_stop() {
  local name=$1 body=$2 f="$OUT/$1.sql" d="$OUT/$1.logt"
  printf '%s\n' "$body" > "$f"; rm -f "$d"
  local t_run=3000 t_stop=$((3000 + WAIT)) t_dump=$((3000 + WAIT + 25000))
  local cmd="@connected:open:$(w "$f"),@after:${t_run}:run.all,@after:${t_stop}:run.stop,@after:${t_dump}:log.dump_t:$(w "$d")"
  NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL $(( (t_dump + 8000) / 1000 )) "$EXE" "$ORA" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
  echo "$d"
}
# ms <hh:mm:ss.mmm> → 하루 기준 ms
ms() { awk -F'[:.]' '{ printf "%d", (($1*3600)+($2*60)+$3)*1000+$4 }' <<< "$1"; }
# latency <덤프> → "요청ms 확정ms 지연ms 확정문구" (없으면 빈 값)
latency() {
  local d=$1 req fin
  req=$(grep -m1 "Cancelling the running statement" "$d" | cut -d'|' -f1)
  fin=$(grep -m1 -E "Execution stopped by user|Stopped by user|ORA-01013" "$d" | cut -d'|' -f1)
  [ -n "$req" ] && [ -n "$fin" ] || { echo ""; return; }
  local a b; a=$(ms "$req"); b=$(ms "$fin")
  echo "$a $b $((b - a)) $(grep -m1 -E "Execution stopped by user|Stopped by user|ORA-01013" "$d" | cut -d'|' -f3- | cut -c1-80)"
}
judge() {
  local label=$1 d=$2 r
  r=$(latency "$d")
  if [ -z "$r" ]; then bad "$label 중지 요청/확정 줄을 못 찾음" "$(grep -E "Cancel|stop|ORA-|error" "$d" | head -8)"; return; fi
  set -- $r
  say "        요청 → 확정 = $3 ms ($4)"
  if [ "$3" -le "$LIMIT" ]; then ok "$label 중지 지연 $3 ms ≤ $LIMIT"; else bad "$label 중지 지연 $3 ms > $LIMIT" "$(grep -E "Cancel|stop|ORA-" "$d" | head -6)"; fi
}

say "=== 실행 중지 지연 E2E · $(date '+%F %T') · gui=$EXE · 프로필 $ORA · ■까지 ${WAIT} ms · 판정선 ${LIMIT} ms"

# ① 대기 중 취소(DBMS_SESSION.SLEEP · 60 s) — 권한/버전 폴백 DBMS_LOCK.SLEEP.
d=$(run_stop "s1_sleep" "BEGIN
  BEGIN DBMS_SESSION.SLEEP(60); EXCEPTION WHEN OTHERS THEN DBMS_LOCK.SLEEP(60); END;
END;
/")
judge "① DBMS_SESSION.SLEEP 60 s" "$d"

# ② PL/SQL CPU 루프(60 s · 서버 CPU 하나) — 루프 안에서 OCIBreak가 걸리는지.
d=$(run_stop "s2_loop" "DECLARE
  t DATE := SYSDATE; n NUMBER := 0;
BEGIN
  WHILE (SYSDATE - t) * 86400 < 60 LOOP n := n + 1; END LOOP;
END;
/")
judge "② PL/SQL CPU 루프 60 s" "$d"

# ③ SQL 실행 중 취소 — 큰 자체 조인 집계(ROWNUM 상한으로 끝은 있음 · 수 분).
d=$(run_stop "s3_join" "SELECT COUNT(*) FROM ALL_OBJECTS a, ALL_OBJECTS b WHERE ROWNUM <= 2000000000")
judge "③ 큰 조인 SELECT" "$d"

say "=== 결과: PASS $pass · FAIL $fail · 보고 = $REPORT"
[ "$fail" -eq 0 ]
