#!/usr/bin/env bash
# cond-pick-e2e.sh — ★ 조건 메뉴 열 이름 인용 정책 E2E(T-325 2차 · 11-04): 격리 홈 + sqlite · 헤드리스 · 기동 명령만(키·마우스 주입 0).
#   `grid.cond.pick:<행>;<열>;<what>` = 그 셀 우클릭 ▸ 조건 ▸ 항목과 같은 길(`grid.rs cond_pick_at`) → `grid.cond.dump:` 산출의 `text=`로 판정.
#   판정 = 예약어(`key` = SQLite 키워드 표) · 특수문자(`my col`) = 인용 · 보통 이름(`normal`) = 비인용 · `editor.quote_idents=always` = 전부 인용.
#   산출물 = target/cond-pick-e2e/ 한 폴더(끝나면 격리 홈은 지운다 · 덤프는 남긴다).
# 사용: scripts/cond-pick-e2e.sh [-e target/debug/nexa-sql(.exe)] [-n target/debug/nsql(.exe)]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXT=""; case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) EXT=".exe";; esac
APP="$ROOT/target/debug/nexa-sql$EXT"; CLI="$ROOT/target/debug/nsql$EXT"
while getopts "e:n:" o; do case $o in e) APP=$OPTARG;; n) CLI=$OPTARG;; esac; done
O="$ROOT/target/cond-pick-e2e"; H="$O/home"; D="$O/data"; rm -rf "$O"; mkdir -p "$H" "$D"
w() { case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) cygpath -w "$1";; *) echo "$1";; esac; }
HW=$(w "$H"); DW=$(w "$D"); OW=$(w "$O")
NSQL_HOME=$HW $CLI conn add Local "sqlite:$(w "$H/local.sqlite")" -d sqlite --no-prompt >/dev/null 2>&1
printf 'CREATE TABLE t("key" TEXT, normal TEXT, "my col" TEXT, n INTEGER);\nINSERT INTO t VALUES (%s, %s, %s, 7);\n' "'k1'" "'v1'" "'m1'" \
  | NSQL_HOME=$HW $CLI run -c Local - >/dev/null 2>&1
printf 'SELECT * FROM t;\n' > "$D/q.sql"
conf() { printf 'ui.lang=en\ndemo.prompted=on\nwindow.main_size=1375,945\n%s\n' "$1" > "$H/settings.conf"; }
run() { # id cmd secs
  local id=$1 cmd=$2 secs=$3
  NSQL_HOME=$HW NSQL_HEADLESS=1 NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" "$APP" Local >"$O/$id.out" 2>"$O/$id.err" &
  local p=$!; sleep "$secs"; kill -0 $p 2>/dev/null && kill $p
}
pass=0; fail=0; v() { if eval "$2"; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1"; fail=$((fail+1)); fi; }
has() { grep -q -F -- "$2" "$1" 2>/dev/null; }
Q="$DW/q.sql"

echo "== A 기본(needed): 예약어·특수문자만 인용 · 값 술어 · 숫자 열"
conf ''
run a "open:$Q,@after:2500:run.all,@after:5000:grid.cond.pick:0;0;null,@after:5300:grid.cond.pick:0;1;eq,@after:5600:grid.cond.pick:0;2;notnull,@after:5900:grid.cond.pick:0;3;eq,@after:6300:grid.cond.dump:$OW/a.txt,@after:6800:file.exit" 8
v "a key = SQLite 키워드 → 인용" 'has "$O/a.txt" "\"key\" IS NULL"'
v "a normal = 비인용 + 값 리터럴" 'has "$O/a.txt" " AND normal = '"'"'v1'"'"'"'
v "a my col = 공백 → 인용" 'has "$O/a.txt" "\"my col\" IS NOT NULL"'
v "a n = 숫자 열 리터럴 따옴표 없음" 'has "$O/a.txt" " AND n = 7"'

echo "== B always: 보통 이름도 인용"
conf 'editor.quote_idents=always'
run b "open:$Q,@after:2500:run.all,@after:5000:grid.cond.pick:0;1;null,@after:5300:grid.cond.pick:0;3;ne,@after:5700:grid.cond.dump:$OW/b.txt,@after:6200:file.exit" 8
v "b normal → \"normal\"" 'has "$O/b.txt" "\"normal\" IS NULL"'
v "b n → \"n\" <> 7" 'has "$O/b.txt" "\"n\" <> 7"'

echo "== C 범위 밖 셀 = 아무 일 없음 · 패닉 없음"
conf ''
run c "open:$Q,@after:2500:run.all,@after:5000:grid.cond.pick:9;9;null,@after:5400:grid.cond.dump:$OW/c.txt,@after:5900:file.exit" 7
v "c 조건 글 비어 있음" 'has "$O/c.txt" "text= focused="'
v "stderr에 panicked 없음" '! grep -l "panicked" "$O"/*.err >/dev/null 2>&1'
rm -rf "$H"
echo "RESULT pass=$pass fail=$fail (dumps: $O)"
[ "$fail" = 0 ]
