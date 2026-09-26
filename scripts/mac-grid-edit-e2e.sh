#!/usr/bin/env bash
# mac-grid-edit-e2e.sh — 그리드 데이터 편집 E2E 자동 시험(docs/87 §10 · 키 주입 0 · 기동 명령만 · 사용자 09-26 "기능 동작은 테스트 자동화").
#   SQLite(격리 홈 Local 프로필)로 ① 수정 3 + 복제 + NULL → 적용 → 재조회 값 ② 삭제 + NULL + 되돌리기/다시 하기 → 적용
#   ③ 키 없는 중복 행 편집(rowid 끔) = 사전 검사 차단(값 불변) ④ 유일 행 편집 = 적용 ⑦ rowid 켬 = 중복 행도 정확히 1행 수정(87 §13 2급)
#   ⑧ PK 열이 빠진 결과 = 숨은 키 열 주입 재조회 뒤 적용(1급-보완) · NSQL_E2E_ORACLE=<접속 문자열>이면 Oracle 임시 표 NSQLT_GE/NSQLT_GE2
#   (PK 표 · 키 없는 표 · DATE 열)로 ⑤⑥⑨⑩ 저장 실증 뒤 DROP(실서버 임시 객체 · 61 §2-4 ⑤).
#   결과 = 표준 출력 PASS/FAIL 줄 + 종료 코드. 앱은 비활성(NSQL_NO_ACTIVATE)으로 띄우고 마지막 명령 + 5 s 여유(App Nap · 61 §4).
# 사용: scripts/mac-grid-edit-e2e.sh [-e target/debug/nexa-sql] [-n target/debug/nsql] [-H <home>] [-P <실제 설정 폴더>] [-d 프로필:방언,…]
#   4-DBMS 실서버: -P "$HOME/Library/Application Support/nexa-sql" -d BISCM:oracle,Repository:postgres,M4PLAN:mssql (사용자 09-26)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"; H="${TMPDIR:-/tmp}/nsql-ge-e2e"
PROF=""; DBMS="${NSQL_E2E_DBMS:-}"
while getopts "e:n:H:P:d:" o; do case $o in e) APP=$OPTARG;; n) NSQL=$OPTARG;; H) H=$OPTARG;; P) PROF=$OPTARG;; d) DBMS=$OPTARG;; esac; done
D="$H/data"; O="$H/out"; rm -rf "$H"; mkdir -p "$D" "$O"
# -P <실제 설정 폴더>: 프로필·기기 키를 격리 홈으로 **복사**(실제 폴더는 건드리지 않음) → 이름 접속(BISCM · M4PLAN · Repository)으로 실서버 스위트.
if [ -n "$PROF" ] && [ -d "$PROF/profiles" ]; then cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null; fi
fail=0; pass=0
ok() { echo "PASS  $1"; pass=$((pass+1)); }
bad() { echo "FAIL  $1"; fail=$((fail+1)); }
run_gui() { # <초> <인자> <기동 명령>
  local secs=$1 arg=$2 cmd=$3
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" "$APP" "$arg" >/dev/null 2>>"$O/stderr.txt" &
  local p=$!; sleep "$secs"; kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
}
cli() { NSQL_HOME="$H" "$NSQL" run -c "$1" "$2" 2>&1; }
expect_grep() { # <이름> <파일/출력> <패턴>
  if echo "$2" | grep -q -- "$3"; then ok "$1"; else bad "$1 (기대 '$3')"; echo "$2" | head -8 | sed 's/^/      /'; fi; }
expect_absent() { if echo "$2" | grep -q -- "$3"; then bad "$1 (있으면 안 됨 '$3')"; echo "$2" | head -6 | sed 's/^/      /'; else ok "$1"; fi; }

