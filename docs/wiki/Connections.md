# Connections

**English** | [한국어](Connections-ko)

## Quick connect — one connection string

1. In the **Connection string (quick connect)** field at the top of the **Database Login** window, enter something like `mysql://user:password@host:3306/DB` and press **[Go]** (or Enter). Oracle `oracle://…`, SQL Server `mssql://…`, PostgreSQL `postgres://…` and a SQLite file path work the same way.
2. A **profile is saved automatically** from what you entered (name = `dialect-host-DB` · if the name already exists, `-2` is appended). Next time, entering just that name is enough.
3. **The password is not saved.** A password in the string is used only for this run; if there is none, the focus moves to the **Password** field so you can type it. To save it, edit the profile, check **Save** next to the password and press [Save].

## Options in the login form

- **No password** (check box to the right of the **User** row) — for profiles that are meant to send no password: Oracle OS authentication, PostgreSQL `trust`, accounts without a password. When it is checked, the app does not ask for a password and sends an empty one. The profile stores `nopassword=on` (not a secret, so nothing is encrypted), and a profile loaded into the form opens with the box checked. For Oracle, leave **User** empty as well to use OS authentication (`/`) — this works with the OCI driver only; the Thin driver reports an error. In the CLI, leaving the password empty in the connection string (`postgres://u:@h:5432/db`) makes `nsql conn add` save `nopassword=on`.
- **Role** (combo box to the right of the **Database type** row · Oracle only) — **Normal** · SYSDBA · SYSOPER. The profile stores `role=SYSDBA` or `role=SYSOPER` (empty = a normal connection). A connection string also accepts the suffix `?role=sysdba` (saved in upper case). The CLI shows it as `… AS SYSDBA`.

## Supported databases

| Database | How | Notes |
|---|---|---|
| Oracle · SQL Server · PostgreSQL · MySQL/MariaDB · SQLite · DuckDB · ODBC | the installer includes only Oracle OCI; all others (Oracle Thin included) are extension drivers installed automatically on first connect (`driver.auto_install`) | see the sections below |
| ClickHouse | extension driver `clickhouse` (HTTP 8123 / HTTPS 8443) | `clickhouse://user:pass@host:8123/db[?tls=required]` · no transactions · grid editing not supported · installed from the latest drivers release |
| Redshift · CockroachDB · YugabyteDB · Greenplum · TimescaleDB | PostgreSQL driver (wire-compatible) | `redshift://` 5439 · `cockroach://` 26257 · `yugabyte://` 5433 · `greenplum://` / `timescale://` 5432 |
| IBM Db2 (LUW) | native DRDA driver (no IBM client · installed automatically on first connect from the latest drivers release · see [Drivers](Drivers.md)) · ODBC fallback with `?driver=odbc` (needs IBM clidriver) | `db2://user:pass@host:50000/DB` · catalog = SYSCAT · Database type **Db2** in the Database Login window |
| Microsoft Access | ODBC adapter + Access dialect (Windows ACE driver · Linux/macOS MDBTools) | `access:C:/path/file.accdb` · Database type **MS Access** |
| SAP HANA · Informix | ODBC preset (connects through the ODBC driver) | `hana://` 30015 (ODBC driver `HDBODBC`) · `informix://` 9088 (`IBM INFORMIX ODBC DRIVER`) · install the vendor ODBC driver first |

- With the PostgreSQL-compatible schemes, writing just the scheme fills in the default port.
- SAP HANA and Informix are not in the **Database type** list of the Database Login window. Either put the scheme in a connection string (`hana://user:pass@host/DB` fills in the ODBC driver name and the default port), or choose **ODBC** and enter an ODBC connection string in the **DSN** field.

## Connecting through ODBC

- With a DSN, use `odbc:DSN=name` (user and password in their own fields). To use a driver directly without a DSN, paste the ODBC connection string as is, e.g. `odbc:Driver={SQL Server};Server=host,1433;Database=DB;Uid=user;Pwd=password` (the password is masked as `***` on screen and in logs).
- **Windows**: the ODBC extension driver is installed automatically on first connect; besides that you only need the **vendor ODBC driver** and a DSN (ODBC Data Source Administrator) · current limitations = stopping a run (■), REF CURSOR not supported.
- **macOS, Linux**: unixODBC and a registered vendor driver are required — see "ODBC on macOS / Linux" below.

### ODBC on macOS / Linux

