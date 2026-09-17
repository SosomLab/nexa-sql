#!/usr/bin/env python3
"""mac-capture-ascii.py — 화면 영역을 캡처해 ASCII로 덤프(이미지 없이 글꼴·배치 확인 · T-100 09-17).
사용: python3 scripts/mac-capture-ascii.py X Y W H [scale=1]   (좌표 = 전역 포인트 · 2번 모니터는 x≥2048)
      python3 scripts/mac-capture-ascii.py --window [dx dy w h]  → nexa-sql 창 위치 기준 오프셋 영역"""
import subprocess, struct, sys, os, tempfile
def bounds():
    r = subprocess.run(["osascript", "-e", 'tell application "System Events" to tell (first process whose name is "nexa-sql") to get {position, size} of window 1'], capture_output=True, text=True).stdout
    return [int(v) for v in r.replace(" ", "").strip().split(",")]
a = sys.argv[1:]
if a and a[0] == "--window":
    x, y, w, h = bounds()
    dx, dy, cw, ch = (int(v) for v in (a[1:5] if len(a) >= 5 else ["0", "40", "260", "70"]))
    x, y, w, h = x + dx, y + dy, cw, ch
else:
    x, y, w, h = (int(v) for v in a[:4])
d = tempfile.mkdtemp()
png, bmp = os.path.join(d, "c.png"), os.path.join(d, "c.bmp")
subprocess.run(["screencapture", "-x", "-R", f"{x},{y},{w},{h}", png], check=True)
subprocess.run(["sips", "-s", "format", "bmp", png, "--out", bmp], capture_output=True, check=True)
b = open(bmp, "rb").read()
off = struct.unpack_from("<I", b, 10)[0]; bw = struct.unpack_from("<i", b, 18)[0]; bh = struct.unpack_from("<i", b, 22)[0]; bpp = struct.unpack_from("<H", b, 28)[0]
row = ((bw * bpp // 8) + 3) // 4 * 4
ramp = " .:-=+*#%@"
def px(cx, cy):
    i = off + (abs(bh) - 1 - cy if bh > 0 else cy) * row + cx * (bpp // 8)
    bb, g, r = b[i], b[i + 1], b[i + 2]
    return 255 - (r * 299 + g * 587 + bb * 114) // 1000
print(f"capture {bw}x{abs(bh)} at ({x},{y}) — 1 char = 1 px")
for cy in range(abs(bh)):
    line = "".join(ramp[min(9, px(cx, cy) * 10 // 256)] for cx in range(bw))
    if line.strip():
        print(line.rstrip())
