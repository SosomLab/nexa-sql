"""드라이버 표 — package-drivers.py · update-driver-index.py 공용(crates/nsql-drivers/src/lib.rs `DRIVER_IDS`·`ext_crate_name`과 같다)."""
DRIVERS = [
    ("sqlite", "nsql-driver-sqlite", "sqlite", "SQLite"),
    ("pg", "nsql-driver-pg", "postgres", "PostgreSQL"),
    ("mysql", "nsql-driver-mysql", "mysql", "MySQL / MariaDB"),
    ("mssql", "nsql-driver-mssql", "mssql", "SQL Server"),
    ("oracle-thin", "nsql-driver-oracle-thin", "oracle", "Oracle (Thin)"),
    ("oracle-oci", "nsql-driver-oracle", "oracle", "Oracle (OCI, Instant Client)"),
    ("odbc", "nsql-driver-odbc", "odbc", "ODBC"),
    ("duckdb", "nsql-driver-duckdb", "duckdb", "DuckDB"),
    ("clickhouse", "nsql-driver-clickhouse", "clickhouse", "ClickHouse"),
]
