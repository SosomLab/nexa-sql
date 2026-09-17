#!/usr/bin/env python3
"""mac-text-compare.py — CoreText 참조(ref.pgm · scripts/mac-text-ref.swift)와 우리 렌더(ours.pgm · nexa-font 테스트)를
픽셀 비교(정렬 오프셋 탐색 · MAE · 잉크 비율)하고 둘을 ASCII로 덤프한다(이미지 없이 눈 대신 · T-100 09-17)."""
import sys, itertools
def load(p):
    b=open(p,'rb').read()
    assert b[:2]==b'P5'
    parts=b.split(b'\n',3); w,h=map(int,parts[1].split()); data=parts[3]
    return w,h,[data[y*w:(y+1)*w] for y in range(h)]
def ascii(rows,x0=0,x1=None,step=1):
    ramp=" .:-=+*#%@"
    out=[]
    for r in rows:
        seg=r[x0:x1]
        out.append("".join(ramp[min(9,v*10//256)] for v in seg[::step]))
    return "\n".join(out)
def ink(rows): return sum(sum(r) for r in rows)
def mae(a,b,dx,dy):
    aw,ah=len(a[0]),len(a); bw,bh=len(b[0]),len(b)
    tot=0;n=0
    for y in range(ah):
        by=y+dy
        if by<0 or by>=bh: continue
        ra=a[y]; rb=b[by]
        for x in range(aw):
            bx=x+dx
            if bx<0 or bx>=bw: continue
            tot+=abs(ra[x]-rb[bx]); n+=1
    return tot/max(n,1)
ref=sys.argv[1] if len(sys.argv)>1 else "target/textref/ref.pgm"
ours=sys.argv[2] if len(sys.argv)>2 else "target/textref/ours.pgm"
rw,rh,R=load(ref); ow,oh,O=load(ours)
best=min(((mae(R,O,dx,dy),dx,dy) for dx in range(-8,9) for dy in range(-8,9)))
print(f"ref {rw}x{rh} ink={ink(R)}  ours {ow}x{oh} ink={ink(O)}  ink ratio ours/ref={ink(O)/max(ink(R),1):.3f}")
print(f"best align dx={best[1]} dy={best[2]} MAE={best[0]:.2f}/255")
lim=int(sys.argv[3]) if len(sys.argv)>3 else 120
print("--- ref"); print(ascii(R,0,lim))
print("--- ours"); print(ascii(O,0,lim))
