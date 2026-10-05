$ErrorActionPreference = 'Stop'

# Per-machine WiX MSI. Silent install needs an elevated shell (choco normally runs elevated).
# 3010/1641 = success, reboot requested/initiated.
$packageArgs = @{
  packageName    = 'nexa-sql'
  fileType       = 'msi'
  url64bit       = 'https://github.com/SosomLab/nexa-sql/releases/download/v@VERSION@/nexa-sql-@VERSION@-windows-x64.msi'
  checksum64     = '@SHA_WIN_X64_MSI@'
  checksumType64 = 'sha256'
  silentArgs     = '/qn /norestart'
  validExitCodes = @(0, 3010, 1641)
}
Install-ChocolateyPackage @packageArgs

Write-Host 'Nexa SQL (GUI) and nsql (CLI) are installed under Program Files. The nsql folder is added to the system PATH; open a new terminal to use it.'
