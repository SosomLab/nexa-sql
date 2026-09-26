#!/usr/bin/env python3
"""cli-wall.py — CLI 한 명령의 프로세스 wall을 **하네스 오버헤드 없이** 잰다(T-228 · 09-26).
   종전 mac-perf-all.sh `cli()`는 실행마다 python3를 두 번 띄워 시각을 찍어(각 ≈ 90 ms) 12 ms짜리 실행이 150~210 ms로 보였다.
   사용: cli-wall.py [-n 12] [-w 2] -- <명령 …>   → 한 줄 `wall min=… med=… max=… ms (n=…)` (앞 w회는 예열로 버림 · 표준 출력/오류는 버림)
   환경 변수(NSQL_HOME 등)는 그대로 물려준다 · 3-OS 동일."""
import subprocess, sys, time, statistics
n, w, args = 12, 2, sys.argv[1:]
while args and args[0] != "--":
    if args[0] == "-n": n = int(args[1]); args = args[2:]
    elif args[0] == "-w": w = int(args[1]); args = args[2:]
    else: break
if args and args[0] == "--": args = args[1:]
if not args:
    print("usage: cli-wall.py [-n N] [-w WARM] -- <cmd...>", file=sys.stderr); sys.exit(2)
ts = []
for _ in range(n):
    t = time.perf_counter()
    subprocess.run(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    ts.append((time.perf_counter() - t) * 1000)
ts = ts[w:] if len(ts) > w else ts
print(f"wall min={min(ts):.1f} med={statistics.median(ts):.1f} max={max(ts):.1f} ms (n={len(ts)})")
