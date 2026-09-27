#!/usr/bin/env bash
# linux-vault-e2e.sh — 세션 자격 금고 E2E(사용자 09-27 실기 결함 재현·회귀): 실제 홈의 **복사본**(원본 무변경)으로 격리 인스턴스를 띄우고
#   기동 명령만으로(키 주입 0) ① CONNECT user@host(묻기 → `pw.answer_from_profile`로 저장 프로필 비밀번호 답) ② DISCONNECT/전용 세션 전환
#   ③ 같은 서버 CONNECT user@host → `[vault] recall(hit)`이어야 한다(종전 = 탐색기 칸 자동 제거가 금고를 지워 miss).
# 사용: scripts/linux-vault-e2e.sh -P <실제 홈(~/.config/nexa-sql)> -o <출력 폴더> [-e target/debug/nexa-sql]
#       프로필 M4PLAN(mssql · 비밀번호 저장)과 oracle://BISCM:BISCM@192.168.0.58:1521/BISCM(실서버 · 읽기)를 쓴다 — 필요하면 -m/-r 로 바꾼다.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/debug/nexa-sql"; HOME_SRC=""; OUT=""; MSSQL="mssql://BISCM_MS@192.168.0.58:1433/M4PLAN_MS"; MSPROF="M4PLAN"; ORA="oracle://BISCM:BISCM@192.168.0.58:1521/BISCM"
while getopts "P:o:e:m:p:r:" o; do case $o in P) HOME_SRC=$OPTARG;; o) OUT=$OPTARG;; e) APP=$OPTARG;; m) MSSQL=$OPTARG;; p) MSPROF=$OPTARG;; r) ORA=$OPTARG;; esac; done
[ -n "$HOME_SRC" ] && [ -n "$OUT" ] || { echo "usage: -P <real home> -o <out dir> [-e exe] [-m mssql-url] [-p mssql-profile] [-r oracle-url]"; exit 2; }
mkdir -p "$OUT"; H="$OUT/home"; rm -rf "${OUT:?}/home"; cp -r "$HOME_SRC" "$H"; rm -f "${H:?}/instance.lock"
export LD_LIBRARY_PATH="${ORACLE_IC_HOME:-$(ls -d "$HOME"/oracle/instantclient_* 2>/dev/null | sort -V | tail -1)}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
pass=0; fail=0
run_case() { local id=$1 title=$2 sql=$3
  printf '%b' "$sql" > "$OUT/$id.sql"
  NSQL_HOME="$H" NSQL_TRACE_VAULT=1 NSQL_NO_ACTIVATE=1 NSQL_STARTUP_CMD="open:$OUT/$id.sql,@after:4000:run.all,@after:9000:pw.answer_from_profile:$MSPROF" "$APP" >/dev/null 2>"$OUT/$id.err" &
  local p=$!; sleep 45; kill "$p" 2>/dev/null; wait "$p" 2>/dev/null
  local hits; hits=$(grep -c "recall(hit)" "$OUT/$id.err")
  local asks; asks=$(grep -c "recall(miss)" "$OUT/$id.err")
  # 기대 = 첫 CONNECT만 묻고(miss 1) 마지막 CONNECT는 적중(hit ≥ 1) · 지워짐(removed) 0.
  local removed; removed=$(grep -c "\[vault:mem\] removed" "$OUT/$id.err")
  local verdict=ok; [ "$hits" -ge 1 ] && [ "$asks" -le 1 ] && [ "$removed" = 0 ] || verdict="FAIL"
  echo "| $id | $title | $verdict | hit=$hits miss=$asks removed=$removed |"
  [ "$verdict" = ok ] && pass=$((pass+1)) || fail=$((fail+1)); }
echo "| id | 시나리오 | 판정 | 비고 |"; echo "|---|---|---|---|"
run_case V1 "묻기 → 오라클 전용 세션 → 같은 서버 재접속"            "CONNECT $MSSQL\nSELECT 1 AS a;\nCONNECT $ORA\nSELECT 1 FROM DUAL;\nCONNECT $MSSQL\nSELECT 2 AS b;\n"
run_case V2 "묻기 → DISCONNECT → 오라클 → DISCONNECT → 재접속"    "CONNECT $MSSQL\nSELECT 1 AS a;\nDISCONNECT\nCONNECT $ORA\nSELECT 1 FROM DUAL;\nDISCONNECT\nCONNECT $MSSQL\nSELECT 2 AS b;\n"
echo "== 합계: 통과 $pass · 실패 $fail"
rm -rf "${OUT:?}/home"
[ "$fail" = 0 ]
