#!/usr/bin/env bash
# win-prod-hold-e2e.sh — ★ 운영 보류 커밋 1단계(docs/114 §5 · D-295~D-301 · D-304 · T-356 · 10-15) 자체 시험. SQLite 격리 홈 · 프로필 유형 PRD ·
#   키 주입 0 · 기동 명령(`run.all` · `run.commit`/`run.rollback` = 메뉴 동작 · `hold.commit_go` = 토스트 [커밋] · 덤프 `tx.dump`/`toast.dump`/`status.dump`).
#   ① 운영 UPDATE(1행) = 커밋 안 됨(보류 · 카드) → Commit 두 번(2단) → 커밋됨 ② 보류 → Rollback → 원복 ③ 스크립트 안 COMMIT = 직전에 멈춤 → [커밋] → 남은 문장 이어 실행(다시 보류)
#   ④ 보류 중 다른 탭 변경 차단 · 같은 탭 DDL 차단(D-298) ⑤ 영향 0행 = 보류 없음 ⑥ 기준 5행: 3행 = 자동 커밋 · WHERE 없음 = 늘 보류 ⑦ 비운영 = 종전(바로 커밋)
# 사용: scripts/win-prod-hold-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:g:n:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
REPORT="$OUT/prod-hold-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -14 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"
mk_home() { # <유형 none|prd> <추가 설정>
  rm -rf "$H"; mkdir -p "$H"
  printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\nrun.prod_confirm=on\n%b' "$2" > "$H/settings.conf"
  NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
  NSQL_HOME="$H" "$NSQL" conn env Local "$1" >/dev/null 2>&1
  printf "CREATE TABLE t_h (id INTEGER PRIMARY KEY, v TEXT);\nINSERT INTO t_h VALUES (1,'o1');\nINSERT INTO t_h VALUES (2,'o2');\nINSERT INTO t_h VALUES (3,'o3');\n" > "$OUT/mk.sql"
  NSQL_HOME="$H" "$NSQL" run -c Local --no-prompt "$(m "$OUT/mk.sql")" >/dev/null 2>&1
}
val() { printf 'SELECT v FROM t_h WHERE id = %s;\n' "$1" > "$OUT/q.sql"; NSQL_HOME="$H" "$NSQL" run -c Local --no-prompt -f csv "$(m "$OUT/q.sql")" 2>&1 | tail -1 | tr -d '\r"'; }
gui() { # <이름> <명령> <초>
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$2" timeout -s KILL "$3" "$EXE" Local > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
}
D() { w "$OUT/$1"; }
has() { grep -q -- "$2" "$OUT/$1" 2>/dev/null; }

