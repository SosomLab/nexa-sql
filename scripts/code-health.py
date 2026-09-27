#!/usr/bin/env python3
"""코드 건강 점검(docs/93) — 컴파일러가 못 보는 미사용·중복·크기를 같은 기준으로 다시 잰다.

사용:
  python scripts/code-health.py                 # nexa-sql + ../nexa-ui + ../nexa-license 전부
  python scripts/code-health.py --cov           # + cargo llvm-cov 요약(느림 · 수 분)
  python scripts/code-health.py --baseline target/code-health/baseline.json   # 기준선과 수치 비교
  python scripts/code-health.py --save-baseline # 이번 결과를 기준선으로 저장

산출: target/code-health/report.md · result.json (저장소 `target/` 아래 = 추적 안 됨).
외부 패키지 0(표준 라이브러리만) · Windows/macOS/Linux 동일.

판정은 **후보**다 — 문자열로 조립하는 키(`format!("grid.{x}")`), 매크로·FFI·직렬화로만 닿는 항목은
사람이 한 번 본다. 확인한 예외는 아래 ALLOW_* 표에 이유와 함께 적는다(다음 실행에서 조용해진다).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPOS = {
    "nexa-sql": ROOT,
    "nexa-ui": ROOT.parent / "nexa-ui",
    "nexa-license": ROOT.parent / "nexa-license",
}
OUT = ROOT / "target" / "code-health"

# ── 확인한 예외(이유 필수) ─────────────────────────────────────────────
# 참조 0이어도 남기는 `pub` 항목: "크레이트/이름": 이유
ALLOW_PUB: dict[str, str] = {
    "nexa-ctl/CONTROL": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/LABEL": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/MONO": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/PANEL": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/PANEL_MS": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/PILL": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/begin_undo_group": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/can_soft_redo": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/control_size_mult_from_code": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/draw_builtin": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/editing_popup_open": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/generation": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/image_front": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/is_modified_cell": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/is_picking": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/open_char": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/overflows": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/overlay_color": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/row_height": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/selected_label": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/set_background": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/set_image_front": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/set_metrics": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/set_show_remaining": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/slot_px": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/wants_keys": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/with_choose_icon": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/with_empty_label": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/with_fade_speed": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/with_font": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/with_label": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/with_tone": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-ctl/with_two_line": "공용 UI 라이브러리 API 짝(빌더 with_*·getter/setter·토큰)",
    "nexa-dlg/marked_files": "공용 파일 대화상자 API",
    "nexa-ext-sdk/as_arr": "확장 SDK 공개 API(확장 작성자용 JSON 접근자)",
    "nexa-ext-sdk/as_bool": "확장 SDK 공개 API(확장 작성자용 JSON 접근자)",
    "nexa-ext-sdk/as_f64": "확장 SDK 공개 API(확장 작성자용 JSON 접근자)",
    "nexa-gfx/glyph_cache_len": "공용 그리기 라이브러리 API(글꼴 폴백·진단 카운터)",
    "nexa-gfx/push_fallback_font": "공용 그리기 라이브러리 API(글꼴 폴백·진단 카운터)",
    "nexa-license/LEASE_DOMAIN": "라이선스 프로토콜 상수(서버 요청·리스 — 서버 단계용 · docs/25)",
    "nexa-license/SERVER_REQUEST_PREFIX": "라이선스 프로토콜 상수(서버 요청·리스 — 서버 단계용 · docs/25)",
    "nexa-sys/pool_len": "공용 OS 층 진단 API",
    "nsql-log/detail_mask": "공용 로그 API 짝(set_detail_mask ↔ detail_mask)",
    "nsql-search/is_cancelled": "검색 핸들 API 짝(cancel ↔ is_cancelled)",
    "nsql-settings/is_preset": "성능 모드 판정 API 짝(is_preset)",
}
# 코드에서 문자열로 안 보이지만 쓰이는 설정 키 접두(조립 키 · 설정 창 전용 등)
ALLOW_SETTING_PREFIX: dict[str, str] = {
    "key.": "단축키 = 명령 id로 조립(`keymap.rs` format!(\"key.{id}\"))",
}
# 설정 크레이트 안의 도움 함수가 읽는 키(앱은 그 함수를 부른다): 키: 이유
ALLOW_SETTING_INTERNAL: dict[str, str] = {}
# 의존 이름이 소스에 안 보여도 필요한 것(feature 활성·링크 전용 등): "크레이트/의존": 이유
ALLOW_DEP: dict[str, str] = {}

ITEM_RE = re.compile(
    r"^\s*pub\s+(?:const\s+fn|async\s+fn|unsafe\s+fn|fn|struct|enum|trait|const|static|type|mod)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
WORD_CACHE: dict[str, int] = {}


def rs_files(repo: Path) -> list[Path]:
    out = []
    for p in repo.rglob("*.rs"):
        parts = set(p.relative_to(repo).parts)  # 저장소 기준(워크트리가 `_cmp/` 아래여도 잰다)
        if "target" in parts or ".git" in parts or "_cmp" in parts:
            continue
        out.append(p)
    return sorted(out)


def read(p: Path) -> str:
    try:
        return p.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return p.read_text(encoding="utf-8", errors="replace")


def crate_of(repo: Path, p: Path) -> str:
    rel = p.relative_to(repo).parts
    # 크레이트 = `src`(또는 tests·examples·benches) 바로 위 폴더 — crates/<c>/src · extensions/sdk/<c>/src 모두.
    for k in range(len(rel) - 1, 0, -1):
        if rel[k] in ("src", "tests", "examples", "benches"):
            return rel[k - 1]
    return rel[0]


def strip_line_comment(line: str) -> str:
    i = line.find("//")
    return line if i < 0 else line[:i]


# ── A. 허용 표시 ───────────────────────────────────────────────────────
def check_allow(files: dict[Path, str]) -> list[dict]:
    # rustc 린트만(`clippy::unused_self` 같은 clippy 린트는 코드 스타일 판단이라 대상 아님).
    rx = re.compile(r"#\[(allow|expect)\(([^)]*(?<!::)\b(dead_code|unused[a-z_]*)\b[^)]*)\)\]")
    out = []
    for p, text in files.items():
        for i, line in enumerate(text.splitlines(), 1):
            if rx.search(line):
                out.append({"file": str(p), "line": i, "text": line.strip()[:140]})
    return out


def is_conditional(line: str) -> bool:
    return "cfg_attr" in line


# ── B. 참조 0인 pub ────────────────────────────────────────────────────
def check_unused_pub(repo_files: dict[str, dict[Path, str]]) -> list[dict]:
    all_text = "\n".join(t for fs in repo_files.values() for t in fs.values())
    counts: dict[str, int] = defaultdict(int)
    for w in re.findall(r"[A-Za-z_][A-Za-z0-9_]*", all_text):
        counts[w] += 1
    out = []
    for repo, fs in repo_files.items():
        for p, text in fs.items():
            if "/tests/" in p.as_posix() or p.name in ("main.rs", "build.rs"):
                continue
            if "/examples/" in p.as_posix() or "/benches/" in p.as_posix():
                continue
            for i, line in enumerate(text.splitlines(), 1):
                m = ITEM_RE.match(line)
                if not m:
                    continue
                name = m.group(1)
                key = f"{crate_of(REPOS[repo], p)}/{name}"
                if key in ALLOW_PUB or name in ("new", "default", "main", "tests"):
                    continue
                if counts[name] <= 1:
                    out.append({"repo": repo, "file": str(p), "line": i, "name": name})
    return out


# ── C. 안 쓰는 Msg ─────────────────────────────────────────────────────
def check_unused_msg(fs: dict[Path, str]) -> list[str]:
    lib = next((p for p in fs if p.as_posix().endswith("nsql-i18n/src/lib.rs")), None)
    if not lib:
        return []
    text = fs[lib]
    m = re.search(r"pub enum Msg \{(.*?)\n\}", text, re.S)
    if not m:
        return []
    names = re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\s*,", m.group(1), re.M)
    # 사용 = i18n 밖의 이름 등장 + i18n 안에서 **번역 표 줄 · `Msg::ALL` 배열 · 시험 모듈 밖**의 `Msg::X`.
    # (표·전체 목록·시험에만 있으면 화면·CLI 어디에도 안 나온다 = 미사용.)
    used: set[str] = set()
    for p, t in fs.items():
        if p != lib:
            used.update(re.findall(r"\b([A-Z][A-Za-z0-9_]*)\b", t))
    lines = text.splitlines()
    a0 = next((i for i, l in enumerate(lines) if "pub const ALL: &'static [Msg]" in l), -1)
    a1 = next((i for i in range(max(a0, 0), len(lines)) if lines[i].startswith("    ];")), -1)
    t0 = next((i for i, l in enumerate(lines) if l.startswith("#[cfg(test)]")), len(lines))
    arm = re.compile(r"^\s*(\|\s*)?Msg::\w+(\s*\|\s*Msg::\w+)*\s*=>")
    for i, l in enumerate(lines):
        if (a0 <= i <= a1) or i >= t0 or arm.match(l):
            continue
        used.update(re.findall(r"Msg::([A-Z][A-Za-z0-9_]*)", l))
    return [n for n in names if n not in used]


# ── D. 안 쓰는 설정 키 ──────────────────────────────────────────────────
def check_unused_settings(fs: dict[Path, str]) -> list[str]:
    reg = next((p for p in fs if p.as_posix().endswith("nsql-settings/src/lib.rs")), None)
    if not reg:
        return []
    keys = re.findall(r'^\s*key:\s*"([^"]+)"', fs[reg], re.M)
    corpus = "\n".join(t for p, t in fs.items() if p != reg)
    # 레지스트리 파일 안에서도 키를 읽는 코드(도움 함수)가 있으니 `key:` 줄만 뺀 본문을 더한다.
    reg_body = "\n".join(l for l in fs[reg].splitlines() if not re.match(r'^\s*key:\s*"', l))
    corpus += "\n" + reg_body
    outside = "\n".join(t for p, t in fs.items() if "/nsql-settings/" not in p.as_posix())
    out = []
    for k in keys:
        if any(k.startswith(pre) for pre in ALLOW_SETTING_PREFIX):
            continue
        if f'"{k}"' not in corpus:
            out.append(k)
        elif f'"{k}"' not in outside and k not in ALLOW_SETTING_INTERNAL:
            # 설정 크레이트 안(레지스트리·프리셋 표)에만 이름이 있다 = 앱이 읽지 않을 수 있다(확인 후보).
            out.append(k + "  (nsql-settings 안에서만)")
    return out


# ── E. 안 쓰는 의존 ────────────────────────────────────────────────────
def check_unused_deps(repo: Path) -> list[dict]:
    out = []
    for toml in sorted(repo.glob("crates/*/Cargo.toml")):
        crate = toml.parent.name
        text = read(toml)
        deps = []
        section = None
        for line in text.splitlines():
            s = line.strip()
            if s.startswith("["):
                section = s
                continue
            if section in ("[dependencies]", "[build-dependencies]") or (
                section and section.startswith("[target.") and section.endswith(".dependencies]")
            ):
                m = re.match(r"^([A-Za-z0-9_-]+)\s*(?:\.workspace)?\s*=", s)
                if m:
                    deps.append(m.group(1))
        src = "\n".join(read(p) for p in toml.parent.rglob("*.rs"))
        for d in deps:
            ident = d.replace("-", "_")
            if re.search(rf"\b{re.escape(ident)}\b", src):
                continue
            if f"{crate}/{d}" in ALLOW_DEP:
                continue
            out.append({"crate": crate, "dep": d})
    return out


# ── F. 크기 ────────────────────────────────────────────────────────────
FN_RE = re.compile(r"^(\s*)(?:pub(?:\([a-z]+\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z0-9_]+)")


def fn_lengths(p: Path, text: str) -> list[dict]:
    lines = text.splitlines()
    out = []
    i = 0
    while i < len(lines):
        m = FN_RE.match(lines[i])
        if not m:
            i += 1
            continue
        depth = 0
        started = False
        j = i
        while j < len(lines):
            code = strip_line_comment(lines[j])
            code = re.sub(r'"(?:\\.|[^"\\])*"', '""', code)
            code = re.sub(r"'(?:\\.|[^'\\])'", "''", code)
            depth += code.count("{") - code.count("}")
            if "{" in code:
                started = True
            if started and depth <= 0:
                break
            if not started and code.rstrip().endswith(";"):
                break
            j += 1
        if started:
            out.append({"file": str(p), "line": i + 1, "name": m.group(2), "len": j - i + 1})
        i += 1
    return out


def check_size(repo_files: dict[str, dict[Path, str]]) -> dict:
    files = []
    fns = []
    for fs in repo_files.values():
        for p, t in fs.items():
            files.append({"file": str(p), "lines": t.count("\n") + 1})
            fns.extend(fn_lengths(p, t))
    files.sort(key=lambda x: -x["lines"])
    fns.sort(key=lambda x: -x["len"])
    return {
        "files_over_3000": [f for f in files if f["lines"] > 3000],
        "top_files": files[:25],
        "fns_over_150": len([f for f in fns if f["len"] > 150]),
        "top_fns": fns[:30],
    }


# ── G. 중복 블록 ───────────────────────────────────────────────────────
def check_duplicates(repo_files: dict[str, dict[Path, str]], window: int = 10) -> list[dict]:
    seen: dict[str, list[tuple[str, int]]] = defaultdict(list)
    for fs in repo_files.values():
        for p, t in fs.items():
            if "/tests/" in p.as_posix():
                continue
            norm = []
            for i, line in enumerate(t.splitlines(), 1):
                s = strip_line_comment(line).strip()
                if not s or s in ("{", "}", "};", "},", ")", ");", "),") or s.startswith("#["):
                    continue
                if s.startswith("use ") or s.startswith("assert"):
                    continue
                norm.append((s, i))
            for k in range(0, len(norm) - window + 1):
                chunk = "\n".join(x[0] for x in norm[k : k + window])
                if len(chunk) < 350:
                    continue
                h = hashlib.sha1(chunk.encode()).hexdigest()
                seen[h].append((str(p), norm[k][1]))
    groups = []
    for h, locs in seen.items():
        files = {f for f, _ in locs}
        if len(locs) >= 2:
            groups.append({"count": len(locs), "files": len(files), "at": locs[:6]})
    # 겹치는 창 묶기: 같은 첫 위치 파일에서 연속된 것은 대표 하나만
    groups.sort(key=lambda g: (-g["count"], g["at"][0]))
    dedup = []
    taken: set[tuple[str, int]] = set()
    for g in groups:
        f, l = g["at"][0]
        if any((f, x) in taken for x in range(l - window * 2, l + 1)):
            continue
        taken.add((f, l))
        dedup.append(g)
    return dedup[:40]


# ── H. 커버리지(선택) ──────────────────────────────────────────────────
def run_cov(repo: Path) -> dict:
    cmd = ["cargo", "llvm-cov", "--workspace", "--summary-only", "--json"]
    r = subprocess.run(cmd, cwd=repo, capture_output=True, text=True, encoding="utf-8")
    if r.returncode != 0:
        return {"error": r.stderr[-600:]}
    data = json.loads(r.stdout)
    per_crate: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    for f in data["data"][0]["files"]:
        name = f["filename"].replace("\\", "/")
        m = re.search(r"/crates/([^/]+)/", name)
        if not m:
            continue
        lines = f["summary"]["lines"]
        per_crate[m.group(1)][0] += lines["covered"]
        per_crate[m.group(1)][1] += lines["count"]
    tot = data["data"][0]["totals"]["lines"]
    return {
        "total_pct": round(tot["percent"], 1),
        "crates": {k: round(100 * c / n, 1) if n else 0.0 for k, (c, n) in sorted(per_crate.items())},
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--cov", action="store_true")
    ap.add_argument("--baseline")
    ap.add_argument("--save-baseline", action="store_true")
    a = ap.parse_args()

    repo_files: dict[str, dict[Path, str]] = {}
    for name, repo in REPOS.items():
        if repo.exists():
            repo_files[name] = {p: read(p) for p in rs_files(repo)}
    flat = {p: t for fs in repo_files.values() for p, t in fs.items()}

    allow = check_allow(flat)
    res = {
        "allow_unconditional": [x for x in allow if not is_conditional(x["text"])],
        "allow_conditional": len([x for x in allow if is_conditional(x["text"])]),
        "unused_pub": check_unused_pub(repo_files),
        "unused_msg": check_unused_msg(repo_files.get("nexa-sql", {})),
        "unused_settings": check_unused_settings(repo_files.get("nexa-sql", {})),
        "unused_deps": [
            dict(d, repo=n) for n, r in REPOS.items() if r.exists() for d in check_unused_deps(r)
        ],
        "size": check_size(repo_files),
        "duplicates": check_duplicates(repo_files),
        "lines": {n: sum(t.count("\n") + 1 for t in fs.values()) for n, fs in repo_files.items()},
    }
    if a.cov:
        res["coverage"] = {n: run_cov(r) for n, r in REPOS.items() if r.exists()}

    summary = {
        "lines": res["lines"],
        "allow_unconditional": len(res["allow_unconditional"]),
        "unused_pub": len(res["unused_pub"]),
        "unused_msg": len(res["unused_msg"]),
        "unused_settings": len(res["unused_settings"]),
        "unused_deps": len(res["unused_deps"]),
        "files_over_3000": len(res["size"]["files_over_3000"]),
        "fns_over_150": res["size"]["fns_over_150"],
        "duplicate_groups": len(res["duplicates"]),
    }
    if "coverage" in res:
        summary["coverage_pct"] = {n: c.get("total_pct") for n, c in res["coverage"].items()}
    res["summary"] = summary

    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "result.json").write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    if a.save_baseline:
        (OUT / "baseline.json").write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")

    base = None
    if a.baseline and Path(a.baseline).exists():
        base = json.loads(Path(a.baseline).read_text(encoding="utf-8")).get("summary")

    md = ["# 코드 건강 점검 보고서", "", "기준 = docs/93. 수치는 **후보**(사람이 확인).", "", "| 항목 | 값 | 기준선 |", "|---|---|---|"]
    for k, v in summary.items():
        b = base.get(k) if base else ""
        md.append(f"| {k} | {v} | {b} |")

    def sect(title, rows, fmt):
        md.extend(["", f"## {title} ({len(rows)})", ""])
        md.extend(fmt(r) for r in rows[:200])

    sect("무조건 allow(dead_code/unused)", res["allow_unconditional"], lambda r: f"- {r['file']}:{r['line']} `{r['text']}`")
    sect("참조 0인 pub", res["unused_pub"], lambda r: f"- {r['file']}:{r['line']} `{r['name']}`")
    sect("안 쓰는 Msg", res["unused_msg"], lambda r: f"- `Msg::{r}`")
    sect("안 쓰는 설정 키", res["unused_settings"], lambda r: f"- `{r}`")
    sect("안 쓰는 의존", res["unused_deps"], lambda r: f"- {r['repo']}/{r['crate']}: `{r['dep']}`")
    sect("3000줄 넘는 파일", res["size"]["files_over_3000"], lambda r: f"- {r['file']} — {r['lines']}")
    sect("긴 함수 상위", res["size"]["top_fns"], lambda r: f"- {r['file']}:{r['line']} `{r['name']}` — {r['len']}줄")
    sect("중복 블록(10줄 창)", res["duplicates"], lambda r: f"- ×{r['count']} " + " · ".join(f"{f}:{l}" for f, l in r["at"]))
    if "coverage" in res:
        md.extend(["", "## 커버리지(단위·통합 시험 · 줄 %)", ""])
        for n, c in res["coverage"].items():
            md.append(f"- **{n}** {c.get('total_pct')} %")
            for k, v in c.get("crates", {}).items():
                md.append(f"  - {k}: {v} %")
    (OUT / "report.md").write_text("\n".join(md) + "\n", encoding="utf-8")

    print(json.dumps(summary, ensure_ascii=False, indent=1))
    print(f"report: {OUT / 'report.md'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
