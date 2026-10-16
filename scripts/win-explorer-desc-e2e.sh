#!/usr/bin/env bash
# win-explorer-desc-e2e.sh — 객체 탐색기 **"Include Description" 설명 포함 검색** E2E(사용자 10-15 · 127차 · docs/28 §7-1).
#   격리 홈에 실제 프로필을 복사해 GUI를 기동 명령으로만 몬다(키 주입 0 · docs/61 §4). 실서버에 임시 표 `nsqlt_desc_probe`를 만들고
#   설명(코멘트)에만 있는 낱말(`giraffe`)로 거른다(61 §2-4 ⑤ 임시 객체 · 끝에 DROP).
#   ① 토글 끔(기본) = 설명 낱말로는 일치 0  ② 설정 `explorer.search_desc=on` 홈 = 기동부터 설명 포함 → 표가 일치
#   ③ 검색 중 토글 켜기(`explorer.desc:on`) = 다시 판정되어 일치  ④ 접속 뒤 토글 켜고 검색(명령 경로)  ⑤ 이름 낱말(`nsqlt_desc`)은 토글과 무관하게 일치
#   ⑥ 한글 설명 낱말(`공급계획` · 설정 켬) = 일치(사용자 10-15 SQL Server 재현 · 기동 명령 환경 변수의 UTF-8 한글)
#   ⑦ (mssql + DB) `USE` 없이 = 현재 DB가 기준 DB(master)여도 **전 DB 검색**(T-351 · 기본 범위 = 전체)이 다른 DB의 표를 찾음
#   ⑧ (mssql + DB) 같은 조건 + 설정 `explorer.search_scope=current` = 현재 DB만 → 안 보임
#   `meta.warm_comments=off`로 두어 코멘트 읽기가 **토글 때문에만** 일어났음을 보인다 · `explorer.filter_expand=on`으로 일치 객체 행이 트리에 보이게(기본 off = 개수만).
#   판정 분리: 덤프마다 "접속 완료"(루트 행 Loaded · Not connected 아님)를 먼저 — 필터 중 일치 0이면 루트 한 줄만 남는 것이 정상 모양이라
#   스키마 행 유무로는 접속을 판정할 수 없다(협업 10-15). 진단 = `explorer.stat`(hits= · busy=) + `NSQL_TRACE_META` stderr(코멘트 요청·응답).
#   타이밍: 첫 명령은 `@connected:`(접속 완료 기준) · 뒤 명령은 기동 기준 `@after:`라 접속이 8 s를 넘으면(동시 빌드 등 부하) 순서가 어긋날 수 있다.
#
# 사용: scripts/win-explorer-desc-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-n target/debug/nsql.exe] [-p Repository:postgres] [-P <실제 설정 폴더>]
#   `-p 프로필:방언[:DB]` — postgres(기본) · oracle · mssql(현재 스키마 `dbo` 가정 · `sp_addextendedproperty` · 셋째 조각 = 임시 표를 만들 DB =
#   CLI 실행 파일 머리에 `USE [DB]; GO` · 탐색기 현재 DB와 같아야 한다 · SSMS 골격은 스키마 노드가 없어 ② 스키마 행 검사는 생략).
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"   # macOS = GNU timeout 없음 → 대역(130차 mac)
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; CLI="$ROOT/target/debug/nsql.exe"; ENTRY="Repository:postgres"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
while getopts "o:g:n:p:P:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; n) CLI=$OPTARG;; p) ENTRY=$OPTARG;; P) PROF=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui-exe] [-n cli-exe] [-p profile:dialect] [-P real-config-dir]"; exit 2; }
[ -x "$EXE" ] || EXE="${EXE%.exe}"; [ -x "$CLI" ] || CLI="${CLI%.exe}"
P="${ENTRY%%:*}"; rest="${ENTRY#*:}"; DIA="${rest%%:*}"; DB=""; case "$rest" in *:*) DB="${rest#*:}";; esac
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
BASE='ui.lang=en
demo.prompted=on
explorer.details=off
explorer.filter_expand=on
window.main_size=1400x900
meta.warm_comments=off
meta.disk_cache=off
'
printf '%s' "$BASE" > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/explorer-desc-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk()    { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
absent() { if echo "$3" | grep -qE -- "$2"; then bad "$1 (있으면 안 됨 $2)" "$3"; else ok "$1"; fi; }
cli() { "$CLI" run -c "$P" --no-prompt "$1" > "$OUT/cli_$2.out" 2>&1; local rc=$?; cat "$OUT/cli_$2.out"; return $rc; }

T="nsqlt_desc_probe"; WORD="giraffe"; KO="공급계획"; DESC="zebra $WORD $KO lookup table"   # 한글 낱말 = 사용자 10-15 SQL Server 재현(⑥)
case "$DIA" in
  postgres) CREATE="DROP TABLE IF EXISTS $T; CREATE TABLE $T (id int); COMMENT ON TABLE $T IS '$DESC';"; DROP="DROP TABLE IF EXISTS $T;";;
  oracle)   CREATE="BEGIN EXECUTE IMMEDIATE 'DROP TABLE $T'; EXCEPTION WHEN OTHERS THEN NULL; END;
