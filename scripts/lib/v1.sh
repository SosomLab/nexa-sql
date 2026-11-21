# scripts/lib/v1.sh — 헤드리스 V1 시나리오 공용 조각(source 전용 · 사용자 11-21 "형식화된 Flow · Blackbox 입력 → 출력" · D-491).
#   시나리오(`scripts/v1/<이름>.sh`)는 이 조각을 읽고: 격리 홈 만들기 → 기동 명령/CLI 실행 → **덤프 문자열 대조**(expect) → 정리.
#   판정 줄 = `PASS <이름> <검사>` / `FAIL <이름> <검사> :: <근거 한 줄>` · 시나리오 종료 코드 = FAIL 수.
#   규칙: 키·마우스 주입 0 · 포커스 탈취 0(NSQL_NO_ACTIVATE) · 실제 설정 폴더는 읽기만(프로필 사본) · 산출 = $V1_OUT(verify.sh가 준 폴더) ·
#   캡처는 기본으로 남기지 않는다(V1_KEEP_CAPTURE=1이거나 실패일 때만) · 실서버가 필요한 시나리오는 프로필 이름을 env로 받는다(익명화).
# 필요한 env: V1_ROOT(저장소) · V1_OUT(산출 폴더) · V1_EXE(nexa-sql) · V1_NSQL(nsql) — verify.sh가 채운다(단독 실행 때 기본값).
V1_ROOT="${V1_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
V1_OUT="${V1_OUT:-$V1_ROOT/target/v1-run}"
case "$(uname -s 2>/dev/null)" in MINGW*|MSYS*|CYGWIN*) _EXE_SUF=.exe;; *) _EXE_SUF=;; esac
V1_EXE="${V1_EXE:-$V1_ROOT/target/debug/nexa-sql$_EXE_SUF}"
V1_NSQL="${V1_NSQL:-$V1_ROOT/target/debug/nsql$_EXE_SUF}"
V1_NAME="${V1_NAME:-$(basename "${0%.sh}")}"
mkdir -p "$V1_OUT"
_v1_fail=0
v1_m() { cygpath -m "$1" 2>/dev/null || echo "$1"; }
v1_pass() { echo "PASS $V1_NAME $1"; }
v1_failmsg() { echo "FAIL $V1_NAME $1 :: $2"; _v1_fail=$((_v1_fail + 1)); }

# 격리 홈 — <lang en|ko> [copy-profiles 0|1]. 프로필 사본이 든 홈은 v1_done에서 즉시 지운다.
v1_home() {
  V1_HOME=$(mktemp -d "${TEMP:-/tmp}/nsql-v1-XXXX")
  printf 'ui.lang=%s\ndemo.prompted=on\nexplorer.details=off\n' "${1:-en}" > "$V1_HOME/settings.conf"
  if [ "${2:-0}" = 1 ]; then
    local P="${NSQL_REAL_HOME:-${APPDATA:-$HOME/.config}/nexa-sql}"
    cp -R "$P/profiles" "$V1_HOME/" 2>/dev/null; cp "$P/device.key" "$V1_HOME/" 2>/dev/null; cp -R "$P/drivers" "$V1_HOME/" 2>/dev/null
  fi
  export NSQL_HOME="$V1_HOME"
}
v1_set() { printf '%s\n' "$@" >> "$V1_HOME/settings.conf"; }   # 설정 줄 추가(`key=value`)

# GUI 헤드리스 1회 — <태그> <기동 명령 목록(쉼표)> [초 상한 · 기본 25] [인자…]. 끝에 file.exit를 붙여 두는 것은 호출자 몫.
v1_gui() {
  local tag=$1 cmd=$2 lim=${3:-25}; shift 3 2>/dev/null || shift $#
  NSQL_HEADLESS=1 NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="$cmd" timeout -s KILL "$lim" "$V1_EXE" "$@" > "$V1_OUT/$tag.stdout" 2> "$V1_OUT/$tag.stderr"
  echo $? > "$V1_OUT/$tag.exit"
}
v1_cli() { "$V1_NSQL" "$@" </dev/null 2>&1; }

# 대조 — <검사 이름> <파일> <기대 문자열(grep -F)>  /  없어야 하는 것은 expect_not.
expect() { if grep -qF -- "$3" "$2" 2>/dev/null; then v1_pass "$1"; else v1_failmsg "$1" "'$3' 없음 · $(head -c 160 "$2" 2>/dev/null | tr '\n' ' ')"; fi; }
expect_not() { if grep -qF -- "$3" "$2" 2>/dev/null; then v1_failmsg "$1" "'$3' 있음"; else v1_pass "$1"; fi; }
expect_eq() { if [ "$2" = "$3" ]; then v1_pass "$1"; else v1_failmsg "$1" "기대 '$3' · 실제 '$2'"; fi; }

# 실서버 시나리오 문턱 — <env 이름…>: 비어 있으면 SKIP(종료 0 · 판정 불가 표시).
v1_need_env() { local e; for e in "$@"; do if [ -z "${!e:-}" ]; then echo "SKIP $V1_NAME env $e 없음(실서버 프로필 이름은 env로 · 익명화)"; exit 0; fi; done; }

# 끝 — 격리 홈 삭제 · `[stall]` 요약 · 종료 코드 = FAIL 수.
v1_done() {
  [ -n "${V1_HOME:-}" ] && rm -rf "$V1_HOME"
  local s; s=$(cat "$V1_OUT"/*.stderr 2>/dev/null | grep -c "\[stall\]")
  [ "${s:-0}" -gt 0 ] && echo "WARN $V1_NAME [stall] ${s}줄"
  exit $_v1_fail
}
