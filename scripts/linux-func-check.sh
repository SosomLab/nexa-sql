#!/usr/bin/env bash
# linux-func-check.sh — 기능 점검 자동화 **Linux판**(win-func-check.ps1 이식 · 사용자 09-26 "전체 개발 기능 리눅스 동작 전수 검사").
#   ★ macOS에서도 그대로 돈다(10-10 · 창 수 = CGWindowList PID 계수 · 나머지 판정 동일) — 맥 기능 점검 = 이 스크립트.
#   시나리오마다 앱을 격리 홈으로 띄우고(`NSQL_STARTUP_CMD`로만 몬다 · OS 키·마우스 주입 0 · docs/61 §4) **자동 판정**한다.
#   Windows판과 다른 점: 이 VM에는 창 캡처 도구가 없다(import·scrot·xdotool 전부 없음) → 캡처 대신
#     ① 프로세스 생존 ② 창 수(`xwininfo -root -tree`의 WM_CLASS `nexa-sql`) ③ stderr 패닉 없음
#     ④ **덤프 명령의 산출 파일 검사**(explorer/grid/details/cellview/txlog/mem/import/sqlprev.dump) ⑤ 저장 파일 검사
#   로 판정한다 — 눈으로 봐야 하는 것(색·툴팁 위치)은 사용자 실기(T-163)로 넘긴다.
#   ★ 10-24(138차 Linux): Windows판에만 있던 28 시나리오(S36·S37·S40·S49·S51~S55·S57·S58·S60·S79~S94)를 §9로 이식 — 판정은 덤프 파일 검사(checks · 공백으로 나누므로 패턴 안 공백은 `.`) · 창 수 · 첫 실행 27/1(S86 = 패턴 공백 → `.`).
#     (이 VM 창 캡처는 이제 가능 = `DISPLAY=:0 xdotool search --name Nexa` → `import -window <id>` · 61 §3 · 이 스크립트는 여전히 캡처 없이 판정)
#
# 사용: scripts/linux-func-check.sh -o <출력폴더> [-e target/debug/nexa-sql] [-n target/debug/nsql] [-O S05,S06]
set -u
# T-346 ⑥(10-15): Windows(Git Bash)에는 창 수 판정 분기가 없어 0/85가 된다(125차 V3) → Windows판으로 안내.
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) echo "Windows: use scripts/win-func-check.ps1 (pwsh -NoProfile -File scripts/win-func-check.ps1 -HomeDir … -DataDir … -Out …) — this script counts windows with xwininfo/CGWindowList only"; exit 2;; esac
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# 기능 점검 = Release(win-func-check.ps1과 같게 · Debug는 기동이 1.5 s라 기동 명령이 밀린다 · 09-26 실측).
APP="$ROOT/target/release/nexa-sql"; CLI="$ROOT/target/release/nsql"; OUT=""; ONLY=""
while getopts "o:e:n:O:" o; do case $o in o) OUT=$OPTARG;; e) APP=$OPTARG;; n) CLI=$OPTARG;; O) ONLY=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-e exe] [-n cli] [-O S01,S02]"; exit 2; }
H="$OUT/home"; D="$OUT/data"; rm -rf "$OUT"; mkdir -p "$H" "$D"
REPORT="$OUT/func-check.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0; warn=0

# ── 창 수 = 이 앱이 만든 X11 최상위 창(WM_CLASS nexa-sql · mutter 프레임은 제외) ───────────
# macOS(10-10 · 117차): xwininfo가 없다 → CGWindowList로 **내가 띄운 PID의 창만** 센다(접근성 권한 불필요 · swiftc 1회 컴파일 →
#   target/func-check-tools/wincount · 111차 협업 세션의 미커밋 사본을 저장소로 올림). Linux = 종전 xwininfo(WM_CLASS nexa-sql).
WINCOUNT="$ROOT/target/func-check-tools/wincount"
if [ "$(uname -s)" = Darwin ] && [ ! -x "$WINCOUNT" ]; then
  mkdir -p "$(dirname "$WINCOUNT")"
  cat > "$WINCOUNT.swift" <<'SWIFT'
import CoreGraphics
import Foundation
let pid = Int32(CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "0") ?? 0
var n = 0
if let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] {
    for w in list {
        if let p = w[kCGWindowOwnerPID as String] as? Int32, p == pid,
           let layer = w[kCGWindowLayer as String] as? Int, layer == 0 { n += 1 }
    }
}
print(n)
SWIFT
  xcrun swiftc -O -o "$WINCOUNT" "$WINCOUNT.swift" || { echo "wincount 컴파일 실패(swiftc 필요)"; exit 2; }
fi
wins_now() { # $1 = 앱 PID(맥) · Linux는 전체 WM_CLASS 수
  if [ "$(uname -s)" = Darwin ]; then "$WINCOUNT" "${1:-0}"; else xwininfo -root -tree 2>/dev/null | grep -c '("nexa-sql" "nexa-sql")'; fi
}

