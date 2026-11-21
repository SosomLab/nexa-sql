#!/usr/bin/env bash
# todo-check.sh — docs/TODO.md 상태 칸 점검(사용자 11-21 · 갱신 안 된 이슈로 재개발·결정 번복이 생기지 않게 · docs/16 §2-10).
#   읽기만 한다. 실패(a 열린 행인데 끝남 · b 닫힌 행인데 남음 · d 머리 형식 위반)가 있으면 종료 1 ·
#   경고(c 머리 날짜 뒤 커밋 · e 묵은 🚧 · f 결정 대기인데 결정 기록 있음)는 `--strict`일 때만 종료 1.
# 언제: 세션 시작 · "개발 잔여 확인" 요청 · 기록(TODO·STATUS) 커밋 직전.
# 사용:  scripts/todo-check.sh               # 전부 출력
#        scripts/todo-check.sh --quiet       # 건수만
#        scripts/todo-check.sh --strict      # 경고도 실패로
#        scripts/todo-check.sh --stale-days 45
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PY="$(command -v python3 || command -v python)"
[[ -n "${PY}" ]] || { echo "python 없음"; exit 2; }
# Windows pyenv 셰임은 인자의 `|`를 깨뜨린다 → 인자에 파이프를 넘기지 않는다 · 출력은 UTF-8.
PYTHONIOENCODING=utf-8 "${PY}" "${ROOT}/scripts/lib/todo_check.py" --root "${ROOT}" "$@"