**1. Prerequisites**
- The ODBC driver ships as an extension driver (`nsql driver install odbc`). Builds that include ODBC built in are developer builds only; the macOS installer (lite) uses the extension. An Intel-Mac (`macos-x86_64`) ODBC package is not published yet (T-401) — Apple Silicon and Linux packages exist.
- unixODBC must be installed: macOS `brew install unixodbc` · Linux `sudo apt-get install unixodbc` (or `unixodbc-dev` to build). `odbcinst -j` shows which `odbcinst.ini` / `odbc.ini` files are used.

**2. Install and register a vendor driver** (SQL Server example)
```
brew tap microsoft/mssql-release https://github.com/Microsoft/homebrew-mssql-release
HOMEBREW_ACCEPT_EULA=Y brew install msodbcsql18 mssql-tools18
odbcinst -q -d          # must list: [ODBC Driver 18 for SQL Server]
```
Homebrew prefix: Intel = `/usr/local` · Apple Silicon = `/opt/homebrew`.

**3. Check the connection outside the app first**
```
isql -v -k "Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes"
```

**4. Enter it in Nexa SQL**
- **Database Login** window, **Database type** = ODBC, **DSN** field = `Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;TrustServerCertificate=yes` (user and password in their own fields), or
- Quick connect: `odbc:Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes`, or
- CLI: `nsql conn add MyMssql "odbc:Driver={ODBC Driver 18 for SQL Server};Server=host,1433;Database=db;UID=u;PWD=p;TrustServerCertificate=yes"`
- The driver name must match `odbcinst -q -d` exactly (`{SQL Server}` is the Windows name and does not exist on macOS/Linux).

**5. DSN instead of a connection string** — `~/.odbc.ini`:
```
[MyMssql]
Driver = ODBC Driver 18 for SQL Server
Server = host,1433
Database = db
TrustServerCertificate = yes
```
then connect with `odbc:DSN=MyMssql` (user and password in their fields).

**6. Other drivers** — install, then make sure the name is registered (`odbcinst -q -d`); if not, add a section to `odbcinst.ini`:
```
[PostgreSQL Unicode]
Driver = /usr/local/lib/psqlodbcw.so      # brew install psqlodbc (Apple Silicon: /opt/homebrew/lib)
```
- MariaDB/MySQL = MariaDB Connector/ODBC · Oracle = Instant Client ODBC package (`libsqora`) · every vendor uses the same `[Name]` + `Driver = <path to the library>` form.

**Transactions** — Nexa SQL keeps the ODBC driver in autocommit mode by default. In manual-commit mode the first statement starts a transaction (autocommit off) and COMMIT or ROLLBACK ends it and turns autocommit back on.

**7. Error guide**

| Message | Meaning | Fix |
|---|---|---|
| `Can't open lib '…' : file not found` / IM002 `Data source name not found` (Nexa SQL adds "Installed ODBC drivers: …", shown as **Driver or DSN not available**) | The driver or DSN name is not registered | Install the driver · use the exact name from `odbcinst -q -d` |
| Same error although the driver is listed | Architecture mismatch (e.g. an arm64 library in an Intel app) | `file <driver library>` must match the app (x86_64 / arm64) |
| `Login timeout expired` | Network (host, port, VPN, firewall) | Check reachability (`nc -z host 1433`) |
| certificate / SSL error | Driver 18 encrypts by default | Add `TrustServerCertificate=yes` (or install the CA) |
| `Login failed for user` | Account / password | Check credentials and the default database |
| `SSL Provider: OpenSSL library could not be loaded` (macOS · Homebrew) | `msodbcsql18` pulls in **openssl@4**, so `/usr/local/opt/openssl` points to 4.x while the driver looks for `libssl.3.dylib` there | Link `libssl.3.dylib` and `libcrypto.3.dylib` inside the openssl@4 `lib` folder to openssl@3 (`scripts/mac-odbc-setup.sh` does this), or start the app with `DYLD_LIBRARY_PATH=$(brew --prefix openssl@3)/lib` |

Scripts (developer checkout): `scripts/mac-odbc-setup.sh` installs unixODBC, ODBC Driver 18 for SQL Server and SQLite3 ODBC and applies the OpenSSL fix · `scripts/mac-odbc-rollback.sh` undoes it in reverse order (dry run by default, `--yes` to apply, `--with-unixodbc` to remove unixODBC too).
