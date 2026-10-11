#!/usr/bin/env bash
# win-paste-run-e2e.sh — "소스 열기로 가져온 함수·프로시저·뷰·트리거 본문을 **새 편집기 탭에 붙여넣고 F5**"가 DBMS마다 정상인지(10-01 · 사용자 점검 요청).
#   CLI로 임시 객체(`NSQLT_*`/`nsqlt_*`)를 만들고 `nsql cat source`로 네 본문을 가져와 한 파일에 이어 붙인 뒤, **Debug GUI를 격리 홈 +
#   기동 명령**(`open:` → `run.all` → `output.dump:`)으로 띄워 일반 탭(객체 탭 아님 · 분할기 경로)의 F5 결과를 Output 덤프로 판정한다.
#   키·마우스 주입 0(docs/61 §4) · 실서버 임시 객체는 끝에 지운다(§2-4 ⑤).
#
# 사용: scripts/win-paste-run-e2e.sh -o <출력폴더> [-e target/debug/nexa-sql.exe] [-n target/debug/nsql.exe] [-P <실제 설정 폴더>]
#        [-p "BISCM:oracle,Repository:postgres,M4PLAN:mssql"]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"; OUT=""; PROF="${APPDATA:-}/nexa-sql"
PROFILES="BISCM:oracle,Repository:postgres,M4PLAN:mssql"
while getopts "o:e:n:P:p:" o; do case $o in o) OUT=$OPTARG;; e) EXE=$OPTARG;; n) NSQL=$OPTARG;; P) PROF=$OPTARG;; p) PROFILES=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir>"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; mkdir -p "$H"; REPORT="$OUT/paste-run-e2e.txt"; : > "$REPORT"
# 격리 홈 = 프로필·기기 키 복사(실제 폴더는 읽기만).
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
run() { printf '%s\n' "$3" > "$OUT/$1_$2.sql"; "$NSQL" run -c "$1" --no-prompt -f csv "$OUT/$1_$2.sql" > "$OUT/$1_$2.out" 2>&1; RC=$?; cat "$OUT/$1_$2.out"; return 0; }
src() { "$NSQL" cat source "$2" "$3" -c "$1" 2>&1; }
gui() { # <프로필> <붙여넣기 파일> <덤프 파일>
  local f; f=$(cygpath -w "$2" 2>/dev/null || echo "$2")
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$f,@after:6000:run.all,@after:14000:output.dump:$3,@after:14500:editor.dump:$3.editor" \
    timeout -s KILL 18 "$EXE" "$1" > "$OUT/gui_$1.stdout" 2> "$OUT/gui_$1.stderr"
}
judge() { # <라벨> <덤프> <기대 완료 줄 수> [compiled 기대 수]
  local d="$2"
  if [ ! -s "$d" ]; then bad "$1: Output 덤프 없음(기동·실행 실패?)" "$(tail -5 "$OUT/gui_$P.stderr" 2>/dev/null)"; return; fi
  local body; body=$(tail -n +2 "$d")
  if echo "$body" | grep -q '✖'; then bad "$1: 오류 줄 있음" "$body"; else ok "$1: 오류 0"; fi
  local done_n; done_n=$(echo "$body" | grep -cE '\[[0-9]+\] (완료|done)')
  if [ "$done_n" -ge "$3" ]; then ok "$1: 문장 완료 $done_n ≥ $3"; else bad "$1: 문장 완료 $done_n < $3" "$body"; fi
  if [ -n "${4:-}" ]; then local c; c=$(echo "$body" | grep -c 'compiled'); if [ "$c" -ge "$4" ]; then ok "$1: compiled $c ≥ $4"; else bad "$1: compiled $c < $4" "$body"; fi; fi
}

for ENTRY in ${PROFILES//,/ }; do
  P="${ENTRY%%:*}"; DIA="${ENTRY##*:}"
  say ""; say "=== $P ($DIA) · $(date '+%F %T') ======================================================"
  PASTE="$OUT/paste_$P.sql"; DUMP="$OUT/dump_$P.txt"; : > "$PASTE"
  case "$DIA" in
    oracle)
      run "$P" 00 'BEGIN
  FOR r IN (SELECT object_name, object_type FROM user_objects WHERE object_name LIKE '"'"'NSQLT!_%'"'"' ESCAPE '"'"'!'"'"' AND object_type IN ('"'"'TRIGGER'"'"','"'"'PROCEDURE'"'"','"'"'FUNCTION'"'"','"'"'VIEW'"'"')) LOOP
    BEGIN EXECUTE IMMEDIATE '"'"'DROP '"'"' || r.object_type || '"'"' '"'"' || r.object_name; EXCEPTION WHEN OTHERS THEN NULL; END;
  END LOOP;
  BEGIN EXECUTE IMMEDIATE '"'"'DROP TABLE NSQLT_T PURGE'"'"'; EXCEPTION WHEN OTHERS THEN NULL; END;
