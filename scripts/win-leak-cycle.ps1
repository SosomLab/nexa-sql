# win-leak-cycle.ps1 — **메모리 릭 점검**(입력 주입 없음 · docs/61 §4): 같은 동작 묶음을 N번 되풀이시키고 주기마다(묶음이 끝난
#   "쉬는 상태"에서) Private · 워킹셋 · 핸들 · GDI · USER · 스레드를 찍는다. 주기가 늘어도 값이 계단처럼 오르지 않아야 한다.
#   동작은 기동 명령의 시차 실행(`@after:<ms>:<명령>`)으로 건다 — 키·마우스를 보내지 않고 포커스도 빼앗지 않는다.
#
# 사용:
#   # 큰 파일 열기 → 닫기 10회
#   pwsh -NoProfile -File scripts/win-leak-cycle.ps1 -HomeDir C:\tmp\home -Cycles 10 -PeriodMs 4000 `
#        -CycleCmds "open:C:\tmp\big20.sql;file.close_tab" -ArgList Local
#   # 10만 행 조회를 되풀이(결과 교체)
#   ... -First "open:C:\tmp\rows.sql" -CycleCmds "run.all" -PeriodMs 6000
#   # 보조 창 열기 → 닫기(토글 명령)
#   ... -CycleCmds "view.log;view.log" -PeriodMs 2000
#
# 읽는 법: 첫 1~2주기는 캐시·글리프가 채워지며 오른다(정상). 그 뒤 **마지막 절반의 기울기**(MB/주기)가 0에 가까우면 누수 없음.
param(
    [string]$Exe = "",
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [string]$First = "",
    [Parameter(Mandatory = $true)][string]$CycleCmds,
    [int]$Cycles = 10,
    [int]$PeriodMs = 4000,
    [int]$StartMs = 4000,
    [string]$ArgList = "",
    [string]$Tag = "leak"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\nexa-sql.exe" }
if ($HomeDir -match '\$' -or -not (Test-Path -LiteralPath $HomeDir -PathType Container)) {
    throw "HomeDir does not exist or looks wrong: '$HomeDir' - create the sandbox folder first and check the path."
}
Add-Type @"
using System; using System.Runtime.InteropServices;
public class NxLeak { [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr h, uint flags); }
"@
# 주기 i의 동작들을 주기의 앞 60% 안에 고르게 놓고, 나머지 40%는 쉬게 둔다(그 끝에서 잰다).
# @(…) = 동작이 하나여도 배열로(문자열이면 `$acts[0]`이 첫 '글자'가 된다).
$acts = @($CycleCmds.Split(';') | Where-Object { $_ })
$cmds = @()
if ($First) { $cmds += $First.Split(';') | Where-Object { $_ } }
for ($i = 0; $i -lt $Cycles; $i++) {
    $t0 = $StartMs + $i * $PeriodMs
    for ($k = 0; $k -lt $acts.Count; $k++) {
        $at = $t0 + [int]($PeriodMs * 0.6 * $k / [Math]::Max(1, $acts.Count))
        $cmds += "@after:${at}:$($acts[$k])"
    }
}
$env:NSQL_HOME = $HomeDir
$env:NSQL_STARTUP_CMD = ($cmds -join ',')
if ($ArgList) { $p = Start-Process -FilePath $Exe -ArgumentList $ArgList -WorkingDirectory (Split-Path $Exe) -PassThru }
else { $p = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru }
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$rows = @()
try {
    function Snap([string]$name) {
        $p.Refresh()
        $row = [pscustomobject]@{
            at = $name; priv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2); ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
            handles = $p.HandleCount; gdi = [NxLeak]::GetGuiResources($p.Handle, 0); user = [NxLeak]::GetGuiResources($p.Handle, 1)
            threads = $p.Threads.Count
        }
        "{0}`t{1,-8} priv={2,7:N2}MB ws={3,6:N1}MB handles={4,4} gdi={5,3} user={6,3} threads={7,2}" -f `
            $Tag, $row.at, $row.priv, $row.ws, $row.handles, $row.gdi, $row.user, $row.threads
        $row
    }
    # 기준선: 첫 주기 직전.
    $wait = $StartMs - 300 - $sw.ElapsedMilliseconds
    if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
    $rows += , (Snap "base" | Select-Object -Last 1)
    for ($i = 0; $i -lt $Cycles; $i++) {
        $target = $StartMs + ($i + 1) * $PeriodMs - 300
        $wait = $target - $sw.ElapsedMilliseconds
        if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
        if ($p.HasExited) { "$Tag`t process exited"; break }
        $out = Snap ("cycle" + ($i + 1))
        $out | Select-Object -First 1
        $rows += , ($out | Select-Object -Last 1)
    }
    # 마지막 절반의 기울기(MB/주기).
    $half = $rows | Select-Object -Last ([Math]::Max(2, [int]($rows.Count / 2)))
    if ($half.Count -ge 2) {
        $slope = ($half[-1].priv - $half[0].priv) / ($half.Count - 1)
        "{0}`tslope(last half) = {1:N3} MB/cycle | handles {2} -> {3} | gdi {4} -> {5} | user {6} -> {7}" -f `
            $Tag, $slope, $half[0].handles, $half[-1].handles, $half[0].gdi, $half[-1].gdi, $half[0].user, $half[-1].user
    }
}
finally {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
