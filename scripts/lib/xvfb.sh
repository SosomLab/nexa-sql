#!/usr/bin/env bash
# xvfb.sh — Linux GUI 자동 시험을 **가상 디스플레이(Xvfb)** 안에서(사용자 11-01 "앞으로 리눅스는 xvfb로" · D-435 · T-458).
#   창이 사용자 화면에 뜨지 않고 포커스·클립보드·입력과 완전히 분리된다(docs/61 §4의 "키 주입 0 · 포커스 안 빼앗음"을 OS 층에서 보장).
#   쓰는 법: 스크립트의 ROOT= 줄(또는 timeout.sh 포함 줄) 다음에 `. "$ROOT/scripts/lib/xvfb.sh"` 한 줄.
#     Linux + NSQL_XVFB≠0 + xvfb-run 있음 + 아직 Xvfb 안이 아니면 → 같은 스크립트를 `xvfb-run -a -s "-screen 0 <화면>"` 아래에서 다시 실행하고
#     그 종료 코드로 끝난다(재진입 표식 NSQL_XVFB_ACTIVE=1 · 자식 스크립트는 다시 감싸지 않는다).
#     NSQL_XVFB=0 = 종전(실제 DISPLAY) · xvfb-run 없음 = 경고 한 줄 뒤 종전(`sudo apt install xvfb x11-utils`) · 화면 = NSQL_XVFB_SCREEN(기본 1600x1000x24).
#   같은 DISPLAY 안에서 `xwininfo -root -tree`(창 수) · `import -window`(캡처)가 그대로 동작한다. macOS·Windows에서는 아무것도 하지 않는다.
#   ★ 사용자 실기 인스턴스(linux-restart-debug.sh)는 이 파일을 포함하지 않는다 — 실기 창은 실제 화면에 떠야 한다.
if [ "$(uname -s)" = Linux ] && [ "${NSQL_XVFB:-1}" != 0 ] && [ -z "${NSQL_XVFB_ACTIVE:-}" ]; then
  if command -v xvfb-run >/dev/null 2>&1; then
    export NSQL_XVFB_ACTIVE=1
    echo "[xvfb] re-exec under Xvfb (${NSQL_XVFB_SCREEN:-1600x1000x24}) — NSQL_XVFB=0 uses the real display" >&2
    exec xvfb-run -a -s "-screen 0 ${NSQL_XVFB_SCREEN:-1600x1000x24}" bash "$0" "$@"
  else
    echo "[xvfb] xvfb-run not found — running on the real display (sudo apt install xvfb x11-utils)" >&2
  fi
fi
