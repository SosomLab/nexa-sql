#!/usr/bin/env bash
# pkg-overload-e2e.sh — Oracle 패키지 오버로드 멤버 E2E(10-10 118차 · 사용자 "PRC_RUN_ALL 둘이 같은 인자로 보인다" 수정분 · 실서버 읽기만 · 키 주입 0).
#   ① 멤버 행 흐린 글 `#1`/`#2` ② 각 멤버 Arguments = 그 오버로드 인자만(인자에 ` #n` 꼬리 없음) ③ 멤버 우클릭 ▸ Generate SQL ▸ Call = 그 오버로드의
#   인자 ④ 패키지 Call = 오버로드마다 블록(`#1`·`#2`) ⑤ 덤으로 `net.changed`(L0 신호 흉내) → 로그 "Network path changed".
#   방법 = 격리 홈(복사 프로필) · 기동 명령 `explorer.expand:<라벨|행>` · `explorer.dump` · `explorer.menu:<행>` + `explorer.pick:gen:call` · `sqlprev.dump`.
#   두 번 띄운다: 1차 = 라벨로 펼쳐 멤버 행 번호를 읽고 · 2차 = 행 번호로 Arguments·메뉴(같은 이름 둘은 라벨로 못 가른다).
# 사용: scripts/pkg-overload-e2e.sh -o <출력> -P <실제 설정 폴더> [-p BISCM] [-s BISCM_SB] [-k PKG_STAT_GATHER_BSY] [-m PRC_RUN_ALL] [-e target/debug/nexa-sql]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/debug/nexa-sql"; OUT=""; PROF=""; PRO="BISCM"; SCH="BISCM_SB"; PKG="PKG_STAT_GATHER_BSY"; MEM="PRC_RUN_ALL"
while getopts "o:P:p:s:k:m:e:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; p) PRO=$OPTARG;; s) SCH=$OPTARG;; k) PKG=$OPTARG;; m) MEM=$OPTARG;; e) APP=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out> -P <settings dir> [-p 프로필] [-s 스키마] [-k 패키지] [-m 멤버] [-e app]"; exit 2; }
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) [ -x "$APP" ] || APP="$APP.exe";; esac
mkdir -p "$OUT"; H="$OUT/home"; O="$OUT/out"; rm -rf "$H" "$O"; mkdir -p "$H" "$O"
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.show_system_schemas=off\n' > "$H/settings.conf"
cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
REPORT="$OUT/pkg-overload-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -10 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk()  { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
nchk() { if echo "$3" | grep -qE -- "$2"; then bad "$1 (없어야 함 $2)" "$3"; else ok "$1"; fi; }
run_gui() { # <초> <기동 명령>
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$2" "$APP" "$PRO" >/dev/null 2>>"$O/stderr.txt" &
  local p=$!; sleep "$1"; kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
}
# 덤프 행 = `depth|tag|label|sub|state|size` · 행 번호 = 0부터(보이는 행).
row_of() { grep -n -- "$2" "$1" | head -"${3:-1}" | tail -1 | cut -d: -f1 | awk '{print $1-1}'; }