/
CREATE TABLE $T (id NUMBER);
COMMENT ON TABLE $T IS '$DESC';"; DROP="DROP TABLE $T;";;
  mssql)    CREATE="IF OBJECT_ID(N'$T', N'U') IS NOT NULL DROP TABLE $T;
CREATE TABLE $T (id int);
EXEC sp_addextendedproperty @name = N'MS_Description', @value = N'$DESC', @level0type = N'SCHEMA', @level0name = N'dbo', @level1type = N'TABLE', @level1name = N'$T';"; DROP="DROP TABLE $T;";;
  *) echo "unknown dialect $DIA"; exit 2;;
esac
say "=== 설명 포함 검색 E2E · 프로필 $P($DIA) · 표 $T · 설명 '$DESC' · $(date '+%F %T')"
{ [ -n "$DB" ] && printf 'USE [%s];\nGO\n' "$DB"; printf '%s\n' "$CREATE"; } > "$OUT/create.sql"
{ [ -n "$DB" ] && printf 'USE [%s];\nGO\n' "$DB"; printf '%s\n' "$DROP"; } > "$OUT/drop.sql"
if ! cli "$OUT/create.sql" create >/dev/null; then bad "임시 표 생성(CLI)" "$(cat "$OUT/cli_create.out")"; say "RESULT pass=$pass fail=$fail"; exit 1; fi
ok "임시 표 생성 + COMMENT(CLI)"

