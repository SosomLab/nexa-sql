#!/usr/bin/env bash
# mac-grid-edit-e2e.sh — 그리드 데이터 편집 E2E 자동 시험(docs/87 §10 · 키 주입 0 · 기동 명령만 · 사용자 09-26 "기능 동작은 테스트 자동화").
#   SQLite(격리 홈 Local 프로필)로 ① 수정 3 + 복제 + NULL → 적용 → 재조회 값 ② 삭제 + NULL + 되돌리기/다시 하기 → 적용
#   ③ 키 없는 중복 행 편집(rowid 끔) = 사전 검사 차단(값 불변) ④ 유일 행 편집 = 적용 ⑦ rowid 켬 = 중복 행도 정확히 1행 수정(87 §13 2급)
#   ⑧ PK 열이 빠진 결과 = 숨은 키 열 주입 재조회 뒤 적용(1급-보완) · NSQL_E2E_ORACLE=<접속 문자열>이면 Oracle 임시 표 NSQLT_GE/NSQLT_GE2
#   (PK 표 · 키 없는 표 · DATE 열)로 ⑤⑥⑨⑩ 저장 실증 뒤 DROP(실서버 임시 객체 · 61 §2-4 ⑤).
#   결과 = 표준 출력 PASS/FAIL 줄 + 종료 코드. 앱은 비활성(NSQL_NO_ACTIVATE)으로 띄우고 마지막 명령 + 5 s 여유(App Nap · 61 §4).
# 사용: scripts/mac-grid-edit-e2e.sh [-e target/debug/nexa-sql] [-n target/debug/nsql] [-H <home>]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"; H="${TMPDIR:-/tmp}/nsql-ge-e2e"
while getopts "e:n:H:" o; do case $o in e) APP=$OPTARG;; n) NSQL=$OPTARG;; H) H=$OPTARG;; esac; done
D="$H/data"; O="$H/out"; rm -rf "$H"; mkdir -p "$D" "$O"
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
# ── Oracle(선택)
if [ -n "${NSQL_E2E_ORACLE:-}" ]; then
  ORA="$NSQL_E2E_ORACLE"
  NSQL_HOME="$H" "$NSQL" conn add ORA "$ORA" -d oracle --no-prompt >/dev/null 2>&1
  cat > "$D/ora_setup.sql" <<'SQL'
