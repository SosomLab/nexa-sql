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
    # ★ 빌드 실패는 멈춘다(10-27 S2): `cargo … | grep | head`는 종료 코드가 head 것이라 실패해도 옛 산출물을 복사·재기동하던 결함 →
    #   PIPESTATUS[0]로 cargo 것을 본다(102 §12 10-25 "검증 사슬은 pipefail" 교훈과 같은 뿌리).
    cargo build --workspace 2>&1 | grep -E '^(warning|error)|Finished' | head -5
    if [ "${PIPESTATUS[0]}" != 0 ]; then echo "[debug] cargo build 실패 — 재기동하지 않음"; exit 1; fi
    if [ "$RELEASE" = 1 ]; then
        # ★ 로컬 Release = lite 판(사용자 10-26 밤 · 3-OS · D-363 · docs/61 §3): 별도 target-dir에 lite로 빌드한 뒤 target/release로 복사.
        #   기본 `cargo build --release`는 full(내장)판으로 되돌리므로 쓰지 않는다 · 종전 full 판은 `*.full`로 보존(확장 드라이버는 복사하지 않는다 ·
        #   NSQL_DRIVER_DIR 또는 설정 폴더 drivers/ · 수정 중인 확장은 scripts/lite-dev-drivers.sh).
        # ★ full 판 판별은 **빌드 전에**(10-27 S1): 종전엔 "target/release가 새 lite와 다르면 .full로"라서 두 번째 실행부터 옛 lite가
        #   .full을 덮어 진짜 full 판이 사라졌다 → 빌드 전 target/release가 직전 lite(target/lite/release)와 다를 때만 full로 보고 보존.
        #   판별 = 마지막으로 복사한 lite의 해시 부속 파일 `<bin>.lite-sha`(없으면(첫 실행) 직전 target/lite/release와 cmp · 3-OS 같은 규칙).
        sha() { { shasum -a 256 "$1" 2>/dev/null || sha256sum "$1"; } | cut -d' ' -f1; }
        FULL_LIST=" " # bash 3.2(맥 기본)에는 연관 배열이 없다 → " nexa-sql nsql " 꼴 문자열
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
