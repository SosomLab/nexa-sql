#!/usr/bin/env bash
# intel-alias-e2e.sh — ★ 자동완성 확정 때 자동 별칭(133차 Linux · 10-21 · 사용자 "마지막 테이블에 자동완성을 하면 Alias가 붙지 않아").
#   한정 이름 관계 자리도 별칭: Oracle `FROM STG.IF_PP0008` → `STG.IF_PP0008 I` · SQL Server `FROM DB.dbo.T` → `DB.dbo.T T`(약어 = `intel.alias_style`
#   기본 abbr · 대문자). 대조군 = 별칭 불허 문장(Oracle `ALTER TABLE 스키마.표` · SQL Server `UPDATE DB.dbo.표`)에는 안 붙음.
#   길 = 격리 홈 · 실서버 읽기 접속 · `editor.caret:end` → `intel.probe` 두 번(채움 뒤) → `ui.key:enter`(앱 안 route · 키 주입 0) → `editor.dump`.
# 사용: scripts/intel-alias-e2e.sh -o <출력폴더> -P <실제 NSQL_HOME> [-d 프로필:방언:한정자:표,…] [-g gui] [-n nsql]
#   기본 -d = APPDB:oracle:APPDB_SB:ANTI_KR_ITEM,PLANDB:mssql:APPDB_MS.dbo:M4E_C300000 (한정자 = `스키마` 또는 `DB.스키마` · 사용자 예 STG.IF_PP0008은 APPDB 계정 권한 밖이라 읽을 수 있는 표로)
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/lib/timeout.sh"
. "$ROOT/scripts/lib/xvfb.sh"   # Linux = Xvfb 가상 디스플레이로 재실행(T-458 · D-435 · NSQL_XVFB=0이면 실제 화면)
# ★ T-452(10-29 · 익명화 뒤): 실서버 목록은 -d 또는 env NSQL_E2E_INTEL_LIST(기본값은 익명 = 실서버에선 해석 실패).
OUT=""; PROF=""; LIST="${NSQL_E2E_INTEL_LIST:-APPDB:oracle:APPDB_SB:ANTI_KR_ITEM,PLANDB:mssql:APPDB_MS.dbo:M4E_C300000}"
EXE="$ROOT/target/debug/nexa-sql.exe"; NSQL="$ROOT/target/debug/nsql.exe"
while getopts "o:P:d:g:n:" o; do case $o in o) OUT=$OPTARG;; P) PROF=$OPTARG;; d) LIST=$OPTARG;; g) EXE=$OPTARG;; n) NSQL=$OPTARG;; esac; done
[ -n "$OUT" ] && [ -n "$PROF" ] || { echo "usage: -o <out dir> -P <real NSQL_HOME> [-d 프로필:방언:한정자:표,…] [-g gui] [-n nsql]"; exit 2; }
[ -x "$EXE" ] || EXE="${EXE%.exe}"
mkdir -p "$OUT"
REPORT="$OUT/intel-alias-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -6 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
w() { cygpath -w "$1" 2>/dev/null || echo "$1"; }

H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/"; cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=en\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1200,800\nintel.insert_alias=on\n' > "$H/settings.conf"

# run_case <이름> <프로필> <본문> <기대 정규식(마지막 줄 전체)>
run_case() {
  local name=$1 conn=$2 body=$3 want=$4
  local sql="$OUT/$name.sql" dump="$OUT/$name.dump" cands="$OUT/$name.cands"
  printf '%s' "$body" > "$sql"; rm -f "$dump" "$cands"
  # 접속 → 열기 → 캐럿 끝 → 완성(채움 요청) → 4 s 뒤 다시 완성(채워진 뒤) → 후보 덤프 → ↓(팝업은 선택 없이 열림 · `intel.key_passthrough`
  #   = 선택 없는 Enter는 편집기 줄바꿈 · 협업 V1 10-21) → Enter(첫 후보 = 정확 일치) → 본문 덤프.
  local cmd="@connected:open:$(w "$sql"),@after:6000:editor.caret:end,@after:6300:intel.probe,@after:10500:intel.probe,@after:11300:intel.dump:$(w "$cands"),@after:11500:ui.key:down,@after:11700:ui.key:enter,@after:12200:editor.dump:$(w "$dump")"
  NSQL_HOME="$H" NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL 16 "$EXE" "$conn" > "$OUT/$name.stdout" 2> "$OUT/$name.stderr"
  local text; text=$(sed -n '2,$p' "$dump" 2>/dev/null | sed -e 's/[[:space:]]*$//' | tail -1)
  if [ -z "$text" ]; then bad "$name 본문 덤프 없음(접속 실패?)" "$(tail -3 "$OUT/$name.stderr" 2>/dev/null)"; return; fi
  if printf '%s' "$text" | grep -qE "$want"; then ok "$name → \`$text\`"; else bad "$name → \`$text\` (기대 /$want/)" "$(head -4 "$cands" 2>/dev/null)"; fi
}

say "=== 자동 별칭 E2E $(date '+%F %T') · $LIST"
IFS=',' read -ra ITEMS <<< "$LIST"
for it in "${ITEMS[@]}"; do
  IFS=':' read -r conn dia qual tbl <<< "$it"
  tag="$conn-$dia"
  # ① 관계 자리의 한정 이름 → `한정자.표 별칭`(별칭 = 영숫자·밑줄 · 표 이름과 다름).
  run_case "$tag-1-from" "$conn" "SELECT * FROM $qual.$tbl" "^SELECT \* FROM $qual\.$tbl [A-Za-z][A-Za-z0-9_]*$"
  # ② 콤마 뒤 둘째 테이블도.
  run_case "$tag-2-list" "$conn" "SELECT * FROM $qual.$tbl A, $qual.$tbl" "^SELECT \* FROM $qual\.$tbl A, $qual\.$tbl [A-Za-z][A-Za-z0-9_]*$"
  # ③ 대조군 = 별칭 불허 문장은 그대로.
  case $dia in
    mssql)  run_case "$tag-3-noalias" "$conn" "UPDATE $qual.$tbl" "^UPDATE $qual\.$tbl$";;
    *)      run_case "$tag-3-noalias" "$conn" "ALTER TABLE $qual.$tbl" "^ALTER TABLE $qual\.$tbl$";;
  esac
done
say "RESULT PASS $pass FAIL $fail"
[ "$fail" -eq 0 ]
