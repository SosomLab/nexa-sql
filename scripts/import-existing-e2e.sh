#!/usr/bin/env bash
# import-existing-e2e.sh — ★ 기존 표 가져오기 사전 판정 E2E(D-455 · 127 §2-4 · 11-04): 격리 홈 + sqlite · 헤드리스 · 기동 명령만(키·마우스 주입 0).
#   판정 = `import.dump:` 산출 파일(자동 연결 · 차단 사유 · 호환 경고 · PK 표본 중복 · 시작 가능 여부) + 적재 뒤 행 수.
#   산출물 = target/import-existing-e2e/ 한 폴더(끝나면 격리 홈은 지운다 · 덤프는 남긴다).
# 사용: scripts/import-existing-e2e.sh [-e target/debug/nexa-sql(.exe)] [-n target/debug/nsql(.exe)]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXT=""; case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) EXT=".exe";; esac
APP="$ROOT/target/debug/nexa-sql$EXT"; CLI="$ROOT/target/debug/nsql$EXT"
while getopts "e:n:" o; do case $o in e) APP=$OPTARG;; n) CLI=$OPTARG;; esac; done
O="$ROOT/target/import-existing-e2e"; H="$O/home"; D="$O/data"; rm -rf "$O"; mkdir -p "$H" "$D"
w() { case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) cygpath -w "$1";; *) echo "$1";; esac; }
HW=$(w "$H"); DW=$(w "$D"); OW=$(w "$O")
NSQL_HOME=$HW $CLI conn add Local "sqlite:$(w "$H/local.sqlite")" -d sqlite --no-prompt >/dev/null 2>&1
printf "CREATE TABLE emp(id INTEGER PRIMARY KEY, name TEXT NOT NULL, sal NUMERIC, dept TEXT NOT NULL DEFAULT 'X', note TEXT);\nINSERT INTO emp(id,name) VALUES (1,'base');\n" | NSQL_HOME=$HW $CLI run -c Local - >/dev/null 2>&1
printf 'id,name,salary,extra\n2,kim,100.5,aa\n3,lee,200,bb\n' > "$D/a.csv"
printf 'id,name,sal\n5,a,x\n5,b,y\n' > "$D/dup.csv"
printf 'id,sal\n7,1\n' > "$D/nn.csv"
printf 'ui.lang=en\ndemo.prompted=on\nwindow.main_size=1375,945\n' > "$H/settings.conf"
run() { # id cmd secs
  local id=$1 cmd=$2 secs=$3
  NSQL_HOME=$HW NSQL_HEADLESS=1 NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" "$APP" Local >"$O/$id.out" 2>"$O/$id.err" &
  local p=$!; sleep "$secs"; kill -0 $p 2>/dev/null && kill $p
}
pass=0; fail=0; v() { if eval "$2"; then echo "  PASS $1"; pass=$((pass+1)); else echo "  FAIL $1"; fail=$((fail+1)); fi; }
has() { grep -q -- "$2" "$1" 2>/dev/null; }

echo "== A 기존 표: 같은 이름만 자동 연결 · 미지정 ✓ = 차단 → skip/map으로 풀고 시작"
run a "@after:1500:import.open:emp;$DW/a.csv,@after:4000:import.dump:$OW/a1.txt,@after:4500:import.set:skip=extra,@after:4800:import.set:map=salary=sal,@after:5300:import.dump:$OW/a2.txt,@after:5800:import.start,@after:8500:import.dump:$OW/a3.txt,@after:9000:file.exit" 10
v "a1 열 5개 읽힘" 'has "$O/a1.txt" "table_cols=Some(5)"'
v "a1 id·name 자동 연결" 'has "$O/a1.txt" "col id .* target=id " && has "$O/a1.txt" "col name .* target=name "'
v "a1 salary·extra 미지정" 'has "$O/a1.txt" "col salary .* target= " && has "$O/a1.txt" "col extra .* target= "'
v "a1 차단 = Unmapped salary·extra" 'has "$O/a1.txt" "Unmapped(\"salary\")" && has "$O/a1.txt" "Unmapped(\"extra\")" && has "$O/a1.txt" "start_blocked=Some"'
v "a2 차단 해소" 'has "$O/a2.txt" "blockers=\[\]" && has "$O/a2.txt" "start_blocked=None"'
v "a2 salary→sal 경고 없음(DECIMAL→NUMERIC)" 'has "$O/a2.txt" "col salary .* target=sal .* issues=\[\]"'
v "a3 2행 적재" 'has "$O/a3.txt" "result=Some(\"ok:2 rows"'
CNT=$(printf 'SELECT COUNT(*) FROM emp;\n' | NSQL_HOME=$HW $CLI run -c Local -f csv - 2>/dev/null | tail -1 | tr -d '\r')
v "emp 행 수 3" '[ "$CNT" = "3" ]'

echo "== B 새 표로 열고 → 기존 표 라디오(표 미선택 = 차단) → 표 고르면 열·자동 연결"
run b "@after:1500:import.open:new;$DW/a.csv,@after:4000:import.set:target=existing,@after:4500:import.dump:$OW/b1.txt,@after:4800:import.set:table=emp,@after:6500:import.dump:$OW/b2.txt,@after:7000:file.exit" 8
v "b1 표 미선택 차단" 'has "$O/b1.txt" "table_name=None" && has "$O/b1.txt" "start_blocked=Some(\"Choose the target table\")"'
v "b2 표 선택 → 열 5 · id 자동 연결" 'has "$O/b2.txt" "table_name=Some(\"emp\")" && has "$O/b2.txt" "table_cols=Some(5)" && has "$O/b2.txt" "col id .* target=id "'

echo "== C 경고: 글 값 → NUMERIC · PK 표본 중복(시작은 가능)"
run c "@after:1500:import.open:emp;$DW/dup.csv,@after:4000:import.dump:$OW/c1.txt,@after:4500:file.exit" 6
v "c1 sal 경고 TextIntoTyped" 'has "$O/c1.txt" "col sal .* issues=\[TextIntoTyped\]"'
v "c1 PK 표본 중복" 'has "$O/c1.txt" "pk_dup=true"'
v "c1 차단 없음(dept 기본값 · note NULL 허용)" 'has "$O/c1.txt" "blockers=\[\]" && has "$O/c1.txt" "start_blocked=None"'

echo "== D 차단: NOT NULL 열(name)에 연결된 원료 없음"
run d "@after:1500:import.open:emp;$DW/nn.csv,@after:4000:import.dump:$OW/d1.txt,@after:4500:file.exit" 6
v "d1 NotNullUnfed name" 'has "$O/d1.txt" "NotNullUnfed(\"name\")" && has "$O/d1.txt" "start_blocked=Some"'

echo "== 패닉 없음"
v "stderr에 panicked 없음" '! grep -l "panicked" "$O"/*.err >/dev/null 2>&1'
rm -rf "$H"
echo "RESULT pass=$pass fail=$fail (dumps: $O)"
[ "$fail" = 0 ]
