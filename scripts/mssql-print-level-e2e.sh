#!/usr/bin/env bash
# mssql-print-level-e2e.sh — 실서버(SQL Server) 메시지 등급 E2E(ⓟ″ · 11-02 · 216차 `message_level_in` 판정 자산 · 3-OS 헤드리스 · 서버 쓰기 0).
#   SQL Server는 TDS Info 토큰(심각도 ≤ 10 = PRINT · RAISERROR ≤ 10)만 메시지로 오므로 Output 줄은 **늘 Info** — 종전에는 `PRINT 'ERROR: …'`가
#   접두 판정으로 빨갛게(✖) 보였다(docs/100 §3-3 · journal 11-01 §9). 판정 = `output.dump` 본문의 접두(`✖ ` Error · `⚠ ` Warn · 없음 = Info ·
#   output.rs `push_at`이 본문에 박는다) → 캡처·픽셀 대조 없이 글로 판정한다(61 §4 덤프·시험 명령 짝 규칙).
#   격리 홈 = `-P <실제 설정 폴더>`의 profiles/device.key 사본(실제 프로필·설정은 건드리지 않음) · 서버에는 PRINT 두 문장만(DDL·DML 없음).
#   생존 = 캡처 없이도 덤프 뒤 5 s 넘게 띄워 `windows.dump`(wins=1)를 받는다(61 §4 · 11-01 216차 스택 넘침 사례) · stderr panic/overflow 0.
# 사용: scripts/mssql-print-level-e2e.sh -o <출력폴더> -P <실제 설정 폴더(%APPDATA%/nexa-sql · ~/.config/nexa-sql)> -c <mssql 프로필> [-g <gui exe>] [-n <nsql exe>]
#   헤드리스 = `NSQL_E2E_HEADLESS=1`(lib/timeout.sh → lib/headless.sh) · 실제 화면에 띄워 보려면 빼고 돌린다. 첫 실측 11-02 Windows(1.5배) = 협업 임시판 ✓.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역 · 끝에서 headless.sh 포함
EXE="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"
[ -x "$EXE.exe" ] && EXE="$EXE.exe"; [ -x "$NSQL.exe" ] && NSQL="$NSQL.exe"
OUT=""; PROF=""; CONN=""
while getopts "o:P:c:g:n:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; c) CONN=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] && [ -n "$CONN" ] || { echo "usage: -o <out dir> -P <real settings dir> -c <mssql profile> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
REPORT="$OUT/mssql-print-level-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -14 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n' > "$H/settings.conf"
[ -d "$PROF/profiles" ] || { say "FAIL  profiles not found under $PROF"; exit 1; }
cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
export NSQL_HOME="$H"
LIST=$("$NSQL" conn list 2>&1)
echo "$LIST" | grep -q "^$CONN " || { bad "프로필 $CONN 없음(격리 홈 사본)" "$LIST"; say "RESULT PASS $pass FAIL $fail"; exit 1; }
echo "$LIST" | grep "^$CONN " | grep -qi "mssql" && ok "프로필 $CONN = mssql(격리 홈 사본 · 실제 설정 미수정)" || bad "프로필 $CONN 방언이 mssql이 아님" "$(echo "$LIST" | grep "^$CONN ")"

SQL="$OUT/print.sql"; DUMP="$OUT/output.txt"; WINS="$OUT/alive_wins.txt"; rm -f "$DUMP" "$WINS"
printf "PRINT 'ERROR: x';\nPRINT 'hello';\n" > "$SQL"
# open → 2.5 s run.all → 6 s output.dump → 12 s windows.dump(덤프 뒤 6 s 생존 증거) → 18 s KILL(= 설계상 종료 · rc 124/137).
CMD="@connected:open:$(w "$SQL"),@connected:@after:2500:run.all,@connected:@after:6000:output.dump:$(w "$DUMP"),@connected:@after:12000:windows.dump:$(w "$WINS")"
T0=$(date +%s)
NSQL_STARTUP_CMD="$CMD" timeout -s KILL 18 "$EXE" "$CONN" > "$OUT/gui.stdout" 2> "$OUT/gui.stderr"
RC=$?; say "gui rc=$RC · $(( $(date +%s) - T0 )) s(124/137 = 제한 시각 KILL = 끝까지 생존)"

if [ -s "$DUMP" ]; then
  ok "output.dump 생성($(head -1 "$DUMP" | tr -d '\r'))"
  E=$(grep -F 'ERROR: x' "$DUMP" | head -1 | tr -d '\r')
  [ -n "$E" ] && ok "PRINT 'ERROR: x' 줄 도착" || bad "PRINT 'ERROR: x' 줄 없음" "$(cat "$DUMP")"
  case "$E" in *"✖ ERROR: x"*|*"⚠ ERROR: x"*) bad "'ERROR: x' 줄이 Error/Warn 접두(✖/⚠) = 접두 판정 회귀" "$E";; *) [ -n "$E" ] && ok "'ERROR: x' 줄 = Info(접두 없음 · SQL Server 메시지는 늘 Info)";; esac
  grep -Fq 'hello' "$DUMP" && ok "PRINT 'hello' 줄 도착" || bad "PRINT 'hello' 줄 없음" "$(cat "$DUMP")"
  grep -Fq '✖' "$DUMP" && bad "Output에 ✖(Error) 줄 있음" "$(grep -F '✖' "$DUMP")" || ok "Output에 ✖ 줄 0"
else
  bad "output.dump 없음(접속 실패 또는 늦음)" "$(cat "$OUT/gui.stderr" 2>/dev/null)"
fi
grep -q 'wins=1' "$WINS" 2>/dev/null && ok "덤프 뒤 6 s 생존(windows.dump wins=1)" || bad "생존 증거 없음(windows.dump)" "$(cat "$WINS" 2>/dev/null)"
if grep -qi 'panic\|overflow' "$OUT/gui.stderr"; then bad "stderr panic/overflow" "$(grep -i 'panic\|overflow' "$OUT/gui.stderr")"; else ok "stderr panic/overflow 0"; fi
say "RESULT PASS $pass FAIL $fail"
[ "$fail" = 0 ]
