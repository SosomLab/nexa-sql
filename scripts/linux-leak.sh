#!/usr/bin/env bash
# linux-leak.sh — `win-leak-cycle.ps1`의 Linux 이식(입력 주입 없음 · docs/61 §4): 같은 동작 묶음을 N번 되풀이시키고(기동 명령의 시차
#   실행 `@after:<ms>:<명령>`) 주기마다 "쉬는 상태"에서 RssAnon · RSS · 스레드 · fd 를 찍는다. 주기가 늘어도 계단처럼 오르지 않아야 한다.
#
# 사용:  scripts/linux-leak.sh -H /tmp/nsql-home -n 10 -p 4000 -C "open:/tmp/big20.sql;file.close_tab" -a Local
#        ... -F "open:/tmp/rows.sql" -C "run.all" -p 6000        # 10만 행 조회 되풀이(결과 교체)
#        ... -C "view.log;view.log" -p 2000                       # 보조 창 열기 → 닫기(토글)
# 읽는 법: 첫 1~2주기는 캐시·글리프가 채워지며 오른다(정상). 그 뒤 **마지막 절반의 기울기**(MB/주기)가 0에 가까우면 누수 없음.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXE="$ROOT/target/release/nexa-sql"; HOME_DIR=""; FIRST=""; CYC=""; N=10; PER=4000; START=4000; ARGS=""; TAG=leak
while getopts "e:H:F:C:n:p:S:a:t:" o; do case $o in e) EXE=$OPTARG;; H) HOME_DIR=$OPTARG;; F) FIRST=$OPTARG;; C) CYC=$OPTARG;; n) N=$OPTARG;; p) PER=$OPTARG;; S) START=$OPTARG;; a) ARGS=$OPTARG;; t) TAG=$OPTARG;; esac; done
[ -d "$HOME_DIR" ] || { echo "home dir missing: '$HOME_DIR'"; exit 2; }
IFS=';' read -ra acts <<< "$CYC"; cmds=()
[ -n "$FIRST" ] && { IFS=';' read -ra f <<< "$FIRST"; cmds+=("${f[@]}"); }
# 주기 i의 동작들을 주기의 앞 60% 안에 고르게 놓고, 나머지 40%는 쉬게 둔다(그 끝에서 잰다).
for ((i=0;i<N;i++)); do t0=$((START + i*PER)); k=0; for a in "${acts[@]}"; do at=$(( t0 + PER*6/10*k/${#acts[@]} )); cmds+=("@after:${at}:${a}"); k=$((k+1)); done; done
export NSQL_HOME="$HOME_DIR" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$(IFS=,; echo "${cmds[*]}")"
T0=$(date +%s%N)
# shellcheck disable=SC2086
"$EXE" $ARGS >/dev/null 2>"$HOME_DIR/$TAG.stderr" &
PID=$!; : > "$HOME_DIR/$TAG.anon"
snap() { local anon rss thr fd; anon=$(awk '/^RssAnon/{print $2}' /proc/$PID/status); rss=$(awk '/^VmRSS/{print $2}' /proc/$PID/status); thr=$(awk '/^Threads/{print $2}' /proc/$PID/status); fd=$(ls /proc/$PID/fd | wc -l)
  printf '%s\t%-8s anon=%7.2f MB rss=%6.1f MB thr=%2d fd=%3d\n' "$TAG" "$1" "$(echo "$anon/1024"|bc -l)" "$(echo "$rss/1024"|bc -l)" "$thr" "$fd"; echo "$anon" >> "$HOME_DIR/$TAG.anon"; }
waitto() { local w=$(( $1 - ($(date +%s%N) - T0)/1000000 )); [ $w -gt 0 ] && sleep "$(echo "$w/1000"|bc -l)"; }
waitto $((START-300)); snap base
for ((i=0;i<N;i++)); do waitto $((START + (i+1)*PER - 300)); kill -0 $PID 2>/dev/null || { echo "$TAG	process exited"; break; }; snap "cycle$((i+1))"; done
awk '{a[NR]=$1} END{h=int(NR/2); if(NR-h>1){printf "'"$TAG"'\tslope(last half) = %.3f MB/cycle\n", (a[NR]-a[h+1])/(NR-h-1)/1024}}' "$HOME_DIR/$TAG.anon"
kill $PID 2>/dev/null; wait $PID 2>/dev/null
