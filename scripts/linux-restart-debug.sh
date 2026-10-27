#!/usr/bin/env bash
# linux-restart-debug.sh — "빌드 및 재시작"(사용자 09-27 · mac-restart-debug.sh의 Linux판): Debug 빌드 → 앞서 이 스크립트가 띄운
#   인스턴스 종료 → Debug 재기동(사용자 실제 설정 · Oracle Instant Client 환경은 `.desktop`과 같게).
#   PID는 스크래치패드(또는 $NSQL_PID_FILE)에 적어 두고 그 PID만 끝낸다(다른 프로세스는 건드리지 않는다 · docs/61 §2-4).
# 사용:  bash scripts/linux-restart-debug.sh            # 빌드 + 재기동
#        bash scripts/linux-restart-debug.sh --no-build # 재기동만
#        bash scripts/linux-restart-debug.sh --release  # Release도 함께 빌드(사용자 병행 시험용 · Release는 라이선스 게이트가 늘 켜진다 ·
#                                                       ★ lite 판 = D-363 · 10-27 T-436: target/lite에 빌드해 target/release로 복사 · 종전 full = *.full)
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
    # 빌드 실패는 멈춘다(종료 코드 = PIPESTATUS[0] · 10-27 T-436 · mac 스크립트와 같은 꼴).
    cargo build -p nexa-sql -p nsql-cli 2>&1 | grep -E '^(warning|error)|Finished' | head -5
    if [ "${PIPESTATUS[0]}" != 0 ]; then echo "[debug] cargo build 실패 — 재기동하지 않음"; exit 1; fi
    if [ "$RELEASE" = 1 ]; then
        # ★ 로컬 Release = lite 판(D-363 · docs/61 §3 · 10-27 T-436): 별도 target-dir에 lite로 빌드 → target/release로 복사.
        #   full 판 판별은 빌드 전(target/release가 직전 lite와 다를 때만 *.full로 보존 · mac S1 교훈).
        #   판별 = 마지막으로 복사한 lite의 해시 부속 파일 `<bin>.lite-sha`(없으면 직전 target/lite/release와 cmp · mac 스크립트와 같은 꼴).
        sha() { { sha256sum "$1" 2>/dev/null || shasum -a 256 "$1"; } | cut -d' ' -f1; }
        FULL_LIST=" " # bash 3.2 호환 · " nexa-sql nsql " 문자열
        for b in nexa-sql nsql; do
            cur="$ROOT/target/release/$b"; lite="$ROOT/target/lite/release/$b"
            [ -x "$cur" ] || continue
            if [ -f "$cur.lite-sha" ]; then
                [ "$(cat "$cur.lite-sha")" != "$(sha "$cur")" ] && FULL_LIST="$FULL_LIST$b "
            elif [ ! -x "$lite" ] || ! cmp -s "$cur" "$lite"; then FULL_LIST="$FULL_LIST$b "; fi
        done
        CARGO_TARGET_DIR="$ROOT/target/lite" cargo build --release -p nexa-sql -p nsql-cli \
            --no-default-features --features nexa-sql/driver-oracle-oci,nsql-cli/driver-oracle-oci \
            2>&1 | grep -E '^(warning|error)|Finished' | head -5
        if [ "${PIPESTATUS[0]}" != 0 ]; then echo "[release] lite cargo build 실패 — target/release는 그대로"; exit 1; fi
        mkdir -p "$ROOT/target/release"
        for b in nexa-sql nsql; do
            if [ -x "$ROOT/target/lite/release/$b" ]; then
                case "$FULL_LIST" in *" $b "*) cp -p "$ROOT/target/release/$b" "$ROOT/target/release/$b.full" ;; esac
                rm -f "$ROOT/target/release/$b" # 제자리 덮기는 cargo 하드링크(deps/) 쪽까지 바꾼다 → 지우고 복사
                cp -p "$ROOT/target/lite/release/$b" "$ROOT/target/release/$b"
                sha "$ROOT/target/release/$b" > "$ROOT/target/release/$b.lite-sha"
            fi
        done
        echo "[release] lite = target/release/nexa-sql · nsql (full 판은 *.full · 확장 드라이버 = NSQL_DRIVER_DIR 또는 설정 폴더 drivers/)"
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
# 앞선 로그는 지우지 않고 회전해 둔다(재현 기록 보존 · 09-27).
[ -f "$LOG" ] && mv -f "$LOG" "$LOG.1"
nohup target/debug/nexa-sql > "$LOG" 2>&1 &
NEW=$!
echo "PID=$NEW" > "$PIDF"
echo "PID=$NEW (log: $LOG · oracle: ${IC:-none})"
