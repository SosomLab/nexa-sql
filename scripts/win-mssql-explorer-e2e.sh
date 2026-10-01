#!/usr/bin/env bash
# win-mssql-explorer-e2e.sh — SQL Server 탐색기 SSMS 골격 E2E(10-01 · docs/101 · T-270).
#   격리 홈에 실제 프로필을 복사해 GUI를 기동 명령으로만 몬다(키 주입 0 · docs/61 §4).
#   ① 접속 직후 `explorer.dump` = 루트 아래 [데이터베이스 ▸ 시스템 데이터베이스 · 사용자 DB…] + [서버 개체] · 현재 DB "(현재)" + 자동 펼침(테이블 · 뷰 · … 보안)
#   ② 편집기 `USE <다른 DB>` F5 → 현재 DB 표식 이동 · 그 DB 펼침 · 테이블 폴더 펼침 = `스키마.이름` 객체 · 둘째 DB 테이블은 다른 DB의 것(`db` 전환)
#   ③ `SHOW CONN` = "현재 DB" 줄 · 서버 헤더 = `(SQL Server 버전 - 로그인)` 은 헤더 덤프가 없어 단위 시험(`server_label`)에 맡긴다.
#
# 사용: scripts/win-mssql-explorer-e2e.sh -o <출력폴더> -g target/debug/nexa-sql.exe [-p M4PLAN] [-d BISCM_MS] [-P <실제 설정 폴더>]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; PROFILE="M4PLAN"; OTHER="BISCM_MS"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
while getopts "o:g:p:d:P:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; p) PROFILE=$OPTARG;; d) OTHER=$OPTARG;; P) PROF=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui-exe] [-p profile] [-d other-db] [-P real-config-dir]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/mssql-explorer-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk() { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }

say "=== SQL Server 탐색기 SSMS 골격 E2E · 프로필 $PROFILE · 다른 DB $OTHER · $(date '+%F %T')"
F="$OUT/use.sql"; printf 'USE %s\nGO\nSHOW CONN\n' "$OTHER" > "$F"; fw=$(cygpath -w "$F" 2>/dev/null || echo "$F")
D1="$OUT/tree1.txt"; D2="$OUT/tree2.txt"; D3="$OUT/tree3.txt"; O1="$OUT/output.txt"; rm -f "$D1" "$D2" "$D3" "$O1"
# 타임라인: 접속 → 6초 트리 덤프 ① → USE 실행 → 11초 덤프 ② → 테이블 폴더(그 DB · 첫 폴더) 펼침 → 15초 덤프 ③ + Output 덤프.
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw,@after:6000:explorer.dump:$D1,@after:6500:run.all,@after:11000:explorer.dump:$D2,@after:11500:explorer.expand:$OTHER/Tables,@after:15500:explorer.dump:$D3,@after:16000:output.dump:$O1" \
  timeout -s KILL 20 "$EXE" "$PROFILE" > "$OUT/gui.stdout" 2> "$OUT/gui.stderr"
if [ -s "$D1" ]; then
  t1=$(cat "$D1"); echo "$t1" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "① 루트 아래 '데이터베이스' 묶음" "\|group:Databases\|데이터베이스\|" "$t1"
  chk "① '시스템 데이터베이스' 묶음(D-251)" "group:SystemDbs" "$t1"
  chk "① '서버 개체' 묶음" "group:ServerObjects" "$t1"
  chk "① 현재 DB 표식 '(현재)'" "\|database\|[A-Za-z_]+\|\(현재\)\|" "$t1"
  chk "① 현재 DB 아래 SSMS 폴더(테이블 · 뷰 · 외부 리소스 · 동의어 · 프로그래밍 기능 · 보안)" "group:Programmability" "$t1"
  chk "① 사용자 DB $OTHER 노드" "\|database\|$OTHER\|" "$t1"
  cur=$(echo "$t1" | grep "(현재)" | head -1 | cut -d'|' -f3)
  # 프로필 Database가 비어 있으면 연결 행의 주 글자 = 서버가 알려 준 현재 DB(101 §3) · 채워져 있으면 흐린 글자에 "현재 DB: x".
  if echo "$t1" | grep -qE "^0\|root\|$cur\||현재 DB: $cur"; then ok "① 연결 행에 현재 DB($cur)"; else bad "① 연결 행에 현재 DB($cur)" "$(head -1 "$D1")"; fi
else bad "① 트리 덤프 없음(기동·접속 실패?)" "$(tail -3 "$OUT/gui.stderr")"; fi
if [ -s "$D2" ]; then
  t2=$(cat "$D2"); echo "--- ②" | tee -a "$REPORT" >/dev/null; echo "$t2" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "② USE 뒤 현재 DB 표식 = $OTHER" "\|database\|$OTHER\|\(현재\)\|" "$t2"
  n=$(echo "$t2" | grep -c "(현재)"); [ "$n" -eq 1 ] && ok "② '(현재)' 표식은 하나" || bad "② '(현재)' 표식 수 $n" "$t2"
  chk "② $OTHER 펼침 = 프로그래밍 기능 묶음 보임" "group:Programmability" "$t2"
else bad "② 트리 덤프 없음" ""; fi
if [ -s "$D3" ]; then
  t3=$(cat "$D3"); echo "--- ③" | tee -a "$REPORT" >/dev/null; echo "$t3" | grep -E "folder\|Tables|\|object\|" | head -8 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "③ 테이블 폴더 읽힘(Tables (n))" "\|folder\|Tables \([0-9]+\)\|" "$t3"
  chk "③ 객체 = 스키마.이름(SSMS)" "\|object\|[A-Za-z_]+\.[A-Za-z_]+\|" "$t3"
else bad "③ 트리 덤프 없음" ""; fi
if [ -s "$O1" ]; then
  o=$(tail -n +2 "$O1"); echo "$o" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "Output: 데이터베이스 컨텍스트 변경 메시지(서버)" "데이터베이스 컨텍스트가|Changed database context" "$o"
  chk "Output: SHOW CONN 현재 DB 줄 = $OTHER" "현재 DB: $OTHER|Current database: $OTHER" "$o"
else bad "Output 덤프 없음" ""; fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
