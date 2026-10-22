#!/usr/bin/env python3
"""bundle-drivers.py - 서명된 패키지들 → 오프라인 전체 번들 `nsql-drivers-all-<ver>-<os>-<arch>.zip`(D-350 - 폐쇄망).

    python3 scripts/bundle-drivers.py --version 0.1.0 --os windows --arch x86_64 --dir out

번들 = out/ 안의 `nsql-driver-*-<os>-<arch>.zip` + `.sig` + `.sha256`(서명 없는 자산은 오류 - 설치기가 거부하므로) + drivers/index.json 스냅샷 +
README.txt. 설치 = `nsql driver install --file <번들>`(이 OS 자산 전부) - GUI 드라이버 관리자.
"""
import argparse, os, sys, zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--os", required=True)
    ap.add_argument("--arch", required=True)
    ap.add_argument("--dir", required=True)
    a = ap.parse_args()
    suffix = f"-{a.os}-{a.arch}.zip"
    assets = sorted(f for f in os.listdir(a.dir) if f.startswith("nsql-driver-") and f.endswith(suffix))
    if not assets:
        sys.exit(f"no assets for {a.os}-{a.arch} in {a.dir}")
    name = f"nsql-drivers-all-{a.version}-{a.os}-{a.arch}.zip"
    path = os.path.join(a.dir, name)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_STORED) as z:  # 안의 zip은 이미 압축됨
        for asset in assets:
            stem = asset[: -len(".zip")]
            for side in (asset, stem + ".sig", stem + ".sha256"):
                p = os.path.join(a.dir, side)
                if not os.path.isfile(p):
                    sys.exit(f"{side} missing - sign first (nexa-license-tool sign-file)")
                z.write(p, side)
        idx = os.path.join(ROOT, "drivers", "index.json")
        if os.path.isfile(idx):
            z.write(idx, "index.json")
        z.writestr(
            "README.txt",
            f"Nexa SQL extension drivers {a.version} ({a.os}-{a.arch}) - offline bundle\n\n"
            "Install everything for this OS:\n"
            f"  nsql driver install --file {name}\n"
            "or open View > Drivers in Nexa SQL and install from file.\n"
            "Each package is verified (sha256 + Ed25519 signature) before it is installed.\n",
        )
    print(f"{name}  ({os.path.getsize(path) / 1048576:.1f} MB - {len(assets)} packages)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
