#!/usr/bin/env bash
# win-intel-args-e2e.sh — 전달인자 자동완성·EXEC 링크·OUT 재추론 E2E(10-08 · T-316/T-317 · docs/82 §7 · 63 §12-2 · 96 §10).
#   DBMS별 저장된 연결(사용자 10-08 "저장된 연결을 사용하면 돼")로 설계대로 동작하는지 — 키 주입 0(기동 명령 `editor.caret` ·
#   `intel.probe`(두 번 = 인자 채움 뒤) · `intel.dump` · `objlink.dump` · `run.all`) + CLI `nsql run`.
#   · Oracle(BISCM): `EXEC s.proc(` · `, ` 뒤 = `이름 => ` 후보 · 이미 적은 이름 제외 · CLI = `:RET` 두 프로시저 재사용(NUMBER → REF CURSOR OUT)에 PLS-00306 없음
#   · SQL Server(M4PLAN): `USE DB` 뒤 `EXEC proc ` = `@이름 = ` · 시스템 프로시저 `EXEC sys.sp_addextendedproperty ` = `@name = ` · 이미 쓴 인자 제외 · `EXEC proc …` 루틴 링크
#   · PostgreSQL(Repository): 임시 함수 `public.nsqlt_args(p_a, p_b)` 생성 → `SELECT nsqlt_args(` = `p_a => ` → 삭제(실서버 임시 객체 · 한 줄 고지)
# 사용: scripts/win-intel-args-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/release/nsql.exe] [-P <실제 설정 폴더>]
#        [-O BISCM] [-1 SP_TEST1] [-2 SP_TEST2] [-a RET,P_PROJECT_CD] [-m M4PLAN] [-D M4PLAN_MS] [-s PROC_TEST] [-A VS_PROJECT_CD] [-r Repository]
#   -O/-m/-r 에 빈 값("")을 주면 그 DBMS는 건너뛴다.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/release/nsql.exe"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
ORA="BISCM"; ORA_P1="SP_TEST1"; ORA_P2="SP_TEST2"; ORA_ARGS="RET,P_PROJECT_CD"
MS="M4PLAN"; MS_DB="M4PLAN_MS"; MS_PROC="PROC_TEST"; MS_ARG="VS_PROJECT_CD"
PG="Repository"
while getopts "o:g:n:P:O:1:2:a:m:D:s:A:r:" o; do case $o in
  o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; P) PROF=$OPTARG;;
  O) ORA=$OPTARG;; 1) ORA_P1=$OPTARG;; 2) ORA_P2=$OPTARG;; a) ORA_ARGS=$OPTARG;;
  m) MS=$OPTARG;; D) MS_DB=$OPTARG;; s) MS_PROC=$OPTARG;; A) MS_ARG=$OPTARG;; r) PG=$OPTARG;;
esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql] [-P real-config-dir] [-O oracle-profile] [-m mssql-profile] [-D mssql-db] [-r pg-profile]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/intel-args-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
skp() { say "  SKIP  $1"; skip=$((skip+1)); }
show() { echo "$1" | head -8 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

# probe <이름> <프로필> <본문> <캐럿> [run] → 후보 덤프 경로를 echo. 다섯째 인자가 "run"이면 **같은 탭**에서 run.all(예: 첫 줄 `USE DB` + GO)을
#   먼저 돌린 뒤 캐럿 자리에서 완성한다(탭별 작업 단위 ⑯ = 새 탭은 기본 DB라 같은 탭이어야 한다 · E2E 10-08).
probe() {
  local name=$1 profile=$2 body=$3 caret=$4 pre=${5:-} f="$OUT/$1.sql" d="$OUT/$1.cands"
  printf '%s' "$body" > "$f"; rm -f "$d"
  local cmd
  if [ -n "$pre" ]; then
    cmd="@connected:open:$(w "$f"),@after:6000:run.all,@after:10000:editor.caret:$caret,@after:10300:intel.probe,@after:14000:intel.probe,@after:15000:intel.dump:$d"
    NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 19 "$EXE" "$profile" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
  else
    cmd="@connected:open:$(w "$f"),@after:6000:editor.caret:$caret,@after:6300:intel.probe,@after:10500:intel.probe,@after:11500:intel.dump:$d"
    NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 15 "$EXE" "$profile" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
  fi
  echo "$d"
}
# links <이름> <프로필> <본문> [run] → objlink 덤프 경로를 echo. "run"이면 같은 탭에서 run.all 뒤(USE 뒤 메타 재분석) 덤프.
links() {
  local name=$1 profile=$2 body=$3 pre=${4:-} f="$OUT/$1.sql" d="$OUT/$1.links"
  printf '%s' "$body" > "$f"; rm -f "$d"
  local cmd
  if [ -n "$pre" ]; then
    cmd="@connected:open:$(w "$f"),@after:6000:run.all,@after:13000:objlink.dump:$d"
    NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 17 "$EXE" "$profile" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
  else
    cmd="@connected:open:$(w "$f"),@after:9000:objlink.dump:$d"
    NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 13 "$EXE" "$profile" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
  fi
  echo "$d"
}
has()  { echo "$2" | grep -qiE -- "$1"; }

say "=== 전달인자 완성·EXEC 링크·OUT 재추론 E2E · $(date '+%F %T') · gui=$EXE · nsql=$NSQL"

# ── Oracle ─────────────────────────────────────────────────────────────────────
if [ -n "$ORA" ]; then
  say "— Oracle · 프로필 $ORA · $ORA_P1 · $ORA_P2 · 인자 $ORA_ARGS"
  IFS=',' read -r A1 A2 <<< "$ORA_ARGS"
  d=$(probe "o1_named" "$ORA" "EXEC $ORA.$ORA_P2(" end)
  if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
    has "^Variable\|$A1 => \|" "$c" && ok "O① \`EXEC $ORA.$ORA_P2(\` = \`$A1 => \`" || bad "O① \`$A1 => \` 없음" "$c"
    has "^Variable\|$A2 => \|" "$c" && ok "O① \`$A2 => \`도" || bad "O① \`$A2 => \` 없음" "$c"
  else bad "O① 후보 덤프 없음" "$(tail -3 "$OUT/o1_named.stderr")"; fi
  d=$(probe "o2_second" "$ORA" "EXEC $ORA.$ORA_P2($A1 => :R, " end)
  if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
    has "^Variable\|$A2 => \|" "$c" && ok "O② \`,\` 뒤 = \`$A2 => \`(둘째 인자)" || bad "O② 둘째 인자 후보 없음" "$c"
    has "^Variable\|$A1" "$c" && bad "O② 이미 적은 \`$A1\`이 남음" "$c" || ok "O② 이미 적은 \`$A1\` 제외"
    has "^Variable\|@" "$c" && bad "O② Oracle에 SQL Server 꼴(\`@이름 = \`)" "$c" || ok "O② \`@이름 = \` 꼴 없음(방언 맞음)"
  else bad "O② 후보 덤프 없음" "$(tail -3 "$OUT/o2_second.stderr")"; fi
  # O③ CLI: 자동 변수 :RET = 첫 프로시저(NUMBER OUT) → 둘째(REF CURSOR OUT)에 재사용 = 서명 타입으로 다시 맞춰 PLS-00306 없음.
  F3="$OUT/o3_out.sql"; printf 'EXEC %s.%s(:RET, '"'"'SSS'"'"');\nEXEC %s.%s(:RET, '"'"'SSS'"'"');\nSHOW VARIABLES;\n' "$ORA" "$ORA_P1" "$ORA" "$ORA_P2" > "$F3"
  o=$("$NSQL" run -c "$ORA" --no-prompt "$(w "$F3")" 2>&1); rc=$?; show "$o"
  has "PLS-00306" "$o" && bad "O③ 둘째 EXEC에서 PLS-00306(재추론 안 됨)" "$o" || ok "O③ \`:RET\` 재사용 = PLS-00306 없음(rc=$rc)"
  has "RET +.*REFCURSOR" "$o" && ok "O③ SHOW VARIABLES = RET REFCURSOR" || bad "O③ RET 타입이 REFCURSOR가 아님" "$o"
else skp "Oracle 건너뜀(-O 빈 값)"; fi

# ── SQL Server ────────────────────────────────────────────────────────────────
if [ -n "$MS" ]; then
  say "— SQL Server · 프로필 $MS · DB $MS_DB · $MS_PROC(@$MS_ARG)"
  USE=$'USE '"$MS_DB"$'\nGO\n'
  d=$(probe "m1_user" "$MS" "${USE}EXEC $MS_PROC " end run)
  if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
    has "^Variable\|@$MS_ARG = \|" "$c" && ok "M① \`USE $MS_DB\` 뒤 \`EXEC $MS_PROC \` = \`@$MS_ARG = \`" || bad "M① \`@$MS_ARG = \` 없음" "$c"
    has "^Variable\|$MS_ARG\|" "$c" && bad "M① \`@\` 없는 이름이 섞임" "$c" || ok "M① \`@\` 없는 이름 없음"
    has "#[0-9]{6,}" "$c" && bad "M① 상세 꼬리 #object_id가 남음" "$c" || ok "M① 상세 꼬리 없음"
  else bad "M① 후보 덤프 없음" "$(tail -3 "$OUT/m1_user.stderr")"; fi
  d=$(probe "m2_sys" "$MS" "EXEC sys.sp_addextendedproperty " end)
  if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
    has "^Variable\|@name = \|" "$c" && ok "M② \`EXEC sys.sp_addextendedproperty \` = \`@name = \`" || bad "M② \`@name = \` 없음" "$c"
    has "^Variable\|@level2name = \|" "$c" && ok "M② \`@level2name = \`까지" || bad "M② \`@level2name = \` 없음" "$c"
  else bad "M② 후보 덤프 없음" "$(tail -3 "$OUT/m2_sys.stderr")"; fi
  d=$(probe "m3_used" "$MS" "EXEC sys.sp_addextendedproperty @name = N'MS_Description', " end)
  if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
    has "^Variable\|@value = \|" "$c" && ok "M③ \`,\` 뒤 = \`@value = \`" || bad "M③ \`@value = \` 없음" "$c"
    has "^Variable\|@name = \|" "$c" && bad "M③ 이미 쓴 \`@name\`이 남음" "$c" || ok "M③ 이미 쓴 \`@name\` 제외"
  else bad "M③ 후보 덤프 없음" "$(tail -3 "$OUT/m3_used.stderr")"; fi
  d=$(links "m4_link" "$MS" "${USE}EXEC $MS_PROC @$MS_ARG = 'BBBB';" run)
  if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
    has "^Routine\|(dbo\.)?$MS_PROC\|true" "$c" && ok "M④ \`EXEC $MS_PROC …\` = 루틴 링크(정상)" || bad "M④ 루틴 링크 없음/미확인" "$c"
    has "\|@$MS_ARG\|" "$c" && bad "M④ 인자 이름이 링크로 잡힘" "$c" || ok "M④ 인자 이름은 링크 아님"
  else bad "M④ 링크 덤프 없음" "$(tail -3 "$OUT/m4_link.stderr")"; fi
else skp "SQL Server 건너뜀(-m 빈 값)"; fi

# ── PostgreSQL ────────────────────────────────────────────────────────────────
if [ -n "$PG" ]; then
  say "— PostgreSQL · 프로필 $PG · 임시 함수 public.nsqlt_args(실서버 임시 객체 · 끝에 삭제)"
  FC="$OUT/pg_create.sql"; printf "CREATE OR REPLACE FUNCTION public.nsqlt_args(p_a integer, p_b text) RETURNS integer LANGUAGE sql AS 'SELECT 1';\n" > "$FC"
  FD="$OUT/pg_drop.sql"; printf 'DROP FUNCTION IF EXISTS public.nsqlt_args(integer, text);\n' > "$FD"
  o=$("$NSQL" run -c "$PG" --no-prompt "$(w "$FC")" 2>&1); rc=$?
  if [ $rc -eq 0 ]; then ok "P⓪ 임시 함수 생성"
    d=$(probe "p1_named" "$PG" "SELECT public.nsqlt_args(" end)
    if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
      has "^Variable\|p_a => \|" "$c" && ok "P① \`SELECT public.nsqlt_args(\` = \`p_a => \`" || bad "P① \`p_a => \` 없음" "$c"
      has "^Variable\|p_b => \|" "$c" && ok "P① \`p_b => \`도" || bad "P① \`p_b => \` 없음" "$c"
    else bad "P① 후보 덤프 없음" "$(tail -3 "$OUT/p1_named.stderr")"; fi
    d=$(probe "p2_second" "$PG" "SELECT public.nsqlt_args(p_a => 1, " end)
    if [ -s "$d" ]; then c=$(cat "$d"); show "$c"
      has "^Variable\|p_b => \|" "$c" && ok "P② \`,\` 뒤 = \`p_b => \`" || bad "P② 둘째 인자 후보 없음" "$c"
      has "^Variable\|p_a => \|" "$c" && bad "P② 이미 적은 \`p_a\`가 남음" "$c" || ok "P② 이미 적은 \`p_a\` 제외"
    else bad "P② 후보 덤프 없음" "$(tail -3 "$OUT/p2_second.stderr")"; fi
    o=$("$NSQL" run -c "$PG" --no-prompt "$(w "$FD")" 2>&1); [ $? -eq 0 ] && ok "P⓪ 임시 함수 삭제" || bad "P⓪ 임시 함수 삭제 실패" "$o"
  else bad "P⓪ 임시 함수 생성 실패" "$o"; fi
else skp "PostgreSQL 건너뜀(-r 빈 값)"; fi

say ""; say "== 합계: 통과 $pass · 실패 $fail · 건너뜀 $skip  ($(date '+%F %T'))"
exit $fail