# ── 시험 자료 ──────────────────────────────────────────────────────────────
cat > "$D/q500.sql" <<'SQL'
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 500)
SELECT i AS id, 'name_' || i AS name, CASE WHEN i % 3 = 0 THEN NULL ELSE i * 1.5 END AS amount, 'SEBANG' AS project_cd, 'memo ' || i AS memo FROM n;
SQL
echo "SELECT * FROM no_such_table_zz;" > "$D/q_err.sql"
# S95(10-10 118차 mac 크래시 회귀): 30만 행(받는 중이라도 10만 행 이상이면 됨) × 행 높이 20~39 px = 내용 200만~1,170만 px → 썸 드래그 3000 px의
#   i32 곱(≥ 6e9)이 넘치던 크기(임계 = 2^31 ÷ 3000 ≈ 72만 px). 10만 행 이상·3000 px이면 3-OS 어느 배율에서도 재현된다.
cat > "$D/q_big.sql" <<'SQL'
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 300000)
SELECT i AS id, i * 2 AS twice FROM n;
SQL
cat > "$D/q_two.sql" <<'SQL'
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 300)
SELECT i AS id, 'a_' || i AS name FROM n;
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 300)
SELECT i AS id, 'b_' || i AS name FROM n;
SQL
printf "select sum(x) from t where sum > 1 and sum < 9 order by sum;\n-- sum sum sum\n" > "$D/q_word.sql"
python3 -c "print('sum ' * 150)" > "$D/q_many.sql"
printf -- "-- file a\nSELECT 1;\n" > "$D/a.sql"
printf -- "-- file b\nSELECT 2;\n" > "$D/b.sql"
printf -- "-- file c\nSELECT 3;\n" > "$D/c.sql"
python3 -c "
import io
open('$D/q_lines.sql','w').write('\n'.join('SELECT %d;' % i for i in range(1,151)))
"
LONGDIR="$D/very_long_directory_name_for_ellipsis_check/another_level_of_nesting_here/and_one_more_level_to_make_it_long"
mkdir -p "$LONGDIR"; echo "SELECT 'long';" > "$LONGDIR/the_script_with_a_fairly_long_file_name_2026-09-26.sql"
printf '{ "version": 1, "folders": [ { "path": "%s" } ] }' "$D" > "$D/fc.nsql-project"
cat > "$D/q_outline.sql" <<'SQL'
DEFINE v_user = 'scott'
VARIABLE rc REFCURSOR
WITH recent AS (SELECT 1 AS id FROM dual), older AS (SELECT 2 AS id FROM dual)
SELECT r.id FROM recent r, older o WHERE r.id = o.id;
CREATE OR REPLACE PACKAGE BODY pkg_report AS
  g_count NUMBER := 0;
  CURSOR c_rows IS SELECT 1 FROM dual;
  PROCEDURE run_report(p_day IN DATE) IS
    v_total NUMBER;
  BEGIN
    NULL;
  END run_report;
  FUNCTION total_rows RETURN NUMBER IS
  BEGIN
    RETURN g_count;
  END total_rows;
END pkg_report;
/
SQL
printf 'SELECT coal\nSELECT * FROM sqlite_m\n' > "$D/q_intel.sql"
printf "DEFINE who = 'O''Neil'\nSELECT 'it''s' AS v, \"a\"\"b\" AS q FROM DUAL;\n" > "$D/q_quote.sql"
printf "DEFINE who = \"O'Neil\"\nSELECT 'a' || 'b' AS v FROM DUAL;\nx = 'broken\n('y')\n" > "$D/q_dq.sql"
SKIP="$D/skipdir"; mkdir -p "$SKIP"
printf 'SELECT 1 FROM t;\nSELECT 2 FROM u;\n' > "$SKIP/ok.sql"
python3 -c "open('$SKIP/big.sql','w').write('SELECT big;\n' * 110000)"
python3 -c "open('$SKIP/bin.dat','wb').write(bytes([83,69,76,69,67,84,0,1,2,3]))"

# 기본 설정(Windows판과 같게): 결과 탭 바 항상 · 영어 · 데모 안내 끔 · 창 크기 고정(좌표 시나리오 기준).
BASECONF=$'lang=en\ngrid.result_tabbar_single=on\ndemo.prompted=on\nwindow.main_size=1375,945\n'

NSQL_HOME="$H" "$CLI" conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1

# ── 시나리오 실행기 ────────────────────────────────────────────────────────
# run_s <id> <제목> <기동명령> [인자] [대기초] [추가설정] [검사식]
#   검사식 = `파일:패턴` 목록(공백 구분 · `!` 접두 = 없어야 함) — 덤프/저장 파일을 확인한다.
run_s() {
  local id=$1 title=$2 cmd=$3 arg=${4:-Local} secs=${5:-4.5} conf=${6:-} checks=${7:-}
  if [ -n "$ONLY" ] && ! echo ",$ONLY," | grep -q ",$id,"; then return; fi
  local home2="$H"
  case "$arg" in NOPROF) home2="$H/noprof"; mkdir -p "$home2"; arg="";; esac
  printf '%s%s' "$BASECONF" "$conf" > "$home2/settings.conf"
  local err="$OUT/$id.stderr.txt"
  NSQL_HOME="$home2" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" "$APP" $arg >"$OUT/$id.stdout.txt" 2>"$err" &
  local p=$!
  sleep "$secs"
  local alive=0 wins=0
  if kill -0 "$p" 2>/dev/null; then alive=1; wins=$(wins_now "$p"); fi
  kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
  local panic=0; grep -qE "panicked|RUST_BACKTRACE" "$err" 2>/dev/null && panic=1
  local verdict="ok" detail=""
  if [ "$alive" = 0 ]; then verdict="FAIL(exited)"
  elif [ "$panic" = 1 ]; then verdict="FAIL(panic)"
  elif [ "$wins" -lt 1 ]; then verdict="FAIL(no window)"
  fi
  # 산출 파일 검사
  local c f pat neg bad=0
  for c in $checks; do
    neg=0; case "$c" in '!'*) neg=1; c=${c#!};; esac
    f="${c%%:*}"; pat="${c#*:}"
    if [ "$neg" = 1 ]; then
      if grep -qE -- "$pat" "$OUT/$f" 2>/dev/null; then bad=1; detail="$detail !$pat"; fi
    else
      if ! grep -qE -- "$pat" "$OUT/$f" 2>/dev/null; then bad=1; detail="$detail ?$pat"; fi
    fi
  done
  [ "$bad" = 1 ] && [ "$verdict" = "ok" ] && verdict="FAIL(check)"
  case "$verdict" in ok) pass=$((pass+1));; *) fail=$((fail+1));; esac
  say "$(printf '| %-4s | %-52s | %-13s | win=%d |%s' "$id" "$title" "$verdict" "$wins" "$detail")"
}

say "== linux-func-check $(date '+%F %T')  commit=$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null)"
say "exe=$APP  home=$H"
say "| id | 시나리오 | 판정 | 창 | 비고 |"
say "|---|---|---|---|---|"

