#!/usr/bin/env bash
# win-maxrows-stop-e2e.sh — "행 수 0 → 실행 → 중지 → 같은 조회 재실행 = 200행" 회귀(사용자 10-09 · 결함 = 중지 오류가 Output 탭을
#   활성화한 뒤 재실행이 활성(Output 자리표시) 그리드의 기본 200을 보냄 → 수정 = 결과가 갈 탭의 그리드 값 `App::run_page_rows`).
#   SQLite 격리 홈 · 키 주입 0 · 훅 `grid.page:<n>`(푸터 입력란과 같은 탭별 값) · 느린 조회 = 상관 부질의(중지가 페치 중간에 걸린다).
#   케이스: ① 푸터 0 → 중지 → 재실행 = 전부(4000) · page=0 · more=false  ② 설정 grid.max_rows=0 같은 흐름  ③ 푸터 50 → (중지 없이 끝남) → 재실행 = 50 · more=true.
#   시각: 실행 1.5 s → 중지 2.3 s(페치 중간) → 재실행 8 s → 덤프 30 s(4,000행 × 부질의 12,000 ≈ 수 초).
# 사용: scripts/win-maxrows-stop-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"
REPORT="$OUT/maxrows-stop-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

# run_case <이름> <설정 max_rows> <훅 page(빈 값 = 없음)> → d1(중지 뒤) · d2(재실행 뒤) 덤프.
run_case() {
  local name=$1 mr=$2 pg=$3 H="$OUT/$1-home"
  rm -rf "$H"; mkdir -p "$H"
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\ngrid.max_rows=%s\n' "$mr" > "$H/settings.conf"
  NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  cat > "$OUT/$name.sql" <<'EOF'
WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n < 4000),
 w(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM w WHERE n < 12000)
SELECT c.n, (SELECT sum(w.n) FROM w WHERE w.n > c.n % 7) AS s FROM c;
EOF
  local hook=""; [ -n "$pg" ] && hook="@after:1500:grid.page:$pg,"
  local cmd="@connected:open:$(w "$OUT/$name.sql"),${hook}@after:1800:run.all,@after:2600:run.stop,@after:7000:grid.dump:$(w "$OUT/$name.d1"),@after:8000:run.all,@after:30000:grid.dump:$(w "$OUT/$name.d2"),@after:30100:result.dump:$(w "$OUT/$name.r2"),@after:30300:log.dump:$(w "$OUT/$name.log")"
  rm -f "$OUT/$name.d1" "$OUT/$name.d2"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 40 "$EXE" Local > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
}

check() {
  local name=$1 want_rows=$2 want_page=$3 want_more=$4 want_cancel=${5:-yes}
  local d2; d2="$(cat "$OUT/$name.d2" 2>/dev/null)"
  if [ "$want_cancel" = yes ]; then
    grep -q "Cancelling the running statement" "$OUT/$name.log" 2>/dev/null && ok "$name 중지가 페치 중간에 걸림" || bad "$name 중지 로그 없음(조회가 너무 빨리 끝남?)" "$(tail -5 "$OUT/$name.log" 2>/dev/null)"
  fi
  echo "$d2" | grep -q "^rows=$want_rows " && ok "$name 재실행 행 수 = $want_rows" || bad "$name 재실행 행 수 ≠ $want_rows" "$d2"
  echo "$d2" | grep -q " page=$want_page\$" && ok "$name 그리드 page = $want_page" || bad "$name 그리드 page ≠ $want_page" "$d2"
  echo "$d2" | grep -q "^fetch more=$want_more " && ok "$name more = $want_more" || bad "$name more ≠ $want_more" "$d2"
}

say "maxrows-stop-e2e $(date '+%F %T')"
run_case footer0 200 0;  check footer0 4000 0 false
run_case setting0 0 "";  check setting0 4000 0 false
run_case footer50 200 50; check footer50 50 50 true no   # 50행은 중지 전에 끝난다 — 탭별 값이 재실행에도 유지되는지만
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
