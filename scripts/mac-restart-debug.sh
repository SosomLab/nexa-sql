#!/usr/bin/env bash
# mac-restart-debug.sh — "빌드 및 재시작"(사용자 09-24): Debug 빌드 → 앞서 이 스크립트가 띄운 인스턴스 종료 → Debug 재기동(사용자 실제 설정).
#   PID는 스크래치패드(또는 $NSQL_PID_FILE)에 적어 두고 그 PID만 끝낸다(다른 프로세스는 건드리지 않는다 · docs/61 §2-4).
#   하네스 허용 목록에 `bash scripts/`가 있어 이 스크립트 한 줄로 확인 창 없이 돈다.
# 사용:  bash scripts/mac-restart-debug.sh            # 빌드 + 재기동
#        bash scripts/mac-restart-debug.sh --no-build # 재기동만
#        bash scripts/mac-restart-debug.sh --release  # Release도 함께 빌드(사용자 병행 시험용)
#        NSQL_PID_FILE=<파일> · NSQL_LOG=<파일> 로 자리 지정(기본 = $TMPDIR/nexa-sql-debug.{pid,log}).
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
    cargo build --workspace 2>&1 | grep -E '^(warning|error)|Finished' | head -5 || true
    if [ "$RELEASE" = 1 ]; then
        # ★ 로컬 Release = lite 판(사용자 10-26 밤 · 3-OS · D-363 · docs/61 §3): 별도 target-dir에 lite로 빌드한 뒤 target/release로 복사.
        #   기본 `cargo build --release`는 full(내장)판으로 되돌리므로 쓰지 않는다 · 종전 판은 `*.full`로 보존 · 확장 dylib도 옆에 둔다.
        CARGO_TARGET_DIR="$ROOT/target/lite" cargo build --release -p nexa-sql -p nsql-cli \
            --no-default-features --features nexa-sql/driver-oracle-oci,nsql-cli/driver-oracle-oci \
            2>&1 | grep -E '^(warning|error)|Finished' | head -5 || true
        mkdir -p "$ROOT/target/release"
        for b in nexa-sql nsql; do
            if [ -x "$ROOT/target/lite/release/$b" ]; then
                [ -x "$ROOT/target/release/$b" ] && ! cmp -s "$ROOT/target/release/$b" "$ROOT/target/lite/release/$b" \
                    && cp -p "$ROOT/target/release/$b" "$ROOT/target/release/$b.full" 2>/dev/null
                cp -p "$ROOT/target/lite/release/$b" "$ROOT/target/release/$b"
            fi
        done
        echo "[release] lite = target/release/nexa-sql · nsql (full 판은 *.full · 확장 드라이버 = NSQL_DRIVER_DIR 또는 설정 폴더 drivers/)"
    fi
fi
# 앞서 띄운 것만 끝낸다(PID 파일 · 그 PID가 이 저장소의 nexa-sql일 때만).
if [ -f "$PIDF" ]; then
    OLD="$(cat "$PIDF" 2>/dev/null | sed 's/^PID=//')"
    if [ -n "$OLD" ] && ps -p "$OLD" -o command= 2>/dev/null | grep -q "target/debug/nexa-sql"; then
        kill "$OLD" 2>/dev/null || true
        sleep 1
    fi
fi
nohup target/debug/nexa-sql > "$LOG" 2>&1 &
NEW=$!
echo "PID=$NEW" > "$PIDF"
echo "PID=$NEW (log: $LOG)"
