#!/bin/sh
# uninstall.sh — Nexa SQL 제거(macOS). pkg 설치본에는 제거기가 없으므로 이 스크립트가 그 역할이다
#   (번들 `Contents/Resources/uninstall.sh`에도 동봉 · docs/33 §3 verify "제거 → 잔여 파일 0(사용자 폴더는 보존)").
#
#   sudo "/Applications/Nexa SQL.app/Contents/Resources/uninstall.sh"          # 앱 + CLI 링크 + pkg 영수증
#   sudo … uninstall.sh --purge                                                  # + 사용자 데이터(설정·프로필·로그·캐시)까지
#
# 지우는 것: /Applications/Nexa SQL.app · /usr/local/bin/nsql(우리 링크일 때만) · pkgutil 영수증(com.sosomlab.nexa-sql).
# 보존(기본): ~/Library/Application Support/nexa-sql · ~/Library/Logs/nexa-sql · ~/Library/Caches/nexa-sql — --purge로만.
set -eu
APP="/Applications/Nexa SQL.app"
LINK="/usr/local/bin/nsql"
PKG_ID="com.sosomlab.nexa-sql"
PURGE=0; [ "${1:-}" = "--purge" ] && PURGE=1

if [ -L "$LINK" ] && [ "$(readlink "$LINK")" = "$APP/Contents/MacOS/nsql" ]; then rm -f "$LINK"; echo "제거: $LINK"; fi
[ -d "$APP" ] && { rm -rf "$APP"; echo "제거: $APP"; }
pkgutil --pkgs 2>/dev/null | grep -q "^$PKG_ID\$" && { pkgutil --forget "$PKG_ID" >/dev/null && echo "영수증 삭제: $PKG_ID"; }

if [ "$PURGE" = 1 ]; then
    # sudo로 돌리면 $HOME이 root라 실제 사용자 홈을 찾는다.
    U="${SUDO_USER:-$(id -un)}"; H="$(dscl . -read "/Users/$U" NFSHomeDirectory 2>/dev/null | awk '{print $2}')"; H="${H:-$HOME}"
    for d in "$H/Library/Application Support/nexa-sql" "$H/Library/Logs/nexa-sql" "$H/Library/Caches/nexa-sql"; do
        [ -e "$d" ] && { rm -rf "$d"; echo "제거(사용자 데이터): $d"; }
    done
else
    echo "사용자 데이터는 보존했다(~/Library/Application Support/nexa-sql 등) — 함께 지우려면 --purge"
fi
echo "완료"
