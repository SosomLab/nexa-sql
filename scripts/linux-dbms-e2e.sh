#!/usr/bin/env bash
# linux-dbms-e2e.sh — 4-DBMS **DDL · DML · TCL · DCL** 실서버 E2E(사용자 09-26 "실제 DDL, DML, DCL 모두 테스트").
#   저장된 **프로필 이름**으로 접속한다(비밀번호는 환경에 안 적는다 · 볼트). CLI `nsql run`으로 문장을 보내고 결과를 검증한다.
#   ⚠ 실서버에 **임시 객체**(`NSQLE_*`)를 만들고 **반드시 지운다**(docs/61 §2-4 ⑤). DCL은 내가 만든 그 임시 표에만 걸고
#     검증 뒤 즉시 REVOKE + DROP 한다(기존 객체·계정 권한은 건드리지 않는다).
#   SQLite는 DCL이 없어 해당 항목을 N/A로 적는다(오류 아님).
#
# 사용: scripts/linux-dbms-e2e.sh -o <출력폴더> [-n target/debug/nsql] [-p "BISCM:oracle,M4PLAN:mssql,Repository:postgres,Demo:sqlite"]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NSQL="$ROOT/target/debug/nsql"; OUT=""; PROFILES="BISCM:oracle,M4PLAN:mssql,Repository:postgres,Demo:sqlite"
while getopts "o:n:p:" o; do case $o in o) OUT=$OPTARG;; n) NSQL=$OPTARG;; p) PROFILES=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-n cli] [-p prof:dialect,...]"; exit 2; }
mkdir -p "$OUT"; REPORT="$OUT/dbms-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()   { say "  PASS  $1"; pass=$((pass+1)); }
bad()  { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -6 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
na()   { say "  N/A   $1"; skip=$((skip+1)); }

run_sql() { # <프로필> <파일> → 출력(표준+오류)
  "$NSQL" run -c "$1" "$2" 2>&1
}
# 검사: 출력에 패턴이 있으면 PASS
chk()  { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
chkn() { if echo "$3" | grep -qE -- "$2"; then bad "$1 (있으면 안 됨 $2)" "$3"; else ok "$1"; fi; }
# 대소문자 무시판(방언마다 권한 이름 표기가 다르다 — Oracle SELECT · PG SELECT · SQL Server SELECT)
chki()  { if echo "$3" | grep -qiE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
chkni() { if echo "$3" | grep -qiE -- "$2"; then bad "$1 (있으면 안 됨 $2)" "$3"; else ok "$1"; fi; }
# 오류 없음 확인(드라이버/서버 오류 문자열)
noerr() { if echo "$2" | grep -qiE 'ORA-[0-9]|SQL Error|error:|에러|Msg [0-9]+,|ERROR:'; then bad "$1 (서버 오류)" "$2"; else ok "$1"; fi; }

for ENTRY in ${PROFILES//,/ }; do
  P="${ENTRY%%:*}"; DIA="${ENTRY##*:}"
  say ""; say "=== $P ($DIA) ======================================================"
  W="$OUT/$P"; mkdir -p "$W"

  # 방언별 조각 --------------------------------------------------------------
  case "$DIA" in
    oracle)
      NUMT="NUMBER"; STRT="VARCHAR2(50)"; DATET="DATE"; NOW="SYSDATE"; DUAL=" FROM DUAL"
      DROPT="BEGIN EXECUTE IMMEDIATE 'DROP TABLE NSQLE_T'; EXCEPTION WHEN OTHERS THEN NULL; END;
/"
      DROPV="BEGIN EXECUTE IMMEDIATE 'DROP VIEW NSQLE_V'; EXCEPTION WHEN OTHERS THEN NULL; END;
/"
      PRIVQ="SELECT GRANTEE, PRIVILEGE FROM USER_TAB_PRIVS WHERE TABLE_NAME = 'NSQLE_T';"
      GRANTS="GRANT SELECT ON NSQLE_T TO PUBLIC;"; REVOKES="REVOKE SELECT ON NSQLE_T FROM PUBLIC;"
      IDXQ="CREATE INDEX NSQLE_IX ON NSQLE_T (NAME);"; DROPIX="BEGIN EXECUTE IMMEDIATE 'DROP INDEX NSQLE_IX'; EXCEPTION WHEN OTHERS THEN NULL; END;
/"
      CMT="COMMENT ON TABLE NSQLE_T IS 'nexa-sql e2e';"
      CMTQ="SELECT COMMENTS FROM USER_TAB_COMMENTS WHERE TABLE_NAME = 'NSQLE_T';"
      ;;
    mssql)
      NUMT="INT"; STRT="NVARCHAR(50)"; DATET="DATETIME"; NOW="GETDATE()"; DUAL=""
      DROPT="IF OBJECT_ID('NSQLE_T','U') IS NOT NULL DROP TABLE NSQLE_T;"
      DROPV="IF OBJECT_ID('NSQLE_V','V') IS NOT NULL DROP VIEW NSQLE_V;"
      PRIVQ="SELECT p.permission_name FROM sys.database_permissions p JOIN sys.objects o ON p.major_id = o.object_id WHERE o.name = 'NSQLE_T';"
      GRANTS="GRANT SELECT ON NSQLE_T TO public;"; REVOKES="REVOKE SELECT ON NSQLE_T FROM public;"
      IDXQ="CREATE INDEX NSQLE_IX ON NSQLE_T (NAME);"; DROPIX="IF EXISTS (SELECT 1 FROM sys.indexes WHERE name='NSQLE_IX') DROP INDEX NSQLE_IX ON NSQLE_T;"
      CMT=""; CMTQ=""
      ;;
    postgres)
      NUMT="INTEGER"; STRT="VARCHAR(50)"; DATET="TIMESTAMP"; NOW="NOW()"; DUAL=""
      DROPT="DROP TABLE IF EXISTS NSQLE_T CASCADE;"; DROPV="DROP VIEW IF EXISTS NSQLE_V;"
      PRIVQ="SELECT grantee, privilege_type FROM information_schema.table_privileges WHERE table_name = 'nsqle_t';"
      GRANTS="GRANT SELECT ON NSQLE_T TO PUBLIC;"; REVOKES="REVOKE SELECT ON NSQLE_T FROM PUBLIC;"
      IDXQ="CREATE INDEX NSQLE_IX ON NSQLE_T (NAME);"; DROPIX="DROP INDEX IF EXISTS NSQLE_IX;"
      CMT="COMMENT ON TABLE NSQLE_T IS 'nexa-sql e2e';"
      CMTQ="SELECT obj_description('nsqle_t'::regclass) AS c;"
      ;;
    sqlite)
      NUMT="INTEGER"; STRT="TEXT"; DATET="TEXT"; NOW="datetime('now')"; DUAL=""
      DROPT="DROP TABLE IF EXISTS NSQLE_T;"; DROPV="DROP VIEW IF EXISTS NSQLE_V;"
      PRIVQ=""; GRANTS=""; REVOKES=""
      IDXQ="CREATE INDEX NSQLE_IX ON NSQLE_T (NAME);"; DROPIX="DROP INDEX IF EXISTS NSQLE_IX;"
      CMT=""; CMTQ=""
      ;;
  esac

  # ── ① DDL: CREATE TABLE · ALTER ADD · CREATE INDEX · CREATE VIEW · COMMENT ──
  cat > "$W/ddl.sql" <<SQL