A="$D/a.sql"; B="$D/b.sql"; C="$D/c.sql"; Q5="$D/q500.sql"; QE="$D/q_err.sql"; QT="$D/q_two.sql"; QBIG="$D/q_big.sql"
QW="$D/q_word.sql"; QM="$D/q_many.sql"; QL="$D/q_lines.sql"; QO="$D/q_outline.sql"; QI="$D/q_intel.sql"
PROJ="$D/fc.nsql-project"

# ── 1. 창·기동·파일 모드 ───────────────────────────────────────────────────
run_s S01 "동시 편집 탭 상단 줄(§29)"              "open:$A,open:$B,@after:1500:ui.sclick:365/80"
run_s S02 "다중 열기 확인 팝업(§30·45·47)"          "file.open_many:$A;$B;$C"
run_s S03 "다중 열기 → 순차 적재(§30)"              "file.open_many:$A;$B;$C,@after:1200:multi.open" Local 5.5
run_s S04 "프로젝트 인자 = 프로젝트 모드(§31)"       "@after:1200:view.project" "Local $PROJ"
run_s S05 "파일 인자 = 파일 모드(§31)"               "" "Local $A $B"
run_s S06 "프로젝트 패널 클릭 = 미리보기 탭(§28)"    "project.load:$PROJ,@after:1200:view.project,@after:2500:ui.click:150/175" Local 5.5
run_s S07 "프로젝트 패널 더블클릭 = 정식 탭(§34)"    "project.load:$PROJ,@after:1200:view.project,@after:2500:ui.dclick:150/175" Local 5.5
run_s S08 "IME 안내 = 입력란 아래(§34 · 접속 창)"    "@after:1500:conn.ime_hint:0/0" NOPROF 4.5
run_s S09 "긴 경로 가운데 축약(§38)"                 "@after:1000:ui.click:25/14" Local 4.5 $'file.recent='"$LONGDIR"$'/the_script_with_a_fairly_long_file_name_2026-09-26.sql\nui.menu_max_width=320\n'

# ── 2. 실행 카드·그리드 ────────────────────────────────────────────────────
run_s S10 "실행 카드 스택(§48·50~52)"                "open:$Q5,@after:1500:run.all,@after:3000:run.all,@after:4200:run.all,@after:5500:grid.dump:$OUT/s10.txt" Local 7 "" "s10.txt:rows="
run_s S11 "오류 5번 = 카드 5장(§50)"                 "open:$QE,@after:1200:run.all,@after:1700:run.all,@after:2200:run.all,@after:2700:run.all,@after:3200:run.all" Local 5.5
run_s S12 "Σ 건수·전체 조회 = 실행 카드(§43)"         "open:$Q5,@after:1200:run.all,@after:3000:ui.click:667/902,@after:4200:ui.click:619/902,@after:5500:grid.dump:$OUT/s12.txt" Local 7 "" "s12.txt:rows="
run_s S13 "그리드 셀 선택·NULL 흐림(§44·49)"          "open:$Q5,@after:1200:run.all,@after:3000:ui.click:420/580,@after:4500:grid.dump:$OUT/s13.txt" Local 9.5 "" "s13.txt:rows="
run_s S14 "행번호 선택 = 행 배경만(§47)"              "open:$Q5,@after:1200:run.all,@after:3000:ui.click:334/580"
run_s S15 "카드 SQL 툴팁 = 카드 왼쪽(§53)"            "open:$Q5,@after:1200:run.all,@after:3000:ui.move:1150/432" Local 5.5
run_s S16 "편집기 우클릭 메뉴 = 카드 위(§54)"          "open:$Q5,@after:1200:run.all,@after:3000:ui.rclick:1000/450" Local 5.5
run_s S17 "풀다운 → 편집기 우클릭 = 배타(§55)"        "@after:1000:ui.click:133/14,@after:1800:ui.rclick:592/269"
run_s S18 "편집 탭 메뉴 → 결과 탭 우클릭(§58)"        "open:$Q5,@after:1200:run.all,@after:3000:ui.rclick:365/80,@after:3800:ui.rclick:380/507" Local 5.5
run_s S19 "결과 탭 메뉴 → 편집기 우클릭(§58)"         "open:$Q5,@after:1200:run.all,@after:3000:ui.rclick:380/507,@after:3800:ui.rclick:592/269" Local 5.5
run_s S20 "그리드 메뉴 → 편집기 우클릭(§58)"          "open:$Q5,@after:1200:run.all,@after:3000:ui.rclick:420/580,@after:3800:ui.rclick:592/269" Local 5.5
run_s S21 "편집 탭 메뉴 → 편집기 우클릭(§58)"         "@after:1000:ui.rclick:365/80,@after:1800:ui.rclick:592/269"
run_s S26 "결과 탭 메뉴(result.menu)"                 "open:$Q5,@after:1200:run.all,@after:3000:result.menu" Local 5.5
run_s S27 "run.toast_max=3 상한(§51)"                 "open:$QE,@after:1200:run.all,@after:1700:run.all,@after:2200:run.all,@after:2700:run.all,@after:3200:run.all" Local 5.5 $'run.toast_max=3\n'
run_s S28 "결과 탭 둘 + 탭 바(§43)"                   "open:$QT,@after:1200:run.all,@after:3500:grid.dump:$OUT/s28.txt" Local 5.5 "" "s28.txt:rows="

