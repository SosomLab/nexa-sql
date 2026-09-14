"""icon-1024.png(헤드리스 Edge 렌더) → PNG 세트 · .ico(PNG 프레임) · .icns · 런타임 RGBA(창 아이콘용)."""
import struct, io, os, sys
from PIL import Image

# 사용: python scripts/pack_icon.py <icon-1024.png> <packaging/branding>
SRC = sys.argv[1] if len(sys.argv) > 1 else 'icon-1024.png'
OUT = sys.argv[2] if len(sys.argv) > 2 else 'packaging/branding'
os.makedirs(os.path.join(OUT, 'png'), exist_ok=True)

im = Image.open(SRC).convert('RGBA')
assert im.size == (1024, 1024), im.size
print('corner alpha', im.getpixel((0, 0))[3], 'center', im.getpixel((512, 512)))

def scaled(n):
    return im.resize((n, n), Image.LANCZOS)

def png_bytes(img):
    b = io.BytesIO(); img.save(b, 'PNG', optimize=True); return b.getvalue()

sizes = [16, 24, 32, 48, 64, 128, 256, 512, 1024]
pngs = {n: png_bytes(scaled(n)) for n in sizes}
for n in sizes:
    open(os.path.join(OUT, 'png', f'nexa-sql-{n}.png'), 'wb').write(pngs[n])
open(os.path.join(OUT, 'nexa-sql-1024.png'), 'wb').write(pngs[1024])
open(os.path.join(OUT, 'nexa-sql-256.png'), 'wb').write(pngs[256])

# .ico — PNG 프레임(Vista+) 16·24·32·48·256
ico_sizes = [16, 24, 32, 48, 256]
hdr = struct.pack('<HHH', 0, 1, len(ico_sizes))
entries = b''; data = b''
offset = 6 + 16 * len(ico_sizes)
for n in ico_sizes:
    d = pngs[n]
    entries += struct.pack('<BBBBHHII', n % 256, n % 256, 0, 0, 1, 32, len(d), offset + len(data))
    data += d
open(os.path.join(OUT, 'nexa-sql.ico'), 'wb').write(hdr + entries + data)

# .icns — PNG 페이로드 타입
icns_types = [(b'icp4', 16), (b'icp5', 32), (b'icp6', 64), (b'ic07', 128), (b'ic08', 256), (b'ic09', 512), (b'ic10', 1024),
              (b'ic11', 32), (b'ic12', 64), (b'ic13', 256), (b'ic14', 512)]
body = b''
for typ, n in icns_types:
    d = pngs[n]
    body += typ + struct.pack('>I', 8 + len(d)) + d
open(os.path.join(OUT, 'nexa-sql.icns'), 'wb').write(b'icns' + struct.pack('>I', 8 + len(body)) + body)

# 런타임 창 아이콘은 코드로 그린다(crates/nexa-sql/src/icon.rs · 정적 자원 0) — 여기서는 만들지 않는다.
print('done', {k: len(v) for k, v in pngs.items()})
