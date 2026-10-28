#!/usr/bin/env bash
# prod-mode-scenario.sh — ★ 운영기(PRD) 점검 시나리오(사용자 10-15 "운영기 테스트 스크립트 · 순서대로 실행하며 기능 확인 · 운영기 변경까지 명령으로").
#   실제 프로필은 건드리지 않는다 — `-P <실제 설정 폴더>`의 profiles/device.key를 격리 홈에 **복사**하고 그 사본에만 `nsql conn env <프로필> prd`.
#   임시 객체 = NSQLT_HOLD_T(표) · NSQLT_HOLD_PRC(프로시저 · Oracle) / NSQLT_HOLD_V(뷰 · SQLite) — 끝(단계 9)에서 정리.
#   단계(docs/114 §11):
#     0 준비 = 격리 홈 · 프로필 사본 운영기(PRD) 전환 확인 · 임시 객체 생성(CLI)
#     1 운영 2단 재실행(E3 · DDL) = 첫 실행은 무장만(토스트) · 3 s 안 두 번째 실행 = 실행
#     2 객체 백업 = CREATE OR REPLACE(F5/편집기) 전 현재 DDL → <홈>/backups/objects/<프로필>_<스키마>.<객체>_<종류>.<17자리 시각>
#     3 운영 보류 커밋 = UPDATE 1행 → 커밋 안 됨(카드 · 상단 Commit 빨간 2단) → Commit 두 번 = 반영
#     4 보류 → Rollback = 원복
#     5 스크립트 안 COMMIT = 직전 멈춤(남은 문장 보존) → [커밋] = COMMIT + 남은 문장 이어 실행 → 다시 보류 → Commit 2단
#     6 보류 중 다른 탭 변경·DDL 차단 · 조회 허용
#     7 탐색기 ▸ 삭제(모달 2단) = 백업 뒤 DROP 전송(G6) · 보류 생략
#     8 (옵션 -L) 보류 한도 = 1분 뒤 자동 롤백(설정 tx.prod_idle_limit_min=1 · 카운트다운 5 s)
#     9 정리 = 임시 객체 삭제 · 격리 홈 유형 되돌림(none)
# 사용: scripts/prod-mode-scenario.sh -P "$APPDATA/nexa-sql" [-c APPDB] [-d oracle|sqlite] [-o target/prod-scenario] [-i] [-k] [-s N] [-L]
#   -i = 단계마다 멈춤(Enter = 다음 · q = 종료) · -k = GUI를 띄운 채 두고 Enter 뒤 닫음(직접 보기 · 창이 앞으로 옴) · -s N = 그 단계만(0·9는 늘)
#   -d sqlite -c Local = 서버 없이 로컬 파일 DB로(프로필을 스크립트가 만든다 · -P 불필요)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/target/prod-scenario"; PROF=""; CONN="APPDB"; DIALECT="oracle"; STEP=""; INTER=0; KEEP=0; LIMIT=0
EXE="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"
while getopts "o:P:c:d:s:g:n:ikL" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; c) CONN=$OPTARG;; d) DIALECT=$OPTARG;; s) STEP=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; i) INTER=1;; k) KEEP=1;; L) LIMIT=1;; esac; done
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) [ -x "$NSQL" ] || NSQL="$NSQL.exe"; [ -x "$EXE" ] || EXE="$EXE.exe";; esac
[ -x "$NSQL" ] && [ -x "$EXE" ] || { echo "nsql/nexa-sql 실행 파일이 없습니다: $NSQL · $EXE (먼저 cargo build -p nexa-sql -p nsql-cli)"; exit 2; }
if [ "$DIALECT" != "sqlite" ] && [ -z "$PROF" ]; then
  case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) PROF="${APPDATA:-}/nexa-sql";; Darwin) PROF="$HOME/Library/Application Support/nexa-sql";; *) PROF="${XDG_CONFIG_HOME:-$HOME/.config}/nexa-sql";; esac
