#!/usr/bin/env bash
# verify.sh — 한 명령 검증 흐름(사용자 11-21 "사전에 점검한 절차를 순차로 돌리는 안정적인 Flow · Blackbox 입력 → 출력만 보고 분석" · D-491).
#   싼 단계 먼저 · 실패해도 끝까지 돌며 전부 수집(`--fail-fast`면 첫 실패에서 멈춤) · 결과 = target/verify/<시각>/summary.txt(단계별 PASS/FAIL/SKIP ·
#   소요 초 · 첫 오류 5줄) + 단계별 원 로그(같은 폴더) · 종료 코드 = 실패 단계 수. 원문 규칙 = docs/102 §5-7 · docs/16 §4-1.
# 사용:
#   scripts/verify.sh --level quick                 # fmt(두 작업공간) · clippy(호스트 · DuckDB 제외) · todo-check
#   scripts/verify.sh --level v2 [--v1 a,b]          # quick + 단위 시험 · 통합 시험 · Debug 빌드 · V1 시나리오
#   scripts/verify.sh --level prepush                # quick + 단위 시험 + 더 새 툴체인 clippy(push 전 1회)
#   scripts/verify.sh --level full                   # v2 + 교차 타깃 clippy(check-3os 전체) · DuckDB 포함
#   옵션: --v1 <이름,…|all>(scripts/v1/<이름>.sh) · --restart(실기 Debug가 떠 있으면 닫고 빌드한 뒤 실제 설정으로 다시 띄움) ·
#         --no-it(통합 시험 생략) · --fail-fast · --out <폴더>
#   통합 시험 프로필 = env NSQL_ORACLE_PROFILE · NSQL_MSSQL_PROFILE · NSQL_PG_PROFILE(실제 이름은 저장소에 쓰지 않는다 · 익명화) ·
#   비어 있거나 서버 TCP 도달 실패 = "SKIP 판정 불가"(실패로 치지 않음).
# 원칙(D-491 · D-490 로컬 ①): clippy는 한 번(이 스크립트의 clippy 단계 = check-3os --quick) · 별도 target 폴더는 `target/it` 하나 ·
#   패키지 인자는 늘 같은 집합(LOCAL_PKGS)이라 feature 통합이 바뀌어 다시 컴파일되지 않게 한다 · V1은 target/debug exe(실기 창이 떠 있으면
#   `--restart` 없이는 빌드·V1을 SKIP).
# ⚠ 작성 중(11-21 · 남은 것 = v2 전체 첫 통과 · 옛 절차 대비 전후 소요 · `cargo build -v` Dirty/Fresh 점검 · DuckDB 제외 효과 측정 · T-510).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
LEVEL=quick; V1LIST=""; RESTART=0; NOIT=0; FAILFAST=0; OUT=""
while [ $# -gt 0 ]; do case "$1" in
  --level) LEVEL=$2; shift 2;; --v1) V1LIST=$2; shift 2;; --restart) RESTART=1; shift;;
  --no-it) NOIT=1; shift;; --fail-fast) FAILFAST=1; shift;; --out) OUT=$2; shift 2;;
  -h|--help) sed -n '2,20p' "$0"; exit 0;; *) echo "알 수 없는 인자: $1"; exit 2;; esac; done
