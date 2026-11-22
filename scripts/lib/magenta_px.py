# 마젠타 근처 픽셀 수(R>200 · G<80 · B>200) — 인자 = PNG 경로
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
print(sum(1 for r, g, b in im.getdata() if r > 200 and g < 80 and b > 200))
