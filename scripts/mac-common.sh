# mac-common.sh — 맥 성능 도구 공용(09-24 · linux-*.sh 이식 · `/proc` 대신 `ps`/`vmmap`). `source` 해서 쓴다.
# ★ T-464(11-01 mac V3 F): python3 기동이 160~270 ms(3.15)라 호출마다 그만큼 늦게 읽혔다 → 창 감지 루프(호출 2회/바퀴)의 값이 350~550 ms 계단으로
#   양자화(352/357 ↔ 549~599 양봉 = 회귀 아님 · 측정 자산). perl Time::HiRes = 10~20 ms(워밍 뒤). bash 3.2라 $EPOCHREALTIME 없음.
now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time*1000' 2>/dev/null || python3 -c 'import time;print(int(time.time()*1000))'; }
# 누적 CPU(ms) — `ps -o time` = [[hh:]mm:]ss.cc
cpu_ms() { ps -o time= -p "$1" 2>/dev/null | awk -F'[:.]' '{n=NF; cs=$(n); s=$(n-1); m=(n>=3)?$(n-2):0; h=(n>=4)?$(n-3):0; printf "%d", ((h*3600+m*60+s)*100+cs)*10}'; }
rss_mb() { ps -o rss= -p "$1" 2>/dev/null | awk '{printf "%.1f", $1/1024}'; }
thr_n() { ps -M -p "$1" 2>/dev/null | awk 'NR>1' | wc -l | tr -d ' '; }
fd_n() { lsof -p "$1" 2>/dev/null | awk 'NR>1' | wc -l | tr -d ' '; }
# Physical footprint(MB · Windows Private에 가까운 값 · vmmap ≈ 0.5 s)
foot_mb() { vmmap --summary "$1" 2>/dev/null | awk '/Physical footprint:/{v=$3; if(v~/G$/){sub(/G/,"",v); printf "%.1f", v*1024} else if(v~/M$/){sub(/M/,"",v); printf "%.1f", v} else if(v~/K$/){sub(/K/,"",v); printf "%.1f", v/1024} else printf "%s", v; exit}'; }
med() { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END{print a[int((NR+1)/2)]}'; }
