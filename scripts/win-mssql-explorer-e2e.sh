#!/usr/bin/env bash
# win-mssql-explorer-e2e.sh — SQL Server 탐색기 SSMS 골격 E2E(10-01 · docs/101 · T-270 · T-271).
#   격리 홈에 실제 프로필을 복사해 GUI를 기동 명령으로만 몬다(키 주입 0 · docs/61 §4).
#   ① 접속 직후 `explorer.dump` = 루트 아래 [데이터베이스 ▸ 시스템 데이터베이스 · 사용자 DB…] + [서버 개체] · 현재 DB "(현재)" + 자동 펼침(테이블 · 뷰 · … 보안)
#   ② 편집기 `USE <다른 DB>` F5 → 현재 DB 표식 이동 · 그 DB 펼침 · 테이블 폴더 펼침 = `스키마.이름` 객체 · 둘째 DB 테이블은 다른 DB의 것(`db` 전환)
#   ③ `SHOW CONN` = "현재 DB" 줄 · 서버 헤더 = `(SQL Server 버전 - 로그인)` 은 헤더 덤프가 없어 단위 시험(`server_label`)에 맡긴다
#   ④ 연결된 서버(T-271): 임시 NSQLT_LNK(권한 없으면 SKIP) → 트리 · CLI 소스/상세 · 삭제 · 권한 없어도 폴더 (0) 읽기
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
LNK_FOLDER='\|folder\|(Linked Servers|연결된 서버) \([0-9]+\)\|'

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

# ④ 연결된 서버(T-271 · 101 §5): 임시 NSQLT_LNK(자기 자신 루프백 · 권한 없으면 건너뜀) → 탐색기 서버 개체 ▸ 연결된 서버 · CLI 소스/상세 · 삭제.
#   `sp_serveroption`은 트랜잭션 안에서 못 돈다 → `SET AUTOCOMMIT ON`. 기동 명령은 `@after:`만(= `@connected:@after:`는 안 된다).
NSQLCLI="${NSQLCLI:-$ROOT/target/debug/nsql.exe}"
dump_linked() {
  D4="$OUT/tree4.txt"; rm -f "$D4"
  NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@after:7500:explorer.expand:서버 개체,@after:9000:explorer.expand:연결된 서버,@after:12500:explorer.dump:$D4" \
    timeout -s KILL 16 "$EXE" "$PROFILE" > "$OUT/gui4.stdout" 2> "$OUT/gui4.stderr"
  [ -s "$D4" ]
}
if [ -x "$NSQLCLI" ]; then
  say "--- ④ 연결된 서버(임시 NSQLT_LNK)"
  L="$OUT/lnk_create.sql"
  printf "SET AUTOCOMMIT ON\nEXEC master.dbo.sp_addlinkedserver @server = N'NSQLT_LNK', @srvproduct = N'', @provider = N'MSOLEDBSQL', @datasrc = N'127.0.0.1,1433', @catalog = N'master'\nGO\nEXEC master.dbo.sp_addlinkedsrvlogin @rmtsrvname = N'NSQLT_LNK', @useself = N'True', @locallogin = NULL\nGO\nEXEC master.dbo.sp_serveroption @server = N'NSQLT_LNK', @optname = N'rpc out', @optvalue = N'true'\nGO\n" > "$L"
  if "$NSQLCLI" run -c "$PROFILE" "$L" > "$OUT/lnk_create.out" 2>&1; then
    if dump_linked; then
      t4=$(cat "$D4"); echo "$t4" | grep -E "ServerObjects|Linked|연결된|NSQLT_LNK" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
      chk "④ 서버 개체 ▸ 연결된 서버 폴더 읽힘" "$LNK_FOLDER" "$t4"
      chk "④ NSQLT_LNK 노드(제품·데이터 원본 흐린 글)" "\|object\|NSQLT_LNK\|" "$t4"
    else bad "④ 트리 덤프 없음" "$(tail -3 "$OUT/gui4.stderr")"; fi
    o=$("$NSQLCLI" cat -c "$PROFILE" source linked_server NSQLT_LNK 2>&1); echo "$o" | head -8 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
    chk "④ 소스 = 재생성 스크립트(sp_addlinkedserver)" "sp_addlinkedserver @server = N'NSQLT_LNK', @srvproduct = N'', @provider = N'MSOLEDBSQL', @datasrc = N'127.0.0.1,1433', @catalog = N'master'" "$o"
    chk "④ 소스 = 로그인 매핑(자기 자격)" "sp_addlinkedsrvlogin @rmtsrvname = N'NSQLT_LNK', @useself = N'True', @locallogin = NULL" "$o"
    chk "④ 소스 = 옵션 rpc out" "@optname = N'rpc out', @optvalue = N'true'" "$o"
    chk "④ 소스 = DROP 줄은 주석(보수적 생성)" "^-- EXEC master.dbo.sp_dropserver" "$o"
    o=$("$NSQLCLI" cat -c "$PROFILE" detail NSQLT_LNK linked_server 2>&1); echo "$o" | head -6 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
    chk "④ 상세 = 공급자·데이터 원본" "MSOLEDBSQL" "$o"
    chk "④ 상세 = 로그인 매핑 절" "로그인 매핑|Login mappings|all logins" "$o"
    printf "SET AUTOCOMMIT ON\nEXEC master.dbo.sp_dropserver @server = N'NSQLT_LNK', @droplogins = 'droplogins'\nGO\n" > "$OUT/lnk_drop.sql"
    "$NSQLCLI" run -c "$PROFILE" "$OUT/lnk_drop.sql" > "$OUT/lnk_drop.out" 2>&1 && ok "④ 임시 연결된 서버 삭제(sp_dropserver droplogins)" || bad "④ 임시 연결된 서버 삭제 실패" "$(cat "$OUT/lnk_drop.out")"
  else
    say "  SKIP  ④ 연결된 서버 만들기 실패(권한 없음 = 정상 · ALTER ANY LINKED SERVER) — $(tail -1 "$OUT/lnk_create.out")"
    # 권한이 없어도 목록 읽기(`sys.servers`)는 된다 → 폴더가 (0)으로 읽히는지.
    if dump_linked; then
      t4=$(cat "$D4"); echo "$t4" | grep -E "ServerObjects|Linked|연결된" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
      chk "④ 서버 개체 ▸ 연결된 서버 폴더 읽힘(없으면 (0))" "$LNK_FOLDER" "$t4"
    else bad "④ 트리 덤프 없음" "$(tail -3 "$OUT/gui4.stderr")"; fi
  fi
fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