CREATE TABLE NSQLT_GE (ID NUMBER PRIMARY KEY, NAME VARCHAR2(20), DT DATE, MEMO VARCHAR2(50));
INSERT INTO NSQLT_GE VALUES (1, 'kim', TO_DATE('2026-07-01 18:45:00','YYYY-MM-DD HH24:MI:SS'), NULL);
INSERT INTO NSQLT_GE VALUES (2, 'lee', TO_DATE('2026-07-02 09:00:00','YYYY-MM-DD HH24:MI:SS'), 'x');
CREATE TABLE NSQLT_GE2 (A VARCHAR2(10), DT DATE, MEMO VARCHAR2(50));
INSERT INTO NSQLT_GE2 VALUES ('k', TO_DATE('2026-07-01 18:45:00','YYYY-MM-DD HH24:MI:SS'), NULL);
INSERT INTO NSQLT_GE2 VALUES ('z', TO_DATE('2026-07-02 09:00:00','YYYY-MM-DD HH24:MI:SS'), 'x');
COMMIT;
SQL
  printf 'DROP TABLE NSQLT_GE;\nDROP TABLE NSQLT_GE2;\n' > "$D/ora_drop.sql"
  printf 'SELECT ID, NAME, DT, MEMO FROM NSQLT_GE ORDER BY ID;\n' > "$D/ora_sel.sql"
  printf 'SELECT A, DT, MEMO FROM NSQLT_GE2 ORDER BY A;\n' > "$D/ora_sel2.sql"
  cli ORA "$D/ora_drop.sql" >/dev/null 2>&1; cli ORA "$D/ora_setup.sql" >/dev/null 2>&1
  echo "=== Oracle ⑤ 키 없는 표(DATE 포함 전 열 WHERE · 바인드 이름 대문자) 저장"
  run_gui 22 "$ORA" "open:$D/ora_sel2.sql,@after:4000:run.all,@after:8000:grid.edit.set:0;2;M1,@after:9000:grid.edit.cmd:row.save,@after:15000:grid.dump:$O/o1.txt"
  d=$(cat "$O/o1.txt" 2>/dev/null); v=$(cli ORA "$D/ora_sel2.sql")
  expect_grep "적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "서버: M1" "$v" "M1"; expect_grep "키 없는 표 = ROWID(2급)" "$d" "kind=Physical"
  echo "=== Oracle ⑥ PK 표 문자·DATE 값 수정 저장"
  run_gui 22 "$ORA" "open:$D/ora_sel.sql,@after:4000:run.all,@after:8000:grid.edit.set:1;1;LEE2,@after:8500:grid.edit.set:1;2;2026-09-26 10:11:12,@after:9000:grid.edit.cmd:row.save,@after:15000:grid.dump:$O/o2.txt"
  d=$(cat "$O/o2.txt" 2>/dev/null); v=$(cli ORA "$D/ora_sel.sql")
  expect_grep "적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "서버: LEE2" "$v" "LEE2"; expect_grep "서버: DATE 2026-09-26 10:11:12" "$v" "2026-09-26 10:11:12"
  expect_grep "Oracle 행 단위 재조회 제자리" "$d" "patched=1/0/0"
  echo "=== Oracle ⑨ PK 열이 빠진 결과 = 숨은 키 열(ID) 주입 → 적용"
  printf 'SELECT NAME, MEMO FROM NSQLT_GE ORDER BY NAME;\n' > "$D/ora_selnk.sql"
  run_gui 24 "$ORA" "open:$D/ora_selnk.sql,@after:4000:run.all,@after:10000:grid.dump:$O/o3a.txt,@after:10500:grid.edit.set:0;1;HK,@after:11000:grid.edit.cmd:row.save,@after:17000:grid.dump:$O/o3.txt"
  da=$(cat "$O/o3a.txt" 2>/dev/null); d=$(cat "$O/o3.txt" 2>/dev/null); v=$(cli ORA "$D/ora_sel.sql")
  expect_grep "숨은 키 열 · 1급" "$da" "kind=Constraint"; expect_grep "숨은 열 1" "$da" "hidden=1"
  expect_grep "적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "서버: MEMO HK" "$v" "HK"
  echo "=== Oracle ⑩ 키 없는 표의 완전 중복 행 = ROWID로 정확히 1행"
  printf "INSERT INTO NSQLT_GE2 SELECT * FROM NSQLT_GE2 WHERE A = 'z';\nCOMMIT;\n" > "$D/ora_dup.sql"; cli ORA "$D/ora_dup.sql" >/dev/null 2>&1
  run_gui 24 "$ORA" "open:$D/ora_sel2.sql,@after:4000:run.all,@after:10000:grid.edit.set:1;2;ONE,@after:11000:grid.edit.cmd:row.save,@after:17000:grid.dump:$O/o4.txt"
  d=$(cat "$O/o4.txt" 2>/dev/null); v=$(cli ORA "$D/ora_sel2.sql")
  expect_grep "적용 뒤 깨끗" "$d" "dirty=false"; expect_grep "서버: ONE 1행" "$(echo "$v" | grep -c ONE)" "^1$"; expect_grep "서버: 원래 x 1행 남음" "$(echo "$v" | grep -c ' x')" "^1$"
  cli ORA "$D/ora_drop.sql" >/dev/null 2>&1
fi
echo "=== 결과: PASS $pass · FAIL $fail (출력 $O)"
[ "$fail" -eq 0 ]
