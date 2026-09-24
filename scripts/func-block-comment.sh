#!/usr/bin/env bash
# 블록 주석 자동 점검(사용자 09-24 "로직 확인을 자동화") — 키 주입 0 · 격리 NSQL_HOME · 기동 명령만.
#   1) 파일 열기(인자) → 전체 선택 → 블록 주석 → 저장 → 종료 : 파일 = `/* … */`로 감싸졌는가
#   2) 같은 파일로 다시 → 원문으로 돌아오는가(왕복)
#   3) 캐럿만(선택 없음) → 첫 줄만 감싸지는가
# 사용: scripts/func-block-comment.sh [target/debug/nexa-sql]   · 종료 코드 0 = 전부 통과
set -u
APP="${1:-target/debug/nexa-sql}"
OUT="${TMPDIR:-/tmp}/nsql-func-block-comment"
rm -rf "$OUT"; mkdir -p "$OUT/home"
export NSQL_HOME="$OUT/home"
export NSQL_NO_ACTIVATE=1
printf 'demo.prompted=on\nproject.restore_last=off\neditor.undo_persist=off\n' > "$NSQL_HOME/settings.conf"
SRC=$'SELECT\n\tA.*\nFROM\n\tT A\nWHERE 1=1\n;'
fail=0
run_case() { # $1 = 이름 · $2 = 기동 명령 · $3 = 기대 내용
  local name="$1" cmds="$2" want="$3"
  NSQL_STARTUP_CMD="$cmds" "$APP" "$OUT/t.sql" >"$OUT/$name.log" 2>&1 &
  local pid=$!
  local i=0
  while kill -0 "$pid" 2>/dev/null && [ $i -lt 60 ]; do sleep 0.25; i=$((i+1)); done
  if kill -0 "$pid" 2>/dev/null; then kill "$pid" 2>/dev/null; echo "✗ $name: 종료 안 됨(15초)"; fail=1; return; fi
  local got; got=$(cat "$OUT/t.sql")
  if [ "$got" == "$want" ]; then echo "✓ $name"; else echo "✗ $name"; diff <(printf '%s' "$want") <(printf '%s' "$got") | head -12; fail=1; fi
}
printf '%s' "$SRC" > "$OUT/t.sql"
run_case "wrap(select_all)" "@after:900:edit.select_all,@after:1100:edit.toggle_block_comment,@after:1300:file.save,@after:1700:file.exit" $'/* SELECT\n\tA.*\nFROM\n\tT A\nWHERE 1=1\n; */'
run_case "unwrap(round-trip)" "@after:900:edit.select_all,@after:1100:edit.toggle_block_comment,@after:1300:file.save,@after:1700:file.exit" "$SRC"
run_case "caret-only(first line)" "@after:900:edit.toggle_block_comment,@after:1100:file.save,@after:1500:file.exit" $'/* SELECT */\n\tA.*\nFROM\n\tT A\nWHERE 1=1\n;'
[ $fail -eq 0 ] && echo "ALL PASS" || echo "FAILED"
exit $fail
