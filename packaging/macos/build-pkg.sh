#!/usr/bin/env bash
# build-pkg.sh — macOS 설치본 `.pkg`(pkgbuild + productbuild · D-50 postinstall = /usr/local/bin/nsql 링크).
#
#   packaging/macos/build-app.sh && packaging/macos/build-pkg.sh
#
# 산출: target/packaging/macos/nexa-sql-<ver>-macos-universal.pkg (+ 서명본은 같은 이름으로 대체)
# 구조: component pkg(Nexa SQL.app → /Applications · scripts/postinstall) → product archive(distribution.xml · 라이선스 화면).
# 서명·공증(DR-20 · 자리만):
#   MACOS_INSTALLER_IDENTITY="Developer ID Installer: …"       → productsign
#   MACOS_NOTARY_PROFILE=<notarytool keychain profile>          → xcrun notarytool submit --wait + stapler
#   둘 다 없으면 미서명 pkg(결과 줄에 "unsigned" 표기).
# 제거: packaging/macos/uninstall.sh (번들 Resources에도 동봉).
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

OUT="$OUT_ROOT/macos"
APP="$OUT/$APP_NAME.app"
[ -d "$APP" ] || die "번들 없음: $APP — 먼저 build-app.sh"
[ -x "$APP/Contents/MacOS/nsql" ] || die "번들이 불완전함(Contents/MacOS/nsql 없음)"

WORK="$OUT/pkg"; rm -rf "$WORK"; mkdir -p "$WORK/root" "$WORK/res"
COMPONENT="$WORK/nexa-sql-component.pkg"
FINAL="$OUT/nexa-sql-$VERSION-macos-universal.pkg"

step "component pkg (pkgbuild)"
cp -R "$APP" "$WORK/root/"
chmod 0755 "$ROOT/packaging/macos/scripts/postinstall"
pkgbuild --root "$WORK/root" \
         --install-location /Applications \
         --identifier "$BUNDLE_ID" \
         --version "$VERSION" \
         --scripts "$ROOT/packaging/macos/scripts" \
         "$COMPONENT" >/dev/null
note "$(basename "$COMPONENT") $(du -h "$COMPONENT" | cut -f1)"

step "product archive (productbuild)"
cp "$ROOT/LICENSE.md" "$WORK/res/LICENSE.md"
cat > "$WORK/res/welcome.txt" <<EOF
Nexa SQL $VERSION — 크로스플랫폼 경량 SQL 클라이언트 + CLI nsql

설치 위치: /Applications/Nexa SQL.app
CLI: 설치 후 /usr/local/bin/nsql → Nexa SQL.app/Contents/MacOS/nsql 링크가 만들어집니다.
제거: sudo "/Applications/Nexa SQL.app/Contents/Resources/uninstall.sh"  (사용자 데이터까지: --purge)
설정·프로필: ~/Library/Application Support/nexa-sql/
EOF
sed -e "s/@VERSION@/$VERSION/g" -e "s|@PKG@|$(basename "$COMPONENT")|g" \
    "$ROOT/packaging/macos/distribution.xml" > "$WORK/distribution.xml"
productbuild --distribution "$WORK/distribution.xml" \
             --resources "$WORK/res" \
             --package-path "$WORK" \
             --version "$VERSION" \
             "$WORK/unsigned.pkg" >/dev/null

step "서명 · 공증"
if [ -n "${MACOS_INSTALLER_IDENTITY:-}" ]; then
    productsign --sign "$MACOS_INSTALLER_IDENTITY" "$WORK/unsigned.pkg" "$FINAL" >/dev/null
    note "productsign: $MACOS_INSTALLER_IDENTITY"
    if [ -n "${MACOS_NOTARY_PROFILE:-}" ]; then
        xcrun notarytool submit "$FINAL" --keychain-profile "$MACOS_NOTARY_PROFILE" --wait
        xcrun stapler staple "$FINAL"; note "공증 + staple ✓"
    else
        note "공증 생략(MACOS_NOTARY_PROFILE 없음)"
    fi
else
    mv "$WORK/unsigned.pkg" "$FINAL"; note "unsigned(MACOS_INSTALLER_IDENTITY 없음 · DR-20)"
fi

step "검증 (설치하지 않고 내용만 — pkgutil --expand · lsbom)"
X="$WORK/expand"; rm -rf "$X"; pkgutil --expand "$FINAL" "$X"
[ -f "$X/Distribution" ] && note "Distribution ✓ ($(grep -o 'hostArchitectures="[^"]*"' "$X/Distribution"))"
lsbom -s "$X/$(basename "$COMPONENT")/Bom" > "$WORK/files.txt"
grep -q "Nexa SQL.app/Contents/MacOS/nsql\$" "$WORK/files.txt" && note "payload: Contents/MacOS/nsql ✓"
grep -q "Nexa SQL.app/Contents/MacOS/nexa-sql\$" "$WORK/files.txt" && note "payload: Contents/MacOS/nexa-sql ✓"
grep -q "Nexa SQL.app/Contents/Resources/icon.icns\$" "$WORK/files.txt" && note "payload: Resources/icon.icns ✓"
[ -x "$X/$(basename "$COMPONENT")/Scripts/postinstall" ] && note "Scripts/postinstall ✓ (/usr/local/bin/nsql 링크)"
note "파일 $(wc -l < "$WORK/files.txt" | tr -d ' ')개 · $(du -h "$FINAL" | cut -f1) · sha256 $(sha256_of "$FINAL")"
echo "PKG=$FINAL"
