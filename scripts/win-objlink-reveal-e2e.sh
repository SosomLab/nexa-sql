#!/usr/bin/env bash
# win-objlink-reveal-e2e.sh — Ctrl 객체 링크 우클릭 ▸ "객체 탐색기에서 보기" E2E(10-01 ㉗ · docs/96 §8).
#   격리 홈에 실제 프로필을 복사해 GUI를 기동 명령으로만 몬다(키 주입 0 · docs/61 §4).
#   ① 테이블 링크(`SELECT * FROM <테이블>`) → `objlink.reveal:<이름>` → 탐색기 선택 경로 = `… / Tables (n) / <테이블>` · 트리 덤프에 그 객체 행
#   ② 패키지 멤버(`pkg.proc` · 실서버의 첫 패키지 프로시저 · 없으면 SKIP) → 선택 경로 = `… / Packages / PKG / Procedures / PROC`
#   ③ 없는 이름(`nsqlt_no_such_table`) = 링크가 미확인이라 reveal 안 됨(ok=false · 선택 없음)
#   ④ 두 연결(-s 두 번째 프로필 먼저 접속 + -p 추가) → 두 번째 칸에서 선택 + `visible=true`(세트 공용 스크롤 · ㉗-b)
#   ⑤ 시스템 객체(㉙): `DBMS_XPLAN.DISPLAY_CURSOR` 루틴 링크 + 시그니처 · `all_tables` 사전 객체 · 모르는 패키지는 링크 아님(`objlink.dump`)
#
# 사용: scripts/win-objlink-reveal-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe] [-p BISCM] [-t 테이블(없으면 user_tables 첫 것)] [-s SQLEDU] [-P <실제 설정 폴더>]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"; PROFILE="BISCM"; TABLE=""; SECOND="SQLEDU"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
while getopts "o:g:n:p:t:s:P:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; p) PROFILE=$OPTARG;; t) TABLE=$OPTARG;; s) SECOND=$OPTARG;; P) PROF=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n cli] [-p profile] [-t table] [-P real-config-dir]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/objlink-reveal-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk() { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
run_gui() { # $1 = sql 파일 · $2 = 링크 이름 · $3 = 선택 경로 파일 · $4 = 덤프 파일 · $5 = 로그 접미
  local fw; fw=$(cygpath -w "$1" 2>/dev/null || echo "$1"); rm -f "$3" "$4"
  NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw,@after:7000:objlink.reveal:$2,@after:12500:explorer.selpath:$3,@after:13000:explorer.dump:$4" \
    timeout -s KILL 16 "$EXE" "$PROFILE" > "$OUT/gui$5.stdout" 2> "$OUT/gui$5.stderr"
}
say "=== Ctrl 링크 ▸ 객체 탐색기에서 보기 E2E · 프로필 $PROFILE · 테이블 $TABLE · $(date '+%F %T')"

say "--- ① 테이블 링크"
if [ -z "$TABLE" ]; then
  Q0="$OUT/tbl.sql"; printf "SELECT table_name FROM (SELECT table_name FROM user_tables ORDER BY table_name) WHERE ROWNUM = 1
" > "$Q0"
  TABLE=$("$NSQL" run -c "$PROFILE" "$Q0" 2>/dev/null | grep -oE '^[A-Z0-9_$#]+$' | grep -vE '^(TABLE_NAME)$' | head -1)
fi
say "        테이블 = $TABLE"
F1="$OUT/t1.sql"; printf 'SELECT *\nFROM %s\nWHERE 1=1;\n' "$TABLE" > "$F1"; S1="$OUT/sel1.txt"; D1="$OUT/tree1.txt"
run_gui "$F1" "$TABLE" "$S1" "$D1" 1
if [ -s "$S1" ]; then
  s1=$(cat "$S1"); say "        > $s1"
  chk "① 선택 = Tables 폴더 아래 $TABLE" "/ Tables( \([0-9]+\))? / $TABLE\$" "$s1"
  t1=$(cat "$D1" 2>/dev/null); chk "① 트리에 그 객체 행(펼쳐져 보임)" "\|object\|$TABLE\|" "$t1"
  chk "① 선택 행이 화면 안(스크롤 맞춤)" "^visible=true$" "$s1"
else bad "① 선택 경로 없음(reveal 실패 · 상태줄 = $(grep -o 'objlink.reveal [^"]*' "$OUT/gui1.stdout" | tail -1))" "$(tail -3 "$OUT/gui1.stderr")"; fi