$DROPV
$DROPT
CREATE TABLE NSQLE_T (ID $NUMT NOT NULL, NAME $STRT, AMT $NUMT, TS $DATET);
$IDXQ
CREATE VIEW NSQLE_V AS SELECT ID, NAME FROM NSQLE_T;
SQL
  [ -n "$CMT" ] && echo "$CMT" >> "$W/ddl.sql"
  o=$(run_sql "$P" "$W/ddl.sql"); echo "$o" > "$W/ddl.out"
  noerr "DDL CREATE TABLE/INDEX/VIEW" "$o"
  # ALTER
  echo "ALTER TABLE NSQLE_T ADD MEMO $STRT;" > "$W/alter.sql"
  o=$(run_sql "$P" "$W/alter.sql"); echo "$o" > "$W/alter.out"; noerr "DDL ALTER TABLE ADD" "$o"
  # 구조 확인
  echo "SELECT ID, NAME, AMT, MEMO FROM NSQLE_T;" > "$W/desc.sql"
  o=$(run_sql "$P" "$W/desc.sql"); chkn "DDL 새 컬럼 조회 가능" 'ORA-|Msg [0-9]+|ERROR:' "$o"
  if [ -n "$CMTQ" ]; then
    echo "$CMTQ" > "$W/cmt.sql"; o=$(run_sql "$P" "$W/cmt.sql"); chk "DDL COMMENT 반영" 'nexa-sql e2e' "$o"
  else na "DDL COMMENT(방언 미지원)"; fi

  # ── ② DML: INSERT 다건 · UPDATE · DELETE · 집계 검증 ────────────────────────
  cat > "$W/dml.sql" <<SQL
