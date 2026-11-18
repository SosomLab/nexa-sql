#!/usr/bin/env bash
# win-error-types-e2e.sh — ★ 오류 유형별 위치 표시 · Output 두 번 클릭 이동 E2E(11-19 · 사용자 "오류도 유형별로 표시되고 바로 이동되는지 검증").
#   틀 = win-error-goto-e2e.sh(격리 홈 · 프로필 복사 · **읽기 접속만 · 객체 생성 0** · 키·마우스 주입 0 · 산출물 = -o 폴더 · 끝나면 격리 홈 삭제).
#   설정 = `editor.error_goto=off`(자동 캐럿 끔 → **Output 이동만으로** 캐럿이 가는지) · `editor.error_analyze=off` · `ui.lang=en` · Oracle = OCI.
#   문서 모양(케이스마다 파일 하나) = 1줄 정상 문장 · 2줄 주석 · 3줄 빈 줄 · 4줄부터 여러 줄 오류 문장(오류 토큰 = 문장 3번째 줄 이후 ·
#     탭 들여쓰기 뒤 · 한글 리터럴 뒤) · 그 뒤 정상 문장 하나. 기대 토큰 자리는 본문의 `§` 표식(파일에 쓸 때 지운다).
#   실행 방식 2가지 = (a) `run.all` (b) `editor.caret:<오류 문장 안>` 뒤 `run.statement`(= 캐럿 문장만 · 사용자가 겪은 "line 1" 방식).
#   명령 = open → (caret) → run → output.dump → editor.caret:0 → output.goto_error:1(뒤에서 첫 오류 머리줄 두 번 클릭과 같은 이동) → editor.dump.
#   판정(케이스 × 방식) = ① Output 머리줄 줄 번호가 오류 문장의 문서 줄 범위 안 ② `line L, col C`면 `at:` 조각 = 문서 L줄 C열부터 ·
#     ③ 이동 뒤 캐럿 = L:C(열 없으면 L:1) ④ 표식 유형은 L:C = 표식 ⑤ 위치 없는 유형은 열 없음 · L = 문장 시작 줄 · 캐럿 = L:1 ⑥ (a)와 (b)의 L:C 같음.
#   기대와 다르면 FAIL로 남기고 원문을 표에(판정 = 개발 세션).
# 사용: scripts/win-error-types-e2e.sh -o <출력폴더> -P <실제 NSQL_HOME> [-d APPDB:oracle-oci] [-c T1,T4]
#   -d = 프로필:방언(문서의 익명 이름 대신 **실제 볼트 프로필 이름**을 넘긴다 · `nsql conn list`) · -c = 케이스 일부만.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"
OUT=""; PROF=""; LIST="APPDB:oracle-oci"; ONLY=""
EXE="$ROOT/target/debug/nexa-sql.exe"
while getopts "o:P:d:g:c:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; d) LIST=$OPTARG;; g) EXE=$OPTARG;; c) ONLY=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out dir> -P <real NSQL_HOME> [-d 프로필:oracle-oci] [-c T1,T2] [-g gui]"; exit 2; }
mkdir -p "$OUT"
PY=$(command -v python3 || command -v python)
export PYTHONUTF8=1 PYTHONIOENCODING=utf-8
REPORT="$OUT/error-types-e2e.txt"; : > "$REPORT"
TABLE="$OUT/error-types-table.md"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

# 격리 홈(프로필·기기 키·비밀 사본 — 판정 직후 삭제).
H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/"
[ -d "$PROF/secrets" ] && cp -R "$PROF/secrets" "$H/"
cp "$PROF/device.key" "$H/" 2>/dev/null
cleanup() { rm -rf "$H"; }
trap cleanup EXIT
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\neditor.error_goto=off\neditor.error_analyze=off\noracle.driver=oci\n' > "$H/settings.conf"

# 도우미(python · 파일로 써서 실행 — 표준 입력 실행 금지): make = 문서 조립 · judge = 판정 한 줄.
HELP="$OUT/error_types_help.py"
cat > "$HELP" <<'PY'
import re, sys
PRE = "SELECT 1 AS a FROM dual;\n-- error types e2e\n\n"
POST = "SELECT 2 AS b FROM dual;\n"

def make(body_path, sql_path, plsql):
    body = open(body_path, encoding="utf-8").read().rstrip("\n")
    term = "\n/\n" if plsql == "1" else ";\n"
    doc = PRE + body + term + POST
    mark = doc.find("§")
    doc_clean = doc.replace("§", "")
    open(sql_path, "w", encoding="utf-8", newline="\n").write(doc_clean)
    s_line = PRE.count("\n") + 1
    e_line = s_line + body.count("\n")
    # 캐럿(b) = 오류 문장 3번째 줄 첫 글자 + 1(글자 인덱스 · 0부터).
    lines = doc_clean.split("\n")
    caret = sum(len(l) + 1 for l in lines[: s_line + 1]) + 1
    if mark >= 0:
        before = doc[:mark]
        ml = before.count("\n") + 1
        mc = len(before) - (before.rfind("\n") + 1) + 1
        mk = f"{ml}:{mc}"
    else:
        mk = "-"
    print(s_line, e_line, mk, caret)

