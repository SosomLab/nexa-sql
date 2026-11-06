#!/usr/bin/env bash
# 전역 세션 규칙 설치 — docs/claude-global-rules.md 의 표식 사이 원문을 ~/.claude/CLAUDE.md 로(3-OS 공통 · 11-06).
#   사용: scripts/install-global-rules.sh            없으면 설치 · 있고 같으면 "같음" · 다르면 diff만 보이고 종료 1
#         scripts/install-global-rules.sh --check    설치 여부·일치 여부만(종료 0 = 같음 · 1 = 없음/다름)
#         scripts/install-global-rules.sh --force    다르면 .bak 남기고 덮어씀
#   대상 폴더는 CLAUDE_HOME 으로 바꿀 수 있다(기본 ~/.claude). 줄끝(CR) 차이는 무시하고 비교한다.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
src="$here/docs/claude-global-rules.md"
dst="${CLAUDE_HOME:-$HOME/.claude}/CLAUDE.md"
force=0; check=0
for a in "$@"; do
  case "$a" in
    --force) force=1 ;;
    --check) check=1 ;;
    -h|--help) sed -n '2,6p' "$0"; exit 0 ;;
    *) echo "모르는 인자: $a" >&2; exit 2 ;;
  esac
done
tmp=$(mktemp -t nsql-global-rules.XXXXXX)
trap 'rm -f "$tmp"' EXIT
awk '/<!-- GLOBAL-RULES-BEGIN -->/{f=1;next} /<!-- GLOBAL-RULES-END -->/{f=0} f' "$src" | tr -d '\r' > "$tmp"
[ -s "$tmp" ] || { echo "원문 표식(GLOBAL-RULES-BEGIN/END)을 못 찾음: $src" >&2; exit 2; }
if [ ! -f "$dst" ]; then
  if [ "$check" = 1 ]; then echo "없음: $dst (설치 필요)"; exit 1; fi
  mkdir -p "$(dirname "$dst")"
  cp "$tmp" "$dst"
  echo "설치됨: $dst"
  exit 0
fi
if diff -q "$tmp" <(tr -d '\r' < "$dst") >/dev/null; then
  echo "같음: $dst"
  exit 0
fi
if [ "$force" = 1 ]; then
  cp "$dst" "$dst.bak"
  cp "$tmp" "$dst"
  echo "덮어씀: $dst (이전 = $dst.bak)"
  exit 0
fi
echo "다름: $dst ↔ 저장소 사본($src) — 아래 diff(- = 현재 PC · + = 사본) 확인 뒤 --force(덮어씀) 또는 사본 갱신"
diff -u <(tr -d '\r' < "$dst") "$tmp" || true
exit 1
