#!/usr/bin/env bash
# build-app.sh — macOS `Nexa SQL.app` 스테이징(Universal 2 · docs/33 §2 · T-72/T-62).
#
#   packaging/macos/build-app.sh [--skip-build] [--targets "aarch64-apple-darwin x86_64-apple-darwin"]
#
# 흐름: cargo build --release(타깃별) → lipo(둘 다 있으면 Universal 2 · 하나면 그것만) → .app 스테이징
#       (Contents/MacOS/{nexa-sql,nsql} · Resources/{icon.icns,Packages,LICENSE…}) → 서명(신원 있으면 Developer ID · 없으면 애드혹)
#       → 번들 안 `nsql --version` 실행 검증.
# 산출: target/packaging/macos/Nexa SQL.app  (git 무시 · /target/)
# 이식 원천: ../nexa-clip/.github/workflows/release.yml "macOS 설치본(.dmg)" 단계(sips→iconutil · 애드혹 서명)를 스크립트로 옮김.
#
# 서명(DR-20 · 자리만): MACOS_SIGN_IDENTITY="Developer ID Application: …" 이 있으면 hardened runtime + 타임스탬프로 서명하고,
#   없으면 애드혹(`-`) — Apple Silicon은 서명 0인 번들을 실행하지 않는다. 공증은 build-pkg.sh/build-dmg.sh에서.
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

SKIP_BUILD=0
TARGETS="${NSQL_MAC_TARGETS:-aarch64-apple-darwin x86_64-apple-darwin}"
while [ $# -gt 0 ]; do
    case "$1" in
        --skip-build) SKIP_BUILD=1 ;;
        --targets) shift; TARGETS="$1" ;;
        *) die "알 수 없는 인자: $1" ;;
    esac
    shift
done

OUT="$OUT_ROOT/macos"
APP="$OUT/$APP_NAME.app"
CARGO="$(cargo_bin)"
BINS=(nexa-sql nsql)

step "빌드 (release · $TARGETS)"
BUILT=()
for T in $TARGETS; do
    if [ "$SKIP_BUILD" = 0 ]; then
        if ! rustup target list --installed 2>/dev/null | grep -q "^$T\$"; then
            note "타깃 std 없음: $T (rustup target add $T) — 건너뜀"; continue
        fi
        "$CARGO" build --release --locked -p nexa-sql -p nsql-cli --target "$T"
    fi
    if [ -x "$ROOT/target/$T/release/nexa-sql" ] && [ -x "$ROOT/target/$T/release/nsql" ]; then
        BUILT+=("$T"); note "✓ $T"
    else
        note "산출물 없음: target/$T/release — 건너뜀"
    fi
done
[ ${#BUILT[@]} -gt 0 ] || die "빌드된 타깃이 없다"

step "스테이징 → $APP"
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
for b in "${BINS[@]}"; do
    if [ ${#BUILT[@]} -ge 2 ]; then
        inputs=(); for T in "${BUILT[@]}"; do inputs+=("$ROOT/target/$T/release/$b"); done
        lipo -create "${inputs[@]}" -output "$APP/Contents/MacOS/$b"
    else
        cp "$ROOT/target/${BUILT[0]}/release/$b" "$APP/Contents/MacOS/$b"
    fi
    chmod 0755 "$APP/Contents/MacOS/$b"
    note "$b: $(lipo -archs "$APP/Contents/MacOS/$b" | tr '\n' ' ')($(du -h "$APP/Contents/MacOS/$b" | cut -f1))"
done
sed "s/@VERSION@/$VERSION/g" "$ROOT/packaging/macos/Info.plist" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
stage_common "$APP/Contents/Resources"
# 제거 절차(D-50 링크 포함)를 번들에도 동봉 — 사용자가 pkg를 잃어도 찾을 수 있게.
install -m 0755 "$ROOT/packaging/macos/uninstall.sh" "$APP/Contents/Resources/uninstall.sh"

step "아이콘 (branding PNG → iconutil · 없으면 branding .icns)"
ICONSET="$OUT/icon.iconset"; rm -rf "$ICONSET"; mkdir -p "$ICONSET"
SRC="$BRANDING/nexa-sql-1024.png"
if have iconutil && have sips && [ -f "$SRC" ]; then
    for pair in "16 icon_16x16" "32 icon_16x16@2x" "32 icon_32x32" "64 icon_32x32@2x" "128 icon_128x128" "256 icon_128x128@2x" \
                "256 icon_256x256" "512 icon_256x256@2x" "512 icon_512x512"; do
        set -- $pair; sips -z "$1" "$1" "$SRC" --out "$ICONSET/$2.png" >/dev/null
    done
    cp "$SRC" "$ICONSET/icon_512x512@2x.png"
    iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"
    note "icon.icns ← iconutil($(du -h "$APP/Contents/Resources/icon.icns" | cut -f1))"
elif [ -f "$BRANDING/nexa-sql.icns" ]; then
    # scripts/pack_icon.py가 만든 PNG 페이로드 .icns(브랜딩 SSOT).
    cp "$BRANDING/nexa-sql.icns" "$APP/Contents/Resources/icon.icns"; note "icon.icns ← branding(pack_icon.py)"
else
    die "아이콘 원천 없음: $SRC / nexa-sql.icns (scripts/pack_icon.py로 생성)"
fi
rm -rf "$ICONSET"

step "서명"
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    codesign --force --deep --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" "$APP"
    note "Developer ID 서명: $MACOS_SIGN_IDENTITY"
else
    codesign --force --deep --sign - "$APP"
    note "애드혹 서명(unsigned 배포 · Gatekeeper 안내는 릴리스 노트)"
fi
codesign --verify --deep --strict "$APP" && note "codesign --verify ✓"

step "검증 — 번들 안 nsql --version"
V="$("$APP/Contents/MacOS/nsql" --version)"
note "$V"
case "$V" in "nsql $VERSION") ;; *) die "버전 불일치: '$V' ≠ 'nsql $VERSION'" ;; esac
plutil -lint "$APP/Contents/Info.plist" >/dev/null && note "Info.plist ✓"
echo "APP=$APP"
