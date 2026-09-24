#!/usr/bin/env bash
# mac-perf-all.sh — 맥 성능 전수(linux-perf-all.sh 이식 · 09-24)(26 §7-3 구성 · 사용자 09-22 "Windows·Mac과 동일하게"): 기동 → 시나리오 → 릭 주기 → CLI 타이밍.
#   Release · 격리 설정 폴더 · 입력 주입 없음 · 결과 = <out>/perf.txt(+ 시나리오별 stderr에 `[frames]`/`[load]`).
#   시험 파일(200 KB · 2 MB · 6.4 MB · 20 MB CRLF·한글 · 65 MB · 10만 행 질의)은 <data>에 없으면 만든다.
#
# 사용:  scripts/mac-perf-all.sh -H /tmp/nsql-home -D /tmp/nsql-data -o /tmp/nsql-perf [-p Local]
#        (프로필 `Local`은 <home>에 `nsql conn add Local sqlite:<home>/local.sqlite -d sqlite --no-prompt`로 만든다 · 실서버 CLI 타이밍은
#         환경 변수 NSQL_PERF_ORACLE · NSQL_PERF_MSSQL · NSQL_PERF_PG 에 **사용자 볼트의 프로필 이름**을 주면 그때만 돈다)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; . "$ROOT/scripts/mac-common.sh"
H=""; D=""; OUT=""; PROF=Local
while getopts "H:D:o:p:" o; do case $o in H) H=$OPTARG;; D) D=$OPTARG;; o) OUT=$OPTARG;; p) PROF=$OPTARG;; esac; done
[ -d "$H" ] && [ -n "$D" ] && [ -n "$OUT" ] || { echo "usage: -H <home> -D <data> -o <out> [-p profile]"; exit 2; }
mkdir -p "$D" "$OUT"; R="$OUT/perf.txt"; : > "$R"
NSQL="$ROOT/target/release/nsql"; APP="$ROOT/target/release/nexa-sql"; SC="$ROOT/scripts"
say() { echo "$@" | tee -a "$R"; }
if [ ! -f "$D/big65m.sql" ]; then
python3 - "$D" <<'PY'
import random, sys; random.seed(7); D=sys.argv[1]
tables=["orders","customers","products","invoices","shipments","emp","dept"]; cols=["id","name","amount","created_at","status","project_cd","qty","memo"]
def line(i, ko):
    t=random.choice(tables); c=random.choice(cols); c2=random.choice(cols)
    if i%7==0: return f"-- section {i}: {'주석 한글 설명 줄' if ko else 'comment line'} {random.randint(1,999999)}"
    if i%5==0: return f"INSERT INTO {t} ({c}, {c2}, status) VALUES ({i}, '{'값_' if ko else 'v_'}{random.randint(1,99999)}', 'A');"
    if i%3==0: return f"UPDATE {t} SET {c} = {c} + {random.randint(1,50)} WHERE {c2} = {i} AND status IN ('A','B');"
    return f"SELECT {c}, {c2}, COUNT(*) AS cnt FROM {t} WHERE {c} > {i} GROUP BY {c}, {c2} ORDER BY cnt DESC;"
def gen(name, target, ko=False, crlf=False):
    eol="\r\n" if crlf else "\n"; size=0; i=0
    with open(f"{D}/{name}","w",encoding="utf-8",newline="") as f:
        while size<target: s=line(i,ko)+eol; f.write(s); size+=len(s.encode()); i+=1
