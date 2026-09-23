#!/usr/bin/env bash
# 확장 SDK 샘플 빌드 → 공식 패키지 폴더에 .wasm 배치 + sha256 갱신(docs/75 · macOS/Linux).
#   scripts/ext-build.sh [rainbow-pairs|hello-ext]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="$ROOT/extensions/sdk"
ONLY="${1:-}"
( cd "$SDK" && cargo build --release )
sha() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
place() {
  local crate="$1" pkg="$2" file="$3"
  if [[ -n "$ONLY" && "$ONLY" != "$pkg" ]]; then return; fi
  local src="$SDK/target/wasm32-unknown-unknown/release/$crate.wasm"
  local dir="$ROOT/extensions/$pkg"
  mkdir -p "$dir"
  cp -f "$src" "$dir/$file"
  local h; h="$(sha "$dir/$file")"
  echo "$dir/$file: $(wc -c < "$dir/$file") bytes sha256 $h"
  local meta="$dir/extension.json"
  if [[ -f "$meta" ]]; then
    python3 - "$meta" "$file" "$h" <<'EOF'
import io, re, sys
meta, fname, h = sys.argv[1:4]
s = io.open(meta, encoding='utf-8').read()
pat = r'("path"\s*:\s*"' + re.escape(fname) + r'"\s*,\s*"sha256"\s*:\s*")[0-9a-f]*(")'
n = re.sub(pat, lambda m: m.group(1) + h + m.group(2), s)
if n != s:
    io.open(meta, 'w', encoding='utf-8', newline='\n').write(n)
    print("  extension.json sha256 updated")
EOF
  fi
}
place rainbow_pairs_ext rainbow-pairs rainbow_pairs.wasm
place hello_ext hello-ext hello_ext.wasm
