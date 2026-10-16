#!/usr/bin/env bash
# win-objbackup-e2e.sh — 객체 변경 전 백업(docs/114 §7-2 · D-302 · D-305 · 10-15) 자체 시험. SQLite 격리 홈 · 키 주입 0 ·
#   편집기 실행(`run.all`)이 DROP/ALTER 전에 현재 DDL을 `<홈>/backups/objects/<프로필>_<스키마>.<객체>_<종류>.<17자리 시각>`으로 쓰는가.
#   ① DROP VIEW → 백업 1 + 뷰 사라짐 ② ALTER TABLE → 백업 1(TABLE) + 컬럼 추가됨 ③ 없는 객체 DROP → 백업 0(건너뜀) · 실행은 오류
#   ④ `backup.object_drop=off` → DROP 백업 0 ⑤ `backup.object_keep=1` + ALTER 둘 → 같은 줄기 파일 1 ⑥ 백업 파일 머리 주석(action) ⑦ 파일 이름 규칙(17자리 숫자 확장자).
# 사용: scripts/win-objbackup-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역(130차 mac)
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
REPORT="$OUT/objbackup-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -14 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"
mk_home() { # <추가 설정>
  rm -rf "$H"; mkdir -p "$H"
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n%b' "$1" > "$H/settings.conf"
  NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  cat > "$OUT/mk.sql" <<'EOF'
CREATE TABLE t_a (id INTEGER PRIMARY KEY, v TEXT);
CREATE VIEW v_a AS SELECT id FROM t_a;
EOF
  NSQL_HOME="$H" "$NSQL" run -c Local --no-prompt "$(m "$OUT/mk.sql")" >/dev/null 2>&1
}
gui() { # <이름> <명령> <초>
  local name=$1 cmd=$2 secs=$3
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL "$secs" "$EXE" Local > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
}
# 편집기에 파일을 열고 실행 → 백업은 비동기(탐색기 메타 스레드)라 넉넉히 기다린 뒤 종료.
run_file() { # <이름> <sql> [추가 명령]
  local name=$1 sql=$2 extra=${3:-}
  printf '%s\n' "$sql" > "$OUT/$name.sql"
  gui "$name" "@connected:open:$(w "$OUT/$name.sql"),@connected:@after:1500:run.all,@connected:@after:6000:objbk.dump:$(w "$OUT/$name.objbk"),@connected:@after:6200:output.dump:$(w "$OUT/$name.out")$extra" 12
}
BK="$H/backups/objects"
count() { ls "$BK" 2>/dev/null | grep -c "^$1\." ; }
tables() { NSQL_HOME="$H" "$NSQL" cat -c Local tables 2>&1; }
views() { NSQL_HOME="$H" "$NSQL" cat -c Local views 2>&1; }

say "objbackup-e2e $(date '+%F %T')"
mk_home ""
gui warm "@after:1500:ui.move:10/10" 4
# ① DROP VIEW v_a → 백업 Local_main.v_a_VIEW.<17자리> 1 + 뷰 사라짐.
run_file a "DROP VIEW v_a;"
[ "$(count Local_main.v_a_VIEW)" = "1" ] && ok "① DROP VIEW → 백업 파일 1" || bad "① 백업 파일 수 $(count Local_main.v_a_VIEW)" "$(ls "$BK" 2>/dev/null; cat "$OUT/a.stderr" | tail -3)"
views | grep -q "v_a" && bad "① 뷰가 남음(DROP 안 됨)" "$(views)" || ok "① DROP 실행됨(뷰 사라짐)"
f=$(ls "$BK"/Local_main.v_a_VIEW.* 2>/dev/null | head -1)
if [ -n "$f" ]; then
  ext="${f##*.}"
  [ "${#ext}" = "17" ] && echo "$ext" | grep -Eq '^[0-9]{17}$' && ok "⑦ 확장자 = 17자리 시각($ext)" || bad "⑦ 확장자 모양 $ext"
  grep -q "action: drop" "$f" && grep -q "CREATE VIEW" "$f" && ok "⑥ 머리 주석 action: drop + DDL 본문" || bad "⑥ 본문 모양" "$(head -4 "$f")"
fi
grep -q "Object backup: main.v_a (VIEW)" "$OUT/a.out" 2>/dev/null && ok "① Output 한 줄(객체 · 파일)" || bad "① Output 백업 줄 없음" "$(cat "$OUT/a.out" 2>/dev/null | head -5)"
# ② ALTER TABLE t_a ADD COLUMN → 백업 TABLE 1 + 컬럼 추가.
run_file b "ALTER TABLE t_a ADD COLUMN c2 TEXT;"
[ "$(count Local_main.t_a_TABLE)" = "1" ] && ok "② ALTER → 백업 파일 1(TABLE)" || bad "② 백업 파일 수 $(count Local_main.t_a_TABLE)" "$(ls "$BK" 2>/dev/null)"
NSQL_HOME="$H" "$NSQL" cat -c Local columns t_a 2>&1 | grep -qi "c2" && ok "② ALTER 실행됨(c2 있음)" || bad "② ALTER 안 됨" "$(NSQL_HOME="$H" "$NSQL" cat -c Local columns t_a 2>&1 | head -5)"
# ③ 없는 객체 DROP → 백업 0 · 건너뜀 로그 · 실행은 서버 오류(막지 않음).
run_file c "DROP TABLE no_such_t;"
[ "$(count Local_main.no_such_t_TABLE)" = "0" ] && ok "③ 없는 객체 = 백업 0" || bad "③ 없는 객체에 백업 생김" "$(ls "$BK")"
grep -q "does not exist yet - skipped" "$OUT/c.out" 2>/dev/null && ok "③ 건너뜀 안내(Output)" || bad "③ 건너뜀 안내 없음" "$(cat "$OUT/c.out" 2>/dev/null | head -5)"
# ④ 끔 = 백업 0.
mk_home "backup.object_drop=off\n"
run_file d "DROP VIEW v_a;"
[ "$(count Local_main.v_a_VIEW)" = "0" ] && ok "④ backup.object_drop=off → 백업 0" || bad "④ 꺼도 백업" "$(ls "$BK")"
views | grep -q "v_a" && bad "④ 뷰 남음" || ok "④ 끔이어도 DROP 실행"
# ⑤ keep 1: ALTER 둘 → 같은 줄기 파일 1(최신만).
mk_home "backup.object_keep=1\n"
run_file e "ALTER TABLE t_a ADD COLUMN c3 TEXT;"
run_file e2 "ALTER TABLE t_a ADD COLUMN c4 TEXT;"
[ "$(count Local_main.t_a_TABLE)" = "1" ] && ok "⑤ keep=1 → 같은 객체 파일 1(최신)" || bad "⑤ 파일 수 $(count Local_main.t_a_TABLE)" "$(ls "$BK")"
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
