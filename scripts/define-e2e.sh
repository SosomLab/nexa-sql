#!/usr/bin/env bash
# define-e2e.sh — `&` 치환 변수 방언 기본(D-272 · 설정 `script.define` · docs/63 §12-4) E2E(3-OS bash · 키 주입 0 · 기동 명령만 · 10-10 사용자
#   "점검 항목이 너무 많아 자동화"). ① CLI `nsql plan`(서버 없음 · 4방언) ② CLI `nsql run`(실서버 = -P 복사 프로필 · 읽기 질의만)
#   ③ GUI(격리 홈 · 프로필 탭 · `input.dump` = 변수 입력 창이 떴는가 · `grid.dump` = 결과 값).
#   기대: Oracle = 미정의 `&D` 묻기 · 다른 DBMS = `'R&D'` 그대로(묻지 않음) · `:setvar Env` → `&Env` = 전 방언 치환 · `SET DEFINE ON` = 전 방언 묻기 ·
#   `script.define=off` = `&Env`도 글자 그대로 · `script.define=on` = MSSQL도 묻기.
# 사용: scripts/define-e2e.sh -o <출력> [-e target/debug/nexa-sql] [-n target/debug/nsql] [-P <실제 설정 폴더>] [-d oracle:BISCM,mssql:M4PLAN,postgres:Repository,sqlite:Demo]
#   -P 없으면 ①만. GUI는 -P와 -d가 있을 때. 대기 = 마지막 @after + 5 s(App Nap · 61 §4).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"; OUT=""; PROF=""; DBMS="${NSQL_E2E_DBMS:-}"
while getopts "o:e:n:P:d:" o; do case $o in o) OUT=$OPTARG;; e) APP=$OPTARG;; n) NSQL=$OPTARG;; P) PROF=$OPTARG;; d) DBMS=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-e app] [-n cli] [-P <settings dir>] [-d dialect:profile,…]"; exit 2; }
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) [ -x "$NSQL" ] || NSQL="$NSQL.exe"; [ -x "$APP" ] || APP="$APP.exe";; esac
mkdir -p "$OUT"; H="$OUT/home"; D="$OUT/data"; O="$OUT/dump"; rm -rf "$H" "$D" "$O"; mkdir -p "$H" "$D" "$O"
printf 'ui.lang=en\ndemo.prompted=on\n' > "$H/settings.conf"
if [ -n "$PROF" ] && [ -d "$PROF/profiles" ]; then cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null; fi
export NSQL_HOME="$H"
REPORT="$OUT/define-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -10 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk()  { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
nchk() { if echo "$3" | grep -qE -- "$2"; then bad "$1 (없어야 함 $2)" "$3"; else ok "$1"; fi; }
setconf() { printf 'ui.lang=en\ndemo.prompted=on\n%s' "$1" > "$H/settings.conf"; }

say "=== D-272 & 치환 E2E · $(date '+%F %T') · cli=$NSQL app=$APP · 홈 $H"

printf "SELECT 'R&D' AS x;\n" > "$D/rd.sql"
printf "SELECT 'R&D' AS x FROM dual;\n" > "$D/rd_ora.sql"
printf ":setvar Env prod\nSELECT '&Env' AS e;\n" > "$D/setvar.sql"
printf ":setvar Env prod\nSELECT '&Env' AS e FROM dual;\n" > "$D/setvar_ora.sql"
printf "SET DEFINE ON\nSELECT 'R&D' AS x;\n" > "$D/defon.sql"
printf "SET DEFINE OFF\nSELECT 'R&D' AS x FROM dual;\n" > "$D/defoff_ora.sql"

# ── ① CLI plan(서버 없음) ───────────────────────────────────────────────────
say "-- ① nsql plan (방언별 · 서버 없음)"
for d in mssql postgres sqlite; do
  t=$("$NSQL" plan -d $d "$D/rd.sql" 2>&1); chk "plan $d: 'R&D' 그대로" "SELECT 'R&D' AS x" "$t"; nchk "plan $d: 미정의 경고 없음" "미정의|undefined" "$t"
  t=$("$NSQL" plan -d $d "$D/setvar.sql" 2>&1); chk "plan $d: :setvar → &Env = prod" "SELECT 'prod' AS e" "$t"
  t=$("$NSQL" plan -d $d "$D/defon.sql" 2>&1); chk "plan $d: SET DEFINE ON = 묻기(치환 시도)" "SELECT 'R' AS x" "$t"
