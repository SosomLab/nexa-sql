#!/usr/bin/env bash
# conn-cmd-e2e.sh — 연결(프로필 · 세션) 명령 E2E(10-01 · 사용자 "연결 관련 테스트 스크립트 — 상태 확인 · 개별 설정 · 개별 확인").
#   격리 홈(`NSQL_HOME`)에 실제 프로필을 **복사**해 돌린다 — 실제 설정·프로필은 읽기만.
#   CLI: `nsql conn list`(상태 · 유형 약어 · 암호 저장 여부) · `conn show <name>` · `conn env <name>`(보기) · `conn env <name> <유형>`(설정 + 확인 ·
#        정식·3자리 약어·별칭 입력 → 표시 = `정식 (약어)` · 사용자 10-01) ·
#        잘못된 값 거부 · `conn test <name>`(접속 확인) · `conn path`.
#   GUI(Windows · -g): 편집기 스크립트 명령(한 탭 F5 한 번) `CONNTYPE prd`·`SET CONNTYPE stg` → Output 한 줄 · `SHOW CONN` → 접속 정보 블록(유형 반영) · 잘못된 값 = 오류 줄 · 팔레트 `session.env:dev` ·
#        **세션·탭 변경 = 임시**(저장 프로필은 그대로) ·
#        `session.info` — 기동 명령으로만(키 주입 0 · docs/61 §4).
#
# 사용: scripts/conn-cmd-e2e.sh -o <출력폴더> [-n target/debug/nsql(.exe)] [-p <프로필>] [-P <실제 설정 폴더>] [-g target/debug/nexa-sql.exe]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NSQL="$ROOT/target/debug/nsql"; OUT=""; PROFILE="BISCM"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"; EXE=""
while getopts "o:n:p:P:g:" o; do case $o in o) OUT=$OPTARG;; n) NSQL=$OPTARG;; p) PROFILE=$OPTARG;; P) PROF=$OPTARG;; g) EXE=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-n cli] [-p profile] [-P real-config-dir] [-g gui-exe]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/conn-cmd-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -8 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk() { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
show() { echo "$1" | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null; }

say "=== 연결 명령 E2E · 프로필 $PROFILE · 격리 홈 $H · $(date '+%F %T')"
say "--- 1. 상태 확인(CLI)"
o=$("$NSQL" conn list 2>&1); show "$o"; chk "conn list = 프로필 목록(이름 · 방언 · 암호 · 대상)" "^$PROFILE +[a-z]+ +" "$o"
o=$("$NSQL" conn show "$PROFILE" 2>&1); show "$o"; chk "conn show = 프로필 상세" "$PROFILE" "$o"
o=$("$NSQL" conn path 2>&1); show "$o"; chk "conn path = 격리 홈 아래" "home" "$o"
o=$("$NSQL" conn env "$PROFILE" 2>&1); show "$o"; chk "conn env(보기) = 현재 유형(정식 + 약어)" "^$PROFILE: (None \(NON\)|Development \(DEV\)|Test \(TST\)|Production \(PRD\))$" "$o"
say "--- 2. 개별 설정 · 개별 확인(CLI)"
for pair in "Development:Development (DEV)" "tst:Test (TST)" "Stg:Test (TST)" "prd:Production (PRD)" "Product:Production (PRD)" "non:None (NON)"; do
  v=${pair%%:*}; want=${pair#*:}; re=$(printf '%s' "$want" | sed 's/[()]/\\&/g')
  o=$("$NSQL" conn env "$PROFILE" "$v" 2>&1); chk "conn env $PROFILE $v → 설정 = $want" "^$PROFILE: $re$" "$o"
  o=$("$NSQL" conn env "$PROFILE" 2>&1); chk "conn env $PROFILE(확인) = $want" "^$PROFILE: $re$" "$o"
done
o=$("$NSQL" conn env "$PROFILE" bogus 2>&1); chk "잘못된 유형 거부(정식·약어 목록)" "none\|non · development\|dev · test\|tst · production\|prd" "$o"
o=$("$NSQL" conn env NO_SUCH_PROFILE 2>&1); chk "없는 프로필 안내" "없습니다|not found" "$o"
say "--- 3. 접속 확인(CLI)"
o=$("$NSQL" conn test "$PROFILE" 2>&1); show "$(echo "$o" | tail -2)"; chk "conn test = 접속 성공(OK|성공|Connected)" "OK|성공|Connected|connected" "$o"

if [ -n "$EXE" ] && [ -x "$EXE" ]; then
  say "--- 4. GUI 편집기 스크립트 명령(한 탭에서 F5 한 번 · 기동 명령 · 키 주입 0)"
  F="$OUT/sess_cmds.sql"
  # 두 번째 `CONNTYPE prd` = 같은 값 → "변경 없음"(러너 경로) · 끝에 다시 prd로 두고, 두 번째 탭(`CONNTYPE prd`만)을 운영 상태에서 F5 →
  #   실행 필요 판단이 운영 2단 확인보다 먼저 = 확인 없이 "변경 없음(실행하지 않음)"(사용자 10-01).
  printf 'SHOW CONN\nCONNTYPE prd\nCONNTYPE prd\nSHOW CONN\nSET CONNTYPE stg\nSHOW CONNECTION\nCONNTYPE bogus\nCONNTYPE non\nCONNTYPE prd\n' > "$F"
  F2="$OUT/sess_noop.sql"; printf 'CONNTYPE prd\n' > "$F2"
  # 세 번째 탭 = 운영에서 `CONNTYPE tst` → 첫 F5는 2단 확인(막힘 · Output 없음) · 3초 안 두 번째 F5 = 진행(사용자 10-01).
  F3="$OUT/sess_leave.sql"; printf 'CONNTYPE tst\n' > "$F3"
  fw=$(cygpath -w "$F" 2>/dev/null || echo "$F"); fw2=$(cygpath -w "$F2" 2>/dev/null || echo "$F2"); fw3=$(cygpath -w "$F3" 2>/dev/null || echo "$F3")
  # Output은 편집기 탭별 → 탭을 바꾸기 전에 그 탭의 Output을 덤프하고 이어 검사한다(D = 1탭 · D2 = 2탭 · D3 = 3탭 첫 F5 뒤 · D4 = 끝).
  #   팔레트 경로 = 테스트 → 운영(올리기 · 바로) → 개발(운영 해제 · 첫 선택 막힘 + Output 안내 · 3초 안 재선택 진행).
  D="$OUT/gui_output.txt"; D2="$OUT/gui_output2.txt"; D3="$OUT/gui_output3.txt"; D4="$OUT/gui_output4.txt"; rm -f "$D" "$D2" "$D3" "$D4"
  NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw,@after:5000:run.all,@after:8000:output.dump:$D,@after:8500:open:$fw2,@after:9500:run.all,@after:10500:output.dump:$D2,@after:11000:open:$fw3,@after:11500:run.all,@after:12300:output.dump:$D3,@after:12600:run.all,@after:13600:session.env:prd,@after:14000:session.env:dev,@after:14800:session.env:dev,@after:15300:session.info,@after:16500:output.dump:$D4" \
    timeout -s KILL 21 "$EXE" "$PROFILE" > "$OUT/gui.stdout" 2> "$OUT/gui.stderr"
  if [ -s "$D" ] && [ -s "$D2" ] && [ -s "$D3" ] && [ -s "$D4" ]; then
    body=$(tail -n +2 "$D"; echo "--- (두 번째 탭)"; tail -n +2 "$D2"; echo "--- (세 번째 탭 · 첫 F5 뒤)"; tail -n +2 "$D3"; echo "--- (세 번째 탭 · 끝)"; tail -n +2 "$D4"); show "$body"
    b3=$(tail -n +2 "$D3"); b4=$(tail -n +2 "$D4")
    if echo "$b3" | grep -q "테스트 (TST)\|Test (TST)"; then bad "운영에서 CONNTYPE tst 첫 F5 = 막혀야 함(바뀜)" "$b3"; else ok "운영에서 CONNTYPE tst 첫 F5 = 2단 확인(변경 없음)"; fi
    chk "3초 안 두 번째 F5 = 테스트 (TST)로 변경" "서버 유형 = 테스트 \(TST\)|= Test \(TST\)" "$b4"
    chk "팔레트 테스트 → 운영(올리기) = 바로" "서버 유형 = 운영 \(PRD\)|= Production \(PRD\)" "$b4"
    chk "팔레트 운영 → 개발 첫 선택 = 2단 안내" "운영 유형을 해제합니다|Leaving the PRODUCTION type" "$b4"
    chk "팔레트 3초 안 재선택 = 개발 (DEV)" "서버 유형 = 개발 \(DEV\)|= Development \(DEV\)" "$b4"
    chk "SHOW CONN(1) = 접속 정보 블록" "접속 정보|Connection info" "$body"
    chk "CONNTYPE prd → 운영 (PRD) · 이 세션만(임시)" "서버 유형 = 운영 \(PRD\) - 이 세션만|= Production \(PRD\) - this session only" "$body"
    chk "CONNTYPE prd 두 번째(같은 값) → 이미 운영 — 변경 없음" "서버 유형이 이미 운영 \(PRD\) - 변경 없음|already Production \(PRD\)" "$body"
    chk "SHOW CONN(2) 유형 = 운영 (PRD) · 임시" "서버 유형: 운영 \(PRD\) \(임시|Server type: Production \(PRD\) \(temporary" "$body"
    chk "SET CONNTYPE stg → 테스트 (TST)" "서버 유형 = 테스트 \(TST\)|= Test \(TST\)" "$body"
    chk "SHOW CONNECTION 유형 = 테스트 (TST)" "서버 유형: 테스트 \(TST\)|Server type: Test \(TST\)" "$body"
    chk "CONNTYPE bogus = 오류 줄(정식·약어 목록)" "CONNTYPE bogus" "$body"
    chk "CONNTYPE non → 없음 (NON)" "서버 유형 = 없음 \(NON\)|= None \(NON\)" "$body"
    chk "운영 상태 탭 'CONNTYPE prd'만 F5 = 2단 확인 없이 변경 없음(실행하지 않음)" "변경 없음\(실행하지 않음\)|nothing to change \(not executed\)" "$body"
    n2=$(echo "$body" | grep -c "변경 없음\|nothing to change"); [ "$n2" -ge 2 ] && ok "변경 없음 줄 수 $n2 ≥ 2(러너 경로 + 무실행 경로)" || bad "변경 없음 줄 수 $n2 < 2" "$body"
    chk "팔레트 session.env:dev → 개발 (DEV)" "서버 유형 = 개발 \(DEV\)|= Development \(DEV\)" "$body"
    chk "팔레트 session.info 유형 = 개발 (DEV)" "서버 유형: 개발 \(DEV\)|Server type: Development \(DEV\)" "$body"
    n=$(echo "$body" | grep -c "접속 정보\|Connection info"); [ "$n" -ge 4 ] && ok "접속 정보 블록 수 $n ≥ 4" || bad "접속 정보 블록 수 $n < 4" "$body"
  else
    bad "GUI Output 덤프 없음(기동·접속 실패?)" "$(tail -3 "$OUT/gui.stderr")"
  fi
  # 세션·탭 변경은 임시(사용자 10-01) — 저장 프로필은 GUI 전의 값(2절 마지막 = 없음) 그대로.
  o=$("$NSQL" conn env "$PROFILE" 2>&1); chk "GUI 유형 지정은 임시 — 저장 프로필 그대로(None)" "^$PROFILE: None \(NON\)$" "$o"
  "$NSQL" conn env "$PROFILE" none >/dev/null 2>&1
fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
