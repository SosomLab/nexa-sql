#!/usr/bin/env bash
# Oracle Instant Client(macOS) 전체 패키지를 한 폴더에 설치한다 — Windows 절차(7z x … -o C:\Oracle)의 macOS 판.
#
#   Intel(x86_64) : 19.16 이 마지막 · Oracle이 **DMG만** 제공 → 마운트 후 복사
#   Apple Silicon : 23.26.2 · 버전 경로에 **ZIP** 제공 → unzip
#
# 사용:
#   scripts/install-instantclient-mac.sh                      # 기본 ~/Oracle 에 basic·sqlplus·tools·sdk·odbc·jdbc 설치
#   INSTANT_CLIENT_ROOT=/opt/oracle scripts/install-instantclient-mac.sh
#   scripts/install-instantclient-mac.sh basic sqlplus        # 원하는 패키지만
#   scripts/install-instantclient-mac.sh --rc                 # ~/.zshrc 에 PATH·TNS_ADMIN·NLS_LANG 추가까지
set -euo pipefail

ROOT="${INSTANT_CLIENT_ROOT:-$HOME/Oracle}"
ADD_RC=0
PKGS=()
for a in "$@"; do
  case "$a" in
    --rc) ADD_RC=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) PKGS+=("$a") ;;
  esac
done
[ ${#PKGS[@]} -eq 0 ] && PKGS=(basic sqlplus tools sdk odbc jdbc)

BASE=https://download.oracle.com/otn_software/mac/instantclient
if [ "$(uname -m)" = "arm64" ]; then
  VER="${INSTANT_CLIENT_VER:-23.26.2.0.0}"; KIND=zip
  DIR_VER=$(echo "$VER" | awk -F. '{printf "%s_%s", $1, $2}')          # 23_26
  url() { echo "$BASE/$(echo "$VER" | tr -d .)00/instantclient-$1-macos.arm64-$VER.zip"; }
else
  VER="${INSTANT_CLIENT_VER:-19.16.0.0.0}"; KIND=dmg
  DIR_VER=$(echo "$VER" | awk -F. '{printf "%s_%s", $1, $2}')          # 19_16
  url() { echo "$BASE/$(echo "$VER" | awk -F. '{printf "%s%02d000", $1, $2}')/instantclient-$1-macos.x64-${VER}dbru.dmg"; }
fi
DEST="$ROOT/instantclient_$DIR_VER"
DL="${TMPDIR:-/tmp}/instantclient-dl"

echo "아키텍처: $(uname -m) · 버전: $VER · 형식: $KIND"
echo "설치 위치: $DEST"
mkdir -p "$DEST" "$DL"

for p in "${PKGS[@]}"; do
  u="$(url "$p")"; f="$DL/$(basename "$u")"
  if [ ! -s "$f" ]; then
    echo "↓ $p — $(basename "$u")"
    curl -fL --retry 3 -o "$f" "$u" || { echo "  ✗ 다운로드 실패(패키지 없음?): $u"; rm -f "$f"; continue; }
  else
    echo "· $p — 캐시 사용"
  fi
  if [ "$KIND" = zip ]; then
    # zip 안에 instantclient_23_26/ 한 겹이 있다 → 그 내용만 DEST로.
    tmp="$DL/x.$p"; rm -rf "$tmp"; mkdir -p "$tmp"
    unzip -qo "$f" -d "$tmp"
    inner="$(find "$tmp" -maxdepth 1 -mindepth 1 -type d | head -1)"
    cp -R -P -p -f "${inner:-$tmp}"/* "$DEST"/
    rm -rf "$tmp"
  else
    mp="$(hdiutil attach -nobrowse -readonly "$f" | awk '/\/Volumes\//{ $1=""; $2=""; sub(/^ +/,""); print; exit}')"
    cp -R -P -p -f "$mp"/* "$DEST"/
    hdiutil detach "$mp" -quiet
    rm -f "$DEST/install_ic.sh" "$DEST/INSTALL_IC_README.txt"
  fi
done

# ★ Gatekeeper 격리 해제 — 안 하면 "확인되지 않은 개발자" 로 dylib·sqlplus 실행이 막힌다.
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true
mkdir -p "$DEST/network/admin"

# ★ 버전 무관 심볼릭 링크 — ~/.zshrc 는 이것만 가리키고, 버전 교체 시 링크만 다시 건다.
ln -sfn "$DEST" "$ROOT/instantclient"

# ★ ODPI-C(nexa-sql)가 기본 탐색하는 ~/lib 에 링크 — GUI 앱은 셸 환경변수를 받지 못하므로 이 경로가 필요하다.
if ls "$DEST"/libclntsh.dylib* >/dev/null 2>&1; then
  mkdir -p "$HOME/lib"
  for l in "$DEST"/libclntsh.dylib* "$DEST"/libclntshcore.dylib* "$DEST"/libnnz*.dylib; do
    [ -e "$l" ] && ln -sf "$l" "$HOME/lib/"
  done
  echo "~/lib 링크: $(ls "$HOME/lib" | tr '\n' ' ')"
fi

RC_BLOCK="# Oracle Instant Client (nexa-sql)
export INSTANT_CLIENT_PATH=\"$ROOT/instantclient\"   # 버전 링크 — 교체 시 링크만 다시 건다
export PATH=\"\$INSTANT_CLIENT_PATH:\$PATH\"
export TNS_ADMIN=\"\$INSTANT_CLIENT_PATH/network/admin\"
export NLS_LANG=KOREAN_KOREA.AL32UTF8
export NSQL_ORACLE_CLIENT_DIR=\"\$INSTANT_CLIENT_PATH\""

if [ "$ADD_RC" = 1 ]; then
  if grep -q "Oracle Instant Client (nexa-sql)" "$HOME/.zshrc" 2>/dev/null; then
    echo "~/.zshrc 이미 설정됨 — 건너뜀"
  else
    printf '\n%s\n' "$RC_BLOCK" >> "$HOME/.zshrc"
    echo "~/.zshrc 에 추가함 — 새 터미널 또는 source ~/.zshrc"
  fi
else
  echo; echo "다음을 ~/.zshrc 에 추가하세요(또는 --rc 로 자동 추가):"; echo "$RC_BLOCK"
fi

echo; echo "설치 파일 $(ls "$DEST" | wc -l | tr -d ' ')개 · sqlplus: $([ -x "$DEST/sqlplus" ] && echo 있음 || echo 없음)"