fi
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"; H="$OUT/home"
REPORT="$OUT/prod-scenario.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  ✅ PASS  $1"; pass=$((pass+1)); }
bad() { say "  ❌ FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }
D() { w "$OUT/$1"; }
has() { grep -q -- "$2" "$OUT/$1" 2>/dev/null; }
hr() { say ""; say "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"; }
step_on() { [ -z "$STEP" ] || [ "$STEP" = "$1" ]; }
pause() { # 단계 끝 — -i면 멈춘다.
  [ "$INTER" = 1 ] || return 0
  printf '\n  ⏎ Enter = 다음 단계 · q = 종료: ' ; read -r a < /dev/tty || true
  [ "$a" = "q" ] && { say "사용자 종료"; finish; }
}
finish() { say ""; say "RESULT PASS $pass FAIL $fail · 보고 = $REPORT · 산출 = $OUT"; exit $([ "$fail" -eq 0 ] && echo 0 || echo 1); }

# ── CLI ───────────────────────────────────────────────────────────────────────────────────────────
cli() { NSQL_HOME="$H" "$NSQL" run -c "$CONN" --no-prompt -f csv "$(m "$1")" 2>&1; }
q() { printf '%s\n' "$1" > "$OUT/q.sql"; cli "$OUT/q.sql"; }
val() { q "SELECT V FROM NSQLT_HOLD_T WHERE ID = $1${FROM_DUAL_END}" | tail -1 | tr -d '\r"'; }
if [ "$DIALECT" = "sqlite" ]; then
  FROM_DUAL_END=";"; DUAL=""; SCHEMA_Q="main"
  OBJ_KIND="VIEW"; OBJ="NSQLT_HOLD_V"; FOLDER="Views"
  obj_exists() { NSQL_HOME="$H" "$NSQL" cat -c "$CONN" views 2>&1 | grep -ci "NSQLT_HOLD_V"; }
  mk_objects() {
    q "CREATE TABLE NSQLT_HOLD_T (ID INTEGER PRIMARY KEY, V TEXT);" >/dev/null
    q "INSERT INTO NSQLT_HOLD_T VALUES (1,'o1');" >/dev/null; q "INSERT INTO NSQLT_HOLD_T VALUES (2,'o2');" >/dev/null; q "INSERT INTO NSQLT_HOLD_T VALUES (3,'o3');" >/dev/null
    q "CREATE VIEW NSQLT_HOLD_V AS SELECT ID FROM NSQLT_HOLD_T;" >/dev/null
  }
  REPLACE_SQL="DROP VIEW NSQLT_HOLD_V;"                          # SQLite = 뷰 DROP(백업 대상)
  DDL_T2="CREATE TABLE NSQLT_HOLD_T2 (A INTEGER);"
else
  FROM_DUAL_END=""; DUAL=" FROM dual"; SCHEMA_Q=""
  OBJ_KIND="PROCEDURE"; OBJ="NSQLT_HOLD_PRC"; FOLDER="Procedures"
  obj_exists() { q "SELECT COUNT(*) AS n FROM user_objects WHERE object_name = 'NSQLT_HOLD_PRC'" | tail -1 | tr -d '\r"'; }
  mk_objects() {
    q "CREATE TABLE NSQLT_HOLD_T (ID NUMBER PRIMARY KEY, V VARCHAR2(20))" >/dev/null
    printf "INSERT INTO NSQLT_HOLD_T VALUES (1,'o1');\nINSERT INTO NSQLT_HOLD_T VALUES (2,'o2');\nINSERT INTO NSQLT_HOLD_T VALUES (3,'o3');\nCOMMIT;\n" > "$OUT/ins.sql"; cli "$OUT/ins.sql" >/dev/null
    q "CREATE OR REPLACE PROCEDURE NSQLT_HOLD_PRC AS BEGIN NULL; END;" >/dev/null
  }
  REPLACE_SQL="CREATE OR REPLACE PROCEDURE NSQLT_HOLD_PRC AS BEGIN NULL; END;"
  DDL_T2="CREATE TABLE NSQLT_HOLD_T2 (A NUMBER)"
fi

# ── GUI(격리 홈 · 기동 명령 · 키 주입 0) ───────────────────────────────────────────────────────────
GUI_PID=""
gui_start() { # <이름> <명령> — 배경 기동 · PID 기억 · 같은 -o 폴더 재사용 때 지난 회차 산출이 판정을 속이지 않게 그 단계 파일을 먼저 지운다.
  local name=$1 cmd=$2
  # ★ T-365(0.2.2 V3): 종전 `rm -f "$OUT/$name".*`는 그 단계의 **입력 `sN.sql`·`sNa.sql`까지** 지워 앱이 빈 파일을 열었다("Nothing to run" · 단계 1~6 연쇄 실패)
  #   → 산출만 지운다(`.sql` 제외 · 덤프·stdout·stderr 전부).
  local f
  for f in "$OUT/$name".* "$OUT/${name}"[a-z].*; do
    case "$f" in *.sql) ;; *) rm -f "$f" 2>/dev/null ;; esac
  done
  if [ "$KEEP" = 1 ]; then
    NSQL_HOME="$H" NSQL_STARTUP_CMD="$cmd" "$EXE" "$CONN" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr" &
  else
    NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" "$EXE" "$CONN" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr" &
  fi
  GUI_PID=$!
}
wait_file() { # <파일> <초> — 덤프 파일이 생길 때까지.
  local f="$OUT/$1" n=$(( $2 * 2 )); while [ $n -gt 0 ] && [ ! -s "$f" ]; do sleep 0.5; n=$((n-1)); done; [ -s "$f" ]
}
gui_end() { # 보기 모드면 Enter까지 유지 · 그 뒤 내가 띄운 PID만 끝낸다.
  if [ "$KEEP" = 1 ]; then printf '\n  👀 창을 확인한 뒤 Enter(창을 닫습니다): '; read -r _ < /dev/tty || true; fi
  [ -n "$GUI_PID" ] && { kill "$GUI_PID" 2>/dev/null; wait "$GUI_PID" 2>/dev/null; }
  GUI_PID=""
}
show() { # <라벨> <파일> — 덤프를 보여 준다.
  say "  · $1: $(head -c 400 "$OUT/$2" 2>/dev/null | tr '\n' ' ')"
}
sqlf() { printf '%s\n' "$2" > "$OUT/$1.sql"; }

