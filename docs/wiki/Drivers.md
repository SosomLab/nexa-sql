# Drivers (드라이버)

> **Self-contained drivers (10-25).** Each driver package works on its own: the IBM Db2 driver speaks DRDA natively (no IBM client needed — the clidriver steps below are only for the Windows ODBC fallback), and on Linux/macOS the ODBC driver bundles its own driver manager — **install only the vendor's ODBC driver; unixODBC is not required**.
> **드라이버 자체 완결(10-25).** Db2 드라이버는 DRDA를 직접 말하므로 IBM 클라이언트가 필요 없습니다(아래 clidriver 절차는 Windows ODBC 폴백용). Linux/macOS의 ODBC 드라이버는 드라이버 관리자를 함께 담고 있어 **벤더 ODBC 드라이버만 설치하면 되고 unixODBC는 필요 없습니다.**

## IBM Db2 — native driver (recommended)

Nexa SQL connects to IBM Db2 (LUW) with its own DRDA driver — no IBM client, no ODBC. The driver package (`db2`, drivers-v0.1.4 or later) downloads and installs itself the first time you connect with `db2://user:password@host:50000/DATABASE` (or choose **Db2** in the login window); you can also run `nsql driver install db2`. Offline: `nsql driver install --file nsql-drivers-all-<ver>-<os>-<arch>.zip`. Encrypted login (SECMEC 9) is used when the server offers it; for production use TLS (`?tls=required`).

## IBM Db2 — 네이티브 드라이버(권장)

Nexa SQL은 자체 DRDA 드라이버로 Db2(LUW)에 접속합니다 — IBM 클라이언트·ODBC가 필요 없습니다. `db2://사용자:비밀번호@호스트:50000/DB`로 처음 접속하면(또는 접속 창 DB 종류 **Db2**) 드라이버 패키지(`db2` · drivers-v0.1.4 이상)를 자동으로 내려받아 설치합니다(`nsql driver install db2`로도 설치). 폐쇄망은 `nsql driver install --file nsql-drivers-all-<버전>-<os>-<arch>.zip`. 서버가 지원하면 암호화 로그인(SECMEC 9)을 쓰며, 운영 환경은 TLS(`?tls=required`)를 권장합니다.

## IBM Db2 — install the IBM Data Server Driver (clidriver) (ODBC fallback · `?driver=odbc`) (English)

Only needed when you explicitly choose the ODBC route with `db2://…?driver=odbc` (mainly Windows). The built-in ODBC adapter then uses a dedicated Db2 dialect. *(On Linux/macOS the standalone clidriver ships only a 32-bit-SQLLEN `libdb2.so`, which does not work with the ODBC driver manager — use the native driver above.)* The IBM client library is **not bundled** — install it once per PC. It is a free, anonymous download (no login): <https://public.dhe.ibm.com/ibmdl/export/pub/software/data/db2/drivers/odbc_cli/>

| OS | File (v12.1.4) | Notes |
|---|---|---|
| Linux x86-64 | `linuxx64_odbc_cli.tar.gz` (≈25 MB) | v12.1 needs glibc 2.34+ and an x86-64-v2 CPU |
| macOS Apple silicon | `macarm64_odbc_cli.tar.gz` (≈18 MB) | supported from v12.1 |
| macOS Intel | `macos64_odbc_cli.tar.gz` (≈21 MB) | Intel Macs are supported up to v11.5.9 |
| Windows x64 | `ntx64_odbc_cli.zip` (≈26 MB) | register with `db2cli install -setup` (administrator) |

Linux / macOS — unpack and register the driver name `IBM DB2 ODBC DRIVER` in `odbcinst.ini`:

```sh
sudo mkdir -p /opt/ibm && cd /opt/ibm
sudo tar xzf ~/Downloads/linuxx64_odbc_cli.tar.gz          # → /opt/ibm/clidriver
printf '[IBM DB2 ODBC DRIVER]\nDriver=/opt/ibm/clidriver/lib/libdb2.so\nFileUsage=1\nDontDLClose=1\n' | sudo tee -a /etc/odbcinst.ini
```

(The ODBC driver library is `lib/libdb2.so` — v12.1.4 has no `libdb2o.so` (checked in CI 10-25). macOS: `lib/libdb2.dylib` (not verified yet).)

Then connect with `db2://user:password@host:50000/DATABASE?driver=odbc`. If the driver is missing, the error lists the ODBC drivers that are installed. To use a different registered name, add `?driver=<name>`.

## IBM Db2 — clidriver 설치(ODBC 폴백 · `?driver=odbc`)(한국어)

`db2://…?driver=odbc`로 ODBC 경로를 명시할 때만 필요합니다(주로 Windows). 이때 내장 ODBC 어댑터가 Db2 방언으로 접속합니다. Linux/macOS의 clidriver는 32비트 SQLLEN `libdb2.so`뿐이라 ODBC 드라이버 관리자와 맞지 않으니 위 네이티브 드라이버를 쓰세요. IBM 클라이언트 라이브러리는 **동봉하지 않으므로** PC마다 한 번 설치합니다(무료 · 로그인 없이 위 주소에서 받음).

1. OS에 맞는 파일(위 표)을 받아 `/opt/ibm`(Windows는 원하는 폴더)에 풉니다.
2. 드라이버 이름 `IBM DB2 ODBC DRIVER`를 등록합니다 — Linux/macOS = `odbcinst.ini`에 `Driver=<clidriver>/lib/libdb2.so`(v12.1.4에는 `libdb2o.so` 없음 · CI 10-25 확인 · macOS `.dylib`는 미확인) · Windows = 관리자 권한으로 `db2cli install -setup`.
3. 접속 = `db2://사용자:비밀번호@호스트:50000/DB?driver=odbc`. 드라이버가 없으면 오류 메시지에 설치된 ODBC 드라이버 목록이 함께 나옵니다. 다른 이름으로 등록했다면 `?driver=<이름>`.

자세한 근거(버전 · 크기 · 재배포 조건 · 연결 키워드) = 저장소 `docs/121a-db2-research.md` ② · 네이티브 드라이버 = `docs/121b-db2-drda-research.md`.
