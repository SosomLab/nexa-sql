#!/usr/bin/env bash
# win-objbackup-prod-e2e.sh — 실서버(Oracle · 기본 APPDB)를 **운영기(PRD) 모드**로 바꾼 격리 홈에서 객체 백업 + 운영 2단 + G6를 자동 시험
#   (사용자 10-15 "APPDB을 운영기 모드로 변경해서 자동화 테스트 진행" · docs/114 §7-2 · D-302/D-305). 실제 프로필은 건드리지 않는다 —
#   `-P <실제 설정 폴더>`의 profiles/device.key를 격리 홈에 복사하고 그 사본에만 `nsql conn env <프로필> prd`.
#   임시 객체 = `NSQLT_BK_PRC`(프로시저 · 61 §2-4 ⑤ 실서버 임시 객체는 묻지 않음 · 끝에 정리).
#   ① CLI로 만들기(백업 없음) ② GUI 편집기 CREATE OR REPLACE(운영 2단 = run.all 두 번) → 백업 파일 1 + Output 줄 ③ GUI 편집기 DROP(2단) → 백업 2 + 객체 없음
#   ④ CLI로 다시 만들기 → GUI 탐색기 삭제(G6: 삭제 창 2단 뒤 첫 호출에 전송) → 백업 3 + 객체 없음 ⑤ 파일 이름 = `<프로필>_<스키마>.NSQLT_BK_PRC_PROCEDURE.<17자리>`.
# 사용: scripts/win-objbackup-prod-e2e.sh -o <출력폴더> -P <실제 설정 폴더(%APPDATA%/nexa-sql)> [-c APPDB] [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역(130차 mac)
OUT=""; PROF=""; CONN="APPDB"; EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:P:c:g:n:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; c) CONN=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out dir> -P <real settings dir> [-c profile] [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
REPORT="$OUT/objbackup-prod-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -14 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\nrun.prod_confirm=on\n' > "$H/settings.conf"
[ -d "$PROF/profiles" ] || { say "FAIL  profiles not found under $PROF"; exit 1; }
cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
export NSQL_HOME="$H"
"$NSQL" conn env "$CONN" prd >/dev/null 2>&1
ENV=$("$NSQL" conn env "$CONN" 2>&1)
echo "$ENV" | grep -qi "Production\|PRD" && ok "격리 홈 사본의 $CONN = 운영(PRD) · 실제 프로필은 그대로" || bad "유형 변경 실패" "$ENV"
cli() { "$NSQL" run -c "$CONN" --no-prompt -f csv "$(m "$1")" 2>&1; }
q() { printf '%s\n' "$1" > "$OUT/q.sql"; cli "$OUT/q.sql"; }
SCHEMA=$(q "SELECT USER AS u FROM dual;" | tail -1 | tr -d '\r"')
[ -n "$SCHEMA" ] && ok "접속 ✓ · 스키마 = $SCHEMA" || { bad "접속 실패(스키마 조회)" "$(q "SELECT USER AS u FROM dual;")"; say "RESULT PASS $pass FAIL $fail"; exit 1; }
BK="$H/backups/objects"; STEM="${CONN}_${SCHEMA}.NSQLT_BK_PRC_PROCEDURE"
count() { ls "$BK" 2>/dev/null | grep -c "^${STEM}\." ; }
exists() { q "SELECT COUNT(*) AS n FROM all_objects WHERE owner = '$SCHEMA' AND object_name = 'NSQLT_BK_PRC';" | tail -1 | tr -d '\r"'; }
create_cli() { q "CREATE OR REPLACE PROCEDURE NSQLT_BK_PRC AS BEGIN NULL; END;" >/dev/null; }
gui() { # <이름> <명령> <초>
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$2" timeout -s KILL "$3" "$EXE" "$CONN" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
}
# 운영 2단 확인 = 같은 본문을 3초 안에 다시 실행(`run.prod_confirm`) → run.all 두 번(1.5 s 간격) · 백업은 비동기라 넉넉히.
run_prod() { # <이름> <sql>
  printf '%s\n' "$2" > "$OUT/$1.sql"
  gui "$1" "@connected:open:$(w "$OUT/$1.sql"),@connected:@after:2000:run.all,@connected:@after:3500:run.all,@connected:@after:12000:objbk.dump:$(w "$OUT/$1.objbk"),@connected:@after:12300:output.dump:$(w "$OUT/$1.out")" 20
}

say "objbackup-prod-e2e $(date '+%F %T') · $CONN · $SCHEMA"
q "DROP PROCEDURE NSQLT_BK_PRC;" >/dev/null 2>&1
gui warm "@after:1500:ui.move:10/10" 5
# ① CLI 생성(GUI 아님 = 백업 없음).
create_cli
[ "$(exists)" = "1" ] && ok "① CLI CREATE → 객체 있음" || bad "① CLI 생성 실패" "$(q "CREATE OR REPLACE PROCEDURE NSQLT_BK_PRC AS BEGIN NULL; END;")"
[ "$(count)" = "0" ] && ok "① CLI 경로 = 백업 없음(설계)" || bad "① CLI인데 백업 생김" "$(ls "$BK")"
# ② GUI CREATE OR REPLACE(운영 2단) → 백업 1.
run_prod b "CREATE OR REPLACE PROCEDURE NSQLT_BK_PRC AS BEGIN NULL; END;"
[ "$(count)" = "1" ] && ok "② 운영 CREATE OR REPLACE(2단) → 백업 파일 1" || bad "② 백업 파일 수 $(count)" "$(ls "$BK" 2>/dev/null; cat "$OUT/b.out" 2>/dev/null | head -6)"
grep -q "Object backup: $SCHEMA.NSQLT_BK_PRC (PROCEDURE)" "$OUT/b.out" 2>/dev/null && ok "② Output 백업 줄" || bad "② Output 백업 줄 없음" "$(cat "$OUT/b.out" 2>/dev/null | head -8)"
grep -qi "done\|완료" "$OUT/b.out" 2>/dev/null && ok "② 실행 이어짐(done)" || bad "② 실행 결과 줄 없음" "$(cat "$OUT/b.out" 2>/dev/null | head -8)"
f=$(ls "$BK"/${STEM}.* 2>/dev/null | head -1)
if [ -n "$f" ]; then
  ext="${f##*.}"; echo "$ext" | grep -Eq '^[0-9]{17}$' && ok "⑤ 이름 = ${STEM}.<17자리 시각>" || bad "⑤ 확장자 모양 $ext"
  grep -q "action: replace" "$f" && grep -qi "PROCEDURE" "$f" && ok "⑤ 머리 주석 action: replace + DDL" || bad "⑤ 본문" "$(head -5 "$f")"
fi
# ③ GUI DROP(운영 2단) → 백업 2 + 객체 없음.
run_prod c "DROP PROCEDURE NSQLT_BK_PRC;"
[ "$(count)" = "2" ] && ok "③ 운영 DROP(2단) → 백업 파일 2(action: drop)" || bad "③ 백업 파일 수 $(count)" "$(ls "$BK" 2>/dev/null; cat "$OUT/c.out" 2>/dev/null | head -6)"
[ "$(exists)" = "0" ] && ok "③ DROP 실행됨(객체 없음)" || bad "③ 객체 남음" "$(cat "$OUT/c.out" 2>/dev/null | head -6)"
# ④ 다시 만들고 탐색기 삭제(G6 · 실서버 운영).
create_cli
gui d "@connected:explorer.expand:$SCHEMA,@connected:@after:2500:explorer.expand:Procedures,@connected:@after:5000:explorer.select:NSQLT_BK_PRC,@connected:@after:5500:explorer.pick:drop,@connected:@after:7000:drop.confirm,@connected:@after:14000:drop.dump:$(w "$OUT/d.dump"),@connected:@after:14300:output.dump:$(w "$OUT/d.out")" 22
[ "$(count)" = "3" ] && ok "④ 탐색기 삭제(운영) → 백업 파일 3" || bad "④ 백업 파일 수 $(count)" "$(cat "$OUT/d.dump" 2>/dev/null; cat "$OUT/d.out" 2>/dev/null | head -6)"
grep -qi "sent\|Done" "$OUT/d.dump" 2>/dev/null && ok "④ 삭제 창 단계 = 전송됨" || bad "④ 창 단계" "$(cat "$OUT/d.dump" 2>/dev/null)"
[ "$(exists)" = "0" ] && ok "④ G6: 운영 탐색기 DROP 첫 호출에 실제 전송(객체 없음)" || bad "④ 객체 남음(G6 재현?)" "$(cat "$OUT/d.out" 2>/dev/null | head -6)"
# 정리.
q "DROP PROCEDURE NSQLT_BK_PRC;" >/dev/null 2>&1
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