say "--- ② 패키지 멤버 링크"
Q="$OUT/pkg.sql"; printf "SELECT object_name || '.' || procedure_name AS X FROM user_procedures WHERE object_type = 'PACKAGE' AND procedure_name IS NOT NULL AND ROWNUM = 1\n" > "$Q"
PM=$("$NSQL" run -c "$PROFILE" "$Q" 2>/dev/null | grep -oE '^[A-Z0-9_$#]+\.[A-Z0-9_$#]+$' | head -1)
if [ -n "$PM" ]; then
  PKG=${PM%%.*}; PRC=${PM#*.}
  F2="$OUT/t2.sql"; printf 'BEGIN\n  %s.%s;\nEND;\n/\n' "$PKG" "$PRC" > "$F2"; S2="$OUT/sel2.txt"; D2="$OUT/tree2.txt"
  run_gui "$F2" "$PM" "$S2" "$D2" 2
  if [ -s "$S2" ]; then
    s2=$(cat "$S2"); say "        > $s2"
    chk "② 선택 = Packages / $PKG / Procedures / $PRC" "/ Packages( \([0-9]+\))? / $PKG / Procedures( \([0-9]+\))? / $PRC\$" "$s2"
    chk "② 선택 행이 화면 안" "^visible=true$" "$s2"
  else bad "② 선택 경로 없음($PM)" "$(tail -3 "$OUT/gui2.stderr")"; fi
else
  say "  SKIP  ② 패키지 프로시저가 없다(user_procedures)"
fi

say "--- ③ 없는 객체 = 메뉴 비활성(reveal 안 됨)"
F3="$OUT/t3.sql"; printf 'SELECT * FROM nsqlt_no_such_table;\n' > "$F3"; S3="$OUT/sel3.txt"; D3="$OUT/tree3.txt"
run_gui "$F3" "nsqlt_no_such_table" "$S3" "$D3" 3
if [ -f "$S3" ]; then
  s3=$(cat "$S3"); say "        > $s3"; echo "$s3" | grep -qE "/ (Tables|Views|Packages|Procedures)" && bad "③ 객체가 선택됐다" "$s3" || ok "③ 미확인 링크 = 객체 선택 없음(기본 선택 = 스키마)"
else bad "③ 덤프 없음" "$(tail -3 "$OUT/gui3.stderr")"; fi
say "--- ④ 두 연결(같은 서버 · $SECOND 먼저 + $PROFILE 추가) = 두 번째 연결 칸에서 찾고 화면 안에 보임(㉗-b)"
F4="$OUT/t4.sql"; cp "$F1" "$F4"; S4="$OUT/sel4.txt"; D4="$OUT/tree4.txt"; rm -f "$S4" "$D4"; fw4=$(cygpath -w "$F4" 2>/dev/null || echo "$F4")
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:connect:$PROFILE,@after:6000:open:$fw4,@after:11000:objlink.reveal:$TABLE,@after:16000:explorer.selpath:$S4,@after:16500:explorer.dump:$D4" \
  timeout -s KILL 19 "$EXE" "$SECOND" > "$OUT/gui4.stdout" 2> "$OUT/gui4.stderr"
if [ -s "$S4" ]; then
  s4=$(cat "$S4"); say "        > $(echo "$s4" | tr '\n' ' ')"
  chk "④ 선택 = $PROFILE 칸의 Tables / $TABLE" "/ Tables( \([0-9]+\))? / $TABLE\$" "$s4"
  chk "④ 선택 행이 화면 안(두 번째 칸이라도 스크롤 맞춤)" "^visible=true$" "$s4"
else bad "④ 선택 경로 없음" "$(tail -3 "$OUT/gui4.stderr")"; fi
say "--- ⑤ 시스템 객체 링크(㉙ · 메타에 없는 DBMS_XPLAN·ALL_TABLES = 내장 표로 판정 · 툴팁 = 시그니처)"
F5="$OUT/t5.sql"; printf "SELECT *\nFROM TABLE (DBMS_XPLAN.DISPLAY_CURSOR(NULL, NULL, 'ALLSTATS LAST')) A;\nSELECT owner, table_name FROM all_tables WHERE ROWNUM < 3;\nSELECT nsqlt_no_such_pkg.foo(1) FROM dual;\n" > "$F5"
L5="$OUT/links5.txt"; rm -f "$L5"; fw5=$(cygpath -w "$F5" 2>/dev/null || echo "$F5")
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw5,@after:7000:objlink.dump:$L5" \
  timeout -s KILL 11 "$EXE" "$PROFILE" > "$OUT/gui5.stdout" 2> "$OUT/gui5.stderr"
if [ -s "$L5" ]; then
  l5=$(cat "$L5"); echo "$l5" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "⑤ DBMS_XPLAN.DISPLAY_CURSOR = 루틴 링크(known) + 시그니처 설명" "^Routine\|DBMS_XPLAN\.DISPLAY_CURSOR\|true\|DBMS_XPLAN\.DISPLAY_CURSOR\(" "$l5"
  chk "⑤ all_tables = 사전 객체 링크(known)" "^Table\|all_tables\|true\|" "$l5"
  echo "$l5" | grep -qi "nsqlt_no_such_pkg" && bad "⑤ 없는 패키지가 링크가 됐다" "$l5" || ok "⑤ 모르는 패키지(nsqlt_no_such_pkg.foo · 호출 자리)는 링크 아님"
else bad "⑤ 링크 덤프 없음" "$(tail -3 "$OUT/gui5.stderr")"; fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
