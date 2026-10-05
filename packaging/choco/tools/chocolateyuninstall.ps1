$ErrorActionPreference = 'Stop'

# Remove the MSI through its ProductCode from the Add/Remove Programs entry (per-machine, HKLM).
$packageArgs = @{
  packageName    = 'nexa-sql'
  softwareName   = 'Nexa SQL*'
  fileType       = 'msi'
  silentArgs     = '/qn /norestart'
  validExitCodes = @(0, 3010, 1605, 1614, 1641)
}

[array]$keys = Get-UninstallRegistryKey -SoftwareName $packageArgs['softwareName']
if ($keys.Count -eq 1) {
  $keys | ForEach-Object {
    $packageArgs['file'] = "$($_.UninstallString)"
    if ($packageArgs['fileType'] -eq 'msi') {
      # UninstallString is "MsiExec.exe /X{ProductCode}" or "/I{ProductCode}"; keep only the ProductCode.
      $packageArgs['silentArgs'] = "$($_.PSChildName) $($packageArgs['silentArgs'])"
      $packageArgs['file'] = ''
    }
    Uninstall-ChocolateyPackage @packageArgs
  }
} elseif ($keys.Count -eq 0) {
  Write-Warning "$($packageArgs['packageName']) has already been uninstalled by other means."
} else {
  Write-Warning "$($keys.Count) matches found for '$($packageArgs['softwareName'])'. Nothing was uninstalled."
  Write-Warning 'Please report this to the package maintainer with the list below:'
  $keys | ForEach-Object { Write-Warning "- $($_.DisplayName)" }
}
