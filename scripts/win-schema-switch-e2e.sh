#!/usr/bin/env bash
# win-schema-switch-e2e.sh — 작업 단위 전환 E2E(10-01 ⑮ · 사용자 "오라클도 툴바에서 변경"): Oracle `ALTER SESSION SET CURRENT_SCHEMA`가
#   편집기에서 성공하면 세션 현재 스키마 → `SHOW CONN` "현재 스키마" 줄 · 운영 2단 확인 없음(문맥 전환 = 데이터 변경 아님) ·
#   CLI `nsql run`도 `current database:` 안내. 격리 홈 · 실제 프로필 복사 · 키 주입 0.
# 사용: scripts/win-schema-switch-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe] [-p BISCM] [-s BISCM_SB] [-P <실제 설정 폴더>]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"; PROFILE="BISCM"; OTHER="BISCM_SB"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
while getopts "o:g:n:p:s:P:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; p) PROFILE=$OPTARG;; s) OTHER=$OPTARG;; P) PROF=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n cli] [-p profile] [-s other-schema] [-P real-config-dir]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/schema-switch-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk() { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }

say "=== 작업 단위 전환 E2E · 프로필 $PROFILE → 스키마 $OTHER · $(date '+%F %T')"
F="$OUT/switch.sql"; printf 'SHOW CONN\nALTER SESSION SET CURRENT_SCHEMA = %s;\nSHOW CONN\nSELECT SYS_CONTEXT('"'"'USERENV'"'"','"'"'CURRENT_SCHEMA'"'"') AS cs FROM dual;\n' "$OTHER" > "$F"
echo "--- CLI" | tee -a "$REPORT" >/dev/null
o=$("$NSQL" run -c "$PROFILE" "$F" 2>&1); echo "$o" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
chk "CLI: ALTER SESSION 뒤 current database 안내 = $OTHER" "current database: $OTHER" "$o"
chk "CLI: 서버 CURRENT_SCHEMA = $OTHER" "^$OTHER\$|^$OTHER " "$o"
echo "--- GUI" | tee -a "$REPORT" >/dev/null
fw=$(cygpath -w "$F" 2>/dev/null || echo "$F"); D="$OUT/output.txt"; rm -f "$D"
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw,@after:5000:run.all,@after:10000:output.dump:$D" \
  timeout -s KILL 13 "$EXE" "$PROFILE" > "$OUT/gui.stdout" 2> "$OUT/gui.stderr"
if [ -s "$D" ]; then
  b=$(tail -n +2 "$D"); echo "$b" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  n=$(echo "$b" | grep -c "접속 정보\|Connection info"); [ "$n" -ge 2 ] && ok "GUI: SHOW CONN 두 번(한 F5 · 2단 확인 없음)" || bad "GUI: SHOW CONN 수 $n < 2(운영 확인에 막혔나?)" "$b"
  chk "GUI: ALTER SESSION 뒤 현재 스키마 = $OTHER" "현재 스키마: $OTHER|Current schema: $OTHER" "$b"
  first=$(echo "$b" | awk '/접속 정보|Connection info/{n++} n==1' | grep "현재 스키마\|Current schema" | head -1)
  echo "$first" | grep -q "$OTHER" && bad "GUI: 전환 전 블록이 이미 $OTHER" "$first" || ok "GUI: 전환 전 블록 = 접속 기본 스키마($first)"
  n2=$(echo "$b" | grep -c "현재 스키마\|Current schema"); [ "$n2" -eq 2 ] && ok "GUI: 현재 스키마 줄 = 블록마다 하나(중복 없음)" || bad "GUI: 현재 스키마 줄 수 $n2" "$b"
else bad "GUI Output 덤프 없음" "$(tail -3 "$OUT/gui.stderr")"; fi
# ④ 탭별 작업 단위(⑯): 탭1에서 ALTER SESSION(= 탭1 단위 BISCM_SB) → 새 탭2 열림(= 연결 기본 BISCM으로 조용히 복귀) → 탭2 SHOW CONN = BISCM
#    → 탭1로 돌아감(= BISCM_SB로 조용히 복귀) → 팔레트 session.info = BISCM_SB(실행 없이 세션 상태만).
echo "--- ④ 탭별 작업 단위" | tee -a "$REPORT" >/dev/null
F1="$OUT/t1.sql"; printf 'ALTER SESSION SET CURRENT_SCHEMA = %s;
' "$OTHER" > "$F1"; F2="$OUT/t2.sql"; printf 'SHOW CONN
' > "$F2"
fw1=$(cygpath -w "$F1" 2>/dev/null || echo "$F1"); fw2=$(cygpath -w "$F2" 2>/dev/null || echo "$F2"); D2="$OUT/tab2.txt"; D1="$OUT/tab1.txt"; rm -f "$D1" "$D2"
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw1,@after:5000:run.all,@after:8000:open:$fw2,@after:10500:run.all,@after:12500:output.dump:$D2,@after:13000:tab.prev,@after:15500:session.info,@after:16500:output.dump:$D1"   timeout -s KILL 19 "$EXE" "$PROFILE" > "$OUT/gui4.stdout" 2> "$OUT/gui4.stderr"
if [ -s "$D2" ] && [ -s "$D1" ]; then
  b2=$(tail -n +2 "$D2"); b1=$(tail -n +2 "$D1")
  echo "$b2" | grep "현재 스키마\|Current schema" | sed 's/^/        tab2> /' | tee -a "$REPORT" >/dev/null
  echo "$b1" | grep "현재 스키마\|Current schema" | sed 's/^/        tab1> /' | tee -a "$REPORT" >/dev/null
  cur=$(echo "$b2" | grep -m1 "현재 스키마\|Current schema" | sed 's/.*: *//')
  [ -n "$cur" ] && [ "$cur" != "$OTHER" ] && ok "④ 새 탭(탭2) = 연결 기본 스키마($cur)로 조용히 복귀" || bad "④ 새 탭 스키마 = $cur(기본값이어야)" "$b2"
  echo "$b1" | grep -q "현재 스키마: $OTHER\|Current schema: $OTHER" && ok "④ 탭1로 돌아가면 $OTHER로 조용히 복귀(실행 없이)" || bad "④ 탭1 복귀 스키마" "$b1"
else bad "④ 덤프 없음(tab1 $([ -s "$D1" ] && echo o || echo x) · tab2 $([ -s "$D2" ] && echo o || echo x))" "$(tail -3 "$OUT/gui4.stderr")"; fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