# ═══ 0. 준비 ════════════════════════════════════════════════════════════════════════════════════
hr; say "▶ 0. 준비 — 격리 홈 · 프로필 사본 운영기(PRD) 전환 · 임시 객체"
rm -rf "$H"; mkdir -p "$H"
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1280,840\nrun.prod_confirm=on\ntx.prod_hold=on\n' > "$H/settings.conf"
[ "$LIMIT" = 1 ] && printf 'tx.prod_idle_limit_min=1\ntx.prod_stale_min=1\ntx.idle_countdown_secs=5\n' >> "$H/settings.conf"
if [ "$DIALECT" = "sqlite" ]; then
  NSQL_HOME="$H" "$NSQL" conn add "$CONN" "sqlite:$(m "$H")/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1
else
  [ -d "$PROF/profiles" ] || { say "❌ 실제 설정 폴더에 profiles가 없습니다: $PROF (-P 로 지정)"; exit 1; }
  cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
fi
say "  \$ nsql conn env $CONN prd            ← 운영기로 변경(격리 홈 사본만)"
NSQL_HOME="$H" "$NSQL" conn env "$CONN" prd >/dev/null 2>&1
ENV=$(NSQL_HOME="$H" "$NSQL" conn env "$CONN" 2>&1)
say "  \$ nsql conn env $CONN                ← $ENV"
echo "$ENV" | grep -qi "Production\|PRD" && ok "0 프로필 사본 = 운영(PRD) · 실제 프로필($PROF)은 그대로" || { bad "0 유형 변경 실패" "$ENV"; finish; }
q "DROP TABLE NSQLT_HOLD_T${FROM_DUAL_END}" >/dev/null 2>&1
if [ "$DIALECT" = "sqlite" ]; then q "DROP VIEW NSQLT_HOLD_V;" >/dev/null 2>&1; else q "DROP PROCEDURE NSQLT_HOLD_PRC" >/dev/null 2>&1; q "DROP TABLE NSQLT_HOLD_T2" >/dev/null 2>&1; fi
mk_objects
[ "$(val 1)" = "o1" ] && ok "0 임시 객체 생성(NSQLT_HOLD_T 3행 · $OBJ)" || { bad "0 임시 객체 생성 실패" "$(q "SELECT COUNT(*) AS n FROM NSQLT_HOLD_T${FROM_DUAL_END}")"; finish; }
SCHEMA=$(if [ "$DIALECT" = "sqlite" ]; then echo main; else q "SELECT USER AS u${DUAL}" | tail -1 | tr -d '\r"'; fi)
BK="$H/backups/objects"; say "  · 스키마 = $SCHEMA · 백업 폴더 = $BK"
# 워밍업(갓 빌드된 exe 첫 기동).
gui_start warm "@after:1200:ui.move:10/10"; sleep 3; kill "$GUI_PID" 2>/dev/null; wait "$GUI_PID" 2>/dev/null; GUI_PID=""
pause

