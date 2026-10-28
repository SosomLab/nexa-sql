#!/usr/bin/env bash
# Linux ODBC 검증 환경 설치(10-24 · Ubuntu/Debian · 맥판 = scripts/mac-odbc-setup.sh · 되돌리기 = scripts/linux-odbc-rollback.sh).
#   ① unixODBC(빌드 전제 + 드라이버 관리자 · unixodbc unixodbc-dev odbcinst)
#   ② SQLite3 ODBC(libsqliteodbc · 자격 증명 없이 어댑터 왕복 검증) ③ PostgreSQL ODBC(odbc-postgresql · psqlodbc Unicode/ANSI)
#   ④ Microsoft ODBC Driver 18 for SQL Server(packages.microsoft.com 저장소 = `packages-microsoft-prod` 패키지 + ACCEPT_EULA=Y · `--no-mssql`로 건너뜀)
#   root가 필요한 부분은 **한 번의 pkexec**(폴킷 GUI 인증)로 묶어 돈다(비대화 sudo 불가 PC · 61 §3). 끝나면 `odbcinst -q -d`에 세 드라이버가 보인다.
#   검증 = scripts/linux-odbc-e2e.sh.
set -eu
MSSQL=1
for a in "$@"; do case "$a" in --no-mssql) MSSQL=0;; --root-part) ROOT_PART=1;; *) echo "usage: $0 [--no-mssql]"; exit 2;; esac; done

if [ "${ROOT_PART:-0}" = 1 ]; then
  # ── root 부분(pkexec로 재실행된다) ──
  [ "$(id -u)" = 0 ] || { echo "root가 아닙니다"; exit 1; }
  export DEBIAN_FRONTEND=noninteractive
  echo "▶ 1. unixODBC + 2. SQLite3 ODBC + 3. PostgreSQL ODBC(apt)"
  apt-get install -y -qq unixodbc unixodbc-dev odbcinst libsqliteodbc odbc-postgresql
  if [ "${MSSQL_ROOT:-1}" = 1 ]; then
    echo "▶ 4. Microsoft ODBC Driver 18(저장소 패키지 → apt update → msodbcsql18 · EULA 동의)"
    if ! dpkg -s packages-microsoft-prod >/dev/null 2>&1; then
      . /etc/os-release
      t=$(mktemp -d); curl -fsSL -o "$t/pmp.deb" "https://packages.microsoft.com/config/ubuntu/${VERSION_ID}/packages-microsoft-prod.deb"
      dpkg -i "$t/pmp.deb" >/dev/null; rm -rf "$t"
    fi
    apt-get update -qq
    ACCEPT_EULA=Y apt-get install -y -qq msodbcsql18
  fi
  echo "▶ 등록 점검(libsqliteodbc는 postinst가 [SQLite3]를 등록한다 · 없으면 수동)"
  if ! odbcinst -q -d 2>/dev/null | grep -q '^\[SQLite3\]'; then
    so=$(ls /usr/lib/*/odbc/libsqlite3odbc.so 2>/dev/null | head -1)
    t=$(mktemp); printf '[SQLite3]\nDescription=SQLite3 ODBC Driver\nDriver=%s\n' "$so" > "$t"; odbcinst -i -d -f "$t"; rm -f "$t"
  fi
  exit 0
fi

echo "▶ root 작업 한 묶음(pkexec 인증 창)"
export DISPLAY="${DISPLAY:-:0}"
MSSQL_ROOT=$MSSQL pkexec env MSSQL_ROOT=$MSSQL bash "$(readlink -f "$0")" --root-part
echo "▶ 등록된 드라이버:"; odbcinst -q -d
cat <<'EOT'
▶ 검증(예 · 자세히 = scripts/linux-odbc-e2e.sh):
  nsql run -c "odbc:Driver={SQLite3};Database=/tmp/odbc-test.sqlite" roundtrip.sql           # CREATE · INSERT(한글) · SELECT
  isql -v -k "Driver={PostgreSQL Unicode};Server=172.16.1.60;Port=5433;Database=repodb;UID=postgres;PWD=…"
  isql -v -k "Driver={ODBC Driver 18 for SQL Server};Server=172.16.1.58,1433;UID=APPDB_MS;PWD=…;TrustServerCertificate=yes"
EOT