# ── SQLite
"$NSQL" conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1 || true
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
cat > "$D/setup.sql" <<'SQL'
DROP TABLE IF EXISTS ge_emp;
CREATE TABLE ge_emp (id INTEGER PRIMARY KEY, name TEXT NOT NULL, salary REAL, hired TEXT, memo TEXT DEFAULT 'none');
INSERT INTO ge_emp (id, name, salary, hired, memo) VALUES (1, 'kim', 100, '2026-01-01 09:00:00', NULL);
INSERT INTO ge_emp (id, name, salary, hired, memo) VALUES (2, 'lee', 200, '2026-02-02 10:00:00', 'x');
INSERT INTO ge_emp (id, name, salary, hired, memo) VALUES (3, 'park', 300, NULL, 'y');
DROP TABLE IF EXISTS ge_dup;
CREATE TABLE ge_dup (a TEXT, b TEXT);
INSERT INTO ge_dup VALUES ('k', 'v1');
INSERT INTO ge_dup VALUES ('k', 'v1');
INSERT INTO ge_dup VALUES ('z', 'v2');
SQL
printf 'SELECT id, name, salary, hired, memo FROM ge_emp ORDER BY id;\n' > "$D/sel.sql"
printf 'SELECT a, b FROM ge_dup ORDER BY a, b;\n' > "$D/seldup.sql"
cli Local "$D/setup.sql" >/dev/null
echo "=== SQLite ① 수정 3 · 복제(키 비움 → 9) · NULL → 적용 → 재조회"
run_gui 20 Local "open:$D/sel.sql,@after:2500:run.all,@after:5500:grid.edit.set:0;1;KIM2,@after:6000:grid.edit.set:1;2;250.5,@after:6500:grid.edit.set:2;3;2026-09-26 14:05,@after:7000:grid.edit.cmd:row.dup,@after:7500:grid.edit.set:3;1;dup,@after:8000:grid.edit.set:3;0;9,@after:8500:grid.edit.set:1;4;NULL,@after:9000:grid.dump:$O/s1.txt,@after:9500:grid.edit.cmd:row.save,@after:14500:grid.dump:$O/s2.txt"
d1=$(cat "$O/s1.txt" 2>/dev/null); d2=$(cat "$O/s2.txt" 2>/dev/null); v=$(cli Local "$D/sel.sql")
expect_grep "적용 전 = 수정 3 · 추가 1" "$d1" "dirty=true"
expect_grep "적용 뒤 = 깨끗" "$d2" "dirty=false"
expect_grep "행 단위 재조회 = 수정 3 · 추가 1 제자리(전체 재조회 없음)" "$d2" "patched=3/1/0"
expect_grep "제자리 값 = KIM2" "$d2" "|KIM2|"
expect_grep "서버: name KIM2" "$v" "KIM2"
expect_grep "서버: salary 250.5" "$v" "250.5"
expect_grep "서버: 복제 행 id 9" "$v" "^ *9 "
expect_grep "서버: hired 2026-09-26 14:05" "$v" "2026-09-26 14:05"
echo "=== SQLite ② 삭제 + NULL + 되돌리기/다시 하기 → 적용(DELETE+UPDATE)"
run_gui 20 Local "open:$D/sel.sql,@after:2500:run.all,@after:5000:grid.select:3;1,@after:5500:grid.edit.cmd:row.del,@after:6000:grid.select:0;2,@after:6500:grid.edit.cmd:grid.edit.set_null,@after:7000:grid.edit.cmd:grid.edit.undo,@after:7500:grid.edit.cmd:grid.edit.redo,@after:8000:grid.dump:$O/s3.txt,@after:8500:grid.edit.cmd:row.save,@after:14000:grid.dump:$O/s4.txt"
d3=$(cat "$O/s3.txt" 2>/dev/null); d4=$(cat "$O/s4.txt" 2>/dev/null); v=$(cli Local "$D/sel.sql")
expect_grep "적용 전 = 삭제 1 · 수정 1" "$d3" "deleted 1"
expect_grep "적용 뒤 = 행 3" "$d4" "rows=3 src=3"
expect_grep "행 단위 재조회 = 수정 1 · 삭제 1" "$d4" "patched=1/0/1"
expect_absent "서버: id 9 삭제됨" "$v" "^ *9 "
echo "=== SQLite ③ 키 없는 중복 행 편집(rowid 끔 = 3급) = 사전 검사 차단(값 불변) ④ 유일 행 = 적용"
NSQL_HOME="$H" "$NSQL" config set grid.edit_rowid off >/dev/null
run_gui 17 Local "open:$D/seldup.sql,@after:2500:run.all,@after:5500:grid.edit.set:0;1;CHANGED,@after:6500:grid.edit.cmd:row.save,@after:11500:grid.dump:$O/s5.txt"
d5=$(cat "$O/s5.txt" 2>/dev/null); v=$(cli Local "$D/seldup.sql")
expect_grep "중복 행 = 변경 집합 유지(적용 안 됨)" "$d5" "dirty=true"
expect_absent "서버: CHANGED 없음" "$v" "CHANGED"
run_gui 17 Local "open:$D/seldup.sql,@after:2500:run.all,@after:5500:grid.edit.set:2;1;OK,@after:6500:grid.edit.cmd:row.save,@after:11500:grid.dump:$O/s6.txt"
d6=$(cat "$O/s6.txt" 2>/dev/null); v=$(cli Local "$D/seldup.sql")
expect_grep "유일 행 = 적용됨" "$d6" "dirty=false"
expect_grep "서버: z OK" "$v" "OK"
expect_grep "3급 = 전 열 비교" "$d6" "kind=AllColumns"
echo "=== SQLite ⑦ rowid 켬(2급) = 중복 행 편집이 정확히 1행에만 · 숨은 열은 화면에 없음"
NSQL_HOME="$H" "$NSQL" config set grid.edit_rowid on >/dev/null
run_gui 18 Local "open:$D/seldup.sql,@after:2500:run.all,@after:6500:grid.dump:$O/s7a.txt,@after:7000:grid.edit.set:0;1;ONE,@after:7500:grid.edit.cmd:row.save,@after:12500:grid.dump:$O/s7.txt"
d7a=$(cat "$O/s7a.txt" 2>/dev/null); d7=$(cat "$O/s7.txt" 2>/dev/null); v=$(cli Local "$D/seldup.sql")
expect_grep "재조회 뒤 = rowid 숨은 열 1 · 물리 식별" "$d7a" "kind=Physical"
expect_grep "숨은 열 = 뒤쪽 1개" "$d7a" "hidden=1"
expect_grep "덤프 행 = 화면 열 2개만(숨은 rowid 제외)" "$d7a" "^0|[A-Za-z]*|k|v1$"
expect_grep "적용 뒤 깨끗" "$d7" "dirty=false"
expect_grep "rowid 행 제자리 재조회" "$d7" "patched=1/0/0"
expect_grep "서버: ONE 1행" "$(echo "$v" | grep -c ONE)" "^1$"
expect_grep "서버: 나머지 중복 행 v1 유지" "$(echo "$v" | grep -c v1)" "^1$"
echo "=== SQLite ⑧ PK 열이 빠진 결과 = 숨은 키 열 주입(1급-보완) → 적용"
printf 'SELECT name, salary FROM ge_emp ORDER BY name;\n' > "$D/selnk.sql"
run_gui 18 Local "open:$D/selnk.sql,@after:2500:run.all,@after:6500:grid.dump:$O/s8a.txt,@after:7000:grid.edit.set:0;1;777,@after:7500:grid.edit.cmd:row.save,@after:12500:grid.dump:$O/s8.txt"
d8a=$(cat "$O/s8a.txt" 2>/dev/null); d8=$(cat "$O/s8.txt" 2>/dev/null); v=$(cli Local "$D/sel.sql")
expect_grep "재조회 뒤 = 숨은 키 열 · 1급" "$d8a" "kind=Constraint"
expect_grep "숨은 열 1 · 키 = 숨은 열" "$d8a" "hidden=1"
expect_grep "적용 뒤 깨끗" "$d8" "dirty=false"
expect_grep "서버: salary 777" "$v" "777"
echo "=== SQLite ⑨ 수동 커밋: 한 행을 3번 적용 = 트랜잭션 로그 UPDATE 3줄(pending) · 사전 검사 Util 3 · 열린 트랜잭션 갱신 3 · 종료 = 미커밋(롤백)"
NSQL_HOME="$H" "$NSQL" config set session.autocommit off >/dev/null
run_gui 24 Local "open:$D/sel.sql,@after:2500:run.all,@after:6000:grid.edit.set:0;1;M1,@after:6500:grid.edit.cmd:row.save,@after:10500:grid.edit.set:0;1;M2,@after:11000:grid.edit.cmd:row.save,@after:15000:grid.edit.set:0;1;M3,@after:15500:grid.edit.cmd:row.save,@after:19000:txlog.dump:$O/s9tx.txt,@after:19500:grid.dump:$O/s9.txt"
NSQL_HOME="$H" "$NSQL" config set session.autocommit on >/dev/null
t9=$(cat "$O/s9tx.txt" 2>/dev/null); d9=$(cat "$O/s9.txt" 2>/dev/null); v=$(cli Local "$D/sel.sql")
expect_grep "적용 3회 = UPDATE 3줄 · 전부 pending" "$(echo "$t9" | grep -c '^User|UPDATE.*|1|Ok|Pending')" "^3$"
expect_grep "사전 검사 = Util 3줄(대상 1행)" "$(echo "$t9" | grep -c '^Util|SELECT COUNT.*|1|Ok|')" "^3$"
expect_grep "열린 트랜잭션 갱신 수 3" "$t9" "^open_updates=3$"
expect_grep "그리드 = 제자리 갱신 M3" "$d9" "|M3|"
expect_grep "종료 = 커밋 안 됨(서버 값 그대로 KIM2)" "$v" "KIM2"
expect_absent "종료 = M3 미커밋" "$v" "M3"
echo "=== SQLite ⑩ LOB(87 §5): 값 창 이미지 미리보기(PNG) · 파일로 저장 = 원본 동일 · 16진수 · 파일에서 넣기(BMP → 셀 · 5,000자 글 → 셀) → 적용 → 서버 검증"
python3 - "$D" <<'PYLOB'
import sys, os
d=sys.argv[1]
png=bytes([0x89,0x50,0x4e,0x47,0x0d,0x0a,0x1a,0x0a,0,0,0,0x0d,0x49,0x48,0x44,0x52,0,0,0,3,0,0,0,2,8,2,0,0,0,0x12,0x16,0xf1,0x4d,0,0,0,0x11,0x49,0x44,0x41,0x54,0x78,0x9c,0x63,0xe4,0x12,0x91,0x83,0,0x26,0x46,0x18,0,0,0x0e,0x0b,0,0xfd,0x03,0x25,0xe5,0x33,0,0,0,0,0x49,0x45,0x4e,0x44,0xae,0x42,0x60,0x82])
bmp=bytes([0x42,0x4d,0x46,0,0,0,0,0,0,0,0x36,0,0,0,0x28,0,0,0,2,0,0,0,2,0,0,0,1,0,0x18,0,0,0,0,0,0x10,0,0,0,0x13,0x0b,0,0,0x13,0x0b,0,0,0,0,0,0,0,0,0,0,0xff,0,0,0xff,0xff,0xff,0,0,0,0,0xff,0,0xff,0,0,0])
open(os.path.join(d,'px.png'),'wb').write(png); open(os.path.join(d,'px.bmp'),'wb').write(bmp)
open(os.path.join(d,'note.txt'),'w').write('x'*5000)
open(os.path.join(d,'lob_setup.sql'),'w').write("DROP TABLE IF EXISTS ge_blob;\nCREATE TABLE ge_blob (id INTEGER PRIMARY KEY, name TEXT, data BLOB);\nINSERT INTO ge_blob VALUES (1, 'a', X'%s');\n" % png.hex())
open(os.path.join(d,'lob_sel.sql'),'w').write("SELECT id, name, data FROM ge_blob ORDER BY id;\n")
open(os.path.join(d,'lob_chk.sql'),'w').write("SELECT id, length(name) AS nlen, length(data) AS dlen, hex(substr(data,1,2)) AS head FROM ge_blob ORDER BY id;\n")
PYLOB
cli Local "$D/lob_setup.sql" >/dev/null
run_gui 26 Local "open:$D/lob_sel.sql,@after:2500:run.all,@after:5500:cellview.open:0;2,@after:7000:cellview.dump:$O/s10a.txt,@after:7500:cellview.save:$O/s10.png,@after:8000:cellview.mode:hex,@after:8500:cellview.dump:$O/s10b.txt,@after:9000:grid.edit.load:0;2;$D/px.bmp,@after:9500:grid.edit.load:0;1;$D/note.txt,@after:10000:grid.dump:$O/s10c.txt,@after:10500:grid.edit.cmd:row.save,@after:15500:cellview.open:0;2,@after:17000:cellview.dump:$O/s10d.txt,@after:17500:grid.dump:$O/s10e.txt"
d10a=$(cat "$O/s10a.txt" 2>/dev/null); d10b=$(cat "$O/s10b.txt" 2>/dev/null); d10c=$(cat "$O/s10c.txt" 2>/dev/null); d10d=$(cat "$O/s10d.txt" 2>/dev/null); d10e=$(cat "$O/s10e.txt" 2>/dev/null); v=$(cli Local "$D/lob_chk.sql")
expect_grep "값 창 = PNG 이미지 3x2 미리보기" "$d10a" "mode=Image kind=PNG bytes=74 image=3x2"
if cmp -s "$O/s10.png" "$D/px.png"; then ok "파일로 저장 = 원본과 동일"; else bad "파일로 저장 = 원본과 동일"; fi
expect_grep "16진수 보기 전환" "$d10b" "mode=Hex"
expect_grep "파일에서 넣기 = 셀 라벨 <px.bmp · 70 bytes>" "$d10c" "px.bmp · 70 bytes"
expect_grep "5,000자 글 넣기 = 변경 집합" "$d10c" "dirty=true"
expect_grep "적용 뒤 깨끗" "$d10e" "dirty=false"
expect_grep "서버: BLOB 70 bytes · 머리 424D(BMP)" "$v" "70  424D"
expect_grep "서버: 글 5,000자(CLOB 길)" "$v" " 5000 "
expect_grep "다시 연 값 창 = BMP 2x2" "$d10d" "kind=BMP bytes=70 image=2x2"
echo "=== SQLite ⑪ 편집 툴바 활성(사용자 09-26): 조회 직후 복제/삭제 = 비활성 → 셀 클릭(선택) = 활성 · 편집 동작 없이도"
run_gui 12 Local "open:$D/sel.sql,@after:2500:run.all,@after:5500:grid.dump:$O/t0.txt,@after:6000:grid.select:0;1,@after:7000:grid.dump:$O/t1.txt"
expect_grep "선택 전 = add 활성 · dup/del 비활성" "$(cat "$O/t0.txt" 2>/dev/null)" "tools add=true dup=false del=false"
expect_grep "셀 선택 뒤 = dup/del 활성(편집 동작 없이)" "$(cat "$O/t1.txt" 2>/dev/null)" "tools add=true dup=true del=true"
# ── 실서버 스위트(방언 공통 4 시나리오 · 임시 표 NSQLT_GE(PK)·NSQLT_GE2(키 없음) 생성 → 시험 → DROP · 61 §2-4 ⑤)
#   dbms_suite <프로필|접속 문자열> <oracle|postgres|mssql> <라벨>
dbms_suite() {
  local T=$1 dl=$2 tag=$3 pk kind
  case $dl in
    oracle)
      cat > "$D/${tag}_setup.sql" <<'SQL'
