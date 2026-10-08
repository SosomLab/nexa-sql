#!/usr/bin/env bash
# win-stop-latency-e2e.sh — 실행 중지(■) 지연 측정 E2E(10-09 · 사용자 "프로시저 실행 중 중지가 5~10초 뒤에야 걸린다" · docs/44 §6).
#   저장된 연결(Oracle BISCM · SQL Server M4PLAN · PostgreSQL Repository)로 오래 도는 문장 셋을 돌리고, 기동 명령으로 N초 뒤 ■(`run.stop`)를
#   눌러 **중지 요청 → 중지 확정** 사이의 시간을 로그 시각(`log.dump_t`)으로 잰다 · 키 주입 0 · 영속 객체 0(블록·조회뿐).
#   DBMS마다 ① 대기(SLEEP/WAITFOR/pg_sleep) ② CPU 루프 ③ 큰 조인 SELECT. 요청 = "Cancelling the running statement…" · 확정 = 앱 판정
#   "Execution stopped by user"(모든 DBMS · MSSQL Attention은 오류 없이 끝남) 또는 드라이버 오류(ORA-01013 · PG 57014).
#   판정선(권장): 각 ≤ 2000 ms. OOB(TCP urgent)가 막힌 망에서는 Oracle break가 서버의 다음 읽기까지 지연될 수 있다(44 §6).
# 사용: scripts/win-stop-latency-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-P <실제 설정 폴더>]
#        [-O BISCM] [-m M4PLAN] [-r Repository] [-w 3000(실행 뒤 ■까지 ms)] [-l 2000(판정선 ms)]   (-O/-m/-r 빈 값 = 건너뜀)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
ORA="BISCM"; MS="M4PLAN"; PG="Repository"; WAIT=3000; LIMIT=2000
while getopts "o:g:P:O:m:r:w:l:" o; do case $o in
  o) OUT=$OPTARG;; g) EXE=$OPTARG;; P) PROF=$OPTARG;; O) ORA=$OPTARG;; m) MS=$OPTARG;; r) PG=$OPTARG;; w) WAIT=$OPTARG;; l) LIMIT=$OPTARG;;
esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-P real-config-dir] [-O oracle] [-m mssql] [-r pg] [-w wait-ms] [-l limit-ms]"; exit 2; }
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

# run_stop <이름> <프로필> <본문> → 시각 덤프 경로. 접속 뒤 열기 → 3 s 뒤 run.all → WAIT ms 뒤 run.stop → 25 s 뒤 로그(시각) 덤프.
run_stop() {
  local name=$1 profile=$2 body=$3 f="$OUT/$1.sql" d="$OUT/$1.logt"
  printf '%s\n' "$body" > "$f"; rm -f "$d"
  local t_run=3000 t_stop=$((3000 + WAIT)) t_dump=$((3000 + WAIT + 25000))
  local cmd="@connected:open:$(w "$f"),@after:${t_run}:run.all,@after:${t_stop}:run.stop,@after:${t_dump}:log.dump_t:$(w "$d")"
  NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL $(( (t_dump + 8000) / 1000 )) "$EXE" "$profile" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
  echo "$d"
}
# ms <hh:mm:ss.mmm> → 하루 기준 ms
ms() { awk -F'[:.]' '{ printf "%d", (($1*3600)+($2*60)+$3)*1000+$4 }' <<< "$1"; }
FIN_RE="Execution stopped by user|Stopped by user|ORA-01013|57014|canceling statement"
# latency <덤프> → "요청ms 확정ms 지연ms 확정문구" (없으면 빈 값)
latency() {
  local d=$1 req fin
  req=$(grep -m1 "Cancelling the running statement" "$d" | cut -d'|' -f1)
  fin=$(grep -m1 -E "$FIN_RE" "$d" | cut -d'|' -f1)
  [ -n "$req" ] && [ -n "$fin" ] || { echo ""; return; }
  local a b; a=$(ms "$req"); b=$(ms "$fin")
  echo "$a $b $((b - a)) $(grep -m1 -E "$FIN_RE" "$d" | cut -d'|' -f3- | cut -c1-90)"
}
judge() {
  local label=$1 d=$2 r
  r=$(latency "$d")
  if [ -z "$r" ]; then bad "$label 중지 요청/확정 줄을 못 찾음" "$(grep -E "Cancel|stop|ORA-|error|Error" "$d" | head -8)"; return; fi
  set -- $r
  say "        요청 → 확정 = $3 ms ($4)"
  if [ "$3" -le "$LIMIT" ]; then ok "$label 중지 지연 $3 ms ≤ $LIMIT"; else bad "$label 중지 지연 $3 ms > $LIMIT" "$(grep -E "Cancel|stop|ORA-|error" "$d" | head -6)"; fi
}

