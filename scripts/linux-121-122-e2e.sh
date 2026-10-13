#!/usr/bin/env bash
# linux-121-122-e2e.sh — 121·122차(win · 10-12~13) 기능의 Linux 첫 실행 E2E(124차 · 사용자 10-13 "이 PC 이후 개발된 기능별 테스트").
#   SQLite Local 프로필(격리 NSQL_HOME · 키 주입 0 · 기동 명령·훅만 · 61 §4)로 **덤프로 판정할 수 있는 기능**을 한 번에 돈다.
#   ① "접속 안 됨" 자리(서버 0 = 칸 0이 트리 영역 맨 위 · 121차 §5 ①) ② Ctrl+Enter 문장 사이 = 가까운 쪽(61 §1-12 · `editor.caret` + `run.statement`)
#   ③ Shift+Enter 값 보기(`grid.edit.view_value` → 값 창) + NULL/빈 문자열 3택(`cellview.null:empty` · SQLite = 빈 문자열 허용 · 87 §5-3)
#   ④ 줄 끝 라벨 Ctrl 홀드(T-337 · `ui.key:ctrl_down/up` + `editor.inline_dump` · 범위 statement = 주석 줄 포함 · document = 전부 · 111)
#   ⑤ 명령 팔레트 id 검색(`palette.query:edit.lower` → `edit.lower_case`) ⑥ 이진 파일 = 안내 탭(D-288 · `(바이너리)` · 읽기 전용 · `file.save` = 파일 불변)
#   ⑦ 큰 이진(≥ `file.async_load_mb` · `file.large_ask_mb=1`)도 선택 목록 없이 안내 탭(§28 수정) ⑧ 우클릭 메뉴 열린 중 스플리터 안 움직임(§10 · 대조군 = 메뉴 없음)
#   ⑨ 전용 세션 P 칩·헤더 "전용 N"(`CONNECT Local` · 54 §10-7) ⑩ 언어 전환 = 열린 설정 창 제목 재라벨(§13 · xdotool 창 제목 · 없으면 SKIP)
#   ⑪ 읽기 전용 탭 `ro` 프로젝트 저장·복원(§28 · 인자 `.nsql-project` · 파일이 안 생기면 SKIP).
#   덤프로 못 보는 것(그리드 열 머리 끌어 놓기 · 입력창 팝업 배타/키 우선권 · 활동 막대 아이콘 모양 · 툴팁 자리 · 거터 정렬 · 상태줄 Col)은
#   캡처(협업) 또는 사용자 실기 몫 — 보고서 끝에 목록을 적는다.
# 사용: scripts/linux-121-122-e2e.sh -o <출력> [-e target/debug/nexa-sql] [-n target/debug/nsql]
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
OUT=""; APP="$ROOT/target/debug/nexa-sql"; NSQL="$ROOT/target/debug/nsql"
while getopts "o:e:n:" o; do case $o in o) OUT=$OPTARG;; e) APP=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out> [-e app] [-n nsql]"; exit 2; }
[ -x "$APP" ] || { [ -x "$APP.exe" ] && APP="$APP.exe"; }
[ -x "$NSQL" ] || { [ -x "$NSQL.exe" ] && NSQL="$NSQL.exe"; }
[ -x "$NSQL" ] || NSQL="$ROOT/target/release/nsql"
[ -x "$APP" ] && [ -x "$NSQL" ] || { echo "실행 파일 없음: $APP / $NSQL"; exit 2; }
OUT=$(realpath -m "$OUT"); mkdir -p "$OUT"; H="$OUT/home"; rm -rf "${H:?}"; mkdir -p "$H"
P="$OUT/proj"; rm -rf "${P:?}"; mkdir -p "$P"
# 설정: 북마크 상시 라벨 끔(Ctrl 홀드만) · Ctrl 범위 = statement · 큰 파일 질문 1 MB(⑦ · 글 파일은 전부 그 아래) · 프로젝트 복원 끔.
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1375x945\nproject.restore_last=off\n' > "$H/settings.conf"
printf 'bookmark.inline_label=off\nbookmark.inline_label_ctrl_scope=statement\nfile.large_ask_mb=1\n' >> "$H/settings.conf"
NSQL_HOME="$H" "$NSQL" conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1 || { echo "Local 프로필 생성 실패"; exit 2; }
R="$OUT/report.txt"; : > "$R"; say() { echo "$*"; echo "$*" >> "$R"; }
pass=0; fail=0; skip=0
ok() { say "  PASS  $1"; pass=$((pass+1)); }; bad() { say "  FAIL  $1"; fail=$((fail+1)); }; sk() { say "  SKIP  $1"; skip=$((skip+1)); }
say "=== 121·122차 기능 Linux E2E · $(date '+%F %T') · app=$APP"
# ── 시험 자산 ─────────────────────────────────────────────────────────────────
# a.sql: 1행 주석(북마크) · 2행 SELECT 1 · 3~11행 빈 줄 · 12행 SELECT 2(북마크). 글자 오프셋 = 2행 시작 8 · 4행 26 · 10행 32 · 12행 34.
printf -- '-- 주석 줄\nSELECT 1 AS one;\n\n\n\n\n\n\n\n\n\nSELECT 2 AS two;\n' > "$P/a.sql"
printf 'CONNECT Local\nSELECT 3 AS three;\n' > "$P/b.sql"
{ printf '\x89PNG\r\n\x1a\n'; head -c 208 /dev/zero; } > "$P/x.png"            # 216 B 이진
{ printf '\x89PNG\r\n\x1a\n'; head -c $((10*1024*1024 - 8)) /dev/zero; } > "$P/x10.png"   # 10 MB 이진(≥ async_load_mb 8)
H0=$(sha256sum "$P/x.png" | cut -d' ' -f1)
NSQL_HOME="$H" "$NSQL" bookmark add "$P/a.sql" 1 --label "주석 라벨" >/dev/null 2>&1 || say "  (북마크 1 추가 실패)"
NSQL_HOME="$H" "$NSQL" bookmark add "$P/a.sql" 12 --label "둘째 라벨" >/dev/null 2>&1 || say "  (북마크 12 추가 실패)"
run_app() { # $1 = 기동 명령 · $2 = 제한 초 · $3 = 이름 · $4.. = 앱 인자
  local cmd=$1 lim=$2 name=$3; shift 3
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL "$lim" "$APP" "$@" >/dev/null 2>"$OUT/$name.stderr"
}
f1() { sed -n 1p "$1" 2>/dev/null; }
# ── ① 접속 안 됨 자리(프로필 없이 기동 · 첫 기동 = 캐시 생성이라 길게) ────────────────────────────────────────────
run_app "@after:7000:explorer.dump:$OUT/r1.txt" 10 r1
L1=$(f1 "$OUT/r1.txt")
say "  ① 탐색기 첫 행 = ${L1:-없음}"
if echo "$L1" | grep -qiE '접속|connect'; then ok "① 서버 0 = 첫 행이 '접속 안 됨' 칸(묶음 없음)"; else bad "① 첫 행이 접속 안 됨 칸이 아님: $L1"; fi
# ── ②~⑦ 한 실행(Local) ─────────────────────────────────────────────────────────────────────────────────────
C="@connected:open:$P/a.sql"
# ② 문장 사이 = 가까운 쪽: 4행(26) → SELECT 1 · 10행(32) → SELECT 2.
C="$C,@after:5000:editor.caret:26,@after:5200:run.statement,@after:6500:grid.col:$OUT/r2a.txt|one"
C="$C,@after:6800:editor.caret:32,@after:7000:run.statement,@after:8300:grid.col:$OUT/r2b.txt|two"
# ③ 값 보기 + 빈 문자열 3택.
C="$C,@after:8800:cellview.open:0;0,@after:9800:cellview.dump:$OUT/r3a.txt"
C="$C,@after:10000:cellview.null:empty,@after:10300:cellview.dump:$OUT/r3b.txt"
# ④ Ctrl 홀드 라벨: 2행 캐럿(8) = 주석 줄 포함 → '주석 라벨'만 · 12행(34) → '둘째 라벨'만 · 뗌 = 없음 · document = 둘.
C="$C,@after:10600:editor.caret:8,@after:10800:ui.key:ctrl_down,@after:11100:editor.inline_dump:$OUT/r4a.txt"
C="$C,@after:11300:editor.caret:34,@after:11600:editor.inline_dump:$OUT/r4b.txt"
C="$C,@after:11800:ui.key:ctrl_up,@after:12100:editor.inline_dump:$OUT/r4c.txt"
C="$C,@after:12300:config.set:bookmark.inline_label_ctrl_scope=document,@after:12600:ui.key:ctrl_down,@after:12900:editor.inline_dump:$OUT/r4d.txt,@after:13100:ui.key:ctrl_up"
# ⑥ 이진 안내 탭 + 저장 차단 · ⑦ 10 MB 이진 = 선택 목록 없이 안내 탭.
C="$C,@after:13400:open:$P/x.png,@after:14400:tabs.dump:$OUT/r6a.txt,@after:14600:editor.dump:$OUT/r6b.txt,@after:14800:file.save"
C="$C,@after:15200:open:$P/x10.png,@after:17200:tabs.dump:$OUT/r7.txt"
# ⑧ 스플리터 대조군 좌표용 그리드 사각형.
C="$C,@after:17500:grid.thumbs:$OUT/geo.txt"
# ⑤ 팔레트(마지막 · 열린 채 끝).
C="$C,@after:17800:view.palette,@after:18300:palette.query:edit.lower,@after:19000:palette.dump:$OUT/r5.txt"
run_app "$C" 22 main Local
A=$(f1 "$OUT/r2a.txt" | tr -d '[:space:]'); B=$(f1 "$OUT/r2b.txt" | tr -d '[:space:]')
say "  ② 4행 캐럿 → $A · 10행 캐럿 → $B"
[ "$A" = 1 ] && ok "② 문장 사이(앞에 가까움) = 앞 문장" || bad "② 4행 캐럿 결과 = '$A' (기대 1)"
[ "$B" = 2 ] && ok "② 문장 사이(뒤에 가까움) = 뒤 문장" || bad "② 10행 캐럿 결과 = '$B' (기대 2)"
R3A=$(f1 "$OUT/r3a.txt"); R3B=$(f1 "$OUT/r3b.txt")
say "  ③ 값 창 = ${R3A:-없음}"; say "  ③ 빈 문자열 뒤 = ${R3B:-없음}"
echo "$R3A" | grep -q 'mode=Text' && ok "③ cellview.open(= Shift+Enter 길) = 값 창 열림(Text)" || bad "③ 값 창 덤프 없음/형식 다름"
echo "$R3B" | grep -q 'null=Empty' && ok "③ SQLite 값 창 빈 문자열 상태(cellview.null:empty)" || bad "③ 빈 문자열 상태 안 됨: $R3B"
for f in r4a r4b r4c r4d; do say "  ④ $f = $(tr '\n' ' ' < "$OUT/$f.txt" 2>/dev/null)"; done
grep -q '주석 라벨' "$OUT/r4a.txt" && ! grep -q '둘째' "$OUT/r4a.txt" && ok "④ Ctrl 홀드 · statement = 캐럿 문장 + 위 주석 줄 북마크만" || bad "④ statement 범위(2행) 라벨 불일치"
grep -q '둘째 라벨' "$OUT/r4b.txt" && ! grep -q '주석' "$OUT/r4b.txt" && ok "④ Ctrl 홀드 · 캐럿 이동 = 그 문장 라벨만" || bad "④ statement 범위(12행) 라벨 불일치"
[ ! -s "$OUT/r4c.txt" ] && ok "④ Ctrl 뗌 = 라벨 없음" || bad "④ Ctrl 뗀 뒤에도 라벨 남음"
grep -q '주석 라벨' "$OUT/r4d.txt" && grep -q '둘째 라벨' "$OUT/r4d.txt" && ok "④ 범위 document(config.set 즉시) = 두 라벨" || bad "④ document 범위 라벨 불일치"
grep -q 'edit.lower_case' "$OUT/r5.txt" 2>/dev/null && ok "⑤ 팔레트 id 검색 edit.lower → edit.lower_case" || bad "⑤ 팔레트 id 검색 실패: $(head -c 200 "$OUT/r5.txt" 2>/dev/null)"
T6=$(grep -F 'x.png' "$OUT/r6a.txt" 2>/dev/null | head -1); say "  ⑥ 탭 = ${T6:-없음} · 편집기 머리 = $(f1 "$OUT/r6b.txt")"
echo "$T6" | grep -qE '\((바이너리|binary)\)' && ok "⑥ 이진 파일 = '(바이너리)' 안내 탭" || bad "⑥ 안내 탭 없음: $T6"
echo "$T6" | awk -F'|' '{print $6}' | grep -q true && ok "⑥ 안내 탭 = 읽기 전용" || bad "⑥ 안내 탭 읽기 전용 아님: $T6"
[ "$(sha256sum "$P/x.png" | cut -d' ' -f1)" = "$H0" ] && ok "⑥ file.save = 원본 불변(읽기 전용 저장 차단)" || bad "⑥ file.save 뒤 원본이 바뀜"
T7=$(grep -F 'x10.png' "$OUT/r7.txt" 2>/dev/null | head -1); say "  ⑦ 탭 = ${T7:-없음}"
echo "$T7" | grep -qE '\((바이너리|binary)\)' && ok "⑦ 10 MB 이진(large_ask_mb=1) = 선택 목록 없이 안내 탭" || bad "⑦ 10 MB 이진 안내 탭 없음(선택 목록에 막힘?)"
# ── ⑧ 메뉴 열린 중 스플리터(대조군 먼저 · 자기 보정) ─────────────────────────────────────────────────────────────
BB=$(grep '^bounds=' "$OUT/geo.txt" 2>/dev/null | cut -d= -f2); IFS=, read -r BX BY BW BH <<< "${BB:-,,,}"
if [ -n "${BX:-}" ] && [ -n "${BY:-}" ]; then
  CX=$((BX + BW / 2)); SY=$((BY - 6))
  base="@connected:open:$P/a.sql,@after:5000:run.all,@after:6500:grid.thumbs:$OUT/g0.txt"
  drag="ui.down:$CX/$SY,@after:8400:ui.move:$CX/$((SY - 40)),@after:8600:ui.move:$CX/$((SY - 90)),@after:8800:ui.up:$CX/$((SY - 90)),@after:9200:grid.thumbs:$OUT/g1.txt"
  run_app "$base,@after:8200:$drag" 12 r8ctl Local
  G0=$(grep '^bounds=' "$OUT/g0.txt" | cut -d= -f2 | cut -d, -f2); G1=$(grep '^bounds=' "$OUT/g1.txt" | cut -d= -f2 | cut -d, -f2)
  say "  ⑧ 대조군(메뉴 없음) 그리드 y: $G0 → $G1"
  if [ -n "$G0" ] && [ -n "$G1" ] && [ "$G1" -lt "$G0" ]; then
    run_app "$base,@after:7600:ui.rclick:$CX/$((SY - 24)),@after:8200:$drag" 12 r8 Local
    M0=$(grep '^bounds=' "$OUT/g0.txt" | cut -d= -f2 | cut -d, -f2); M1=$(grep '^bounds=' "$OUT/g1.txt" | cut -d= -f2 | cut -d, -f2)
    say "  ⑧ 메뉴 열린 채 그리드 y: $M0 → $M1"
    [ "$M0" = "$M1" ] && ok "⑧ 우클릭 메뉴 열린 중 스플리터 끌기 = 제자리" || bad "⑧ 메뉴 열린 중에도 스플리터가 움직임($M0 → $M1)"
  else sk "⑧ 대조군에서 스플리터가 안 움직임(y $SY가 스플리터 아님) — 좌표 보정 필요"; fi
