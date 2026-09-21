#!/usr/bin/env bash
# linux-probe.sh — `win-big-probe.ps1`의 Linux 이식(입력 주입 없음 · docs/61 §4): 격리 설정 폴더 + 기동 명령으로 앱을 띄워
#   **초 단위로** CPU(ms) · RSS · RssAnon(≈ Private) · VmHWM(피크 RSS) · 스레드 · fd 를 찍는다(`/proc/<pid>`만 읽는다).
#   큰 파일 열기 · 메모리 회수 · 유휴 CPU를 앞뒤로 비교할 때 쓴다. Release로 잰다(Debug의 "멈춤"은 최적화 없는 빌드의 느림일 수 있다).
#
# 사용:  scripts/linux-probe.sh -H /tmp/nsql-home -c "open:/tmp/big60.sql,@after:2500:bigfile.open" -s 12 -t open60 -a Local
#        -e <exe>(기본 target/release/nexa-sql) · -i <간격 초>(기본 1) · -c <기동 명령> · -a <실행 인자(프로필)> · -t <태그>
# 읽는 법: cpu+= 는 그 간격 동안 쓴 CPU(ms) — 열고 난 뒤의 값이 유휴 CPU다 · peak이 상주보다 많이 크면 적재 경로에 사본이 있다.
#   stderr는 <home>/<tag>.stderr 에 남는다(`NSQL_TRACE_FRAMES=1`을 함께 주면 `[frames]`·`[load]` 줄이 여기에).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXE="$ROOT/target/release/nexa-sql"; HOME_DIR=""; CMD=""; SECS=20; TAG=probe; ARGS=""; IV=1
while getopts "e:H:c:s:t:a:i:" o; do case $o in e) EXE=$OPTARG;; H) HOME_DIR=$OPTARG;; c) CMD=$OPTARG;; s) SECS=$OPTARG;; t) TAG=$OPTARG;; a) ARGS=$OPTARG;; i) IV=$OPTARG;; esac; done
[ -d "$HOME_DIR" ] || { echo "home dir missing (create the sandbox folder first): '$HOME_DIR'"; exit 2; }
CLK=$(getconf CLK_TCK)
export NSQL_HOME="$HOME_DIR" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$CMD"   # 시험 창이 전경 포커스를 가져가지 않게(docs/61 §4)
# shellcheck disable=SC2086
"$EXE" $ARGS >"$HOME_DIR/$TAG.stdout" 2>"$HOME_DIR/$TAG.stderr" &
PID=$!; prev=0
snap() {
  local tt=$1 stat cpu rss anon hwm thr fd
  [ -r /proc/$PID/stat ] || return 1
  stat=$(cat /proc/$PID/stat); stat=${stat##*) }; set -- $stat
  cpu=$(( (${12} + ${13}) * 1000 / CLK ))
  rss=$(awk '/^VmRSS/{print $2}' /proc/$PID/status); anon=$(awk '/^RssAnon/{print $2}' /proc/$PID/status)
  hwm=$(awk '/^VmHWM/{print $2}' /proc/$PID/status); thr=$(awk '/^Threads/{print $2}' /proc/$PID/status)
  fd=$(ls /proc/$PID/fd 2>/dev/null | wc -l)
  printf '%s\tt=%5.1fs cpu+=%5d ms rss=%6.1f MB anon=%6.1f MB peak=%6.1f MB thr=%2d fd=%3d\n' \
    "$TAG" "$tt" $((cpu - prev)) "$(echo "$rss/1024" | bc -l)" "$(echo "$anon/1024" | bc -l)" "$(echo "$hwm/1024" | bc -l)" "$thr" "$fd"
  prev=$cpu
}
t=0
while [ "$(echo "$t < $SECS" | bc)" = 1 ]; do
  sleep "$IV"; t=$(echo "$t + $IV" | bc)
  kill -0 $PID 2>/dev/null || { echo "$TAG	process exited at t=${t}s"; break; }
  snap "$t"
done
kill $PID 2>/dev/null; wait $PID 2>/dev/null   # 내가 띄운 PID만 끝낸다
echo "$TAG	stopped pid $PID"
