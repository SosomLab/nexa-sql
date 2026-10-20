#!/usr/bin/env bash
# win-find-origin-e2e.sh — ★ 131차 ⑧(사용자 10-20 새벽): 찾기 틀을 **선택 없이** 열면(지난 검색어가 남아 있어도) 캐럿을 옮기지 않고 ·
#   **Esc = 검색(이동)을 한 번도 안 했으면 열 때 위치(`find_origin`)로 복귀 · 검색으로 일치에 갔으면 그 자리 유지(⑩ · 사용자 10-20)** ·
#   **× 버튼 = 그대로 닫기**. 종전 = 지난 검색어가 있으면 Ctrl+F가 바로 다음 일치로 점프(131차 V1 기준값 d4 = 4:7).
#   길 = 격리 홈 · 5줄 SQL(`FROM`은 4행) · `editor.caret:30`(= 4행 2열) → `edit.find` → `ui.key:char:…`로 'FROM' 입력(일치 4:7) →
#   `find.cancel`(Esc 길) → 다시 `editor.caret:30` → `edit.find`(지난 검색어 FROM 남음) → `find.cancel` → 한 번 더 열어 → `find.close`(× 길).
#   판정 = `editor.dump` 머리 `caret=줄:열` 6개. 키 주입 0(`ui.key:`는 앱 안 route) · 실서버 없음.
# 사용: scripts/win-find-origin-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"
while getopts "o:g:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui]"; exit 2; }
mkdir -p "$OUT"
REPORT="$OUT/find-origin-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -4 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1100,700\n' > "$H/settings.conf"
F="$OUT/multi.sql"; printf 'SELECT a,\n       b,\n       c\n  FROM t\n WHERE a = 1;\n' > "$F"
D() { echo "@after:$1:editor.dump:$(w "$OUT/$2.txt")"; }
CMD="open:$(w "$F"),@after:1500:editor.caret:30,$(D 2000 d0),@after:2500:edit.find,$(D 3200 d1)"
CMD="$CMD,@after:3600:ui.key:char:F,@after:3800:ui.key:char:R,@after:4000:ui.key:char:O,@after:4200:ui.key:char:M,$(D 5000 d2)"
CMD="$CMD,@after:5500:find.cancel,$(D 6300 d3)"                                   # Esc 길 = 열 때 위치로 복귀
CMD="$CMD,@after:7000:editor.caret:30,@after:7500:edit.find,$(D 8300 d4),@after:8700:find.cancel,$(D 9400 d5)"   # 지난 검색어 = 열기만 · Esc = 복귀
CMD="$CMD,@after:10000:editor.caret:30,@after:10500:edit.find,@after:11000:ui.key:char:F,@after:11200:ui.key:char:R,@after:11400:ui.key:char:O,@after:11600:ui.key:char:M,@after:12300:find.close,$(D 13000 d6)"  # × 길 = 그대로
CMD="$CMD,@after:13500:editor.caret:30,@after:14000:edit.find,@after:14500:ui.key:enter,@after:15200:find.cancel,$(D 15900 d7)"  # 지난 검색어 열기 → Enter(다음 찾기) → Esc = 그 자리(⑩)
rm -f "$OUT"/d?.txt
NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$CMD" timeout -s KILL 19 "$EXE" > "$OUT/stdout.txt" 2> "$OUT/stderr.txt"
caret() { head -1 "$OUT/$1.txt" 2>/dev/null | sed -n 's/.*caret=\([0-9:]*\).*/\1/p'; }
sel() { head -1 "$OUT/$1.txt" 2>/dev/null | sed -n 's/.*|sel=\([0-9-]*\).*/\1/p'; }
say "find-origin-e2e $(date '+%F %T') · d0=$(caret d0) d1=$(caret d1) d2=$(caret d2) d3=$(caret d3) d4=$(caret d4) d5=$(caret d5) d6=$(caret d6) d7=$(caret d7)"
chk() { local got; got=$(caret "$1"); [ "$got" = "$2" ] && ok "$1 $3 = $2" || bad "$1 $3: $got ≠ $2" "$(head -1 "$OUT/$1.txt" 2>/dev/null)"; }
[ "$(caret d0)" = "4:2" ] || { bad "d0 시작 캐럿 ≠ 4:2(editor.caret:30 · 파일 꼴 확인)"; }
chk d1 4:2 "열기(선택 없음) = 캐럿 그대로"
chk d2 4:7 "'FROM' 입력 = 일치(4행)로 이동"
chk d3 4:7 "검색으로 이동한 뒤 Esc(find.cancel) = 그 자리 유지(⑩)"
chk d4 4:2 "지난 검색어로 다시 열기 = 캐럿 그대로(점프 없음)"
chk d5 4:2 "검색 안 하고 Esc = 열 때 위치로 복귀"
chk d6 4:7 "×(find.close) = 일치 자리 그대로(복귀 없음)"
chk d7 4:7 "지난 검색어 열기 → Enter(다음 찾기) → Esc = 그 자리 유지(⑩)"
# ⑪ 찾기로 이동한 자리는 **찾은 글이 선택된 상태**(FROM = 글자 31~35) · Esc/×로 닫아도 선택 유지.
for d in d2 d3 d6 d7; do [ "$(sel $d)" = "31-35" ] && ok "$d 일치 'FROM' 선택 상태(sel=31-35)" || bad "$d 선택: $(sel $d) ≠ 31-35"; done
[ "$(sel d5)" = "-" ] && ok "d5 검색 안 함 = 선택 없음" || bad "d5 선택: $(sel d5) ≠ -"
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
