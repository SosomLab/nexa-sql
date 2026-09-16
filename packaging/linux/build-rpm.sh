#!/usr/bin/env bash
# build-rpm.sh — Linux 설치본 `.rpm`(rpmbuild + nexa-sql.spec · deb와 같은 FHS 스테이징을 포장).
#
#   packaging/linux/build-deb.sh [--skip-build] && packaging/linux/build-rpm.sh
#
# 스테이징 = build-deb.sh가 만든 target/packaging/linux/deb-root/usr (없으면 build-deb.sh를 먼저 돌린다 · 같은 바이너리 보증).
# 산출: target/packaging/linux/nexa-sql-<ver>-1.<x86_64|aarch64>.rpm
# 검증(설치 없이): rpm -qpi / -qpl / -K. 설치 스모크는 rpm 계열 러너가 없어 CI에서도 목록 검증만(ubuntu는 rpm 설치 불가).
source "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"

have rpmbuild || die "rpmbuild 없음(apt-get install rpm / dnf install rpm-build)"
TARGET="${NSQL_LINUX_TARGET:-x86_64-unknown-linux-gnu}"
case "$TARGET" in aarch64-*) RARCH=aarch64 ;; *) RARCH=x86_64 ;; esac

OUT="$OUT_ROOT/linux"; STAGE="$OUT/deb-root/usr"
if [ ! -x "$STAGE/bin/nsql" ]; then
    note "스테이징 없음 → build-deb.sh 실행"
    "$ROOT/packaging/linux/build-deb.sh" "$@" >/dev/null
fi
[ -x "$STAGE/bin/nsql" ] || die "스테이징 없음: $STAGE"

TOP="$OUT/rpmbuild"; rm -rf "$TOP"; mkdir -p "$TOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
# RPM Version 필드는 '-'를 못 쓴다 — 사전 릴리스 접미사는 '~'로(정렬상 앞선다 · rpm 규약).
RPM_VER="${VERSION//-/\~}"

step "rpmbuild ($RARCH · $RPM_VER)"
rpmbuild -bb "$ROOT/packaging/linux/nexa-sql.spec" \
    --define "_topdir $TOP" \
    --define "_version $RPM_VER" \
    --define "_stagedir $STAGE" \
    --target "$RARCH" >/dev/null
FINAL="$(find "$TOP/RPMS" -name '*.rpm' | head -1)"
[ -n "$FINAL" ] || die "rpm 산출물 없음"
DEST="$OUT/$(basename "$FINAL")"; mv "$FINAL" "$DEST"; FINAL="$DEST"
note "$(basename "$FINAL") $(du -h "$FINAL" | cut -f1) · sha256 $(sha256_of "$FINAL")"

step "검증(설치 없이)"
rpm -qpi "$FINAL" 2>/dev/null | sed -n '1,8p' | sed 's/^/    /'
rpm -qpl "$FINAL" 2>/dev/null | grep -E 'bin/(nexa-sql|nsql)$|nexa-sql.desktop$|256x256/apps/nexa-sql.png$' | sed 's/^/    /'
echo "RPM=$FINAL"
