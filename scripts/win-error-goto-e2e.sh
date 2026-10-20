#!/usr/bin/env bash
# win-error-goto-e2e.sh — ★ T-371(131차 win · 10-19 · 사용자 "문법 오류 위치로 커서 이동"): 실행 오류가 위치를 주면 편집기 캐럿이 그 자리로.
#   위치 단위 = "보낸 문장 기준 글자 인덱스(0부터)"(nsql-core `DbError.position`) → 드라이버마다 바이트/줄을 글자로 바꿔 준다 → 러너
#   `map_error_position` → 호스트 `run::error_caret(base, start, pos)` = 편집기 글자 인덱스. 한글 리터럴 **뒤**에 오류 토큰을 둬 바이트/글자
#   변환을 검증한다(`'가나다'` = 글자 3 · UTF-8 9바이트).
#   케이스(방언마다 2 + T-372 2 · SQLite +1) = ① 한 문장 `SELECT '가나다' AS x FROMM dual` → 캐럿 = `FROMM` 시작(줄 1 · 열 19) · SQL Server = 줄 번호만 주므로 **줄 시작**(열 1)
#                     ② 두 문장(첫 문장 정상 · 둘째 문장이 ①) → 캐럿 = 둘째 문장 시작 + 위치(줄 2 · 열 19 · SQL Server 열 1).
#   판정 = `editor.dump` 머리 `caret=줄[:열]`(열이 없으면 줄만 판정 + 안내). 실서버 = 읽기 접속만(객체 생성 0) · 키 주입 0 · 격리 홈.
#   ★ T-372(132차 · 10-20) 고급 분석 = ③ `SELECT DISTINC '가나다' AS x FROM dual` → 서버 위치는 리터럴(열 16 · SQL Server = 줄)인데 캐럿은
#     **첫 의심 지점** `DISTINC`(열 8 · SQL Server도 열 8) + Output에 "Suspected typo" 한 줄 · ④ 둘째 문장 = 줄 2 열 8 ·
#     ⑤ 향상 모드(`perf.boost=on` · SQLite만) = 강제 끔 → 서버 위치 그대로(SQLite = `near "AS"` 열 22).
# 사용: scripts/win-error-goto-e2e.sh -o <출력폴더> -P <실제 NSQL_HOME> [-d Local:sqlite,Repository:postgres,BISCM:oracle,BISCM:oracle-oci,M4PLAN:mssql]
#   `오라클 프로필:oracle-oci` = 같은 프로필을 `oracle.driver=oci`로(격리 홈 설정) · 그 밖은 기본(thin).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"
OUT=""; PROF=""; LIST="Local:sqlite,Repository:postgres,BISCM:oracle,BISCM:oracle-oci,M4PLAN:mssql"
EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:P:d:g:n:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; d) LIST=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out dir> -P <real NSQL_HOME> [-d 프로필:방언,…] [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"
REPORT="$OUT/error-goto-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -6 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

# 격리 홈 하나(프로필·기기 키 복사 · SQLite `Local`은 홈 안 파일).
H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
settings() { # $1 = oracle driver(thin|oci|'') · $2 = 추가 줄(예 perf.boost=on)
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\neditor.error_goto=on\neditor.error_analyze=on\n' > "$H/settings.conf"
  [ -n "$1" ] && printf 'oracle.driver=%s\n' "$1" >> "$H/settings.conf"
  [ -n "${2:-}" ] && printf '%s\n' "$2" >> "$H/settings.conf"
}

# 케이스 본문 — 첫 문장은 방언마다 정상인 꼴(Oracle = FROM dual 필요 · 그 밖은 dual 없음).
first_stmt() { case $1 in oracle*) echo "SELECT 1 AS a FROM dual;";; *) echo "SELECT 1 AS a;";; esac; }
BAD="SELECT '가나다' AS x FROMM dual"   # `FROMM` 시작 = 글자 18(0부터) = 열 19
BAD2="SELECT DISTINC '가나다' AS x FROM dual"   # ③ 오타 `DISTINC` = 열 8 · 서버 위치 = 리터럴 열 16(파서가 멈춘 곳)