END;
/' >/dev/null
      o=$(run "$P" 10 'CREATE TABLE NSQLT_T (ID NUMBER, NAME VARCHAR2(50));
CREATE OR REPLACE FUNCTION NSQLT_F (P_N IN NUMBER) RETURN NUMBER IS BEGIN RETURN P_N * 2; END NSQLT_F;
/
CREATE OR REPLACE PROCEDURE NSQLT_P (P_N IN NUMBER, P_OUT OUT NUMBER) AS BEGIN P_OUT := NSQLT_F(P_N); END NSQLT_P;
/
CREATE OR REPLACE VIEW NSQLT_V AS SELECT ID, NAME AS NM FROM NSQLT_T;
CREATE OR REPLACE TRIGGER NSQLT_TRG BEFORE INSERT ON NSQLT_T FOR EACH ROW BEGIN :NEW.NAME := UPPER(:NEW.NAME); END;
/'); [ "$RC" -eq 0 ] && ok "임시 객체 생성" || bad "임시 객체 생성" "$o"
      for k in "function NSQLT_F" "procedure NSQLT_P" "view NSQLT_V" "trigger NSQLT_TRG"; do src "$P" $k >> "$PASTE"; printf '\n' >> "$PASTE"; done
      gui "$P" "$PASTE" "$DUMP"
      judge "$P 붙여넣기 F5(함수·프로시저·뷰·트리거)" "$DUMP" 4 4
      run "$P" 90 'DROP TRIGGER NSQLT_TRG;
DROP VIEW NSQLT_V;
DROP PROCEDURE NSQLT_P;
DROP FUNCTION NSQLT_F;
DROP TABLE NSQLT_T PURGE;' >/dev/null; [ "$RC" -eq 0 ] && ok "정리" || bad "정리(종료 $RC)"
      ;;
    postgres)
      run "$P" 00 'DROP TABLE IF EXISTS nsqlt_t CASCADE; DROP FUNCTION IF EXISTS nsqlt_f(int); DROP FUNCTION IF EXISTS nsqlt_tf(); DROP PROCEDURE IF EXISTS nsqlt_p(int);' >/dev/null
      o=$(run "$P" 10 'CREATE TABLE nsqlt_t (id int PRIMARY KEY, name text);
CREATE FUNCTION nsqlt_f(p int) RETURNS int LANGUAGE sql AS $$ SELECT p * 2 $$;
CREATE PROCEDURE nsqlt_p(p int) LANGUAGE plpgsql AS $$ BEGIN PERFORM 1; END $$;
CREATE VIEW nsqlt_v AS SELECT id, name AS nm FROM nsqlt_t;
CREATE FUNCTION nsqlt_tf() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.name := upper(NEW.name); RETURN NEW; END $$;
CREATE TRIGGER nsqlt_trg BEFORE INSERT ON nsqlt_t FOR EACH ROW EXECUTE FUNCTION nsqlt_tf();'); [ "$RC" -eq 0 ] && ok "임시 객체 생성" || bad "임시 객체 생성" "$o"
      for k in "function nsqlt_f" "procedure nsqlt_p" "view nsqlt_v"; do src "$P" $k >> "$PASTE"; printf '\n' >> "$PASTE"; done
      printf 'DROP TRIGGER IF EXISTS nsqlt_trg ON nsqlt_t;\n' >> "$PASTE"; src "$P" trigger nsqlt_trg >> "$PASTE"; printf '\n' >> "$PASTE"
      gui "$P" "$PASTE" "$DUMP"
      judge "$P 붙여넣기 F5(함수·프로시저·뷰·DROP+트리거)" "$DUMP" 5
      run "$P" 90 'DROP TABLE nsqlt_t CASCADE; DROP FUNCTION nsqlt_f(int); DROP FUNCTION nsqlt_tf(); DROP PROCEDURE nsqlt_p(int);' >/dev/null; [ "$RC" -eq 0 ] && ok "정리" || bad "정리(종료 $RC)"
      ;;
    mssql)
      run "$P" 00 "IF OBJECT_ID(N'nsqlt_trg', N'TR') IS NOT NULL DROP TRIGGER nsqlt_trg; IF OBJECT_ID(N'nsqlt_v', N'V') IS NOT NULL DROP VIEW nsqlt_v; IF OBJECT_ID(N'nsqlt_p', N'P') IS NOT NULL DROP PROCEDURE nsqlt_p; IF OBJECT_ID(N'nsqlt_f', N'FN') IS NOT NULL DROP FUNCTION nsqlt_f; IF OBJECT_ID(N'nsqlt_t', N'U') IS NOT NULL DROP TABLE nsqlt_t;" >/dev/null
      o=$(run "$P" 10 'CREATE TABLE nsqlt_t (id int PRIMARY KEY, name nvarchar(50));
GO
CREATE FUNCTION nsqlt_f(@p int) RETURNS int AS BEGIN RETURN @p * 2; END;
GO
CREATE PROCEDURE nsqlt_p @p int AS BEGIN SELECT @p * 2 AS d; END;
GO
CREATE VIEW nsqlt_v AS SELECT id, name AS nm FROM nsqlt_t;
GO
CREATE TRIGGER nsqlt_trg ON nsqlt_t AFTER INSERT AS BEGIN SET NOCOUNT ON; END;
GO'); [ "$RC" -eq 0 ] && ok "임시 객체 생성" || bad "임시 객체 생성" "$o"
      for k in "function nsqlt_f" "procedure nsqlt_p" "view nsqlt_v" "trigger nsqlt_trg"; do src "$P" $k >> "$PASTE"; printf '\n' >> "$PASTE"; done
      gui "$P" "$PASTE" "$DUMP"
      judge "$P 붙여넣기 F5(함수·프로시저·뷰·트리거)" "$DUMP" 4
      run "$P" 90 'DROP TRIGGER nsqlt_trg; DROP VIEW nsqlt_v; DROP PROCEDURE nsqlt_p; DROP FUNCTION nsqlt_f; DROP TABLE nsqlt_t;' >/dev/null; [ "$RC" -eq 0 ] && ok "정리" || bad "정리(종료 $RC)"
      ;;
  esac
done
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
