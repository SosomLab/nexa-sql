#!/usr/bin/env bash
# packaging/lib.sh — 3-OS 패키징 스크립트 공용 조각(source 해서 쓴다 · 실행 파일 아님).
#
# 원칙(docs/33 DR-27): 설치본만 · 목적별 exe(GUI `nexa-sql` · CLI `nsql`) · 사용자 데이터는 OS 사용자 폴더.
# 이식 원천: ../nexa-clip/packaging(render-manifests.sh의 sha 함수 · release.yml 스테이징 단계) — 여기서는
#   "스크립트 = 로컬에서도 CI에서도 같은 명령"으로 재구성했다(CI가 셸 조각을 따로 들고 있으면 갈라진다).
#
# 제공:
#   ROOT            저장소 루트         OUT_ROOT  = $ROOT/target/packaging (git 무시 · /target/)
#   VERSION         Cargo.toml [workspace.package] version (NSQL_VERSION로 덮어쓰기 가능)
#   APP_NAME        "Nexa SQL"          BUNDLE_ID = com.sosomlab.nexa-sql
#   step / note / die / sha256_of / have / cargo_bin / stage_common <dir> / third_party_notices <파일>

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_ROOT="$ROOT/target/packaging"
BRANDING="$ROOT/packaging/branding"
APP_NAME="Nexa SQL"
BUNDLE_ID="com.sosomlab.nexa-sql"
PUBLISHER="SosomLab"
HOMEPAGE="https://github.com/SosomLab/nexa-sql"

# 버전 = 워크스페이스 단일 원천. 태그 빌드는 release.yml이 NSQL_VERSION으로 넘긴다(태그≠Cargo 버전이면 meta 잡이 멈춘다).
VERSION="${NSQL_VERSION:-$(grep -m1 '^version = ' "$ROOT/Cargo.toml" | sed -E 's/version = "(.*)"/\1/')}"

step() { printf '\n── %s ──\n' "$*"; }
note() { printf '  · %s\n' "$*"; }
die()  { printf '  ✗ %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# sha256 — OS마다 도구 이름이 다르다(리눅스 sha256sum · macOS shasum).
sha256_of() {
    if have sha256sum; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# ★ rustup 툴체인을 쓴다(check-3os.sh와 같은 처방 · 09-16 mac 실기): PATH에 Homebrew cargo가 앞서면 크로스 타깃 std를 못 찾는다.
cargo_bin() {
    if have rustup; then
        local rb; rb="$(dirname "$(rustup which cargo 2>/dev/null || echo "$HOME/.cargo/bin/cargo")")"
        [ -x "$rb/cargo" ] && { echo "$rb/cargo"; return; }
    fi
    command -v cargo
}

# 세 OS가 똑같이 담는 공통 파일: 라이선스 2종 · README · THIRD-PARTY-NOTICES · Packages/(동봉 패키지 · 지금은 비어 있음).
# 동봉 패키지 원천 = $ROOT/Packages (NSQL_PACKAGES_DIR로 덮어쓰기). 없으면 빈 폴더만 만든다 —
#   런타임 로더 순서(docs/09 "Default → 동봉 → 설치 → 사용자")의 '동봉' 자리다.
stage_common() {
    local dst="$1"
    mkdir -p "$dst/Packages"
    install -m 0644 "$ROOT/LICENSE.md" "$dst/LICENSE.md"
    install -m 0644 "$ROOT/LICENSE.ko.md" "$dst/LICENSE.ko.md"
    install -m 0644 "$ROOT/README.md" "$dst/README.md"
    local pk="${NSQL_PACKAGES_DIR:-$ROOT/Packages}"
    if [ -d "$pk" ]; then cp -R "$pk/." "$dst/Packages/"; note "Packages/ ← $pk"; else note "Packages/ 비어 있음(동봉 패키지 없음)"; fi
    third_party_notices "$dst/THIRD-PARTY-NOTICES.txt"
}

# THIRD-PARTY-NOTICES — cargo metadata에서 라이선스 목록(T-10 cargo-deny는 후속 · scripts/third-party-notices.sh).
third_party_notices() {
    if "$ROOT/scripts/third-party-notices.sh" "$1" >/dev/null; then note "THIRD-PARTY-NOTICES ← cargo metadata"
    else die "THIRD-PARTY-NOTICES 생성 실패"; fi
}