# run_case <이름> <프로필> <방언> <파일 본문> <기대 줄> <기대 열>
run_case() {
  local name=$1 conn=$2 dia=$3 body=$4 eline=$5 ecol=$6 want_out=${7:-}
  local sql="$OUT/$name.sql" dump="$OUT/$name.dump" log="$OUT/$name.log" outd="$OUT/$name.out"
  printf '%s' "$body" > "$sql"
  rm -f "$dump" "$log" "$outd"
  # `@connected:`는 `open:`만 접속 뒤로 미루고 `@after:N`은 기동 기준 절대 시각 — Oracle OCI 첫 접속(Instant Client 적재 1.3 s + 접속)이
  #   2.5 s를 넘겨 run.all이 접속 전에 떨어진 적이 있다(2차 V1 OCI-1 = 실행 로그 없음 · 캐럿 1:1 그대로) → 5 s/12 s.
  local cmd="@connected:open:$(w "$sql"),@after:5000:run.all,@after:12000:editor.dump:$(w "$dump"),@after:12300:log.dump:$(w "$log"),@after:12600:output.dump:$(w "$outd")"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 17 "$EXE" "$conn" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
  local head; head=$(head -1 "$dump" 2>/dev/null)
  local caret; caret=$(printf '%s' "$head" | sed -nE 's/.*caret=([0-9]+(:[0-9]+)?).*/\1/p')
  local got_line=${caret%%:*} got_col=""
  case $caret in *:*) got_col=${caret#*:};; esac
  if [ -z "$caret" ]; then bad "$name 덤프 없음/캐럿 없음(접속·실행 실패?)" "$(tail -4 "$log" 2>/dev/null)"; return; fi
  if [ "$got_line" = "$eline" ]; then ok "$name 캐럿 줄 = $eline"; else bad "$name 캐럿 줄 $got_line ≠ $eline" "$(grep -iE "error|ORA-|near|syntax" "$log" 2>/dev/null | tail -3)"; fi
  if [ -n "$got_col" ]; then
    if [ "$got_col" = "$ecol" ]; then ok "$name 캐럿 열 = $ecol"; else bad "$name 캐럿 열 $got_col ≠ $ecol(위치 단위 = 글자? 바이트?)" "$(grep -iE "error|ORA-|near|syntax" "$log" 2>/dev/null | tail -3)"; fi
  else
    say "  SKIP  $name 열 판정 불가(editor.dump 머리에 열 없음 · caret=줄:열 훅 필요)"; skip=$((skip+1))
  fi
  if [ -n "$want_out" ]; then
    if grep -q "$want_out" "$outd" 2>/dev/null; then ok "$name Output '$want_out'"; else bad "$name Output에 '$want_out' 없음" "$(head -5 "$outd" 2>/dev/null)"; fi
  fi
}

say "error-goto-e2e $(date '+%F %T') · 대상 = $LIST"
IFS=',' read -ra PAIRS <<< "$LIST"
for pair in "${PAIRS[@]}"; do
  conn=${pair%%:*}; dia=${pair#*:}
  drv=""; case $dia in oracle-oci) drv=oci;; oracle) drv=thin;; esac
  settings "$drv"
  tag="${conn}-${dia}"
  ecol=19; case $dia in mssql) ecol=1;; esac
  # ① 한 문장
  run_case "$tag-1" "$conn" "$dia" "$BAD" 1 "$ecol"
  # ② 두 문장(둘째가 오류)
  run_case "$tag-2" "$conn" "$dia" "$(first_stmt "$dia")
$BAD;" 2 "$ecol"
  # ③ ★ T-372 고급 분석: 서버 위치(리터럴) 앞의 오타 `DISTINC` = 열 8 — SQL Server도 열까지(종전 줄만) · Output 한 줄.
  run_case "$tag-3" "$conn" "$dia" "$BAD2" 1 8 "Suspected typo"
  # ④ 둘째 문장의 오타 = 줄 2 열 8.
  run_case "$tag-4" "$conn" "$dia" "$(first_stmt "$dia")
$BAD2;" 2 8
  # ⑤ 향상 모드 = 분석 강제 끔 → 서버 위치 그대로 · 서버 없는 SQLite만. SQLite는 문자열 리터럴을 별칭으로 받으므로 멈춘 토큰 = `AS`
  #   (`near "AS"` · 바이트 27 → 글자 21 → 열 22 · ③ Output의 server position 1:22와 같은 값) — 리터럴에서 멈추는 Oracle/PG(열 16)와 다르다.
  if [ "$dia" = sqlite ]; then
    settings "" "perf.boost=on"
    run_case "$tag-5-boost" "$conn" "$dia" "$BAD2" 1 22
    settings ""
  fi
done
say "RESULT PASS $pass FAIL $fail SKIP $skip"
[ "$fail" -eq 0 ]