# GUI 한 번: <기동 명령> <초> <꼬리표> — 덤프 = $OUT/tree_<꼬리표>.txt · 진단 = stat_<꼬리표>.txt · 서브셸 아님(집계가 산다 · 102 §12).
#   기동 명령 끝에 `explorer.stat`을 덧붙이고(덤프 +500 ms) stderr에 메타 스레드 추적을 남긴다.
run_gui() {
  local cmd=$1 secs=$2 tag=$3
  local dump="$OUT/tree_$tag.txt" stat="$OUT/stat_$tag.txt"
  rm -f "$dump" "$stat"
  local last; last=$(echo "$cmd" | grep -oE '@after:[0-9]+' | tail -1 | tr -dc '0-9'); last=$((${last:-15000} + 500))
  NSQL_NO_ACTIVATE=1 NSQL_TRACE_META=1 NSQL_STARTUP_CMD="$cmd,@after:$last:explorer.stat:$stat" \
    timeout -s KILL "$secs" "$EXE" "$P" > "$OUT/gui_$tag.stdout" 2> "$OUT/gui_$tag.stderr"
  if grep -q 'panicked' "$OUT/gui_$tag.stderr"; then bad "$tag: 패닉" "$(grep -A3 panicked "$OUT/gui_$tag.stderr")"; fi
  if [ -s "$dump" ]; then
    if grep -qE '^0\|root\|' "$dump" && ! grep -q 'Not connected' "$dump" && grep -qE '^0\|root\|[^|]*\|[^|]*\|Loaded' "$dump"; then
      ok "$tag: 접속 완료(루트 행 Loaded)"
    else
      bad "$tag: 접속 전/실패(루트 행)" "$(head -2 "$dump")"
    fi
  else
    bad "$tag: 트리 덤프 없음(기동·접속 실패?)" "$(tail -3 "$OUT/gui_$tag.stderr")"
  fi
  { echo "--- $tag stat: $(cat "$stat" 2>/dev/null)"; echo "--- $tag meta trace(comment): $(grep -ci 'comment' "$OUT/gui_$tag.stderr" 2>/dev/null) lines"; } | tee -a "$REPORT" >/dev/null
}
show() { sed 's/^/        > /' "$1" | head -14 | tee -a "$REPORT" >/dev/null; }
ROW="\|object\|([A-Za-z_]+\.)?$T\|"   # SSMS 골격 = `dbo.이름`
# mssql + DB: GUI도 접속 뒤 `USE [DB]`(편집기 open + run.all)로 현재 DB를 맞춘 뒤 검색(프로필 기준 DB가 master인 경우 · 10-15) → 뒤 명령은 +6 s.
PRE=""; T0=0
if [ "$DIA" = mssql ] && [ -n "$DB" ]; then
  printf 'USE [%s]\nGO\n' "$DB" > "$OUT/use.sql"; uw=$(cygpath -w "$OUT/use.sql" 2>/dev/null || echo "$OUT/use.sql")
  PRE="@connected:open:$uw,@after:3000:run.all,"; T0=6000
fi
# 기동 명령 조립: <첫 명령(접속 뒤)> <둘째 ms:명령>… — DB가 있으면 첫 명령도 @after:(T0)로.
cmd() { # $1 = 첫 명령 · 나머지 = "ms:명령"(기동 기준 · T0을 더한다)
  local first=$1; shift; local out
  if [ -n "$PRE" ]; then out="${PRE}@after:$T0:$first"; else out="@connected:$first"; fi
  local it; for it in "$@"; do out="$out,@after:$(( ${it%%:*} + T0 )):${it#*:}"; done
  echo "$out"
}

