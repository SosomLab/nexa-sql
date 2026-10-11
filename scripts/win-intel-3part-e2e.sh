#!/usr/bin/env bash
# win-intel-3part-e2e.sh — SQL Server 3부 이름 자동완성 E2E(10-01 ⑭ · docs/82 §7 · journal §21).
#   접속 DB(로그인 기본 = master)와 다른 DB `BISCM_MS`를 접두로: `BISCM_MS.` = 그 DB의 스키마 · `BISCM_MS.dbo.` = 그 DB·스키마의 객체 ·
#   `t.`(FROM BISCM_MS.dbo.T t) = 그 테이블의 컬럼. 기동 명령 `editor.caret` · `intel.probe`(두 번 = 채움 뒤) · `intel.dump` · 키 주입 0.
# 사용: scripts/win-intel-3part-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-p M4PLAN] [-d BISCM_MS] [-t M4E_C300000] [-P <실제 설정 폴더>]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; PROFILE="M4PLAN"; DB="BISCM_MS"; TBL="M4E_C300000"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
while getopts "o:g:p:d:t:P:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; p) PROFILE=$OPTARG;; d) DB=$OPTARG;; t) TBL=$OPTARG;; P) PROF=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui] [-p profile] [-d other-db] [-t table] [-P real-config-dir]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/intel-3part-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }

# probe <이름> <본문 파일 내용> <캐럿> → 후보 덤프 경로를 echo.
probe() {
  local name=$1 body=$2 caret=$3 f="$OUT/$1.sql" d="$OUT/$1.cands"
  printf '%s' "$body" > "$f"; rm -f "$d"; local fw; fw=$(cygpath -w "$f" 2>/dev/null || echo "$f")
  # 접속 → 열기 → 캐럿 → 완성(채움 요청) → 4초 뒤 다시 완성(채워진 뒤) → 덤프.
  NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw,@after:6000:editor.caret:$caret,@after:6300:intel.probe,@after:10500:intel.probe,@after:11500:intel.dump:$d" \
    timeout -s KILL 15 "$EXE" "$PROFILE" > "$OUT/$1.stdout" 2> "$OUT/$1.stderr"
  echo "$d"
}
say "=== SQL Server 3부 이름 완성 E2E · 프로필 $PROFILE · 다른 DB $DB · 표 $TBL · $(date '+%F %T')"
d=$(probe "p1_schemas" "SELECT * FROM $DB." end)
if [ -s "$d" ]; then
  c=$(cat "$d"); echo "$c" | head -6 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  echo "$c" | grep -qE "^Schema\|dbo\|" && ok "① \`$DB.\` = 그 DB의 스키마(dbo)" || bad "① \`$DB.\` 스키마 없음" "$c"
  echo "$c" | grep -qE "^Keyword\|" && bad "① 키워드가 섞임" "$c" || ok "① 키워드 없음(종류 자리)"
else bad "① 후보 덤프 없음" "$(tail -3 "$OUT/p1_schemas.stderr")"; fi
d=$(probe "p2_objects" "SELECT * FROM $DB.dbo." end)
if [ -s "$d" ]; then
  c=$(cat "$d"); echo "$c" | head -6 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  echo "$c" | grep -qE "^(Table|View)\|$TBL\|" && ok "② \`$DB.dbo.\` = 그 DB·스키마의 테이블($TBL)" || bad "② \`$DB.dbo.\` 테이블 없음" "$c"
  n=$(echo "$c" | grep -cE "^(Table|View)\|"); [ "$n" -ge 10 ] && ok "② 관계 객체 $n개" || bad "② 관계 객체 $n개(<10)" "$c"
else bad "② 후보 덤프 없음" "$(tail -3 "$OUT/p2_objects.stderr")"; fi
body=$'SELECT t.\nFROM '"$DB"'.dbo.'"$TBL"' t'
d=$(probe "p3_columns" "$body" 9)
if [ -s "$d" ]; then
  c=$(cat "$d"); echo "$c" | head -6 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  n=$(echo "$c" | grep -cE "^Column\|"); [ "$n" -ge 1 ] && ok "③ \`t.\`(FROM $DB.dbo.$TBL t) = 컬럼 $n개" || bad "③ 컬럼 없음" "$c"
else bad "③ 후보 덤프 없음" "$(tail -3 "$OUT/p3_columns.stderr")"; fi
# ④ DB 전환 뒤에도 데이터베이스 목록이 남아 있는가(⑰ · 툴바 메뉴 = 같은 버킷): `USE BISCM_MS` 실행 뒤 `USE |` 완성에 DB 목록.
f4="$OUT/p4_use.sql"; printf 'USE %s
GO
' "$DB" > "$f4"; f5="$OUT/p4_after.sql"; printf 'USE ' > "$f5"; d4="$OUT/p4.cands"; rm -f "$d4"
fw4=$(cygpath -w "$f4" 2>/dev/null || echo "$f4"); fw5=$(cygpath -w "$f5" 2>/dev/null || echo "$f5")
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw4,@after:6000:run.all,@after:9500:open:$fw5,@after:10500:editor.caret:end,@after:10800:intel.probe,@after:12500:intel.probe,@after:13500:intel.dump:$d4"   timeout -s KILL 17 "$EXE" "$PROFILE" > "$OUT/p4.stdout" 2> "$OUT/p4.stderr"
if [ -s "$d4" ]; then
  c=$(cat "$d4"); echo "$c" | head -4 | sed 's/^/        > /' | tee -a "$REPORT" >/dev/null
  n=$(echo "$c" | grep -cE "^Database\|"); [ "$n" -ge 2 ] && ok "④ USE $DB 뒤에도 \`USE |\` = 데이터베이스 $n개(목록 버킷 보존)" || bad "④ 전환 뒤 DB 목록 $n개" "$c"
  echo "$c" | grep -qE "^Database\|$DB\|" && ok "④ 목록에 $DB" || bad "④ 목록에 $DB 없음" "$c"
else bad "④ 후보 덤프 없음" "$(tail -3 "$OUT/p4.stderr")"; fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
