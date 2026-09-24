# mac-common.sh — 맥 성능 도구 공용(09-24 · linux-*.sh 이식 · `/proc` 대신 `ps`/`vmmap`). `source` 해서 쓴다.
now_ms() { python3 -c 'import time;print(int(time.time()*1000))'; }
# 누적 CPU(ms) — `ps -o time` = [[hh:]mm:]ss.cc
cpu_ms() { ps -o time= -p "$1" 2>/dev/null | awk -F'[:.]' '{n=NF; cs=$(n); s=$(n-1); m=(n>=3)?$(n-2):0; h=(n>=4)?$(n-3):0; printf "%d", ((h*3600+m*60+s)*100+cs)*10}'; }
rss_mb() { ps -o rss= -p "$1" 2>/dev/null | awk '{printf "%.1f", $1/1024}'; }
thr_n() { ps -M -p "$1" 2>/dev/null | awk 'NR>1' | wc -l | tr -d ' '; }
fd_n() { lsof -p "$1" 2>/dev/null | awk 'NR>1' | wc -l | tr -d ' '; }
# Physical footprint(MB · Windows Private에 가까운 값 · vmmap ≈ 0.5 s)
foot_mb() { vmmap --summary "$1" 2>/dev/null | awk '/Physical footprint:/{v=$3; if(v~/G$/){sub(/G/,"",v); printf "%.1f", v*1024} else if(v~/M$/){sub(/M/,"",v); printf "%.1f", v} else if(v~/K$/){sub(/K/,"",v); printf "%.1f", v/1024} else printf "%s", v; exit}'; }
med() { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
