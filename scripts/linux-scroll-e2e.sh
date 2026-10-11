#!/usr/bin/env bash
# linux-scroll-e2e.sh — 결과 그리드 스크롤 E2E(10-12 · 사용자 "가로 스크롤이 안 된다" · "스크롤을 클릭하고 움직이면 릴리즈가 안 돼 놓은 뒤에도 움직인다").
#   SQLite 로컬 프로필(격리 NSQL_HOME · 키 주입 0 · 기동 명령만 · 61 §4)에서 30열×400행을 띄우고 **앱 안에서 만든 마우스 사건**(`ui.down/move/up` ·
#   `ui.wheel/hwheel`)으로 ① 가로 휠 = scroll_x 증가 ② 가로 썸 드래그 = 따라옴 ③ 푸터 도구줄 위에서 놓음 = 드래그 끝(버튼 없이 움직여도 scroll_x 그대로)
#   ④ 세로 썸을 조건 줄로 끌어 올려 놓음 = 드래그 끝(scroll_y 그대로) ⑤ 가로 휠 반대 방향 = 되돌아옴 을 판정한다.
#   썸 좌표는 하드코딩이 아니라 훅 `grid.thumbs`(그때의 썸 중심)에서 읽는다 — 다만 기동 명령은 한 줄이라 **같은 창 크기의 1차 실행**에서 좌표를 얻고
#   2차 실행에서 쓴다(두 실행은 같은 격리 홈 · 같은 `window.main_size`).
# 사용: scripts/linux-scroll-e2e.sh -o <출력> [-e target/debug/nexa-sql] [-n target/debug/nsql]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
OUT=""; APP="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"
while getopts "o:e:n:" o; do case $o in o) OUT=$OPTARG;; e) APP=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out> [-e app] [-n nsql]"; exit 2; }
# Windows(Git Bash)에서는 기본 경로에 `.exe`가 붙는다(121차 win · 협업 실측 = 인자만 주면 수정 없이 돈다).
[ -x "$APP" ] || { [ -x "$APP.exe" ] && APP="$APP.exe"; }
[ -x "$NSQL" ] || { [ -x "$NSQL.exe" ] && NSQL="$NSQL.exe"; }
[ -x "$NSQL" ] || NSQL="$ROOT/target/release/nsql"
[ -x "$NSQL" ] || { [ -x "$NSQL.exe" ] && NSQL="$NSQL.exe"; }
OUT=$(realpath -m "$OUT")
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "${H:?}"; mkdir -p "$H"
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1375x945\n' > "$H/settings.conf"
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1 || { echo "Local 프로필 생성 실패"; exit 2; }
R="$OUT/report.txt"; : > "$R"; say() { echo "$*"; echo "$*" >> "$R"; }
pass=0; fail=0; ok() { say "  PASS  $1"; pass=$((pass+1)); }; bad() { say "  FAIL  $1"; fail=$((fail+1)); }
say "=== 그리드 스크롤 E2E · $(date '+%F %T')"
# 30열 × 400행(가로·세로 바 둘 다 필요).
{ printf 'WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<400) SELECT'
  for i in $(seq 1 30); do printf ' x*%d AS column_%02d%s' "$i" "$i" "$([ "$i" -lt 30 ] && echo ,)"; done; printf ' FROM n;\n'; } > "$OUT/wide.sql"