CREATE TABLE NSQLT_GE (ID NUMBER PRIMARY KEY, NAME VARCHAR2(20), DT DATE, MEMO VARCHAR2(50));
INSERT INTO NSQLT_GE VALUES (1, 'kim', TO_DATE('2026-07-01 18:45:00','YYYY-MM-DD HH24:MI:SS'), NULL);
INSERT INTO NSQLT_GE VALUES (2, 'lee', TO_DATE('2026-07-02 09:00:00','YYYY-MM-DD HH24:MI:SS'), 'x');
CREATE TABLE NSQLT_GE2 (A VARCHAR2(10), DT DATE, MEMO VARCHAR2(50));
INSERT INTO NSQLT_GE2 VALUES ('k', TO_DATE('2026-07-01 18:45:00','YYYY-MM-DD HH24:MI:SS'), NULL);
INSERT INTO NSQLT_GE2 VALUES ('z', TO_DATE('2026-07-02 09:00:00','YYYY-MM-DD HH24:MI:SS'), 'x');
COMMIT;
SQL
      printf "INSERT INTO NSQLT_GE2 SELECT * FROM NSQLT_GE2 WHERE A = 'z';\nCOMMIT;\n" > "$D/${tag}_dup.sql"; kind=Physical;;
    postgres)
      cat > "$D/${tag}_setup.sql" <<'SQL'
