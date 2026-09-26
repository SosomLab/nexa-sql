#!/usr/bin/env bash
# linux-func-check.sh — 기능 점검 자동화 **Linux판**(win-func-check.ps1 이식 · 사용자 09-26 "전체 개발 기능 리눅스 동작 전수 검사").
#   시나리오마다 앱을 격리 홈으로 띄우고(`NSQL_STARTUP_CMD`로만 몬다 · OS 키·마우스 주입 0 · docs/61 §4) **자동 판정**한다.
#   Windows판과 다른 점: 이 VM에는 창 캡처 도구가 없다(import·scrot·xdotool 전부 없음) → 캡처 대신
#     ① 프로세스 생존 ② 창 수(`xwininfo -root -tree`의 WM_CLASS `nexa-sql`) ③ stderr 패닉 없음
#     ④ **덤프 명령의 산출 파일 검사**(explorer/grid/details/cellview/txlog/mem/import/sqlprev.dump) ⑤ 저장 파일 검사
#   로 판정한다 — 눈으로 봐야 하는 것(색·툴팁 위치)은 사용자 실기(T-163)로 넘긴다.
#
# 사용: scripts/linux-func-check.sh -o <출력폴더> [-e target/debug/nexa-sql] [-n target/debug/nsql] [-O S05,S06]
set -u
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
wins_now() { xwininfo -root -tree 2>/dev/null | grep -c '("nexa-sql" "nexa-sql")'; }

# ── 시험 자료 ──────────────────────────────────────────────────────────────
cat > "$D/q500.sql" <<'SQL'
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 500)
SELECT i AS id, 'name_' || i AS name, CASE WHEN i % 3 = 0 THEN NULL ELSE i * 1.5 END AS amount, 'SEBANG' AS project_cd, 'memo ' || i AS memo FROM n;
SQL
echo "SELECT * FROM no_such_table_zz;" > "$D/q_err.sql"
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

"$CLI" conn add Local "sqlite:$H/local.sqlite" -d sqlite --no-prompt >/dev/null 2>&1 || true
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
  if kill -0 "$p" 2>/dev/null; then alive=1; wins=$(wins_now); fi
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

A="$D/a.sql"; B="$D/b.sql"; C="$D/c.sql"; Q5="$D/q500.sql"; QE="$D/q_err.sql"; QT="$D/q_two.sql"
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
run_s S13 "그리드 셀 선택·NULL 흐림(§44·49)"          "open:$Q5,@after:1200:run.all,@after:3000:ui.click:420/580,@after:4500:grid.dump:$OUT/s13.txt" Local 5.5 "" "s13.txt:rows="
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

say ""
say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%T'))"
[ "$fail" = 0 ]
