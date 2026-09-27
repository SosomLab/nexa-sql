#!/usr/bin/env bash
# linux-restart-debug.sh — "빌드 및 재시작"(사용자 09-27 · mac-restart-debug.sh의 Linux판): Debug 빌드 → 앞서 이 스크립트가 띄운
#   인스턴스 종료 → Debug 재기동(사용자 실제 설정 · Oracle Instant Client 환경은 `.desktop`과 같게).
#   PID는 스크래치패드(또는 $NSQL_PID_FILE)에 적어 두고 그 PID만 끝낸다(다른 프로세스는 건드리지 않는다 · docs/61 §2-4).
# 사용:  bash scripts/linux-restart-debug.sh            # 빌드 + 재기동
#        bash scripts/linux-restart-debug.sh --no-build # 재기동만
#        bash scripts/linux-restart-debug.sh --release  # Release도 함께 빌드(사용자 병행 시험용 · Release는 라이선스 게이트가 늘 켜진다)
#        NSQL_PID_FILE=<파일> · NSQL_LOG=<파일> · ORACLE_IC_HOME=<폴더>(기본 ~/oracle/instantclient_* 중 최신)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
PIDF="${NSQL_PID_FILE:-${TMPDIR:-/tmp}/nexa-sql-debug.pid}"
LOG="${NSQL_LOG:-${TMPDIR:-/tmp}/nexa-sql-debug.log}"
BUILD=1; RELEASE=0
for a in "$@"; do
    case "$a" in
        --no-build) BUILD=0 ;;
        --release) RELEASE=1 ;;
    esac
done
if [ "$BUILD" = 1 ]; then
    cargo build -p nexa-sql -p nsql-cli 2>&1 | grep -E '^(warning|error)|Finished' | head -5 || true
    if [ "$RELEASE" = 1 ]; then
        cargo build --release -p nexa-sql -p nsql-cli 2>&1 | grep -E '^(warning|error)|Finished' | head -5 || true
    fi
fi
# 앞서 띄운 것만 끝낸다(PID 파일 · 그 PID가 이 저장소의 nexa-sql일 때만).
if [ -f "$PIDF" ]; then
    OLD="$(sed 's/^PID=//' "$PIDF" 2>/dev/null)"
    if [ -n "$OLD" ] && ps -p "$OLD" -o command= 2>/dev/null | grep -q "target/debug/nexa-sql"; then
        kill "$OLD" 2>/dev/null || true
        sleep 1
    fi
fi
# Oracle Instant Client(scripts/install-instantclient-linux.sh · .desktop과 같은 변수).
IC="${ORACLE_IC_HOME:-}"
if [ -z "$IC" ]; then IC="$(ls -d "$HOME"/oracle/instantclient_* 2>/dev/null | sort -V | tail -1)"; fi
if [ -n "$IC" ] && [ -d "$IC" ]; then
    export LD_LIBRARY_PATH="$IC${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    export TNS_ADMIN="$IC/network/admin"
    export NLS_LANG="${NLS_LANG:-AMERICAN_AMERICA.AL32UTF8}"
fi
nohup target/debug/nexa-sql > "$LOG" 2>&1 &
NEW=$!
echo "PID=$NEW" > "$PIDF"
echo "PID=$NEW (log: $LOG · oracle: ${IC:-none})"