CREATE TABLE nsqlt_ge (id integer PRIMARY KEY, name varchar(20), dt timestamp, memo varchar(50));
INSERT INTO nsqlt_ge VALUES (1, 'kim', '2026-07-01 18:45:00', NULL);
INSERT INTO nsqlt_ge VALUES (2, 'lee', '2026-07-02 09:00:00', 'x');
CREATE TABLE nsqlt_ge2 (a varchar(10), dt timestamp, memo varchar(50));
INSERT INTO nsqlt_ge2 VALUES ('k', '2026-07-01 18:45:00', NULL);
INSERT INTO nsqlt_ge2 VALUES ('z', '2026-07-02 09:00:00', 'x');
SQL
      printf "INSERT INTO nsqlt_ge2 SELECT * FROM nsqlt_ge2 WHERE a = 'z';\n" > "$D/${tag}_dup.sql"; kind=Physical;;
    mssql)
      cat > "$D/${tag}_setup.sql" <<'SQL'
CREATE TABLE NSQLT_GE (ID int PRIMARY KEY, NAME nvarchar(20), DT datetime2(0), MEMO nvarchar(50));
INSERT INTO NSQLT_GE VALUES (1, 'kim', '2026-07-01 18:45:00', NULL);
INSERT INTO NSQLT_GE VALUES (2, 'lee', '2026-07-02 09:00:00', 'x');
CREATE TABLE NSQLT_GE2 (A nvarchar(10), DT datetime2(0), MEMO nvarchar(50));
INSERT INTO NSQLT_GE2 VALUES ('k', '2026-07-01 18:45:00', NULL);
INSERT INTO NSQLT_GE2 VALUES ('z', '2026-07-02 09:00:00', 'x');
SQL
      printf "INSERT INTO NSQLT_GE2 SELECT * FROM NSQLT_GE2 WHERE A = 'z';\n" > "$D/${tag}_dup.sql"; kind=AllColumns;;
  esac
  printf 'DROP TABLE NSQLT_GE;\nDROP TABLE NSQLT_GE2;\n' > "$D/${tag}_drop.sql"
  printf 'SELECT ID, NAME, DT, MEMO FROM NSQLT_GE ORDER BY ID;\n' > "$D/${tag}_sel.sql"
  printf 'SELECT A, DT, MEMO FROM NSQLT_GE2 ORDER BY A;\n' > "$D/${tag}_sel2.sql"
  printf 'SELECT NAME, MEMO FROM NSQLT_GE ORDER BY NAME;\n' > "$D/${tag}_selnk.sql"
  cli "$T" "$D/${tag}_drop.sql" >/dev/null 2>&1; cli "$T" "$D/${tag}_setup.sql" >/dev/null 2>&1
  local v; v=$(cli "$T" "$D/${tag}_sel.sql")
  if ! echo "$v" | grep -q "kim"; then bad "$tag 준비(임시 표 생성·접속)"; echo "$v" | head -4 | sed 's/^/      /'; return; fi
  echo "=== $tag ⑤ 키 없는 표(DATE 포함 · $kind) 저장"
  run_gui 22 "$T" "open:$D/${tag}_sel2.sql,@after:4000:run.all,@after:8000:grid.edit.set:0;2;M1,@after:9000:grid.edit.cmd:row.save,@after:15000:grid.dump:$O/${tag}1.txt"
  local d; d=$(cat "$O/${tag}1.txt" 2>/dev/null); v=$(cli "$T" "$D/${tag}_sel2.sql")
  expect_grep "$tag 적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "$tag 서버: M1" "$v" "M1"; expect_grep "$tag 키 없는 표 = $kind" "$d" "kind=$kind"
  echo "=== $tag ⑥ PK 표 문자·DATE 값 수정 저장(행 단위 재조회)"
  run_gui 22 "$T" "open:$D/${tag}_sel.sql,@after:4000:run.all,@after:8000:grid.edit.set:1;1;LEE2,@after:8500:grid.edit.set:1;2;2026-09-26 10:11:12,@after:9000:grid.edit.cmd:row.save,@after:15000:grid.dump:$O/${tag}2.txt"
  d=$(cat "$O/${tag}2.txt" 2>/dev/null); v=$(cli "$T" "$D/${tag}_sel.sql")
  expect_grep "$tag 적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "$tag 서버: LEE2" "$v" "LEE2"; expect_grep "$tag 서버: DATE 2026-09-26 10:11:12" "$v" "2026-09-26 10:11:12"
  expect_grep "$tag 행 단위 재조회 제자리" "$d" "patched=1/0/0"
  echo "=== $tag ⑨ PK 열이 빠진 결과 = 숨은 키 열 주입 → 적용"
  run_gui 24 "$T" "open:$D/${tag}_selnk.sql,@after:4000:run.all,@after:10000:grid.dump:$O/${tag}3a.txt,@after:10500:grid.edit.set:0;1;HK,@after:11000:grid.edit.cmd:row.save,@after:17000:grid.dump:$O/${tag}3.txt"
  local da; da=$(cat "$O/${tag}3a.txt" 2>/dev/null); d=$(cat "$O/${tag}3.txt" 2>/dev/null); v=$(cli "$T" "$D/${tag}_sel.sql")
  expect_grep "$tag 숨은 키 열 · 1급" "$da" "kind=Constraint"; expect_grep "$tag 숨은 열 1" "$da" "hidden=1"
  expect_grep "$tag 적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "$tag 서버: MEMO HK" "$v" "HK"
  echo "=== $tag ⑩ 키 없는 표의 완전 중복 행 = 물리 식별자면 정확히 1행 · 전 열 비교면 차단"
  cli "$T" "$D/${tag}_dup.sql" >/dev/null 2>&1
  run_gui 24 "$T" "open:$D/${tag}_sel2.sql,@after:4000:run.all,@after:10000:grid.edit.set:1;2;ONE,@after:11000:grid.edit.cmd:row.save,@after:17000:grid.dump:$O/${tag}4.txt"
  d=$(cat "$O/${tag}4.txt" 2>/dev/null); v=$(cli "$T" "$D/${tag}_sel2.sql")
  if [ "$kind" = Physical ]; then
    expect_grep "$tag 적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "$tag 서버: ONE 1행" "$(echo "$v" | grep -c ONE)" "^1$"; expect_grep "$tag 서버: 원래 x 1행 남음" "$(echo "$v" | grep -c ' x')" "^1$"
  else
    expect_grep "$tag 중복 행 = 사전 검사 차단(변경 집합 유지)" "$d" "dirty=true"; expect_absent "$tag 서버: ONE 없음" "$v" "ONE"
  fi
  cli "$T" "$D/${tag}_drop.sql" >/dev/null 2>&1
}
if [ -n "${NSQL_E2E_ORACLE:-}" ]; then NSQL_HOME="$H" "$NSQL" conn add ORA "$NSQL_E2E_ORACLE" -d oracle --no-prompt >/dev/null 2>&1; dbms_suite ORA oracle Oracle; fi
# -d "BISCM:oracle,Repository:postgres,M4PLAN:mssql"(또는 NSQL_E2E_DBMS) = 격리 홈으로 복사한 프로필 이름으로 실서버 스위트.
if [ -n "$DBMS" ]; then
  IFS=',' read -ra PAIRS <<< "$DBMS"
  for pr in "${PAIRS[@]}"; do dbms_suite "${pr%%:*}" "${pr##*:}" "${pr%%:*}"; done
fi
echo "=== 결과: PASS $pass · FAIL $fail (출력 $O)"
[ "$fail" -eq 0 ]
