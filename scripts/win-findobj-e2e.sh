#!/usr/bin/env bash
# win-findobj-e2e.sh — 객체 찾기 창 자체 시험(docs/113 §7 · 10-15). SQLite 격리 홈 · 키 주입 0 · 기동 명령으로 창을 열고 검색 → 덤프 판정.
#   준비 = 표 2(컬럼에 검색어 · 설명 없음) · 뷰(소스에 검색어) · 트리거(소스에 검색어) · 이름에 검색어가 있는 표.
#   케이스: ① 이름·컬럼·소스 일치가 각각 행으로 ② 대소문자 구분 켬 = 소문자 검색어는 소스 대문자 줄을 못 봄 ③ 낱말 = `customer_id`는 안 걸림
#           ④ pick 0 = 소스 탭 열림(editor.dump에 CREATE …) ⑤ 상한 1 = 상태줄 "상한" 안내.
# 사용: scripts/win-findobj-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
REPORT="$OUT/findobj-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -14 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
mk_home() { # <추가 설정>
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\n%b' "$1" > "$H/settings.conf"
}
mk_home ""
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
cat > "$OUT/mk.sql" <<'EOF'
CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER, amount REAL);
CREATE TABLE customer (id INTEGER PRIMARY KEY, name TEXT);
CREATE VIEW v_big AS
  SELECT o.id, o.amount
  FROM orders o
  WHERE o.customer_id IN (SELECT id FROM CUSTOMER);
CREATE TRIGGER trg_orders AFTER INSERT ON orders BEGIN
  UPDATE customer SET name = name WHERE id = NEW.customer_id;
END;
EOF
NSQL_HOME="$H" "$NSQL" run -c Local --no-prompt "$(m "$OUT/mk.sql")" >/dev/null 2>&1
printf -- '-- doc\n' > "$OUT/doc.sql"
gui() { # <이름> <명령> <초>
  local name=$1 cmd=$2 secs=$3
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL "$secs" "$EXE" Local > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
}
PRE="@connected:open:$(w "$OUT/doc.sql"),@after:2500:findobj.open"
has() { grep -q -- "$2" "$OUT/$1" 2>/dev/null; }

say "findobj-e2e $(date '+%F %T')"
# 워밍업: 갓 링크된 exe의 첫 기동(Defender 검사 · 글꼴 캐시)이 느려 첫 케이스가 시간 안에 못 돈다(10-15) → 빈 기동 1회.
gui warm "@after:1500:findobj.open" 4
# ① 기본 검색 "customer": 이름(customer 표) · 컬럼(orders.customer_id) · 소스(v_big · trg_orders)
gui a "$PRE,@after:4500:findobj.query:customer,@after:8000:findobj.dump:$(w "$OUT/a.dump")" 10
if [ -f "$OUT/a.dump" ]; then
  head -1 "$OUT/a.dump" | grep -q "running=false" && ok "① 검색 완료(running=false)" || bad "① 검색이 안 끝남" "$(cat "$OUT/a.dump")"
  has a.dump "Table|main|customer|.*|name" && ok "① 이름 일치 = customer 표" || bad "① 이름 일치 없음" "$(cat "$OUT/a.dump")"
  has a.dump "Table|main|orders|.*columns" && ok "① 컬럼 일치 = orders.customer_id" || bad "① 컬럼 일치 없음" "$(cat "$OUT/a.dump")"
  has a.dump "View|main|v_big|.*source lines" && ok "① 소스 일치 = v_big(뷰 정의)" || bad "① 뷰 소스 일치 없음" "$(cat "$OUT/a.dump")"
  has a.dump "Trigger|main|trg_orders|.*source lines" && ok "① 소스 일치 = trg_orders(트리거)" || bad "① 트리거 소스 일치 없음" "$(cat "$OUT/a.dump")"
else
  bad "① 덤프 없음" "$(tail -4 "$OUT/a.stderr")"
fi
# ② 대소문자 구분(c:) "CUSTOMER" = 뷰의 `FROM CUSTOMER` 줄만 · orders 컬럼(소문자)은 안 걸림 · 트리거(소문자)도 안 걸림
gui b "$PRE,@after:4500:findobj.query:c:CUSTOMER,@after:8000:findobj.dump:$(w "$OUT/b.dump")" 10
has b.dump "View|main|v_big|" && ok "② Aa: 뷰(대문자 CUSTOMER) 일치" || bad "② Aa 뷰 일치 없음" "$(cat "$OUT/b.dump" 2>/dev/null)"
if has b.dump "Trigger|main|trg_orders|"; then bad "② Aa인데 소문자 트리거가 걸림" "$(cat "$OUT/b.dump")"; else ok "② Aa: 소문자 트리거 제외"; fi
# ③ 낱말(w:) "customer" = 컬럼 customer_id는 제외 · 표 이름 customer · 소스 `FROM CUSTOMER`(낱말) 포함
gui c "$PRE,@after:4500:findobj.query:w:customer,@after:8000:findobj.dump:$(w "$OUT/c.dump")" 10
if has c.dump "Table|main|orders|"; then bad "③ 낱말인데 customer_id 컬럼이 걸림" "$(cat "$OUT/c.dump")"; else ok "③ 낱말: customer_id 제외"; fi
has c.dump "Table|main|customer|" && ok "③ 낱말: customer 표 포함" || bad "③ customer 표 없음" "$(cat "$OUT/c.dump" 2>/dev/null)"
# ④ pick 0 = 첫 행(정렬 = 이름순 → customer 표? 소스 없는 표 = 탐색기에서 보기) → 뷰 행을 고르려면 뷰만 검색: "v_big" 이름 일치 + 소스
gui d "$PRE,@after:4500:findobj.query:o.amount,@after:8000:findobj.pick:0,@after:11000:editor.dump:$(w "$OUT/d.editor")" 13
has d.editor "CREATE VIEW v_big" && ok "④ pick 0 = 뷰 소스 탭 열림(editor.dump에 CREATE VIEW)" || bad "④ 소스 탭 없음" "$(cat "$OUT/d.editor" 2>/dev/null; tail -3 "$OUT/d.stderr")"
# ⑤ 상한 1 = 상태줄 안내
mk_home "findobj.max_hits=1\n"
gui e "$PRE,@after:4500:findobj.query:customer,@after:8000:findobj.dump:$(w "$OUT/e.dump")" 10
head -1 "$OUT/e.dump" 2>/dev/null | grep -q "notes=.*limit\|notes=.*상한" && ok "⑤ 상한 1 = 안내(notes)" || bad "⑤ 상한 안내 없음" "$(head -1 "$OUT/e.dump" 2>/dev/null)"
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
