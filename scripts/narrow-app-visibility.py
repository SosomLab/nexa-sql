#!/usr/bin/env python3
"""`app/*.rs`의 `pub(crate) fn` 중 자기 파일 밖에서 부르지 않는 것을 비공개로 좁힌다(docs/93 §4 — 캡슐화).

판정: 이름이 다른 .rs 파일(크레이트 전체)에서 `.이름(` · `::이름(` · `이름)` 참조로 한 번도 안 나오면 비공개.
같은 이름의 다른 타입 메서드가 있으면 '밖에서 쓰임'으로 보아 그대로 둔다(안전한 쪽).
멱등 — 다시 돌려도 된다. 이어서 cargo check로 확인.
"""
import io
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "crates/nexa-sql/src"
files = {p: io.open(p, encoding="utf-8").read() for p in SRC.rglob("*.rs")}
app_files = [p for p in files if p.parent.name == "app"]
decl = re.compile(r"^    pub\(crate\) fn ([a-z_0-9]+)", re.M)
changed = 0
for p in app_files:
    text = files[p]
    names = decl.findall(text)
    for n in names:
        use = re.compile(r"(?:\.|::|\b)" + n + r"\b(?!\s*[:=])")
        outside = any(use.search(t) for q, t in files.items() if q != p)
        if not outside:
            text = re.sub(r"^    pub\(crate\) fn " + n + r"\b", "    fn " + n, text, flags=re.M)
            changed += 1
    if text != files[p]:
        io.open(p, "w", encoding="utf-8", newline="\n").write(text)
        files[p] = text
print("narrowed", changed)
