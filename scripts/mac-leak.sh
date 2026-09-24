#!/usr/bin/env bash
# mac-leak.sh — `linux-leak.sh`의 맥 이식: 같은 동작 묶음을 N번 되풀이(기동 명령 `@after`) · 주기마다 쉬는 상태에서 footprint·RSS·스레드.
# 사용: scripts/mac-leak.sh -H <home> -n 8 -p 5000 -C "open:<f>;file.close_tab" [-F "<첫 명령>"] [-a Local] [-t tag]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; . "$ROOT/scripts/mac-common.sh"
EXE="$ROOT/target/release/nexa-sql"; HOME_DIR=""; FIRST=""; CYC=""; N=10; PER=4000; START=4000; ARGS=""; TAG=leak
while getopts "e:H:F:C:n:p:S:a:t:" o; do case $o in e) EXE=$OPTARG;; H) HOME_DIR=$OPTARG;; F) FIRST=$OPTARG;; C) CYC=$OPTARG;; n) N=$OPTARG;; p) PER=$OPTARG;; S) START=$OPTARG;; a) ARGS=$OPTARG;; t) TAG=$OPTARG;; esac; done
[ -d "$HOME_DIR" ] || { echo "home dir missing: '$HOME_DIR'"; exit 2; }
IFS=';' read -ra acts <<< "$CYC"; cmds=()
[ -n "$FIRST" ] && { IFS=';' read -ra f <<< "$FIRST"; cmds+=("${f[@]}"); }
for ((i=0;i<N;i++)); do t0=$((START + i*PER)); k=0; for a in "${acts[@]}"; do at=$(( t0 + PER*6/10*k/${#acts[@]} )); cmds+=("@after:${at}:${a}"); k=$((k+1)); done; done
export NSQL_HOME="$HOME_DIR" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$(IFS=,; echo "${cmds[*]}")"
T0=$(now_ms)
# shellcheck disable=SC2086
"$EXE" $ARGS >/dev/null 2>"$HOME_DIR/$TAG.stderr" &
PID=$!; : > "$HOME_DIR/$TAG.foot"
snap() { local foot rss thr; foot=$(foot_mb $PID); rss=$(rss_mb $PID); thr=$(thr_n $PID)
  printf '%s\t%-8s footprint=%7.1f MB rss=%6.1f MB thr=%2d\n' "$TAG" "$1" "$foot" "$rss" "$thr"; echo "$foot" >> "$HOME_DIR/$TAG.foot"; }
waitto() { local w=$(( $1 - ($(now_ms) - T0) )); [ $w -gt 0 ] && sleep "$(echo "$w/1000"|bc -l)"; }
waitto $((START-600)); snap base
for ((i=0;i<N;i++)); do waitto $((START + (i+1)*PER - 600)); kill -0 $PID 2>/dev/null || { echo "$TAG	process exited"; break; }; snap "cycle$((i+1))"; done
awk '{a[NR]=$1} END{h=int(NR/2); if(NR-h>1){printf "'"$TAG"'\tslope(last half) = %.3f MB/cycle\n", (a[NR]-a[h+1])/(NR-h-1)}}' "$HOME_DIR/$TAG.foot"
kill $PID 2>/dev/null; wait $PID 2>/dev/null
