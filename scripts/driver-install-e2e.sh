#!/usr/bin/env bash
# 확장 드라이버 내려받기·설치·적재 E2E(10-24 · 138차 Linux · 3-OS 공용 · D-350 lite 흐름 검증 · 맥 10-23 §45 "id | 설치 | driver check" 표의 자동판).
#   격리 홈에서: ① `driver search`(실 GitHub raw index.json) ② 색인의 이 OS 드라이버 전부(또는 -i 목록) `driver install <id>`(sha256 + Ed25519 검증)
#   ③ `driver check <id>`(실제 dlopen · 함수표 · abi) ④ `driver list` ⑤ 확장 전용 DuckDB로 왕복(`duckdb:<파일>` CREATE·한글 INSERT·SELECT)
#   ⑥ `driver rm sqlite` → list에서 사라짐. 네트워크 필요(없으면 전부 SKIP).
# 사용: scripts/driver-install-e2e.sh -o <출력폴더> [-n target/debug/nsql] [-i sqlite,duckdb] [-k 홈 유지]
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/timeout.sh"
OUT=""; NSQL="$ROOT/target/debug/nsql"; IDS=""; KEEP=0
while [ $# -gt 0 ]; do case "$1" in -o) OUT=$2; shift 2;; -n) NSQL=$2; shift 2;; -i) IDS=$2; shift 2;; -k) KEEP=1; shift;; *) echo "usage: $0 -o <out> [-n nsql] [-i id,id] [-k]"; exit 2;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir>"; exit 2; }
mkdir -p "$OUT"; REPORT="$OUT/driver-install-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && printf '%s\n' "$2" | head -6 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
skp() { say "  SKIP  $1"; skip=$((skip+1)); }
H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
printf 'driver.auto_install=off\n' > "$H/settings.conf"
run() { NSQL_HOME="$H" timeout -s KILL 300 "$NSQL" "$@" 2>&1; }

say "① driver search(실 색인)"
if ! timeout 6 bash -c 'exec 3<>/dev/tcp/raw.githubusercontent.com/443' 2>/dev/null; then skp "GitHub 미도달 — 전부 건너뜀"; say "결과: PASS 0 · FAIL 0 · SKIP 1"; exit 0; fi
o=$(run driver search --refresh); printf '%s\n' "$o" > "$OUT/search.txt"
n=$(printf '%s\n' "$o" | grep -cE '^(sqlite|pg|mysql|mssql|oracle-thin|oracle-oci|odbc|duckdb)\b')
[ "$n" -ge 1 ] && ok "search = $n drivers in index" || bad "search 결과 없음" "$o"
[ -z "$IDS" ] && IDS=$(printf '%s\n' "$o" | grep -oE '^(sqlite|pg|mysql|mssql|oracle-thin|oracle-oci|odbc|duckdb)\b' | sort -u | tr '\n' ',' | sed 's/,$//')
say "  대상 = $IDS"

say "② install + ③ check"
IFS=, ; for id in $IDS; do unset IFS
  o=$(run driver install "$id"); printf '%s\n' "$o" > "$OUT/install-$id.txt"
  if printf '%s' "$o" | grep -qiE 'installed|설치됨|already installed|이미 설치'; then ok "install $id ($(printf '%s\n' "$o" | tail -1 | grep -oiE '[0-9.]+ MB|(key|키) [a-z0-9-]+' | tr '\n' ' '))"; else bad "install $id" "$o"; continue; fi
  o=$(run driver check "$id"); printf '%s\n' "$o" > "$OUT/check-$id.txt"
  if printf '%s' "$o" | grep -qE '^abi += [0-9]'; then ok "check $id (abi $(printf '%s' "$o" | sed -nE 's/^abi += ([0-9]+).*/\1/p'))"; else bad "check $id(dlopen·함수표)" "$o"; fi
done; unset IFS

say "④ driver list"
o=$(run driver list); printf '%s\n' "$o" > "$OUT/list.txt"
IFS=, ; for id in $IDS; do unset IFS; printf '%s' "$o" | grep -qE "^$id\b" && ok "list has $id" || bad "list missing $id" "$o"; done; unset IFS

say "⑤ DuckDB(확장 전용) 왕복"
if printf ',%s,' "$IDS" | grep -q ',duckdb,'; then
  DB="$OUT/rt.duckdb"; rm -f "$DB" "$DB.wal"
  printf "CREATE TABLE t(id INTEGER, name VARCHAR);\nINSERT INTO t VALUES (1, '한글'), (2, '가나다');\nSELECT id, name FROM t ORDER BY id;\n" > "$OUT/rt.sql"
  o=$(run run -c "duckdb:$DB" "$OUT/rt.sql"); printf '%s\n' "$o" > "$OUT/duckdb-rt.txt"
  if printf '%s' "$o" | grep -q '한글' && printf '%s' "$o" | grep -q '가나다' && printf '%s' "$o" | grep -qE '2 rows?|2행'; then ok "duckdb 왕복 한글 2행(확장 .so + 동봉 libduckdb)"; else bad "duckdb 왕복" "$o"; fi
else skp "duckdb 미대상"; fi

say "⑥ driver rm"
if printf ',%s,' "$IDS" | grep -q ',sqlite,'; then
  o=$(run driver rm sqlite); l=$(run driver list)
  printf '%s' "$l" | grep -qE '^sqlite\b' && bad "rm sqlite 뒤 list에 남음" "$o
$l" || ok "rm sqlite → list에서 사라짐"
else skp "sqlite 미대상"; fi

[ "$KEEP" = 1 ] || rm -rf "$H/drivers"
say "결과: PASS $pass · FAIL $fail · SKIP $skip · 보고 = $REPORT"
[ $fail = 0 ]