# ── 3. 편집기 선택·다중 캐럿 ───────────────────────────────────────────────
run_s S22 "Quick Skip Next(§59)"                      "open:$QW,@after:1200:ui.click:390/141,@after:1800:edit.expand_selection,@after:2200:edit.expand_selection,@after:2600:edit.skip_occurrence"
run_s S23 "Edit 메뉴 6그룹 · Selection ▸(§60)"        "@after:1000:ui.click:133/14,@after:1600:ui.move:133/215"
run_s S24 "좁은 창에서 하위 메뉴 잘림 없음(§60)"       "@after:1000:ui.click:133/14,@after:1600:ui.move:133/215" Local 4.5 $'window.main_size=430,600\n'
run_s S25 "우클릭 메뉴 → 비활성 항목 hover(§56)"       "@after:1000:ui.rclick:592/269,@after:1600:ui.move:640/300"
run_s S29 "선택 되돌리기 soft undo(§65)"               "open:$QW,@after:1200:ui.click:390/141,@after:1800:edit.expand_selection,@after:2200:edit.expand_selection,@after:2600:edit.expand_selection,@after:3000:edit.soft_undo"
run_s S30 "선택 다시 실행 soft redo(§65)"              "open:$QW,@after:1200:ui.click:390/141,@after:1800:edit.expand_selection,@after:2200:edit.expand_selection,@after:2600:edit.expand_selection,@after:3000:edit.soft_undo,@after:3400:edit.soft_undo,@after:3800:edit.soft_redo" Local 5
run_s S31 "다중 선택 상한 max_occurrences(§66)"        "open:$QM,@after:1200:ui.click:372/141,@after:1800:edit.select_all_occurrences" Local 4.5 $'editor.max_occurrences=100\n'
run_s S32 "상한 = Split into Lines(§68)"               "open:$QL,@after:1200:ui.click:400/141,@after:1800:edit.select_all,@after:2400:edit.split_lines" Local 4.5 $'editor.max_occurrences=100\n'
run_s S65 "줄 변경 표시 = 복제한 줄(§113)"             "open:$Q5,@after:1000:ui.click:400/141,@after:1500:edit.duplicate_line"
run_s S64 "줄 번호 끔 + 줄 변경 표시(§113)"            "open:$Q5,@after:1000:ui.click:400/141,@after:1500:edit.duplicate_line" Local 3.5 $'editor.line_numbers=off\n'
run_s S70 "SQL '' 이스케이프 = 쌍 아님(§118)"          "open:$D/q_quote.sql,@after:1200:ui.click:474/141"
run_s S75 "\"O'Neil\" 뒤 ' 짝 + 줄 끝 재동기화(§126)"  "open:$D/q_dq.sql,@after:1200:ui.click:452/141"

# ── 4. 북마크 ──────────────────────────────────────────────────────────────
run_s S33 "북마크 토글 + 패널(§70)"                    "open:$Q5,@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle,@after:2800:view.bookmarks"
run_s S34 "북마크 다음/이전 이동(§70)"                 "open:$Q5,@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle,@after:2800:ui.click:400/141,@after:3200:bookmark.next"
run_s S35 "북마크 패널 우클릭 메뉴(§71)"                "open:$Q5,@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle,@after:2800:view.bookmarks,@after:3400:ui.rclick:150/182" Local 5
run_s S56 "줄 번호 끔 + 북마크 슬림 거터(§95)"          "open:$Q5,@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.set_3,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle" Local 4 $'editor.line_numbers=off\n'
run_s S61 "거터 우클릭 = 북마크 메뉴(없는 줄 · §107)"   "open:$Q5,@after:1000:bookmark.clear_doc,@after:1500:ui.rclick:335/141"
run_s S62 "거터 우클릭 = 있는 줄(§107·§108)"            "open:$Q5,@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1500:bookmark.set_3,@after:2000:ui.rclick:335/141" Local 4
run_s S63 "거터 빈 영역 우클릭 = 보기만(§108)"          "open:$Q5,@after:1000:bookmark.clear_doc,@after:1500:ui.rclick:335/400"
run_s S69 "북마크 패널 문서 이름 = 탭 id(§118)"         "file.new,@after:800:ui.click:400/141,@after:1000:bookmark.toggle,@after:1400:view.bookmarks,@after:2000:tab.rename_to:NoName1"

# ── 5. 프로젝트 ────────────────────────────────────────────────────────────
run_s S38 "프로젝트 탐색기 아이콘·루트 우클릭(§76)"     "project.load:$PROJ,@after:1200:view.project,@after:2500:ui.click:60/132,@after:3200:ui.rclick:120/132" Local 5
run_s S39 "프로젝트 탐색기 아이콘 끔(§76)"              "project.load:$PROJ,@after:1200:view.project,@after:2500:ui.click:60/132" Local 4.5 $'project.icons=off\n'
run_s S41 "탭 우클릭 = Reveal in Project Explorer(§78)" "project.load:$PROJ,open:$QL,@after:1500:ui.rclick:470/80" Local 4
run_s S42 "활성 탭 따라가기 켬(§78)"                    "project.load:$PROJ,@after:800:view.project,@after:1500:open:$QL" Local 4.5 $'project.auto_reveal=on\n'
run_s S43 "활성 탭 따라가기 끔(기본)(§78)"              "project.load:$PROJ,@after:800:view.project,@after:1500:open:$QL" Local 4.5
run_s S44 "프로젝트 탐색기 가로 스크롤(§79)"            "project.load:$PROJ,@after:1200:view.project,@after:2500:ui.hwheel:180/300/360" Local 4.5
run_s S45 "세로 스크롤 = 필터 상자와 안 겹침(§79)"      "project.load:$PROJ,@after:1200:view.project,@after:2500:ui.wheel:180/300/-120" Local 4.5 $'window.main_size=1000,420\n'
run_s S46 "프로젝트 필터 틀 토글 넷(§80)"               "project.load:$PROJ,@after:1200:view.project,@after:2500:ui.click:150/175,@after:3200:ui.click:180/560" Local 5
run_s S47 "북마크 패널 필터 틀(§80)"                    "open:$Q5,@after:1000:view.bookmarks" Local 3.5
run_s S48 "확장 패널 검색 틀(§80)"                      "@after:1000:view.extensions" Local 3.5 $'extensions.enabled=on\n'
run_s S50 "필터 Path 토글 = 경로 일치(§81)"             "project.load:$PROJ,@after:1200:view.project,@after:2000:project.filter:p:check/another" Local 4.5
run_s S59 "파일 모드 + 프로젝트 패널 = OPEN FILES(§103)" "open:$A,open:$B,@after:1200:view.project" Local 3.5
run_s S76 "검색어 이력 드롭다운(§127)"                  "project.load:$PROJ,@after:1200:view.project,@after:1600:project.filter:one,@after:1800:ui.click:100/163,@after:2000:ui.click:180/560,@after:2200:project.filter:two,@after:2400:ui.click:100/163,@after:2600:ui.click:180/560,@after:2800:project.filter:,@after:3000:ui.click:100/163" Local 4.5 $'search.history_rows=5\n'

