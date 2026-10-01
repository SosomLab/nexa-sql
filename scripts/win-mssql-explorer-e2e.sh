#!/usr/bin/env bash
# win-mssql-explorer-e2e.sh — SQL Server 탐색기 SSMS 골격 E2E(10-01 · docs/101 · T-270 · T-271).
#   격리 홈에 실제 프로필을 복사해 GUI를 기동 명령으로만 몬다(키 주입 0 · docs/61 §4).
#   ① 접속 직후 `explorer.dump` = 루트 아래 [데이터베이스 ▸ 시스템 데이터베이스 · 사용자 DB…] + [서버 개체] · 현재 DB "(현재)" + 자동 펼침(테이블 · 뷰 · … 보안)
#   ② 편집기 `USE <다른 DB>` F5 → 현재 DB 표식 이동 · 그 DB 펼침 · 테이블 폴더 펼침 = `스키마.이름` 객체 · 둘째 DB 테이블은 다른 DB의 것(`db` 전환)
#   ③ `SHOW CONN` = "현재 DB" 줄 · 서버 헤더 = `(SQL Server 버전 - 로그인)` 은 헤더 덤프가 없어 단위 시험(`server_label`)에 맡긴다
#   ④ 연결된 서버(T-271): 임시 NSQLT_LNK(권한 없으면 SKIP) → 트리 · CLI 소스/상세 · 삭제 · 권한 없어도 폴더 (0) 읽기
#   ⑤ 서버 보안·서버 개체(㉖): 루트 묶음 셋 · `/보안` 펼침 · 로그인/서버 역할/엔드포인트 · CLI 상세·목록 · ⑥ DB 용량(㉕ `explorer.dbsizes`)
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
  chk "③ 테이블 옆 용량(㉕-c · 끝 열)" "\|object\|dbo\.[^|]*\|[^|]*\|[^|]*\|[0-9.]+[KMGTP]?B?$" "$t3"
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
# ⑤ 서버 수준 보안·서버 개체(10-01 ㉖ · 101 §6): 루트 = [데이터베이스 · 보안 · 서버 개체] · `/보안` = 서버 수준만(DB 아래 "보안"과 구분) ·
#    로그인 폴더 = 로그인 ≥ 1 · 서버 역할 = 고정 역할(sysadmin …) · 서버 개체 = 백업 디바이스 · 엔드포인트(≥ 1 · TSQL Default TCP) · 연결된 서버 · 트리거.
# ⑥ DB 용량(㉕): `explorer.dbsizes`(= 우클릭 "용량 확인") 뒤 덤프 끝 열 = DB 노드 용량 · 데이터베이스 묶음 = 합.
say "--- ⑤ 서버 보안·서버 개체 · ⑥ DB 용량"
D5="$OUT/tree5.txt"; rm -f "$D5"
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@after:9000:explorer.expand:/보안,@after:10500:explorer.expand:로그인,@after:12000:explorer.expand:서버 역할,@after:13500:explorer.expand:/서버 개체,@after:15000:explorer.expand:엔드포인트,@after:16000:explorer.dbsizes,@after:20000:explorer.dump:$D5" \
  timeout -s KILL 23 "$EXE" "$PROFILE" > "$OUT/gui5.stdout" 2> "$OUT/gui5.stderr"
