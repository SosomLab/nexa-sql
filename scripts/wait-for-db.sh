#!/usr/bin/env bash
# DB 컨테이너가 접속을 받을 때까지 대기(최대 5분). 환경변수 NSQL_*_URL이 있는 것만.
set -uo pipefail
wait_port() { # host port label
  for i in $(seq 1 60); do
    if (echo > /dev/tcp/"$1"/"$2") >/dev/null 2>&1; then echo "$3 ready ($1:$2)"; return 0; fi
    sleep 5
  done
  echo "$3 not ready ($1:$2)"; return 1
}
rc=0
[ -n "${NSQL_ORACLE_URL:-}" ]   && { wait_port "${ORACLE_HOST:-oracle}" 1521 oracle || rc=1; }
[ -n "${NSQL_MSSQL_URL:-}" ]    && { wait_port "${MSSQL_HOST:-mssql}" 1433 mssql || rc=1; }
[ -n "${NSQL_POSTGRES_URL:-}" ] && { wait_port "${POSTGRES_HOST:-postgres}" 5432 postgres || rc=1; }
[ -n "${NSQL_MYSQL_URL:-}" ]    && { wait_port "${MYSQL_HOST:-mysql}" 3306 mysql || rc=1; }
exit $rc