case "$LEVEL" in quick|v2|prepush|full) ;; *) echo "--level quick|v2|prepush|full"; exit 2;; esac
STAMP=$(date +%Y%m%d-%H%M%S); OUT="${OUT:-$ROOT/target/verify/$STAMP}"; mkdir -p "$OUT"
SUM="$OUT/summary.txt"
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) WIN=1; EXE_SUF=.exe;; *) WIN=0; EXE_SUF=;; esac
# 패키지 집합(B-1 (다) · 11-21 측정으로 정함 = 아래 주석 참조): 로컬은 DuckDB(nexa-sql · nsql-cli가 쓰지 않는 확장 전용 · 빌드 무거움)를 뺀다.
LOCAL_PKGS=(--workspace --exclude nsql-driver-duckdb)
[ "$LEVEL" = full ] && LOCAL_PKGS=(--workspace)
FAILN=0; STOP=0
printf '# verify %s · level=%s · v1=%s · %s\n\n| 단계 | 결과 | 초 | 메모 |\n|---|---|---|---|\n' "$STAMP" "$LEVEL" "${V1LIST:--}" "$(git rev-parse --short HEAD 2>/dev/null)" > "$SUM"
# ── 모니터링 규격(사용자 11-21 "Blackbox의 단점 = 모니터링이 어렵다" · docs/claude-batch-flow.md §6 · 8항목) ──────────────────────
#   ① 작업판 start/done ② 진행 n/N + 요약은 단계가 끝날 때마다 덧붙임 ③ 예상 = 이력 중앙값(지난 3회) ④ 살아 있다는 신호 = 30초마다 경과 + 로그 마지막 줄
#   ⑤ 단계 상한 = 중앙값 × 3(최소 120 s · 최대 3600 s · 이력 없으면 기본값) → TIMEOUT + 그 단계 프로세스 트리 종료(내가 띄운 것만) ⑥ 첫 실패 즉시 표시
#   ⑦ 단계 PID를 신호에 ⑧ 한 화면 = scripts/status.sh · target/status/now.md
. "$ROOT/scripts/lib/board.sh"
HIST="$ROOT/target/verify/history.tsv"; touch "$HIST"
BID="verify-$STAMP"; SES="${BOARD_SESSION:-협업}"
declare -A DEF=([fmt-ui]=120 [check3os]=1800 [todo]=60 [test]=1800 [clippy-newer]=3600 [it]=1200 [build]=1800 [v1]=900 [restart]=60)
median() { awk -F'\t' -v l="$LEVEL" -v s="$1" '$1==l && $2==s {a[++n]=$3} END { if (!n) { print ""; exit } m=0; k=0; for (i=n; i>=1 && k<3; i--) b[++k]=a[i]; asort(b); print b[int((k+1)/2)] }' "$HIST"; }
# 단계별 하한(11-22 · v1은 시나리오가 늘면 지난 이력 × 3이 모자라 TIMEOUT — 9개 시나리오 판이 120 s에 잘림 · 하한 600 s).
declare -A MINS=([v1]=600)
limit() { local m lo=${MINS[$1]:-120}; m=$(median "$1"); if [ -z "$m" ]; then echo "${DEF[$1]:-1800}"; else m=$((m * 3)); [ $m -lt $lo ] && m=$lo; [ $m -gt 3600 ] && m=3600; echo $m; fi; }
PLAN=(); STEPN=0
remaining() { local s t=0 i m; for ((i=STEPN; i<${#PLAN[@]}; i++)); do s=${PLAN[$i]}; m=$(median "$s"); t=$((t + ${m:-0})); done; echo $t; }
row() { printf '| %s | %s | %s | %s |\n' "$1" "$2" "$3" "$4" >> "$SUM"; echo "[$2] $1 ($3 s) $4"; }
first_err() { grep -m5 -E "^error|^ERROR|error\[|FAILED|panicked|✗|^FAIL |Diff in|^warning: unused" "$1" 2>/dev/null | cut -c1-200; }
killtree() { if [ "$WIN" = 1 ] && [ -r "/proc/$1/winpid" ]; then taskkill //F //T //PID "$(cat "/proc/$1/winpid")" >/dev/null 2>&1; else pkill -P "$1" 2>/dev/null; kill "$1" 2>/dev/null; fi; }
# step <이름> <명령…> — 로그 = $OUT/<이름>.log · 배경 실행 + 30초 신호 + 상한.
step() {
  local name=$1; shift
  STEPN=$((STEPN + 1))
  if [ "$STOP" = 1 ]; then row "$name" SKIP 0 "fail-fast로 중단"; return; fi
  local lim; lim=$(limit "$name"); local t0=$SECONDS
  "$@" > "$OUT/$name.log" 2>&1 &
  local pid=$! last=$SECONDS to=0
  board step "$BID" "$STEPN/${#PLAN[@]}" "$name 시작 · 상한 ${lim}s · PID $pid" "$(( $(remaining) + 0 ))"
  while kill -0 "$pid" 2>/dev/null; do
    sleep 2
    if [ $((SECONDS - t0)) -gt "$lim" ]; then killtree "$pid"; to=1; break; fi
    if [ $((SECONDS - last)) -ge 30 ]; then last=$SECONDS
      board step "$BID" "$STEPN/${#PLAN[@]}" "$name $((SECONDS - t0))s · $(tail -n1 "$OUT/$name.log" 2>/dev/null | tr -d '\r' | cut -c1-90)" "$(( $(remaining) + ${lim:-0} - (SECONDS - t0) ))"
    fi
  done
  wait "$pid" 2>/dev/null; local rc=$?; local dt=$((SECONDS - t0))
  [ "$to" = 1 ] && rc=124
  # 이력 = 실제로 끝까지 돈 판(PASS · FAIL)만 — SKIP(1 s)·TIMEOUT을 넣으면 중앙값이 줄어 다음 상한이 120 s로 떨어진다(11-22 it 단계 TIMEOUT).
  [ $rc -ne 99 ] && [ $rc -ne 124 ] && printf '%s\t%s\t%s\t%s\n' "$LEVEL" "$name" "$dt" "$(date +%F)" >> "$HIST"
  if [ $rc -eq 0 ]; then row "$name" PASS "$dt" "$(grep -m1 -E 'test result|통과|PASS' "$OUT/$name.log" 2>/dev/null | cut -c1-80)"
  elif [ $rc -eq 99 ]; then row "$name" SKIP "$dt" "$(tail -1 "$OUT/$name.log" | cut -c1-120)"
  else
    FAILN=$((FAILN + 1))
    if [ $rc -eq 124 ]; then row "$name" TIMEOUT "$dt" "상한 ${lim}s 초과 · 프로세스 트리 종료 · $OUT/$name.log"; else row "$name" FAIL "$dt" "rc=$rc · $OUT/$name.log"; fi
    first_err "$OUT/$name.log" | sed 's/^/    /' >> "$SUM"
    [ "$FAILN" = 1 ] && board alert "$BID" "첫 실패 = $name($([ $rc -eq 124 ] && echo TIMEOUT || echo "rc=$rc")) · $(first_err "$OUT/$name.log" | head -1 | cut -c1-100)"
    [ "$FAILFAST" = 1 ] && STOP=1
  fi
}

# ── 단계들 ───────────────────────────────────────────────────────────────────────────────────────
s_fmt_ui() { [ -d ../nexa-ui ] || { echo "nexa-ui 없음"; return 99; }; (cd ../nexa-ui && cargo fmt --all -- --check); }
s_check3os() { CHECK3OS_PKGS="${LOCAL_PKGS[*]}" bash scripts/check-3os.sh "$@"; }
# 단위 시험 단계는 통합 시험용 프로필 env를 지운다(11-21 첫 실행: env가 남아 통합 시험이 병렬 · thin 지정 없이 실서버에 붙어 Oracle 7건 FAIL — 통합은 it 단계에서만).
s_test() { env -u NSQL_ORACLE_PROFILE -u NSQL_MSSQL_PROFILE -u NSQL_PG_PROFILE -u NSQL_MYSQL_PROFILE -u NSQL_ODBC_PROFILE -u NSQL_SSH_PROFILE cargo test -q "${LOCAL_PKGS[@]}" 2>&1 | grep -vE '^\s*$|running [0-9]+ tests|^test .* ok$'; return "${PIPESTATUS[0]}"; }
s_newer_clippy() { CHECK3OS_NEWER_ONLY=1 CHECK3OS_PKGS="${LOCAL_PKGS[*]}" bash scripts/check-3os.sh --quick; }
s_it() {
  [ "$NOIT" = 1 ] && { echo "SKIP --no-it"; return 99; }
  local p
  for p in NSQL_ORACLE_PROFILE NSQL_MSSQL_PROFILE NSQL_PG_PROFILE; do [ -n "${!p:-}" ] || { echo "SKIP 판정 불가(env $p 없음)"; return 99; }; done
  if [ "$WIN" = 1 ] && command -v pwsh >/dev/null; then
    local down; down=$(pwsh -NoProfile -File "$ROOT/scripts/lib/reach.ps1" "$NSQL_ORACLE_PROFILE" "$NSQL_MSSQL_PROFILE" "$NSQL_PG_PROFILE" </dev/null 2>/dev/null | grep -c FAIL)
    [ "${down:-0}" -gt 0 ] && { echo "SKIP 판정 불가(사내 DBMS TCP 도달 실패 ${down}곳)"; return 99; }
  fi
  CARGO_TARGET_DIR=target/it NSQL_ORACLE_DRIVERS="${NSQL_ORACLE_DRIVERS:-thin}" cargo test -p nsql-drivers --test integration -- --test-threads=1 2>&1 | grep -E "test result|FAILED|panicked|^---- "; return "${PIPESTATUS[0]}"
}
debug_running() { if [ "$WIN" = 1 ]; then pwsh -NoProfile -Command "@(Get-Process nexa-sql -ErrorAction SilentlyContinue | Where-Object { \$_.Path -like '*\\target\\debug\\nexa-sql.exe' }).Count" </dev/null 2>/dev/null | tr -d '\r'; else pgrep -f "$ROOT/target/debug/nexa-sql" | wc -l; fi; }
s_build() {
  local n; n=$(debug_running)
  if [ "${n:-0}" -gt 0 ]; then
    [ "$RESTART" = 1 ] || { echo "SKIP 실기 Debug가 떠 있음(--restart로 닫고 빌드)"; return 99; }
    echo "실기 Debug 종료(--restart)"
    if [ "$WIN" = 1 ]; then pwsh -NoProfile -Command "Get-Process nexa-sql -ErrorAction SilentlyContinue | Where-Object { \$_.Path -like '*\\target\\debug\\nexa-sql.exe' } | Stop-Process -Force" </dev/null; else pkill -f "$ROOT/target/debug/nexa-sql"; fi
  fi
  cargo build -q -p nexa-sql -p nsql-cli
}
s_v1() {
  [ -x "target/debug/nexa-sql$EXE_SUF" ] || { echo "SKIP Debug exe 없음"; return 99; }
  local list=$V1LIST f rc=0
  [ "$list" = all ] && list=$(ls scripts/v1/*.sh | xargs -n1 basename | sed 's/\.sh$//' | tr '\n' ',')
  for f in ${list//,/ }; do
    [ -f "scripts/v1/$f.sh" ] || { echo "FAIL $f 시나리오 없음"; rc=$((rc + 1)); continue; }
    V1_OUT="$OUT/v1-$f" V1_NAME="$f" bash "scripts/v1/$f.sh" </dev/null; rc=$((rc + $?))
  done
  return $rc
}
s_restart() {
  [ "$RESTART" = 1 ] || return 99
  if [ "$WIN" = 1 ]; then pwsh -NoProfile -Command "\$p = Start-Process -FilePath '$(cygpath -w "$ROOT/target/debug/nexa-sql.exe")' -WorkingDirectory '$(cygpath -w "$ROOT")' -PassThru; 'PID ' + \$p.Id" </dev/null
  else nohup "$ROOT/target/debug/nexa-sql" >/dev/null 2>&1 & echo "PID $!"; fi
}
s_todo() { bash scripts/todo-check.sh --quiet; }

# ── 수준별 순서(싼 것 먼저) ───────────────────────────────────────────────────────────────────────
PLAN=(fmt-ui check3os todo)
case "$LEVEL" in
  prepush) PLAN+=(test clippy-newer) ;;
  v2|full) PLAN+=(test it build); [ -n "$V1LIST" ] && PLAN+=(v1); [ "$RESTART" = 1 ] && PLAN+=(restart) ;;
esac
EST=$(( ($(remaining) + 59) / 60 ))
board start "$BID" "$SES" "verify --level $LEVEL${V1LIST:+ --v1 $V1LIST}" "$EST"
[ "$EST" = 0 ] && board step "$BID" "0/${#PLAN[@]}" "예상 없음(첫 실행)" ""
for s in "${PLAN[@]}"; do
  case "$s" in
    fmt-ui) step fmt-ui s_fmt_ui ;;
    check3os) if [ "$LEVEL" = full ]; then step check3os s_check3os; else step check3os s_check3os --quick; fi ;;
    todo) step todo s_todo ;;
    test) step test s_test ;;
    clippy-newer) step clippy-newer s_newer_clippy ;;
    it) step it s_it ;;
    build) step build s_build ;;
    v1) step v1 s_v1 ;;
    restart) step restart s_restart ;;
  esac
done
printf '\n실패 단계 %s · 로그 = %s\n' "$FAILN" "$OUT" >> "$SUM"
board done "$BID" "$([ "$FAILN" = 0 ] && echo ok || echo fail)" "실패 $FAILN · $(basename "$OUT")/summary.txt"
echo; cat "$SUM"
exit "$FAILN"
