#!/usr/bin/env bash
# ★ T-8 가져오기 ⑦ 측정(docs/127 §6 · 11-04): SQLite 격리 홈에서 N행 표를 만들고 → csv / xlsx / parquet / jsonl로 내보낸 뒤 →
#   `nsql import --create`로 새 표에 다시 넣어 **형식별 적재 행/s · 살펴보기(--dry-run) 시간**을 잰다(Debug/Release exe 모두 가능 · 산출 = target/import-bench/).
#   사용: scripts/import-bench.sh [-n 100000] [-e <nsql exe>] · 되풀이 = 같은 폴더 덮어쓰기(61 §2-4 디스크 규칙) · 끝에 산출 삭제는 tmp-clean 몫.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
N=100000; CLI="$ROOT/target/debug/nsql"
while getopts "n:e:" o; do case $o in n) N=$OPTARG;; e) CLI=$OPTARG;; esac; done
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) CLI="${CLI%.exe}.exe"; w(){ cygpath -w "$1"; };; *) w(){ printf '%s' "$1"; };; esac
T="$ROOT/target/import-bench"; H="$T/home"; rm -rf "$T"; mkdir -p "$H"
export NSQL_HOME; NSQL_HOME="$(w "$H")"
DB="$(w "$H/bench.sqlite")"
"$CLI" conn add B "sqlite:$DB" -d sqlite --no-prompt >/dev/null 2>&1
now_ms(){ python -c 'import time;print(int(time.time()*1000))' 2>/dev/null || date +%s%3N; }
echo "== 원본 표 src($N행 · id INTEGER · name TEXT · amount NUMERIC · d TEXT 날짜 · flag INTEGER)"
printf 'CREATE TABLE src(id INTEGER PRIMARY KEY, name TEXT, amount NUMERIC, d TEXT, flag INTEGER);\nWITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < %s) INSERT INTO src SELECT x, %s || x, (x %% 1000) + 0.25, date(%s, (x %% 365) || %s), x %% 2 FROM c;\nSELECT COUNT(*) FROM src;\n' "$N" "'name_'" "'2024-01-01'" "' days'" | "$CLI" run -c B - 2>/dev/null | tail -3
for f in csv jsonl xlsx parquet; do
  t0=$(now_ms); "$CLI" export -c B -q "select * from src" -f $f -o "$(w "$T/src.$f")" >/dev/null 2>"$T/export-$f.err"; t1=$(now_ms)
  sz=$(stat -c %s "$T/src.$f" 2>/dev/null || stat -f %z "$T/src.$f")
  echo "export $f: $((t1-t0)) ms · $sz B"
done
echo "== 살펴보기(--dry-run · 표본 1000행)"
for f in csv jsonl xlsx parquet; do
  t0=$(now_ms); "$CLI" import -c B -t "t_$f" --dry-run "$(w "$T/src.$f")" >"$T/dry-$f.txt" 2>&1; t1=$(now_ms)
  echo "dry-run $f: $((t1-t0)) ms · $(grep -cE '^  [A-Za-z_0-9]+: ' "$T/dry-$f.txt") 열"
done
echo "== 적재(--create · 새 표 · 배치 1000 · 커밋 끝에 한 번)"
for f in csv jsonl xlsx parquet; do
  t0=$(now_ms); "$CLI" import -c B -t "t_$f" --create --commit-every 0 "$(w "$T/src.$f")" >"$T/import-$f.txt" 2>&1; rc=$?; t1=$(now_ms)
  rows=$(printf 'select count(*) from t_%s;\n' "$f" | "$CLI" run -c B - 2>/dev/null | grep -E '^ *[0-9]+ *$' | tr -d ' ' | tail -1)
  ms=$((t1-t0)); rate=$(( ms > 0 ? rows * 1000 / ms : 0 ))
  echo "import $f: rc=$rc · ${rows:-?}행 · $ms ms · $rate 행/s · $(tail -1 "$T/import-$f.txt" | cut -c1-120)"
done
echo "== 산출 = $T (tmp-clean 대상)"