# ═══ 1. 운영 2단 재실행(E3 · DDL) ═══════════════════════════════════════════════════════════════
if step_on 1; then
hr; say "▶ 1. 운영 2단 재실행(DDL) — 첫 실행은 무장만(빨간 토스트 · 서버로 안 감) · 3 s 안 두 번째 실행 = 실행"
sqlf s1 "$DDL_T2"
gui_start s1 "@connected:open:$(D s1.sql),@connected:@after:1500:run.all,@connected:@after:2500:status.dump:$(D s1a.st),@connected:@after:2700:toast.dump:$(D s1a.toast),@connected:@after:3000:run.all,@connected:@after:6000:output.dump:$(D s1b.out),@connected:@after:6200:status.dump:$(D s1b.st)"
wait_file s1a.st 20 && show "첫 실행 상태줄" s1a.st
has s1a.st "run again\|again\|Production" && ok "1 첫 실행 = 무장만(상태줄 안내)" || bad "1 무장 안내 없음" "$(cat "$OUT/s1a.st" 2>/dev/null)"
wait_file s1b.out 12; show "두 번째 실행 Output" s1b.out
has s1b.out "done" && ok "1 두 번째 실행 = 서버 실행(done)" || bad "1 두 번째 실행 결과 없음" "$(cat "$OUT/s1b.out" 2>/dev/null)"
gui_end
q "DROP TABLE NSQLT_HOLD_T2${FROM_DUAL_END}" >/dev/null 2>&1
pause; fi

