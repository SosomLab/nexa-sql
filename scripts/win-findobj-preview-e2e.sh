#!/usr/bin/env bash
# win-findobj-preview-e2e.sh — 객체 찾기 **미리보기 반응**(검색 중 · 마우스 클릭 vs 방향키) 계측 시나리오(docs/113 §5-4 · 10-16).
#   사용자 지시: "APPDB 프로필로 연결해 ITEM_CD로 조회한 뒤 View/Procedure가 검색 중에 이동되는지 확인하고 마우스 클릭과 방향키 이동의 차이를 확인"
#   맹점 보완(사용자 "컨트롤이 받는 사건만 재면 같게 측정된다"): 클릭은 **OS 사건과 같은 길**(`findobj_window_event` = CursorMoved+Pressed+Released)로
#   합성해 창 안 모든 분기를 지나고(`consumed=`로 누가 먹었나 판정) · 계측 사슬 = `[preview] <src> · os-wait · event→intent · queued · handled` →
#   `[preview] <src> · same tab|new object · event→text` → `[preview] <src> · painted · event→frame`(log.dump).
#   격리 홈 + 실제 프로필 **사본**(-P · 실제 폴더는 건드리지 않음) · 키 주입 0 · 실서버는 읽기만(소스 조회).
# 사용: scripts/win-findobj-preview-e2e.sh -o <출력폴더> -P <실제 NSQL_HOME> [-c APPDB] [-q ITEM_CD] [-k View] [-g exe] [-n nsql]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역(130차 mac)
OUT=""; PROF=""; CONN="APPDB"; Q="ITEM_CD"; KIND="View"; MINSRC=2
EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:P:c:q:k:m:g:n:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; c) CONN=$OPTARG;; q) Q=$OPTARG;; k) KIND=$OPTARG;; m) MINSRC=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out dir> -P <real NSQL_HOME> [-c profile] [-q query] [-k View|Procedure] [-g gui] [-n nsql]"; exit 2; }
mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd)"
REPORT="$OUT/findobj-preview-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -14 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
# 실제 프로필·기기 키 **복사**(실제 폴더는 그대로) → 이름 접속.
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1300,850\nfindobj.preview_click=on\nlog.dev_mode=on\n' > "$H/settings.conf"
printf -- '-- doc\n' > "$OUT/doc.sql"
gui() { # <이름> <명령> <초>
  local name=$1 cmd=$2 secs=$3
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL "$secs" "$EXE" "$CONN" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
}
A() { echo "@connected:@after:$1:$2"; } # 접속 기준 지연(ms) 명령
hdr() { head -1 "$OUT/$1" 2>/dev/null; }
geom() { grep -m1 '^consumed=' "$OUT/$1" 2>/dev/null; } # consumed=… 줄(종전 = 4행 고정 → 10-17 `present=` 줄이 4행에 끼어 131차 V3 A-2/A-3 헛실패 → 접두로 찾는다)
caret() { head -1 "$OUT/$1" 2>/dev/null | sed -n 's/.*caret=\([0-9]*\).*/\1/p'; }
field() { echo "$1" | sed -n "s/.*$2 \([0-9-]*\) ms.*/\1/p"; }

say "findobj-preview-e2e $(date '+%F %T') · conn=$CONN q=$Q kind=$KIND"
gui warm "@after:1500:findobj.open" 4

