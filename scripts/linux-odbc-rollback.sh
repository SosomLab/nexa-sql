#!/usr/bin/env bash
# Linux ODBC 검증(10-24 · scripts/linux-odbc-setup.sh)으로 설치한 것을 **되돌린다**(맥판 = scripts/mac-odbc-rollback.sh).
#   기본 = 목록만(dry-run) · `--yes` = 실제 실행(root 부분은 pkexec 한 번) · `--with-unixodbc` = 빌드 전제(unixodbc-dev 등)까지 제거.
#   되돌리는 순서(설치 역순): msodbcsql18 → packages-microsoft-prod(저장소·키 · apt update) → odbc-postgresql → libsqliteodbc → 남은 odbcinst 등록 → (선택) unixODBC.
#   앱 안에 남는 것 = 사용자가 만든 ODBC 프로필(`nsql conn rm <이름>`) · 격리 홈·scratchpad 시험 파일(자동 정리).
set -u
YES=0; UNIX=0
for a in "$@"; do case "$a" in --yes) YES=1;; --with-unixodbc) UNIX=1;; --root-part) ROOT_PART=1;; *) echo "usage: $0 [--yes] [--with-unixodbc]"; exit 2;; esac; done
has() { dpkg -s "$1" >/dev/null 2>&1; }

if [ "${ROOT_PART:-0}" = 1 ]; then
  export DEBIAN_FRONTEND=noninteractive
  pk=""; for p in msodbcsql18 odbc-postgresql libsqliteodbc; do has $p && pk="$pk $p"; done
  [ -n "$pk" ] && { echo "+ apt-get purge -y$pk"; apt-get purge -y -qq $pk; }
  if has packages-microsoft-prod; then echo "+ apt-get purge packages-microsoft-prod + apt update"; apt-get purge -y -qq packages-microsoft-prod; rm -f /etc/apt/sources.list.d/microsoft-prod.list /usr/share/keyrings/microsoft-prod.gpg; apt-get update -qq; fi
  for d in "SQLite3" "SQLite" "PostgreSQL Unicode" "PostgreSQL ANSI" "ODBC Driver 18 for SQL Server"; do
    odbcinst -q -d 2>/dev/null | grep -qx "\[$d\]" && { echo "+ odbcinst -u -d -n \"$d\""; odbcinst -u -d -n "$d" >/dev/null; }
  done
  if [ "${UNIX_ROOT:-0}" = 1 ]; then echo "+ apt-get purge unixodbc-dev unixodbc odbcinst"; apt-get purge -y -qq unixodbc-dev unixodbc odbcinst; fi
  apt-get autoremove -y -qq >/dev/null
  exit 0
fi

echo "▶ 되돌릴 것:"
for p in msodbcsql18 packages-microsoft-prod odbc-postgresql libsqliteodbc; do has $p && echo "  패키지 $p"; done
echo "  odbcinst 등록: $(odbcinst -q -d 2>/dev/null | tr '\n' ' ')"
[ "$UNIX" = 1 ] && echo "  unixODBC(unixodbc-dev unixodbc odbcinst) = 제거(--with-unixodbc)" || echo "  unixODBC = 유지(개발 빌드 전제 · 61 §3 · 제거하려면 --with-unixodbc)"
if [ "$YES" = 1 ]; then
  export DISPLAY="${DISPLAY:-:0}"
  pkexec env UNIX_ROOT=$UNIX bash "$(readlink -f "$0")" --root-part
  echo "▶ 남은 등록 드라이버:"; odbcinst -q -d 2>/dev/null || echo "  (unixODBC 없음 또는 등록 0)"
else
  echo "※ 실제로 되돌리려면: $0 --yes [--with-unixodbc]"
fi
