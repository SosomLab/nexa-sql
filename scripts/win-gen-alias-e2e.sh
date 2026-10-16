#!/usr/bin/env bash
# win-gen-alias-e2e.sh — Generate SQL 별칭·포맷 자체 시험(10-15 사용자 "intel.alias_style = A, B, C인데 소문자 t" ·
#   "alias 대문자 사용 여부 설정 · 기본 사용" · "사용자가 설정한 Format에 맞춰서").
#   SQLite 격리 홈(키 주입 0) · 표 하나 만들고 탐색기 메뉴 ▸ Generate SQL ▸ SELECT → `sqlprev.dump` 판정.
#   케이스: ① 기본(약어 + 대문자 + 포맷 on) = `SC.`/`FROM … SC` · 키워드 대문자(Basic 기본) ② letters = `A.` ③ upper off + abbr = `sc.`
#           ④ format off = 생성 원문 그대로(`    SC.col,` 4칸 들여쓰기 유지 = 포맷터 미적용 증거) ⑤ CLI `nsql cat gen … select alias=letters upper=0` = `a.`
# 사용: scripts/win-gen-alias-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역(130차 mac)
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"   # 절대 경로(프로필 대상 · 협업 V1 지적 10-15)
REPORT="$OUT/gen-alias-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

# run_case <이름> <settings 추가 줄들> → $OUT/<이름>.sp (sqlprev.dump)
run_case() {
  local name=$1 extra=$2 H="$OUT/$1-home"
  rm -rf "$H"; mkdir -p "$H"
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n%b' "$extra" > "$H/settings.conf"
  NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  printf 'CREATE TABLE sales_customer (id INTEGER PRIMARY KEY, cust_name TEXT, created_at TEXT);\n' > "$OUT/$name.sql"
  NSQL_HOME="$H" "$NSQL" run -c Local --no-prompt "$(m "$OUT/$name.sql")" >/dev/null 2>&1
  local cmd="@after:2500:explorer.expand:main,@after:4000:explorer.expand:Tables,@after:5500:explorer.menu:sales_customer,@after:6100:explorer.pick:gen:select,@after:10000:sqlprev.dump:$(w "$OUT/$name.sp")"
  rm -f "$OUT/$name.sp"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 14 "$EXE" Local > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
  [ -f "$OUT/$name.sp" ] || bad "$name sqlprev.dump 없음" "$(tail -5 "$OUT/$name.stderr")"
}
has()  { grep -q -- "$2" "$OUT/$1.sp" 2>/dev/null; }

say "gen-alias-e2e $(date '+%F %T')"
# ① 기본 = 약어 + 대문자 + 포맷(Basic 기본 = 키워드 대문자).
run_case default ""
has default "SC.id"            && ok "① 기본: 별칭 = 약어 대문자(SC.)"            || bad "① SC. 없음" "$(cat "$OUT/default.sp")"
has default "sales_customer SC$" && ok "① 기본: FROM … SC"                 || bad "① FROM … SC 없음" "$(cat "$OUT/default.sp")"
has default "SC.id = :id"      && ok "① 기본: WHERE 키 = 바인드(SC.id = :id)"    || bad "① WHERE 없음" "$(cat "$OUT/default.sp")"
# ② letters = A.
run_case letters "intel.alias_style=letters\n"
has letters "A.id"             && ok "② letters: 별칭 = A."                      || bad "② A. 없음" "$(cat "$OUT/letters.sp")"
has letters "sales_customer A$" && ok "② letters: FROM … A"                 || bad "② FROM … A 없음" "$(cat "$OUT/letters.sp")"
# ③ upper off + abbr = sc.
run_case lower "intel.alias_upper=off\n"
has lower "sc.id"              && ok "③ upper off: 별칭 = sc."                   || bad "③ sc. 없음" "$(cat "$OUT/lower.sp")"
# ④ format off = 생성 원문(4칸 들여쓰기 `    SC.` 그대로 · 포맷터면 들여쓰기가 설정값으로 바뀐다).
run_case nofmt "gen.format=off\n"
has nofmt "^    SC.id,"        && ok "④ format off: 생성 원문 들여쓰기 4칸 유지"  || bad "④ 원문 아님" "$(cat "$OUT/nofmt.sp")"
# ①에서 포맷이 실제로 돌았는가 = 원문(4칸) 모양과 달라야 한다(Basic 기본 indent = 탭/설정값).
if has default "^    SC.id,"; then bad "① 포맷 미적용(원문 4칸 그대로)" "$(cat "$OUT/default.sp")"; else ok "① 포맷 적용(원문 4칸 들여쓰기 아님)"; fi
# ⑤ CLI = 설정 + 토큰.
H="$OUT/default-home"
cli=$(NSQL_HOME="$H" "$NSQL" cat -c Local -s main gen select sales_customer alias=letters upper=0 2>&1)
echo "$cli" > "$OUT/cli.txt"
echo "$cli" | grep -q "a.id"   && ok "⑤ CLI alias=letters upper=0 = a."         || bad "⑤ CLI a. 없음" "$cli"
cli2=$(NSQL_HOME="$H" "$NSQL" cat -c Local -s main gen select sales_customer 2>&1)
echo "$cli2" | grep -q "SC.id" && ok "⑤ CLI 기본 = SC."                          || bad "⑤ CLI SC. 없음" "$cli2"
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
