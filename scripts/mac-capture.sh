#!/usr/bin/env bash
# mac-capture.sh — macOS 위키/매뉴얼 캡처 자동화(T-103 · 사용자 09-17 "맥 기준 캡처로 매뉴얼").
#   `NSQL_HOME`을 임시 폴더로 돌려 실서버 프로필이 섞이지 않게 하고, 데모 SQLite(examples/demo.sql)로 창마다 찍는다.
#   창 좌표는 System Events(접근성 권한 필요)에서 얻어 `screencapture -R`로 그 창만 담는다 · 키 입력도 System Events.
# 사용:  scripts/mac-capture.sh [출력 폴더=target/capture-mac]   (APP=… NSQL=… 로 실행 파일 재지정)
#   결과: 01-login.png · 02-main.png · 03-fetch.png · 04-find.png · 05-search.png · 06-palette.png · 07-log.png ·
#         08-settings.png · 09-connections.png  → docs/wiki/images/ 로 복사해 위키가 참조.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$ROOT/target/capture-mac}
APP=${APP:-$ROOT/target/release/nexa-sql}
NSQL=${NSQL:-$ROOT/target/release/nsql}
mkdir -p "$OUT"
export NSQL_HOME="$OUT/home"
rm -rf "$NSQL_HOME"; mkdir -p "$NSQL_HOME"
# 데모 DB는 짧은 경로에(접속 창 Target 열에 그대로 보인다).
DEMO_DIR=/tmp/nexa-sql-demo; mkdir -p "$DEMO_DIR"; DB="$DEMO_DIR/demo.sqlite"; rm -f "$DB"
"$NSQL" run -c "sqlite:$DB" "$ROOT/examples/demo.sql" >/dev/null
"$NSQL" conn add Demo "sqlite:$DB" --no-prompt >/dev/null
# ★ 프로세스는 **PID**로 고른다 — 이름으로 고르면 같이 떠 있는 다른 nexa-sql(실서버 접속 중일 수 있음)에 키를 보낸다.
PROC=''
se() { osascript -e "tell application \"System Events\" to $1"; }
act() { se "tell $PROC to set frontmost to true" >/dev/null; sleep 0.4; }
key() { if [ -n "${2:-}" ]; then se "key code $1 using {$2}" >/dev/null; else se "key code $1" >/dev/null; fi; sleep 0.5; }
typ() { osascript -e "tell application \"System Events\" to keystroke \"$1\"" >/dev/null; sleep 0.3; }
cap() {
  local b x y w h
  b=$(se "tell $PROC to get {position, size} of window ${2:-1}" | tr -d ' ')
  IFS=, read -r x y w h <<<"$b"
  screencapture -x -R "$x,$y,$w,$h" "$OUT/$1.png"
  echo "  $1.png (${w}x${h} @ $x,$y)"
}
APP_PID=
launch() { "$APP" "$@" >/dev/null 2>&1 & APP_PID=$!; PROC="(first process whose unix id is $APP_PID)"; sleep 2.5; act; }
quit() { if [ -n "$APP_PID" ]; then kill "$APP_PID" 2>/dev/null || true; APP_PID=; sleep 0.6; fi; }
trap quit EXIT
RETURN=36; ESC=53; F10=109

echo "▶ 로그인 창(첫 실행 · 설정 없음) → Esc로 닫고 인자의 데모 DB로"
launch "sqlite:$DB"
cap 01-login
key $ESC; sleep 0.6
typ "SELECT e.empno, e.ename, e.job, e.sal, d.dname FROM emp e JOIN dept d ON d.deptno = e.deptno ORDER BY e.sal DESC;"
key $RETURN "command down"; sleep 1.2
cap 02-main

echo "▶ 페치(세그먼트 200 · 더 있음)"
key 17 "command down"          # ⌘T 새 편집기
typ "SELECT * FROM sales ORDER BY id;"
key $RETURN "command down"; sleep 1.2
cap 03-fetch

echo "▶ 찾기"
key 3 "command down"           # ⌘F
typ "SAL"; sleep 0.4
cap 04-find
key $ESC

echo "▶ 파일 검색 패널"
key 3 "command down, shift down"
sleep 0.5; cap 05-search
key 3 "command down, shift down"

echo "▶ 명령 팔레트"
key 35 "command down, shift down"   # ⌘⇧P
sleep 0.5; cap 06-palette
key $ESC

echo "▶ 로그 창"
key $F10; sleep 0.6; cap 07-log; key $F10

echo "▶ 환경 설정"
key 35 "command down, shift down"; typ "Preferences"; sleep 0.4; key $RETURN; sleep 1.2
cap 08-settings
key $ESC; sleep 0.5

echo "▶ 접속 창"
key 45 "command down, shift down"   # ⌘⇧N(접속 창 · ⌘⇧C는 nexa-clip 전역 키와 충돌)
sleep 0.8; cap 09-connections
key $ESC
quit
echo "완료: $OUT"
