# 확장 SDK 샘플 빌드 → 공식 패키지 폴더에 .wasm 배치 + sha256 갱신(docs/75 · Windows).
#   pwsh -NoProfile -File scripts/ext-build.ps1            # 전부
#   pwsh -NoProfile -File scripts/ext-build.ps1 -Only rainbow-pairs
# 산출물: extensions/<패키지>/<이름>.wasm · extension.json의 files[].sha256 갱신(있을 때) · 크기·시간 출력.
param([string]$Only = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$sdk = Join-Path $root "extensions\sdk"
$sw = [System.Diagnostics.Stopwatch]::StartNew()
Push-Location $sdk
try {
    cargo build --release 2>&1 | ForEach-Object { "  $_" }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally { Pop-Location }
Write-Output ("build: {0:N0} ms" -f $sw.Elapsed.TotalMilliseconds)
# (샘플 crate 이름 → 패키지 폴더 · 배치 파일 이름)
$map = @(
    @{ crate = "rainbow_pairs_ext"; pkg = "rainbow-pairs"; file = "rainbow_pairs.wasm" },
    @{ crate = "hello_ext";         pkg = "hello-ext";     file = "hello_ext.wasm" }
)
foreach ($m in $map) {
    if ($Only -and $Only -ne $m.pkg) { continue }
    $src = Join-Path $sdk ("target\wasm32-unknown-unknown\release\" + $m.crate + ".wasm")
    $dir = Join-Path $root ("extensions\" + $m.pkg)
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    $dst = Join-Path $dir $m.file
    Copy-Item -LiteralPath $src -Destination $dst -Force
    $hash = (Get-FileHash -LiteralPath $dst -Algorithm SHA256).Hash.ToLowerInvariant()
    $size = (Get-Item -LiteralPath $dst).Length
    Write-Output ("{0}: {1} bytes sha256 {2}" -f $dst, $size, $hash)
    $meta = Join-Path $dir "extension.json"
    if (Test-Path -LiteralPath $meta) {
        $text = Get-Content -LiteralPath $meta -Raw -Encoding UTF8
        $pattern = '("path"\s*:\s*"' + [regex]::Escape($m.file) + '"\s*,\s*"sha256"\s*:\s*")[0-9a-f]*(")'
        $new = [regex]::Replace($text, $pattern, ('${1}' + $hash + '${2}'))
        if ($new -ne $text) {
            [System.IO.File]::WriteAllText($meta, $new, (New-Object System.Text.UTF8Encoding($false)))
            Write-Output ("  extension.json sha256 updated")
        }
    }
}