done
t=$("$NSQL" plan -d oracle "$D/rd.sql" 2>&1); chk "plan oracle: 미정의 &D = 치환 시도" "SELECT 'R' AS x" "$t"
t=$("$NSQL" plan -d oracle "$D/defoff_ora.sql" 2>&1); chk "plan oracle: SET DEFINE OFF = 그대로" "SELECT 'R&D' AS x" "$t"
setconf 'script.define=off'
t=$("$NSQL" plan -d mssql "$D/setvar.sql" 2>&1); chk "plan mssql · script.define=off: &Env 그대로" "SELECT '&Env' AS e" "$t"
t=$("$NSQL" plan -d oracle "$D/rd.sql" 2>&1); chk "plan oracle · off: 'R&D' 그대로" "SELECT 'R&D' AS x" "$t"
setconf 'script.define=on'
t=$("$NSQL" plan -d mssql "$D/rd.sql" 2>&1); chk "plan mssql · script.define=on: 묻기(치환 시도)" "SELECT 'R' AS x" "$t"
setconf ''

# ── ② CLI run(실서버 · 읽기 질의만) ─────────────────────────────────────────
if [ -n "$DBMS" ]; then
  say "-- ② nsql run (실서버 · 읽기 질의)"
  IFS=',' read -ra PAIRS <<< "$DBMS"
  for pr in "${PAIRS[@]}"; do
    d=${pr%%:*}; p=${pr#*:}
    case $d in
      oracle)
        t=$("$NSQL" run -c "$p" --no-prompt -f csv "$D/rd_ora.sql" 2>&1); chk "run $p(oracle): 미정의 &D = 오류/묻기" "정의되지 않았|undefined|not defined" "$t"
        t=$("$NSQL" run -c "$p" --no-prompt -f csv "$D/setvar_ora.sql" 2>&1); chk "run $p(oracle): :setvar → prod" "^prod$" "$t";;
      *)
        t=$("$NSQL" run -c "$p" --no-prompt -f csv "$D/rd.sql" 2>&1); chk "run $p($d): 'R&D' 그대로" "^R&D$" "$t"
        t=$("$NSQL" run -c "$p" --no-prompt -f csv "$D/setvar.sql" 2>&1); chk "run $p($d): :setvar → prod" "^prod$" "$t";;
    esac
  done
fi

# ── ③ GUI(격리 홈 · 프로필 탭) ───────────────────────────────────────────────
run_gui() { # <초> <프로필> <기동 명령>
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$3" "$APP" "$2" >/dev/null 2>>"$O/stderr.txt" &
  local p=$!; sleep "$1"; kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
}
if [ -n "$DBMS" ] && [ -x "$APP" ]; then
  say "-- ③ GUI (프로필 탭 · 입력 창 덤프)"
  IFS=',' read -ra PAIRS <<< "$DBMS"
  for pr in "${PAIRS[@]}"; do
    d=${pr%%:*}; p=${pr#*:}
    rd="$D/rd.sql"; sv="$D/setvar.sql"; [ "$d" = oracle ] && { rd="$D/rd_ora.sql"; sv="$D/setvar_ora.sql"; }
    rm -f "$O/$p"_*.txt
    run_gui 14 "$p" "open:$rd,@after:4000:run.all,@after:8000:input.dump:$O/${p}_rd_in.txt,@after:8500:grid.dump:$O/${p}_rd_grid.txt"
    i=$(cat "$O/${p}_rd_in.txt" 2>/dev/null); g=$(cat "$O/${p}_rd_grid.txt" 2>/dev/null)
    if [ "$d" = oracle ]; then
      chk "gui $p(oracle): 'R&D' = 입력 창 열림" "open=true" "$i"; chk "gui $p(oracle): 묻는 이름 = macro D" "macro\|D\|" "$i"
    else
      chk "gui $p($d): 'R&D' = 입력 창 없음" "open=false" "$i"; chk "gui $p($d): 결과 1행 R&D" "R&D" "$g"
    fi
    run_gui 14 "$p" "open:$sv,@after:4000:run.all,@after:8000:input.dump:$O/${p}_sv_in.txt,@after:8500:grid.dump:$O/${p}_sv_grid.txt"
    i=$(cat "$O/${p}_sv_in.txt" 2>/dev/null); g=$(cat "$O/${p}_sv_grid.txt" 2>/dev/null)
    chk "gui $p($d): :setvar → 입력 창 없음" "open=false" "$i"; chk "gui $p($d): :setvar → prod" "prod" "$g"
    if [ "$d" != oracle ]; then
      run_gui 14 "$p" "open:$D/defon.sql,@after:4000:run.all,@after:8000:input.dump:$O/${p}_defon_in.txt"
      i=$(cat "$O/${p}_defon_in.txt" 2>/dev/null); chk "gui $p($d): SET DEFINE ON = 입력 창(&D)" "macro\|D\|" "$i"
      setconf 'script.define=on'
      run_gui 14 "$p" "open:$rd,@after:4000:run.all,@after:8000:input.dump:$O/${p}_on_in.txt"
      i=$(cat "$O/${p}_on_in.txt" 2>/dev/null); chk "gui $p($d): script.define=on = 입력 창(&D)" "open=true" "$i"
      setconf ''
    fi
  done
fi

say "=== 결과: PASS $pass · FAIL $fail"
[ "$fail" = 0 ]
