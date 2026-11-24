#!/usr/bin/env python3
"""release-notes-merge.py — 릴리스 노트 합성(T-319 · 버전 인자화 · 10-10 · 배치 = 공개해 온 꼴 11-24 D-536).

release.yml이 만든 초안 본문(제목 줄 `# Nexa SQL X` · 영어 생성 본문 → `---` → 한국어 생성 본문)에 "변경 절" 파일을 끼운다:
  변경 파일 = 영어 변경 글 … `---` … 한국어 변경 글
  합성(★ D-536 · v0.6.0~v0.9.0 공개 본문과 같은 배치) =
      제목 → 영어 변경 → 한국어 변경 → 영어 생성 본문 → `---` → 한국어 생성 본문
  · 변경 글 맨 앞의 머리 줄(`## What's in this release …` · `## 이번 판에 담긴 것…`)은 뗀다(공개 본문에는 머리가 없다).
  · `--with-links` = 영어 변경 글 끝에 Full details · Distribution note 줄을 덧붙인다(종전 동작 · 기본은 붙이지 않는다 —
    공개 본문에 없고 생성 본문이 DEVLOG 링크와 설치 경로를 이미 담는다).
  종전 배치(영어 변경을 첫 `---` 앞 · 한국어 변경을 맨 끝)는 v0.7.0·v0.7.1에서 매번 손으로 다시 배치했다.
사용: scripts/release-notes-merge.py [--with-links] <버전> <초안 본문 파일> <변경 파일> > notes.md
      (초안 = `gh release view vX --json body -q .body > draft.md`)
"""
import sys

HEADS = ("## What's in this release", "## 이번 판에 담긴 것")


def strip_head(text: str) -> str:
    """변경 글 맨 앞의 머리 줄 하나를 뗀다(있을 때만)."""
    lines = text.strip("\n").split("\n")
    if lines and lines[0].startswith(HEADS):
        lines = lines[1:]
    return "\n".join(lines).strip("\n")


def merge(ver: str, draft_text: str, changes_text: str, with_links: bool = False) -> str:
    draft = draft_text.rstrip("\n").split("\n")
    changes = changes_text.strip("\n")
    if "\n---\n" not in changes:
        raise ValueError("변경 파일에 `---`(영어/한국어 구분)가 없다")
    en, ko = changes.split("\n---\n", 1)
    en, ko = strip_head(en), strip_head(ko)
    if with_links:
        en += (
            f"\n\nFull details: [docs/DEVLOG.md](https://github.com/SosomLab/nexa-sql/blob/v{ver}/docs/DEVLOG.md)"
            f" · [docs/journal](https://github.com/SosomLab/nexa-sql/tree/v{ver}/docs/journal)\n\n"
            "Distribution note: this version is published to GitHub Releases, winget (`winget install SosomLab.NexaSQL`),"
            " Chocolatey (`choco install nexa-sql`), Homebrew (`kiros33/tap`) and the Linux repository (pkg.sosomlab.com)."
            " winget and Chocolatey pick it up after each repository's review, so they may lag behind for a while."
            # ★ 채널 상태가 바뀌면 이 줄도(v0.2.7 = choco 재개 · 고정 문구가 실제와 어긋나 공개 뒤 정정 · 10-21 ·
            #   T-478 11-15 = winget 재개 반영 · 검수 지연은 채널별 guard가 건너뛸 수 있어 "may lag"로 일반화).
        )
    if "---" not in draft:
        raise ValueError("초안에 `---`가 없다")
    if not draft or not draft[0].startswith("# "):
        raise ValueError("초안 첫 줄이 제목(`# …`)이 아니다")
    # 두 번 합성 금지: 제목과 생성 본문 사이에 이미 글이 있으면(= 변경 절이 끼어 있다) 멈춘다 — 생성 본문의 첫 줄은
    #   release.yml이 쓰는 고정 문장이다.
    body = [l for l in draft[1:] if l.strip()]
    if body and not body[0].startswith("Lightweight cross-platform SQL client"):
        raise ValueError("초안에 이미 변경 절이 있다(두 번 합성 금지)")
    rest = draft[1:]
    while rest and not rest[0].strip():
        rest = rest[1:]
    out = [draft[0], ""] + en.split("\n") + [""] + ko.split("\n") + [""] + rest
    return "\n".join(out) + "\n"


def main() -> int:
    # 오류 글도 UTF-8로(한글 Windows 콘솔에서 깨져 보였다 · 11-24).
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8")
    args = sys.argv[1:]
    with_links = "--with-links" in args
    args = [a for a in args if a != "--with-links"]
    if len(args) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    ver, draft_path, changes_path = args
    try:
        out = merge(
            ver,
            open(draft_path, encoding="utf-8").read(),
            open(changes_path, encoding="utf-8").read(),
            with_links,
        )
    except ValueError as e:
        print(str(e), file=sys.stderr)
        return 2
    # Windows 콘솔(cp949)에서 '—' 같은 글자가 UnicodeEncodeError → 늘 UTF-8로 쓴다(125차 v0.2.0).
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stdout.write(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