if [ -s "$D5" ]; then
  t5=$(cat "$D5"); echo "$t5" | grep -E "group:|folder\|(로그인|서버 역할|자격 증명|암호화|감사|백업|엔드포인트|연결된 서버|트리거)|object\|(sa|sysadmin|TSQL)|database\|" | head -40 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "⑤ 루트 = 서버 보안 묶음(데이터베이스와 같은 층)" "^1\|group:ServerSecurity\|(보안|Security)\|" "$t5"
  chk "⑤ 루트 = 서버 개체 묶음" "^1\|group:ServerObjects\|(서버 개체|Server Objects)\|" "$t5"
  chk "⑤ 보안 ▸ 폴더 6(로그인 · 서버 역할 · 자격 증명 · 암호화 공급자 · 감사 · 서버 감사 사양)" "^2\|folder\|(자격 증명|Credentials)" "$t5"
  chk "⑤ 로그인 폴더 읽힘(≥ 1)" "^2\|folder\|(로그인|Logins) \([1-9][0-9]*\)\|" "$t5"
  chk "⑤ 서버 역할 = 고정 역할 sysadmin(FIXED)" "^3\|object\|sysadmin\|FIXED" "$t5"
  chk "⑤ 서버 개체 ▸ 폴더 4(백업 디바이스 · 엔드포인트 · 연결된 서버 · 트리거)" "^2\|folder\|(백업 디바이스|Backup Devices)" "$t5"
  chk "⑤ 엔드포인트 읽힘(TSQL Default TCP)" "^3\|object\|TSQL Default TCP\|" "$t5"
  chk "⑥ DB 노드 용량(끝 열 · master)" "^[23]\|database\|master\|[^|]*\|[^|]*\|[0-9.]+[KMGTP]?B?$" "$t5"
  chk "⑥ 데이터베이스 묶음 = 합(끝 열)" "^1\|group:Databases\|[^|]*\|[^|]*\|[^|]*\|[0-9.]+[KMGTP]?B?$" "$t5"
else bad "⑤ 트리 덤프 없음" "$(tail -3 "$OUT/gui5.stderr")"; fi
if [ -x "$NSQLCLI" ]; then
  o=$("$NSQLCLI" cat -c "$PROFILE" detail sysadmin server_role 2>&1); echo "$o" | head -6 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "⑤ CLI 상세 = 서버 역할 sysadmin 속성(type_desc SERVER_ROLE · members)" "SERVER_ROLE" "$o"
  o=$("$NSQLCLI" cat -c "$PROFILE" logins 2>&1); echo "$o" | head -4 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "⑤ CLI 목록 = 로그인(SQL_LOGIN)" "SQL_LOGIN|WINDOWS_LOGIN" "$o"
fi
# ⑦ 현재 DB가 master인 채 다른 DB(BISCM_MS)의 Tables를 펼쳐도 용량(㉕-d · `is_cur` 조건 제거).
say "--- ⑦ 현재 DB 아닌 DB의 테이블 용량"
D7="$OUT/tree7.txt"; rm -f "$D7"
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@after:7000:explorer.expand:$OTHER,@after:8500:explorer.expand:$OTHER/Tables,@after:14000:explorer.dump:$D7" \
  timeout -s KILL 17 "$EXE" "$PROFILE" > "$OUT/gui7.stdout" 2> "$OUT/gui7.stderr"
if [ -s "$D7" ]; then
  t7=$(cat "$D7"); echo "$t7" | grep -E "\|object\|dbo\." | head -3 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "⑦ 현재 DB(master) 아닌 $OTHER 테이블 옆 용량" "\|object\|dbo\.[^|]*\|[^|]*\|[^|]*\|[0-9.]+[KMGTP]?B?$" "$t7"
  chk "⑦ DB 노드 = 읽힌 테이블 합 + \"+\"(인덱스 미읽음 · ㉝)" "\|database\|$OTHER\|[^|]*\|[^|]*\|[0-9.]+[KMGTP]?B?\+$" "$t7"
else bad "⑦ 트리 덤프 없음" "$(tail -3 "$OUT/gui7.stderr")"; fi
# ⑧ 루틴 세부 타입(10-02 ㉛): 저장 프로시저 라벨 = `dbo.이름`(종전 `(P)` 없음) · CLI 상세 `type` 행 = `SQL_STORED_PROCEDURE`.
say "--- ⑧ 루틴 세부 타입"
D8="$OUT/tree8.txt"; rm -f "$D8"
NSQL_NO_ACTIVATE=1 PROCDB="${PROCDB:-M4PLAN_MS}"
NSQL_STARTUP_CMD="@after:7000:explorer.expand:$PROCDB,@after:8500:explorer.expand:$PROCDB/프로그래밍 기능,@after:10000:explorer.expand:$PROCDB/저장 프로시저,@after:14000:explorer.dump:$D8" \
  timeout -s KILL 17 "$EXE" "$PROFILE" > "$OUT/gui8.stdout" 2> "$OUT/gui8.stderr"
