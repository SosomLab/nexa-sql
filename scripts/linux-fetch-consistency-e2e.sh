#!/usr/bin/env bash
# linux-fetch-consistency-e2e.sh — 페치 일관성 E2E(10-11 · 사용자 "Count 된 행보다 더 많이 조회되는 것 같다" · docs/43 §9).
#   실서버(저장된 프로필 · 읽기만 · 임시 객체 0 · 키 주입 0 · 기동 명령만 · 61 §4)에서 큰 결과(기본 APPDB · M4S_I002040 · PROJECT_CD='ACME' · 12만 행)를
#   세 순서로 받고 **ROWID 열 전부**를 파일로 떠서(훅 `grid.col`) ① 행 수 = 서버 COUNT(*) ② 중복 0 ③ 누락 0(= ①∧②)을 판정한다.
#     A 정상: 200행 → ⇊ 전체 조회 → 끝까지.
#     B 사용자 순서: 200행 → ⇊ 전체 조회 → 중간 ■ → 스크롤 끝(자동 페이지 = 커서 없음·ORDER BY 없음 → 엄격 재질의·교체) → ⇊ 전체 조회 → 끝까지.
#     C ORDER BY: B와 같되 ORDER BY ROWID(커서 없음 → OFFSET 페이지 · 교체 없음).
# 사용: scripts/linux-fetch-consistency-e2e.sh -o <출력> [-P <실제 설정 폴더>] [-p APPDB] [-t "M4S_I002040 A WHERE 1=1 AND A.PROJECT_CD = 'ACME'"]
#        [-e target/debug/nexa-sql] [-n target/release/nsql] [-w <전체 조회 대기 s · 기본 70>] [-O A,B,C]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역(130차 mac)
OUT=""; PROF="${APPDATA:-$HOME/.config}/nexa-sql"; P=APPDB; FROM="M4S_I002040 A WHERE 1=1 AND A.PROJECT_CD = 'ACME'"
APP="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/release/nsql"; WAIT=70; ONLY=""
while getopts "o:P:p:t:e:n:w:O:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; p) P=$OPTARG;; t) FROM=$OPTARG;; e) APP=$OPTARG;; n) NSQL=$OPTARG;; w) WAIT=$OPTARG;; O) ONLY=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out> [-P real-config] [-p profile] [-t \"<FROM … WHERE …>\"] [-e app] [-n nsql] [-w secs] [-O A,B,C]"; exit 2; }
# Windows(Git Bash)에서는 기본 경로에 `.exe`가 붙는다(121차 win · 협업 실측 = 인자만 주면 수정 없이 돈다).
[ -x "$APP" ] || { [ -x "$APP.exe" ] && APP="$APP.exe"; }
[ -x "$NSQL" ] || { [ -x "$NSQL.exe" ] && NSQL="$NSQL.exe"; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\n' > "$H/settings.conf"
R="$OUT/report.txt"; : > "$R"; say() { echo "$*"; echo "$*" >> "$R"; }
pass=0; fail=0; ok() { say "  PASS  $1"; pass=$((pass+1)); }; bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -6 | sed 's/^/        /' | tee -a "$R" >/dev/null; }
# ★ 10-24(138차): 서버 쪽 연결 리셋(os error 104 · "서버 연결 끊김")은 제품 판정이 아니라 환경 → ENV로 따로 세고 합계에서 실패로 치지 않는다(V3 D17 첫 회 4/4 오판 방지).
env=0; envc() { say "  ENV   $1"; env=$((env+1)); [ -n "${2:-}" ] && echo "$2" | head -4 | sed 's/^/        /' | tee -a "$R" >/dev/null; }
ENV_RE='Connection reset by peer|os error 104|Server connection lost|서버 연결 끊김|Broken pipe'
say "=== 페치 일관성 E2E · 프로필 $P · FROM $FROM · $(date '+%F %T')"
# 기대값 = 서버 COUNT(*)(DISTINCT ROWID와 같아야 한다).
printf 'SELECT COUNT(*) AS CNT, COUNT(DISTINCT ROWID) AS RIDS FROM %s;\n' "$FROM" > "$OUT/cnt.sql"
CNT=$(timeout 60 "$NSQL" run -c "$P" "$OUT/cnt.sql" 2>/dev/null | grep -oE '^[0-9]+\s+[0-9]+' | head -1 | awk '{print $1}')
[ -n "$CNT" ] || { say "  서버 건수 조회 실패 — 중단"; exit 2; }
say "  서버 COUNT(*) = $CNT"
# 시나리오 실행: $1 id · $2 설명 · $3 SQL · $4 기동 명령 체인(상대 ms) · $5 총 대기 s
run_case() {
  local id=$1 title=$2 sql=$3 chain=$4 secs=$5
  if [ -n "$ONLY" ] && ! echo ",$ONLY," | grep -q ",$id,"; then return; fi
  say "--- $id $title"
  printf '%s\n' "$sql" > "$OUT/$id.sql"; rm -f "$OUT/$id.rid.txt" "$OUT/$id.grid.txt" "$OUT/$id.log.txt"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$OUT/$id.sql,$chain,@after:$((secs*1000-1500)):grid.col:$OUT/$id.rid.txt|RID,@after:$((secs*1000-1000)):grid.dump:$OUT/$id.grid.txt,@after:$((secs*1000-500)):log.dump:$OUT/$id.log.txt" \
    timeout -s KILL "$((secs+4))" "$APP" "$P" > "$OUT/$id.stdout" 2> "$OUT/$id.stderr"
  # 환경 판정 먼저: 실행 로그·stderr에 연결 리셋이 있으면 이 시나리오는 ENV(재실행 대상) — 행 수·중복 판정은 뜻이 없다.
  if cat "$OUT/$id.log.txt" "$OUT/$id.stderr" 2>/dev/null | grep -qE "$ENV_RE"; then envc "$id 서버 연결 리셋(환경 · 재실행 대상)" "$(cat "$OUT/$id.log.txt" "$OUT/$id.stderr" 2>/dev/null | grep -E "$ENV_RE" | head -2)"; return; fi
  if [ ! -s "$OUT/$id.rid.txt" ]; then bad "$id ROWID 덤프 없음(결과 없음 또는 시간 부족)" "$(tail -3 "$OUT/$id.stderr")"; return; fi
  local n dup more
  n=$(grep -c '' "$OUT/$id.rid.txt"); dup=$(sort "$OUT/$id.rid.txt" | uniq -d | wc -l); more=$(grep -oE 'more=(true|false)' "$OUT/$id.grid.txt" | head -1)
  say "        행 $n · 중복 ROWID $dup · $more · 로그: $(grep -cE '재질의:|커서에서 .*이어 받음' "$OUT/$id.log.txt") 페이지 줄 · $(grep -c '취소하는 중' "$OUT/$id.log.txt") 취소"
  [ "$n" = "$CNT" ] && ok "$id 행 수 = COUNT(*) ($n)" || bad "$id 행 수 $n ≠ COUNT(*) $CNT (차이 $((n-CNT)))"
  [ "$dup" = 0 ] && ok "$id 중복 ROWID 없음" || bad "$id 중복 ROWID ${dup}건" "$(sort "$OUT/$id.rid.txt" | uniq -d | head -3)"
  [ "$more" = "more=false" ] && ok "$id 더 없음(끝까지 받음)" || bad "$id more 상태 = $more"
  # 재질의 확인 토스트(10-12 ④): 덤프 파일이 있으면 버튼 동작 `grid.refetch:<탭>:<offset>:<limit>`이 있어야 한다.
  if [ -f "$OUT/$id.toast.txt" ]; then
    if grep -q '|grid.refetch:' "$OUT/$id.toast.txt"; then ok "$id 재질의 확인 토스트 + [계속] 버튼($(grep -o 'grid.refetch:[0-9:]*' "$OUT/$id.toast.txt" | head -1))"; else bad "$id 재질의 확인 토스트 없음" "$(cat "$OUT/$id.toast.txt")"; fi
  fi
}
Q="SELECT ROWID AS RID, A.* FROM $FROM"
QO="SELECT ROWID AS RID, A.* FROM $FROM ORDER BY ROWID"
# A: 200 → 전체 조회 → 끝.
run_case A "정상 전체 조회(⇊ 끝까지)" "$Q;" "@after:3000:run.all,@after:6000:grid.fetch_all" "$WAIT"
# B: 200 → 전체 조회 → 11 s에 ■ → 15 s 스크롤 끝(자동 페이지 → 엄격 재질의·교체) → 재질의가 끝날 만큼 기다린 뒤 다시 ⇊ → 끝.
# ★ 10-12 ④ 재질의 확인(`grid.refetch_confirm` 기본 on): 스크롤 끝의 자동 페이지가 엄격 교체가 될 참이면 토스트로 묻는다 →
#   17 s에 토스트 덤프(버튼 동작 `grid.refetch:`가 있어야) → `toast.action` = [계속] → 재질의 진행 → 30 s ⇊.
run_case B "사용자 순서(전체 조회 → ■ → 스크롤 끝 → 확인 토스트 [계속] → 재질의 → ⇊)" "$Q;" "@after:3000:run.all,@after:6000:grid.fetch_all,@after:11000:run.stop,@after:15000:grid.vdrag:3000,@after:17000:toast.dump:$OUT/B.toast.txt,@after:17500:toast.action,@after:30000:grid.fetch_all" "$((WAIT+40))"
# C: ORDER BY ROWID — 커서 없음이면 OFFSET 페이지(교체 없음).
run_case C "ORDER BY ROWID(■ → 스크롤 끝 = OFFSET 페이지 → ⇊)" "$QO;" "@after:3000:run.all,@after:6000:grid.fetch_all,@after:11000:run.stop,@after:15000:grid.vdrag:3000,@after:22000:grid.fetch_all" "$((WAIT+30))"
say ""; say "== 합계: 통과 $pass · 실패 $fail · 환경 $env  ($(date '+%F %T'))"
[ "$env" = 0 ] || say "   환경 $env = 서버 연결 리셋 — 제품 판정 아님 · 같은 시나리오를 -O로 재실행"
[ "$fail" = 0 ]
