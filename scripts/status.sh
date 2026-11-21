#!/usr/bin/env bash
# status.sh — 한 화면 보기(사용자 11-21 "어떤 작업이 언제 시작해 어느 정도 진행됐고 언제쯤 끝날지" · D-491 · docs/claude-batch-flow.md §6).
#   ① 작업판(진행 중 · 최근 끝난 일 = target/status/now.md) ② 프로세스(cargo · rustc · link · nexa-sql · nsql · python · gh — 이 저장소 것 위주 ·
#   rustc는 --out-dir로 target 폴더 추정) ③ Debug 실기 창 ④ 저장소(브랜치 · origin 대비 · 미커밋 수) ⑤ 마지막 CI(3초 넘으면 생략).
# 사용: bash scripts/status.sh [--watch](5초마다) [--md](now.md 내용만) · PowerShell에서도: bash scripts/status.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
. "$ROOT/scripts/lib/board.sh"
MODE=once; [ "${1:-}" = "--watch" ] && MODE=watch; [ "${1:-}" = "--md" ] && MODE=md
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) WIN=1;; *) WIN=0;; esac
procs() {
  if [ "$WIN" = 1 ]; then
    pwsh -NoProfile -Command '
      $root = "'"$(cygpath -w "$ROOT")"'"
      $names = @("cargo.exe","rustc.exe","link.exe","rust-lld.exe","nexa-sql.exe","nsql.exe","python.exe","gh.exe","clippy-driver.exe")
      Get-CimInstance Win32_Process | Where-Object { $names -contains $_.Name } | ForEach-Object {
        $c = [string]$_.CommandLine; $p = [string]$_.ExecutablePath
        $mine = ($c -like "*$root*") -or ($p -like "$root*") -or ($_.Name -in @("cargo.exe","rustc.exe","link.exe","rust-lld.exe","clippy-driver.exe"))
        if (-not $mine) { return }
        $td = ""; if ($c -match "--out-dir\s+""?([^"" ]+)") { $td = $Matches[1] -replace [regex]::Escape($root + "\"), "" -replace "\\deps$", "" }
        $cpu = (Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue).CPU
        "{0}`t{1}`t{2}`t{3:N0}`t{4}" -f $_.ProcessId, $_.Name, $_.CreationDate.ToString("HH:mm:ss"), $cpu, $td
      }' </dev/null 2>/dev/null | tr -d '\r'
  else
    ps -eo pid,comm,lstart,time,args 2>/dev/null | grep -E "cargo|rustc|nexa-sql|nsql|gh " | grep -v grep | head -20
  fi
}
show() {
  board_render
  if [ "$MODE" = md ]; then cat "$BOARD_MD"; return; fi
  cat "$BOARD_MD"
  echo; echo "## 프로세스"; echo; echo "| PID | 이름 | 시작 | CPU 초 | target 폴더 |"; echo "|---|---|---|---|---|"
  local p; p=$(procs); if [ -z "$p" ]; then echo "| (없음) | | | | |"; else echo "$p" | awk -F'\t' '{printf "| %s | %s | %s | %s | %s |\n",$1,$2,$3,$4,($5==""?"-":$5)}'; fi
  local dbg; dbg=$(echo "$p" | awk -F'\t' '$2=="nexa-sql.exe"{print $1}' | head -1)
  echo; echo "## Debug 실기 창: $([ -n "$dbg" ] && echo "떠 있음(PID $dbg · 경로 확인은 위 표)" || echo "닫힘")"
  local br ah dirty; br=$(git branch --show-current 2>/dev/null); ah=$(git rev-list --count origin/main..HEAD 2>/dev/null); dirty=$(git status --short 2>/dev/null | wc -l)
  echo; echo "## 저장소: $br · origin/main보다 ${ah:-?} 앞 · 미커밋 $dirty"
  if command -v gh >/dev/null; then
    echo; echo "## 마지막 CI"; timeout 3 gh run list --limit 2 --json workflowName,status,conclusion,headSha,createdAt \
      -q '.[] | "- \(.workflowName) \(.status) \(.conclusion // "-") \(.headSha[0:8]) \(.createdAt)"' 2>/dev/null || echo "- (3초 넘어 생략)"
  fi
}
if [ "$MODE" = watch ]; then while :; do clear; show; sleep 5; done; else show; fi
