#!/usr/bin/env bash
# lite-dev-drivers.sh — lite Release(확장 드라이버 없는 판)를 **이 작업 트리에서 막 빌드한 확장 드라이버**로 돌리기 위한 드라이버 폴더 생성(10-26 · 140차 mac · D-363).
#   target/lite/release/libnsql_driver_*.dylib(.so/.dll)마다 `drivers/<id>/0.0.0-dev/{nexa-driver.json, <lib>}`를 만든다 →
#   `NSQL_DRIVER_DIR=<폴더> target/release/nexa-sql`로 띄우면 설치본과 같은 확장 경로를 **수정 중인 코드**로 시험할 수 있다
#   (설정 폴더 `drivers/`의 배포판 0.1.x는 건드리지 않는다 · 매니페스트 `protocol` = 지금 ABI 버전).
# 사용: bash scripts/lite-dev-drivers.sh [-s target/lite/release] [-o target/lite/dev-drivers] [-i sqlite,oracle-thin,…]
#   끝에 띄우는 명령 한 줄을 출력한다. 3-OS 공용(라이브러리 확장자만 OS별).
# ★ `-i`를 권장(10-27 S3): 지정하지 않으면 target/lite/release에 남아 있는 **예전 빌드의 cdylib(옛 코덱)** 까지 전부 실리므로,
#   방금 빌드한 드라이버만 고르고 출력의 수정 시각(mtime)으로 같은 빌드인지 확인한다 · 시각이 서로 다르면 경고를 찍는다.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/target/lite/release"; OUT="$ROOT/target/lite/dev-drivers"; IDS=""
while getopts "s:o:i:" o; do case $o in s) SRC=$OPTARG;; o) OUT=$OPTARG;; i) IDS=$OPTARG;; esac; done
case "$(uname -s)" in Darwin) EXT=dylib; PFX=lib;; MINGW*|MSYS*|CYGWIN*|Windows_NT) EXT=dll; PFX="";; *) EXT=so; PFX=lib;; esac
PROTO=$(grep -oE 'pub const ABI_VERSION: u32 = [0-9]+' "$ROOT/crates/nsql-abi/src/lib.rs" | grep -oE '[0-9]+$'); PROTO=${PROTO:-2}
n=0; newest=0; oldest=0
mtime() { stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null || echo 0; }
for lib in "$SRC"/${PFX}nsql_driver_*.$EXT; do
    [ -f "$lib" ] || continue
    stem=$(basename "$lib" ".$EXT"); stem=${stem#"$PFX"nsql_driver_}
    case "$stem" in oracle) id=oracle-oci;; oracle_thin) id=oracle-thin;; *) id=${stem//_/-};; esac
    if [ -n "$IDS" ] && ! printf ',%s,' "$IDS" | grep -q ",$id,"; then continue; fi
    case "$id" in oracle-oci|oracle-thin) dialect=oracle;; pg) dialect=postgres;; *) dialect=$id;; esac
    d="$OUT/$id/0.0.0-dev"; mkdir -p "$d"; cp -p "$lib" "$d/"
    # 동봉 런타임(DuckDB)은 cdylib 옆에 있어야 한다(64 §5).
    [ "$id" = duckdb ] && for r in "$SRC"/${PFX}duckdb.$EXT "$SRC"/duckdb.$EXT; do [ -f "$r" ] && cp -p "$r" "$d/"; done
    printf '{"id":"%s","version":"0.0.0-dev","dialect":"%s","protocol":%s,"lib":"%s","name":"%s (dev)"}\n' \
        "$id" "$dialect" "$PROTO" "$(basename "$lib")" "$id" > "$d/nexa-driver.json"
    m=$(mtime "$lib"); [ "$m" -gt "$newest" ] && newest=$m; { [ "$oldest" = 0 ] || [ "$m" -lt "$oldest" ]; } && oldest=$m
    n=$((n+1)); echo "  $id → $d  ($(date -r "$m" '+%m-%d %H:%M' 2>/dev/null || date -d "@$m" '+%m-%d %H:%M' 2>/dev/null))"
done
[ "$n" -gt 0 ] || { echo "확장 dylib 없음: $SRC (먼저 CARGO_TARGET_DIR=target/lite cargo build --release -p nsql-driver-<id> …)"; exit 2; }
echo "드라이버 ${n}개 · protocol ${PROTO} · 폴더 = ${OUT}"
# 수정 시각이 10분 넘게 벌어지면 예전 빌드가 섞인 것 — `-i`로 좁히라고 알린다(S3).
if [ $((newest - oldest)) -gt 600 ]; then echo "⚠ 수정 시각이 $(( (newest - oldest) / 60 ))분 벌어짐 — 예전 빌드가 섞였을 수 있다 · -i <id,…>로 방금 빌드한 것만 고르기"; fi
echo "실행: NSQL_DRIVER_DIR=\"$OUT\" \"$ROOT/target/release/nexa-sql\" &"
