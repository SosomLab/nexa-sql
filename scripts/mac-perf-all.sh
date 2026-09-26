#!/usr/bin/env bash
# mac-perf-all.sh — 맥 성능 전수(linux-perf-all.sh 이식 · 09-24)(26 §7-3 구성 · 사용자 09-22 "Windows·Mac과 동일하게"): 기동 → 시나리오 → 릭 주기 → CLI 타이밍.
#   Release · 격리 설정 폴더 · 입력 주입 없음 · 결과 = <out>/perf.txt(+ 시나리오별 stderr에 `[frames]`/`[load]`).
#   시험 파일(200 KB · 2 MB · 6.4 MB · 20 MB CRLF·한글 · 65 MB · 10만 행 질의)은 <data>에 없으면 만든다.
#
# 사용:  scripts/mac-perf-all.sh -H /tmp/nsql-home -D /tmp/nsql-data -o /tmp/nsql-perf [-p Local]
#        신규 기능(Oracle) 시나리오 s9~s13 · leak.search/details = NSQL_PERF_ORACLE_TARGET=<접속 문자열|프로필> 일 때만(09-26).
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
# ★ 신규 기능 시나리오(09-26 · 100차 후반 = 탐색기 검색 인덱스 84 · 메타 3층 85 · 코멘트 워머 · 객체 상세 86 · SQL Preview 83) — 실서버가 있어야 의미가 있어
#   환경 변수 NSQL_PERF_ORACLE_TARGET(접속 문자열 또는 <home> 볼트의 프로필 이름)이 있을 때만 돈다. 행 번호 = 접속 직후 트리(0 서버 · 1 현재 스키마 · 2 첫 폴더 · 3 첫 객체).
if [ -n "${NSQL_PERF_ORACLE_TARGET:-}" ]; then
ORA="$NSQL_PERF_ORACLE_TARGET"; export NSQL_TRACE_FRAMES=1
probe s9.ora.idx      24 "@after:22000:explorer.stat:$OUT/s9.stat,@after:22500:mem.dump:$OUT/s9.mem" "$ORA"
NSQL_HOME=$H "$NSQL" config set explorer.search_index off >/dev/null 2>&1; NSQL_HOME=$H "$NSQL" config set meta.warm_comments off >/dev/null 2>&1; NSQL_HOME=$H "$NSQL" config set meta.warm_columns_max 0 >/dev/null 2>&1
probe s9b.ora.nowarm  24 "@after:22000:explorer.stat:$OUT/s9b.stat,@after:22500:mem.dump:$OUT/s9b.mem" "$ORA"
NSQL_HOME=$H "$NSQL" config reset explorer.search_index >/dev/null 2>&1; NSQL_HOME=$H "$NSQL" config reset meta.warm_comments >/dev/null 2>&1; NSQL_HOME=$H "$NSQL" config reset meta.warm_columns_max >/dev/null 2>&1
probe s10.ora.search  26 "@after:5000:explorer.filter:1171,@after:24000:explorer.stat:$OUT/s10.stat,@after:24500:mem.dump:$OUT/s10.mem" "$ORA"
# 행 번호(트리 = 0 루트 · 1~ 스키마 알파벳 · 현재 스키마는 자동 펼침 → 그 아래 첫 폴더 Tables) — 서버마다 다르므로 환경 변수로 덮어쓴다(BISCM = Tables 6 · 첫 테이블 7).
RT=${NSQL_PERF_ROW_TABLES:-6}; RO=$((RT+1)); RO2=$((RT+2)); RO3=$((RT+3))
probe s11.ora.details 28 "@after:5000:view.object_details,@after:6000:explorer.expand:$RT,@after:10000:explorer.select$RO,@after:14000:explorer.select$RO2,@after:18000:explorer.expand:$RO,@after:21000:explorer.select$RO2,@after:25000:details.dump:$OUT/s11.details,@after:25500:mem.dump:$OUT/s11.mem" "$ORA"
probe s12.ora.sqlprev 24 "@after:6000:explorer.expand:$RT,@after:10000:explorer.menu:$RO,@after:11000:explorer.pick:gen:ddl,@after:20000:sqlprev.dump:$OUT/s12.sqlprev,@after:20500:mem.dump:$OUT/s12.mem" "$ORA"
NSQL_HOME=$H "$NSQL" config set meta.detail_ttl_secs 15 >/dev/null 2>&1; NSQL_HOME=$H "$NSQL" config set meta.cols_ttl_secs 15 >/dev/null 2>&1
probe s13.ora.reclaim 62 "@after:5000:view.object_details,@after:6000:explorer.expand:$RT,@after:10000:explorer.select$RO,@after:13000:explorer.select$RO2,@after:16000:explorer.select$RO3,@after:19000:mem.dump:$OUT/s13a.mem,@after:59000:mem.dump:$OUT/s13b.mem" "$ORA"
NSQL_HOME=$H "$NSQL" config reset meta.detail_ttl_secs >/dev/null 2>&1; NSQL_HOME=$H "$NSQL" config reset meta.cols_ttl_secs >/dev/null 2>&1
unset NSQL_TRACE_FRAMES
say "=== leak: explorer search on/off ×8 (oracle)"; bash "$SC/mac-leak.sh" -H "$H" -n 8 -p 6000 -S 8000 -C "explorer.filter:1171;explorer.filter:" -a "$ORA" -t leak.search | tee -a "$R"
say "=== leak: object details toggle ×10 (oracle)"; bash "$SC/mac-leak.sh" -H "$H" -n 10 -p 2500 -S 8000 -F "explorer.expand:$RT;explorer.select$RO" -C "view.object_details;view.object_details" -a "$ORA" -t leak.details | tee -a "$R"
for f in s9.stat s9b.stat s10.stat s9.mem s9b.mem s10.mem s11.mem s12.mem s13a.mem s13b.mem s11.details s12.sqlprev; do [ -f "$OUT/$f" ] && { say "--- $f"; head -c 1500 "$OUT/$f" | tee -a "$R"; echo | tee -a "$R"; }; done
fi
say "=== leak: open 6 MB → close ×8"; bash "$SC/mac-leak.sh" -H "$H" -n 8 -p 5000 -C "open:$D/big6m.sql;file.close_tab" -a "$PROF" -t leak.big | tee -a "$R"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 100000 >/dev/null 2>&1
say "=== leak: run 100k rows ×8"; bash "$SC/mac-leak.sh" -H "$H" -n 8 -p 7000 -F "open:$D/rows100k.sql" -C "run.all" -a "$PROF" -t leak.rows | tee -a "$R"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 200 >/dev/null 2>&1
say "=== leak: log window toggle ×10"; bash "$SC/mac-leak.sh" -H "$H" -n 10 -p 2000 -C "view.log;view.log" -a "$PROF" -t leak.log | tee -a "$R"
say "=== leak: 2 MB tab open/close ×8"; bash "$SC/mac-leak.sh" -H "$H" -n 8 -p 4000 -C "open:$D/mid2m.sql;file.close_tab" -a "$PROF" -t leak.mid | tee -a "$R"
say "=== CLI timing (3 runs each)"
# ★ wall = scripts/cli-wall.py(프로세스 12회 · 예열 2 · min/med · T-228 09-26: 종전 python3 두 번 띄우기가 실행마다 +150 ms를 얹어 12 ms 실행이 156~210 ms로 보였다).
cli() { local tag=$1 prof=$2 f=$3 home=${4:-}; local wall line; wall=$( { [ -n "$home" ] && export NSQL_HOME=$home; python3 "$ROOT/scripts/cli-wall.py" -- "$NSQL" run -c "$prof" "$f"; } ); line=$( { [ -n "$home" ] && export NSQL_HOME=$home; "$NSQL" run -c "$prof" "$f" --timing 2>&1; } | grep -E '^⏱' | tr '\n' ' '); say "$tag $wall $line"; }
cli cli.sqlite.100k "$PROF" "$D/rows100k.sql" "$H"
cli cli.sqlite.200  "$PROF" "$D/rows200.sql" "$H"
if [ -n "${NSQL_PERF_ORACLE:-}" ]; then printf "SELECT LEVEL AS n, RPAD('x', 900, 'y') AS pad FROM dual CONNECT BY LEVEL <= 200;\n" > "$D/q_ora.sql"; printf 'SELECT COUNT(*) FROM user_objects;\n' > "$D/q_ora_cnt.sql"; cli cli.oracle.200 "$NSQL_PERF_ORACLE" "$D/q_ora.sql"; cli cli.oracle.cnt "$NSQL_PERF_ORACLE" "$D/q_ora_cnt.sql"; fi
if [ -n "${NSQL_PERF_MSSQL:-}" ]; then printf 'SELECT TOP 200 * FROM INFORMATION_SCHEMA.COLUMNS;\n' > "$D/q_ms.sql"; cli cli.mssql.200 "$NSQL_PERF_MSSQL" "$D/q_ms.sql"; fi
if [ -n "${NSQL_PERF_PG:-}" ]; then printf 'SELECT * FROM pg_catalog.pg_class LIMIT 200;\n' > "$D/q_pg.sql"; cli cli.pg.200 "$NSQL_PERF_PG" "$D/q_pg.sql"; fi
say "=== done $(date)"
