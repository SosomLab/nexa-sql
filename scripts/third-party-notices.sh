#!/usr/bin/env bash
# third-party-notices.sh — THIRD-PARTY-NOTICES 초안 생성(bash 래퍼 · 본체는 third-party-notices.py · 표준 라이브러리만).
#   사용: scripts/third-party-notices.sh <출력 파일>     (기본 target/packaging/THIRD-PARTY-NOTICES.txt)
#   T-10(cargo-deny 정책 게이트·라이선스 전문 동봉)은 후속 — 여기서는 cargo metadata의 목록만.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/target/packaging/THIRD-PARTY-NOTICES.txt}"
mkdir -p "$(dirname "$OUT")"
PY="$(command -v python3 || command -v python || true)"
[ -n "$PY" ] || { echo "python3 없음 — THIRD-PARTY-NOTICES를 만들 수 없다" >&2; exit 1; }
# rustup 툴체인의 cargo를 앞세운다(Homebrew cargo가 앞서면 --locked 해석이 달라질 수 있다 · check-3os.sh 처방).
[ -d "$HOME/.cargo/bin" ] && export PATH="$HOME/.cargo/bin:$PATH"
exec "$PY" "$ROOT/scripts/third-party-notices.py" "$OUT" --manifest-path "$ROOT/Cargo.toml"
