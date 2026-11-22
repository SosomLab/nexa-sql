# license_warn.ps1 — 성능·캡처 스크립트 공용 조각(T-244 · 11-23 · license_warn.sh와 같은 판정): 측정 홈이 무료판이면 시작에 경고 한 줄(중단하지 않음).
#   사용: . (Join-Path $PSScriptRoot "lib\license_warn.ps1"); Write-LicenseWarning -Cli <nsql.exe> -HomeDir <NSQL_HOME> [-Sink { param($s) … }]
function Write-LicenseWarning([string]$Cli, [string]$HomeDir, [scriptblock]$Sink = { param($s) Write-Output $s }) {
    $prev = $env:NSQL_HOME
    try {
        $env:NSQL_HOME = $HomeDir
        $st = (& $Cli license status 2>$null | Where-Object { $_ -match '^state\s' } | Select-Object -First 1) -replace '^state\s+', ''
    } catch { $st = "" } finally { $env:NSQL_HOME = $prev }
    if (-not $st -or $st.Trim() -eq "free") {
        $label = if ($st) { $st.Trim() } else { "unknown" }
        & $Sink ("⚠ license = $label (home=$HomeDir) — files over 2 MB hit the free-edition large-file gate, so open/edit timings are skewed. Install a test license first: `$env:NSQL_HOME='$HomeDir'; & '$Cli' license install <file> (T-244)")
    }
}
