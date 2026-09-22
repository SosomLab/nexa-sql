# win-kill-stale-hooks.ps1 — Claude Code 훅(GitKraken `gk ai hook run --host claude-code`)이
# 남긴 좀비 프로세스 트리(bash.exe → bash.exe → gk_*.exe)를 정리한다.
#
# 배경(2026-09-22): GitKraken 플러그인의 PermissionRequest 훅이 `--blocking` + timeout 86400초로
# 정의돼 있어, 권한 확인을 VS Code에서 답하면 gk 프로세스가 24시간 동안 기다린다.
# 권한 확인 한 번 = bash 2 + gk 1 + conhost 1 이 남는다.
#
# 규칙: 명령줄이 훅 패턴과 정확히 일치하고, MinAgeMin 분 이상 된 것만 종료한다
#       (진행 중인 권한 확인의 훅은 건드리지 않는다). 세션 본체 claude.exe·cargo·사용자 셸은 대상 아님.
#
# 사용:  pwsh -NoProfile -File scripts/win-kill-stale-hooks.ps1            # 종료
#        pwsh -NoProfile -File scripts/win-kill-stale-hooks.ps1 -DryRun    # 목록만
#        -MinAgeMin 10 (기본)  -Log <경로> (기본 %LOCALAPPDATA%\Temp\gk-hook-cleanup.log)
#        -Register [-IntervalMin 30]  : 작업 스케줄러에 현재 사용자 주기 작업 등록
#        -Unregister                  : 그 작업 제거
[CmdletBinding()]
param(
    [int]$MinAgeMin = 10,
    [switch]$DryRun,
    [string]$Log = (Join-Path $env:LOCALAPPDATA 'Temp\gk-hook-cleanup.log'),
    [switch]$Register,
    [switch]$Unregister,
    [int]$IntervalMin = 30
)

$ErrorActionPreference = 'Stop'
$TaskName = 'claude-gk-hook-cleanup'
# bash 래퍼의 명령줄은 `gk.exe\" ai hook …`(이스케이프된 따옴표) · gk 본체는 `gk.exe ai hook …`
$Pattern  = 'gk(_[\d_]+)?\.exe\\?"?\s+ai hook run --host claude-code'

function Write-Log([string]$msg) {
    $line = "{0:yyyy-MM-dd HH:mm:ss} {1}" -f (Get-Date), $msg
    Write-Host $line
    if ($Log) {
        try {
            $dir = Split-Path $Log -Parent
            if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
            Add-Content -Path $Log -Value $line -Encoding utf8
        } catch {}
    }
}

if ($Unregister) {
    try { Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction Stop; Write-Log "task '$TaskName' removed" }
    catch { Write-Log "task '$TaskName' not found" }
    return
}

if ($Register) {
    $pwshExe = (Get-Process -Id $PID).Path
    $script  = $PSCommandPath
    $action  = New-ScheduledTaskAction -Execute $pwshExe `
        -Argument "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$script`" -MinAgeMin $MinAgeMin"
    $trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Minutes $IntervalMin)
    $settings = New-ScheduledTaskSettingsSet -Hidden -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Minutes 5) `
        -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings -Force | Out-Null
    Write-Log "task '$TaskName' registered: every $IntervalMin min, script=$script, min age=$MinAgeMin min"
    return
}

$now = Get-Date
$procs = Get-CimInstance Win32_Process | Where-Object {
    $_.CommandLine -and $_.CommandLine -match $Pattern -and $_.Name -match '^(bash|sh|gk(_[\d_]+)?)\.exe$'
}
$stale = @($procs | Where-Object { ($now - $_.CreationDate).TotalMinutes -ge $MinAgeMin })
$fresh = @($procs | Where-Object { ($now - $_.CreationDate).TotalMinutes -lt $MinAgeMin })

if ($stale.Count -eq 0) {
    if ($fresh.Count -gt 0) { Write-Log "nothing stale (fresh hook processes kept: $($fresh.Count))" }
    elseif (-not $DryRun -or $procs.Count -eq 0) { Write-Verbose "nothing to do" }
    if ($DryRun) { Write-Host "no stale hook processes (>= $MinAgeMin min)" }
    return
}

$trees = ($stale | Where-Object { $_.Name -like 'gk*' }).Count
$mb = [math]::Round(($stale | Measure-Object WorkingSetSize -Sum).Sum / 1MB)
Write-Log ("found {0} stale hook processes ({1} gk trees, ~{2} MB, oldest {3:N0} min); fresh kept: {4}" -f `
    $stale.Count, $trees, $mb, ((($stale | Sort-Object CreationDate | Select-Object -First 1).CreationDate) | ForEach-Object { ($now - $_).TotalMinutes }), $fresh.Count)

if ($DryRun) {
    $stale | Sort-Object CreationDate | ForEach-Object {
        "{0,6} {1,-16} {2,5:N0} min  {3}" -f $_.ProcessId, $_.Name, ($now - $_.CreationDate).TotalMinutes, $_.CommandLine.Substring(0, [math]::Min(90, $_.CommandLine.Length))
    }
    return
}

# 자식(gk) 먼저 — gk가 죽으면 `bash -c`는 스스로 끝나므로(자식 대기가 풀림) bash는 대개 이미 사라져 있다.
$killed = 0; $gone = 0
foreach ($p in ($stale | Sort-Object { if ($_.Name -like 'gk*') { 0 } else { 1 } })) {
    if (-not (Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue)) { $gone++; continue }
    try { Stop-Process -Id $p.ProcessId -Force -ErrorAction Stop; $killed++ }
    catch { Write-Log ("skip pid {0} {1}: {2}" -f $p.ProcessId, $p.Name, $_.Exception.Message) }
}
Write-Log "killed $killed, exited by themselves $gone, of $($stale.Count)"
