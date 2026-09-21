# win-big-probe.ps1 — 격리 설정 폴더 + 기동 명령으로 앱을 띄워 **초 단위로** CPU · 상주(Working Set · Private) · 피크 워킹셋 ·
#   응답 여부를 찍는다(입력 주입 없음 · docs/61 §4). 큰 파일 열기 · 메모리 회수 · 유휴 CPU를 앞뒤로 비교할 때 쓴다.
#
# 사용(Release로 잰다 — Debug의 "멈춤"은 최적화 없는 빌드의 느림일 수 있다):
#   pwsh -NoProfile -File scripts/win-big-probe.ps1 -HomeDir C:\tmp\nsql-home `
#        -Cmd "open:C:\tmp\big60.sql,@after:2500:bigfile.open" -Secs 12 -Tag open60 -ArgList Local
#
# 읽는 법: cpu+= 는 그 1초 동안 쓴 CPU(ms) — 열고 난 뒤의 값이 유휴 CPU다 · peakWS가 상주보다 많이 크면 적재 경로에 사본이 있다.
param(
    [string]$Exe = "",
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [string]$Cmd = "",
    [int]$Secs = 20,
    [string]$Tag = "probe",
    [string]$ArgList = ""
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\nexa-sql.exe" }
if ($HomeDir -match '\$' -or -not (Test-Path -LiteralPath $HomeDir -PathType Container)) {
    # 메시지는 영어로(콘솔 코드 페이지에 따라 한글이 깨진다).
    throw "HomeDir does not exist or looks wrong: '$HomeDir' - create the sandbox folder first and check the path."
}
$env:NSQL_HOME = $HomeDir
$env:NSQL_NO_ACTIVATE = "1"  # 시험 창이 사용자의 전경 포커스를 가져가지 않게(docs/61 §4)
$env:NSQL_STARTUP_CMD = $Cmd
if ($ArgList) { $p = Start-Process -FilePath $Exe -ArgumentList $ArgList -WorkingDirectory (Split-Path $Exe) -PassThru }
else { $p = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru }
try {
    $prev = 0.0
    foreach ($i in 1..$Secs) {
        Start-Sleep -Seconds 1
        $p.Refresh()
        if ($p.HasExited) { "$Tag`t process exited at t=${i}s"; break }
        $cpu = $p.TotalProcessorTime.TotalMilliseconds
        "{0}`t t={1,2}s cpu+={2,5:N0}ms ws={3,6:N1}MB priv={4,6:N1}MB peakWS={5,6:N1}MB resp={6}" -f `
            $Tag, $i, ($cpu - $prev), ($p.WorkingSet64 / 1MB), ($p.PrivateMemorySize64 / 1MB), ($p.PeakWorkingSet64 / 1MB), $p.Responding
        $prev = $cpu
    }
}
finally {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
