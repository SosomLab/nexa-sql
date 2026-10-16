#!/usr/bin/env bash
# timeout.sh — GNU `timeout`이 없는 OS(macOS 기본)를 위한 대역(130차 mac · 10-17 · V3에서 win-*/conn-cmd E2E가 "timeout not found"로 앱을 못 띄움).
#   쓰는 법: 스크립트의 ROOT= 줄 다음에 `. "$ROOT/scripts/lib/timeout.sh"` — 있으면 그대로(GNU) · 없으면 gtimeout(Homebrew coreutils) ·
#   그것도 없으면 bash 대역 `timeout [-s SIG] SECS CMD…`(종료 코드 = 명령 그대로 · 제한 시간에 죽으면 128+신호 = GNU와 같은 꼴).
if ! command -v timeout >/dev/null 2>&1; then
  if command -v gtimeout >/dev/null 2>&1; then
    timeout() { gtimeout "$@"; }
  else
    timeout() {
      local sig=TERM
      if [ "$1" = -s ]; then sig=$2; shift 2; fi
      local secs=$1; shift
      "$@" &
      local p=$!
      ( sleep "$secs"; kill -"$sig" "$p" 2>/dev/null ) &
      local w=$!
      wait "$p"; local rc=$?
      kill "$w" 2>/dev/null; wait "$w" 2>/dev/null
      return $rc
    }
  fi
fi
