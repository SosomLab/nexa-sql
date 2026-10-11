#!/usr/bin/env bash
# win-vars-e2e.sh — 변수 Scope·층 E2E(10-07 · T-304 · D-264 · docs/63 §11-4) — 서버 없이 SQLite 메모리 DB로 CLI `nsql run`.
#   검사: `VAR x GLOBAL`(선언+층 한 줄) · `VAR A NUMBER = 10 GLOBAL` · 탭(세션) 가림 `VAR A` · 대입은 사는 층에 · `SHOW VARIABLES` Active 열 ·
#        `VAR A DROP`(앞 층 지우면 아래 층 복귀) · `VAR CLEAR` / `VAR CLEAR GLOBAL` / `VAR CLEAR ALL` · 자동 타입 재추론(`Var.auto_ty`) ·
#        엄격 타입 파싱(`VARCHAR2(50 CHAR)`=50 · 잘못된 길이 = 오류) · 글로벌 파일 `vars/global.sql` 보존·전파(두 번째 실행에서 읽힘).
#   공통 예제 `examples/variables/common.sql`는 두 확장 시점(`vars.expand_at` assign/use)으로 두 번 돈다(README).
#
# 사용: scripts/win-vars-e2e.sh -o <출력폴더> [-n target/release/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NSQL="$ROOT/target/release/nsql.exe"; OUT=""
while getopts "o:n:" o; do case $o in o) OUT=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-n cli]"; exit 2; }
[ -x "$NSQL" ] || NSQL="${NSQL%.exe}"
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
printf 'ui.lang=ko\ndemo.prompted=on\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/vars-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk() { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
nchk() { if echo "$3" | grep -qE -- "$2"; then bad "$1 (없어야 함 $2)" "$3"; else ok "$1"; fi; }
show() { echo "$1" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null; }
run() { "$NSQL" run -c sqlite::memory: "$1" "${@:2}" 2>&1; }

say "=== 변수 Scope E2E · $(date '+%F %T') · $NSQL · 격리 홈 $H"

# ── 1. 선언 + 층 한 줄 · 가림 · Active 열 · DROP = 아래 층 복귀 ─────────────────────────────────
F1="$OUT/scope1.sql"
cat > "$F1" <<'SQL'
VAR A NUMBER = 10 GLOBAL
SHOW VARIABLES
VAR A
EXEC :A := 5
SHOW VARIABLES
SELECT :A AS a_tab;
VAR A DROP
SHOW VARIABLES
SELECT :A AS a_global;
EXEC :A := 20
SHOW VARIABLES
VAR A DROP GLOBAL
SHOW VARIABLES
SQL
o=$(run "$F1"); show "$o"
chk "①-a 글로벌 선언 한 줄 = Layer global · Active *" "A +.*NUMBER.*10.*[Gg]lobal.*\*" "$o"
chk "①-b VAR A(타입 없음) = 탭 선언 → 글로벌 가림(탭 줄 Active · 글로벌 줄 Active 아님)" "A +.*[Tt]ab.*\*" "$o"
chk "①-c 탭 A에 대입 5 → 조회 5" "a_tab[^0-9]*5|^ *5 *$" "$o"
chk "①-d VAR A DROP = 탭 층 삭제 → 글로벌 10 복귀" "a_global[^0-9]*10|^ *10 *$" "$o"
chk "①-e 대입은 사는 층에(D-264) = 글로벌 A가 20" "A +.*20.*[Gg]lobal" "$o"
nchk "①-f VAR A DROP GLOBAL 뒤 A 없음" "^ *A +\|" "$o"

# ── 2. CLEAR 셋 ───────────────────────────────────────────────────────────────────────────────
F2="$OUT/scope2.sql"
cat > "$F2" <<'SQL'
VAR G1 NUMBER = 1 GLOBAL
VAR G2 VARCHAR2(10) = 'g' GLOBAL
VAR T1 NUMBER = 7
EXEC :T2 := 'two'
SELECT 'MARK_A' AS m;
SHOW VARIABLES
VAR CLEAR
SELECT 'MARK_B' AS m;
SHOW VARIABLES
VAR T3 NUMBER = 3
VAR CLEAR GLOBAL
SELECT 'MARK_C' AS m;
SHOW VARIABLES
VAR G3 NUMBER = 9 GLOBAL
VAR T4 NUMBER = 4
VAR CLEAR ALL
SELECT 'MARK_D' AS m;
SHOW VARIABLES
SQL
o=$(run "$F2"); show "$o"
seg_a=$(echo "$o" | awk '/MARK_A/{f=1} /MARK_B/{f=0} f'); seg_b=$(echo "$o" | awk '/MARK_B/{f=1} /MARK_C/{f=0} f')
seg_c=$(echo "$o" | awk '/MARK_C/{f=1} /MARK_D/{f=0} f'); seg_d=$(echo "$o" | awk '/MARK_D/{f=1} f')
chk "②-a 선언 넷 보임(G1·G2 global · T1·T2 tab)" "G1 +.*global" "$seg_a"; chk "②-a' T2 자동 타입 tab" "T2 +.*tab +auto" "$seg_a"
nchk "②-b VAR CLEAR = 탭 변수 T1·T2 없음" "^ *T[12] +" "$seg_b"
chk "②-c VAR CLEAR = 글로벌 G1·G2 유지(글로벌 유지 · 사용자 10-07)" "G1 +.*global" "$seg_b"
# `VAR CLEAR GLOBAL` = 탭 층 + 글로벌 층(docs/63 §11-4 "GLOBAL = 글로벌 층도") → T3도 함께 사라진다.
nchk "②-d VAR CLEAR GLOBAL = 글로벌 G1·G2 없음" "^ *G[12] +" "$seg_c"
nchk "②-e VAR CLEAR GLOBAL = 탭 T3도 없음(탭 + 글로벌)" "^ *T3 +" "$seg_c"
chk "②-e' CLEAR GLOBAL 뒤 0 rows" "^ *0 rows" "$seg_c"
nchk "②-f VAR CLEAR ALL = 전부 없음(G3·T4)" "^ *(G3|T4) +" "$seg_d"