# ── 시나리오 A: 검색 **중**(현재 스키마 결과가 먼저 들어온 뒤 · 다른 스키마·소스 작업은 아직 진행) 클릭 → 키 ↓ → 키 ↓.
#   +2.5 s 창 열기 · +4.0 s 검색 · +7.0 s 종류 행 선택(소스 줄 있는 첫 View) · +7.3 s 소스 1번째 줄 **클릭 합성** · +9.0 s 덤프/캐럿
#   · +9.5 s ↓ · +11.0 s 덤프/캐럿 · +11.5 s ↓ · +13 s 덤프 · +13.2 s 로그 · +35 s 최종(검색 끝 판정).
# ★ 10-24(138차 Linux V3 D10): 종전 `$D\\s1.txt`(Git Bash 백슬래시)는 Linux에서 글자 그대로 `\`가 돼 덤프가 안 생겼다 → 구분자 `/`(Windows 파일 API도 `/` 허용 · cygpath -w 결과 뒤에 붙여도 된다).
D="$(w "$OUT")"
CMD="@connected:open:$(w "$OUT/doc.sql"),$(A 2500 findobj.open),$(A 4000 "findobj.query:$Q"),$(A 7000 "findobj.selkind:$KIND:$MINSRC"),$(A 7200 "status.dump:$D/s1.txt")"
CMD="$CMD,$(A 7300 findobj.clickpre:src),$(A 7400 "status.dump:$D/s2.txt"),$(A 9000 "findobj.dump:$D/d1.dump"),$(A 9100 "editor.dump:$D/e1.txt")"
CMD="$CMD,$(A 9500 findobj.key:down),$(A 11000 "findobj.dump:$D/d2.dump"),$(A 11100 "editor.dump:$D/e2.txt"),$(A 11200 "status.dump:$D/s3.txt")"
CMD="$CMD,$(A 11500 findobj.key:down),$(A 13000 "editor.dump:$D/e3.txt"),$(A 13200 "log.dump:$D/log1.txt"),$(A 35000 "findobj.dump:$D/d9.dump")"
gui a "$CMD" 43

s1="$(cat "$OUT/s1.txt" 2>/dev/null)"; s2="$(cat "$OUT/s2.txt" 2>/dev/null)"; c1=""   # c1 = A 덤프 없을 때도 정의(set -u · D10 "바인딩 해제한 변수")
say "  info  selkind → $s1"
say "  info  clickpre → $s2"
case "$s1" in *"row=Some"*) ok "A-0 $KIND(소스 줄 있음) 행 선택" ;; *) bad "A-0 $KIND 행 없음(검색 결과에 소스 일치 ${KIND}가 아직/전혀 없음)" "$(hdr d1.dump)" ;; esac
# 클릭 줄 = 창 덤프 `last_opened=Some((행, 줄))`(상태줄은 "Source of … opened"로 곧 덮인다 · 협업 16차).
line1="$(geom d1.dump | sed -n 's/.*last_opened=Some((\([0-9]*\), \([0-9]*\))).*/\2/p')"
nsrc="$(grep -c '^pre|[1-9]' "$OUT/d1.dump" 2>/dev/null)"; nsrc="${nsrc:-0}"
if [ -f "$OUT/d1.dump" ]; then
  hdr d1.dump | grep -q "running=true" && ok "A-1 클릭 시점 = 검색 중(running=true)" || say "  info  A-1 클릭 시점에 검색이 이미 끝남(running=false) — 검색 중 판정은 못 함"
  g="$(geom d1.dump)"; say "  info  $g"
  case "$g" in consumed=preview*) ok "A-2 합성 클릭을 **미리보기 줄**이 받음(다른 컨트롤이 먼저 먹지 않음)" ;; *) bad "A-2 클릭을 다른 분기가 먹음" "$g" ;; esac
  c1="$(caret e1.txt)"
  if [ -n "$line1" ] && [ "$c1" = "$line1" ]; then ok "A-3 클릭 → 편집기 캐럿 = 클릭한 소스 줄($line1)"; else bad "A-3 클릭 뒤 캐럿($c1) ≠ 클릭 줄($line1)" "$(head -2 "$OUT/e1.txt")"; fi
  hdr e1.txt | grep -q "|Preview|" && ok "A-3b 미리보기 탭이 열림(종류 Preview)" || bad "A-3b 탭 종류" "$(hdr e1.txt)"
else
  bad "A 덤프 없음" "$(tail -4 "$OUT/a.stderr")"
