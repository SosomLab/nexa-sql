#!/usr/bin/env python3
"""update-driver-index.py — 릴리스된 자산 목록으로 drivers/index.json의 `latest`·`platforms`를 갱신(D-350 · drivers.yml publish).

    python3 scripts/update-driver-index.py --version 0.1.0 --assets-dir staged [--index drivers/index.json]

자산 이름 `nsql-driver-<id>-<ver>-<os>-<arch>.zip`에서 id·플랫폼을 읽어 드라이버마다 { id · name · dialect · latest · platforms · tag_prefix }를
넣거나 갱신한다(tag_prefix = "drivers-v" · 한 태그에 전부). 없는 id는 그대로 둔다.
"""
import argparse, io, json, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from package_drivers_table import DRIVERS  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--assets-dir", required=True)
    ap.add_argument("--index", default=os.path.join(ROOT, "drivers", "index.json"))
    a = ap.parse_args()
    pat = re.compile(r"^nsql-driver-(?P<id>.+)-" + re.escape(a.version) + r"-(?P<plat>(windows|macos|linux)-(x86_64|aarch64))\.zip$")
    plats = {}
    for f in os.listdir(a.assets_dir):
        m = pat.match(f)
        if m:
            plats.setdefault(m.group("id"), set()).add(m.group("plat"))
    if not plats:
        sys.exit("no assets matched")
    doc = json.load(io.open(a.index, encoding="utf-8"))
    drivers = doc.setdefault("drivers", [])
    by_id = {d.get("id"): d for d in drivers}
    for did, _crate, dialect, name in DRIVERS:
        if did not in plats:
            continue
        d = by_id.get(did)
        if d is None:
            d = {"id": did}
            drivers.append(d)
            by_id[did] = d
        d.update({"name": name, "dialect": dialect, "latest": a.version, "tag_prefix": "drivers-v",
                  "platforms": sorted(plats[did]), "license": "PolyForm-Noncommercial-1.0.0", "sxs": did in ("oracle-oci", "odbc")})
    io.open(a.index, "w", encoding="utf-8", newline="\n").write(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
    print(f"index: {len(plats)} drivers → latest {a.version}: " + ", ".join(sorted(plats)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
