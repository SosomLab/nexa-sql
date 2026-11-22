#!/usr/bin/env bash
# 빌드 산출물을 빠른 디스크(SSD)로 돌린다 — 이 PC의 D는 HDD라 cargo 빌드·링크·시험이 D를 100 % 물어 다른 작업이 멈춘다(사용자 11-22).
#
#   scripts/fast-target.sh status                 지금 어디에 쓰는지 · 빠른 디스크 여유
#   scripts/fast-target.sh on  [--root S:/] [--dirs "debug it"]
#   scripts/fast-target.sh off [--dirs "debug it"]
#
# 방식 = `target/<하위>`(기본 debug · it)만 빠른 디스크의 `<root>/nexa-sql/target/<하위>`로 **접합(junction)** 한다.
#   · `target/` 자체는 D에 남는다 → 작업판(`target/status`) · 검증 요약·이력(`target/verify`)은 임시 디스크를 닫아도 남는다.
#   · 스크립트·문서의 `target/debug/…` 경로는 그대로 쓴다(CARGO_TARGET_DIR을 바꾸지 않는다).
#   · 옛 산출물은 `target/<하위>.hdd`로 이름만 바꿔 둔다(복사 없음 · off 때 되돌린다) → 빠른 디스크에서는 **처음 한 번 다시 빌드**한다.
# 빠른 디스크 = 기본 `S:/`(임시 가상 디스크 `C:\fast-scratch\open.ps1` · 상한 30 GB · 관리자 권한) · 환경 변수 NSQL_FAST_ROOT 또는 --root로 바꾼다.
# 임시 디스크를 닫기(close.ps1) **전에 반드시 off** — 닫으면 접합이 끊겨 빌드가 실패한다(status가 "끊김"으로 알린다 · off로 풀면 된다).
# 켜고 끌 때는 cargo와 target/debug 실행 파일이 돌고 있지 않아야 한다(돌고 있으면 멈춘다).
set -u
cd "$(dirname "$0")/.." || exit 1
ROOT="${NSQL_FAST_ROOT:-S:/}"
DIRS="debug it"
FORCE=0
cmd="${1:-status}"
shift || true
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="$2"; shift 2 ;;
    --dirs) DIRS="$2"; shift 2 ;;
    --force) FORCE=1; shift ;;   # 실행 중 검사 생략(그 하위 폴더를 아무도 쓰지 않는 것이 확실할 때만)
    *) echo "모르는 인자: $1"; exit 2 ;;
  esac
done
ROOT="${ROOT%/}"
DEST="$ROOT/nexa-sql/target"

win() { cygpath -w "$1"; }
is_junction() { [ -L "$1" ] || { [ -d "$1" ] && [ "$(cmd //c "fsutil reparsepoint query \"$(win "$1")\"" 2>/dev/null | grep -c 'Mount Point\|마운트')" != "0" ]; }; }
busy() {
  [ "$FORCE" = "1" ] && { echo 0; return; }
  # 이 저장소 target을 쓰는 cargo·rustc·link · target/debug 실행 파일
  powershell -NoProfile -Command "@(Get-CimInstance Win32_Process | Where-Object { (\$_.Name -match '^(cargo|rustc|link)\.exe\$' -and \$_.CommandLine -like '*nexa-sql*') -or (\$_.ExecutablePath -like '$(win "$PWD")\\target\\*') }).Count" 2>/dev/null | tr -d '\r'
}

case "$cmd" in
  status)
    if [ -d "$ROOT/" ]; then
      echo "빠른 디스크: $ROOT (여유 $(df -h "$ROOT/" 2>/dev/null | awk 'NR==2{print $4}'))"
    else
      echo "빠른 디스크: $ROOT 없음 — 임시 디스크를 먼저 연다(관리자 pwsh: C:\\fast-scratch\\open.ps1)"
    fi
    for d in $DIRS; do
      t="target/$d"
      if is_junction "$t"; then
        if [ -d "$t/" ] && ls "$t/" >/dev/null 2>&1; then echo "  $t → 빠른 디스크(접합)"; else echo "  $t → ⚠ 끊김(대상 없음) — 'off'로 푼다"; fi
      elif [ -d "$t" ]; then echo "  $t = D(HDD)"
      else echo "  $t 없음"; fi
      [ -d "$t.hdd" ] && echo "    (옛 산출물 보관: $t.hdd)"
    done
    ;;
  on)
    [ -d "$ROOT/" ] || { echo "빠른 디스크 $ROOT 없음 — 관리자 pwsh에서 C:\\fast-scratch\\open.ps1 을 먼저 실행"; exit 1; }
    n="$(busy)"; [ "${n:-0}" = "0" ] || { echo "cargo 또는 target 실행 파일이 돌고 있음($n) — 끝낸 뒤 다시"; exit 1; }
    mkdir -p target
    for d in $DIRS; do
      t="target/$d"
      if is_junction "$t"; then echo "  $t 이미 접합"; continue; fi
      if [ -d "$t" ]; then
        [ -e "$t.hdd" ] && { echo "  $t.hdd 가 이미 있음 — 건너뜀(직접 확인)"; continue; }
        mv "$t" "$t.hdd" || { echo "  $t 이름 바꾸기 실패(열린 파일?)"; continue; }
      fi
      mkdir -p "$DEST/$d"
      powershell -NoProfile -Command "New-Item -ItemType Junction -Path '$(win "$PWD/target/$d")' -Target '$(win "$DEST/$d")' | Out-Null" >/dev/null 2>&1
      if is_junction "$t"; then echo "  $t → $DEST/$d"; else echo "  $t 접합 실패"; [ -d "$t.hdd" ] && mv "$t.hdd" "$t"; fi
    done
    echo "켬 — 다음 빌드는 빠른 디스크에서 처음부터(한 번). 임시 디스크를 닫기 전에 'off'."
    ;;
  off)
    n="$(busy)"; [ "${n:-0}" = "0" ] || { echo "cargo 또는 target 실행 파일이 돌고 있음($n) — 끝낸 뒤 다시"; exit 1; }
    for d in $DIRS; do
      t="target/$d"
      if is_junction "$t"; then
        powershell -NoProfile -Command "[IO.Directory]::Delete('$(win "$PWD/target/$d")')" >/dev/null 2>&1   # 접합만 지운다(대상 내용은 그대로 · 재귀 아님)
        is_junction "$t" && { echo "  $t 접합 풀기 실패"; continue; }
        echo "  $t 접합 풂"
      fi
      if [ -d "$t.hdd" ] && [ ! -e "$t" ]; then mv "$t.hdd" "$t" && echo "  $t ← 옛 산출물 되돌림"; fi
    done
    echo "끔 — 빌드는 다시 D에. 빠른 디스크의 산출물($DEST)은 임시 디스크를 닫으면 사라진다."
    ;;
  *) echo "사용: $0 status|on|off [--root <경로>] [--dirs \"debug it\"]"; exit 2 ;;
esac
