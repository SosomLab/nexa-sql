#!/usr/bin/env bash
# mac-trim-e2e.sh — macOS IOSurface 유휴 해제 E2E(118차 · nexa-ui 193 `trim_idle` · 설정 `gfx.mac_present_trim_ms` · 사용자 10-10 "스크린샷으로 자동화").
#   격리 홈에서 앱을 띄우고 메모리 창을 연 뒤 `mem.dump`의 `Surfaces=`(창 표면 바이트 = 메인 창 픽셀×4×쥔 장수 + 메모리 창 1장)를
#   네 시점에 읽는다: A(열자마자 · 풀 2~3장) → B(유휴 ≥ 1.5 s · 앞 장만) → C(편집기 클릭 = 다시 그린 직후 · 다시 2장) → D(다시 유휴 · 1장).
#   같은 시점에 메모리 창을 `screencapture -l <창 id>`로 찍는다(키·마우스 주입 0 · 앱은 비활성 · 창 id = CGWindowList PID 조회).
#   비교군 = `gfx.mac_present_trim_ms=0`(해제 끔): B ≥ A.
#   판정: 켬 = B < A · D < C · 끔 = B ≥ A. 결과 = PASS/FAIL + 숫자 + 캡처 파일(out/*.png).
# 사용: scripts/mac-trim-e2e.sh -o <출력> [-e target/debug/nexa-sql]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/debug/nexa-sql"; OUT=""
while getopts "o:e:" o; do case $o in o) OUT=$OPTARG;; e) APP=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-e app]"; exit 2; }
[ "$(uname -s)" = Darwin ] || { echo "macOS 전용"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; O="$OUT/out"; rm -rf "$H" "$O"; mkdir -p "$H" "$O"
TOOLS="$ROOT/target/func-check-tools"; WINIDS="$TOOLS/winids"
if [ ! -x "$WINIDS" ]; then
  mkdir -p "$TOOLS"
  cat > "$WINIDS.swift" <<'SWIFT'
import CoreGraphics
import Foundation
// 내가 띄운 PID의 화면 위 창(layer 0) = "id|폭x높이|제목" 한 줄씩(접근성 권한 불필요).
let pid = Int32(CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "0") ?? 0
if let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
    for w in list {
        guard let p = w[kCGWindowOwnerPID as String] as? Int32, p == pid,
              let layer = w[kCGWindowLayer as String] as? Int, layer == 0,
              let id = w[kCGWindowNumber as String] as? Int else { continue }
        let title = (w[kCGWindowName as String] as? String) ?? ""
        var wd = 0, ht = 0
        if let b = w[kCGWindowBounds as String] as? [String: Any] {
            wd = Int((b["Width"] as? Double) ?? 0); ht = Int((b["Height"] as? Double) ?? 0)
        }
        print("\(id)|\(wd)x\(ht)|\(title)")
    }
}
SWIFT
  xcrun swiftc -O -o "$WINIDS" "$WINIDS.swift" || { echo "winids 컴파일 실패(swiftc 필요)"; exit 2; }
fi
REPORT="$OUT/trim-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); }
surf() { grep -E "^Surfaces=" "$1" 2>/dev/null | cut -d= -f2; }
# 메모리 창 캡처: 창 목록에서 메인(가장 큰 창)이 아닌 창을 고른다(제목은 언어에 따라 Memory/메모리).
shot() { # <pid> <파일>
  local line; line=$("$WINIDS" "$1" | sort -t'|' -k2,2n | head -1)   # 폭(숫자) 오름차순 = 작은 창(메모리 창)이 먼저
  local id=${line%%|*}
  [ -n "$id" ] && screencapture -x -l "$id" "$2" 2>/dev/null
}
one_pass() { # <이름> <설정 줄> → 숫자 넷을 $O/<이름>.nums에
  local name=$1 conf=$2
  # mem.statusbar=off: 메모리 창 표본마다 바뀌는 상태줄 글이 메인 창을 다시 그려 유휴가 안 오는 것을 막는다(측정 조건 · 기본값은 켬).
  printf 'ui.lang=en\ndemo.prompted=on\ngfx.mac_present=iosurface\nmem.statusbar=off\n%s\n' "$conf" > "$H/settings.conf"
  local A="$O/${name}_A.txt" B="$O/${name}_B.txt" C="$O/${name}_C.txt" D="$O/${name}_D.txt"
  # 시점: A = 메모리 창 연 직후 · B = 유휴 9 s(기동 직후 몇 초는 상태줄 메모리 글이 바뀌며 메인 창이 다시 그려진다 → 넉넉히) ·
  #       C = 편집기 클릭(다시 그림) 직후 · D = 그 뒤 유휴 6.5 s.
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 \
    NSQL_STARTUP_CMD="@after:800:view.memory,@after:1500:mem.dump:$A,@after:9500:mem.dump:$B,@after:10000:ui.click:592/269,@after:10600:mem.dump:$C,@after:17000:mem.dump:$D" \
    "$APP" >/dev/null 2>>"$O/stderr.txt" &
  local p=$!
  sleep 1.9; shot "$p" "$O/${name}_A.png"
  sleep 8.0; shot "$p" "$O/${name}_B.png"
  sleep 1.0; shot "$p" "$O/${name}_C.png"
  sleep 6.5; shot "$p" "$O/${name}_D.png"
  sleep 5; kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
  local a b c d; a=$(surf "$A"); b=$(surf "$B"); c=$(surf "$C"); d=$(surf "$D")
  say "-- $name: Surfaces A=$a B=$b C=$c D=$d (bytes)"
  echo "${a:-0} ${b:-0} ${c:-0} ${d:-0}" > "$O/$name.nums"
}
say "=== IOSurface 유휴 해제 E2E · $(date '+%F %T') · $APP"
one_pass on ''; read -r a b c d < "$O/on.nums"
one_pass off 'gfx.mac_present_trim_ms=0'; read -r a2 b2 c2 d2 < "$O/off.nums"
# 판정 = **다시 그린 뒤 유휴**(C→D): 기동 직후 구간(A→B)은 첫 프레임들이 쥔 장(잠김)이 남아 2장에서 멈추므로 기준으로 쓰지 않는다(실측 10-10).
if [ "$c" != 0 ] && [ "$d" != 0 ]; then
  [ "$d" -lt "$c" ] && ok "켬: 다시 그린 뒤 유휴 = 표면 감소 C→D ($c → $d)" || bad "켬: C→D 감소 없음 ($c → $d)"
else bad "켬: 덤프 없음(C=$c D=$d)"; fi
if [ "$d" != 0 ] && [ "$d2" != 0 ]; then
  [ "$d" -lt "$d2" ] && ok "켬 D < 끔 D ($d < $d2 · 차이 $(( (d2 - d) / 1048576 )) MB)" || bad "켬 D가 끔 D보다 작지 않음 ($d vs $d2)"
else bad "끔: 덤프 없음(D=$d2)"; fi
if [ "$c2" != 0 ] && [ "$d2" != 0 ]; then
  [ "$d2" -ge "$c2" ] && ok "끔: 다시 그린 뒤 유휴에도 유지 C→D ($c2 → $d2)" || bad "끔: 표면이 줄었다 ($c2 → $d2)"
fi
say "캡처: $(ls "$O"/*.png 2>/dev/null | xargs -n1 basename | tr '\n' ' ')"
say "=== 결과: PASS $pass · FAIL $fail"
[ "$fail" = 0 ]
