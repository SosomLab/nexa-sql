#!/usr/bin/env bash
# linux-inventory.sh — 성능 전수 A단계 **인벤토리**(docs/71 §3-A · win-inventory.ps1 이식 · 사용자 09-26).
#   D1 최종 용량(ELF 섹션별) · D2 정적 crate 수 · D3 동적 라이브러리(ELF NEEDED + 실행 중 실제 로드) · D4 구성 파일 · 설정 키 수.
#   외부 도구 없이 `readelf`/`size`(binutils)와 /proc 만 쓴다. Release 바이너리를 잰다.
# 사용: scripts/linux-inventory.sh -o <출력폴더> [-H <격리홈>]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; OUT=""; H=""
while getopts "o:H:" o; do case $o in o) OUT=$OPTARG;; H) H=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out> [-H <home>]"; exit 2; }
mkdir -p "$OUT"; [ -n "$H" ] || H="$OUT/home"; mkdir -p "$H"
R="$OUT/inventory.txt"; : > "$R"
say() { echo "$@" | tee -a "$R"; }
APP="$ROOT/target/release/nexa-sql"; CLI="$ROOT/target/release/nsql"

say "== linux-inventory $(date '+%F %T')  commit=$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null)"

say ""; say "== D1 최종 용량(예산 30 MB · DR-17)"
for f in "$APP" "$CLI"; do
  [ -x "$f" ] || { say "  (없음) $f"; continue; }
  say "$(printf '  %-12s %8.2f MB' "$(basename "$f")" "$(stat -c%s "$f" | awk '{print $1/1048576}')")"
  if command -v readelf >/dev/null; then
    # mawk에는 strtonum이 없다 → 16진 크기를 python3으로 환산(gawk 의존 없음).
    readelf -S "$f" 2>/dev/null | python3 -c "
import sys,re
rows=[]
for ln in sys.stdin:
    m=re.match(r'\s*\[\s*\d+\]\s+(\S+)\s+\S+\s+\S+\s+\S+\s+([0-9a-f]+)', ln)
    if m and m.group(1).startswith('.'): rows.append((m.group(1), int(m.group(2),16)))
for n,sz in sorted(rows,key=lambda r:-r[1])[:6]: print('      %-14s %8.2f MB' % (n, sz/1048576))
" | tee -a "$R"
  fi
done

say ""; say "== D2 정적 crate 수(DR-3 예외 원장)"
for p in nexa-sql nsql-cli; do
  n=$(cd "$ROOT" && cargo tree -p "$p" --edges normal 2>/dev/null | grep -oE '^[ │├└─]*[a-z0-9_-]+ v' | sed 's/[ │├└─]*//; s/ v$//' | sort -u | wc -l)
  ws=$(cd "$ROOT" && cargo tree -p "$p" --edges normal 2>/dev/null | grep -c 'nsql-')
  say "$(printf '  %-12s crate=%-4s (워크스페이스 참조 %s회)' "$p" "$n" "$ws")"
done
say "  외부 crate 직접 의존(워크스페이스 Cargo.toml):"
sed -n '/^\[workspace.dependencies\]/,/^\[/p' "$ROOT/Cargo.toml" | grep -E '^[a-z][a-z0-9_-]* = "' | sed 's/^/      /' | tee -a "$R"

say ""; say "== D3 동적 라이브러리(기동 때 올리는 것)"
for f in "$APP" "$CLI"; do
  [ -x "$f" ] || continue
  n=$(readelf -d "$f" 2>/dev/null | grep -c 'NEEDED')
  say "  $(basename "$f"): NEEDED $n"
  readelf -d "$f" 2>/dev/null | grep 'NEEDED' | sed 's/.*\[\(.*\)\]/      \1/' | tee -a "$R"
done
say "  (실행 중 실제 로드 — 기동 4초 뒤 /proc/<pid>/maps)"
NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 "$APP" >/dev/null 2>&1 & P=$!
sleep 4
if kill -0 $P 2>/dev/null; then
  tot=$(awk '{print $6}' /proc/$P/maps 2>/dev/null | grep -E '\.so' | sort -u | wc -l)
  say "      실제 로드된 .so = $tot"
  awk '{print $6}' /proc/$P/maps 2>/dev/null | grep -E '\.so' | sort -u | sed 's|.*/|      |' | head -40 | tee -a "$R"
fi
kill $P 2>/dev/null; wait $P 2>/dev/null

say ""; say "== D4 구성 파일(격리 홈에 실제로 생긴 것)"
find "$H" -type f 2>/dev/null | while read -r f; do
  printf '      %-52s %8s B\n' "${f#$H/}" "$(stat -c%s "$f")"
done | tee -a "$R"
say "      폴더: $(find "$H" -type d 2>/dev/null | wc -l) · 파일: $(find "$H" -type f 2>/dev/null | wc -l)"

say ""; say "== 설정 키(39 §3 부하원 = 끄는 키가 있는가)"
say "  전체: $(NSQL_HOME=$H "$CLI" config list all 2>/dev/null | grep -cE '^\s*[a-z]+\.')"
say "  perf: $(NSQL_HOME=$H "$CLI" config list perf 2>/dev/null | grep -cE '^\s*[a-z]+\.')"
say ""; say "== done $(date '+%T')"
