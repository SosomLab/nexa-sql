# Drivers

**English** | [한국어](Drivers-ko)

> **Installer = Oracle OCI only (10-23 · D-350).** Every other driver (PostgreSQL · SQL Server · MySQL/MariaDB · SQLite · Oracle Thin · DuckDB · ClickHouse · Db2 · ODBC) is an extension that installs automatically on the first connect (`driver.auto_install`, on by default) or with `nsql driver install <id>` — releases `drivers-v<ver>`.

> **Self-contained drivers (10-25).** Each driver package works on its own: the IBM Db2 driver speaks DRDA natively (no IBM client needed — the clidriver steps below are only for the Windows ODBC fallback), and on Linux/macOS the ODBC driver bundles its own driver manager — **install only the vendor's ODBC driver; unixODBC is not required**.

## IBM Db2 — native driver (recommended)

Nexa SQL connects to IBM Db2 (LUW) with its own DRDA driver — no IBM client, no ODBC. The driver package (`db2`, from the latest `drivers-v<ver>` release) downloads and installs itself the first time you connect with `db2://user:password@host:50000/DATABASE` (or choose Database type **Db2** in the Database Login window); you can also run `nsql driver install db2`. Offline: `nsql driver install --file nsql-drivers-all-<ver>-<os>-<arch>.zip`. Encrypted login (SECMEC 9) is used when the server offers it; for production use TLS (`?tls=required`).

## IBM Db2 — install the IBM Data Server Driver (clidriver) (ODBC fallback · `?driver=odbc`)

Only needed when you explicitly choose the ODBC route with `db2://…?driver=odbc` (mainly Windows). The ODBC driver extension then uses a dedicated Db2 dialect. *(On Linux/macOS the standalone clidriver ships only a 32-bit-SQLLEN `libdb2.so`, which does not work with the ODBC driver manager — use the native driver above.)* The IBM client library is **not bundled** — install it once per PC. It is a free, anonymous download (no login): <https://public.dhe.ibm.com/ibmdl/export/pub/software/data/db2/drivers/odbc_cli/>

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

Background details (versions · sizes · redistribution terms · connection keywords) = `docs/121a-db2-research.md` ② in the repository · native driver = `docs/121b-db2-drda-research.md`.