gen("small200k.sql",200_000); gen("mid2m.sql",2_000_000); gen("big6m.sql",6_400_000); gen("big20m_ko_crlf.sql",20_000_000,True,True); gen("big65m.sql",65_000_000)
q="WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<%d)\nSELECT x AS id, 'name_'||x AS name, x*1.5 AS amount, 'status'||(x%%7) AS status, 'memo text for row '||x AS memo FROM n;\n"
open(f"{D}/rows100k.sql","w").write(q%100000); open(f"{D}/rows200.sql","w").write(q%200)
PY
fi
say "=== env: $(sysctl -n hw.ncpu) cores · $(( $(sysctl -n hw.memsize)/1048576 )) MB RAM · $(sysctl -n machdep.cpu.brand_string) · macOS $(sw_vers -productVersion) · $(date)"
say "=== sizes"; ls -l "$APP" "$NSQL" | awk '{printf "%s %.2f MB\n",$NF,$5/1048576}' | tee -a "$R"
NSQL_HOME=$H NSQL_NO_ACTIVATE=1 "$APP" >/dev/null 2>&1 & P=$!; sleep 4; kill $P; wait $P 2>/dev/null   # 빌드 직후 첫 실행은 버린다(26 §7-5)
say "=== startup (5 runs · login+main)"; bash "$SC/mac-startup.sh" -H "$H" -n 5 -s 3 -t startup | tee -a "$R"
say "=== startup connected $PROF (5 runs)"; bash "$SC/mac-startup.sh" -H "$H" -n 5 -s 3 -a "$PROF" -t startup.conn | tee -a "$R"
export NSQL_TRACE_FRAMES=1
probe() { local tag=$1 secs=$2 cmd=$3 args=${4:-}; say "=== scenario $tag"; bash "$SC/mac-probe.sh" -H "$H" -s "$secs" -t "$tag" -c "$cmd" -a "$args" | tail -4 | tee -a "$R"; grep -E '^\[frames\] (tracing|n=)|^\[load\]|^\[startup\]' "$H/$tag.stderr" | tail -4 | tee -a "$R"; }
probe s1.idle        8  ""
probe s2.connected   10 "" "$PROF"
probe s3.win4        12 "@after:3000:view.log,@after:3500:view.variables,@after:4000:view.colors,@after:4500:view.keys" "$PROF"
probe s4.ext         10 "@after:3000:view.extensions" "$PROF"
probe s5.script2m    12 "open:$D/mid2m.sql" "$PROF"
probe s5b.script6m   14 "open:$D/big6m.sql,@after:2500:bigfile.open" "$PROF"
probe s5c.script20m  16 "open:$D/big20m_ko_crlf.sql,@after:2500:bigfile.open" "$PROF"
probe s5d.script65m  22 "open:$D/big65m.sql,@after:2500:bigfile.open" "$PROF"
probe s6.rows200     12 "open:$D/rows200.sql,@after:3000:run.all" "$PROF"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 100000 >/dev/null 2>&1      # 10만 행은 상한을 올려야 실제로 받는다(기본 200)
probe s7.rows100k    14 "open:$D/rows100k.sql,@after:3000:run.all" "$PROF"
probe s7b.rows100k.close 20 "open:$D/rows100k.sql,@after:3000:run.all,@after:10000:file.close_tab" "$PROF"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 200 >/dev/null 2>&1
NSQL_HOME=$H "$NSQL" config set perf.boost on >/dev/null 2>&1
probe s8.boost       10 "" "$PROF"
NSQL_HOME=$H "$NSQL" config set perf.boost off >/dev/null 2>&1
unset NSQL_TRACE_FRAMES
say "=== leak: open 6 MB → close ×8"; bash "$SC/mac-leak.sh" -H "$H" -n 8 -p 5000 -C "open:$D/big6m.sql;file.close_tab" -a "$PROF" -t leak.big | tee -a "$R"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 100000 >/dev/null 2>&1
say "=== leak: run 100k rows ×8"; bash "$SC/mac-leak.sh" -H "$H" -n 8 -p 7000 -F "open:$D/rows100k.sql" -C "run.all" -a "$PROF" -t leak.rows | tee -a "$R"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 200 >/dev/null 2>&1
say "=== leak: log window toggle ×10"; bash "$SC/mac-leak.sh" -H "$H" -n 10 -p 2000 -C "view.log;view.log" -a "$PROF" -t leak.log | tee -a "$R"
say "=== leak: 2 MB tab open/close ×8"; bash "$SC/mac-leak.sh" -H "$H" -n 8 -p 4000 -C "open:$D/mid2m.sql;file.close_tab" -a "$PROF" -t leak.mid | tee -a "$R"
say "=== CLI timing (3 runs each)"
cli() { local tag=$1 prof=$2 f=$3 home=${4:-}; for i in 1 2 3; do local t0=$(python3 -c 'import time;print(int(time.time()*1000))'); local line; line=$( { [ -n "$home" ] && export NSQL_HOME=$home; "$NSQL" run -c "$prof" "$f" --timing 2>&1; } | grep -E '^⏱' | tail -1); say "$tag run$i wall=$(( $(python3 -c 'import time;print(int(time.time()*1000))')-t0 ))ms $line"; done; }
cli cli.sqlite.100k "$PROF" "$D/rows100k.sql" "$H"
cli cli.sqlite.200  "$PROF" "$D/rows200.sql" "$H"
if [ -n "${NSQL_PERF_ORACLE:-}" ]; then printf "SELECT LEVEL AS n, RPAD('x', 900, 'y') AS pad FROM dual CONNECT BY LEVEL <= 200;\n" > "$D/q_ora.sql"; printf 'SELECT COUNT(*) FROM user_objects;\n' > "$D/q_ora_cnt.sql"; cli cli.oracle.200 "$NSQL_PERF_ORACLE" "$D/q_ora.sql"; cli cli.oracle.cnt "$NSQL_PERF_ORACLE" "$D/q_ora_cnt.sql"; fi
if [ -n "${NSQL_PERF_MSSQL:-}" ]; then printf 'SELECT TOP 200 * FROM INFORMATION_SCHEMA.COLUMNS;\n' > "$D/q_ms.sql"; cli cli.mssql.200 "$NSQL_PERF_MSSQL" "$D/q_ms.sql"; fi
if [ -n "${NSQL_PERF_PG:-}" ]; then printf 'SELECT * FROM pg_catalog.pg_class LIMIT 200;\n' > "$D/q_pg.sql"; cli cli.pg.200 "$NSQL_PERF_PG" "$D/q_pg.sql"; fi
say "=== done $(date)"
