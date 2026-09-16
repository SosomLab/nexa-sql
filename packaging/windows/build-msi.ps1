<#
.SYNOPSIS
  build-msi.ps1 — Windows 설치본(MSI · WiX v4) 빌드(D-49 · DR-27 · T-72/T-62).

.DESCRIPTION
  흐름: (wix 도구 확인) → cargo build --release(타깃) → 스테이징(target\packaging\windows\stage)
        → LICENSE.md → license.rtf → wix build → (signtool · 시크릿 있을 때만) → 산출물 sha256.
  산출: target\packaging\windows\nexa-sql-<ver>-windows-<x64|arm64>.msi

  WiX v4 설치(1회 · 로컬/CI 동일):
      dotnet tool install --global wix           # 또는 dotnet tool update --global wix
      wix extension add --global WixToolset.UI.wixext
  이 스크립트는 wix가 없으면 위 명령을 시도하고(-NoToolInstall 이면 안내만 하고 멈춘다).

  조용한 설치 검증 절차(관리자 PowerShell · CI의 windows 러너에서도 같은 순서 · release.yml "smoke"):
      msiexec /i .\nexa-sql-0.0.1-windows-x64.msi /qn /norestart /l*v install.log
      & "$env:ProgramFiles\Nexa SQL\nsql.exe" --version          # "nsql 0.0.1"
      Get-ItemProperty 'HKLM:\SOFTWARE\SosomLab\Nexa SQL'          # InstallDir · Version · PathAdded
      msiexec /x .\nexa-sql-0.0.1-windows-x64.msi /qn /norestart
      Test-Path "$env:ProgramFiles\Nexa SQL"                        # False = 잔여 파일 0(사용자 폴더 %APPDATA%\nexa-sql 는 보존)
  기능 선택 예: ADDLOCAL=Main,PathEnv,SqlAssoc  · PATH를 빼려면 REMOVE=PathEnv.

.PARAMETER Target
  Rust 타깃 트리플. 기본 x86_64-pc-windows-msvc (aarch64-pc-windows-msvc → -Arch arm64 자동).
.PARAMETER SkipBuild
  이미 빌드된 target\<Target>\release\*.exe 를 그대로 포장.
.PARAMETER Version
  MSI 버전(기본 = Cargo.toml [workspace.package] version · NSQL_VERSION 환경변수). 사전 릴리스 접미사("-dev.abc")는 MSI 규칙상 뗀다.
#>
[CmdletBinding()]
param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [switch]$SkipBuild,
    [switch]$NoToolInstall,
    [string]$Version = ""
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Out = Join-Path $Root "target\packaging\windows"
$Stage = Join-Path $Out "stage"
$Branding = Join-Path $Root "packaging\branding"

function Step($m) { Write-Host "`n── $m ──" }
function Note($m) { Write-Host "  · $m" }

