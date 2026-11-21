#!/usr/bin/env python3
"""todo_check.py — docs/TODO.md 상태 칸 점검(사용자 11-21 "갱신되지 않은 이슈로 재개발·결정 번복이 없게").

상태 칸(표의 마지막 열) 첫머리 = 현재 상태 한 줄(docs/16 §2-10):
    <기호> <YYYY-MM-DD> · 남음: <조각 | 없음> · 근거: <커밋 | journal MM-DD §n | 파일:줄>  ‖  <이력…>
기호 = ✅ 끝남 · 🚧 일부 남음 · ☐ 손대지 않음 · ⏳ 외부·실기 대기 · ⏸ 보류 · ➡ 대체·합침 · ✗ 제외 · ❓ 확인 필요.
ID 칸 표식 = 머리 ✅ ↔ ID `✅` · 머리 ➡/✗ ↔ ID `~~…~~` · 나머지 = 표식 없음.

점검(번호 = 사용자 11-21 지시 B-4):
  a  열린 행(ID 표식 없음)인데 머리가 ✅/➡/✗
  b  닫힌 행(ID ✅)인데 머리 "남음:"이 "없음"으로 시작하지 않음("없음(설명)"은 허용) · ID 취소선인데 머리가 ➡/✗ 아님
  c  열린 행의 번호가 머리 날짜 **뒤** 커밋 메시지에 나옴(갱신 누락 후보 · 경고)
  d  머리 형식 위반(분리 ` ‖ ` 앞 첫머리가 형식에 안 맞음)
  e  🚧 인데 머리 날짜가 N일(기본 30) 넘음(묵은 진행 중 · 경고) — ⏳·⏸ 제외
  f  STATUS 맨 위 시작점의 "사용자 결정 대기" 구간 번호 중 docs/10 결정 기록에 같은 번호가 이미 있는 것(번복 위험 · 경고)
종료 코드 = a·b·d 실패가 있으면 1(c·e·f는 경고만 · `--strict`면 경고도 1).
사용: python scripts/lib/todo_check.py [--root <저장소>] [--stale-days 30] [--strict] [--quiet]
"""
import argparse
import datetime as dt
import re
import subprocess
import sys

SYMS = "✅🚧☐⏳⏸➡✗❓"
HEAD_RE = re.compile(
    r"^(?P<sym>[" + SYMS + r"])\s+(?P<date>\d{4}-\d{2}-\d{2})\s+·\s+남음:\s*(?P<left>.+?)\s+·\s+근거:\s*(?P<ev>.+)$"
)
ID_RE = re.compile(r"\b([TDEUSCOX]-\d+[a-z]?)\b")
SEP = "‖"


def cells(line):
    s = line.strip()
    if not s.startswith("|"):
        return None
    parts = [c.strip() for c in s.strip("|").split("|")]
    return parts if len(parts) >= 3 else None


def id_mark(first):
    f = first.strip()
    if f.startswith("✅"):
        return "done"
    if f.startswith("~~"):
        return "struck"
    return "open"


def head_of(status):
    return status.split(SEP, 1)[0].strip()


def git_log(root):
    try:
        out = subprocess.run(
            ["git", "-C", root, "log", "--all", "--date=short", "--format=%ad%x09%s"],
            capture_output=True, text=True, encoding="utf-8", errors="replace", check=True,
        ).stdout
    except Exception as e:  # git 없음 → c 생략
        print(f"  (c 생략: git log 실패 {e})")
        return []
    rows = []
    for ln in out.splitlines():
        if "\t" in ln:
            d, s = ln.split("\t", 1)
            rows.append((d, s))
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".")
    ap.add_argument("--stale-days", type=int, default=30)
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    a = ap.parse_args()
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:
        pass
    root = a.root
    todo = open(f"{root}/docs/TODO.md", encoding="utf-8").read().splitlines()
    today = dt.date.today()
    found = {k: [] for k in "abcdef"}
    rows = []
    for no, line in enumerate(todo, 1):
        c = cells(line)
        if not c or set(c[0]) <= set("-: "):
            continue
        m = ID_RE.search(c[0])
        if not m:
            continue
        rows.append((no, m.group(1), id_mark(c[0]), head_of(c[-1])))

    logs = git_log(root)
    for no, tid, mark, head in rows:
        hm = HEAD_RE.match(head)
        where = f"TODO.md:{no} {tid}"
        if not hm:
            found["d"].append(f"{where} — 머리 형식 아님: {head[:70]}")
            continue
        sym, date, left = hm.group("sym"), hm.group("date"), hm.group("left").strip()
        if mark == "open" and sym in "✅➡✗":
            found["a"].append(f"{where} — 열린 행인데 머리 {sym} · ID 표식 맞추기")
        if mark == "done" and (sym != "✅" or not left.startswith("없음")):
            found["b"].append(f"{where} — ID ✅인데 머리 {sym} · 남음: {left[:40]}")
        if mark == "struck" and sym not in "➡✗":
            found["b"].append(f"{where} — ID 취소선인데 머리 {sym}")
        if mark == "open" and sym not in "⏸":
            pat = re.compile(r"\b" + re.escape(tid) + r"\b")
            later = [(d, s) for d, s in logs if d > date and pat.search(s)]
            if later:
                d, s = later[0]
                found["c"].append(f"{where} — 머리 {date} 뒤 커밋 {len(later)}건(최근 {d} {s[:50]})")
        if sym == "🚧":
            try:
                age = (today - dt.date.fromisoformat(date)).days
            except ValueError:
                age = 0
            if age > a.stale_days:
                found["e"].append(f"{where} — 🚧 {age}일 묵음")

    # f: STATUS 맨 위 시작점의 결정 대기 구간 × 결정 기록.
    try:
        status = open(f"{root}/docs/STATUS.md", encoding="utf-8").read()
        dec = open(f"{root}/docs/10-decision-record.md", encoding="utf-8").read()
        i = status.find("다음 세션 시작점")
        para = status[i: status.find("\n", i)] if i >= 0 else ""
        j = para.find("사용자 결정 대기")
        seg = para[j: para.find("→", j)] if j >= 0 else ""
        for tid in sorted(set(ID_RE.findall(seg))):
            if tid.startswith("T-") and re.search(r"^\| D-\d+[^\n]*\b" + re.escape(tid) + r"\b", dec, re.M):
                found["f"].append(f"STATUS 결정 대기 {tid} — 10-decision-record에 같은 번호의 D 행 있음(번복 확인)")
    except OSError as e:
        print(f"  (f 생략: {e})")

    names = {
        "a": "열린 행인데 끝남 표기", "b": "닫힌 행인데 남음·기호 불일치", "c": "머리 날짜 뒤 커밋(갱신 누락 후보)",
        "d": "머리 형식 위반", "e": "묵은 🚧", "f": "결정 대기인데 결정 기록 있음",
    }
    fail = sum(len(found[k]) for k in "abd")
    warn = sum(len(found[k]) for k in "cef")
    for k in "abdcef":
        if found[k]:
            print(f"── {k} {names[k]}: {len(found[k])}")
            if not a.quiet:
                for x in found[k]:
                    print("   " + x)
    print(f"★ todo-check: 행 {len(rows)} · 실패 {fail}(a·b·d) · 경고 {warn}(c·e·f)")
    return 1 if fail or (a.strict and warn) else 0


if __name__ == "__main__":
    sys.exit(main())
