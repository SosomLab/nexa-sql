#!/usr/bin/env bash
# headless.sh — 앱 자체 헤드리스 백엔드(NSQL_HEADLESS=1 · D-436 · T-459 2차 · 11-01) — 3-OS 공통 · xvfb.sh 다음 줄에 포함한다.
#   NSQL_FC_HEADLESS=1 이면 NSQL_HEADLESS=1 NSQL_NO_ACTIVATE=1을 내보낸다 = 창이 화면에 전혀 뜨지 않고(OS 창 0) 포커스·입력·클립보드와 분리.
#   헤드리스에서는 OS가 창을 못 보므로(xwininfo · EnumWindows · CGWindowList = 0) 창 수 판정 = 기동 명령 `windows.dump:<파일>`(첫 줄 wins=N) ·
#   캡처 = `capture:<png>`(주 창) · `capture:<png>@<창 제목 일부>`(보조 창) — 창별 paint 코드 무관 · 3-OS 같은 픽셀.
#   기본 = 꺼짐(종전 = Linux Xvfb(D-435) · Windows/mac 실제 화면) · 켜기 = `NSQL_FC_HEADLESS=1 scripts/linux-func-check.sh …` ·
#   Windows = `win-func-check.ps1 -Headless`. 사용자 실기 인스턴스 스크립트는 이 파일을 포함하지 않는다.
if [ "${NSQL_FC_HEADLESS:-0}" != 0 ]; then
  export NSQL_HEADLESS=1 NSQL_NO_ACTIVATE=1
  echo "[headless] NSQL_HEADLESS=1 — windows stay hidden; window count = windows.dump · capture = capture:<png>[@<title>]" >&2
fi
fc_headless() { [ "${NSQL_HEADLESS:-0}" != 0 ]; }
# 기동 명령 꼬리 = 대기 끝 무렵에 창 목록 덤프(`@after` 시계는 기동 명령 처리 시점부터 · 기동 ≈2 s는 FC_WAIT_SCALE이 덮는다).
#   $1 = 덤프 파일 · $2 = 대기 초(스케일 적용 뒤) → ms = 초×1000×0.75 − 500(최소 500).
fc_wins_tail() {
  local ms; ms=$(awk -v s="$2" 'BEGIN{v=int(s*1000*0.75-500); if (v<500) v=500; printf "%d", v}')
  printf '@after:%s:windows.dump:%s' "$ms" "$1"
}
fc_wins_read() { grep -o 'wins=[0-9]*' "$1" 2>/dev/null | head -1 | cut -d= -f2; }