def judge(sql_path, out_path, ed_path, s_line, e_line, mark, kind):
    doc = open(sql_path, encoding="utf-8").read().split("\n")
    try:
        out = open(out_path, encoding="utf-8").read().split("\n")
    except OSError:
        out = []
    try:
        ed = open(ed_path, encoding="utf-8").readline()
    except OSError:
        ed = ""
    s_line, e_line = int(s_line), int(e_line)
    if kind == "tail":
        e_line = len(doc)
    hi = None
    for i, l in enumerate(out):
        if "✖" in l:
            hi = i
    if hi is None:
        print("\t".join(["?", "?", "(Output에 ✖ 줄 없음)", "FAIL", "NA", "FAIL", "NA", "NA", "-"]))
        return
    head = out[hi].strip()
    m = re.search(r"line (\d+)(?:, col (\d+))?: (.*)$", head)
    code = (re.search(r"ORA-\d+", head) or re.search(r"\[(-?\d+)\]", head))
    code = code.group(0) if code else "?"
    if not m:
        print("\t".join([code, "?", head, "FAIL", "NA", "FAIL", "NA", "NA", "-"]))
        return
    L = int(m.group(1)); C = int(m.group(2)) if m.group(2) else None
    at = None
    if hi + 1 < len(out) and out[hi + 1].strip().startswith("at:"):
        at = out[hi + 1].strip()[3:].strip()
    line_txt = doc[L - 1] if 0 < L <= len(doc) else ""
    tok = "-"
    if C:
        t = re.match(r"[A-Za-z0-9_$#.]+|\S", line_txt[C - 1:])
        tok = t.group(0) if t else "(빈)"
    r1 = "PASS" if s_line <= L <= e_line else "FAIL"
    if C is None:
        r2 = "NA"
    elif at is None:
        r2 = "FAIL(at 없음)"
    else:
        r2 = "PASS" if line_txt[C - 1:].strip().startswith(at) or line_txt[C - 1:].startswith(at) else f"FAIL(at={at!r})"
    cm = re.search(r"caret=(\d+)(?::(\d+))?", ed)
    want = f"{L}:{C or 1}"
    got = f"{cm.group(1)}:{cm.group(2) or 1}" if cm else "?"
    r3 = "PASS" if got == want else f"FAIL(caret={got})"
    r4 = "NA"
    if kind == "mark":
        r4 = "PASS" if C is not None and f"{L}:{C}" == mark else f"FAIL(기대 {mark})"
    r5 = "NA"
    if kind == "none":
        r5 = "PASS" if C is None and L == s_line and got == f"{L}:1" else "FAIL"
    lc = f"{L}:{C}" if C else f"{L}"
    print("\t".join([code, tok, head + (f" ⏎ at: {at}" if at else ""), r1, r2, r3, r4, r5, lc]))

if sys.argv[1] == "make":
    make(*sys.argv[2:5])
else:
    judge(*sys.argv[2:9])
PY