# ── 6. 파일 검색 ───────────────────────────────────────────────────────────
run_s S71 "파일 검색 제외 로그 = 제외됨(N)(§120)"       "view.search,@after:1000:search.run:SELECT,@after:3200:ui.click:120/249" "Local $SKIP" 4.5
run_s S72 "파일 검색 범위 필터(§122)"                   "view.search,@after:1000:search.run:SELECT|*.sql;-big*" "Local $SKIP" 4
run_s S73 "검색 결과 가로 스크롤 + 열린 탭 필터(§124)"   "open:$QO,@after:800:view.search,@after:1200:search.run:SELECT|*.sql,@after:3200:ui.click:120/249,@after:3800:ui.hwheel:180/300/360" "Local $SKIP" 5

# ── 7. 인텔리센스 · 아웃라인(§117·§129) ────────────────────────────────────
run_s S66 "코드 완성 팝업 = Ctrl+Space(§117)"           "open:$QO,@after:1000:ui.click:432/181,@after:1600:edit.complete"
run_s S77 "내장 함수 완성 = 시그니처 열(§129)"          "open:$QI,@after:1000:ui.click:700/121,@after:1600:edit.complete"
run_s S78 "사전 뷰 = FROM 뒤 sqlite_m(§129)"            "open:$QI,@after:1000:ui.click:700/141,@after:1600:edit.complete"
run_s S67 "Goto Symbol 팔레트(§117)"                    "open:$QO,@after:1200:goto.symbol"
run_s S68 "아웃라인 패널(§117)"                         "open:$QO,@after:1200:view.outline,@after:2500:mem.dump:$OUT/s68.txt" Local 4.5 "" "s68.txt:."
run_s S74 "이진 파일 아웃라인 = 무시 + 안내(§125)"      "open:$SKIP/bin.dat,@after:1200:view.outline,@after:1800:goto.symbol" Local 4
# ★ 10-10 118차 mac 크래시 회귀(nexa-ctl scroll.rs i32 넘침 · nexa-ui 197차): 행 수 0(전체) 30만 행 → 썸을 잡고 1000 px 끌기(좌표 = 그리드가 셈)
#   → 살아 있고(Debug면 종전 abort) 세로 끝에 닿아야 한다(Release면 종전 감겨서 0 근처 = scroll_end=false). 판정 = 행 ≥ 10만(Debug는 30만 행
#   수신에 18 s 걸려 부분 결과로 판정 · Release는 끝까지) + scroll_end=true. 드래그 3000 px = 창 밖까지(끝 클램프). 대기 = 마지막 @after + 5 s(61 §4 ·
#   비활성 앱은 App Nap으로 타이머가 늦게 깬다 — 10 s로는 덤프 직전에 종료됐다 · 협업 10-10).
# ★ 폴더 변경 감시(T-293 · Windows ReadDirectoryChangesW · macOS FSEvents(118차) · Linux inotify(119차 · T-321)): 프로젝트를 열고 앱 밖에서
#   파일을 만든 것처럼(`fs.write` = 앱 프로세스가 쓰지만 감시 경로는 같다) → 패널 플래시 "Folder change applied"(3-OS 같은 판정).
# 플래시는 사건(macOS FSEvents 0.2 s · Linux inotify 즉시 + 묶음 0.4 s) 뒤 유지 시간 안에 덤프(5.0 s) · 대기 = 마지막 @after + 5 s · 판정 패턴에 공백 불가(`.`) ·
# 글은 화면 언어를 따르므로(`ui.lang=system` · 이 VM = ko) 영어·한국어 둘 다 허용(10-11 Linux 첫 실행에서 한국어 글로 빗나갔다).
# ★ 10-11 119차(T-321 · nexa-ui 201차 inotify): Linux도 macOS와 같은 플래시 판정(종전 "TTL 폴백 · 생존만"은 백엔드가 없던 때).
run_s S96 "폴더 변경 감시 = 플래시 '폴더 변경 반영'(T-293 · macOS FSEvents · Linux inotify)" "project.load:$PROJ,@after:1200:view.project,@after:3500:fs.write:$D/s96_new.sql|SELECT 96;,@after:5000:flash.dump:$OUT/s96.txt" Local 11 "" "s96.txt:(Folder.change.applied|폴더.변경.반영)"
run_s S95 "큰 결과 썸 드래그 = i32 넘침 없음(10-10 크래시 회귀)" "open:$QBIG,@after:1200:grid.page:0,@after:1500:run.all,@after:8000:grid.vdrag:3000,@after:9000:grid.dump:$OUT/s95.txt" Local 14 "" "s95.txt:rows=[0-9]{6} s95.txt:scroll_end=true"
# ★ S97(10-11 119차 · T-321 · nexa-ui 200차): Linux도 OS 네트워크 신호(rtnetlink)가 등록된다 — `health.dump`의 `netwatch=true`(종전 Linux = false · `probe.stale_secs` 폴백).
#   권한 불필요(AF_NETLINK 멀티캐스트 수신) · 신호 자체(VPN 토글)는 사용자 실기 · 설정 `net.watch` 기본 켬.
run_s S97 "네트워크 신호 등록(T-321 · Linux rtnetlink · health.dump netwatch=true)" "@after:3000:health.dump:$OUT/s97.txt" Local 6 "" "s97.txt:netwatch=true"

