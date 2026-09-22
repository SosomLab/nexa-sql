#!/usr/bin/env bash
# wiki-publish.sh — `docs/wiki/*.md` + `docs/wiki/images/`를 GitHub 위키 저장소(nexa-sql.wiki)로 올린다(T-169 · 09-22).
#   원본은 저장소의 docs/wiki/(Home.md = 목차) · 위키 저장소는 형제 폴더 ../_wiki/nexa-sql.wiki에 clone/pull · 바뀐 것만 커밋.
#   push는 사용자가 명시적으로 부를 때만(기본 = --dry-run처럼 커밋까지 · `--push`를 주면 push).
# 사용: bash scripts/wiki-publish.sh [--push]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/docs/wiki"
DEST="$ROOT/../_wiki/nexa-sql.wiki"
REMOTE="git@kiros33.github.com:SosomLab/nexa-sql.wiki.git"   # SSH 별칭(CLAUDE.md)
if [ ! -d "$DEST/.git" ]; then
  mkdir -p "$(dirname "$DEST")"
  git clone "$REMOTE" "$DEST"
else
  git -C "$DEST" pull --ff-only
fi
cp "$SRC"/*.md "$DEST"/
if [ -d "$SRC/images" ]; then mkdir -p "$DEST/images"; cp -r "$SRC/images/." "$DEST/images/" 2>/dev/null || true; fi
git -C "$DEST" add -A
if git -C "$DEST" diff --cached --quiet; then echo "wiki: 변경 없음"; exit 0; fi
git -C "$DEST" commit -q -m "wiki: sync from docs/wiki ($(git -C "$ROOT" rev-parse --short HEAD))"
echo "wiki: 커밋 $(git -C "$DEST" rev-parse --short HEAD)"
if [ "${1:-}" = "--push" ]; then git -C "$DEST" push origin HEAD; echo "wiki: push 완료"; else echo "wiki: push 생략(--push)"; fi