say "prod-hold-e2e $(date '+%F %T')"
mk_home prd ""
gui warm "@after:1500:ui.move:10/10" 8
# ① 운영 UPDATE 1행 → 보류(커밋 안 됨) → Commit ×2(2단 · 5 s 안) → 커밋됨.
printf "UPDATE t_h SET v = 'x1' WHERE id = 1;\n" > "$OUT/a.sql"
gui a "@connected:open:$(D a.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D a1.tx),@connected:@after:4700:toast.dump:$(D a1.toast),@connected:@after:5000:run.commit,@connected:@after:5500:tx.dump:$(D a2.tx),@connected:@after:6000:run.commit,@connected:@after:9000:tx.dump:$(D a3.tx)" 16
has a1.tx "pending=1|held=stmts=1 rows=1 at_commit=false" && ok "① 운영 UPDATE = 보류(pending 1 · 1행 · 카드)" || bad "① 보류 상태" "$(cat "$OUT/a1.tx" 2>/dev/null)"
has a1.toast "commit pending" && has a1.toast "hold.commit" && ok "① 보류 카드(토스트 · [Commit] 버튼)" || bad "① 카드 없음" "$(cat "$OUT/a1.toast" 2>/dev/null)"
has a2.tx "armed=true" && ok "① Commit 첫 클릭 = 무장(빨강 2단)" || bad "① 무장 안 됨" "$(cat "$OUT/a2.tx" 2>/dev/null)"
has a3.tx "pending=0|held=none|armed=false" && ok "① 두 번째 클릭 = 커밋(pending 0)" || bad "① 커밋 안 됨" "$(cat "$OUT/a3.tx" 2>/dev/null)"
[ "$(val 1)" = "x1" ] && ok "① 서버 값 = x1(커밋 반영)" || bad "① 서버 값 $(val 1)"
# ② 보류 → Rollback → 원복.
printf "UPDATE t_h SET v = 'x2' WHERE id = 2;\n" > "$OUT/b.sql"
gui b "@connected:open:$(D b.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D b1.tx),@connected:@after:5000:run.rollback,@connected:@after:8000:tx.dump:$(D b2.tx)" 15
has b1.tx "pending=1|held=stmts=1" && has b2.tx "pending=0|held=none" && ok "② 보류 → Rollback = 대기 0" || bad "② 롤백 흐름" "$(cat "$OUT/b1.tx" "$OUT/b2.tx" 2>/dev/null)"
[ "$(val 2)" = "o2" ] && ok "② 서버 값 원복(o2)" || bad "② 서버 값 $(val 2)"
# ③ 스크립트 안 COMMIT = 직전 멈춤(남은 문장 보존) → [커밋] → COMMIT + 남은 UPDATE → 다시 보류 → Commit ×2.
printf "UPDATE t_h SET v = 'a3' WHERE id = 3;\nCOMMIT;\nUPDATE t_h SET v = 'b3' WHERE id = 3;\n" > "$OUT/c.sql"
gui c "@connected:open:$(D c.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D c1.tx),@connected:@after:4700:toast.dump:$(D c1.toast),@connected:@after:5000:hold.commit_go,@connected:@after:8500:tx.dump:$(D c2.tx),@connected:@after:9000:run.commit,@connected:@after:9800:run.commit,@connected:@after:12500:tx.dump:$(D c3.tx)" 19
has c1.tx "at_commit=true" && grep -q "remaining=[1-9]" "$OUT/c1.tx" && ok "③ COMMIT 직전 멈춤(남은 문장 보존)" || bad "③ 멈춤 상태" "$(cat "$OUT/c1.tx" 2>/dev/null)"
has c1.toast "COMMIT needs your confirmation" && ok "③ COMMIT 확인 토스트" || bad "③ 토스트" "$(cat "$OUT/c1.toast" 2>/dev/null)"
has c2.tx "pending=1|held=stmts=1 rows=1 at_commit=false" && ok "③ [커밋] 뒤 남은 UPDATE 실행 → 다시 보류" || bad "③ 이어 실행" "$(cat "$OUT/c2.tx" 2>/dev/null)"
has c3.tx "pending=0" && [ "$(val 3)" = "b3" ] && ok "③ 최종 커밋 = b3" || bad "③ 최종 값 $(val 3)" "$(cat "$OUT/c3.tx" 2>/dev/null)"
# ④ 보류 중 다른 탭 변경 차단 · 같은 탭 DDL 차단(D-298) · 조회 허용.
printf "UPDATE t_h SET v = 'zz' WHERE id = 1;\n" > "$OUT/d2.sql"
printf "CREATE TABLE t_x (a INTEGER);\n" > "$OUT/d3.sql"
printf "SELECT COUNT(*) FROM t_h;\n" > "$OUT/d4.sql"
gui d "@connected:open:$(D a.sql),@connected:@after:1500:run.all,@connected:@after:4000:open:$(D d2.sql),@connected:@after:4500:run.all,@connected:@after:5500:status.dump:$(D d1.st),@connected:@after:5700:tx.dump:$(D d1.tx),@connected:@after:6000:open:$(D d3.sql),@connected:@after:6500:run.all,@connected:@after:7500:status.dump:$(D d2.st),@connected:@after:8000:open:$(D d4.sql),@connected:@after:8500:run.all,@connected:@after:10500:grid.dump:$(D d3.grid)" 17
has d1.st "Production changes are held" && has d1.tx "pending=1" && ok "④ 보류 중 다른 탭 변경 = 차단" || bad "④ 다른 탭 차단" "$(cat "$OUT/d1.st" "$OUT/d1.tx" 2>/dev/null)"
has d2.st "Production changes are held" && ok "④ 보류 중 DDL = 차단" || bad "④ DDL 차단" "$(cat "$OUT/d2.st" 2>/dev/null)"
NSQL_HOME="$H" "$NSQL" cat -c Local tables 2>&1 | grep -q "t_x" && bad "④ 차단된 DDL이 실행됨" || ok "④ t_x 없음(DDL 안 감)"
[ -s "$OUT/d3.grid" ] && ok "④ 보류 중 조회는 허용(그리드 덤프 있음)" || bad "④ 조회 덤프 없음"
[ "$(val 1)" = "x1" ] && ok "④ 종료 = 보류분 롤백(x1 그대로)" || bad "④ 값 $(val 1)"
# ⑤ 영향 0행 = 보류 없음.
printf "UPDATE t_h SET v = 'q' WHERE id = 99;\n" > "$OUT/e.sql"
gui e "@connected:open:$(D e.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D e1.tx)" 11
has e1.tx "pending=0|held=none" && ok "⑤ 영향 0행 = 보류 없음(B)" || bad "⑤" "$(cat "$OUT/e1.tx" 2>/dev/null)"
# ⑥ 기준 5행: 3행(WHERE 있음) = 자동 커밋 · WHERE 없음 = 늘 보류.
mk_home prd "tx.prod_hold_rows=5\n"
printf "UPDATE t_h SET v = 'm' WHERE id IN (1,2,3);\n" > "$OUT/f.sql"
gui f "@connected:open:$(D f.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D f1.tx),@connected:@after:4700:output.dump:$(D f1.out)" 11
has f1.tx "pending=0|held=none" && [ "$(val 2)" = "m" ] && ok "⑥ 기준 5 · 3행 = 자동 커밋(m)" || bad "⑥ 기준 미만 커밋" "$(cat "$OUT/f1.tx" "$OUT/f1.out" 2>/dev/null)"
has f1.out "below the hold threshold - committed" && ok "⑥ Output 안내(기준 미만 커밋)" || bad "⑥ 안내 없음" "$(cat "$OUT/f1.out" 2>/dev/null)"
printf "UPDATE t_h SET v = 'n';\n" > "$OUT/g.sql"
gui g "@connected:open:$(D g.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D g1.tx)" 11
has g1.tx "pending=1|held=stmts=1 rows=3" && ok "⑥ WHERE 없는 UPDATE(3행 < 5) = 늘 보류" || bad "⑥ WHERE 없음 보류" "$(cat "$OUT/g1.tx" 2>/dev/null)"
[ "$(val 2)" = "m" ] && ok "⑥ 보류분은 종료 때 롤백(m 유지)" || bad "⑥ 값 $(val 2)"
# ⑦ 비운영 = 종전(바로 커밋).
mk_home none ""
gui h "@connected:open:$(D a.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D h1.tx)" 11
has h1.tx "pending=0|held=none" && [ "$(val 1)" = "x1" ] && ok "⑦ 비운영 = 바로 커밋(x1)" || bad "⑦ 비운영" "$(cat "$OUT/h1.tx" 2>/dev/null; val 1)"
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
