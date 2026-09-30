#!/usr/bin/env bash
# oracle-ddl-e2e.sh — Oracle **객체 유형별 DDL 전수 E2E**(사용자 09-30 "View, Function, Package, Procedure 등 객체 선언과
#   테이블·인덱스·PK/UK·Constraints 생성 등 DBMS Client가 실행해야 할 전체 유형별 쿼리를 전수 테스트").
#   CLI `nsql run`(GUI와 같은 코어 · 분할기·러너·컴파일 검사)으로 문장을 보내고, `nsql cat source/gen/errors`로 **가져온 소스가
#   만든 소스와 같은지**(소스 열기 = `CREATE OR REPLACE ` + ALL_SOURCE + `/`) · 컴파일 상태 · DDL 생성을 검증한다.
#   ⚠ 실서버에 **임시 객체**(`NSQLT_*`)를 만들고 **끝에 반드시 지운다**(docs/61 §2-4 ⑤). 기존 객체는 건드리지 않는다.
#   권한이 없어 못 만드는 유형(MATERIALIZED VIEW · SYNONYM 등)은 N/A로 적는다(오류 아님).
#
# 사용: scripts/oracle-ddl-e2e.sh -o <출력폴더> [-n target/debug/nsql] [-p biscm]
#   3-OS 공통(bash · Windows는 Git Bash). 종료 코드 = 실패 수(0 = 전부 통과).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NSQL="$ROOT/target/debug/nsql"; OUT=""; PROF="biscm"; OWNER="BISCM"
while getopts "o:n:p:s:" o; do case $o in o) OUT=$OPTARG;; n) NSQL=$OPTARG;; p) PROF=$OPTARG;; s) OWNER=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-n cli] [-p profile]"; exit 2; }
[ -x "$NSQL" ] || [ -x "$NSQL.exe" ] || { echo "nsql not found: $NSQL"; exit 2; }
mkdir -p "$OUT"; REPORT="$OUT/oracle-ddl-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()   { say "  PASS  $1"; pass=$((pass+1)); }
bad()  { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
na()   { say "  N/A   $1"; skip=$((skip+1)); }

# run <이름> <SQL 본문> → 출력(표준+오류) · 종료 코드는 $RC
run() { printf '%s\n' "$2" > "$OUT/$1.sql"; "$NSQL" run -c "$PROF" --no-prompt -f csv "$OUT/$1.sql" > "$OUT/$1.out" 2>&1; RC=$?; cat "$OUT/$1.out"; }
noerr() { if echo "$2" | grep -qiE 'ORA-[0-9]|PLS-[0-9]|SP2-|error:|Warning:.*compilation'; then bad "$1 (서버 오류)" "$2"; else ok "$1"; fi; }
chk()   { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
chki()  { if echo "$3" | grep -qiE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
# 권한 부족(ORA-01031)이면 N/A · 그 밖의 오류면 FAIL
priv_or_ok() { if echo "$2" | grep -q 'ORA-01031'; then na "$1 (권한 없음 ORA-01031)"; else noerr "$1" "$2"; fi; }

say "=== Oracle DDL 전수 E2E · 프로필 $PROF · $(date '+%F %T') ==="
say "--- 0. 사전 정리(남은 임시 객체)"
CLEAN='BEGIN
  FOR r IN (SELECT object_name, object_type FROM user_objects WHERE object_name LIKE '"'"'NSQLT!_%'"'"' ESCAPE '"'"'!'"'"' AND object_type IN ('"'"'TRIGGER'"'"','"'"'PACKAGE'"'"','"'"'PROCEDURE'"'"','"'"'FUNCTION'"'"','"'"'VIEW'"'"','"'"'MATERIALIZED VIEW'"'"','"'"'SYNONYM'"'"','"'"'SEQUENCE'"'"','"'"'TYPE'"'"')) LOOP
    BEGIN EXECUTE IMMEDIATE '"'"'DROP '"'"' || r.object_type || '"'"' '"'"' || r.object_name; EXCEPTION WHEN OTHERS THEN NULL; END;
  END LOOP;
  FOR r IN (SELECT table_name FROM user_tables WHERE table_name LIKE '"'"'NSQLT!_%'"'"' ESCAPE '"'"'!'"'"' ORDER BY CASE WHEN table_name = '"'"'NSQLT_T'"'"' THEN 2 ELSE 1 END) LOOP
    BEGIN EXECUTE IMMEDIATE '"'"'DROP TABLE '"'"' || r.table_name || '"'"' CASCADE CONSTRAINTS PURGE'"'"'; EXCEPTION WHEN OTHERS THEN NULL; END;
  END LOOP;
END;
/'
o=$(run 00_clean "$CLEAN"); noerr "사전 정리 블록" "$o"

say "--- 1. 테이블 · 제약(PK · UK · CHECK · NOT NULL · FK) · 인덱스 · 코멘트 · 시퀀스"
o=$(run 10_table 'CREATE TABLE NSQLT_T (
  ID      NUMBER,
  NAME    VARCHAR2(50) NOT NULL,
  GRP     VARCHAR2(10),
  AMT     NUMBER(10,2) DEFAULT 0,
  CREATED DATE DEFAULT SYSDATE
);
ALTER TABLE NSQLT_T ADD CONSTRAINT NSQLT_T_PK PRIMARY KEY (ID);
ALTER TABLE NSQLT_T ADD CONSTRAINT NSQLT_T_UK UNIQUE (NAME);
ALTER TABLE NSQLT_T ADD CONSTRAINT NSQLT_T_CK CHECK (AMT >= 0);
CREATE TABLE NSQLT_C (CID NUMBER CONSTRAINT NSQLT_C_PK PRIMARY KEY, TID NUMBER, MEMO VARCHAR2(100));
ALTER TABLE NSQLT_C ADD CONSTRAINT NSQLT_C_FK FOREIGN KEY (TID) REFERENCES NSQLT_T (ID) ON DELETE CASCADE;
CREATE INDEX NSQLT_T_IX ON NSQLT_T (GRP);
CREATE INDEX NSQLT_T_IX2 ON NSQLT_T (GRP, CREATED);
COMMENT ON TABLE NSQLT_T IS '"'"'nexa-sql e2e 표'"'"';
COMMENT ON COLUMN NSQLT_T.NAME IS '"'"'이름'"'"';
CREATE SEQUENCE NSQLT_SEQ START WITH 1 INCREMENT BY 1 NOCACHE;'); noerr "CREATE TABLE ×2 · PK · UK · CHECK · FK · INDEX ×2 · COMMENT ×2 · SEQUENCE" "$o"
o=$(run 11_cons "SELECT constraint_name, constraint_type FROM user_constraints WHERE table_name IN ('NSQLT_T','NSQLT_C') ORDER BY 1;")
chk "제약 = PK" 'NSQLT_T_PK,P' "$o"; chk "제약 = UK" 'NSQLT_T_UK,U' "$o"; chk "제약 = CHECK" 'NSQLT_T_CK,C' "$o"; chk "제약 = FK" 'NSQLT_C_FK,R' "$o"
o=$(run 12_idx "SELECT index_name, uniqueness FROM user_indexes WHERE table_name = 'NSQLT_T' ORDER BY 1;")
chk "인덱스 NSQLT_T_IX" 'NSQLT_T_IX,NONUNIQUE' "$o"; chk "PK 인덱스(UNIQUE)" 'NSQLT_T_PK,UNIQUE' "$o"; chk "복합 인덱스" 'NSQLT_T_IX2' "$o"
o=$(run 13_cmt "SELECT comments FROM user_tab_comments WHERE table_name = 'NSQLT_T';"); chk "테이블 코멘트" 'nexa-sql e2e' "$o"
o=$(run 14_alter 'ALTER TABLE NSQLT_T ADD (NOTE VARCHAR2(20));
ALTER TABLE NSQLT_T MODIFY (NOTE VARCHAR2(40));
ALTER TABLE NSQLT_T RENAME COLUMN NOTE TO NOTE2;
ALTER TABLE NSQLT_T DROP COLUMN NOTE2;
ALTER TABLE NSQLT_T DISABLE CONSTRAINT NSQLT_T_CK;
ALTER TABLE NSQLT_T ENABLE CONSTRAINT NSQLT_T_CK;'); noerr "ALTER TABLE ADD · MODIFY · RENAME COLUMN · DROP COLUMN · DISABLE/ENABLE CONSTRAINT" "$o"
o=$("$NSQL" cat gen ddl NSQLT_T table -c "$PROF" 2>&1); chk "Generate SQL ▸ DDL(테이블) = CREATE TABLE" 'CREATE TABLE "[A-Z_]*"\."NSQLT_T"' "$o"; chk "DDL(테이블)에 PK 제약" 'NSQLT_T_PK' "$o"

say "--- 2. 뷰 · 함수 · 프로시저 · 패키지(스펙+본문) · 트리거 · 타입 — 생성 + 소스 왕복 + 컴파일 상태"
F_SRC='FUNCTION NSQLT_F (P_N IN NUMBER) RETURN NUMBER IS
BEGIN
  -- 두 배(한글 주석 · 소스 왕복 검증용)
  RETURN P_N * 2;
END NSQLT_F;'
P_SRC='PROCEDURE NSQLT_P (P_N IN NUMBER, P_OUT OUT NUMBER) AS
  V_X NUMBER := 0;
BEGIN
  IF P_N > 0 THEN
    V_X := NSQLT_F(P_N);
  END IF;
  P_OUT := V_X;
  DBMS_OUTPUT.PUT_LINE('"'"'NSQLT_P: '"'"' || P_OUT);
END NSQLT_P;'
PK_SRC='PACKAGE NSQLT_PKG AS
  C_VER CONSTANT VARCHAR2(10) := '"'"'1.0'"'"';
  FUNCTION TWICE (P_N IN NUMBER) RETURN NUMBER;
  PROCEDURE SAY (P_MSG IN VARCHAR2);
END NSQLT_PKG;'
PB_SRC='PACKAGE BODY NSQLT_PKG AS
  FUNCTION TWICE (P_N IN NUMBER) RETURN NUMBER IS
  BEGIN
    RETURN P_N * 2;
  END TWICE;
  PROCEDURE SAY (P_MSG IN VARCHAR2) IS
  BEGIN
    DBMS_OUTPUT.PUT_LINE(P_MSG);
  END SAY;
END NSQLT_PKG;'
TR_SRC='TRIGGER NSQLT_TRG
BEFORE INSERT ON NSQLT_T
FOR EACH ROW
BEGIN
  IF :NEW.ID IS NULL THEN
    :NEW.ID := NSQLT_SEQ.NEXTVAL;
  END IF;
END;'
TY_SRC='TYPE NSQLT_TY AS OBJECT (
  ID   NUMBER,
  NAME VARCHAR2(50)
);'
o=$(run 20_view 'CREATE OR REPLACE VIEW NSQLT_V AS SELECT ID, NAME, GRP, AMT FROM NSQLT_T WHERE AMT >= 0;'); noerr "CREATE VIEW" "$o"
o=$(run 21_func "CREATE OR REPLACE $F_SRC
/"); noerr "CREATE FUNCTION" "$o"
o=$(run 22_proc "CREATE OR REPLACE $P_SRC
/"); noerr "CREATE PROCEDURE" "$o"
o=$(run 23_pkg "CREATE OR REPLACE $PK_SRC
/
CREATE OR REPLACE $PB_SRC
/"); noerr "CREATE PACKAGE + PACKAGE BODY(한 스크립트 · / 구분)" "$o"
o=$(run 24_trg "CREATE OR REPLACE $TR_SRC
/"); noerr "CREATE TRIGGER" "$o"
o=$(run 25_type "CREATE OR REPLACE $TY_SRC
/"); noerr "CREATE TYPE" "$o"
o=$(run 26_syn 'CREATE OR REPLACE SYNONYM NSQLT_SYN FOR NSQLT_T;'); priv_or_ok "CREATE SYNONYM" "$o"
o=$(run 27_mv 'CREATE MATERIALIZED VIEW NSQLT_MV AS SELECT GRP, COUNT(*) CNT FROM NSQLT_T GROUP BY GRP;'); priv_or_ok "CREATE MATERIALIZED VIEW" "$o"

o=$(run 28_status "SELECT object_name, object_type, status FROM user_objects WHERE object_name LIKE 'NSQLT!_%' ESCAPE '!' ORDER BY 1, 2;")
for obj in "NSQLT_F,FUNCTION" "NSQLT_P,PROCEDURE" "NSQLT_PKG,PACKAGE" "NSQLT_PKG,PACKAGE BODY" "NSQLT_TRG,TRIGGER" "NSQLT_V,VIEW" "NSQLT_TY,TYPE" "NSQLT_SEQ,SEQUENCE" "NSQLT_T_PK,INDEX"; do
  chk "상태 VALID: $obj" "^$obj,VALID" "$o"
done

# 소스 왕복: 소스 열기 = `CREATE OR REPLACE ` + 만든 본문 + `/`(줄 끝 공백만 무시)
norm() { sed -e 's/[[:space:]]*$//' -e '/^$/d'; }
roundtrip() { # <라벨> <종류> <이름> <기대 본문>
  local got exp
  got=$("$NSQL" cat source "$2" "$3" -c "$PROF" 2>&1 | norm)
  exp=$(printf 'CREATE OR REPLACE %s\n/\n' "$4" | norm)
  if [ "$got" = "$exp" ]; then ok "소스 왕복 = 원문 일치: $1"; else bad "소스 왕복 불일치: $1" "$(diff <(echo "$exp") <(echo "$got") | head -12)"; fi
}
roundtrip "FUNCTION NSQLT_F(스키마 한정)" function NSQLT_F "${F_SRC/FUNCTION NSQLT_F /FUNCTION $OWNER.NSQLT_F }"
roundtrip "PROCEDURE NSQLT_P(스키마 한정)" procedure NSQLT_P "${P_SRC/PROCEDURE NSQLT_P /PROCEDURE $OWNER.NSQLT_P }"
roundtrip "PACKAGE NSQLT_PKG(스펙 · 스키마 한정)" package NSQLT_PKG "${PK_SRC/PACKAGE NSQLT_PKG /PACKAGE $OWNER.NSQLT_PKG }"
roundtrip "PACKAGE BODY NSQLT_PKG(스키마 한정)" body NSQLT_PKG "${PB_SRC/PACKAGE BODY NSQLT_PKG /PACKAGE BODY $OWNER.NSQLT_PKG }"
roundtrip "TRIGGER NSQLT_TRG(이름·ON 테이블 스키마 한정)" trigger NSQLT_TRG "$(printf '%s' "$TR_SRC" | sed -e "1s/^TRIGGER NSQLT_TRG$/TRIGGER $OWNER.NSQLT_TRG/" -e "s/^BEFORE INSERT ON NSQLT_T$/BEFORE INSERT ON $OWNER.NSQLT_T/")"
roundtrip "TYPE NSQLT_TY(스키마 한정)" type NSQLT_TY "${TY_SRC/TYPE NSQLT_TY /TYPE $OWNER.NSQLT_TY }"
o=$("$NSQL" cat source view NSQLT_V -c "$PROF" 2>&1); chk "소스 열기(뷰) = GET_DDL 머리(FORCE … VIEW … NSQLT_V (컬럼) AS)" 'CREATE OR REPLACE .*VIEW .*NSQLT_V"? \(' "$o"; chk "뷰 본문" 'WHERE AMT >= 0' "$o"
o=$("$NSQL" cat gen ddl NSQLT_P procedure -c "$PROF" 2>&1); chk "Generate SQL ▸ DDL(프로시저) = DBMS_METADATA + /" '^/$' "$o"; chk "DDL(프로시저) 본문" 'DBMS_OUTPUT.PUT_LINE' "$o"
o=$("$NSQL" cat gen ddl NSQLT_PKG package -c "$PROF" 2>&1); chk "DDL(패키지) = 스펙 + 본문 두 블록(/ 2개)" '^/$' "$o"; n=$(echo "$o" | grep -c '^/$'); [ "$n" -ge 2 ] && ok "DDL(패키지) / 개수 = $n(≥2)" || bad "DDL(패키지) / 개수 = $n(<2)" "$o"
o=$("$NSQL" cat errors NSQLT_P -c "$PROF" 2>&1); chk "컴파일 오류 없음(NSQLT_P)" 'No errors' "$o"

# 소스 열기 결과를 F5(전체 실행)로 다시 실행 = 한 항목(분할기 회귀 · 사용자 09-30 "줄 단위 실행")
"$NSQL" cat source procedure NSQLT_P -c "$PROF" > "$OUT/30_src_p.sql" 2>/dev/null
o=$("$NSQL" plan -d oracle "$OUT/30_src_p.sql" 2>&1); chk "소스 열기 본문의 분할 = 1항목(Block)" 'items=1' "$o"
o=$(run 31_rerun "$(cat "$OUT/30_src_p.sql")"); noerr "소스 열기 본문 그대로 재실행(CREATE OR REPLACE … /)" "$o"
"$NSQL" cat source package NSQLT_PKG -c "$PROF" > "$OUT/32_src_pkg.sql" 2>/dev/null
"$NSQL" cat source body NSQLT_PKG -c "$PROF" >> "$OUT/32_src_pkg.sql" 2>/dev/null
o=$("$NSQL" plan -d oracle "$OUT/32_src_pkg.sql" 2>&1); chk "스펙+본문 이어 붙인 본문의 분할 = 2항목" 'items=2' "$o"
o=$(run 33_rerun_pkg "$(cat "$OUT/32_src_pkg.sql")"); noerr "패키지 스펙+본문 재실행" "$o"
"$NSQL" cat gen ddl NSQLT_PKG package -c "$PROF" > "$OUT/34_ddl_pkg.sql" 2>/dev/null
o=$(run 35_rerun_ddl "$(cat "$OUT/34_ddl_pkg.sql")"); noerr "Generate SQL ▸ DDL(패키지) 본문 그대로 재실행" "$o"

say "--- 3. 컴파일 오류 보고(일부러 틀린 프로시저) · SHOW ERRORS"
o=$(run 40_bad 'CREATE OR REPLACE PROCEDURE NSQLT_BAD AS
BEGIN
  SELECT 1 INTO; -- 일부러 틀림
END NSQLT_BAD;
/
SHOW ERRORS'); chk "Warning: … created with compilation errors" 'compilation errors' "$o"; chk "오류 줄·글(줄/열 ERROR: …)" '^[0-9]+/[0-9]+ ERROR' "$o"
o=$("$NSQL" cat errors NSQLT_BAD -c "$PROF" 2>&1); chk "cat errors = 목록(Line Col Severity Text)" 'ERROR' "$o"
o=$(run 41_bad_status "SELECT status FROM user_objects WHERE object_name = 'NSQLT_BAD';"); chk "NSQLT_BAD = INVALID" 'INVALID' "$o"

say "--- 4. 실행(트리거·함수·프로시저·패키지 · DBMS_OUTPUT) · DML"
o=$(run 50_dml "INSERT INTO NSQLT_T (NAME, GRP, AMT) VALUES ('a', 'g1', 10);
INSERT INTO NSQLT_T (NAME, GRP, AMT) VALUES ('b', 'g1', 20);
INSERT INTO NSQLT_T (ID, NAME, GRP, AMT) VALUES (100, 'c', 'g2', 30);
INSERT INTO NSQLT_C (CID, TID, MEMO) VALUES (1, 100, 'child');
COMMIT;
SELECT COUNT(*) AS CNT, MIN(ID) AS MINID FROM NSQLT_T;
SELECT NSQLT_F(21) AS F FROM DUAL;
SELECT NSQLT_PKG.TWICE(4) AS T FROM DUAL;
SELECT * FROM NSQLT_V ORDER BY ID;"); noerr "DML INSERT ×4(트리거 ID 채움) · COMMIT · SELECT" "$o"
chk "행 수 3 · 트리거가 ID 1부터" '^3,1$' "$o"; chk "함수 호출 = 42" '^42$' "$o"; chk "패키지 함수 = 8" '^8$' "$o"
o=$(run 51_out 'SET SERVEROUTPUT ON
DECLARE V NUMBER; BEGIN NSQLT_P(5, V); NSQLT_PKG.SAY('"'"'hello from pkg '"'"' || NSQLT_PKG.C_VER); END;
/
VAR X NUMBER
EXEC NSQLT_P(7, :X)
PRINT X'); noerr "익명 블록 · EXEC · PRINT" "$o"; chk "DBMS_OUTPUT(프로시저) 도착" 'NSQLT_P: 10' "$o"; chk "DBMS_OUTPUT(패키지 · 상수 C_VER) 도착" 'hello from pkg 1.0' "$o"; chk "PRINT X = 14" '14' "$o"
o=$(run 52_ck "INSERT INTO NSQLT_T (NAME, GRP, AMT) VALUES ('neg', 'g1', -1);"); chk "CHECK 제약 위반 = ORA-02290" 'ORA-02290' "$o"
o=$(run 53_uk "INSERT INTO NSQLT_T (NAME, GRP, AMT) VALUES ('a', 'g1', 1);"); chk "UK 제약 위반 = ORA-00001" 'ORA-00001' "$o"
o=$(run 54_fk "INSERT INTO NSQLT_C (CID, TID, MEMO) VALUES (2, 9999, 'x');"); chk "FK 제약 위반 = ORA-02291" 'ORA-02291' "$o"
o=$(run 55_cascade "DELETE FROM NSQLT_T WHERE ID = 100; COMMIT; SELECT COUNT(*) FROM NSQLT_C;"); chk "ON DELETE CASCADE = 자식 0" '^0$' "$o"

say "--- 5. 정리(DROP 전 유형)"
o=$(run 90_drop 'DROP TRIGGER NSQLT_TRG;
DROP PACKAGE NSQLT_PKG;
DROP PROCEDURE NSQLT_P;
DROP PROCEDURE NSQLT_BAD;
DROP FUNCTION NSQLT_F;
DROP VIEW NSQLT_V;
DROP TYPE NSQLT_TY;
DROP SEQUENCE NSQLT_SEQ;
DROP TABLE NSQLT_C CASCADE CONSTRAINTS PURGE;
DROP TABLE NSQLT_T CASCADE CONSTRAINTS PURGE;'); noerr "DROP TRIGGER · PACKAGE · PROCEDURE ×2 · FUNCTION · VIEW · TYPE · SEQUENCE · TABLE ×2" "$o"
o=$(run 91_drop_opt "$CLEAN"); noerr "잔여(시노님 · MV) 정리 블록" "$o"
o=$(run 92_left "SELECT COUNT(*) AS LEFTOVER FROM user_objects WHERE object_name LIKE 'NSQLT!_%' ESCAPE '!';"); chk "남은 임시 객체 = 0" '^0$' "$o"

say ""; say "== 합계: 통과 $pass · 실패 $fail · N/A $skip  ($(date '+%F %T'))"
exit $fail