# ═══ 2. 객체 백업 ════════════════════════════════════════════════════════════════════════════════
if step_on 2; then
hr; say "▶ 2. 객체 백업 — 변경 DDL($OBJ_KIND) 실행 전 현재 DDL을 파일로(설정 Object backup · 운영 2단은 두 번 실행)"
sqlf s2 "$REPLACE_SQL"
gui_start s2 "@connected:open:$(D s2.sql),@connected:@after:1500:run.all,@connected:@after:3000:run.all,@connected:@after:9000:output.dump:$(D s2.out),@connected:@after:9200:objbk.dump:$(D s2.objbk)"
wait_file s2.out 22; show "Output" s2.out
has s2.out "Object backup: " && ok "2 Output에 백업 줄(객체 → 파일)" || bad "2 백업 줄 없음" "$(cat "$OUT/s2.out" 2>/dev/null)"
f=$(ls "$BK"/*"${OBJ}_${OBJ_KIND}."* 2>/dev/null | head -1)
if [ -n "$f" ]; then ok "2 백업 파일 = $(basename "$f")"; say "  · 머리 주석: $(head -2 "$f" | tr '\n' ' ')"; else bad "2 백업 파일 없음" "$(ls "$BK" 2>/dev/null)"; fi
gui_end
pause; fi

# ═══ 3. 운영 보류 커밋 → Commit 2단 ════════════════════════════════════════════════════════════
if step_on 3; then
hr; say "▶ 3. 운영 보류 — UPDATE 1행은 커밋되지 않고 보류(주황 카드 · 대기 1) → 상단 Commit 첫 클릭 = 빨간 무장 · 두 번째 = 커밋"
sqlf s3 "UPDATE NSQLT_HOLD_T SET V = 'x1' WHERE ID = 1${FROM_DUAL_END}"
gui_start s3 "@connected:open:$(D s3.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D s3a.tx),@connected:@after:5500:toast.dump:$(D s3a.toast),@connected:@after:6000:run.commit,@connected:@after:6500:tx.dump:$(D s3b.tx),@connected:@after:6700:toast.dump:$(D s3b.toast),@connected:@after:7500:run.commit,@connected:@after:10500:tx.dump:$(D s3c.tx)"
wait_file s3a.tx 20; show "보류 상태(tx.dump)" s3a.tx; wait_file s3a.toast 8; show "카드(toast.dump)" s3a.toast
has s3a.tx "pending=1|held=stmts=1 rows=1" && ok "3 UPDATE = 보류(대기 1 · 1행 · 커밋 안 됨)" || bad "3 보류 안 됨" "$(cat "$OUT/s3a.tx" 2>/dev/null)"
has s3a.toast "commit pending" && ok "3 주황 고정 카드 [Commit]" || bad "3 카드 없음" "$(cat "$OUT/s3a.toast" 2>/dev/null)"
if [ "$DIALECT" != "sqlite" ]; then v=$(val 1); [ "$v" = "o1" ] && ok "3 다른 세션(CLI)에서는 아직 o1(미커밋)" || bad "3 CLI 값 $v(커밋됐으면 결함)"; fi
wait_file s3b.tx 8; show "첫 클릭 뒤" s3b.tx
has s3b.tx "armed=true" && ok "3 Commit 첫 클릭 = 무장(빨간 카드 · 초 줄어듦)" || bad "3 무장 안 됨" "$(cat "$OUT/s3b.tx" 2>/dev/null)"
wait_file s3c.tx 8; show "두 번째 클릭 뒤" s3c.tx
has s3c.tx "pending=0|held=none" && ok "3 두 번째 클릭 = 커밋(대기 0)" || bad "3 커밋 안 됨" "$(cat "$OUT/s3c.tx" 2>/dev/null)"
gui_end
v=$(val 1); [ "$v" = "x1" ] && ok "3 서버 값 = x1(커밋 반영)" || bad "3 서버 값 $v"
pause; fi

# ═══ 4. 보류 → Rollback ═════════════════════════════════════════════════════════════════════════
if step_on 4; then
hr; say "▶ 4. 보류 → Rollback — 보류된 변경을 상단 Rollback(확인 없음)으로 되돌림"
sqlf s4 "UPDATE NSQLT_HOLD_T SET V = 'x2' WHERE ID = 2${FROM_DUAL_END}"
gui_start s4 "@connected:open:$(D s4.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D s4a.tx),@connected:@after:5500:run.rollback,@connected:@after:8500:tx.dump:$(D s4b.tx)"
wait_file s4a.tx 20; wait_file s4b.tx 10; show "보류" s4a.tx; show "롤백 뒤" s4b.tx
has s4a.tx "pending=1" && has s4b.tx "pending=0|held=none" && ok "4 보류 → Rollback = 대기 0" || bad "4 롤백 흐름" "$(cat "$OUT/s4a.tx" "$OUT/s4b.tx" 2>/dev/null)"
gui_end
v=$(val 2); [ "$v" = "o2" ] && ok "4 서버 값 원복(o2)" || bad "4 서버 값 $v"
pause; fi

# ═══ 5. 스크립트 안 COMMIT ══════════════════════════════════════════════════════════════════════
if step_on 5; then
hr; say "▶ 5. 스크립트 안 COMMIT — UPDATE; COMMIT; UPDATE → COMMIT 직전에 멈춤(카드 'COMMIT 확인') → [커밋] = COMMIT + 남은 문장 → 다시 보류 → Commit 2단"
printf "UPDATE NSQLT_HOLD_T SET V = 'a3' WHERE ID = 3;\nCOMMIT;\nUPDATE NSQLT_HOLD_T SET V = 'b3' WHERE ID = 3;\n" > "$OUT/s5.sql"
gui_start s5 "@connected:open:$(D s5.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D s5a.tx),@connected:@after:4700:toast.dump:$(D s5a.toast),@connected:@after:5500:hold.commit_go,@connected:@after:9500:tx.dump:$(D s5b.tx),@connected:@after:10000:run.commit,@connected:@after:11000:run.commit,@connected:@after:14000:tx.dump:$(D s5c.tx)"
wait_file s5a.tx 20; show "COMMIT 직전" s5a.tx; show "카드" s5a.toast
has s5a.tx "at_commit=true" && ok "5 사용자 COMMIT 직전 멈춤(남은 문장 보존)" || bad "5 멈춤 안 됨" "$(cat "$OUT/s5a.tx" 2>/dev/null)"
wait_file s5b.tx 12; show "[커밋] 뒤" s5b.tx
has s5b.tx "pending=1|held=stmts=1 rows=1 at_commit=false" && ok "5 [커밋] → COMMIT 통과 + 남은 UPDATE 실행 → 다시 보류" || bad "5 이어 실행" "$(cat "$OUT/s5b.tx" 2>/dev/null)"
wait_file s5c.tx 10; show "최종" s5c.tx
gui_end
v=$(val 3); has s5c.tx "pending=0" && [ "$v" = "b3" ] && ok "5 최종 커밋 = b3" || bad "5 최종 값 $v" "$(cat "$OUT/s5c.tx" 2>/dev/null)"
pause; fi

# ═══ 6. 보류 중 다른 탭 차단 ════════════════════════════════════════════════════════════════════
if step_on 6; then
hr; say "▶ 6. 보류 중 다른 탭 — 변경 차단 · DDL 차단 · 조회 허용(상태줄 안내 · 서버로 안 감)"
sqlf s6a "UPDATE NSQLT_HOLD_T SET V = 'zz' WHERE ID = 1${FROM_DUAL_END}"
sqlf s6b "UPDATE NSQLT_HOLD_T SET V = 'yy' WHERE ID = 1${FROM_DUAL_END}"
sqlf s6c "$DDL_T2"
sqlf s6d "SELECT COUNT(*) AS N FROM NSQLT_HOLD_T${FROM_DUAL_END}"
gui_start s6 "@connected:open:$(D s6a.sql),@connected:@after:1500:run.all,@connected:@after:4000:open:$(D s6b.sql),@connected:@after:4500:run.all,@connected:@after:5500:status.dump:$(D s6b.st),@connected:@after:6000:open:$(D s6c.sql),@connected:@after:6500:run.all,@connected:@after:7500:status.dump:$(D s6c.st),@connected:@after:8000:open:$(D s6d.sql),@connected:@after:8500:run.all,@connected:@after:11000:grid.dump:$(D s6d.grid),@connected:@after:11200:tx.dump:$(D s6.tx)"
wait_file s6b.st 20; show "다른 탭 변경" s6b.st
has s6b.st "held - commit or roll back" && ok "6 보류 중 다른 탭 변경 = 차단" || bad "6 다른 탭 차단 안 됨" "$(cat "$OUT/s6b.st" 2>/dev/null)"
wait_file s6c.st 6; show "DDL" s6c.st
has s6c.st "held - commit or roll back" && ok "6 보류 중 DDL = 차단" || bad "6 DDL 차단 안 됨" "$(cat "$OUT/s6c.st" 2>/dev/null)"
wait_file s6d.grid 8; [ -s "$OUT/s6d.grid" ] && ok "6 조회는 허용(결과 그리드)" || bad "6 조회 결과 없음"
wait_file s6.tx 4; has s6.tx "pending=1" && ok "6 보류는 그대로(대기 1)" || bad "6 대기 상태" "$(cat "$OUT/s6.tx" 2>/dev/null)"
gui_end
v=$(val 1); [ "$v" = "x1" ] && ok "6 창 종료 = 보류분 롤백(x1 유지)" || bad "6 값 $v"
pause; fi

# ═══ 7. 탐색기 삭제 창(백업 + G6) ═══════════════════════════════════════════════════════════════
if step_on 7; then
hr; say "▶ 7. 탐색기 ▸ 삭제… — 모달 2단 확인 → DDL 백업 → DROP 전송(운영 · 첫 호출에 실제 전송 = G6) · 보류 생략"
[ "$DIALECT" = "sqlite" ] && q "CREATE VIEW NSQLT_HOLD_V AS SELECT ID FROM NSQLT_HOLD_T;" >/dev/null 2>&1
before=$(ls "$BK" 2>/dev/null | grep -c "${OBJ}_${OBJ_KIND}\.")
# 첫 펼침은 접속 **1 s 뒤**(130차 mac · 10-17): 지연 0의 `@connected:explorer.expand`는 SQLite처럼 스키마 행이 아직 안 그려진 순간에 들어가 ok=false →
#   뒤 단계가 전부 빗나갔다(Oracle은 시점이 달라 통과). 규칙 = 탐색기 명령은 접속 뒤 첫 그리기 이후(61 §4).
gui_start s7 "@connected:@after:1000:explorer.expand:$SCHEMA,@connected:@after:3500:explorer.expand:$FOLDER,@connected:@after:6000:explorer.select:$OBJ,@connected:@after:6500:explorer.pick:drop,@connected:@after:8000:drop.confirm,@connected:@after:14000:drop.dump:$(D s7.dump),@connected:@after:14300:output.dump:$(D s7.out)"
wait_file s7.dump 32; show "삭제 창" s7.dump
has s7.dump "open=true" && (has s7.dump "sent" || has s7.dump "Done") && ok "7 삭제 창 = 전송됨(G6 · 첫 호출에 실제 전송)" || bad "7 창 단계" "$(cat "$OUT/s7.dump" 2>/dev/null)"
after=$(ls "$BK" 2>/dev/null | grep -c "${OBJ}_${OBJ_KIND}\.")
[ "$after" -gt "$before" ] && ok "7 DROP 전 백업 파일 +1($before → $after)" || bad "7 백업 파일 안 늘음($before → $after)" "$(ls "$BK" 2>/dev/null)"
gui_end
[ "$(obj_exists)" = "0" ] && ok "7 서버에서 $OBJ 사라짐(DROP 실제 전송)" || bad "7 객체 남음" "$(cat "$OUT/s7.out" 2>/dev/null | head -6)"
pause; fi

# ═══ 8. (옵션) 보류 한도 롤백 ═══════════════════════════════════════════════════════════════════
if [ "$LIMIT" = 1 ] && step_on 8; then
hr; say "▶ 8. 보류 한도 — 운영 한도 1분(시험 설정) 지나면 설정과 무관하게 롤백(카운트다운 5 s)"
sqlf s8 "UPDATE NSQLT_HOLD_T SET V = 'lim' WHERE ID = 2${FROM_DUAL_END}"
gui_start s8 "@connected:open:$(D s8.sql),@connected:@after:1500:run.all,@connected:@after:4500:tx.dump:$(D s8a.tx),@connected:@after:80000:tx.dump:$(D s8b.tx),@connected:@after:80500:output.dump:$(D s8b.out)"
wait_file s8a.tx 20; show "보류" s8a.tx; say "  · 한도(1분 + 카운트다운 5 s)를 기다립니다…"
wait_file s8b.tx 90; show "한도 뒤" s8b.tx
has s8b.tx "pending=0|held=none" && ok "8 한도 지남 = 자동 롤백(대기 0)" || bad "8 한도 롤백 안 됨" "$(cat "$OUT/s8b.tx" 2>/dev/null)"
gui_end
v=$(val 2); [ "$v" = "o2" ] && ok "8 서버 값 원복(o2)" || bad "8 값 $v"
pause; fi

# ═══ 9. 정리 ════════════════════════════════════════════════════════════════════════════════════
hr; say "▶ 9. 정리 — 임시 객체 삭제 · 격리 홈 유형 되돌림"
q "DROP TABLE NSQLT_HOLD_T${FROM_DUAL_END}" >/dev/null 2>&1
if [ "$DIALECT" = "sqlite" ]; then q "DROP VIEW NSQLT_HOLD_V;" >/dev/null 2>&1; else q "DROP PROCEDURE NSQLT_HOLD_PRC" >/dev/null 2>&1; q "DROP TABLE NSQLT_HOLD_T2" >/dev/null 2>&1; fi
NSQL_HOME="$H" "$NSQL" conn env "$CONN" none >/dev/null 2>&1
say "  \$ nsql conn env $CONN none           ← $(NSQL_HOME="$H" "$NSQL" conn env "$CONN" 2>&1)"
ok "9 임시 객체 정리 · 사본 유형 none(실제 프로필은 처음부터 그대로)"
finish
