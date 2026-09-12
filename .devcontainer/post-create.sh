#!/usr/bin/env bash
# Codespaces/devcontainer 생성 후 1회: 형제 저장소 nexa-ui(path 의존) · cargo fetch · DB 준비 대기.
set -euo pipefail
if [ ! -d /workspaces/nexa-ui/.git ]; then
  git clone --depth 1 https://github.com/SosomLab/nexa-ui /workspaces/nexa-ui
else
  git -C /workspaces/nexa-ui pull --ff-only || true
fi
cd /workspaces/nexa-sql
cargo fetch
bash scripts/wait-for-db.sh || true
echo "준비 완료 — 통합 테스트: scripts/it.sh"
