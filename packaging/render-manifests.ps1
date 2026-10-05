<#
.SYNOPSIS
  Render the winget and Chocolatey manifests for one release — the single substitution point.

.DESCRIPTION
  render-manifests.ps1 -Version 0.1.5 -Assets <dir with the released files> -Out <output dir>

  The asset directory must contain the released MSI exactly as uploaded
  (nexa-sql-<version>-windows-x64.msi). The SHA-256 and the MSI ProductCode are read from that
  file — a hash or GUID typed by hand will eventually be wrong, and a wrong one shows up as an
  install failure on a user's machine. Any @PLACEHOLDER@ left after rendering is an error.

  Windows only (the ProductCode is read through the Windows Installer COM object).
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$Version,
  [Parameter(Mandatory)] [string]$Assets,
  [Parameter(Mandatory)] [string]$Out
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$msi = Join-Path $Assets "nexa-sql-$Version-windows-x64.msi"
if (-not (Test-Path -LiteralPath $msi)) { throw "asset not found: $msi" }

$sha = (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash.ToUpperInvariant()

# ProductCode from the MSI Property table (WiX v4 generates a new one per build; UpgradeCode is fixed).
$msiPath = [string](Resolve-Path -LiteralPath $msi).Path
$installer = New-Object -ComObject WindowsInstaller.Installer
$db = $installer.OpenDatabase($msiPath, 0)   # 0 = msiOpenDatabaseModeReadOnly
$view = $db.OpenView("SELECT Value FROM Property WHERE Property = 'ProductCode'")
$view.Execute()
$rec = $view.Fetch()
if ($null -eq $rec) { throw "ProductCode not found in $msi" }
$productCode = [string]$rec.StringData(1)
$view.Close()
if ($productCode -notmatch '^\{[0-9A-Fa-f-]{36}\}$') { throw "unexpected ProductCode: $productCode" }

$date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')

function Fill([string]$src, [string]$dst) {
  $text = Get-Content -LiteralPath $src -Raw -Encoding UTF8
  $text = $text.Replace('@VERSION@', $Version).Replace('@DATE@', $date).Replace('@SHA_WIN_X64_MSI@', $sha).Replace('@PRODUCT_CODE@', $productCode)
  if ($text -match '@[A-Z_0-9]+@') { throw "unfilled placeholder in $src : $($Matches[0])" }
  # Submission files are English/ASCII only (store moderation; also keeps BOM/encoding issues away).
  if ($text -match '[^\x00-\x7F]') { throw "non-ASCII character in $src : $($Matches[0])" }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dst) | Out-Null
  # YAML/nuspec = UTF-8 without BOM; Chocolatey ps1 = UTF-8 with BOM (chocolatey.org requirement).
  $bom = [IO.Path]::GetExtension($dst) -ieq '.ps1'
  [IO.File]::WriteAllText($dst, $text, (New-Object Text.UTF8Encoding($bom)))
}

# winget: microsoft/winget-pkgs path = manifests/<first letter lower>/<Publisher>/<Package>/<version>
$id = 'SosomLab.NexaSQL'
$wdir = Join-Path $Out "winget/manifests/s/SosomLab/NexaSQL/$Version"
Fill (Join-Path $here 'winget/version.yaml')   (Join-Path $wdir "$id.yaml")
Fill (Join-Path $here 'winget/locale.yaml')    (Join-Path $wdir "$id.locale.en-US.yaml")
Fill (Join-Path $here 'winget/installer.yaml') (Join-Path $wdir "$id.installer.yaml")

# Chocolatey: ready for `choco pack`
$cdir = Join-Path $Out 'choco/nexa-sql'
Fill (Join-Path $here 'choco/nexa-sql.nuspec')               (Join-Path $cdir 'nexa-sql.nuspec')
Fill (Join-Path $here 'choco/tools/chocolateyinstall.ps1')   (Join-Path $cdir 'tools/chocolateyinstall.ps1')
Fill (Join-Path $here 'choco/tools/chocolateyuninstall.ps1') (Join-Path $cdir 'tools/chocolateyuninstall.ps1')

Write-Host "rendered: version=$Version sha256=$sha productCode=$productCode -> $Out"
