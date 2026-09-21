# win-startup-probe.ps1 — **시작 비용** 측정(입력 주입 없음 | docs/61 §4): 프로세스를 띄워 ① 메인 창이 보일 때까지의 시간
#   ② `-SettleSecs`초까지 쓴 CPU ③ 그때의 Private | 워킹셋 | 핸들 | 스레드를 `-Runs`번 재고 중앙값을 낸다.
#   두 빌드를 비교할 때는 번갈아 돌린다(파일 캐시·백신 검사의 영향을 고르게).
#
# 사용:
#   pwsh -NoProfile -File scripts/win-startup-probe.ps1 -HomeDir C:\tmp\home -ArgList Local -Runs 5
#   pwsh -NoProfile -File scripts/win-startup-probe.ps1 -HomeDir C:\tmp\home -Exe D:\old\nexa-sql.exe -Tag old
param(
    [string]$Exe = "",
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [string]$Cmd = "",
    [string]$ArgList = "",
    [int]$Runs = 5,
    [int]$SettleSecs = 3,
    [string]$Tag = "startup"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\nexa-sql.exe" }
if ($HomeDir -match '\$' -or -not (Test-Path -LiteralPath $HomeDir -PathType Container)) {
    throw "HomeDir does not exist or looks wrong: '$HomeDir' - create the sandbox folder first and check the path."
}
function Median($xs) { $s = @($xs | Sort-Object); $s[[int][Math]::Floor(($s.Count - 1) / 2)] }
$env:NSQL_HOME = $HomeDir
$env:NSQL_STARTUP_CMD = $Cmd
$rows = @()
foreach ($r in 1..$Runs) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    if ($ArgList) { $p = Start-Process -FilePath $Exe -ArgumentList $ArgList -WorkingDirectory (Split-Path $Exe) -PassThru }
    else { $p = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru }
    try {
        $shown = -1
        while ($sw.ElapsedMilliseconds -lt 15000) {
            $p.Refresh()
            if ($p.MainWindowHandle -ne [IntPtr]::Zero) { $shown = $sw.ElapsedMilliseconds; break }
            Start-Sleep -Milliseconds 10
        }
        $left = $SettleSecs * 1000 - $sw.ElapsedMilliseconds
        if ($left -gt 0) { Start-Sleep -Milliseconds $left }
        $p.Refresh()
        $row = [pscustomobject]@{
            shown = $shown; cpu = [math]::Round($p.TotalProcessorTime.TotalMilliseconds)
            priv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2); ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
            handles = $p.HandleCount; threads = $p.Threads.Count
        }
        "{0}`t run{1}: window {2,5} ms | cpu({3}s) {4,5} ms | priv {5,6:N2} MB | ws {6,5:N1} MB | handles {7} | threads {8}" -f `
            $Tag, $r, $row.shown, $SettleSecs, $row.cpu, $row.priv, $row.ws, $row.handles, $row.threads
        $rows += $row
    }
    finally { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Milliseconds 600
}
"{0}`t MEDIAN: window {1} ms | cpu {2} ms | priv {3:N2} MB | ws {4:N1} MB" -f `
    $Tag, (Median ($rows | ForEach-Object shown)), (Median ($rows | ForEach-Object cpu)), (Median ($rows | ForEach-Object priv)), (Median ($rows | ForEach-Object ws))
