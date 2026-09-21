#!/usr/bin/env bash
# Oracle Instant Client(Linux x86-64) 설치 — `install-instantclient-mac.sh`의 Linux 판(사용자 09-22 · Ubuntu 26.04에서 확인).
#   sudo 없이 홈 폴더에 풀고, 앱·`sqlplus`가 찾는 `libaio.so.1`은 Ubuntu 24.04+의 `libaio1t64`(`libaio.so.1t64`)에 **폴더 안 심볼릭 링크**로 잇는다
#   (시스템 폴더는 건드리지 않는다). 패키지 설치만 관리자 권한이 필요하다 — 터미널에 sudo가 없으면 `pkexec`(GUI 인증 창)로 시도한다.
#
# 사용:
#   scripts/install-instantclient-linux.sh                 # ~/oracle/instantclient_<ver> 에 basic + sqlplus
#   INSTANT_CLIENT_ROOT=/opt/oracle scripts/install-instantclient-linux.sh
#   scripts/install-instantclient-linux.sh basic sqlplus sdk tools
#   scripts/install-instantclient-linux.sh --rc            # ~/.zshrc(또는 ~/.bashrc)에 LD_LIBRARY_PATH·PATH·TNS_ADMIN·NLS_LANG 블록 추가까지
# 앱: 터미널에서 띄우면 자동 탐지 ③(`LD_LIBRARY_PATH`)으로 찾는다 · 데스크톱 런처는 설정 ▸ DBMS ▸ Oracle 직접 지정(`oracle.client_dir`) 또는
#     `NSQL_ORACLE_CLIENT_DIR`. [docs/64 §2](../docs/64-dbms-clients-and-driver-packaging.md)
set -euo pipefail
ROOT="${INSTANT_CLIENT_ROOT:-$HOME/oracle}"; ADD_RC=0; PKGS=()
for a in "$@"; do case "$a" in --rc) ADD_RC=1 ;; -h|--help) sed -n '2,14p' "$0"; exit 0 ;; *) PKGS+=("$a") ;; esac; done
[ ${#PKGS[@]} -eq 0 ] && PKGS=(basic sqlplus)
[ "$(uname -m)" = x86_64 ] || { echo "x86_64만 지원(이 기기: $(uname -m) — ARM은 Oracle이 linux.arm64 패키지를 따로 낸다)"; exit 2; }
BASE=https://download.oracle.com/otn_software/linux/instantclient   # 버전 없는 "latest" 영구 주소(로그인 불필요)
DL="${TMPDIR:-/tmp}/instantclient-dl"; mkdir -p "$ROOT" "$DL"
for p in "${PKGS[@]}"; do
  f="$DL/instantclient-$p-linuxx64.zip"
  if [ ! -s "$f" ]; then echo "↓ $p"; curl -fL --retry 3 -o "$f" "$BASE/instantclient-$p-linuxx64.zip" || { echo "  ✗ 다운로드 실패: $p"; rm -f "$f"; continue; }; else echo "· $p — 캐시 사용"; fi
  unzip -qo "$f" -d "$ROOT"       # zip 안의 instantclient_<ver>/ 가 그대로 ROOT 아래에 놓인다
done
DEST="$(ls -d "$ROOT"/instantclient_* 2>/dev/null | sort -V | tail -1)"; [ -n "$DEST" ] || { echo "풀린 폴더가 없다"; exit 1; }
echo "설치 위치: $DEST"
# libaio: Ubuntu 24.04+ = libaio1t64(libaio.so.1t64) · 그 전/다른 배포판 = libaio1(libaio.so.1).
if ! ldconfig -p | grep -qE 'libaio\.so\.1(t64)?\b'; then
  echo "libaio가 없다 → 설치(관리자 권한)"
  if sudo -n true 2>/dev/null; then sudo apt-get install -y libaio1t64 || sudo apt-get install -y libaio1
  elif command -v pkexec >/dev/null; then pkexec env DEBIAN_FRONTEND=noninteractive apt-get install -y libaio1t64 || pkexec apt-get install -y libaio1
  else echo "  ✗ sudo/pkexec 불가 — 'apt install libaio1t64'를 직접 실행한 뒤 다시"; fi
fi
t64="$(ldconfig -p | awk '/libaio\.so\.1t64/{print $NF; exit}')"
if [ -n "$t64" ] && [ ! -e "$DEST/libaio.so.1" ]; then ln -s "$t64" "$DEST/libaio.so.1"; echo "링크: $DEST/libaio.so.1 → $t64"; fi
mkdir -p "$DEST/network/admin"
[ -f "$DEST/network/admin/tnsnames.ora" ] || printf '# tnsnames.ora — TNS_ADMIN=%s\n# ORCL = (DESCRIPTION = (ADDRESS = (PROTOCOL = TCP)(HOST = db.example.com)(PORT = 1521)) (CONNECT_DATA = (SERVICE_NAME = orclpdb1)))\n' "$DEST/network/admin" > "$DEST/network/admin/tnsnames.ora"
if [ "$ADD_RC" = 1 ]; then
  RC="$HOME/.zshrc"; [ -n "${ZSH_VERSION:-}" ] || [ "$(basename "${SHELL:-}")" = zsh ] || RC="$HOME/.bashrc"
  if ! grep -q 'Oracle Instant Client' "$RC" 2>/dev/null; then
    cat >> "$RC" <<EOR

# --- Oracle Instant Client ($(date +%F) · $DEST) ---
export ORACLE_IC_HOME="$DEST"
export LD_LIBRARY_PATH="\$ORACLE_IC_HOME\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
export PATH="\$ORACLE_IC_HOME:\$PATH"
export TNS_ADMIN="\$ORACLE_IC_HOME/network/admin"
export NLS_LANG="AMERICAN_AMERICA.AL32UTF8"
# --- end Oracle Instant Client ---
EOR
    echo "$RC 에 환경 블록 추가 — 새 셸에서 적용"
  fi
fi
echo "확인:"; LD_LIBRARY_PATH="$DEST" "$DEST/sqlplus" -V 2>/dev/null | head -2 || echo "  sqlplus 없음(basic만 설치했으면 정상)"
LD_LIBRARY_PATH="$DEST" ldd "$DEST/libclntsh.so" | grep -c 'not found' | xargs -I{} echo "  libclntsh.so 미해결 의존: {}"