else sk "⑧ 그리드 사각형을 못 얻음"; fi
# ── ⑨ 전용 세션 P 칩 ─────────────────────────────────────────────────────────────────────────────────────
run_app "@connected:open:$P/b.sql,@after:5000:run.all,@after:7500:explorer.header:$OUT/r9.txt,@after:7700:log.dump:$OUT/r9log.txt" 10 r9 Local
say "  ⑨ 헤더 = $(tr '\n' ' ' < "$OUT/r9.txt" 2>/dev/null)"
grep -qE '전용 1|private 1' "$OUT/r9.txt" 2>/dev/null && ok "⑨ CONNECT 전용 세션 = 헤더 '연결 1개 · 전용 1'" || bad "⑨ 헤더에 전용 수 없음(로그 전용 줄 = $(grep -c '전용 세션' "$OUT/r9log.txt" 2>/dev/null))"
# ── ⑩ 언어 전환 = 설정 창 제목 재라벨(xdotool) ─────────────────────────────────────────────────────────────────
if command -v xdotool >/dev/null 2>&1 && [ -n "${DISPLAY:-}" ]; then
  ( run_app "@after:5000:edit.prefs:,@after:9000:config.set:ui.lang=en,@after:14000:config.set:ui.lang=ko" 19 r10 ) &
  sleep 7.5; T_KO1=$(xdotool search --name '^Nexa SQL - 환경 설정' 2>/dev/null | wc -l)
  sleep 4;   T_EN=$(xdotool search --name '^Nexa SQL - Preferences' 2>/dev/null | wc -l)
  sleep 5;   T_KO2=$(xdotool search --name '^Nexa SQL - 환경 설정' 2>/dev/null | wc -l)
  wait
  say "  ⑩ 설정 창 제목: ko=$T_KO1 → en=$T_EN → ko=$T_KO2"
  if [ "$T_KO1" -ge 1 ] && [ "$T_EN" -ge 1 ] && [ "$T_KO2" -ge 1 ]; then ok "⑩ 언어 전환 = 열린 설정 창 제목 즉시 재라벨(ko→en→ko)";
  elif [ "$T_KO1" -eq 0 ] && [ "$T_EN" -eq 0 ]; then sk "⑩ 설정 창을 xdotool로 못 찾음(제목 꼴 확인 필요)"; else bad "⑩ 창 제목 재라벨 안 됨(ko=$T_KO1 en=$T_EN ko=$T_KO2)"; fi
