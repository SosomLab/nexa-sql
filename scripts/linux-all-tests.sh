#!/usr/bin/env bash
# linux-all-tests.sh — 전체 테스트 자동화(사용자 09-22): 두 저장소의 fmt → clippy(-D warnings) → cargo test --workspace → nexa-sql
#   3-OS 검사(check-3os.sh) → 실서버 통합(**저장된 프로필 이름**으로 · 비밀번호는 환경에 안 적음) → CLI 기능 점검. 결과 = <out>/summary.txt + 단계별 로그.
#
# 사용:  scripts/linux-all-tests.sh -o /tmp/nsql-tests [-u ../nexa-ui]
#        실서버: NSQL_ORACLE_PROFILE=BISCM NSQL_MSSQL_PROFILE=M4PLAN NSQL_PG_PROFILE=Repository scripts/linux-all-tests.sh -o …
#        (Oracle 수동 지정 테스트는 NSQL_ORACLE_CLIENT_DIR_TEST — 없으면 $ORACLE_IC_HOME · 둘 다 없으면 건너뜀)
#        IT=0 이면 실서버 단계를 건너뛴다. ⚠ 통합 테스트는 실서버에 임시 객체(`nexa_it` · `nsql_it_*` · `#…` · `pg_temp`)를 만들고 지운다.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; UI="$ROOT/../nexa-ui"; LIC="$ROOT/../nexa-license"; OUT=""
while getopts "o:u:" o; do case $o in o) OUT=$OPTARG;; u) UI=$OPTARG;; esac; done
[ -n "$OUT" ] || { echo "usage: -o <out dir> [-u <nexa-ui dir>]"; exit 2; }
mkdir -p "$OUT"; SUM="$OUT/summary.txt"; : > "$SUM"
export NSQL_NO_ACTIVATE=1
step() { local name=$1 dir=$2; shift 2; local log="$OUT/$name.log" t0=$(date +%s)
  ( cd "$dir" && "$@" ) >"$log" 2>&1; local rc=$? t=$(( $(date +%s) - t0 ))
  local passed failed; passed=$(grep -hoE 'test result: ok\. [0-9]+ passed' "$log" | awk '{s+=$4} END{print s+0}'); failed=$(grep -hoE '[0-9]+ failed' "$log" | awk '{s+=$1} END{print s+0}')
  printf '%-30s rc=%d %5ds  passed=%s failed=%s\n' "$name" "$rc" "$t" "$passed" "$failed" | tee -a "$SUM"; return $rc; }
step ui.fmt       "$UI"   cargo fmt --all -- --check
step ui.clippy    "$UI"   cargo clippy --workspace --all-targets -- -D warnings
step ui.test      "$UI"   cargo test --workspace
# 형제 저장소 둘째 = nexa-license(09-27 · nsql-license가 path 의존 · 검증 전용).
if [ -d "$LIC" ]; then
  step lic.fmt    "$LIC"  cargo fmt --all -- --check
  step lic.clippy "$LIC"  cargo clippy --workspace --all-targets --all-features -- -D warnings
  step lic.test   "$LIC"  cargo test --workspace --all-features
fi
step sql.fmt      "$ROOT" cargo fmt --all -- --check
step sql.clippy   "$ROOT" cargo clippy --workspace --all-targets -- -D warnings
step sql.test     "$ROOT" cargo test --workspace
step sql.check3os "$ROOT" bash scripts/check-3os.sh
if [ "${IT:-1}" = 1 ] && [ -n "${NSQL_ORACLE_PROFILE:-}${NSQL_MSSQL_PROFILE:-}${NSQL_PG_PROFILE:-}" ]; then
  step sql.integration "$ROOT" cargo test -p nsql-drivers --test integration -- --nocapture --test-threads=1
  CD="${NSQL_ORACLE_CLIENT_DIR_TEST:-${ORACLE_IC_HOME:-}}"
  [ -n "${NSQL_ORACLE_PROFILE:-}" ] && [ -n "$CD" ] && NSQL_ORACLE_CLIENT_DIR_TEST="$CD" step sql.oracle_client_manual "$ROOT" cargo test -p nsql-drivers --test oracle_client_manual -- --nocapture --test-threads=1
fi
NSQL="$ROOT/target/release/nsql"; [ -x "$NSQL" ] || NSQL="$ROOT/target/debug/nsql"
step cli.plan.oracle "$ROOT" "$NSQL" plan -d oracle examples/golden-session-vars.sql
step cli.plan.mssql  "$ROOT" "$NSQL" plan -d mssql  examples/golden-session-vars.sql
step cli.run.sqlite  "$ROOT" "$NSQL" run -c sqlite::memory: -d sqlite examples/variables/sqlite.sql
step cli.run.demo    "$ROOT" "$NSQL" run -c sqlite::memory: -d sqlite examples/demo.sql
step cli.config.list "$ROOT" "$NSQL" config list all
NSQL_HOME="$OUT/lic-home" step cli.license.status "$ROOT" "$NSQL" license status
echo "--- done $(date) ---" | tee -a "$SUM"
