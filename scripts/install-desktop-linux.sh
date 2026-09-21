#!/usr/bin/env bash
# install-desktop-linux.sh — 개발 PC의 사용자 영역에 nexa-sql `.desktop` + hicolor 아이콘을 설치한다(sudo 0 · 사용자 09-22 "리눅스에서 아이콘이
#   기본 톱니바퀴"). Wayland/GNOME은 창 아이콘을 앱이 직접 줄 수 없고 **창의 `app_id`(= `nexa-sql`)와 같은 이름의 `.desktop`의 `Icon=`** 을 쓴다.
#   패키지(deb/rpm · packaging/linux)가 같은 파일을 /usr/share에 놓는다 — 이 스크립트는 설치본 없이 target/release로 쓰는 개발 PC용.
# 사용:  scripts/install-desktop-linux.sh [-e <exe>] [--oracle <Instant Client 폴더>] [--remove]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; EXE="$ROOT/target/release/nexa-sql"; ORA=""; REMOVE=0
while [ $# -gt 0 ]; do case $1 in -e) EXE=$2; shift;; --oracle) ORA=$2; shift;; --remove) REMOVE=1;; esac; shift; done
APPS="$HOME/.local/share/applications"; ICONS="$HOME/.local/share/icons/hicolor"
if [ $REMOVE = 1 ]; then rm -f "$APPS/nexa-sql.desktop"; for s in 16 24 32 48 64 128 256 512; do rm -f "$ICONS/${s}x${s}/apps/nexa-sql.png"; done; echo "removed"; exit 0; fi
mkdir -p "$APPS"
for s in 16 24 32 48 64 128 256 512; do
  src="$ROOT/packaging/branding/png/nexa-sql-$s.png"; [ -f "$src" ] || continue
  mkdir -p "$ICONS/${s}x${s}/apps" && cp -f "$src" "$ICONS/${s}x${s}/apps/nexa-sql.png"
done
# Exec: 데스크톱 런처는 셸 환경(.zshrc)이 없다 → Oracle 폴더를 주면 env로 LD_LIBRARY_PATH·TNS_ADMIN을 넣는다(앱 자동 탐지 ③).
EXEC="\"$EXE\" %F"; [ -n "$ORA" ] && EXEC="env LD_LIBRARY_PATH=\"$ORA\" TNS_ADMIN=\"$ORA/network/admin\" NLS_LANG=AMERICAN_AMERICA.AL32UTF8 $EXEC"
sed -e "s|^Exec=.*|Exec=$EXEC|" -e "s|^Icon=.*|Icon=nexa-sql|" "$ROOT/packaging/linux/nexa-sql.desktop" > "$APPS/nexa-sql.desktop"
chmod 644 "$APPS/nexa-sql.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f -t "$ICONS" 2>/dev/null
command -v xdg-icon-resource >/dev/null && xdg-icon-resource forceupdate 2>/dev/null
echo "installed: $APPS/nexa-sql.desktop (Exec=$EXEC) · icons $ICONS/*/apps/nexa-sql.png — 실행 중인 앱은 다시 띄워야 app_id로 아이콘이 붙는다"
