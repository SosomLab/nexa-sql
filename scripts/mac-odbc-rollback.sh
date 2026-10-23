#!/usr/bin/env bash
# macOS ODBC 검증(10-24 · Intel 맥 · journal 10-24)을 위해 설치한 것을 **전부 되돌린다**.
#   기본 = 목록만 보여 준다(dry-run) · `--yes` = 실제 실행 · `--with-unixodbc` = 빌드 전제(unixODBC · 61 §3 ⑥)까지 제거.
#
# 설치했던 것(설치 스크립트 = scripts/mac-odbc-setup.sh · 되돌리는 순서는 그 역순):
#   1. Homebrew 탭 신뢰 + 탭      : brew trust microsoft/mssql-release · brew tap microsoft/mssql-release
#   2. msodbcsql18(+ 의존 openssl@4) : brew install msodbcsql18 → odbcinst.ini [ODBC Driver 18 for SQL Server] 자동 등록
#   3. OpenSSL 3 링크 2개          : /usr/local/Cellar/openssl@4/<ver>/lib/{libssl,libcrypto}.3.dylib → openssl@3
#                                     (드라이버가 /usr/local/opt/openssl/lib/에서 1.0/1.1/3.0만 찾는데 opt/openssl = openssl@4라 필요)
#   4. sqliteodbc                  : brew install sqliteodbc → odbcinst -i -d [SQLite3]
#   5. (빌드 전제 · 기본 유지)      : brew install unixodbc
#   앱 안에 남는 것 = 사용자가 만든 ODBC 프로필(`nsql conn rm <이름>` 또는 로그인 창 Delete) · 격리 홈·scratchpad의 시험 파일(자동 정리).
set -u
YES=0; UNIX=0
for a in "$@"; do case "$a" in --yes) YES=1;; --with-unixodbc) UNIX=1;; *) echo "usage: $0 [--yes] [--with-unixodbc]"; exit 2;; esac; done
BREW=$(command -v brew || echo /usr/local/bin/brew)
PFX=$("$BREW" --prefix)
run() { if [ "$YES" = 1 ]; then echo "+ $*"; "$@"; else echo "  (dry-run) $*"; fi; }

echo "▶ 1. OpenSSL 3 링크(openssl@4 Cellar 안)"
for v in "$PFX"/Cellar/openssl@4/*/lib; do
  for f in libssl.3.dylib libcrypto.3.dylib; do
    [ -L "$v/$f" ] && run rm -f "$v/$f"
  done
done

echo "▶ 2. SQLite3 ODBC 드라이버 등록 해제 + 패키지"
"$PFX"/bin/odbcinst -q -d 2>/dev/null | grep -q '^\[SQLite3\]' && run "$PFX"/bin/odbcinst -u -d -n "SQLite3"
"$BREW" list --versions sqliteodbc >/dev/null 2>&1 && run "$BREW" uninstall sqliteodbc

echo "▶ 3. Microsoft ODBC Driver 18 등록 해제 + 패키지"
"$PFX"/bin/odbcinst -q -d 2>/dev/null | grep -q 'ODBC Driver 18 for SQL Server' && run "$PFX"/bin/odbcinst -u -d -n "ODBC Driver 18 for SQL Server"
"$BREW" list --versions msodbcsql18 >/dev/null 2>&1 && run "$BREW" uninstall msodbcsql18

echo "▶ 4. msodbcsql18의 의존으로 들어온 openssl@4(다른 패키지가 안 쓸 때만)"
if "$BREW" list --versions openssl@4 >/dev/null 2>&1; then
  users=$("$BREW" uses --installed openssl@4 2>/dev/null | tr '\n' ' ')
  if [ -z "$users" ]; then run "$BREW" uninstall openssl@4;
  elif [ "$YES" = 0 ] && [ "$users" = "microsoft/mssql-release/msodbcsql18 " ]; then echo "  (dry-run) $BREW uninstall openssl@4   # 3단계에서 msodbcsql18이 지워지면 사용처 0";
  else echo "  유지: openssl@4를 쓰는 패키지 = $users"; fi
fi

echo "▶ 5. Homebrew 탭 신뢰 해제 + 탭 제거"
"$BREW" tap | grep -q '^microsoft/mssql-release$' && { run "$BREW" untrust microsoft/mssql-release; run "$BREW" untap microsoft/mssql-release; }

if [ "$UNIX" = 1 ]; then
  echo "▶ 6. unixODBC(빌드 전제 · --with-unixodbc)"
  "$BREW" list --versions unixodbc >/dev/null 2>&1 && run "$BREW" uninstall unixodbc
else
  echo "▶ 6. unixODBC = 유지(개발 빌드 전제 · 61 §3 ⑥ · 제거하려면 --with-unixodbc)"
fi

echo "▶ 남은 등록 드라이버:"; "$PFX"/bin/odbcinst -q -d 2>/dev/null || echo "  (unixODBC 없음 또는 등록 0)"
[ "$YES" = 1 ] || echo "※ 실제로 되돌리려면: $0 --yes"