say "=== 실행 중지 지연 E2E · $(date '+%F %T') · gui=$EXE · ■까지 ${WAIT} ms · 판정선 ${LIMIT} ms"

# ── Oracle ─────────────────────────────────────────────────────────────────────
if [ -n "$ORA" ]; then
  say "— Oracle · 프로필 $ORA"
  # ① 대기 중 취소(DBMS_SESSION.SLEEP · 60 s · 18c+). 예외 처리기를 두지 않는다 — `WHEN OTHERS`는 ■의 ORA-01013까지 삼킨다(협업 10-09).
  d=$(run_stop "o1_sleep" "$ORA" "BEGIN
  DBMS_SESSION.SLEEP(60);
END;
/")
  judge "O① DBMS_SESSION.SLEEP 60 s" "$d"
  # ①-b 참고(판정 아님): `WHEN OTHERS`가 ORA-01013을 삼키고 다시 자는 꼴(실측 10-09 = 취소를 막지 않음 13 ms).
  d=$(run_stop "o1b_swallow" "$ORA" "BEGIN
  BEGIN DBMS_SESSION.SLEEP(60); EXCEPTION WHEN OTHERS THEN DBMS_SESSION.SLEEP(60); END;
END;
/")
  r=$(latency "$d"); if [ -n "$r" ]; then set -- $r; say "  INFO  O①-b WHEN OTHERS 삼킴 = 요청 → 확정 $3 ms ($4)"; else say "  INFO  O①-b WHEN OTHERS 삼킴 = 25 s 안에 확정 없음"; fi
  # ② PL/SQL CPU 루프(60 s · 서버 CPU 하나) — 루프 안에서 OCIBreak가 걸리는지.
  d=$(run_stop "o2_loop" "$ORA" "DECLARE
  t DATE := SYSDATE; n NUMBER := 0;
BEGIN
  WHILE (SYSDATE - t) * 86400 < 60 LOOP n := n + 1; END LOOP;
END;
/")
  judge "O② PL/SQL CPU 루프 60 s" "$d"
  # ③ SQL 실행 중 취소 — 큰 자체 조인 집계(ROWNUM 상한으로 끝은 있음 · 수 분).
  d=$(run_stop "o3_join" "$ORA" "SELECT COUNT(*) FROM ALL_OBJECTS a, ALL_OBJECTS b WHERE ROWNUM <= 2000000000")
  judge "O③ 큰 조인 SELECT" "$d"
fi

# ── SQL Server ─────────────────────────────────────────────────────────────────
#   취소 = TDS Attention(`mssql.encrypt=login`) 또는 접속 끊기(`required` · 44 §6) — 저장된 프로필 설정 그대로 잰다.
if [ -n "$MS" ]; then
  say "— SQL Server · 프로필 $MS"
  d=$(run_stop "m1_waitfor" "$MS" "WAITFOR DELAY '00:01:00';")
  judge "M① WAITFOR DELAY 60 s" "$d"
  d=$(run_stop "m2_loop" "$MS" "DECLARE @t DATETIME = GETDATE(), @i BIGINT = 0;
WHILE DATEDIFF(SECOND, @t, GETDATE()) < 60 SET @i = @i + 1;")
  judge "M② T-SQL CPU 루프 60 s" "$d"
  d=$(run_stop "m3_join" "$MS" "SELECT COUNT_BIG(*) FROM sys.all_objects a CROSS JOIN sys.all_objects b CROSS JOIN sys.all_objects c;")
  judge "M③ 큰 조인 SELECT" "$d"
fi

# ── PostgreSQL ─────────────────────────────────────────────────────────────────
#   취소 = CancelRequest(새 TCP 접속 · 별도 스레드 · 44 §6) → 57014.
if [ -n "$PG" ]; then
  say "— PostgreSQL · 프로필 $PG"
  d=$(run_stop "p1_sleep" "$PG" "SELECT pg_sleep(60);")
  judge "P① pg_sleep 60 s" "$d"
  d=$(run_stop "p2_loop" "$PG" "DO \$\$
DECLARE t timestamptz := clock_timestamp(); n bigint := 0;
BEGIN
  WHILE clock_timestamp() - t < interval '60 seconds' LOOP n := n + 1; END LOOP;
END \$\$;")
  judge "P② PL/pgSQL CPU 루프 60 s" "$d"
  d=$(run_stop "p3_join" "$PG" "SELECT COUNT(*) FROM generate_series(1, 100000) a, generate_series(1, 100000) b;")
  judge "P③ 큰 조인 SELECT" "$d"
fi

say "=== 결과: PASS $pass · FAIL $fail · 보고 = $REPORT"
[ "$fail" -eq 0 ]