# ① 토글 끔(기본) — 설명 낱말 = 일치 0(표가 트리에 없다 · 루트만 남는 것이 정상 모양).
run_gui "$(cmd "explorer.filter:$WORD" "16000:explorer.dump:$OUT/tree_off.txt")" $((20 + T0/1000)) off; t1=$(cat "$OUT/tree_off.txt" 2>/dev/null); show "$OUT/tree_off.txt"
absent "① 토글 끔 = 설명 낱말($WORD)로는 표가 안 보임" "$ROW" "$t1"
# ② 설정에 켜 둔 홈 = 기동부터 설명 포함(토글과 설정은 한 값).
printf '%sexplorer.search_desc=on\n' "$BASE" > "$H/settings.conf"
run_gui "$(cmd "explorer.filter:$WORD" "18000:explorer.dump:$OUT/tree_preset.txt")" $((22 + T0/1000)) preset; t2=$(cat "$OUT/tree_preset.txt" 2>/dev/null); show "$OUT/tree_preset.txt"
chk "② 설정 explorer.search_desc=on = 설명 낱말($WORD)로 표 $T 일치" "$ROW" "$t2"
[ "$DIA" = mssql ] || chk "② 스키마 행 = 일치 N" "\|schema\|[^|]*\|[^|]*[0-9]+ match" "$t2"
printf '%s' "$BASE" > "$H/settings.conf"
# ③ 검색 중 토글 켜기 = 같은 글을 다시 판정(인덱스는 그대로).
run_gui "$(cmd "explorer.filter:$WORD" "10000:explorer.desc:on" "20000:explorer.dump:$OUT/tree_mid.txt")" $((24 + T0/1000)) mid; t3=$(cat "$OUT/tree_mid.txt" 2>/dev/null); show "$OUT/tree_mid.txt"
chk "③ 검색 중 토글 켬 = 표 $T 일치" "$ROW" "$t3"
# ④ 접속 뒤 토글 켜고 검색(명령 경로 · 토글 → 글).
run_gui "$(cmd "explorer.desc:on" "8000:explorer.filter:$WORD" "20000:explorer.dump:$OUT/tree_on.txt")" $((24 + T0/1000)) on; t4=$(cat "$OUT/tree_on.txt" 2>/dev/null); show "$OUT/tree_on.txt"
chk "④ 토글 켠 뒤 검색 = 표 $T 일치" "$ROW" "$t4"
# ⑤ 이름 낱말은 토글과 무관(회귀).
run_gui "$(cmd "explorer.filter:nsqlt_desc" "16000:explorer.dump:$OUT/tree_name.txt")" $((20 + T0/1000)) name; t5=$(cat "$OUT/tree_name.txt" 2>/dev/null); show "$OUT/tree_name.txt"
chk "⑤ 이름 낱말(nsqlt_desc) = 표 일치(회귀)" "$ROW" "$t5"
# ⑥ 한글 설명 낱말(설정 켬).
printf '%sexplorer.search_desc=on\n' "$BASE" > "$H/settings.conf"
run_gui "$(cmd "explorer.filter:$KO" "18000:explorer.dump:$OUT/tree_ko.txt")" $((22 + T0/1000)) ko; t6=$(cat "$OUT/tree_ko.txt" 2>/dev/null); show "$OUT/tree_ko.txt"
chk "⑥ 한글 설명 낱말($KO) = 표 $T 일치" "$ROW" "$t6"
printf '%s' "$BASE" > "$H/settings.conf"

# ⑦⑧ 전 DB 검색(mssql + DB · USE 없이 · 기준 DB에서 검색).
if [ "$DIA" = mssql ] && [ -n "$DB" ]; then
  PRE_SAVE=$PRE; T0_SAVE=$T0; PRE=""; T0=0
  printf '%sexplorer.search_desc=on\n' "$BASE" > "$H/settings.conf"
  run_gui "$(cmd "explorer.filter:$WORD" "20000:explorer.dump:$OUT/tree_alldb.txt")" 24 alldb; t7=$(cat "$OUT/tree_alldb.txt" 2>/dev/null); show "$OUT/tree_alldb.txt"
  chk "⑦ USE 없이 전 DB 검색(기본 범위) = 다른 DB의 표 $T 일치" "$ROW" "$t7"
  chk "⑦ 일치가 그 DB 노드 아래" "\\|database\\|$DB\\|" "$t7"
  printf '%sexplorer.search_desc=on\nexplorer.search_scope=current\n' "$BASE" > "$H/settings.conf"
  run_gui "$(cmd "explorer.filter:$WORD" "16000:explorer.dump:$OUT/tree_curdb.txt")" 20 curdb; t8=$(cat "$OUT/tree_curdb.txt" 2>/dev/null); show "$OUT/tree_curdb.txt"
  absent "⑧ 범위 = 현재 DB(master) → 다른 DB의 표는 안 보임" "$ROW" "$t8"
  printf '%s' "$BASE" > "$H/settings.conf"
  PRE=$PRE_SAVE; T0=$T0_SAVE
fi
cli "$OUT/drop.sql" drop >/dev/null && ok "임시 표 DROP(CLI)" || bad "임시 표 DROP(CLI)" "$(cat "$OUT/cli_drop.out")"
say "RESULT pass=$pass fail=$fail"
[ "$fail" -eq 0 ]
