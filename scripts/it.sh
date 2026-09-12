#!/usr/bin/env bash
# 통합 테스트(실서버) — 환경변수 NSQL_ORACLE_URL · NSQL_MSSQL_URL 이 있는 것만 실행된다(없으면 건너뜀).
#   Codespaces/devcontainer: 그대로 실행.  로컬: docker compose -f .devcontainer/docker-compose.yml up -d oracle mssql 후
#   ORACLE_HOST=localhost MSSQL_HOST=localhost NSQL_ORACLE_URL=oracle://nexa:nexa@localhost:1521/FREEPDB1 … scripts/it.sh
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/wait-for-db.sh
cargo test -p nsql-drivers --test integration -- --nocapture --test-threads=1