if [ -s "$D8" ]; then
  t8=$(cat "$D8"); echo "$t8" | grep -E "\|object\|dbo\.(SP_|PROC)" | head -3 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "⑧ 프로시저 라벨 = dbo.이름(괄호 타입 없음)" "\|object\|dbo\.[A-Za-z0-9_]+\|[^|]*\|[^|]*\|" "$t8"
  echo "$t8" | grep -qE "\|object\|dbo\.[A-Za-z0-9_]+\([A-Z]+\)\|" && bad "⑧ 라벨에 (P) 같은 타입이 남아 있음" "$(echo "$t8" | grep -E '\([A-Z]+\)\|' | head -3)" || ok "⑧ 라벨에 타입 괄호 없음"
  P8=$(echo "$t8" | grep -oE "\|object\|dbo\.[A-Za-z0-9_]+\|" | head -1 | sed 's/|object|dbo\.//; s/|$//')
  if [ -n "$P8" ] && [ -x "$NSQLCLI" ]; then
    # CLI는 접속 DB(master)에서만 보므로 다른 DB의 프로시저 상세는 건너뛴다 — 상세 `type` 행은 단위 시험(detail.rs)과 GUI 실기로.
    o=$("$NSQLCLI" cat -c "$PROFILE" -s dbo detail "$P8" procedure 2>&1)
    if echo "$o" | grep -qE "SQL_STORED_PROCEDURE|CLR_STORED_PROCEDURE"; then ok "⑧ CLI 상세 type = SQL_STORED_PROCEDURE"; else say "  SKIP  ⑧ CLI 상세(접속 DB가 master라 $PROCDB 프로시저 없음)"; fi
  else sk() { :; }; say "  SKIP  ⑧ 프로시저 없음 또는 CLI 없음"; fi
else bad "⑧ 트리 덤프 없음" "$(tail -3 "$OUT/gui8.stderr")"; fi
# ⑨ 테이블 반환 함수 완성(10-02 ㉜): `USE M4PLAN_MS` 뒤 `SELECT * FROM |`에 `FN_TABLE_CO`(SQL_INLINE_TABLE_VALUED_FUNCTION)가 **테이블 함수**로.
say "--- ⑨ FROM 자리 테이블 반환 함수 완성"
TVFDB="${TVFDB:-M4PLAN_MS}"; TVF="${TVF:-FN_TABLE_CO}"
# 같은 탭에서(새 탭은 ⑯ 탭별 작업 단위로 연결 기본 DB(master)로 되돌아간다) — USE + SELECT 한 파일 · 캐럿 끝 = `FROM ` 뒤.
F9="$OUT/use9.sql"; printf 'USE %s\nGO\nSELECT * FROM ' "$TVFDB" > "$F9"; D9="$OUT/cands9.txt"; rm -f "$D9"
fw9=$(cygpath -w "$F9" 2>/dev/null || echo "$F9")
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw9,@after:5000:run.all,@after:9000:editor.caret:end,@after:9500:intel.probe,@after:12000:intel.dump:$D9" \
  timeout -s KILL 15 "$EXE" "$PROFILE" > "$OUT/gui9.stdout" 2> "$OUT/gui9.stderr"
if [ -s "$D9" ]; then
  c9=$(cat "$D9"); echo "$c9" | grep -iE "\|$TVF\|" | head -2 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  chk "⑨ FROM 완성에 $TVF 있음" "\|$TVF\|" "$c9"
  chk "⑨ $TVF = 테이블 함수로 판정(detail)" "\|$TVF\|table function" "$c9"
else bad "⑨ 완성 덤프 없음" "$(tail -3 "$OUT/gui9.stderr")"; fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
