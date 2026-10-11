#!/usr/bin/env bash
# win-output-result-e2e.sh — Output 탭과 결과 탭 전환 E2E(10-01 ㉘ · docs/100 §3).
#   격리 홈에 실제 프로필을 복사해 GUI를 기동 명령으로만 몬다(키 주입 0 · docs/61 §4).
#   파일 = `SHOW CONN` + `SELECT … FROM dual` 한 벌.
#   ① 첫 F5: SHOW CONN 메시지로 Output 탭이 켜져도(`output.activate = no_results`) 결과 셋이 오면 **결과 탭으로 돌아온다** · 행 수 1
#   ② 둘째 F5(Output이 활성인 채 실행): 결과는 Output 자리표시가 아니라 **가장 최근 결과 탭**으로 가고 그 탭이 활성(㉘-b "2행 가져옴인데 결과 탭이 비어 있다")
#
# 사용: scripts/win-output-result-e2e.sh -o <출력폴더> [-g target/debug/nexa-sql.exe] [-p BISCM] [-P <실제 설정 폴더>]
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""; EXE="$ROOT/target/debug/nexa-sql.exe"; PROFILE="BISCM"; PROF="${APPDATA:-$HOME/.config}/nexa-sql"
while getopts "o:g:p:P:" o; do case $o in o) OUT=$OPTARG;; g) EXE=$OPTARG;; p) PROFILE=$OPTARG;; P) PROF=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-g gui-exe] [-p profile] [-P real-config-dir]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "$H"; mkdir -p "$H"
[ -d "$PROF/profiles" ] && cp -R "$PROF/profiles" "$H/" && cp "$PROF/device.key" "$H/" 2>/dev/null
printf 'ui.lang=ko\ndemo.prompted=on\nexplorer.details=off\nwindow.main_size=1400x900\n' > "$H/settings.conf"
export NSQL_HOME="$H"
REPORT="$OUT/output-result-e2e.txt"; : > "$REPORT"
say() { echo "$*"; echo "$*" >> "$REPORT"; }
pass=0; fail=0
ok()  { say "  PASS  $1"; pass=$((pass+1)); }
bad() { say "  FAIL  $1"; fail=$((fail+1)); [ -n "${2:-}" ] && echo "$2" | head -12 | sed 's/^/        /' | tee -a "$REPORT" >/dev/null; }
chk() { if echo "$3" | grep -qE -- "$2"; then ok "$1"; else bad "$1 (기대 $2)" "$3"; fi; }
say "=== Output ↔ 결과 탭 전환 E2E · 프로필 $PROFILE · $(date '+%F %T')"
F="$OUT/t.sql"; printf 'SHOW CONN\nSELECT 1 AS x FROM dual;\n' > "$F"; fw=$(cygpath -w "$F" 2>/dev/null || echo "$F")
R1="$OUT/res1.txt"; R2="$OUT/res2.txt"; rm -f "$R1" "$R2"
NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="@connected:open:$fw,@after:5000:run.all,@after:9000:result.dump:$R1,@after:9500:run.all,@after:13500:result.dump:$R2" \
  timeout -s KILL 16 "$EXE" "$PROFILE" > "$OUT/gui.stdout" 2> "$OUT/gui.stderr"
if [ -s "$R1" ] && [ -s "$R2" ]; then
  r1=$(cat "$R1"); r2=$(cat "$R2"); say "        1> $(head -1 "$R1")"; say "        2> $(head -1 "$R2")"
  chk "① 첫 F5: Output이 아니라 결과 탭이 활성" "^active=[^|]*\|output=false\|" "$r1"
  chk "① 활성 그리드 행 수 = 1" "\|rows=1$" "$r1"
  chk "① Output 탭은 만들어져 있다(메시지 있음)" "\|true$" "$r1"
  chk "② 둘째 F5(Output 활성 상태에서 실행): 결과 탭이 활성 · 행 1" "^active=[^|]*\|output=false\|rows=1" "$r2"
  n=$(echo "$r2" | tail -n +2 | grep -c "|false$"); [ "$n" -eq 1 ] && ok "② 결과 탭은 하나뿐(새 탭을 만들지 않고 최근 결과 탭 재사용)" || bad "② 결과 탭 수 $n" "$r2"
else bad "덤프 없음(1 $([ -s "$R1" ] && echo o || echo x) · 2 $([ -s "$R2" ] && echo o || echo x))" "$(tail -3 "$OUT/gui.stderr")"; fi
say ""; say "== 합계: 통과 $pass · 실패 $fail  ($(date '+%F %T'))"
exit $fail
