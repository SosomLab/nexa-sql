#!/usr/bin/env bash
# output-flush-e2e.sh — ★ GUI Output 탭 **실행 중 서버 메시지 실시간 표시** E2E(130차 mac · 10-17 · 사용자 지시 "flush 메시지 실시간 Output 시험" ·
#   협업 세션 target/flush-demo 사본을 저장소 자산으로 · docs/32 서버 메시지·실행 중 로그).
#   방언마다 임시 프로시저 NSQLT_FLUSH_DEMO(메시지 5줄 · 1 s 간격 · 끝 2 s)를 만들고 GUI에서 실행 → 1 s마다 `output.dump` → 덤프별 줄 수로 판정 → 프로시저 삭제.
#   | 방언 | 수단 | 기대 |
#   | SQL Server | RAISERROR … WITH NOWAIT | 실행 중 한 줄씩 누적(실시간) |
#   | PostgreSQL | RAISE NOTICE(NoticeResponse) | 실시간 |
#   | Oracle(Thin·OCI) | DBMS_OUTPUT = 서버 버퍼 · 호출 뒤 GET_LINE | 끝난 뒤 6줄 일괄(구조상 한계 · 결함 아님) |
#   | SQLite | 서버 메시지 없음 · 클라이언트 PROMPT만 | 문장 사이 PROMPT 즉시 · 무거운 질의 중엔 수단 없음 |
#   격리 홈 + 실제 프로필 **사본**(-P · 비밀번호는 봉투 그대로) · 키 주입 0 · NSQL_NO_ACTIVATE · 내가 띄운 PID만 종료 · 실서버 = NSQLT_* 임시 객체만(DROP은 -x).
# 사용: scripts/output-flush-e2e.sh -o <출력> [-P <실제 설정 폴더>] [-m <SQL Server 프로필>] [-p <PG 프로필>] [-r <Oracle 프로필>] [-s] [-x] [-e app] [-n nsql]
#   -s = SQLite(프로필을 스크립트가 만든다 · -P 불필요) · 서버 케이스는 -P 필수 · 예: -P ~/Library/Application\ Support/nexa-sql -m M4PLAN -p Repository -r BISCM -s
#   -x = 끝에 임시 프로시저 DROP(기본 = **남긴다** · 사용자가 직접 실행해 볼 수 있게 · 10-17 협업 세션이 사용자에게 알린 상태와 같게 · NSQLT_* = 61 §2-4 ⑤ 임시 객체)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXE="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"; OUT=""; PROF=""; MS=""; PG=""; ORA=""; LITE=0; DROP=0
while getopts "o:P:m:p:r:sxe:n:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; m) MS=$OPTARG;; p) PG=$OPTARG;; r) ORA=$OPTARG;; s) LITE=1;; x) DROP=1;; e) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out> [-P <settings dir>] [-m prof] [-p prof] [-r prof] [-s]"; exit 2; }
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) [ -x "$NSQL" ] || NSQL="$NSQL.exe"; [ -x "$EXE" ] || EXE="$EXE.exe";; esac
[ -x "$EXE" ] && [ -x "$NSQL" ] || { echo "앱 없음: $EXE / $NSQL"; exit 2; }
{ [ -n "$MS$PG$ORA" ] && [ -z "$PROF" ]; } && { echo "서버 케이스는 -P <실제 설정 폴더>가 필요합니다"; exit 2; }
case "$OUT" in /*) ;; *) OUT="$ROOT/$OUT";; esac
mkdir -p "$OUT"; D="$OUT/sql"; mkdir -p "$D"
REPORT="$OUT/output-flush-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); }
mtime() { stat -f %m "$1" 2>/dev/null || stat -c %Y "$1" 2>/dev/null; }

# ── 방언별 SQL(생성 · 실행 · 삭제) ────────────────────────────────────────────────────────────────
cat > "$D/mssql-create.sql" <<'SQL'
CREATE OR ALTER PROCEDURE dbo.NSQLT_FLUSH_DEMO @n INT = 5, @gap_sec INT = 1, @tail_sec INT = 2 AS
BEGIN
    SET NOCOUNT ON;
    DECLARE @i INT = 1, @t VARCHAR(30), @d CHAR(8);
    WHILE @i <= @n
    BEGIN
        SET @t = CONVERT(VARCHAR(30), SYSDATETIME(), 121);
        RAISERROR('NSQLT flush %d/%d at %s', 0, 1, @i, @n, @t) WITH NOWAIT;   -- 심각도 0 + NOWAIT = 즉시 전송(PRINT는 버퍼링)
        SET @d = CONVERT(CHAR(8), DATEADD(SECOND, @gap_sec, 0), 108);
        WAITFOR DELAY @d;
        SET @i += 1;
    END
    SET @d = CONVERT(CHAR(8), DATEADD(SECOND, @tail_sec, 0), 108);
    WAITFOR DELAY @d;
    SET @t = CONVERT(VARCHAR(30), SYSDATETIME(), 121);
    RAISERROR('NSQLT flush done at %s', 0, 1, @t) WITH NOWAIT;
END
GO
SQL
printf 'EXEC dbo.NSQLT_FLUSH_DEMO;\nGO\n' > "$D/mssql-run.sql"
printf 'DROP PROCEDURE IF EXISTS dbo.NSQLT_FLUSH_DEMO;\nGO\n' > "$D/mssql-drop.sql"
cat > "$D/oracle-create.sql" <<'SQL'
CREATE OR REPLACE PROCEDURE NSQLT_FLUSH_DEMO(p_n IN NUMBER DEFAULT 5, p_gap_sec IN NUMBER DEFAULT 1, p_tail_sec IN NUMBER DEFAULT 2) AS
BEGIN
    FOR i IN 1 .. p_n LOOP
        DBMS_OUTPUT.PUT_LINE('NSQLT flush ' || i || '/' || p_n || ' at ' || TO_CHAR(SYSTIMESTAMP, 'YYYY-MM-DD HH24:MI:SS.FF3'));
        DBMS_SESSION.SLEEP(p_gap_sec);
    END LOOP;
    DBMS_SESSION.SLEEP(p_tail_sec);
    DBMS_OUTPUT.PUT_LINE('NSQLT flush done at ' || TO_CHAR(SYSTIMESTAMP, 'YYYY-MM-DD HH24:MI:SS.FF3'));
END;
/
SQL
printf 'SET SERVEROUTPUT ON\nEXEC NSQLT_FLUSH_DEMO\n' > "$D/oracle-run.sql"
printf 'DROP PROCEDURE NSQLT_FLUSH_DEMO\n' > "$D/oracle-drop.sql"
cat > "$D/pg-create.sql" <<'SQL'
CREATE OR REPLACE PROCEDURE public.nsqlt_flush_demo(p_n int DEFAULT 5, p_gap_sec int DEFAULT 1, p_tail_sec int DEFAULT 2) LANGUAGE plpgsql AS $$
BEGIN
    FOR i IN 1 .. p_n LOOP
        RAISE NOTICE 'NSQLT flush %/% at %', i, p_n, clock_timestamp();
        PERFORM pg_sleep(p_gap_sec);
    END LOOP;
    PERFORM pg_sleep(p_tail_sec);
    RAISE NOTICE 'NSQLT flush done at %', clock_timestamp();
END $$;
SQL
printf 'CALL public.nsqlt_flush_demo();\n' > "$D/pg-run.sql"
printf 'DROP PROCEDURE IF EXISTS public.nsqlt_flush_demo;\n' > "$D/pg-drop.sql"   # 인자 없이 이름으로(시그니처 (int,int,int) · '()'는 일치 안 함 · 10-17)
cat > "$D/sqlite-run.sql" <<'SQL'
PROMPT NSQLT flush 1/2 (client)
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 3000000) SELECT count(*) FROM c;
PROMPT NSQLT flush 2/2 (client)
SQL

# ── 케이스 실행 ────────────────────────────────────────────────────────────────────────────────────
# <이름> <프로필> <방언> [oracle 드라이버] → $OUT/out-<이름>/o2..o16.txt · 판정은 호출자
case_run() {
  local n=$1 prof=$2 dia=$3 drv=${4:-}; local H="$OUT/g-$n" O="$OUT/out-$n"; rm -rf "$H" "$O"; mkdir -p "$H" "$O"
  if [ "$dia" = sqlite ]; then
    printf 'ui.lang=en\ndemo.prompted=on\noutput.auto_show=on\n' > "$H/settings.conf"
    NSQL_HOME="$H" "$NSQL" conn add "$prof" "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  else
    cp -R "$PROF/profiles" "$H/" 2>/dev/null; cp "$PROF/device.key" "$H/" 2>/dev/null
    printf 'ui.lang=en\ndemo.prompted=on\noutput.auto_show=on\n' > "$H/settings.conf"
    [ -n "$drv" ] && echo "oracle.driver=$drv" >> "$H/settings.conf"
    NSQL_HOME="$H" "$NSQL" run -c "$prof" --no-prompt "$D/$dia-create.sql" > "$O/create.out" 2>&1 || { say "  (생성 실패 · $O/create.out)"; }
  fi
  local C="@connected:open:$D/$dia-run.sql,@connected:@after:2000:run.all" s
  for s in $(seq 2 16); do C="$C,@connected:@after:$((s*1000+500)):output.dump:$O/o$s.txt"; done
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$C" "$EXE" "$prof" > "$O/stdout" 2> "$O/stderr" & local p=$!
  sleep 24; kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
  if [ "$dia" != sqlite ] && [ "$DROP" = 1 ]; then NSQL_HOME="$H" "$NSQL" run -c "$prof" --no-prompt "$D/$dia-drop.sql" > "$O/drop.out" 2>&1 || say "  (삭제 실패 · $O/drop.out)"; fi
  # 표 = t(첫 덤프 기준 초) · flush 줄 수 · done 유무
  local t0="" x mt
  : > "$O/table.txt"
  for s in $(seq 2 16); do
    x="$O/o$s.txt"; [ -f "$x" ] || continue
    mt=$(mtime "$x"); [ -z "$t0" ] && t0=$mt
    printf '%s %s %s\n' "$((mt - t0))" "$(grep -c 'NSQLT flush [0-9]' "$x")" "$(grep -q 'NSQLT flush done' "$x" && echo Y || echo -)" >> "$O/table.txt"
  done
  say "-- $n ($prof${drv:+ · $drv}): $(awk '{printf "+%ss=%s%s ", $1, $2, ($3=="Y"?"✓":"")}' "$O/table.txt")"
  grep -qi panick "$O/stderr" && bad "$n stderr 패닉" || true
}
# 판정 도우미: 덤프 표에서 "줄 1~4개 + done 없음"(= 실행 중 부분 표시)이 있었는가 · 마지막 덤프 줄 수
partial_seen() { awk '$2>=1 && $2<=4 && $3=="-" {f=1} END {exit !f}' "$1"; }
last_lines()  { tail -1 "$1" | awk '{print $2}'; }
last_done()   { tail -1 "$1" | awk '{print $3}'; }

if [ -n "$MS" ]; then
  case_run mssql "$MS" mssql; T="$OUT/out-mssql/table.txt"
  partial_seen "$T" && ok "SQL Server: 실행 중 한 줄씩 표시(RAISERROR WITH NOWAIT)" || bad "SQL Server: 부분 표시 없음"
  [ "$(last_lines "$T")" = 5 ] && [ "$(last_done "$T")" = Y ] && ok "SQL Server: 끝 = 5줄 + done" || bad "SQL Server: 끝 상태 $(tail -1 "$T")"
fi
if [ -n "$PG" ]; then
  case_run pg "$PG" pg; T="$OUT/out-pg/table.txt"
  partial_seen "$T" && ok "PostgreSQL: 실행 중 한 줄씩 표시(RAISE NOTICE)" || bad "PostgreSQL: 부분 표시 없음"
  [ "$(last_lines "$T")" = 5 ] && [ "$(last_done "$T")" = Y ] && ok "PostgreSQL: 끝 = 5줄 + done" || bad "PostgreSQL: 끝 상태 $(tail -1 "$T")"
fi
if [ -n "$ORA" ]; then
  for drv in thin oci; do
    case_run "ora_$drv" "$ORA" oracle "$drv"; T="$OUT/out-ora_$drv/table.txt"
    partial_seen "$T" && bad "Oracle $drv: 부분 표시가 있음(DBMS_OUTPUT 구조와 다름 · 확인)" || ok "Oracle $drv: 실행 중 표시 없음 = 호출 뒤 일괄(구조상 · 결함 아님)"
    [ "$(last_lines "$T")" = 5 ] && [ "$(last_done "$T")" = Y ] && ok "Oracle $drv: 끝 = 5줄 + done(DBMS_OUTPUT 회수)" || bad "Oracle $drv: 끝 상태 $(tail -1 "$T")"
  done
fi
if [ "$LITE" = 1 ]; then
  case_run sqlite Local sqlite; T="$OUT/out-sqlite/table.txt"
  [ "$(head -1 "$T" | awk '{print $2}')" -ge 1 ] 2>/dev/null && ok "SQLite: 첫 PROMPT가 무거운 질의 전에 즉시 표시" || bad "SQLite: 첫 덤프 $(head -1 "$T")"
  [ "$(last_lines "$T")" = 2 ] && ok "SQLite: 끝 = PROMPT 2줄(서버 메시지 경로 없음)" || bad "SQLite: 끝 상태 $(tail -1 "$T")"
fi
say "== 결과: PASS $pass · FAIL $fail · 보고 $REPORT"
[ "$fail" = 0 ]