# ── 3. 자동 타입 재추론 · 엄격 타입 파싱 ────────────────────────────────────────────────────────
F3="$OUT/scope3.sql"
cat > "$F3" <<'SQL'
EXEC :X := 1
SHOW VARIABLES
EXEC :X := 'Number to String'
SHOW VARIABLES
VAR Y VARCHAR2(50 CHAR) = 'y'
SHOW VARIABLES
VAR Z VARCHAR2(abc)
SHOW VARIABLES
SQL
o=$(run "$F3"); show "$o"
chk "③-a 선언 없는 X = 처음 NUMBER" "X +.*NUMBER" "$o"
chk "③-b 글 대입 뒤 X = 글 타입(자동 재추론 · bin56)" "X +.*(VARCHAR|STRING|TEXT|CHAR)" "$o"
chk "③-c VARCHAR2(50 CHAR) = 길이 50" "Y +.*VARCHAR2?\(50" "$o"
chk "③-d 잘못된 길이 = 오류(4000 폴백 아님)" "VARCHAR2\(abc\)|잘못|invalid|오류|error" "$o"
nchk "③-e Z가 4000으로 만들어지지 않음" "Z +.*4000" "$o"

# ── 4. 글로벌 파일(vars/global.sql · T-307): 설정 `vars.cli_global` 기본 끔 = CLI는 쓰지 않음 · 켜면 실행 시작 때 읽고 끝날 때 쓴다(GUI와 같은 파일).
F4a="$OUT/scope4a.sql"; printf 'VAR KEEP NUMBER = 77 GLOBAL\nSELECT :KEEP AS kept;\n' > "$F4a"
F4b="$OUT/scope4b.sql"; printf 'SHOW VARIABLES\nSELECT :KEEP AS kept;\nVAR KEEP DROP GLOBAL\n' > "$F4b"
o=$(run "$F4a"); show "$o"
chk "④-a 글로벌 선언 = 같은 실행에서 바인드 77" "kept[^0-9]*77|^ *77 *$" "$o"
if [ -f "$H/vars/global.sql" ]; then bad "④-b 기본(vars.cli_global=off) = CLI는 global.sql을 쓰지 않아야 함"; else ok "④-b 기본 off = global.sql 없음"; fi
"$NSQL" config set vars.cli_global on >/dev/null 2>&1
o=$(run "$F4a"); o2=$(run "$F4b"); show "$o2"
chk "④-c vars.cli_global=on → 첫 실행이 global.sql을 씀" "." "$(ls "$H"/vars/global.sql 2>&1)"
chk "④-d 두 번째 실행이 글로벌 KEEP=77을 읽음(SHOW VARIABLES global 층)" "KEEP +.*77.*global" "$o2"
chk "④-e 두 번째 실행 바인드 77" "kept[^0-9]*77|^ *77 *$" "$o2"
if [ -f "$H/vars/global.sql" ]; then bad "④-f DROP GLOBAL 뒤 실행 끝 = 파일 삭제(남길 것 없음)"; else ok "④-f DROP GLOBAL 뒤 실행 끝 = 파일 없음"; fi
"$NSQL" config set vars.cli_global off >/dev/null 2>&1

# ── 5. 공통 예제 두 확장 시점(README) ──────────────────────────────────────────────────────────
for mode in assign use; do
  "$NSQL" config set vars.expand_at "$mode" >/dev/null 2>&1
  o=$(run "$ROOT/examples/variables/common.sql" KOREA); show "$(echo "$o" | tail -6)"
  nchk "⑤ common.sql(expand_at=$mode) 오류 없음" "panicked|ORA-|SQLITE_ERROR|error:" "$o"
done
"$NSQL" config set vars.expand_at assign >/dev/null 2>&1

say "=== 결과: PASS $pass · FAIL $fail → $REPORT"
[ "$fail" -eq 0 ]
