#!/usr/bin/env bash
# dbms-source-e2e.sh — PostgreSQL · SQL Server **소스 열기 보수적 생성 왕복 E2E**(10-01 · docs/100 §5 · T-268).
#   임시 객체(`nsqlt_*`)를 만들고 `nsql cat source`로 가져온 본문이 **스키마 한정·참조 한정** 형태인지 검사한 뒤, 그 본문을
#   그대로 다시 실행해(CREATE OR REPLACE / CREATE OR ALTER) 오류가 없는지 본다. 끝에 전부 지운다(docs/61 §2-4 ⑤).
#   Oracle은 `scripts/oracle-ddl-e2e.sh`(67) · MySQL은 프로필이 없어 순수 함수 시험만(`conservative_qualification_helpers`).
#
# 사용: scripts/dbms-source-e2e.sh -o <출력폴더> [-n target/debug/nsql] [-p "Repository:postgres,M4PLAN:mssql"]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NSQL="$ROOT/target/debug/nsql"; OUT=""; PROFILES="Repository:postgres,M4PLAN:mssql"
while getopts "o:n:p:" o; do case $o in o) OUT=$OPTARG;; n) NSQL=$OPTARG;; p) PROFILES=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-n cli] [-p prof:dialect,...]"; exit 2; }
mkdir -p "$OUT"; REPORT="$OUT/dbms-source-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; RC=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
run() { printf '%s\n' "$3" > "$OUT/$1_$2.sql"; "$NSQL" run -c "$1" --no-prompt -f csv "$OUT/$1_$2.sql" > "$OUT/$1_$2.out" 2>&1; RC=$?; cat "$OUT/$1_$2.out"; return 0; }
rc_ok() { if [ "$RC" -eq 0 ]; then ok "$1"; else bad "$1 (종료 $RC)" "$2"; fi; }
chk()   { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
src()   { "$NSQL" cat source "$2" "$3" -c "$1" 2>&1; }

for ENTRY in ${PROFILES//,/ }; do
  P="${ENTRY%%:*}"; DIA="${ENTRY##*:}"
  say ""; say "=== $P ($DIA) · $(date '+%F %T') ======================================================"
  case "$DIA" in
    postgres)
      o=$(run "$P" 00_clean 'DROP TABLE IF EXISTS nsqlt_t CASCADE;
DROP FUNCTION IF EXISTS nsqlt_f(int);
DROP FUNCTION IF EXISTS nsqlt_tf();
DROP PROCEDURE IF EXISTS nsqlt_p(int);'); rc_ok "사전 정리" "$o"
      o=$(run "$P" 10_create 'CREATE TABLE nsqlt_t (id int PRIMARY KEY, name text);
CREATE VIEW nsqlt_v AS SELECT id, name AS nm FROM nsqlt_t;
CREATE FUNCTION nsqlt_f(p int) RETURNS int LANGUAGE sql AS $$ SELECT p * 2 $$;
CREATE PROCEDURE nsqlt_p(p int) LANGUAGE plpgsql AS $$ BEGIN PERFORM 1; END $$;
CREATE FUNCTION nsqlt_tf() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.name := upper(NEW.name); RETURN NEW; END $$;
CREATE TRIGGER nsqlt_trg BEFORE INSERT ON nsqlt_t FOR EACH ROW EXECUTE FUNCTION nsqlt_tf();'); rc_ok "CREATE TABLE · VIEW · FUNCTION · PROCEDURE · TRIGGER FUNCTION · TRIGGER" "$o"
      s=$(src "$P" function nsqlt_f); chk "함수 소스 = 스키마 한정(pg_get_functiondef)" 'CREATE OR REPLACE FUNCTION "?public"?\.nsqlt_f\(' "$s"
      printf '%s\n' "$s" > "$OUT/${P}_src_f.sql"; o=$(run "$P" 20_rerun_f "$(cat "$OUT/${P}_src_f.sql")"); rc_ok "함수 소스 재실행" "$o"
      s=$(src "$P" procedure nsqlt_p); chk "프로시저 소스 = 스키마 한정" 'CREATE OR REPLACE PROCEDURE "?public"?\.nsqlt_p\(' "$s"
      printf '%s\n' "$s" > "$OUT/${P}_src_p.sql"; o=$(run "$P" 21_rerun_p "$(cat "$OUT/${P}_src_p.sql")"); rc_ok "프로시저 소스 재실행" "$o"
      s=$(src "$P" view nsqlt_v); chk "뷰 소스 = 스키마 한정 머리" 'CREATE OR REPLACE VIEW "?public"?\."?nsqlt_v"? AS' "$s"; chk "뷰 컬럼 별칭 보존(name AS nm)" 'AS nm' "$s"
      printf '%s\n' "$s" > "$OUT/${P}_src_v.sql"; o=$(run "$P" 22_rerun_v "$(cat "$OUT/${P}_src_v.sql")"); rc_ok "뷰 소스 재실행(CREATE OR REPLACE)" "$o"
      s=$(src "$P" trigger nsqlt_trg); chk "트리거 소스 = ON 표 스키마 한정" 'ON "?public"?\.nsqlt_t' "$s"; chk "트리거 함수 참조 스키마 한정" 'EXECUTE FUNCTION "?public"?\.nsqlt_tf\(' "$s"
      printf 'DROP TRIGGER nsqlt_trg ON nsqlt_t;\n%s\n' "$s" > "$OUT/${P}_src_trg.sql"; o=$(run "$P" 23_rerun_trg "$(cat "$OUT/${P}_src_trg.sql")"); rc_ok "트리거 소스 재실행(DROP 뒤 CREATE)" "$o"
      s=$("$NSQL" cat source function nsqlt_f qualify=off -c "$P" 2>&1); chk "CLI qualify=off = pg_get_functiondef 그대로(한정 유지 · PG는 늘 한정)" 'nsqlt_f' "$s"
      o=$(run "$P" 50_dml "INSERT INTO nsqlt_t VALUES (1, 'abc'); SELECT name FROM nsqlt_t; SELECT nsqlt_f(21);"); chk "트리거 동작(upper) = ABC" 'ABC' "$o"; chk "함수 = 42" '42' "$o"
      o=$(run "$P" 90_drop 'DROP TABLE nsqlt_t CASCADE;
DROP FUNCTION nsqlt_f(int);
DROP FUNCTION nsqlt_tf();
DROP PROCEDURE nsqlt_p(int);'); rc_ok "정리 DROP" "$o"
      o=$(run "$P" 91_left "SELECT COUNT(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relname LIKE 'nsqlt_%' AND n.nspname = current_schema();"); chk "남은 임시 객체 = 0" '^0$' "$o"
      ;;
    mssql)
      o=$(run "$P" 00_clean 'IF OBJECT_ID(N'"'"'nsqlt_trg'"'"', N'"'"'TR'"'"') IS NOT NULL DROP TRIGGER nsqlt_trg;
IF OBJECT_ID(N'"'"'nsqlt_v'"'"', N'"'"'V'"'"') IS NOT NULL DROP VIEW nsqlt_v;
IF OBJECT_ID(N'"'"'nsqlt_p'"'"', N'"'"'P'"'"') IS NOT NULL DROP PROCEDURE nsqlt_p;
IF OBJECT_ID(N'"'"'nsqlt_f'"'"', N'"'"'FN'"'"') IS NOT NULL DROP FUNCTION nsqlt_f;
IF OBJECT_ID(N'"'"'nsqlt_t'"'"', N'"'"'U'"'"') IS NOT NULL DROP TABLE nsqlt_t;'); rc_ok "사전 정리" "$o"
      o=$(run "$P" 10_create 'CREATE TABLE nsqlt_t (id int PRIMARY KEY, name nvarchar(50));
GO
CREATE VIEW nsqlt_v AS SELECT id, name AS nm FROM nsqlt_t;
GO
CREATE PROCEDURE nsqlt_p @p int AS BEGIN SELECT @p * 2 AS d; END;
GO
CREATE FUNCTION nsqlt_f(@p int) RETURNS int AS BEGIN RETURN @p * 2; END;
GO
CREATE TRIGGER nsqlt_trg ON nsqlt_t AFTER INSERT AS BEGIN SET NOCOUNT ON; END;
GO'); rc_ok "CREATE TABLE · VIEW · PROCEDURE · FUNCTION · TRIGGER" "$o"
      s=$("$NSQL" cat source procedure nsqlt_p qualify=off -c "$P" 2>&1); chk "CLI qualify=off = 저장 정의 그대로(한정 없음 · 재실행 전)" 'CREATE OR ALTER PROCEDURE nsqlt_p' "$s"
      s=$(src "$P" procedure nsqlt_p); chk "프로시저 소스 = CREATE OR ALTER + [스키마]. 한정" 'CREATE OR ALTER PROCEDURE \[[A-Za-z_]+\]\.nsqlt_p' "$s"
      printf '%s\n' "$s" > "$OUT/${P}_src_p.sql"; o=$(run "$P" 20_rerun_p "$(cat "$OUT/${P}_src_p.sql")"); rc_ok "프로시저 소스 재실행" "$o"
      s=$(src "$P" function nsqlt_f); chk "함수 소스 = [스키마]. 한정" 'CREATE OR ALTER FUNCTION \[[A-Za-z_]+\]\.nsqlt_f\(' "$s"
      printf '%s\n' "$s" > "$OUT/${P}_src_f.sql"; o=$(run "$P" 21_rerun_f "$(cat "$OUT/${P}_src_f.sql")"); rc_ok "함수 소스 재실행" "$o"
      s=$(src "$P" view nsqlt_v); chk "뷰 소스 = [스키마]. 한정 · 별칭 보존" 'CREATE OR ALTER VIEW \[[A-Za-z_]+\]\.nsqlt_v AS SELECT id, name AS nm' "$s"
      printf '%s\n' "$s" > "$OUT/${P}_src_v.sql"; o=$(run "$P" 22_rerun_v "$(cat "$OUT/${P}_src_v.sql")"); rc_ok "뷰 소스 재실행" "$o"
      s=$(src "$P" trigger nsqlt_trg); chk "트리거 소스 = 이름 한정" 'CREATE OR ALTER TRIGGER \[[A-Za-z_]+\]\.nsqlt_trg' "$s"; chk "트리거 ON 표 = 부모 스키마 한정" 'ON \[[A-Za-z_]+\]\.nsqlt_t' "$s"
      printf '%s\n' "$s" > "$OUT/${P}_src_trg.sql"; o=$(run "$P" 23_rerun_trg "$(cat "$OUT/${P}_src_trg.sql")"); rc_ok "트리거 소스 재실행" "$o"
      o=$(run "$P" 50_dml "INSERT INTO nsqlt_t VALUES (1, N'abc'); DECLARE @sql nvarchar(200) = N'SELECT ' + QUOTENAME(SCHEMA_NAME()) + N'.nsqlt_f(21) AS f'; EXEC(@sql); EXEC nsqlt_p 4;"); chk "함수 = 42" '42' "$o"; chk "프로시저 = 8" '8' "$o"
      o=$(run "$P" 90_drop 'DROP TRIGGER nsqlt_trg;
DROP VIEW nsqlt_v;
DROP PROCEDURE nsqlt_p;
DROP FUNCTION nsqlt_f;
DROP TABLE nsqlt_t;'); rc_ok "정리 DROP" "$o"
      o=$(run "$P" 91_left "SELECT COUNT(*) FROM sys.objects WHERE name LIKE 'nsqlt[_]%';"); chk "남은 임시 객체 = 0" '^0$' "$o"
      ;;
    *) say "  (알 수 없는 방언 $DIA)";;
  esac
done
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