INSERT INTO NSQLE_T (ID, NAME, AMT, TS) VALUES (1, 'kim', 100, $NOW);
INSERT INTO NSQLE_T (ID, NAME, AMT, TS) VALUES (2, 'lee', 200, $NOW);
INSERT INTO NSQLE_T (ID, NAME, AMT, TS) VALUES (3, 'park', 300, $NOW);
UPDATE NSQLE_T SET AMT = 250, MEMO = 'updated' WHERE ID = 2;
DELETE FROM NSQLE_T WHERE ID = 3;
SQL
  o=$(run_sql "$P" "$W/dml.sql"); echo "$o" > "$W/dml.out"; noerr "DML INSERT/UPDATE/DELETE" "$o"
  echo "SELECT COUNT(*) AS N, SUM(AMT) AS S FROM NSQLE_T;" > "$W/agg.sql"
  o=$(run_sql "$P" "$W/agg.sql"); echo "$o" > "$W/agg.out"
  chk "DML 결과 = 2행" '(^| )2( |$)' "$o"
  chk "DML 합계 = 350" '350' "$o"
  echo "SELECT ID, NAME FROM NSQLE_V ORDER BY ID;" > "$W/view.sql"
  o=$(run_sql "$P" "$W/view.sql"); chk "DML 뷰 조회 = kim/lee" 'kim' "$o"

  # ── ③ TCL: 수동 커밋에서 ROLLBACK · COMMIT ──────────────────────────────────
  case "$DIA" in
    oracle) TCLSQL="INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (91, 'rollback_me', 1);
ROLLBACK;
INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (92, 'commit_me', 1);
COMMIT;" ;;
    mssql)  TCLSQL="BEGIN TRANSACTION;
INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (91, 'rollback_me', 1);
ROLLBACK;
BEGIN TRANSACTION;
INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (92, 'commit_me', 1);
COMMIT;" ;;
    postgres) TCLSQL="BEGIN;
INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (91, 'rollback_me', 1);
ROLLBACK;
BEGIN;
INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (92, 'commit_me', 1);
COMMIT;" ;;
    sqlite) TCLSQL="BEGIN;
INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (91, 'rollback_me', 1);
ROLLBACK;
BEGIN;
INSERT INTO NSQLE_T (ID, NAME, AMT) VALUES (92, 'commit_me', 1);
COMMIT;" ;;
  esac
  echo "$TCLSQL" > "$W/tcl.sql"
  o=$(run_sql "$P" "$W/tcl.sql"); echo "$o" > "$W/tcl.out"; noerr "TCL BEGIN/ROLLBACK/COMMIT 실행" "$o"
  echo "SELECT ID, NAME FROM NSQLE_T WHERE ID IN (91, 92) ORDER BY ID;" > "$W/tclchk.sql"
  o=$(run_sql "$P" "$W/tclchk.sql"); echo "$o" > "$W/tclchk.out"
  chkn "TCL ROLLBACK된 행(91) 없음" 'rollback_me' "$o"
  chk  "TCL COMMIT된 행(92) 있음"  'commit_me' "$o"

  # ── ④ DCL: 내가 만든 임시 표에만 GRANT → 권한 조회 → REVOKE → 확인 ─────────
  if [ -n "$GRANTS" ]; then
    echo "$GRANTS" > "$W/grant.sql"; o=$(run_sql "$P" "$W/grant.sql"); echo "$o" > "$W/grant.out"
    noerr "DCL GRANT SELECT (임시 표 · PUBLIC)" "$o"
    echo "$PRIVQ" > "$W/priv.sql"; o=$(run_sql "$P" "$W/priv.sql"); echo "$o" > "$W/priv1.out"
    chki "DCL 권한 조회 = SELECT 보임" 'select' "$o"
    echo "$REVOKES" > "$W/revoke.sql"; o=$(run_sql "$P" "$W/revoke.sql"); echo "$o" > "$W/revoke.out"
    noerr "DCL REVOKE SELECT" "$o"
    o=$(run_sql "$P" "$W/priv.sql"); echo "$o" > "$W/priv2.out"
    chkni "DCL 회수 뒤 권한 사라짐" 'public' "$o"
  else
    na "DCL(SQLite는 권한 개념 없음)"
  fi

  # ── ⑤ 뒷정리: 만든 임시 객체 전부 삭제 ─────────────────────────────────────
  { echo "$DROPV"; echo "$DROPIX"; echo "$DROPT"; } > "$W/cleanup.sql"
  o=$(run_sql "$P" "$W/cleanup.sql"); echo "$o" > "$W/cleanup.out"; noerr "뒷정리 DROP VIEW/INDEX/TABLE" "$o"
  echo "SELECT COUNT(*) AS N FROM NSQLE_T;" > "$W/gone.sql"
  o=$(run_sql "$P" "$W/gone.sql")
  if echo "$o" | grep -qiE 'ORA-00942|Invalid object name|Table or view not found|does not exist|no such table|유효하지 않습니다'; then ok "뒷정리 확인 = 표 없음"
  else bad "뒷정리 확인 = 표가 남아 있음" "$o"; fi
done

say ""
say "== 합계: 통과 $pass · 실패 $fail · N/A $skip  ($(date '+%F %T'))"
[ "$fail" = 0 ]