fi
c2="$(caret e2.txt)"; c3="$(caret e3.txt)"
if [ -n "$c2" ] && [ -n "$c1" ] && [ "$c2" -gt "$c1" ] 2>/dev/null; then ok "A-4 ↓ 한 번 → 캐럿 다음 소스 줄($c1 → $c2)"; else bad "A-4 ↓ 뒤 캐럿($c2)이 앞으로 가지 않음(앞 $c1)" "$(cat "$OUT/s3.txt" 2>/dev/null)"; fi
if [ "$nsrc" -ge 3 ] 2>/dev/null; then
  if [ -n "$c3" ] && [ "$c3" -gt "$c2" ] 2>/dev/null; then ok "A-5 ↓ 두 번째 → 캐럿($c2 → $c3)"; else bad "A-5 ↓ 두 번째 캐럿($c3)" ""; fi
else
  [ "$c3" = "$c2" ] && ok "A-5 소스 줄 ${nsrc}개 = 마지막 줄에서 ↓ 유지($c3)" || bad "A-5 마지막 줄 ↓ 뒤 캐럿($c2 → $c3)" ""
fi
# 같은 객체·줄 이동 = 재조회 없이 같은 탭(새 객체 요청은 첫 클릭 1회만).
if [ -f "$OUT/log1.txt" ]; then
  n_new="$(grep -c "\[preview\] .*new object" "$OUT/log1.txt")"; n_same="$(grep -c "\[preview\] .*same tab" "$OUT/log1.txt")"
  [ "$n_new" = "1" ] && ok "A-6 새 객체 조회 = 1회(클릭) · 같은 탭 이동 = ${n_same}회(키)" || bad "A-6 새 객체 조회 ${n_new}회(기대 1)" "$(grep "\[preview\]" "$OUT/log1.txt")"
  want_same=$(( nsrc >= 3 ? 2 : 1 ))
  [ "$n_same" -ge "$want_same" ] 2>/dev/null && ok "A-7 ↓ = 같은 탭 이동 ${n_same}회(기대 ≥${want_same} · 소스 줄 ${nsrc})" || bad "A-7 같은 탭 이동 ${n_same}회(기대 ≥${want_same})" "$(grep "\[preview\]" "$OUT/log1.txt")"
  # 계측표: 클릭 vs 키 — 각 단계 ms(OS 큐 대기 · 사건→의도 · 의도→수거 · 처리 · 사건→본문 · 사건→첫 프레임).
  say "  ── 계측(ms) ── src | os-wait | event→intent | queued | handled | event→text | event→frame"
  for src in click key; do
    while IFS= read -r l; do
      [ -n "$l" ] || continue
      osw="$(field "$l" os-wait)"; ei="$(field "$l" "event→intent")"; qd="$(field "$l" queued)"; hd="$(field "$l" handled)"
      say "  $src | $osw | $ei | $qd | $hd"
    done <<< "$(grep "\[preview\] $src · os-wait" "$OUT/log1.txt")"
    grep "\[preview\] $src · .*event→text" "$OUT/log1.txt" | sed 's/^/  /' | tee -a "$REPORT" >/dev/null
    grep "\[preview\] $src · painted" "$OUT/log1.txt" | sed 's/^/  /' | tee -a "$REPORT" >/dev/null
  done
  grep "\[source\]" "$OUT/log1.txt" | sed 's/^/  /' | tee -a "$REPORT" >/dev/null
  # 판정선: 클릭의 사건→본문(새 객체 = 서버 왕복 포함)이 2 s 안 · 키(같은 탭)의 사건→본문이 100 ms 안 · 클릭 경로의 창 안 처리(event→intent)가 50 ms 안.
  ct="$(grep "\[preview\] click · new object" "$OUT/log1.txt" | head -1 | sed -n 's/.*event→text \([0-9]*\) ms.*/\1/p')"
  kt="$(grep "\[preview\] key · same tab" "$OUT/log1.txt" | head -1 | sed -n 's/.*event→text \([0-9]*\) ms.*/\1/p')"
  ci="$(grep "\[preview\] click · os-wait" "$OUT/log1.txt" | head -1 | sed -n 's/.*event→intent \([0-9]*\) ms.*/\1/p')"
  [ -n "$ct" ] && [ "$ct" -lt 2000 ] && ok "A-8 클릭 사건→본문 ${ct} ms < 2000(새 객체 · 서버 왕복 포함)" || bad "A-8 클릭 사건→본문(${ct:-없음})" ""
  [ -n "$kt" ] && [ "$kt" -lt 100 ] && ok "A-9 키 사건→본문 ${kt} ms < 100(같은 탭)" || bad "A-9 키 사건→본문(${kt:-없음})" ""
  [ -n "$ci" ] && [ "$ci" -lt 50 ] && ok "A-10 클릭 창 안 처리(사건→의도) ${ci} ms < 50 = 클릭에 지연 장치 없음" || bad "A-10 클릭 사건→의도(${ci:-없음})" ""
  # ★ 화면 기준(10-16): 검색이 도는 동안 메인 창이 실제로 그려졌는가 — 모델만 보면 놓치는 "미리보기 탭이 안 보임"(WM_PAINT 굶김).
  pf="$(grep "\[preview\] click · painted" "$OUT/log1.txt" | head -1 | sed -n 's/.*event→frame \([0-9]*\) ms.*/\1/p')"
  [ -n "$pf" ] && [ "$pf" -lt 2500 ] && ok "A-12 클릭 뒤 메인 창 첫 프레임 ${pf} ms(검색 중 화면 갱신)" || bad "A-12 검색 중 메인 창이 그려지지 않음(painted 없음 = WM_PAINT 굶김)" "$(grep "\[preview\]" "$OUT/log1.txt")"
