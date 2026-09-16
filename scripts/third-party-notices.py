#!/usr/bin/env python3
"""THIRD-PARTY-NOTICES 초안 — `cargo metadata`의 라이선스 목록(표준 라이브러리만 · PIL 등 외부 모듈 0).

사용:  python3 scripts/third-party-notices.py <출력 파일> [--manifest-path Cargo.toml]
       (bash 래퍼 = scripts/third-party-notices.sh · PowerShell은 이 파일을 직접 부른다)

무엇을 담나
- 워크스페이스 멤버(nsql-*·nexa-sql)는 제외. 형제 저장소 nexa-ui(path 의존)는 "SosomLab 자체 크레이트" 절에 따로 적는다.
- 나머지 = 제3자 crate: 이름 · 버전 · 라이선스(SPDX 식) · 저장소 URL. 라이선스 전문은 담지 않는다 —
  전문 동봉·금지 라이선스 게이트는 T-10(cargo-deny)에서 잇는다. 이 초안은 "무엇을 썼는가"의 목록이다.
- 드라이버 어댑터의 예외 crate(DR-3)도 여기 그대로 드러난다(원장 docs/10 §3과 대조 가능).
"""
import json
import subprocess
import sys
from collections import OrderedDict


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    out = sys.argv[1]
    extra = sys.argv[2:]
    cmd = ["cargo", "metadata", "--format-version", "1", "--locked"] + extra
    meta = json.loads(subprocess.check_output(cmd))
    members = set(meta["workspace_members"])
    pkgs = {p["id"]: p for p in meta["packages"]}

    # 실제로 링크되는 것만: resolve 그래프에서 워크스페이스 멤버가 닿는 노드(dev-dependency 제외는 하지 않는다 — 초안).
    resolve = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen = set()
    stack = list(members)
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in resolve.get(pid, {}).get("deps", []):
            # dev-dependency만인 간선은 배포 산출물에 링크되지 않는다.
            kinds = {k.get("kind") for k in dep.get("dep_kinds", [])}
            if kinds and kinds <= {"dev"}:
                continue
            stack.append(dep["pkg"])

    own, third = OrderedDict(), OrderedDict()
    for pid in sorted(seen, key=lambda i: (pkgs[i]["name"], pkgs[i]["version"])):
        p = pkgs[pid]
        if pid in members:
            continue
        row = (p["name"], p["version"], p.get("license") or p.get("license_file") or "(미기재)", p.get("repository") or "")
        # path 의존(source == None)이면서 멤버가 아니면 형제 저장소(nexa-ui) 크레이트.
        (own if p.get("source") is None else third)[pid] = row

    lines = [
        "THIRD-PARTY NOTICES — Nexa SQL",
        "",
        "Nexa SQL 자체는 PolyForm Noncommercial 1.0.0(LICENSE.md)이다. 이 파일은 정적으로 링크된 제3자 Rust crate의",
        "목록과 각 라이선스 식별자(SPDX)를 적는다. 목록은 `cargo metadata`(Cargo.lock 고정)에서 생성한다 —",
        "scripts/third-party-notices.py · 라이선스 전문 동봉·정책 게이트는 T-10(cargo-deny).",
        "",
        "== SosomLab 자체 크레이트(형제 저장소 · path 의존) ==",
    ]
    for name, ver, lic, repo in own.values():
        lines.append(f"  {name} {ver}  [{lic}]  {repo}")
    lines += ["", f"== 제3자 crate ({len(third)}개) ==", ""]
    for name, ver, lic, repo in third.values():
        lines.append(f"  {name} {ver}  [{lic}]  {repo}")
    # 라이선스별 집계 — 정책 검토(NC와 양립 · copyleft 유무)를 한눈에.
    tally = OrderedDict()
    for _, _, lic, _ in third.values():
        tally[lic] = tally.get(lic, 0) + 1
    lines += ["", "== 라이선스별 개수 =="]
    for lic, n in sorted(tally.items(), key=lambda kv: -kv[1]):
        lines.append(f"  {n:4d}  {lic}")
    lines.append("")
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines))
    print(f"{out}: 제3자 {len(third)}개 · 자체 {len(own)}개")
    return 0


if __name__ == "__main__":
    sys.exit(main())
