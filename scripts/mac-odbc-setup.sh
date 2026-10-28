#!/usr/bin/env bash
# macOS ODBC 검증 환경 설치(10-24 · Intel/Apple Silicon · journal 10-24 · 되돌리기 = scripts/mac-odbc-rollback.sh).
#   ① unixODBC(빌드 전제이자 드라이버 관리자) ② Microsoft ODBC Driver 18(SQL Server) + openssl@4 환경 처방 ③ SQLite3 ODBC(왕복 검증용)
#   끝나면 `odbcinst -q -d`에 [ODBC Driver 18 for SQL Server] · [SQLite3]가 보인다. 검증 명령은 아래 "검증" 절.
#   Linux 대응 = apt: unixodbc unixodbc-dev · Microsoft 저장소 msodbcsql18(ACCEPT_EULA=Y) · libsqliteodbc(odbcinst.ini 수동 등록).
set -eu
BREW=$(command -v brew || echo /usr/local/bin/brew)
PFX=$("$BREW" --prefix)
export HOMEBREW_NO_AUTO_UPDATE=1

echo "▶ 1. unixODBC"
"$BREW" list --versions unixodbc >/dev/null 2>&1 || "$BREW" install unixodbc

echo "▶ 2. Microsoft ODBC Driver 18 for SQL Server(탭 신뢰 → 설치 · odbcinst.ini 자동 등록)"
"$BREW" tap | grep -q '^microsoft/mssql-release$' || "$BREW" tap microsoft/mssql-release https://github.com/Microsoft/homebrew-mssql-release
"$BREW" trust microsoft/mssql-release >/dev/null 2>&1 || true
"$BREW" list --versions msodbcsql18 >/dev/null 2>&1 || HOMEBREW_ACCEPT_EULA=Y "$BREW" install msodbcsql18

echo "▶ 2-b. openssl@4 환경 처방 — 드라이버는 \$PFX/opt/openssl/lib/에서 OpenSSL 1.0/1.1/3.0만 찾는다(opt/openssl = openssl@4면 실패)"
if [ "$(readlink "$PFX/opt/openssl" 2>/dev/null | grep -c 'openssl@4')" = 1 ]; then
  "$BREW" list --versions openssl@3 >/dev/null 2>&1 || "$BREW" install openssl@3
  d=$(readlink -f "$PFX/opt/openssl@4")/lib
  for f in libssl.3.dylib libcrypto.3.dylib; do
    [ -e "$d/$f" ] || ln -s "$PFX/opt/openssl@3/lib/$f" "$d/$f"
  done
  echo "  링크: $d/{libssl,libcrypto}.3.dylib → openssl@3"
else
  echo "  opt/openssl이 openssl@4가 아니라 처방 불필요"
fi

echo "▶ 3. SQLite3 ODBC(왕복 검증용 · 자격 증명 없이 접속+조회)"
"$BREW" list --versions sqliteodbc >/dev/null 2>&1 || "$BREW" install sqliteodbc
if ! "$PFX"/bin/odbcinst -q -d 2>/dev/null | grep -q '^\[SQLite3\]'; then
  t=$(mktemp); printf '[SQLite3]\nDescription=SQLite3 ODBC Driver\nDriver=%s/lib/libsqlite3odbc.dylib\n' "$PFX" > "$t"
  "$PFX"/bin/odbcinst -i -d -f "$t"; rm -f "$t"
fi

echo "▶ 등록된 드라이버:"; "$PFX"/bin/odbcinst -q -d
cat <<'EOF'
▶ 검증(예):
  # 드라이버·네트워크·TLS·로그인 단계(비밀번호 틀리면 28000 '로그인하지 못했습니다' = 거기까지 정상)
  isql -v -k "Driver={ODBC Driver 18 for SQL Server};Server=172.16.1.58,1433;UID=BISCM_MS;PWD=…;TrustServerCertificate=yes"
  printf '…\n' | nsql run -c "odbc:Driver={ODBC Driver 18 for SQL Server};Server=172.16.1.58,1433;Database=M4PLAN_MS;TrustServerCertificate=yes;UID=BISCM_MS" --password-stdin q.sql
  # 어댑터 왕복(자격 증명 불필요)
  nsql run -c "odbc:Driver={SQLite3};Database=/tmp/odbc-test.sqlite" roundtrip.sql   # CREATE · INSERT(한글) · SELECT
EOF
