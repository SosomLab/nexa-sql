#!/usr/bin/env bash
# linux-perf-all.sh — Linux 성능 전수(26 §7-3 구성 · 사용자 09-22 "Windows·Mac과 동일하게"): 기동 → 시나리오 → 릭 주기 → CLI 타이밍.
#   Release · 격리 설정 폴더 · 입력 주입 없음 · 결과 = <out>/perf.txt(+ 시나리오별 stderr에 `[frames]`/`[load]`).
#   시험 파일(200 KB · 2 MB · 6.4 MB · 20 MB CRLF·한글 · 65 MB · 10만 행 질의)은 <data>에 없으면 만든다.
#
# 사용:  scripts/linux-perf-all.sh -H /tmp/nsql-home -D /tmp/nsql-data -o /tmp/nsql-perf [-p Local]
#        (프로필 `Local`은 <home>에 `nsql conn add Local sqlite:<home>/local.sqlite -d sqlite --no-prompt`로 만든다 · 실서버 CLI 타이밍은
#         환경 변수 NSQL_PERF_ORACLE · NSQL_PERF_MSSQL · NSQL_PERF_PG 에 **사용자 볼트의 프로필 이름**을 주면 그때만 돈다)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
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
say "=== env: $(nproc) cores · $(free -m | awk '/^Mem/{print $2" MB RAM"}') · $(uname -sr) · $(date)"
say "=== sizes"; ls -la "$APP" "$NSQL" | awk '{printf "%s %.2f MB\n",$9,$5/1048576}' | tee -a "$R"
NSQL_HOME=$H NSQL_NO_ACTIVATE=1 "$APP" >/dev/null 2>&1 & P=$!; sleep 4; kill $P; wait $P 2>/dev/null   # 빌드 직후 첫 실행은 버린다(26 §7-5)
say "=== startup (5 runs · login+main)"; bash "$SC/linux-startup.sh" -H "$H" -n 5 -s 3 -t startup | tee -a "$R"
say "=== startup connected $PROF (5 runs)"; bash "$SC/linux-startup.sh" -H "$H" -n 5 -s 3 -a "$PROF" -t startup.conn | tee -a "$R"
export NSQL_TRACE_FRAMES=1
probe() { local tag=$1 secs=$2 cmd=$3 args=${4:-}; say "=== scenario $tag"; bash "$SC/linux-probe.sh" -H "$H" -s "$secs" -t "$tag" -c "$cmd" -a "$args" | tail -4 | tee -a "$R"; grep -E '^\[frames\] (tracing|n=)|^\[load\]' "$H/$tag.stderr" | tail -3 | tee -a "$R"; }
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
say "=== leak: open 6 MB → close ×8"; bash "$SC/linux-leak.sh" -H "$H" -n 8 -p 5000 -C "open:$D/big6m.sql;file.close_tab" -a "$PROF" -t leak.big | tee -a "$R"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 100000 >/dev/null 2>&1
say "=== leak: run 100k rows ×8"; bash "$SC/linux-leak.sh" -H "$H" -n 8 -p 7000 -F "open:$D/rows100k.sql" -C "run.all" -a "$PROF" -t leak.rows | tee -a "$R"
NSQL_HOME=$H "$NSQL" config set grid.max_rows 200 >/dev/null 2>&1
say "=== leak: log window toggle ×10"; bash "$SC/linux-leak.sh" -H "$H" -n 10 -p 2000 -C "view.log;view.log" -a "$PROF" -t leak.log | tee -a "$R"
say "=== leak: 2 MB tab open/close ×8"; bash "$SC/linux-leak.sh" -H "$H" -n 8 -p 4000 -C "open:$D/mid2m.sql;file.close_tab" -a "$PROF" -t leak.mid | tee -a "$R"

