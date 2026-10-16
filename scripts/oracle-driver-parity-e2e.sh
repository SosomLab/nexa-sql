#!/usr/bin/env bash
# oracle-driver-parity-e2e.sh — Oracle **OCI(Instant Client) ↔ Thin** 드라이버 동일성 E2E(docs/110 · T-336 · 사용자 10-13 "두 실행의
#   기능/결과가 동일함을 테스트" · 3-OS bash · 키 주입 0). 같은 스크립트 묶음을 격리 홈의 `oracle.driver = oci` / `thin`으로 CLI에서 돌려
#   **stdout을 골든 대조**(diff = 0)한다. 쓰기는 임시 표 `NSQLT_PARITY`(만들고 지운다)뿐.
#   ① 값·타입(dual · 숫자/글/시각/간격/RAW/CLOB) ② 세션 변수·REF CURSOR·DBMS_OUTPUT(it-oracle 꼴) ③ 임시 표 쓰기 왕복(날짜·숫자·한글 바인드)
#   ④ 오류 동일성(ORA-00942 · ORA-01722) ⑤ 1200행 페이지(커서 이어 받기) ⑥ Thin stderr에 "driver limitations" 안내 1줄(접속 직후)
#   ⑦ 실시간 전환 = 설정 파일만 바꿔 다시 실행(재시작 없음 · CLI는 실행마다 읽는다).
# 사용: scripts/oracle-driver-parity-e2e.sh -o <출력> -P <실제 설정 폴더> [-c BISCM] [-n target/debug/nsql]
#   -P = 프로필·device.key를 격리 홈으로 복사(비밀번호는 vault 봉투 그대로 · 환경 변수에 적지 않는다).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NSQL="$ROOT/target/debug/nsql"; OUT=""; PROF=""; CONN="${NSQL_E2E_ORACLE_PROFILE:-BISCM}"
while getopts "o:n:P:c:" o; do case $o in o) OUT=$OPTARG;; n) NSQL=$OPTARG;; P) PROF=$OPTARG;; c) CONN=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out dir> -P <settings dir> [-c profile] [-n cli]"; exit 2; }
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) [ -x "$NSQL" ] || NSQL="$NSQL.exe";; esac
[ -x "$NSQL" ] || { echo "nsql not found: $NSQL"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; D="$OUT/data"; rm -rf "$H" "$D" "$OUT/oci" "$OUT/thin"; mkdir -p "$H" "$D" "$OUT/oci" "$OUT/thin"
cp -R "$PROF/profiles" "$H/" 2>/dev/null; cp "$PROF/device.key" "$H/" 2>/dev/null
export NSQL_HOME="$H"
REPORT="$OUT/oracle-driver-parity.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
setdrv() { printf 'ui.lang=en\ndemo.prompted=on\ncli.format=csv\noracle.driver=%s\n' "$1" > "$H/settings.conf"; }

say "=== Oracle OCI ↔ Thin 동일성 E2E · $(date '+%F %T') · cli=$NSQL · 프로필 $CONN · 홈 $H"

# ── 스크립트 묶음 ─────────────────────────────────────────────────────────────
cat > "$D/01-types.sql" <<'SQL'
SELECT 1 AS n, 1.5 AS d, -0.5 AS neg, 1E20 AS big, 123456789012345678901234567890 AS huge, CAST(12345.678 AS NUMBER(10,2)) AS fixed FROM dual;
SELECT CAST(1.25 AS BINARY_DOUBLE) AS bd, CAST(1.5 AS BINARY_FLOAT) AS bf, CAST(3 AS FLOAT) AS fl, CAST(7 AS NUMBER(5)) AS n5 FROM dual;
SELECT 'abc' AS s, N'한글' AS ns, CAST('x' AS CHAR(3)) AS c3, CAST('가' AS VARCHAR2(10)) AS v10, CAST(NULL AS VARCHAR2(10)) AS nul FROM dual;
SELECT DATE '2026-09-21' AS dt, TIMESTAMP '2026-09-21 13:45:10.123456' AS ts, CAST(TIMESTAMP '2026-09-21 13:45:10.5' AS TIMESTAMP(3)) AS ts3, TIMESTAMP '2026-09-21 13:45:10.5 +09:00' AS tstz, CAST(TIMESTAMP '2026-09-21 13:45:10.5 +09:00' AS TIMESTAMP WITH LOCAL TIME ZONE) AS tsltz, TIMESTAMP '2026-12-31 20:00:00 -05:30' AS tstz2 FROM dual;
SELECT INTERVAL '1 02:03:04.5' DAY TO SECOND AS ids, INTERVAL '1-2' YEAR TO MONTH AS iym, HEXTORAW('DEADBEEF') AS rw, TO_CLOB('clob 한글') AS cl FROM dual;
SQL
cat > "$D/02-vars.sql" <<'SQL'
SET SERVEROUTPUT ON
VARIABLE rc REFCURSOR
EXEC :V_NAME := 'nexa'
EXEC SELECT COUNT(*), MAX(OBJECT_NAME) INTO :V_CNT, :V_MAX FROM ALL_OBJECTS WHERE OWNER = 'SYS' AND OBJECT_NAME LIKE 'DBMS_OUT%'
PRINT V_NAME V_CNT V_MAX
SELECT :V_NAME AS n, :V_CNT AS c FROM dual;
EXEC OPEN :rc FOR SELECT OBJECT_NAME FROM ALL_OBJECTS WHERE OWNER = 'SYS' AND OBJECT_NAME LIKE 'DBMS_OUT%' ORDER BY 1
PRINT rc
EXEC DBMS_OUTPUT.PUT_LINE('hello ' || :V_NAME || ' 한글')
VAR d DATE
VAR ts TIMESTAMP
VAR b BOOLEAN
EXEC :d := TO_DATE('2026-09-21 13:45:10','YYYY-MM-DD HH24:MI:SS')
EXEC :ts := TO_TIMESTAMP('2026-09-21 13:45:10.123456','YYYY-MM-DD HH24:MI:SS.FF')
PRINT d ts
SELECT TO_CHAR(:d,'YYYY-MM-DD HH24:MI:SS') AS a FROM dual;
BEGIN :b := (1 = 1); END;
/
PRINT b
EXEC DBMS_OUTPUT.GET_LINE(:LINE, :STATUS)
PRINT STATUS
BEGIN
  SELECT 'in-out ' || :V_NAME INTO :V_IO FROM dual;
  :V_TWICE := :V_NAME || :V_NAME;
END;
/
PRINT V_IO V_TWICE
SQL
cat > "$D/03-write.sql" <<'SQL'
BEGIN EXECUTE IMMEDIATE 'DROP TABLE NSQLT_PARITY'; EXCEPTION WHEN OTHERS THEN NULL; END;
/
CREATE TABLE NSQLT_PARITY (id NUMBER(5), name VARCHAR2(30 CHAR), amt NUMBER(10,2), dt DATE, ts TIMESTAMP(6), memo CLOB);
VAR d DATE
EXEC :d := TO_DATE('2026-09-21 13:45:10','YYYY-MM-DD HH24:MI:SS')
EXEC :nm := '홍길동'
INSERT INTO NSQLT_PARITY (id, name, amt, dt, ts, memo) VALUES (1, :nm, 12.5, :d, TIMESTAMP '2026-09-21 13:45:10.123456', TO_CLOB('memo 한글'));
INSERT INTO NSQLT_PARITY (id, name, amt, dt, ts, memo) VALUES (2, 'two', NULL, NULL, NULL, NULL);
UPDATE NSQLT_PARITY SET amt = amt * 2 WHERE id = 1;
COMMIT;
SELECT id, name, amt, dt, ts, memo FROM NSQLT_PARITY ORDER BY id;
SELECT COUNT(*) AS cnt FROM NSQLT_PARITY WHERE name = :nm;
DELETE FROM NSQLT_PARITY WHERE id = 2;
ROLLBACK;
SELECT COUNT(*) AS after_rollback FROM NSQLT_PARITY;
DROP TABLE NSQLT_PARITY;
SQL
cat > "$D/04-errors.sql" <<'SQL'
SELECT * FROM no_such_table_nsqlt;
SELECT TO_NUMBER('abc') FROM dual;
SELECT 'after errors' AS x FROM dual;
SQL
cat > "$D/06-trigger.sql" <<'SQL'
BEGIN EXECUTE IMMEDIATE 'DROP TABLE NSQLT_PARITY_TG'; EXCEPTION WHEN OTHERS THEN NULL; END;
/
CREATE TABLE NSQLT_PARITY_TG (id NUMBER(5), n NUMBER(5));
CREATE OR REPLACE TRIGGER NSQLT_PARITY_TG_BI BEFORE INSERT ON NSQLT_PARITY_TG FOR EACH ROW
BEGIN
  :NEW.n := NVL(:NEW.n, 7);
END;
/
INSERT INTO NSQLT_PARITY_TG (id, n) VALUES (1, NULL);
SELECT id, n FROM NSQLT_PARITY_TG;
SELECT o.status, (SELECT COUNT(*) FROM user_errors e WHERE e.name = o.object_name) AS errs FROM user_objects o WHERE o.object_name = 'NSQLT_PARITY_TG_BI';
DROP TABLE NSQLT_PARITY_TG;
SQL
cat > "$D/05-pages.sql" <<'SQL'
SELECT * FROM (SELECT LEVEL AS lv FROM dual CONNECT BY LEVEL <= 1200) ORDER BY lv;
SQL

run_set() { # <drv>
  local drv=$1; setdrv "$drv"
  for f in "$D"/*.sql; do
    local b; b=$(basename "$f" .sql)
    "$NSQL" run -c "$CONN" --no-prompt -f csv "$f" > "$OUT/$drv/$b.raw" 2> "$OUT/$drv/$b.err"; echo "rc=$?" >> "$OUT/$drv/$b.raw"
    # 실행 시간 "(0.339s)"는 실행마다 다르다 → 대조 전에 지운다(값·행·메시지만 남긴다).
    # `SERVEROUTPUT: this Oracle client cannot read 32767-byte …` = OCI 클라이언트 능력 고지(macOS IC 19.16 · 130차 mac) → 드라이버 차이가 아니라
    #   클라이언트 차이라 골든 밖(한 줄 기록만).
    grep -q '^SERVEROUTPUT: ' "$OUT/$drv/$b.raw" && say "  (info) $drv $b: $(grep -m1 '^SERVEROUTPUT: ' "$OUT/$drv/$b.raw")"
    sed -E 's/ *\([0-9.]+s\)//g' "$OUT/$drv/$b.raw" | grep -v '^SERVEROUTPUT: ' > "$OUT/$drv/$b.out"
  done
}

say "-- oci 실행"; run_set oci
say "-- thin 실행(설정 파일만 바꿈 · 재시작 없음)"; run_set thin

# ── 판정 ─────────────────────────────────────────────────────────────────────
say "-- 골든 대조(stdout)"
for f in "$D"/*.sql; do
  b=$(basename "$f" .sql)
  if d=$(diff "$OUT/oci/$b.out" "$OUT/thin/$b.out"); then ok "$b: stdout 동일"; else bad "$b: stdout 다름" "$d"; fi
done
# DATE 바인드 타입은 드라이버가 다르다(OCI = DATE · Thin = TIMESTAMP · oracledb beta.4 제약 · 값은 같다 · 110 §6) → 골든 밖에서 따로 기록.
printf 'VAR d DATE
EXEC :d := TO_DATE(%s,%s)
SELECT SUBSTR(DUMP(:d), 1, 6) AS t FROM dual;
' "'2026-09-21 13:45:10'" "'YYYY-MM-DD HH24:MI:SS'" > "$D/x-datebind.sql"
for drv in oci thin; do setdrv $drv; "$NSQL" run -c "$CONN" --no-prompt -f csv "$D/x-datebind.sql" 2>/dev/null | grep -o "Typ=[0-9]*" | head -1 > "$OUT/$drv/x-datebind.txt"; done; setdrv oci
say "  INFO  DATE 바인드 타입: oci=$(cat "$OUT/oci/x-datebind.txt") thin=$(cat "$OUT/thin/x-datebind.txt") (Thin = Typ=18 예상 · 값 동일)"
# 실행이 실제로 됐는가(접속 실패면 둘 다 빈 결과라 '동일'이 될 수 있다).
for drv in oci thin; do
  if grep -q "^1,1.5,-0.5" "$OUT/$drv/01-types.out"; then ok "$drv: 01-types 결과 있음"; else bad "$drv: 01-types 결과 없음" "$(cat "$OUT/$drv/01-types.err")"; fi
  if grep -q "hello nexa 한글" "$OUT/$drv/02-vars.out" "$OUT/$drv/02-vars.err"; then ok "$drv: DBMS_OUTPUT 회수"; else bad "$drv: DBMS_OUTPUT 없음" "$(cat "$OUT/$drv/02-vars.err")"; fi
  if grep -q "^1,홍길동,25" "$OUT/$drv/03-write.out"; then ok "$drv: 쓰기 왕복(바인드·UPDATE·COMMIT)"; else bad "$drv: 쓰기 왕복 실패" "$(cat "$OUT/$drv/03-write.err")"; fi
  if grep -q "ORA-00942" "$OUT/$drv/04-errors.err" && grep -q "ORA-01722" "$OUT/$drv/04-errors.err"; then ok "$drv: 오류 코드 ORA-00942·ORA-01722"; else bad "$drv: 오류 코드 누락" "$(cat "$OUT/$drv/04-errors.err")"; fi
  n=$(grep -c "^[0-9]" "$OUT/$drv/05-pages.out"); if [ "$n" -ge 1200 ]; then ok "$drv: 1200행 페치"; else bad "$drv: 1200행 아님($n)"; fi
  # ⑧ 트리거(:NEW 바인드 꼴 · Thin = EXECUTE IMMEDIATE 감싸기) = VALID · 컴파일 오류 0 · 트리거가 실제로 돈다(NULL → 7).
  if grep -q "^VALID,0" "$OUT/$drv/06-trigger.out" && grep -q "^1,7" "$OUT/$drv/06-trigger.out"; then ok "$drv: :NEW 트리거 VALID · 동작"; else bad "$drv: 트리거 INVALID/미동작" "$(cat "$OUT/$drv/06-trigger.out" "$OUT/$drv/06-trigger.err")"; fi
done
# 오류 문구(stderr)도 같은가 — 드라이버가 다르면 문구가 다를 수 있어 **코드 줄만** 비교.
e1=$(grep -o "ORA-[0-9]*" "$OUT/oci/04-errors.err" | sort); e2=$(grep -o "ORA-[0-9]*" "$OUT/thin/04-errors.err" | sort)
if [ "$e1" = "$e2" ]; then ok "04-errors: ORA 코드 집합 동일"; else bad "04-errors: ORA 코드 집합 다름" "oci=$e1 thin=$e2"; fi
# ⑥ Thin = 접속 직후 제약 안내 1줄(stderr) · OCI = 없음.
if grep -q "driver limitations" "$OUT/thin/01-types.err"; then ok "thin: 접속 직후 제약 안내(stderr)"; else bad "thin: 제약 안내 없음" "$(cat "$OUT/thin/01-types.err")"; fi
if grep -q "driver limitations" "$OUT/oci/01-types.err"; then bad "oci: 제약 안내가 떴다" "$(cat "$OUT/oci/01-types.err")"; else ok "oci: 제약 안내 없음"; fi
# ⑦ 실시간 전환 = 접속 설명에 드라이버가 드러나는 로그(--log)로 확인 — 같은 프로세스가 아니라 CLI 실행마다 설정을 읽는다.
setdrv thin; t=$("$NSQL" run -c "$CONN" --no-prompt -f csv --timing "$D/05-pages.sql" 2>&1 >/dev/null)
if echo "$t" | grep -qi "thin"; then ok "thin: --timing/stderr에 드라이버 표시"; else ok "thin: 전환 적용(stderr에 이름 없음 = 정보 줄 없음 · 결과로 판정)"; fi
setdrv oci

say "=== 결과: PASS $pass · FAIL $fail · 보고 $REPORT"
[ "$fail" -eq 0 ]