else
  bad "A 로그 없음" ""
fi
[ -f "$OUT/d9.dump" ] && { hdr d9.dump | grep -q "running=false" && ok "A-11 검색 완료(35 s 안)" || say "  info  A-11 35 s에도 검색 중: $(hdr d9.dump)"; }

# ── 시나리오 B(대조): 검색이 **끝난 뒤** 같은 동작 — 같은 객체를 클릭 → ↓ 로 열어 검색 중/뒤 차이를 표로.
CMD="@connected:open:$(w "$OUT/doc.sql"),$(A 2500 findobj.open),$(A 4000 "findobj.query:$Q"),$(A 30000 "findobj.selkind:$KIND:$MINSRC"),$(A 30300 findobj.clickpre:src)"
CMD="$CMD,$(A 32000 "editor.dump:$D/be1.txt"),$(A 32500 findobj.key:down),$(A 34000 "editor.dump:$D/be2.txt"),$(A 34200 "log.dump:$D/log2.txt"),$(A 34300 "findobj.dump:$D/bd.dump")"
gui b "$CMD" 42
if [ -f "$OUT/log2.txt" ]; then
  hdr bd.dump | grep -q "running=false" && ok "B-0 대조 = 검색 끝난 뒤" || say "  info  B-0 아직 검색 중"
  bc1="$(caret be1.txt)"; bc2="$(caret be2.txt)"
  [ -n "$bc1" ] && [ -n "$bc2" ] && [ "$bc2" -gt "$bc1" ] 2>/dev/null && ok "B-1 클릭($bc1) → ↓($bc2)" || bad "B-1 캐럿 $bc1 → $bc2" ""
  say "  ── 대조(검색 뒤) ──"
  grep "\[preview\]\|\[source\]" "$OUT/log2.txt" | sed 's/^/  /' | tee -a "$REPORT" >/dev/null
else
  bad "B 로그 없음" "$(tail -4 "$OUT/b.stderr")"
fi

say "결과: PASS $pass · FAIL $fail"
[ "$fail" = "0" ]