else sk "⑩ xdotool/DISPLAY 없음"; fi
# ── ⑪ 읽기 전용 탭 ro 저장·복원(프로젝트 인자) ──────────────────────────────────────────────────────────────────
PJ="$P/t.nsql-project"
run_app "@after:5000:open:$P/x.png,@after:7000:project.save,@after:8500:tabs.dump:$OUT/r11a.txt" 11 r11a "$PJ"
if [ -s "$PJ" ]; then
  run_app "@after:7000:tabs.dump:$OUT/r11b.txt" 10 r11b "$PJ"
  T11=$(grep -F 'x.png' "$OUT/r11b.txt" 2>/dev/null | head -1); say "  ⑪ 복원 탭 = ${T11:-없음}"
  echo "$T11" | grep -qE '\((바이너리|binary)\)' && echo "$T11" | awk -F'|' '{print $6}' | grep -q true && ok "⑪ 이진 안내 탭 = 프로젝트 저장 → 복원(ro)" || bad "⑪ 복원 뒤 안내 탭/읽기 전용 아님: $T11"
else sk "⑪ 프로젝트 파일이 안 생김($PJ · 인자 경로가 새 프로젝트를 만들지 않음) — 사용자 실기"; fi
# ── 패닉 ─────────────────────────────────────────────────────────────────────────────────────────────
if grep -qi panic "$OUT"/*.stderr 2>/dev/null; then bad "패닉 발생: $(grep -il panic "$OUT"/*.stderr | tr '\n' ' ')"; else ok "패닉 없음(전 실행)"; fi
say ""; say "== 합계: 통과 $pass · 실패 $fail · 건너뜀 $skip  ($(date '+%F %T'))"
say "== 덤프로 못 보는 것(캡처·실기 몫): 열 머리 끌어 놓기(편집기·필터 상자 삽입) · 입력창 팝업 배타·이력 토글·키 우선권 · 행수 상자 메뉴 위로 ·"
say "   활동 막대 아이콘 모양 · 우클릭 메뉴 아이콘 칸 · 툴팁 자리(버튼 위·헤더 아래) · 거터 세로 정렬 · 패널 오른쪽 테두리 · 상태줄 Col 시각 열 ·"
say "   Where Aa(단위 시험 있음) · 날짜 '지금'(Ctrl+;) · 이진 미리보기 한 번 클릭 · 상태줄 전용 접두 · Thin GUI 전환 토스트(= oracle-driver-parity-e2e.sh CLI)"
[ "$fail" = 0 ]
