#!/usr/bin/env bash
# win-reveal-matrix-e2e.sh — "객체 탐색기에서 보기" 조합 매트릭스 E2E(10-01 ㉗-i · 사용자 "각 조합 · DBMS별 · MC/DC로 누락 없이").
#   격리 홈에 실제 프로필(암호 저장)을 복사해 GUI를 기동 명령으로만 몬다(키 주입 0 · docs/61 §4). 케이스마다 새 프로세스(= 첫 시도).
#   조건(MC/DC · 각 조건이 다른 조건 고정 아래 독립으로 바뀌는 쌍이 있다 — docs/96 §9 표):
#     C1 연결 수(1/2/3) · C2 둘째 연결 = 같은 서버 다른 계정 | 다른 서버 · C3 탭 = 마지막 연결 | 되돌아온 연결(A→B→A) ·
#     C4 스키마/DB 전환 횟수(0/1/n · 편집기 문장 = 툴바 전환과 같은 길) · C5 객체 종류(테이블/프로시저/패키지 멤버/함수/컬럼) · C6 폴더 선확장
#   판정(케이스마다): ① 메뉴 판정 `reveal=true`(링크 덤프) ② 선택 경로 = 기대 정규식 ③ `visible=true`.
#
# 사용: scripts/win-reveal-matrix-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe] [-P <실제 설정 폴더>] [-k <케이스 접두 필터>] [-R](원격 PRD 포함)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"; ONLY=""; REMOTE=0
while getopts "o:g:n:P:k:R" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; P) PROF=$OPTARG;; k) ONLY=$OPTARG;; R) REMOTE=1;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n cli] [-P real-config-dir] [-k case-prefix] [-R]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/reveal-matrix-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
sk()  { say "  SKIP  $1"; skip=$((skip+1)); }
chk() { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
# CLI 한 값(한 열 X · 머리줄·구분선 제외).
one() { local f="$OUT/q_$RANDOM.sql"; printf '%s\n' "$2" > "$f"; "$NSQL" run -c "$1" "$f" 2>/dev/null | grep -vE '^([Xx]|-+|\s*)$|rows|Connected|connection|transaction' | grep -oE '^[A-Za-z0-9_$#.]+$' | head -1; }
say "=== 탐색기에서 보기 매트릭스 E2E · $(date '+%F %T')"
# ── 대상 객체(동적 · 저장된 암호 프로필)
O_B=$(one BISCM "SELECT table_name AS X FROM (SELECT table_name FROM user_tables ORDER BY 1) WHERE ROWNUM = 1")
O_B_PROC=$(one BISCM "SELECT object_name AS X FROM (SELECT object_name FROM user_objects WHERE object_type = 'PROCEDURE' ORDER BY 1) WHERE ROWNUM = 1")
O_B_PKGM=$(one BISCM "SELECT object_name || '.' || procedure_name AS X FROM (SELECT object_name, procedure_name FROM user_procedures WHERE object_type = 'PACKAGE' AND procedure_name IS NOT NULL ORDER BY 1, 2) WHERE ROWNUM = 1")
O_B_COL=$(one BISCM "SELECT table_name || '.' || column_name AS X FROM (SELECT table_name, column_name FROM user_tab_columns WHERE table_name = '$O_B' ORDER BY column_id) WHERE ROWNUM = 1")
O_E=$(one SQLEDU "SELECT table_name AS X FROM (SELECT table_name FROM user_tables ORDER BY 1) WHERE ROWNUM = 1")
O_S=$(one BISCM_SB "SELECT table_name AS X FROM (SELECT table_name FROM user_tables ORDER BY 1) WHERE ROWNUM = 1")
P_T=$(one Repository "SELECT table_name AS X FROM information_schema.tables WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY 1 LIMIT 1")
P_F=$(one Repository "SELECT routine_name AS X FROM information_schema.routines WHERE routine_schema = current_schema() AND routine_type = 'FUNCTION' ORDER BY 1 LIMIT 1")
M_T=$(one M4PLAN "SELECT TOP 1 name AS X FROM BISCM_MS.sys.tables ORDER BY name")
M_MASTER=$(one M4PLAN "SELECT TOP 1 name AS X FROM master.sys.tables ORDER BY name")
say "  대상: BISCM=$O_B proc=$O_B_PROC pkg=$O_B_PKGM col=$O_B_COL · SQLEDU=$O_E · BISCM_SB=$O_S · PG=$P_T fn=$P_F · MSSQL BISCM_MS=$M_T master=$M_MASTER"
# ── 케이스 실행기: run_case 이름 시작프로필 "SQL 본문" "앞 단계(@after:…, 쉼표로 끝)" 기준ms 링크이름 기대정규식 [선확장라벨]
run_case() {
  local name=$1 start=$2 sql=$3 pre=$4 t0=$5 link=$6 expect=$7 preexp=${8:-}
  if [ -n "$ONLY" ] && [[ "$name" != $ONLY* ]]; then return; fi
  say "--- $name"
  local d="$OUT/$name"; mkdir -p "$d"; printf '%b' "$sql" > "$d/t.sql"; local fw; fw=$(cygpath -w "$d/t.sql" 2>/dev/null || echo "$d/t.sql")
  local L="$d/links.txt" S="$d/sel.txt"; rm -f "$L" "$S"
  local exp=""; [ -n "$preexp" ] && exp="@after:$((t0-2500)):explorer.expand:$preexp,"
  local t1=$((t0)) t2=$((t0+500)) t3=$((t0+6500)) kill=$(( (t0+12000)/1000 ))
  NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw,${pre}${exp}@after:$t1:objlink.dump:$L,@after:$t2:objlink.reveal:$link,@after:$t3:explorer.selpath:$S" \
    timeout -s KILL "$kill" "$EXE" "$start" > "$d/gui.stdout" 2> "$d/gui.stderr"
  if [ -s "$L" ] && [ -s "$S" ]; then
    local l s; l=$(cat "$L"); s=$(cat "$S")
    say "        > $(echo "$l" | grep -i "|$link|" | head -1)"; say "        > $(echo "$s" | head -2 | tr '\n' ' ')"
    chk "$name ① 메뉴 판정 reveal=true" "\|$link\|true\|[^|]*\|reveal=true$" "$l"
    chk "$name ② 선택 경로" "$expect" "$s"
    chk "$name ③ 화면 안" "^visible=true$" "$s"
  else bad "$name 덤프 없음(links $([ -s "$L" ] && echo o || echo x) · sel $([ -s "$S" ] && echo o || echo x))" "$(tail -3 "$d/gui.stderr")"; fi
}
T_O="SELECT * FROM %s A WHERE 1=1;\n"
# ── A. 단일 연결(C1=1 · C5 종류 · C6 선확장)
run_case A1-ora-table BISCM "$(printf "$T_O" "$O_B")" "" 7000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
run_case A2-ora-table-lower SQLEDU "$(printf "$T_O" "$(echo "$O_E" | tr 'A-Z' 'a-z')")" "" 7000 "$(echo "$O_E" | tr 'A-Z' 'a-z')" "/ SQLEDU / Tables( \([0-9]+\))? / $O_E\$"
run_case A3-ora-table-preexpanded BISCM "$(printf "$T_O" "$O_B")" "" 9000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$" "BISCM/Tables"
if [ -n "$O_B_PROC" ]; then run_case A4-ora-proc BISCM "BEGIN $O_B_PROC(); END;\n/\n" "" 7000 "$O_B_PROC" "/ BISCM / Procedures( \([0-9]+\))? / $O_B_PROC\$"; else sk "A4 프로시저 없음"; fi
if [ -n "$O_B_PKGM" ]; then run_case A5-ora-pkg-member BISCM "BEGIN $O_B_PKGM(); END;\n/\n" "" 7000 "$O_B_PKGM" "/ Packages( \([0-9]+\))? / ${O_B_PKGM%%.*} / Procedures( \([0-9]+\))? / ${O_B_PKGM#*.}\$"; else sk "A5 패키지 멤버 없음"; fi
if [ -n "$O_B_COL" ]; then run_case A6-ora-column BISCM "SELECT A.${O_B_COL#*.} FROM $O_B A;\n" "" 7000 "$O_B_COL" "/ $O_B / Columns( \([0-9]+\))? / ${O_B_COL#*.}\$"; else sk "A6 컬럼 없음"; fi
run_case A7-pg-table Repository "$(printf "$T_O" "$P_T")" "" 7000 "$P_T" "/ Tables( \([0-9]+\))? / $P_T\$"
if [ -n "$P_F" ]; then run_case A8-pg-function Repository "SELECT $P_F();\n" "" 7000 "$P_F" "/ Functions( \([0-9]+\))? / $P_F\$"; else sk "A8 PG 함수 없음"; fi
run_case A9-mssql-master-table M4PLAN "$(printf "$T_O" "$M_MASTER")" "" 7000 "$M_MASTER" "/ master / Tables( \([0-9]+\))? / dbo\.$M_MASTER\$"
# ── B. 둘 · 같은 서버 다른 계정(C1=2 · C2=같은 서버 · C3 방향)
run_case B1-same-server-last SQLEDU "$(printf "$T_O" "$O_B")" "@after:4000:connect:BISCM," 9000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
run_case B2-same-server-back SQLEDU "$(printf "$T_O" "$O_E")" "@after:4000:connect:BISCM,@after:6500:session.bind:SQLEDU," 10000 "$O_E" "/ SQLEDU / Tables( \([0-9]+\))? / $O_E\$"
run_case B3-same-server-sb-first BISCM_SB "$(printf "$T_O" "$O_B")" "@after:4000:connect:BISCM," 9000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
run_case B4-same-server-aba-b BISCM "$(printf "$T_O" "$O_B")" "@after:4000:connect:SQLEDU,@after:6500:session.bind:BISCM," 10000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
# ── C. 둘 · 다른 서버(C2=다른 서버 · 방언 섞임)
run_case C1-ora-then-mssql BISCM "USE BISCM_MS\nGO\n$(printf "$T_O" "$M_T")" "@after:4000:connect:M4PLAN,@after:6500:run.all," 11000 "$M_T" "/ BISCM_MS / Tables( \([0-9]+\))? / dbo\.$M_T\$"
run_case C2-mssql-then-ora M4PLAN "$(printf "$T_O" "$O_B")" "@after:4000:connect:BISCM," 9000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
run_case C3-ora-then-pg BISCM "$(printf "$T_O" "$P_T")" "@after:4000:connect:Repository," 9000 "$P_T" "/ Tables( \([0-9]+\))? / $P_T\$"
run_case C4-ora-pg-back-ora BISCM "$(printf "$T_O" "$O_B")" "@after:4000:connect:Repository,@after:6500:session.bind:BISCM," 10000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
if [ "$REMOTE" = 1 ]; then
  O_R=$(one SNOPDB_19c "SELECT table_name AS X FROM (SELECT table_name FROM user_tables ORDER BY 1) WHERE ROWNUM = 1")
  run_case C5-ora-then-remote-prd BISCM "$(printf "$T_O" "$O_R")" "@after:4000:connect:SNOPDB_19c," 12000 "$O_R" "/ BISCM / Tables( \([0-9]+\))? / $O_R\$"
else sk "C5 원격 PRD(SNOPDB_19c) = -R 없이 건너뜀"; fi
# ── D. 셋(C1=3)
run_case D1-three-bind-middle SQLEDU "$(printf "$T_O" "$O_B")" "@after:4000:connect:BISCM,@after:6500:connect:M4PLAN,@after:9000:session.bind:BISCM," 12500 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
run_case D2-three-bind-first SQLEDU "$(printf "$T_O" "$O_E")" "@after:4000:connect:BISCM,@after:6500:connect:M4PLAN,@after:9000:session.bind:SQLEDU," 12500 "$O_E" "/ SQLEDU / Tables( \([0-9]+\))? / $O_E\$"
run_case D3-three-last-mssql SQLEDU "USE BISCM_MS\nGO\n$(printf "$T_O" "$M_T")" "@after:4000:connect:BISCM,@after:6500:connect:M4PLAN,@after:9000:run.all," 13500 "$M_T" "/ BISCM_MS / Tables( \([0-9]+\))? / dbo\.$M_T\$"
# ── E. 스키마/DB 전환(C4 = 1 · n · 되돌림) — 편집기 문장 = 툴바 전환과 같은 길(DbChanged)
run_case E1-ora-switch-1 BISCM "ALTER SESSION SET CURRENT_SCHEMA = SQLEDU;\n$(printf "$T_O" "$O_E")" "@after:4000:run.all," 8500 "$O_E" "/ SQLEDU / Tables( \([0-9]+\))? / $O_E\$"
run_case E2-ora-switch-3 BISCM "ALTER SESSION SET CURRENT_SCHEMA = SQLEDU;\nALTER SESSION SET CURRENT_SCHEMA = BISCM_SB;\nALTER SESSION SET CURRENT_SCHEMA = BISCM;\n$(printf "$T_O" "$O_B")" "@after:4000:run.all," 9000 "$O_B" "/ BISCM / Tables( \([0-9]+\))? / $O_B\$"
run_case E3-ora-switch-to-sb BISCM "ALTER SESSION SET CURRENT_SCHEMA = BISCM_SB;\n$(printf "$T_O" "$O_S")" "@after:4000:run.all," 8500 "$O_S" "/ BISCM_SB / Tables( \([0-9]+\))? / $O_S\$"
run_case E4-mssql-use-1 M4PLAN "USE BISCM_MS\nGO\n$(printf "$T_O" "$M_T")" "@after:4000:run.all," 9000 "$M_T" "/ BISCM_MS / Tables( \([0-9]+\))? / dbo\.$M_T\$"
run_case E5-mssql-use-3 M4PLAN "USE BISCM_MS\nGO\nUSE master\nGO\nUSE BISCM_MS\nGO\n$(printf "$T_O" "$M_T")" "@after:4000:run.all," 10000 "$M_T" "/ BISCM_MS / Tables( \([0-9]+\))? / dbo\.$M_T\$"
run_case E6-mssql-use-back-master M4PLAN "USE BISCM_MS\nGO\nUSE master\nGO\n$(printf "$T_O" "$M_MASTER")" "@after:4000:run.all," 9500 "$M_MASTER" "/ master / Tables( \([0-9]+\))? / dbo\.$M_MASTER\$"
# ── F. 조합 + 전환(C1=2 · C4=1)
run_case F1-two-conn-then-switch SQLEDU "ALTER SESSION SET CURRENT_SCHEMA = BISCM_SB;\n$(printf "$T_O" "$O_S")" "@after:4000:connect:BISCM,@after:6500:run.all," 11000 "$O_S" "/ BISCM_SB / Tables( \([0-9]+\))? / $O_S\$"
say ""; say "== 합계: 통과 $pass · 실패 $fail · 건너뜀 $skip  ($(date '+%F %T'))"
exit $fail