# ── 8. 91차 이후 신기능(Linux 첫 확인) ─────────────────────────────────────
run_s L01 "메모리 모니터 창 + 덤프(docs/80 · T-197)"    "@after:1200:view.memory,@after:2500:mem.dump:$OUT/L01.txt" Local 4 "" "L01.txt:."
run_s L02 "그리드 편집 덤프(docs/87 · T-182)"           "open:$Q5,@after:1500:run.all,@after:3500:grid.dump:$OUT/L02.txt" Local 5 "" "L02.txt:rows="
run_s L03 "트랜잭션 로그 창 + 덤프(docs/44)"            "open:$Q5,@after:1500:run.all,@after:3500:view.txlog,@after:4500:txlog.dump:$OUT/L03.txt" Local 6 "" "L03.txt:."
run_s L04 "세션 창(docs/52 · sessions_win)"             "@after:1200:view.sessions" Local 3.5
run_s L05 "설정 창(prefs)"                              "@after:1200:edit.prefs" Local 3.5
run_s L06 "확장 패널 + 관리자(docs/50)"                 "@after:1200:view.extensions" Local 3.5 $'extensions.enabled=on\next.disable_mgr=off\n'
run_s L07 "로그 창(docs/48)"                            "@after:1200:view.log" Local 3.5
run_s L08 "변수 창(docs/63)"                            "open:$Q5,@after:1500:view.variables" Local 4
run_s L09 "명령 팔레트"                                 "@after:1200:view.palette" Local 3.5
run_s L10 "찾기 바 + 정규식(docs/29)"                   "open:$Q5,@after:1200:edit.find" Local 3.5
run_s L11 "미니맵 켬(T-97)"                             "open:$QL,@after:1500:ui.click:400/141" Local 4 $'editor.minimap=on\n'
run_s L12 "큰 파일 모드 L1/L2(docs/59)"                 "open:$SKIP/big.sql" Local 8 $'bigfile.confirm=off\n'
# ★ 라이선스(docs/23 §4-4 · 25 §13-3 · T-34/T-36 · 09-27): 창·덤프 · 무효 파일 거부 · 탭 상한(게이트 = 숨은 `license.gates` · 기본 끔 D-145).
#   L13·L14 = `ui.lang=en` 고정 — 기본 언어가 OS 표시 언어를 따르므로(62f4ea6) 한국어 로캘에서 배지·`State=` 판정이 어긋난다(10-03).
echo "hello" > "$D/bad.license"
SIX="file.new,@after:200:file.new,@after:400:file.new,@after:600:file.new,@after:800:file.new,@after:1000:file.new"
run_s L13 "라이선스 창 + 덤프(T-34)"                     "@after:1200:help.license,@after:2500:license.dump:$OUT/L13.txt" Local 4 $'ui.lang=en\n' "L13.txt:open=true L13.txt:badge=Free"
run_s L14 "무효 라이선스 파일 = 설치 거부(쓰지 않음)"   "@after:1200:license.install:$D/bad.license,@after:2500:license.dump:$OUT/L14.txt" Local 4 $'ui.lang=en\n' "L14.txt:note=warn !L14.txt:State=licensed"
run_s L15 "편집 탭 상한 5(게이트 on · D-46)"            "@after:1200:$SIX,@after:3200:bm.stat:$OUT/L15.txt" Local 4.5 $'license.gates=on\n' "L15.txt:^4:.title !L15.txt:^5:.title"
# L16: 게이트는 기본 끔(D-145 09-28) — Debug·Release 모두 `license.gates=on`일 때만 적용(docs/23 §4-2).
run_s L16 "게이트 off(기본) = 탭 제한 없음"             "@after:1200:$SIX,@after:3200:bm.stat:$OUT/L16.txt" Local 4.5 "" "L16.txt:^6:.title"