say "=== 패키지 오버로드 E2E · $(date '+%F %T') · $PRO $SCH.$PKG.$MEM · $APP"
# ── 1차: 라벨로 펼쳐 멤버 행 번호 읽기 + net.changed 흉내
# 현재 스키마(BISCM)가 자동 펼침이라 라벨 `Packages`·`Procedures`는 그쪽에 먼저 걸린다 → 탐색기 필터로 패키지 하나만 보이게 한 뒤 라벨로 펼친다.
#   필터는 일치 개수만 보이고 자동 펼침은 꺼져 있다(`explorer.filter_expand` 기본 off) → 필터 안에서는 라벨이 유일하니 차례로 펼친다.
PRE="@after:4000:explorer.filter:$PKG,@after:6500:explorer.expand:$SCH,@after:8000:explorer.expand:Packages,@after:9500:explorer.expand:$PKG,@after:11000:explorer.expand:Procedures"
run_gui 20 "$PRE,@after:13000:explorer.dump:$O/p1.txt,@after:13500:net.changed,@after:14300:log.dump:$O/log1.txt"
P1=$(cat "$O/p1.txt" 2>/dev/null)
chk "1차: 멤버 #1 행(흐린 글 #1)" "item:Procedure\|$MEM\|#1\|" "$P1"
chk "1차: 멤버 #2 행(흐린 글 #2)" "item:Procedure\|$MEM\|#2\|" "$P1"
chk "net.changed → 로그 'Network path changed'" "Network path changed|네트워크 경로 변경" "$(cat "$O/log1.txt" 2>/dev/null)"
R1=$(row_of "$O/p1.txt" "item:Procedure|$MEM|#1|"); R2=$(row_of "$O/p1.txt" "item:Procedure|$MEM|#2|"); RP=$(row_of "$O/p1.txt" "|object|$PKG|")
say "    행 번호: 패키지=$RP · #1=$R1 · #2=$R2"
if [ -z "$R1" ] || [ -z "$R2" ]; then bad "멤버 행을 못 찾아 2차 생략"; say "=== 결과: PASS $pass · FAIL $fail"; exit 1; fi
# ── 2차: 행 번호로(뒤 행부터 펼쳐 앞 행 번호를 보존) Arguments 펼치고 덤프 · #2 멤버 Call · 패키지 Call
A2=$((R2+1)); A1=$((R1+1))
# #2 메뉴는 #1을 펼치기 **전에**(펼치면 #2 행 번호가 밀린다) · 패키지 행(위쪽)은 영향 없음.
run_gui 36 "$PRE,@after:13000:explorer.expand:$R2,@after:14000:explorer.expand:$A2,@after:15500:explorer.menu:$R2,@after:16100:explorer.pick:gen:call,@after:19000:sqlprev.dump:$O/call2.txt,@after:19500:explorer.expand:$R1,@after:20500:explorer.expand:$A1,@after:22000:explorer.dump:$O/p2.txt,@after:22500:explorer.menu:$RP,@after:23100:explorer.pick:gen:call,@after:27000:sqlprev.dump:$O/callpkg.txt"
P2=$(cat "$O/p2.txt" 2>/dev/null)
# #1 멤버 아래 인자 블록 = #1 행 다음부터 #2 행 전까지 · #2 블록 = #2 행 다음부터 다음 item:Procedure 행 전까지.
blk() { awk -v s="$1" -v e="$2" 'NR>s && NR<e' "$O/p2.txt"; }
L1=$(grep -n -- "item:Procedure|$MEM|#1|" "$O/p2.txt" | head -1 | cut -d: -f1); L2=$(grep -n -- "item:Procedure|$MEM|#2|" "$O/p2.txt" | head -1 | cut -d: -f1)
LN=$(awk -v s="$L2" 'NR>s && /item:Procedure\|/ {print NR; exit}' "$O/p2.txt"); [ -n "$LN" ] || LN=$(wc -l < "$O/p2.txt")
B1=$(blk "$L1" "$L2"); B2=$(blk "$L2" "$LN")
chk "#1 Arguments = P_SCOPE" "item:Argument\|P_SCOPE\|IN VARCHAR2" "$B1"
chk "#1 Arguments = P_DEGREE" "item:Argument\|P_DEGREE\|IN NUMBER" "$B1"
nchk "#1 Arguments에 P_OWNER 없음" "P_OWNER" "$B1"
chk "#2 Arguments = P_OWNER" "item:Argument\|P_OWNER\|IN VARCHAR2" "$B2"
chk "#2 Arguments = P_TABLE_LIST" "item:Argument\|P_TABLE_LIST\|IN VARCHAR2" "$B2"
nchk "#2 Arguments에 P_SCOPE 없음" "P_SCOPE" "$B2"
nchk "인자 흐린 글에 ' #2' 꼬리 없음" "IN [A-Z0-9 ]+ #[0-9]" "$B1$B2"
C2=$(cat "$O/call2.txt" 2>/dev/null); CP=$(cat "$O/callpkg.txt" 2>/dev/null)
chk "#2 멤버 Call = P_OWNER 인자" "P_OWNER *=> *:p_owner" "$C2"
nchk "#2 멤버 Call에 P_SCOPE 없음" "P_SCOPE" "$C2"
chk "패키지 Call = $MEM #1 블록" "$MEM #1" "$CP"
chk "패키지 Call = $MEM #2 블록" "$MEM #2" "$CP"
say "=== 결과: PASS $pass · FAIL $fail"
[ "$fail" = 0 ]