# ── 09-26 신설 시나리오 8~12(docs/71 §3-C · 탐색기 검색 인덱스 · 메타 3층 · 객체 상세 · SQL Preview · L3 회수) ──
#    실서버 프로필이 있을 때만 돈다(NSQL_PERF_ORACLE). Linux 첫 측정(91차에는 이 기능들이 없었다).
if [ -n "${NSQL_PERF_ORACLE:-}" ]; then
  OP="$NSQL_PERF_ORACLE"
  export NSQL_TRACE_FRAMES=1
  say "=== scenario s8.meta (탐색기 검색 인덱스 + 메타 3층 워머 · 84 · 85)"
  bash "$SC/linux-probe.sh" -H "$H" -s 26 -t s8.meta -c "@after:22000:explorer.stat:$OUT/s8.stat.txt,@after:23000:mem.dump:$OUT/s8.mem.txt" -a "$OP" | tail -4 | tee -a "$R"
  [ -f "$OUT/s8.stat.txt" ] && head -6 "$OUT/s8.stat.txt" | sed 's/^/    /' | tee -a "$R"
  [ -f "$OUT/s8.mem.txt" ] && grep -iE 'meta|total' "$OUT/s8.mem.txt" | head -6 | sed 's/^/    /' | tee -a "$R"
  say "=== scenario s9b.meta.off (A/B — 인덱스·워머 끔)"
  NSQL_HOME=$H "$NSQL" config set explorer.search_index off >/dev/null 2>&1
  NSQL_HOME=$H "$NSQL" config set meta.warm_comments off >/dev/null 2>&1
  NSQL_HOME=$H "$NSQL" config set meta.warm_columns_max 0 >/dev/null 2>&1
  bash "$SC/linux-probe.sh" -H "$H" -s 26 -t s9b.meta.off -c "@after:23000:mem.dump:$OUT/s9b.mem.txt" -a "$OP" | tail -4 | tee -a "$R"
  NSQL_HOME=$H "$NSQL" config set explorer.search_index on >/dev/null 2>&1
  NSQL_HOME=$H "$NSQL" config set meta.warm_comments on >/dev/null 2>&1
  say "=== scenario s9.search (탐색기 검색 진행 애니메이션 · 84 §4)"
  bash "$SC/linux-probe.sh" -H "$H" -s 14 -t s9.search -c "@after:5000:explorer.filter:1171" -a "$OP" | tail -4 | tee -a "$R"
  grep -E '^\[frames\] n=' "$H/s9.search.stderr" | tail -2 | sed 's/^/    /' | tee -a "$R"
  say "=== scenario s10.details (객체 상세 패널 · 86)"
  bash "$SC/linux-probe.sh" -H "$H" -s 20 -t s10.details -c "@after:4000:view.object_details,@after:6000:explorer.expand:1,@after:8000:explorer.expand:2,@after:10000:explorer.select:3,@after:12000:explorer.select:4,@after:14000:details.dump:$OUT/s10.det.txt" -a "$OP" | tail -4 | tee -a "$R"
  say "=== scenario s11.sqlprev (SQL Preview 모달 · 83 §4)"
  bash "$SC/linux-probe.sh" -H "$H" -s 20 -t s11.sqlprev -c "@after:6000:explorer.expand:1,@after:8000:explorer.expand:2,@after:10000:explorer.menu:3,@after:12000:explorer.pick:gen:ddl,@after:15000:sqlprev.dump:$OUT/s11.prev.txt" -a "$OP" | tail -4 | tee -a "$R"
  say "=== scenario s12.l3ttl (L3 TTL 회수 · 85 §4)"
  NSQL_HOME=$H "$NSQL" config set meta.detail_ttl_secs 15 >/dev/null 2>&1
  NSQL_HOME=$H "$NSQL" config set meta.cols_ttl_secs 15 >/dev/null 2>&1
  bash "$SC/linux-probe.sh" -H "$H" -s 62 -t s12.l3ttl -c "@after:6000:view.object_details,@after:8000:explorer.expand:1,@after:10000:explorer.expand:2,@after:12000:explorer.select:3,@after:18000:mem.dump:$OUT/s12.m18.txt,@after:58000:mem.dump:$OUT/s12.m58.txt" -a "$OP" | tail -4 | tee -a "$R"
  for f in "$OUT/s12.m18.txt" "$OUT/s12.m58.txt"; do [ -f "$f" ] && say "    $(basename "$f"): $(grep -iE 'MetaDetail|MetaCols' "$f" | tr '\n' ' ')"; done
  NSQL_HOME=$H "$NSQL" config set meta.detail_ttl_secs 300 >/dev/null 2>&1
  NSQL_HOME=$H "$NSQL" config set meta.cols_ttl_secs 300 >/dev/null 2>&1
  unset NSQL_TRACE_FRAMES
fi
say "=== CLI timing (3 runs each)"
cli() { local tag=$1 prof=$2 f=$3 home=${4:-}; for i in 1 2 3; do local t0=$(date +%s%N); local line; line=$( { [ -n "$home" ] && export NSQL_HOME=$home; "$NSQL" run -c "$prof" "$f" --timing 2>&1; } | grep -E '^⏱' | tail -1); say "$tag run$i wall=$(( ($(date +%s%N)-t0)/1000000 ))ms $line"; done; }
cli cli.sqlite.100k "$PROF" "$D/rows100k.sql" "$H"
cli cli.sqlite.200  "$PROF" "$D/rows200.sql" "$H"
if [ -n "${NSQL_PERF_ORACLE:-}" ]; then printf "SELECT LEVEL AS n, RPAD('x', 900, 'y') AS pad FROM dual CONNECT BY LEVEL <= 200;\n" > "$D/q_ora.sql"; printf 'SELECT COUNT(*) FROM user_objects;\n' > "$D/q_ora_cnt.sql"; cli cli.oracle.200 "$NSQL_PERF_ORACLE" "$D/q_ora.sql"; cli cli.oracle.cnt "$NSQL_PERF_ORACLE" "$D/q_ora_cnt.sql"; fi
if [ -n "${NSQL_PERF_MSSQL:-}" ]; then printf 'SELECT TOP 200 * FROM INFORMATION_SCHEMA.COLUMNS;\n' > "$D/q_ms.sql"; cli cli.mssql.200 "$NSQL_PERF_MSSQL" "$D/q_ms.sql"; fi
if [ -n "${NSQL_PERF_PG:-}" ]; then printf 'SELECT * FROM pg_catalog.pg_class LIMIT 200;\n' > "$D/q_pg.sql"; cli cli.pg.200 "$NSQL_PERF_PG" "$D/q_pg.sql"; fi
say "=== done $(date)"
