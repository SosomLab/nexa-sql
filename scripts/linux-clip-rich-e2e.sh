#!/usr/bin/env bash
# Linux 클립보드 서식 복사 E2E(10-24 · 138차 · T-60 Linux): X11 소유권에 UTF8_STRING + `text/html`을 함께 올리는지 **다른 프로세스**(python3 Gtk)가 읽어 판정.
#   ★ 사용자의 CLIPBOARD를 덮어쓰지 않는다 — Debug 빌드의 `NSQL_CLIP_SELECTION=<이름>`(사설 selection · clipboard_x11.rs atoms())로 올리고 같은 이름을 읽는다
#   (CLAUDE.md §3 "자동 시험에 복사 명령 금지"를 지키는 길). 쓰는 쪽 = `cargo test … write_rich_and_hold_for_another_process -- --ignored`(NSQL_CLIP_HOLD_MS 동안 유지).
#   판정 = ① TARGETS에 text/html ② UTF8_STRING 본문 = 글자 ③ text/html 본문 = HTML(한글 포함) ④ 순수 시험 payload_picks_bytes_by_target 통과.
# 사용: scripts/linux-clip-rich-e2e.sh -o <출력폴더> [-C <저장소 루트 · 기본 = 이 스크립트의 저장소>] [-H <유지 ms · 6000>]
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/timeout.sh"
. "$ROOT/scripts/lib/xvfb.sh"   # Linux = Xvfb 가상 디스플레이로 재실행(T-458 · D-435 · NSQL_XVFB=0이면 실제 화면)
OUT=""; REPO="$ROOT"; HOLD=6000
while [ $# -gt 0 ]; do case "$1" in -o) OUT=$2; shift 2;; -C) REPO=$(cd "$2" && pwd); shift 2;; -H) HOLD=$2; shift 2;; *) echo "usage: $0 -o <out> [-C repo] [-H hold_ms]"; exit 2;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir>"; exit 2; }
mkdir -p "$OUT"; REPORT="$OUT/clip-rich-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; skip=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && printf '%s\n' "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
skp() { say "  SKIP  $1"; skip=$((skip+1)); }
export DISPLAY="${DISPLAY:-:0}"
# ★ GNOME Wayland 세션에서 python Gtk는 기본이 Wayland 백엔드 → X11 selection을 못 본다(10-24 첫 실행 NOOWNER) → 판독기는 X11 고정.
export GDK_BACKEND=x11
python3 -c "import gi; gi.require_version('Gtk','3.0'); from gi.repository import Gtk" 2>/dev/null || { skp "python3-gi(Gtk 3) 없음 — 상대 프로세스 불가"; say "결과: PASS 0 · FAIL 0 · SKIP 1"; exit 0; }
SEL="NEXA_SQL_CLIP_E2E_$$"
TEXT=$'id\tname\n1\t한글 서식'
HTML='<table><tr><td>1</td><td>한글 서식</td></tr></table>'

say "④ 순수 시험(대상별 바이트 선택)"
o=$(cd "$REPO" && timeout -s KILL 900 cargo test -q -p nexa-sql clipboard_x11::tests::payload_picks_bytes_by_target 2>&1); rc=$?
printf '%s\n' "$o" > "$OUT/unit.txt"
[ $rc = 0 ] && printf '%s' "$o" | grep -qE 'test result: ok\. 1 passed' && ok "payload_picks_bytes_by_target" || bad "순수 시험 rc=$rc" "$(tail -8 "$OUT/unit.txt")"

say "①~③ 사설 selection $SEL 왕복(쓰기 = ignored 시험 · 읽기 = python3 Gtk)"
( cd "$REPO" && NSQL_CLIP_SELECTION="$SEL" NSQL_CLIP_TEXT="$TEXT" NSQL_CLIP_HTML="$HTML" NSQL_CLIP_HOLD_MS="$HOLD" \
    timeout -s KILL $(( HOLD / 1000 + 900 )) cargo test -q -p nexa-sql clipboard_x11::tests::write_rich_and_hold_for_another_process -- --ignored > "$OUT/writer.txt" 2>&1 ) &
WPID=$!
# 소유권이 잡힐 때까지(시험 바이너리 빌드 포함) 기다린다 — 소유자가 생기면 읽는다.
cat > "$OUT/reader.py" <<'PY'
import sys, gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, Gdk
sel = Gdk.Atom.intern(sys.argv[1], False)
cb = Gtk.Clipboard.get(sel)
ok, targets = cb.wait_for_targets()
if not ok or not targets:
    print("NOOWNER"); sys.exit(3)
names = [t.name() for t in targets]
print("TARGETS=" + ",".join(names))
txt = cb.wait_for_text()
print("TEXT=" + (txt if txt is not None else "<none>"))
sd = cb.wait_for_contents(Gdk.Atom.intern("text/html", False))
print("HTML=" + (sd.get_data().decode("utf-8", "replace") if sd is not None else "<none>"))
PY
got=""; for i in $(seq 1 180); do
  got=$(timeout -s KILL 20 python3 "$OUT/reader.py" "$SEL" 2>>"$OUT/reader.err"); rc=$?
  case $rc in 0) break;; esac
  kill -0 $WPID 2>/dev/null || { got="WRITER_EXITED"; break; }
  sleep 5
done
printf '%s\n' "$got" > "$OUT/reader.txt"
if [ "$got" = "WRITER_EXITED" ] || [ -z "$got" ]; then bad "소유자 없음(쓰기 시험이 끝났거나 빌드 실패)" "$(tail -6 "$OUT/writer.txt")"; else
  printf '%s' "$got" | grep -qE '^TARGETS=.*text/html' && ok "① TARGETS에 text/html($(printf '%s' "$got" | sed -nE 's/^TARGETS=(.*)/\1/p'))" || bad "① TARGETS에 text/html 없음" "$got"
  [ "$(printf '%s\n' "$got" | sed -nE 's/^TEXT=(.*)/\1/p')" = "id	name" ] && ok "② UTF8_STRING 첫 줄 = 글자" || { printf '%s' "$got" | grep -q 'TEXT=id' && ok "② UTF8_STRING = 글자(여러 줄)" || bad "② UTF8_STRING 본문" "$got"; }
  printf '%s' "$got" | grep -qF "HTML=$HTML" && ok "③ text/html 본문 = HTML(한글 포함)" || bad "③ text/html 본문" "$got"
fi
wait $WPID 2>/dev/null
grep -qE 'test result: ok' "$OUT/writer.txt" && ok "쓰기 시험 종료 = ok" || bad "쓰기 시험 결과" "$(tail -5 "$OUT/writer.txt")"
say "결과: PASS $pass · FAIL $fail · SKIP $skip · 보고 = $REPORT"
[ $fail = 0 ]
