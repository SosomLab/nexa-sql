#!/usr/bin/env bash
# mac-openfiles-e2e.sh — ★ T-352 macOS Finder/`open -a` 파일 열기 E2E(130차 mac · 10-17 · nexa-sys `open_files` · Apple Event `odoc`).
#   A) 훅 경로: `NSQL_STARTUP_CMD=os.open:<경로>`(OS 사건 흉내 · `pump_os_open_files` 같은 길) → tabs.dump에 탭 · health.dump `openfiles=true`.
#   B) 실제 LaunchServices 경로: Debug 바이너리 + packaging/macos/Info.plist로 임시 번들(애드혹 서명 · 번들 ID 시험용)을 만들어
#      `open -g -n --env … -a <번들> x.sql`(기동 때 odoc) → 떠 있는 채 `open -g -a <번들> y.sql`(같은 인스턴스에 두 번째 odoc) → 탭 둘.
#   키·마우스 주입 0 · `-g` = 포커스 안 뺏음 · 격리 홈 · 내가 띄운 PID만 종료(번들 실행 파일 경로로 찾는다).
# 사용: scripts/mac-openfiles-e2e.sh -o <출력> [-e target/debug/nexa-sql]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/debug/nexa-sql"; OUT=""
while getopts "o:e:" o; do case $o in o) OUT=$OPTARG;; e) APP=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-e app]"; exit 2; }
[ "$(uname -s)" = Darwin ] || { echo "macOS 전용"; exit 2; }
[ -x "$APP" ] || { echo "앱 없음: $APP"; exit 2; }
case "$OUT" in /*) ;; *) OUT="$ROOT/$OUT";; esac
mkdir -p "$OUT"; H="$OUT/home"; O="$OUT/out"; D="$OUT/docs"; rm -rf "$H" "$O" "$D"; mkdir -p "$H" "$O" "$D"
REPORT="$OUT/openfiles-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); }
has() { grep -qE "$2" "$1" 2>/dev/null; }
printf 'ui.lang=en\ndemo.prompted=on\n' > "$H/settings.conf"
printf 'SELECT 1 AS x;\n' > "$D/x.sql"; printf 'SELECT 2 AS y;\n' > "$D/y.sql"

# ── A) 훅 경로 ─────────────────────────────────────────────────────────────────────────────────────
say "== A) os.open 훅(OS 사건 흉내)"
NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 \
  NSQL_STARTUP_CMD="@after:1500:os.open:$D/x.sql,@after:2500:os.open:$D/y.sql,@after:5000:tabs.dump:$O/a_tabs.txt,@after:5300:health.dump:$O/a_health.txt" \
  "$APP" >/dev/null 2>"$O/a_stderr.txt" &
P=$!; sleep 11; kill "$P" 2>/dev/null; wait "$P" 2>/dev/null
has "$O/a_tabs.txt" 'x\.sql' && ok "A1 훅 x.sql 탭" || bad "A1 훅 x.sql 탭 없음"
has "$O/a_tabs.txt" 'y\.sql' && ok "A2 훅 y.sql 탭" || bad "A2 훅 y.sql 탭 없음"
has "$O/a_health.txt" '^openfiles=true' && ok "A3 openfiles=true(델리게이트 메서드 설치)" || bad "A3 openfiles=true 아님"
has "$O/a_stderr.txt" 'panicked' && bad "A4 stderr 패닉" || ok "A4 stderr 패닉 0"

# ── B) 실제 LaunchServices 경로(임시 번들) ──────────────────────────────────────────────────────────
say "== B) 임시 번들 + open -a (Apple Event odoc)"
B="$OUT/NexaSQLOpenTest.app"; rm -rf "$B"; mkdir -p "$B/Contents/MacOS" "$B/Contents/Resources"
cp "$APP" "$B/Contents/MacOS/nexa-sql"
sed -e 's/@VERSION@/0.0.0-test/g' -e 's#<string>com.sosomlab.nexa-sql</string>#<string>com.sosomlab.nexa-sql.openfiles-test</string>#' \
    "$ROOT/packaging/macos/Info.plist" > "$B/Contents/Info.plist"
codesign --force --sign - "$B" >/dev/null 2>&1 || say "  (애드혹 서명 실패 — 계속)"
EXE="$B/Contents/MacOS/nexa-sql"
bpid() { pgrep -f "^$EXE" | head -1; }
# 기동 때 x.sql(odoc) + 덤프 예약 · 8 s 뒤 떠 있는 인스턴스에 y.sql(두 번째 odoc · `-n` 없음 = 같은 인스턴스).
open -g -n --env "NSQL_HOME=$H" --env "NSQL_NO_ACTIVATE=1" \
  --env "NSQL_STARTUP_CMD=@after:6000:tabs.dump:$O/b1_tabs.txt,@after:12000:tabs.dump:$O/b2_tabs.txt,@after:12300:health.dump:$O/b_health.txt" \
  -a "$B" "$D/x.sql" 2>>"$O/b_open.txt"
sleep 8; open -g -a "$B" "$D/y.sql" 2>>"$O/b_open.txt"
sleep 10
P=$(bpid); n=$(pgrep -f "^$EXE" | wc -l | tr -d ' ')
has "$O/b1_tabs.txt" 'x\.sql' && ok "B1 기동 odoc = x.sql 탭" || bad "B1 기동 odoc x.sql 탭 없음"
has "$O/b2_tabs.txt" 'x\.sql' && has "$O/b2_tabs.txt" 'y\.sql' && ok "B2 떠 있는 인스턴스에 y.sql 추가" || bad "B2 y.sql 탭 없음"
[ "$n" = 1 ] && ok "B3 프로세스 1(두 번째 open이 새 인스턴스를 만들지 않음)" || bad "B3 프로세스 수 = $n"
has "$O/b_health.txt" '^openfiles=true' && ok "B4 openfiles=true" || bad "B4 openfiles=true 아님"
for p in $(pgrep -f "^$EXE"); do kill "$p" 2>/dev/null; done; sleep 1
say "== 결과: PASS $pass · FAIL $fail · 보고 $REPORT"
[ "$fail" = 0 ]
