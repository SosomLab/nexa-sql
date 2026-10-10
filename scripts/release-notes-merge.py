#!/usr/bin/env python3
"""release-notes-merge.py — 릴리스 노트 합성(T-319 · 버전 인자화 · 10-10).

release.yml이 만든 초안 본문(머리 표·설치·저장소·서명 절 · 영어 → `---` → 한국어)에 "변경 절" 파일을 끼운다:
  변경 파일 = `## What's in this release (since X)` … `---` … `## 이번 판에 담긴 것(X 이후)` …
  합성 = 영어 변경 절(+ Full details · Distribution note 줄)을 초안의 **첫 `---` 앞**에 · 한국어 변경 절을 **맨 끝**에.
사용: scripts/release-notes-merge.py <버전> <초안 본문 파일> <변경 파일> > notes.md
      (초안 = `gh release view vX --json body -q .body > draft.md`)
"""
import sys


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    ver, draft_path, changes_path = sys.argv[1:]
    draft = open(draft_path, encoding="utf-8").read().rstrip("\n").split("\n")
    changes = open(changes_path, encoding="utf-8").read().strip("\n")
    if "\n---\n" not in changes:
        print("변경 파일에 `---`(영어/한국어 구분)가 없다", file=sys.stderr)
        return 2
    en, ko = changes.split("\n---\n", 1)
    en = en.strip("\n") + (
        f"\n\nFull details: [docs/DEVLOG.md](https://github.com/SosomLab/nexa-sql/blob/v{ver}/docs/DEVLOG.md)"
        f" · [docs/journal](https://github.com/SosomLab/nexa-sql/tree/v{ver}/docs/journal)\n\n"
        "Distribution note: this version is published to GitHub Releases, Homebrew (`kiros33/tap`) and the Linux"
        " repository (pkg.sosomlab.com). winget and Chocolatey are paused until the pending reviews finish."
    )
    try:
        cut = draft.index("---")
    except ValueError:
        print("초안에 `---`가 없다", file=sys.stderr)
        return 2
    if any(l.startswith("## What's in this release") for l in draft):
        print("초안에 이미 변경 절이 있다(두 번 합성 금지)", file=sys.stderr)
        return 2
    out = draft[:cut] + en.split("\n") + [""] + draft[cut:] + ["", ko.strip("\n")]
    sys.stdout.write("\n".join(out) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
