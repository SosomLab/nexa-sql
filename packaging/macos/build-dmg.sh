#!/usr/bin/env bash
# build-dmg.sh — 편의용 `.dmg`(drag-to-Applications · hdiutil). 설치본 원칙(DR-27)의 주 산출물은 .pkg이고,
#   dmg는 같은 번들에서 함께 만드는 보조 형식이다(docs/33 §2 "편의용 .dmg도 같은 번들에서 생성" · Homebrew cask 후속).
#   ⚠ dmg 경로에는 postinstall이 없으므로 /usr/local/bin/nsql 링크는 사용자가 직접(README 안내) 또는 cask `binary`가 만든다.
#
#   packaging/macos/build-app.sh && packaging/macos/build-dmg.sh
#
# 산출: target/packaging/macos/nexa-sql-<ver>-macos-universal.dmg
# 공증(자리만): MACOS_NOTARY_PROFILE 이 있고 번들이 Developer ID로 서명돼 있으면 notarytool submit + stapler.
# 이식 원천: ../nexa-clip/.github/workflows/release.yml "macOS 설치본(.dmg)" 단계.
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

OUT="$OUT_ROOT/macos"
APP="$OUT/$APP_NAME.app"
[ -d "$APP" ] || die "번들 없음: $APP — 먼저 build-app.sh"
FINAL="$OUT/nexa-sql-$VERSION-macos-universal.dmg"
DROOT="$OUT/dmgroot"; rm -rf "$DROOT"; mkdir -p "$DROOT"

step "dmg 루트 구성"
cp -R "$APP" "$DROOT/"
ln -s /Applications "$DROOT/Applications"
cp "$ROOT/README.md" "$ROOT/LICENSE.md" "$DROOT/"
cat > "$DROOT/CLI 설치 안내.txt" <<EOF
Nexa SQL $VERSION

1) "Nexa SQL"을 Applications 폴더로 끌어다 놓습니다.
2) 터미널에서 nsql을 쓰려면 링크를 하나 만듭니다(pkg 설치본은 자동):
   sudo ln -sfn "/Applications/Nexa SQL.app/Contents/MacOS/nsql" /usr/local/bin/nsql
3) 서명되지 않은 빌드가 실행되지 않으면(Gatekeeper):
   xattr -dr com.apple.quarantine "/Applications/Nexa SQL.app"
제거: sudo "/Applications/Nexa SQL.app/Contents/Resources/uninstall.sh"
EOF

step "hdiutil create"
rm -f "$FINAL"
hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$DROOT" -ov -format UDZO -quiet "$FINAL"
note "$(basename "$FINAL") $(du -h "$FINAL" | cut -f1) · sha256 $(sha256_of "$FINAL")"

if [ -n "${MACOS_NOTARY_PROFILE:-}" ] && [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    step "공증"
    xcrun notarytool submit "$FINAL" --keychain-profile "$MACOS_NOTARY_PROFILE" --wait
    xcrun stapler staple "$FINAL"; note "공증 + staple ✓"
else
    note "공증 생략(MACOS_NOTARY_PROFILE/MACOS_SIGN_IDENTITY 없음 · unsigned)"
fi

step "검증 — 마운트해서 번들 안 nsql --version"
MNT="$(hdiutil attach -readonly -nobrowse -noautoopen "$FINAL" | awk -F'\t' '/\/Volumes\//{print $NF}' | tail -1)"
[ -n "$MNT" ] || die "마운트 실패"
V="$("$MNT/$APP_NAME.app/Contents/MacOS/nsql" --version)"; note "$V"
ls "$MNT" | sed 's/^/    /'
hdiutil detach "$MNT" -quiet
rm -rf "$DROOT"
echo "DMG=$FINAL"