# ── 9. Windows판(win-func-check.ps1)에만 있던 시나리오 이식(10-24 · 138차 Linux · S36·S37·S40·S49·S51~S55·S57·S58·S60·S79~S94) ──
# 좌표·대기·설정은 Windows판 그대로(BASECONF 창 1375×945 · 2코어 VM이라 +0.5 s). 자동 판정 = 생존·창 수 + 덤프 파일 검사(checks) · 화면 모양은 캡처 몫.
PSEL="$D/fc-sel.nsql-project"; printf '{ "version": 1, "folders": [ { "path": "." } ], "selected": "q_lines.sql" }' > "$PSEL"
PBIG="$D/fc-big.nsql-project"; printf '{ "version": 1, "folders": [ { "path": "%s" } ] }' "$ROOT" > "$PBIG"
PWS="$D/fc-ws.nsql-project"; PSAVE="$D/fc-save.nsql-project"
write_pws() { cat > "$PWS" <<JSON
{ "version": 1, "folders": [ { "path": "." } ], "active": 1, "panel": "bookmarks", "profiles": [ "Local" ], "tabs": [ { "path": "$QL", "title": "q_lines.sql", "line": 40, "col": 0, "anchor": "SELECT 41;", "before": "SELECT 40;", "after": "SELECT 42;" }, { "title": "Notes", "line": 1, "col": 0, "text": "-- restored scratch\nSELECT 'ws';" } ], "bookmarks": { "version": 1, "next_id": 2, "next_group": 2, "groups": [ { "id": 1, "name": "", "default": true, "enabled": true } ], "items": [ { "id": 1, "doc": { "file": "$QL" }, "line": 4, "col": 0, "text": "SELECT 5;", "before": "SELECT 4;", "after": "SELECT 6;", "hash": "0", "lines": 150, "group": 1, "shared": false, "state": "live", "created": 0, "visited": 0 } ] } }
JSON
}
write_pws
run_s S40 "프로젝트 열기 = 마지막 선택 위치 복원(§77)"     "project.load:$PSEL,@after:1200:project.panel:on" Local 4.5
run_s S49 "필터 안내 글 두 줄(일치 없음 → 상한)(§80)"      "project.load:$PBIG,@after:1200:project.panel:on,@after:2000:project.filter:zzqq" Local 5 $'project.scan_max=100\n'
run_s S51 "프로젝트 로딩 = 작업 환경 복원 + OPEN FILES(§89)" "project.load:$PWS,@after:1500:project.panel:on,@after:3500:tabs.dump:$OUT/s51_tabs.txt" Local 5 "" "s51_tabs.txt:q_lines.sql s51_tabs.txt:Notes"
run_s S37 "풀다운 Project ▸ Save New Project… = 파일 저장 창(§75)" "open:$QL,@after:1000:ui.click:75/14,@after:1600:ui.click:120/48" Local 5
# S36: 워크스페이스를 비운 뒤 CLI로 심고 GUI가 같은 파일을 읽는다(B8 · 미니맵 틱 · 줄 끝 라벨).
rm -f "$H/workspaces/default.nsql-workspace"
NSQL_HOME="$H" "$CLI" bookmark add "$QL" 2 --label "monthly totals" >/dev/null 2>&1
NSQL_HOME="$H" "$CLI" bookmark add "$QL" 5 >/dev/null 2>&1
NSQL_HOME="$H" "$CLI" bookmark add "$QL" 120 >/dev/null 2>&1
say "  cli: $(NSQL_HOME="$H" "$CLI" bookmark list 2>/dev/null | grep -c 'q_lines') bookmarks on q_lines.sql"
run_s S36 "CLI로 심은 북마크 = 거터·미니맵 틱·줄 끝 라벨(§74)" "open:$QL,@after:2500:bm.stat:$OUT/s36_bm.txt" Local 4.5 "" "s36_bm.txt:marks=3"
printf '{ "version": 1, "folders": [ { "path": "." } ] }' > "$PSAVE"
run_s S52 "프로젝트 저장 3번 = 탭 그대로 · 파일에 탭 경로(§92)" "project.load:$PSAVE,open:$A,open:$B,@after:1000:project.panel:on,@after:1300:bookmark.set_1,@after:1400:ui.click:400/161,@after:1500:bookmark.set_2,@after:1600:ui.click:460/80,@after:1700:bookmark.set_1,@after:1800:project.save,@after:2600:project.save,@after:3400:project.save,@after:4200:tabs.dump:$OUT/s52_tabs.txt" Local 5.5 "" "s52_tabs.txt:a.sql s52_tabs.txt:b.sql"
saved=$(cat "$PSAVE" 2>/dev/null)
say "  file: tabs=$(printf '%s' "$saved" | grep -c '"tabs"') a.sql=$(printf '%s' "$saved" | grep -c '"path": "a\.sql"') b.sql=$(printf '%s' "$saved" | grep -c '"path": "b\.sql"') mn1x2=$(printf '%s' "$saved" | grep -o '"mn": 1' | wc -l) mn2x1=$(printf '%s' "$saved" | grep -o '"mn": 2' | wc -l) blob=$(printf '%s' "$saved" | grep -c 'NSQL-BLOBS') tabs_in_dump=$(grep -c '|' "$OUT/s52_tabs.txt" 2>/dev/null)"
write_pws
run_s S53 "프로젝트 로딩 = 옛 탭 닫고 교체 · 패널·접속 표식 복원(§92)" "open:$A,@after:1200:project.load:$PWS,@after:3000:tabs.dump:$OUT/s53_tabs.txt" Local 4 "" "s53_tabs.txt:q_lines.sql !s53_tabs.txt:a.sql"
run_s S54 "북마크 패널 더블클릭 = 정식 탭(§92)"             "view.bookmarks,@after:1500:ui.dclick:150/182,@after:3000:tabs.dump:$OUT/s54_tabs.txt" Local 4.5 "" "s54_tabs.txt:q_lines"
run_s S55 "북마크 패널 한 번 클릭 = 미리보기 탭(§92)"       "view.bookmarks,@after:1500:ui.click:150/182,@after:3000:tabs.dump:$OUT/s55_tabs.txt" Local 4.5 "" "s55_tabs.txt:q_lines"
write_pws
run_s S57 "프로젝트 닫기 = 처음 실행 상태 + 북마크 초기화(§98)" "open:$A,@after:1000:project.load:$PWS,@after:2500:project.close,@after:3200:view.bookmarks,@after:4000:tabs.dump:$OUT/s57_tabs.txt" Local 5 "" "!s57_tabs.txt:q_lines.sql !s57_tabs.txt:a.sql"
# S58 폴더 모드 = 두 번째 인자가 폴더 → 북마크는 <폴더>/.nsql/workspaces/에.
rm -f "$D/.nsql/workspaces/default.nsql-workspace"
run_s S58 "폴더 모드 = 북마크가 <폴더>/.nsql/에(§99)"        "open:$Q5,@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2200:view.bookmarks" "Local $D" 5.5
say "  file: folder_ws=$([ -f "$D/.nsql/workspaces/default.nsql-workspace" ] && echo true || echo false)"
rm -f "$H/workspaces/default.nsql-workspace"
run_s S60 "파일 모드 북마크 = 사건 없이도 디바운스 저장(§104)" "open:$Q5,@after:800:ui.click:400/141,@after:1000:bookmark.toggle" Local 5
say "  file: ws_saved=$([ -f "$H/workspaces/default.nsql-workspace" ] && echo true || echo false)"
# ── 115차 8차 묶음(10-07) · 훅 = fs.write/fs.delete · mem.trim · tabs.dump · ext.view · ext.banner · flash.dump · editor.dump ──
run_s S79 "Output 탭 = 오류 실행 뒤 자동 표시·전환(108차 · docs/100 §3)" "open:$QE,@after:1200:run.all,@after:3500:output.dump:$OUT/s79_output.txt" Local 5 "" "s79_output.txt:no_such_table_zz"
run_s S80 "메모리 창 = 묶음 소계 · 전용 WS · [힙 정리] · ▲/▼(115-8 · docs/80 §4)" "open:$Q5,@after:1200:run.all,@after:2500:view.memory,@after:5000:mem.trim,@after:6200:mem.dump:$OUT/s80_mem.txt" Local 7.5 "" "s80_mem.txt:private_ws=[1-9]"
S81NEW="$D/s81_new_file.sql"; rm -f "$S81NEW"
run_s S81 "폴더 변경 감시 → 플래시(필터 없음 · 스낵바 · 페이드)(T-293/T-299)" "project.load:$PROJ,@after:1000:project.panel:on,@after:2600:fs.write:$S81NEW|SELECT 81;,@after:3600:flash.dump:$OUT/s81_flash.txt" Local 4.5 "" "s81_flash.txt:Folder.change|폴더.변경"
rm -f "$S81NEW"
run_s S82 "향상 모드 = 플래시 즉시 표시·즉시 숨김(페이드 0)(115-8)" "project.load:$PROJ,@after:1000:project.panel:on,@after:2600:fs.write:$S81NEW|SELECT 82;,@after:3400:flash.dump:$OUT/s82_flash.txt" Local 4.2 $'perf.boost=on\n' "s82_flash.txt:instant=true"
rm -f "$S81NEW"
run_s S83 "기존 파일 덮어쓰기 = 플래시 없음(실제 변경 때만)(115-8)" "project.load:$PROJ,open:$A,@after:1000:project.panel:on,@after:2600:fs.write:$A|-- file a changed\nSELECT 1;,@after:3600:flash.dump:$OUT/s83_flash.txt" Local 4.5 "" "!s83_flash.txt:Folder.change|폴더.변경"
printf -- "-- file a\nSELECT 1;\n" > "$A"
DELF="$D/s84_delete_me.sql"; printf "SELECT 'delete me';\n" > "$DELF"
run_s S84 "삭제된 파일 탭 = 진빨강 ● · 닫기 확인 팝업이 띠 위(115-8)" "open:$DELF,open:$A,@after:1200:fs.delete:$DELF,@after:2200:tab.prev,@after:4200:tabs.dump:$OUT/s84_tabs.txt,@after:4400:file.close_tab" Local 5.7 $'file.external_poll_ms=500\n' "s84_tabs.txt:s84_delete_me.*deleted=true"
CHGF="$D/s85_change_me.sql"; printf "SELECT 'before';\n" > "$CHGF"
run_s S85 "외부 변경 띠 ▸ 디스크 보기 = 읽기 전용 · ● 없음 · 1행 1열(115-8)" "open:$CHGF,@after:1000:ui.click:600/141,@after:1200:fs.write:$CHGF|SELECT 'after';,@after:4000:ext.banner:diff,@after:4800:tabs.dump:$OUT/s85_tabs.txt,@after:4900:editor.dump:$OUT/s85_editor.txt" Local 5.8 $'file.external_poll_ms=500\n' "s85_tabs.txt:ro=true"
RSF="$D/s86_main.rs"; printf '//! tool keygen --out <env> [--id root-v1] [--pass-env NAME | --pass-stdin]\n//! issue  (--tier trial|pro|org)\nmod ledger;\nfn main() { let x = [1, 2]; }\n' > "$RSF"
run_s S86 "모르는 확장자 = Plain Text · 괄호 물결 없음(115-8)" "open:$RSF,@after:1200:ui.click:600/141,@after:1800:editor.dump:$OUT/s86_editor.txt" Local 4 "" "s86_editor.txt:Plain.Text"
# 확장 설치본 흉내 = <홈>/extensions/<id>/installed.json + <id>/<버전>/extension.json(+wasm) — 매니저 `installed_in` 규약.
EXTSRC="$ROOT/extensions/rainbow-pairs"; EXTVER=$(grep -oE '"version": *"[^"]+"' "$EXTSRC/extension.json" | head -1 | sed -E 's/.*"([^"]+)"$/\1/'); EXTVER=${EXTVER:-1.1.1}
mkdir -p "$H/extensions/rainbow-pairs/$EXTVER"; cp "$EXTSRC/extension.json" "$H/extensions/rainbow-pairs/$EXTVER/"; [ -f "$EXTSRC/rainbow_pairs.wasm" ] && cp "$EXTSRC/rainbow_pairs.wasm" "$H/extensions/rainbow-pairs/$EXTVER/"
printf '{\n  "format": 1,\n  "id": "rainbow-pairs",\n  "name": "Rainbow Pairs",\n  "version": "%s",\n  "kind": "wasm",\n  "placed": []\n}\n' "$EXTVER" > "$H/extensions/rainbow-pairs/installed.json"
run_s S87 "확장 뷰 탭 = 세션 표식 숨김(기본)(extensions.tab_badge)" "view.extensions,@after:1500:ext.view:rainbow-pairs,@after:2800:tabs.dump:$OUT/s87_tabs.txt" Local 4.3 $'extensions.enabled=on\n' "s87_tabs.txt:Extension"
run_s S88 "확장 뷰 탭 = 표식 켬(extensions.tab_badge=on)"       "view.extensions,@after:1500:ext.view:rainbow-pairs" Local 4 $'extensions.enabled=on\nextensions.tab_badge=on\n'
run_s S89 "탭 툴팁 저장 일시(ms · 맨 아래 줄)(115-8)"          "open:$A,@after:1200:ui.move:470/80" Local 4.3 $'tabs.tooltip=on\n'
S90SAVED="$D/s90_saved_new.sql"; rm -f "$S90SAVED"
run_s S90 "새 파일 저장 = 색인 재열거 플래시 + 트리 추가(T-299)" "project.load:$PROJ,@after:1000:project.panel:on,@after:2000:file.new,@after:2600:file.save_as:$S90SAVED,@after:3300:flash.dump:$OUT/s90_flash.txt" Local 4.2 "" "s90_flash.txt:background_active=true"
say "  file: saved_exists=$([ -f "$S90SAVED" ] && echo true || echo false)"
run_s S91 "기동 명령 @after → 설정 창(검색어 flash) 열림(T-305)" "@after:1200:edit.prefs:flash" Local 4.5
run_s S92 "기동 명령 @after → 라이선스 창 열림(T-305)"          "@after:1200:help.license" Local 4.5
CSVIMP="$D/s93_import.csv"; printf 'id,name\n1,a\n2,b\n' > "$CSVIMP"
run_s S93 "기동 명령 @after → Import 창 열림(T-305)"            "@after:1200:import.open:t_s93;$CSVIMP" Local 5
run_s S94 "부분 결과 + 필터 = 서버 승격(T-285 · grid.filter_server=auto)" "open:$Q5,@after:1200:run.all,@after:3000:grid.addfilter:0;gt;400,@after:5500:grid.dump:$OUT/s94_grid.txt" Local 6.7 "" "s94_grid.txt:promoted=true"

say ""
say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%T'))"
[ "$fail" = 0 ]
