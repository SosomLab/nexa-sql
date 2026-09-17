# bench-boost.ps1 — 실행 속도 향상(perf.boost) 켬/끔 성능 비교(docs/45 · T-90e): Release exe로 GUI 3회(기동·유휴 자원·유휴 CPU) +
#   CLI 3회(nsql run --timing · 프로필 SNOPDB_19c(Oracle) · M4PLAN(SQL Server)). 결과 = targetench-boost.csv + 중앙값 요약.
# 사용:  pwsh -File scriptsench-boost.ps1 [-Runs 3] [-Out targetench-boost.csv]   (실행 전 cargo build --release · 프로필 비밀번호 저장돼 있어야)
param(
    [string]$Root = "D:\Projects\kiros33\nexa-sql",
    [int]$Runs = 3,
    [string]$Out = "D:\Projects\kiros33\nexa-sql\target\bench-boost.csv"
)
$ErrorActionPreference = "Continue"
$gui = Join-Path $Root "target\release\nexa-sql.exe"
$cli = Join-Path $Root "target\release\nsql.exe"
$rows = New-Object System.Collections.Generic.List[object]
function Rec($mode, $metric, $unit, $value, $note) { $rows.Add([pscustomobject]@{mode=$mode; metric=$metric; unit=$unit; value=[math]::Round($value, 2); note=$note}) }

function Gui-Sample($mode) {
    for ($r = 1; $r -le $Runs; $r++) {
        Get-Process nexa-sql -ErrorAction SilentlyContinue | Stop-Process -Force
        Start-Sleep -Milliseconds 500
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $p = Start-Process -FilePath $gui -WorkingDirectory $Root -PassThru
        $shown = $false
        while ($sw.ElapsedMilliseconds -lt 20000) {
            Start-Sleep -Milliseconds 10
            $p.Refresh()
            if ($p.MainWindowHandle -ne 0) { $shown = $true; break }
        }
        $t_win = $sw.ElapsedMilliseconds
        Rec $mode "gui.startup_to_window" "ms" $t_win "run $r (창 핸들 생성까지)"
        Start-Sleep -Seconds 4
        $p.Refresh()
        Rec $mode "gui.idle.working_set" "MB" ($p.WorkingSet64/1MB) "run $r (기동 4초 뒤)"
        Rec $mode "gui.idle.private_bytes" "MB" ($p.PrivateMemorySize64/1MB) "run $r"
        Rec $mode "gui.idle.threads" "count" $p.Threads.Count "run $r"
        Rec $mode "gui.idle.handles" "count" $p.HandleCount "run $r"
        $cpu0 = $p.TotalProcessorTime.TotalMilliseconds
        Start-Sleep -Seconds 10
        $p.Refresh()
        $cpu1 = $p.TotalProcessorTime.TotalMilliseconds
        Rec $mode "gui.idle.cpu_10s" "ms" ($cpu1 - $cpu0) "run $r (유휴 10초 동안 CPU 시간 · 창 포커스 없음)"
        Stop-Process -Id $p.Id -Force
        Start-Sleep -Milliseconds 300
    }
}

function Cli-Query($mode, $profile, $sql, $label) {
    $tmp = Join-Path $env:TEMP "bench-$label.sql"
    Set-Content -Path $tmp -Value $sql -Encoding UTF8
    for ($r = 1; $r -le $Runs; $r++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $err = & $cli run -c $profile --no-prompt --timing -f csv $tmp 2>&1 1>$null
        $ms = $sw.ElapsedMilliseconds
        $line = ($err | Out-String)
        $exec = [regex]::Match($line, 'execute\s+([\d.]+)\s*ms').Groups[1].Value
        $fetch = [regex]::Match($line, 'fetch\s+([\d.]+)\s*ms').Groups[1].Value
        Rec $mode "cli.$label.wall" "ms" $ms "run $r (프로세스 전체 · 접속 포함)"
        if ($exec) { Rec $mode "cli.$label.execute" "ms" ([double]$exec) "run $r (--timing Execute)" }
        if ($fetch) { Rec $mode "cli.$label.fetch" "ms" ([double]$fetch) "run $r (--timing Fetch)" }
    }
}

foreach ($mode in @("off", "on")) {
    & $cli config set perf.boost $mode | Out-Null
    Gui-Sample $mode
    Cli-Query $mode "SNOPDB_19c" "SELECT * FROM M4S_I002040 A WHERE A.PROJECT_CD = 'SEBANG';" "oracle_200"
    Cli-Query $mode "SNOPDB_19c" "SELECT COUNT(*) FROM M4S_I002040;" "oracle_count"
    Cli-Query $mode "M4PLAN" "SELECT TOP 200 * FROM INFORMATION_SCHEMA.COLUMNS;" "mssql_200"
}
& $cli config set perf.boost off | Out-Null
$rows | Export-Csv -Path $Out -NoTypeInformation -Encoding UTF8
$rows | Group-Object mode, metric | ForEach-Object {
    $vals = $_.Group.value | Sort-Object
    $med = $vals[[math]::Floor(($vals.Count - 1) / 2)]
    "{0,-4} {1,-28} median={2,10} unit={3}" -f $_.Group[0].mode, $_.Group[0].metric, $med, $_.Group[0].unit
}
