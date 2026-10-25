#!/usr/bin/env python3
"""package-drivers.py - 빌드된 드라이버 cdylib → 확장 패키지 zip(D-350 - docs/22 §2-3 - §3).

    python3 scripts/package-drivers.py --version 0.1.0 --target-dir target/release --os windows --arch x86_64 --out out [--ids pg,mysql]

패키지 하나 = `nsql-driver-<id>-<ver>-<os>-<arch>.zip` = { nexa-driver.json(id - version - dialect - protocol=ABI_VERSION - lib) ·
<cdylib> - LICENSE.md }. 서명(.sig)·해시(.sha256)는 `nexa-license-tool sign-file`이 옆에 만든다(drivers.yml).
ABI 버전은 crates/nsql-abi/src/lib.rs의 `ABI_VERSION`을 읽는다(어긋남 방지). 라이브러리가 없는 id는 건너뛴다(10-23부터 odbc도 전 OS · unixODBC).
"""
import argparse, io, json, os, re, sys, zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# (id, crate, dialect, name) - crates/nsql-drivers/src/lib.rs `DRIVER_IDS`·`ext_crate_name`과 같은 표.
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
    ("db2", "nsql-driver-db2", "db2", "IBM Db2"),
]
WINDOWS_ONLY: set = set()  # odbc도 전 OS(Linux/mac = unixODBC 동적 · 10-24)
# 드라이버 옆에 동봉하는 런타임 공유 라이브러리(Windows 이름 기준 · 다른 OS는 lib*.so/.dylib로 바꾼다).
BUNDLED = {"duckdb": ["duckdb.dll"]}


def driver_version():
    src = io.open(os.path.join(ROOT, "crates", "nsql-abi", "src", "lib.rs"), encoding="utf-8").read()
    m = re.search(r'pub const DRIVER_VERSION: &str = "([^"]+)";', src)
    if not m:
        sys.exit("DRIVER_VERSION not found")
    return m.group(1)


def abi_version():
    src = io.open(os.path.join(ROOT, "crates", "nsql-abi", "src", "lib.rs"), encoding="utf-8").read()
    m = re.search(r"pub const ABI_VERSION: u32 = (\d+);", src)
    if not m:
        sys.exit("ABI_VERSION not found")
    return int(m.group(1))


def find_runtime(target_dir, name):
    """target/<profile>/<name> 또는 build/libduckdb-sys-*/out/<name>(download-lib가 푼 자리)."""
    direct = os.path.join(target_dir, name)
    if os.path.isfile(direct):
        return direct
    build = os.path.join(target_dir, "build")
    if os.path.isdir(build):
        for d in sorted(os.listdir(build)):
            if d.startswith("libduckdb-sys-"):
                for root, _dirs, files in os.walk(os.path.join(build, d, "out")):
                    if name in files:
                        return os.path.join(root, name)
    return None


UNIXODBC_NOTICE = """Third-party notices for nsql-driver-odbc (Linux / macOS build)

== Bundled native libraries (statically linked) ==

  unixODBC (Driver Manager)  [LGPL-2.1-or-later]  https://www.unixodbc.org/
    Linked statically into nsql-driver-odbc on Linux and macOS via the odbc-sys crate
    (feature vendored-unix-odbc, crate unix-odbc). Not used on Windows (inbox odbc32.dll).
    Source of the combined work is available at https://github.com/SosomLab/nexa-sql,
    which satisfies LGPL section 6 (relinking). The unixODBC source and license text:
    https://github.com/lurcher/unixODBC/blob/master/COPYING

The driver itself is licensed under PolyForm Noncommercial 1.0.0 (see LICENSE.md).
"""


def lib_name(crate, os_name):
    stem = crate.replace("-", "_")
    if os_name == "windows":
        return f"{stem}.dll"
    if os_name == "macos":
        return f"lib{stem}.dylib"
    return f"lib{stem}.so"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--target-dir", required=True, help="cdylib가 있는 폴더(target/<triple>/release)")
    ap.add_argument("--os", required=True, choices=["windows", "macos", "linux"])
    ap.add_argument("--arch", required=True, choices=["x86_64", "aarch64"])
    ap.add_argument("--out", required=True)
    ap.add_argument("--ids", default="", help="쉼표 구분 - 비면 전부")
    a = ap.parse_args()
    want = {i for i in a.ids.split(",") if i} or None
    abi = abi_version()
    # ★ 버전 원천 하나(nsql-abi DRIVER_VERSION · 내장 = 확장): 태그(--version)와 어긋나면 멈춘다(앱 release.yml의 태그≠Cargo.toml 검사와 같은 뜻).
    dv = driver_version()
    if a.version != dv:
        sys.exit(f"--version {a.version} != crates/nsql-abi DRIVER_VERSION {dv} - bump the constant (or the tag) so built-in and extension drivers carry the same version")
    os.makedirs(a.out, exist_ok=True)
    license_path = os.path.join(ROOT, "LICENSE.md")
    made = []
    for did, crate, dialect, name in DRIVERS:
        if want and did not in want:
            continue
        if did in WINDOWS_ONLY and a.os != "windows":
            continue
        lib = lib_name(crate, a.os)
        src = os.path.join(a.target_dir, lib)
        if not os.path.isfile(src):
            print(f"[skip] {did}: {src} 없음", file=sys.stderr)
            continue
        asset = f"nsql-driver-{did}-{a.version}-{a.os}-{a.arch}.zip"
        manifest = {"id": did, "version": a.version, "dialect": dialect, "protocol": abi, "lib": lib, "name": name}
        path = os.path.join(a.out, asset)
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("nexa-driver.json", json.dumps(manifest, indent=2) + "\n")
            info = zipfile.ZipInfo(lib)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o100755 if a.os != "windows" else 0o100644) << 16
            info.create_system = 3
            with open(src, "rb") as f:
                z.writestr(info, f.read())
            if os.path.isfile(license_path):
                z.write(license_path, "LICENSE.md")
            # ★ D-362(10-25): Linux/macOS odbc cdylib에는 unixODBC(LGPL-2.1-or-later)가 정적 링크돼 있다 → 고지문 동봉(LGPL §6 ·
            #   본문은 scripts/third-party-notices.py의 "Bundled native libraries" 절과 같은 내용 · 영어).
            if did == "odbc" and a.os != "windows":
                z.writestr("THIRD-PARTY-NOTICES.txt", UNIXODBC_NOTICE)
            # 동봉 런타임(DuckDB = download-lib가 받은 공유 라이브러리 · target/<profile>/에 build.rs가 복사해 둔다).
            for extra in BUNDLED.get(did, []):
                e = extra if a.os == "windows" else ("lib" + extra.replace(".dll", ".so" if a.os == "linux" else ".dylib"))
                src_e = find_runtime(a.target_dir, e)
                if not src_e:
                    sys.exit(f"{did}: bundled runtime {e} missing under {a.target_dir}")
                info_e = zipfile.ZipInfo(e)
                info_e.compress_type = zipfile.ZIP_DEFLATED
                info_e.external_attr = (0o100755 if a.os != "windows" else 0o100644) << 16
                info_e.create_system = 3
                with open(src_e, "rb") as f:
                    z.writestr(info_e, f.read())
        made.append(asset)
        print(f"{asset}  ({os.path.getsize(path) / 1048576:.1f} MB - {lib})")
    if not made:
        sys.exit("no driver packaged")
    return 0


if __name__ == "__main__":
    sys.exit(main())
