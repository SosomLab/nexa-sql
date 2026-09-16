#!/usr/bin/env python3
"""make-ico.py — branding PNG 세트(png/nexa-sql-{16,24,32,48,256}.png) → nexa-sql.ico (PNG 프레임 · Vista+).

표준 라이브러리만 쓴다(PIL 불요) — scripts/pack_icon.py의 ICO 부분을 PNG 프레임 입력으로 재구성한 것.
1024 원본에서 전 세트를 다시 만들 때는 pack_icon.py(PIL)를, PNG 세트가 이미 있을 때(평소)는 이걸 쓴다.

사용:  python3 packaging/windows/make-ico.py [branding 디렉터리] [출력 .ico]
       (기본: packaging/branding → packaging/branding/nexa-sql.ico)
검증:  기존 ICO와 바이트 동일(cmp) — 같은 PNG 바이트를 같은 순서로 담기 때문.
"""
import os
import struct
import sys

SIZES = [16, 24, 32, 48, 256]


def main() -> int:
    here = os.path.dirname(os.path.abspath(__file__))
    branding = sys.argv[1] if len(sys.argv) > 1 else os.path.join(here, "..", "branding")
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(branding, "nexa-sql.ico")
    frames = []
    for n in SIZES:
        p = os.path.join(branding, "png", f"nexa-sql-{n}.png")
        with open(p, "rb") as f:
            d = f.read()
        if d[:8] != b"\x89PNG\r\n\x1a\n":
            print(f"PNG 아님: {p}", file=sys.stderr)
            return 1
        frames.append((n, d))
    # ICONDIR(6) + ICONDIRENTRY(16)*n + PNG 데이터. 폭/높이 256은 0으로 적는다(바이트 한 칸).
    hdr = struct.pack("<HHH", 0, 1, len(frames))
    entries, data = b"", b""
    offset = 6 + 16 * len(frames)
    for n, d in frames:
        entries += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(d), offset + len(data))
        data += d
    with open(out, "wb") as f:
        f.write(hdr + entries + data)
    print(f"{out}: {len(frames)}프레임 {6 + 16 * len(frames) + len(data)}바이트")
    return 0


if __name__ == "__main__":
    sys.exit(main())