run_app() { NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$OUT/wide.sql,@after:2500:run.all,$1" timeout -s KILL "$2" "$APP" Local >/dev/null 2>"$OUT/$3.stderr"; }
sx() { grep -oE 'scroll_x=-?[0-9]+' "$1" 2>/dev/null | head -1 | cut -d= -f2; }
sy() { sed -n 2p "$1" 2>/dev/null | grep -oE 'scroll_y=-?[0-9]+' | cut -d= -f2; }
# 1차: 바를 깨운 뒤 썸 중심·그리드 사각형.
# 1차는 새 격리 홈의 첫 기동(글꼴·아이콘 캐시 생성)이라 더 기다린다.
run_app "@after:6500:ui.wheel:800/700/0,@after:6700:ui.hwheel:800/700/0,@after:7000:grid.thumbs:$OUT/geo.txt" 10 geo
[ -s "$OUT/geo.txt" ] || { say "  썸 좌표를 얻지 못함(결과 없음?) — 중단"; cat "$OUT/geo.stderr" | tail -3; exit 2; }
V=$(grep '^v=' "$OUT/geo.txt" | cut -d= -f2); HT=$(grep '^h=' "$OUT/geo.txt" | cut -d= -f2); BB=$(grep '^bounds=' "$OUT/geo.txt" | cut -d= -f2)
IFS=, read -r VX VY <<< "$V"; IFS=, read -r HX HY <<< "$HT"; IFS=, read -r BX BY BW BH <<< "$BB"
[ -n "$VX" ] && [ -n "$HX" ] || { say "  썸 없음(v=$V · h=$HT) — 중단"; exit 2; }
CX=$((BX + BW / 2)); CY=$((BY + BH / 2)); FOOT_Y=$((BY + BH - 8)); STRIP_Y=$((BY + 4))
say "  세로 썸 $VX,$VY · 가로 썸 $HX,$HY · 그리드 $BB · 안쪽 점 $CX,$CY · 푸터 y $FOOT_Y · 조건 줄 y $STRIP_Y"
# 2차: 가로 휠 ×3 → 반대 → 가로 썸 드래그 → 푸터에서 놓음 → 버튼 없이 이동 → 세로 썸 드래그 → 조건 줄로 올려 놓음 → 버튼 없이 이동.
C="@after:4500:ui.hwheel:$CX/$CY/120,@after:4700:ui.hwheel:$CX/$CY/120,@after:4900:ui.hwheel:$CX/$CY/120,@after:5200:grid.thumbs:$OUT/a.txt"
C="$C,@after:5400:ui.hwheel:$CX/$CY/-120,@after:5700:grid.thumbs:$OUT/b.txt"
C="$C,@after:6000:ui.hwheel:$CX/$CY/0,@after:6200:grid.thumbs:$OUT/h0.txt"
# 가로 썸 자리는 휠로 움직인 뒤의 값(h0)을 쓴다 — 하지만 기동 명령은 미리 짜야 하므로 썸을 다시 0으로 돌려 1차 좌표를 쓴다.
C="$C,@after:6400:ui.hwheel:$CX/$CY/-600,@after:6600:ui.hwheel:$CX/$CY/-600,@after:6800:grid.thumbs:$OUT/h1.txt"
C="$C,@after:7000:ui.down:$HX/$HY,@after:7200:ui.move:$((HX + 40))/$HY,@after:7400:grid.thumbs:$OUT/c.txt,@after:7600:ui.move:$((HX + 60))/$FOOT_Y,@after:7800:ui.up:$((HX + 60))/$FOOT_Y,@after:8000:grid.thumbs:$OUT/d.txt"
C="$C,@after:8200:ui.move:$CX/$CY,@after:8400:ui.move:$((CX + 80))/$((CY + 20)),@after:8600:grid.thumbs:$OUT/e.txt"
C="$C,@after:9000:ui.wheel:$CX/$CY/0,@after:9200:ui.down:$VX/$VY,@after:9400:ui.move:$VX/$((VY + 60)),@after:9600:grid.dump:$OUT/f.txt,@after:9800:ui.move:$VX/$STRIP_Y,@after:10000:ui.up:$VX/$STRIP_Y,@after:10200:grid.dump:$OUT/g.txt"
C="$C,@after:10400:ui.move:$CX/$CY,@after:10600:ui.move:$CX/$((CY + 60)),@after:10800:grid.dump:$OUT/i.txt"
run_app "$C" 13 main
A=$(sx "$OUT/a.txt"); B=$(sx "$OUT/b.txt"); H1=$(sx "$OUT/h1.txt"); Cc=$(sx "$OUT/c.txt"); D=$(sx "$OUT/d.txt"); E=$(sx "$OUT/e.txt")
F=$(sy "$OUT/f.txt"); G=$(sy "$OUT/g.txt"); I=$(sy "$OUT/i.txt")
say "  가로 휠 ×3 → $A · 반대 → $B · 되돌림 → $H1 · 썸 끌기 → $Cc · 푸터에서 놓음 → $D · 버튼 없이 이동 → $E"
say "  세로 썸 끌기 → $F · 조건 줄에서 놓음 → $G · 버튼 없이 이동 → $I"
[ -n "$A" ] && [ "$A" -gt 0 ] && ok "① 가로 휠 = scroll_x 증가($A)" || bad "① 가로 휠 뒤 scroll_x=$A"
[ -n "$B" ] && [ -n "$A" ] && [ "$B" -lt "$A" ] && ok "⑤ 반대 휠 = 되돌아옴($A → $B)" || bad "⑤ 반대 휠 뒤 scroll_x=$B"
[ "$H1" = 0 ] && ok "되돌림 = 0(썸 좌표 재사용 가능)" || bad "되돌림 뒤 scroll_x=$H1 (썸 좌표 어긋남)"
[ -n "$Cc" ] && [ "$Cc" -gt 0 ] && ok "② 가로 썸 끌기 = 따라옴($Cc)" || bad "② 썸 끌기 뒤 scroll_x=$Cc"
[ -n "$D" ] && [ "$D" = "$E" ] && ok "③ 푸터 위에서 놓음 = 드래그 끝(놓은 뒤 $D · 이동 뒤 $E)" || bad "③ 놓은 뒤 $D ≠ 이동 뒤 $E (드래그 잔존)"
[ -n "$F" ] && [ "$F" -gt 0 ] && ok "세로 썸 끌기 = 따라옴($F)" || bad "세로 썸 끌기 뒤 scroll_y=$F"
[ -n "$G" ] && [ "$G" = "$I" ] && ok "④ 조건 줄 위에서 놓음 = 드래그 끝(놓은 뒤 $G · 이동 뒤 $I)" || bad "④ 놓은 뒤 $G ≠ 이동 뒤 $I (드래그 잔존)"
grep -qi panic "$OUT/main.stderr" && bad "패닉 발생" || ok "패닉 없음"
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
[ "$fail" = 0 ]
