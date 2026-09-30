# 개발 중인 확장 패키지를 **설치본에 그대로 반영**(삭제 후 재설치와 같은 결과 · 사용자 09-30 "for kiros33이 수정되면 삭제 후 재설치").
#   pwsh -NoProfile -File scripts/ext-sync-installed.ps1                      # sql-formatter-kiros33
#   pwsh -NoProfile -File scripts/ext-sync-installed.ps1 -Id rainbow-pairs
# 하는 일 = 저장소 `extensions/<id>/extension.json`을 읽어 `<설정 폴더>/extensions/<id>/`를 지우고 `<version>/`에 파일·메타 사본을 두고
#   `installed.json`을 쓴다(앱 `manager::install_traced`와 같은 배치 · dest 배치 없음 = placed []). 설정 폴더 = `NSQL_HOME` 또는 OS 사용자 폴더.
#   앱이 떠 있으면 wasm이 잠겨 있을 수 있으니 먼저 끝낸다(이 저장소 target/ 아래 인스턴스만).
param([string]$Id = "sql-formatter-kiros33")
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$pkg = Join-Path $repo ("extensions\" + $Id)
$metaPath = Join-Path $pkg "extension.json"
if (-not (Test-Path $metaPath)) { throw "no package: $metaPath" }
$meta = Get-Content $metaPath -Raw | ConvertFrom-Json
$home_ = if ($env:NSQL_HOME) { $env:NSQL_HOME } else { Join-Path $env:APPDATA "nexa-sql" }
$root = Join-Path $home_ "extensions"
$dir = Join-Path $root $Id
$keep = Join-Path $dir $meta.version
if (Test-Path $dir) {
    Remove-Item -Recurse -Force $dir
    Write-Output ("removed {0}" -f $dir)
}
New-Item -ItemType Directory -Force $keep | Out-Null
foreach ($f in $meta.files) {
    $src = Join-Path $pkg $f.path
    $bytes = [System.IO.File]::ReadAllBytes($src)
    $hash = ([System.Security.Cryptography.SHA256]::Create().ComputeHash($bytes) | ForEach-Object { $_.ToString("x2") }) -join ""
    if ($hash -ne $f.sha256) { throw ("sha256 mismatch for {0}: {1} != {2} (run scripts/ext-build.ps1 first)" -f $f.path, $hash, $f.sha256) }
    $dst = Join-Path $keep $f.path
    New-Item -ItemType Directory -Force (Split-Path -Parent $dst) | Out-Null
    Copy-Item $src $dst -Force
    Write-Output ("store {0} ({1} bytes · sha256 ok)" -f $dst, $bytes.Length)
}
Copy-Item $metaPath (Join-Path $keep "extension.json") -Force
$rec = @"
{
  "format": 1,
  "id": "$($meta.id)",
  "name": "$($meta.name)",
  "version": "$($meta.version)",
  "kind": "$($meta.kind)",
  "placed": []
}
"@
[System.IO.File]::WriteAllText((Join-Path $dir "installed.json"), $rec, (New-Object System.Text.UTF8Encoding($false)))
Write-Output ("installed {0} {1} → {2}" -f $meta.id, $meta.version, $dir)