# ── 버전(단일 원천 = Cargo.toml) ──
if (-not $Version) { $Version = $env:NSQL_VERSION }
if (-not $Version) {
    $Version = (Select-String -Path (Join-Path $Root "Cargo.toml") -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
$FullVersion = $Version
$MsiVersion = ($Version -split "-")[0]      # MSI ProductVersion = 숫자 3자리만
$Arch = if ($Target -like "aarch64-*") { "arm64" } else { "x64" }
$Slug = "windows-$Arch"

# ── WiX v4 ──
Step "WiX v4 도구"
if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
    if ($NoToolInstall) { throw "wix 없음 — dotnet tool install --global wix ; wix extension add --global WixToolset.UI.wixext" }
    dotnet tool install --global wix | Out-Null
    $env:PATH = "$env:USERPROFILE\.dotnet\tools;$env:PATH"
}
Note ("wix " + (wix --version))
# UI 확장(기능 트리·라이선스 화면). 이미 있으면 조용히 지나간다.
$exts = (wix extension list --global 2>$null) -join "`n"
if ($exts -notmatch "WixToolset.UI.wixext") { wix extension add --global WixToolset.UI.wixext | Out-Null }

# ── 빌드 ──
Step "빌드 (release · $Target)"
if (-not $SkipBuild) {
    & cargo build --release --locked -p nexa-sql -p nsql-cli --target $Target
    if ($LASTEXITCODE -ne 0) { throw "cargo build 실패" }
}
$BinDir = Join-Path $Root "target\$Target\release"
foreach ($exe in "nexa-sql.exe", "nsql.exe") {
    if (-not (Test-Path (Join-Path $BinDir $exe))) { throw "산출물 없음: $BinDir\$exe" }
}

# ── 스테이징(docs/33 §2 레이아웃) ──
Step "스테이징 → $Stage"
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path (Join-Path $Stage "Packages") | Out-Null
Copy-Item (Join-Path $BinDir "nexa-sql.exe") $Stage
Copy-Item (Join-Path $BinDir "nsql.exe") $Stage
Copy-Item (Join-Path $Root "LICENSE.md"), (Join-Path $Root "LICENSE.ko.md"), (Join-Path $Root "README.md") $Stage
$Pk = if ($env:NSQL_PACKAGES_DIR) { $env:NSQL_PACKAGES_DIR } else { Join-Path $Root "Packages" }
if (Test-Path $Pk) { Copy-Item -Recurse (Join-Path $Pk "*") (Join-Path $Stage "Packages"); Note "Packages\ ← $Pk" } else { Note "Packages\ 비어 있음" }
# THIRD-PARTY-NOTICES — scripts/third-party-notices.py(표준 라이브러리만). 러너·개발기 모두 python 있음.
$py = Get-Command python -ErrorAction SilentlyContinue
if (-not $py) { $py = Get-Command python3 -ErrorAction SilentlyContinue }
if (-not $py) { throw "python 없음 — THIRD-PARTY-NOTICES 생성 불가" }
& $py.Source (Join-Path $Root "scripts\third-party-notices.py") (Join-Path $Stage "THIRD-PARTY-NOTICES.txt") --manifest-path (Join-Path $Root "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "THIRD-PARTY-NOTICES 실패" }
# 라이선스 화면용 RTF — LICENSE.md 본문을 그대로(마크다운 기호 포함) 고정폭으로. 역슬래시·중괄호만 이스케이프.
$lic = Get-Content (Join-Path $Root "LICENSE.md") -Raw -Encoding UTF8
$esc = $lic -replace '\\', '\\\\' -replace '\{', '\{' -replace '\}', '\}'
$body = ($esc -split "`r?`n" | ForEach-Object {
    # 비ASCII 문자는 \uN? 로(RTF 유니코드 이스케이프)
    $sb = New-Object System.Text.StringBuilder
    foreach ($ch in $_.ToCharArray()) { if ([int]$ch -gt 127) { [void]$sb.Append('\u' + [int]$ch + '?') } else { [void]$sb.Append($ch) } }
    $sb.ToString() + '\par'
}) -join "`n"
"{\rtf1\ansi\ansicpg65001\deff0{\fonttbl{\f0\fmodern Consolas;}}\f0\fs18`n$body`n}" | Set-Content -Path (Join-Path $Stage "license.rtf") -Encoding ASCII
Note "license.rtf ← LICENSE.md"

# ── wix build ──
Step "wix build ($Arch · $MsiVersion)"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Msi = Join-Path $Out "nexa-sql-$FullVersion-$Slug.msi"
& wix build -arch $Arch `
    -d "Version=$MsiVersion" -d "Staging=$Stage" -d "Branding=$Branding" -d "Arch=$Arch" `
    -ext WixToolset.UI.wixext `
    -culture en-US `
    -o $Msi `
    (Join-Path $PSScriptRoot "nexa-sql.wxs")
if ($LASTEXITCODE -ne 0) { throw "wix build 실패" }

# ── 서명(DR-20 · 자리만): 인증서 지문(WINDOWS_SIGN_THUMBPRINT) 또는 PFX(WINDOWS_SIGN_PFX + WINDOWS_SIGN_PFX_PASSWORD) 있을 때만 ──
Step "서명"
$signtool = Get-Command signtool -ErrorAction SilentlyContinue
if ($signtool -and ($env:WINDOWS_SIGN_THUMBPRINT -or $env:WINDOWS_SIGN_PFX)) {
    $args = @("sign", "/fd", "SHA256", "/tr", "http://timestamp.digicert.com", "/td", "SHA256", "/d", "Nexa SQL")
    if ($env:WINDOWS_SIGN_THUMBPRINT) { $args += @("/sha1", $env:WINDOWS_SIGN_THUMBPRINT) }
    else { $args += @("/f", $env:WINDOWS_SIGN_PFX); if ($env:WINDOWS_SIGN_PFX_PASSWORD) { $args += @("/p", $env:WINDOWS_SIGN_PFX_PASSWORD) } }
    & $signtool.Source @args $Msi
    if ($LASTEXITCODE -ne 0) { throw "signtool 실패" }
    Note "signtool ✓"
} else {
    Note "unsigned(서명 시크릿/signtool 없음 · SmartScreen 안내는 릴리스 노트)"
}

Step "산출물"
$sha = (Get-FileHash -Algorithm SHA256 $Msi).Hash.ToLower()
Note ("{0}  {1:N0} bytes  sha256 {2}" -f (Split-Path -Leaf $Msi), (Get-Item $Msi).Length, $sha)
Write-Output "MSI=$Msi"
