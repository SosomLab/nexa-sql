#!/usr/bin/env bash
# conn-profile-e2e.sh — ★ 접속 프로필 보강 E2E(T-31b · D-457~D-459 · 11-04): 격리 홈 · CLI만(sqlite · 서버 0) · 키 주입 0.
#   ① 삭제 = .deleted 보관 → `conn deleted` 목록 → 접속 실패 → `conn restore` → 접속 됨
#   ② 읽기 전용 = `--readonly` / `conn readonly on|off` → UPDATE·CREATE 거절(exit≠0 · "Read-only") · SELECT 통과 · 끄면 통과
#   ③ 내보내기/가져오기 = 비밀번호 없음 · 변수 포함 · 새 홈에 import → 같은 이름 skip → --overwrite = replaced
#   산출물 = target/conn-profile-e2e/(끝나면 격리 홈 삭제 · 로그만 남김).
# 사용: scripts/conn-profile-e2e.sh [-n target/debug/nsql(.exe)]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXT=""; case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) EXT=".exe";; esac
CLI="$ROOT/target/debug/nsql$EXT"
while getopts "n:" o; do case $o in n) CLI=$OPTARG;; esac; done
O="$ROOT/target/conn-profile-e2e"; H1="$O/home1"; H2="$O/home2"; rm -rf "$O"; mkdir -p "$H1" "$H2"
w() { case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) cygpath -w "$1";; *) echo "$1";; esac; }
H1W=$(w "$H1"); H2W=$(w "$H2"); OW=$(w "$O")
pass=0; fail=0; v() { if eval "$2"; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1"; fail=$((fail+1)); fi; }
n1() { NSQL_HOME=$H1W "$CLI" "$@"; }
n2() { NSQL_HOME=$H2W "$CLI" "$@"; }
printf 'ui.lang=en\nconnect.deleted_keep_days=7\n' > "$H1/settings.conf"
printf 'ui.lang=en\n' > "$H2/settings.conf"
printf 'SELECT 1 AS one;\n' > "$O/sel.sql"
printf 'CREATE TABLE t_ro(a INTEGER);\n' > "$O/ddl.sql"
printf 'UPDATE t_x SET a = 1;\n' > "$O/upd.sql"

echo "== ① 삭제 보관 · 복구"
n1 conn add p1 "sqlite:$(w "$H1/p1.sqlite")" -d sqlite --no-prompt >/dev/null 2>&1
n1 conn var p1 REGION=kr >/dev/null 2>&1
v "① add p1" 'n1 conn list 2>/dev/null | grep -q "^p1 "'
RM=$(n1 conn rm p1 2>&1)
v "① rm = .deleted 안내(7일)" 'echo "$RM" | grep -q "\.deleted (kept 7 days"'
v "① 목록에서 사라짐 · 파일 수 0" '! n1 conn list 2>/dev/null | grep -q "^p1 " && [ "$(ls "$H1/profiles"/*.conf 2>/dev/null | wc -l)" = 0 ]'
v "① conn deleted에 p1" 'n1 conn deleted 2>/dev/null | grep -q " p1 "'
v "① 지운 프로필로 접속 = 실패" '! n1 run -c p1 --no-prompt "$(w "$O/sel.sql")" >/dev/null 2>&1'
v "① restore" 'n1 conn restore p1 2>&1 | grep -q "restored"'
v "① 복구 뒤 접속 · 변수 보존" 'n1 run -c p1 --no-prompt -f csv "$(w "$O/sel.sql")" 2>/dev/null | grep -q "^1" && n1 conn var p1 2>/dev/null | grep -q "REGION=kr"'
v "① 살아 있는 이름 restore = 오류" '! n1 conn restore p1 >/dev/null 2>&1'

echo "== ② 읽기 전용"
n1 conn add ro "sqlite:$(w "$H1/ro.sqlite")" -d sqlite --no-prompt --readonly >/dev/null 2>&1
v "② --readonly 저장 = readonly on" 'n1 conn readonly ro 2>/dev/null | grep -q "read-only on" && grep -q "^readonly=on" "$H1/profiles/"*.conf'
n1 run -c ro --no-prompt "$(w "$O/ddl.sql")" >"$O/ro_ddl.out" 2>"$O/ro_ddl.err"; RC=$?
v "② CREATE 거절(exit≠0 · Read-only)" '[ "$RC" != 0 ] && grep -q "Read-only" "$O/ro_ddl.err" "$O/ro_ddl.out"'
v "② SELECT 통과" 'n1 run -c ro --no-prompt -f csv "$(w "$O/sel.sql")" 2>/dev/null | grep -q "^1"'
v "② 접속 줄에 read-only 표시" 'n1 run -c ro --no-prompt "$(w "$O/sel.sql")" 2>/dev/null | grep -q "read-only"'
v "② readonly off" 'n1 conn readonly ro off 2>/dev/null | grep -q "read-only off"'
v "② 끄면 CREATE 통과" 'n1 run -c ro --no-prompt "$(w "$O/ddl.sql")" >/dev/null 2>&1'

echo "== ③ 내보내기 · 가져오기"
n1 conn readonly ro on >/dev/null 2>&1
v "③ export -o 파일(2개 · 비밀번호 없음)" 'n1 conn export -o "$(w "$O/prof.conf")" 2>&1 | grep -q "2 profile(s)" && ! grep -q "secret=" "$O/prof.conf" && grep -q "var.REGION=kr" "$O/prof.conf" && grep -q "readonly=on" "$O/prof.conf"'
v "③ export 이름 지정 = 1개" 'n1 conn export -o "$(w "$O/one.conf")" p1 2>&1 | grep -q "1 profile(s)" && ! grep -q "name=ro" "$O/one.conf"'
v "③ import(새 홈) = 2 added" 'n2 conn import "$(w "$O/prof.conf")" 2>&1 | grep -q "2 added"'
v "③ 가져온 홈 목록 · ro 읽기 전용 유지" 'n2 conn list 2>/dev/null | grep -q "^p1 " && n2 conn readonly ro 2>/dev/null | grep -q "read-only on"'
v "③ 다시 import = 2 skipped" 'n2 conn import "$(w "$O/prof.conf")" 2>&1 | grep -q "2 skipped"'
v "③ --overwrite = 2 replaced" 'n2 conn import "$(w "$O/prof.conf")" --overwrite 2>&1 | grep -q "2 replaced"'
v "③ 빈 파일 import = 오류" 'printf "# x\n" > "$O/empty.conf" && ! n2 conn import "$(w "$O/empty.conf")" >/dev/null 2>&1'
v "③ help conn에 새 하위 명령" 'n1 help conn 2>/dev/null | grep -q "restore <name>" && n1 help conn 2>/dev/null | grep -q -- "--readonly"'

rm -rf "$H1" "$H2"
echo "RESULT pass=$pass fail=$fail (logs: $O)"
[ "$fail" = 0 ]