# 케이스 = 이름 · 유형(mark = 표식 기대 · none = 위치 없음 기대 · free = 서버가 준 자리 기록 · tail = 문서 끝까지 한 문장) · PL/SQL(1) · 본문.
TAB=$'\t'
declare -a CASES=(
"T1|mark|0|SELECT d.dummy
     , '가나다' AS k
${TAB}, '라마' AS k2, d.§NO_SUCH_COL AS c
  FROM dual d"
"T2|mark|0|SELECT t.dummy
     , '가나다' AS k
${TAB}FROM §NO_SUCH_TABLE_X t"
"T3|free|0|SELECT '가나다' AS k
     , 1 AS n
${TAB}, '라마' AS k2, 1 a b FROM dual"
"T4|free|0|SELECT '가나다' AS k
     , 1 AS n
${TAB}, '라마' AS k2, LISTAGG2(DISTINCT d.dummy, ',') WITHIN GROUP (ORDER BY d.dummy) AS x
  FROM dual d"
"T5|free|0|SELECT '가나다' AS k
     , 1 AS n
${TAB}, '라마' AS k2, (1 + 2 AS x
  FROM dual"
"T6|free|0|SELECT '가나다' AS k
     , 1 AS n
${TAB}, '라마' AS k2,
  FROM dual"
"T7|free|0|SELECT '가나다' AS k
     , 1 AS n
  FROM dual
${TAB}WHERE '라마' = '라마' ORDER BY 1 extra_tok"
"T8|tail|0|SELECT '가나다' AS k
     , 1 AS n
${TAB}, '라마' AS k2, 'abc AS x
  FROM dual"
"T9|free|0|SELECT '가나다' AS k
     , d.dummy
${TAB}, '라마' AS k2, COUNT(*) AS n
  FROM dual d
 GROUP BY '가나다'"
"T10|none|0|SELECT '가나다' AS k
     , 1 AS n
${TAB}, '라마' AS k2, TO_NUMBER('x') AS v
  FROM dual"
"T11|mark|1|BEGIN
  NULL;
${TAB}DBMS_OUTPUT.PUT_LINE('가나다'); §no_such_proc_x(1);
END;"
"T12|none|1|DECLARE
  v VARCHAR2(2);
BEGIN
${TAB}v := '가나다';
END;"
)

# run_case <프로필> <케이스> <방식 a|b>
run_case() {
  local conn=$1 spec=$2 mode=$3
  local name=${spec%%|*}; local rest=${spec#*|}; local kind=${rest%%|*}; rest=${rest#*|}; local plsql=${rest%%|*}; local body=${rest#*|}
  local id="$name-$mode" b="$OUT/$name.body" sql="$OUT/$name.sql"
  printf '%s' "$body" > "$b"
  read -r s_line e_line mark caret <<< "$("$PY" -I "$HELP" make "$b" "$sql" "$plsql")"
  local outd="$OUT/$id.out" edd="$OUT/$id.dump" log="$OUT/$id.log"
  rm -f "$outd" "$edd" "$log"
  local run="@after:5000:run.all"
  [ "$mode" = b ] && run="@after:5000:editor.caret:$caret,@after:5400:run.statement"
  local cmd="@connected:open:$(w "$sql"),$run,@after:12000:output.dump:$(w "$outd"),@after:12300:log.dump:$(w "$log"),@after:12600:editor.caret:0,@after:12900:output.goto_error:1,@after:13300:editor.dump:$(w "$edd")"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 18 "$EXE" "$conn" > "$OUT/$id.stdout" 2> "$OUT/$id.stderr"
  local row; row=$("$PY" -I "$HELP" judge "$sql" "$outd" "$edd" "$s_line" "$e_line" "$mark" "$kind")
  echo "$name|$mode|$s_line-$e_line|$mark|$row" >> "$OUT/rows.txt"
  say "  $id 문장 줄 $s_line-$e_line · 표식 $mark → $(echo "$row" | cut -f9) · $(echo "$row" | cut -f4-8 | tr '\t' ' ')"
}

say "error-types-e2e $(date '+%F %T') · 대상 = $LIST · 설정 = error_goto off · error_analyze off"
: > "$OUT/rows.txt"
IFS=',' read -ra PAIRS <<< "$LIST"
for pair in "${PAIRS[@]}"; do
  conn=${pair%%:*}
  for spec in "${CASES[@]}"; do
    n=${spec%%|*}
    if [ -n "$ONLY" ] && ! echo ",$ONLY," | grep -q ",$n,"; then continue; fi
    run_case "$conn" "$spec" a
    run_case "$conn" "$spec" b
  done
done

# 표(케이스 × 방식) + ⑥ (a)/(b) L:C 비교.
{
  echo "| 케이스 | 방식 | 문장 줄 | 표식 | 코드 | 서버 자리 토큰 | Output 머리줄(원문) | ① | ② | ③ | ④ | ⑤ | ⑥ |"
  echo "|---|---|---|---|---|---|---|---|---|---|---|---|---|"
  while IFS='|' read -r n mode rng mk row; do
    IFS=$'\t' read -r code tok head r1 r2 r3 r4 r5 lc <<< "$row"
    other=$(grep "^$n|$([ "$mode" = a ] && echo b || echo a)|" "$OUT/rows.txt" | cut -d'|' -f5- | cut -f9)
    if [ -z "$other" ]; then r6=NA; elif [ "$other" = "$lc" ]; then r6=PASS; else r6="FAIL($lc≠$other)"; fi
    echo "| $n | $mode | $rng | $mk | $code | \`$tok\` | \`$(echo "$head" | sed 's/|/¦/g')\` | $r1 | $r2 | $r3 | $r4 | $r5 | $r6 |"
  done < "$OUT/rows.txt"
} > "$TABLE"
cat "$TABLE" >> "$REPORT"
P=$(grep -o "PASS" "$TABLE" | wc -l); F=$(grep -o "FAIL" "$TABLE" | wc -l)
say "RESULT PASS $P FAIL $F(표 = $TABLE)"
[ "$F" -eq 0 ]
