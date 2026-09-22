# win-perf-all.ps1 — **Windows 성능 전수**(docs/71 점검 프로세스의 실행기 · Linux `linux-perf-all.sh`와 같은 구성):
#   A 인벤토리 → B 기동 → C 시나리오 상주·CPU → D **향상 모드 A/B** → E 릭 주기 → F 벤치.
#   Release · 격리 설정 폴더 · **입력 주입 없음**(기동 명령 `NSQL_STARTUP_CMD`로만 몬다 · docs/61 §4).
#
# 사용:
#   pwsh -NoProfile -File scripts/win-perf-all.ps1 -HomeDir C:\tmp\nsql-home -DataDir C:\tmp\nsql-data -Out target\perf-win
#   pwsh -NoProfile -File scripts/win-perf-all.ps1 -HomeDir ... -DataDir ... -Out ... -Stages boost      # 한 단계만
#
# 읽는 법: 같은 기기 안의 **전/후 A/B**와 예산(docs/26 §5 · 39 §2) 대비로 본다. 빌드 직후 첫 실행은 버린다(docs/61 §4).
param(
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [Parameter(Mandatory = $true)][string]$DataDir,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Exe = "",
    [string]$Cli = "",
    [string]$Stages = "inventory,startup,scenarios,boost,leak,bench",
    [string]$Only = "",
    [int]$Runs = 3,
    [int]$SettleSecs = 6
)
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\nexa-sql.exe" }
if (-not $Cli) { $Cli = Join-Path $root "target\release\nsql.exe" }
foreach ($d in @($HomeDir, $DataDir, $Out)) { if (-not (Test-Path -LiteralPath $d)) { New-Item -ItemType Directory -Path $d -Force | Out-Null } }
$report = Join-Path $Out "perf.txt"
$stageList = @($Stages.Split(",") | ForEach-Object { $_.Trim().ToLower() })   # 주의: param의 [string]$Stages와 대소문자만 다른 이름을 쓰면 배열이 문자열로 되돌아간다(PowerShell 변수는 대소문자 무시)
$lines = New-Object System.Collections.Generic.List[string]
function Say($s) { $lines.Add($s); Write-Output $s; Add-Content -LiteralPath $report -Value $s -Encoding UTF8 }
if (Test-Path -LiteralPath $report) { Clear-Content -LiteralPath $report }
function Median($xs) { $s = @($xs | Sort-Object); if ($s.Count -eq 0) { return 0 }; $s[[int][Math]::Floor(($s.Count - 1) / 2)] }

Add-Type @"
using System; using System.Runtime.InteropServices;
public class NxPerf { [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr h, uint flags); }
"@ -ErrorAction SilentlyContinue

$env:NSQL_HOME = $HomeDir
$env:NSQL_NO_ACTIVATE = "1"   # 시험 창이 사용자의 전경 포커스를 가져가지 않게

Say ("== win-perf-all " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss") + "  commit=" + (& git -C $root rev-parse --short HEAD 2>$null))
Say ("exe=" + $Exe + "  home=" + $HomeDir + "  data=" + $DataDir)

# ── 시험 자료(없으면 만든다) ─────────────────────────────────────────────────
$sql2m = Join-Path $DataDir "big2m.sql"
$rowsSql = Join-Path $DataDir "rows100k.sql"
$projFile = Join-Path $DataDir "perf.nsql-project"
$sqlite = Join-Path $HomeDir "local.sqlite"
if (-not (Test-Path -LiteralPath $sql2m)) {
    $sb = New-Object Text.StringBuilder
    $i = 0
    while ($sb.Length -lt 2MB) {
        $i++
        [void]$sb.AppendLine("-- statement $i")
        [void]$sb.AppendLine("SELECT a.id, a.name, a.amount, a.created_at FROM orders a WHERE a.project_cd = 'SEBANG' AND a.id > $i ORDER BY a.id;")
    }
    Set-Content -LiteralPath $sql2m -Value $sb.ToString() -Encoding UTF8
}
if (-not (Test-Path -LiteralPath $rowsSql)) {
    Set-Content -LiteralPath $rowsSql -Encoding UTF8 -Value @"
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 100000)
SELECT i AS id, 'name_' || i AS name, i * 1.5 AS amount, 'SEBANG' AS project_cd, 'memo ' || i AS memo FROM n;
"@
}
if (-not (Test-Path -LiteralPath $projFile)) {
    $dj = $DataDir.Replace('\', '/')
    Set-Content -LiteralPath $projFile -Encoding UTF8 -Value "{ `"version`": 1, `"folders`": [ { `"path`": `"$dj`" } ] }"
}
if (-not (Test-Path -LiteralPath (Join-Path $HomeDir "profiles"))) {
    & $Cli conn add Local ("sqlite:" + $sqlite) -d sqlite --no-prompt 2>&1 | Out-Null
}
# 결과 시나리오는 상한 때문에 200행에서 멈춘다 — 격리 홈에서만 올린다(향상 모드 강제 대상이 아니라 A/B에 영향 없음).
& $Cli config set grid.max_rows 100000 2>&1 | Out-Null

# ── 공통: 앱을 기동 명령으로 띄워 안정 뒤 자원을 재고 유휴 CPU를 본다 ────────
function Kill-App {
    # 앞 시나리오가 큰 결과를 들고 있으면 종료에 수 초가 걸린다 — 끝나기를 기다리지 않고 다음 표본을 띄우면
    # 새 프로세스가 "이미 죽은 프로세스"로 보이는 0 표본이 나온다(09-22에 실제로 겪음).
    $ps = @(Get-Process nexa-sql -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($root, "OrdinalIgnoreCase") })
    foreach ($q in $ps) { try { $q.Kill() } catch {} }
    foreach ($q in $ps) { try { $null = $q.WaitForExit(8000) } catch {} }
    Start-Sleep -Milliseconds 400
}
function Sample-Scenario([string]$tag, [string]$cmd, [int]$warmSecs = 8, [string]$argList = "Local") {
    Kill-App
    $env:NSQL_STARTUP_CMD = $cmd
    $p = if ($argList) { Start-Process -FilePath $Exe -ArgumentList $argList -WorkingDirectory (Split-Path $Exe) -PassThru }
    else { Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt 20000) { Start-Sleep -Milliseconds 20; $p.Refresh(); if ($p.MainWindowHandle -ne 0) { break } }
    Start-Sleep -Seconds $warmSecs
    $p.Refresh()
    $cpu0 = $p.TotalProcessorTime.TotalMilliseconds
    Start-Sleep -Seconds $SettleSecs
    $p.Refresh()
    $cpu1 = $p.TotalProcessorTime.TotalMilliseconds
    $gdi = 0; $usr = 0
    try { $gdi = [NxPerf]::GetGuiResources($p.Handle, 0); $usr = [NxPerf]::GetGuiResources($p.Handle, 1) } catch {}
    $r = [pscustomobject]@{
        tag = $tag; private = [math]::Round($p.PrivateMemorySize64 / 1MB, 2); ws = [math]::Round($p.WorkingSet64 / 1MB, 2)
        peak = [math]::Round($p.PeakWorkingSet64 / 1MB, 2); handles = $p.HandleCount; gdi = $gdi; user = $usr
        threads = $p.Threads.Count; idlecpu = [math]::Round($cpu1 - $cpu0, 0)
    }
    try { $p.Kill(); $null = $p.WaitForExit(8000) } catch {}
    Start-Sleep -Milliseconds 400
    $r
}
function Sample-Retry([string]$tag, [string]$cmd, [int]$warmSecs, [string]$argList) {
    $r = Sample-Scenario $tag $cmd $warmSecs $argList
    if ($r.private -le 0) { Start-Sleep -Seconds 2; $r = Sample-Scenario $tag $cmd $warmSecs $argList }
    $r
}
function Row($r) {
    "{0,-22} priv={1,7:N2} ws={2,7:N2} peak={3,7:N2} hnd={4,4} gdi={5,3} usr={6,3} thr={7,3} idleCPU={8,5} ms/{9}s" -f `
        $r.tag, $r.private, $r.ws, $r.peak, $r.handles, $r.gdi, $r.user, $r.threads, $r.idlecpu, $SettleSecs
}

# 규정 시나리오(docs/71 §3 C단계 · 26 §7-3의 구성을 그대로 이어받고 새 기능을 덧붙인다)
$scen = @(
    @{ tag = "1.startup"; cmd = ""; warm = 8; args = "" },
    @{ tag = "2.connected"; cmd = ""; warm = 8; args = "Local" },
    @{ tag = "3.windows4"; cmd = "view.log,view.txlog,view.sessions,edit.prefs"; warm = 8; args = "Local" },
    @{ tag = "4.script2m"; cmd = ("open:" + $sql2m); warm = 10; args = "Local" },
    @{ tag = "5.rows100k"; cmd = ("open:" + $rowsSql + ",@after:2000:run.all"); warm = 14; args = "Local" },
    @{ tag = "6.project"; cmd = ("project.load:" + $projFile + ",@after:1500:view.project"); warm = 10; args = "Local" },
    @{ tag = "7.extpanel"; cmd = "view.extensions"; warm = 8; args = "Local" }
)

if ($Only) {
    $keep = @($Only.Split(",") | ForEach-Object { $_.Trim() })
    $scen = @($scen | Where-Object { $t = $_.tag; ($keep | Where-Object { $t -like ("*" + $_ + "*") }).Count -gt 0 })
}

# ── A. 인벤토리 ──────────────────────────────────────────────────────────────
if ($stageList -contains "inventory") {
    Say ""
    Say "== A. inventory (용량 · 동적/정적 라이브러리 · 구성 파일 · 설정 키)"
    $invOut = Join-Path $Out "inventory.txt"
    & (Join-Path $PSScriptRoot "win-inventory.ps1") -Exe $Exe -Cli $Cli -HomeDir $HomeDir -Out $invOut | ForEach-Object { Say ("  " + $_) }
}

# ── B. 기동 ──────────────────────────────────────────────────────────────────
if ($stageList -contains "startup") {
    Say ""
    Say "== B. startup (창이 보이기까지 · 안정까지 CPU · Runs=$Runs)"
    & (Join-Path $PSScriptRoot "win-startup-probe.ps1") -Exe $Exe -HomeDir $HomeDir -ArgList "Local" -Runs $Runs -SettleSecs 3 2>&1 | ForEach-Object { Say ("  " + $_) }
}

# ── C. 시나리오 상주·CPU ─────────────────────────────────────────────────────
if ($stageList -contains "scenarios") {
    Say ""
    Say "== C. scenarios (상주 · 핸들/GDI/USER · 스레드 · 유휴 CPU)"
    foreach ($s in $scen) { Say ("  " + (Row (Sample-Retry $s.tag $s.cmd $s.warm $s.args))) }
}

# ── D. 향상 모드 A/B(같은 시나리오를 off/on으로 번갈아) ──────────────────────
if ($stageList -contains "boost") {
    Say ""
    Say "== D. perf.boost A/B (같은 시나리오를 off/on 번갈아 · 차이가 곧 향상 모드의 효과)"
    $res = @{}
    foreach ($s in $scen) {
        foreach ($mode in @("off", "on")) {
            & $Cli config set perf.boost $mode 2>&1 | Out-Null
            $r = Sample-Retry ($s.tag + "/" + $mode) $s.cmd $s.warm $s.args
            $res[$s.tag + "/" + $mode] = $r
            Say ("  " + (Row $r))
        }
    }
    & $Cli config set perf.boost off 2>&1 | Out-Null
    Say "  -- delta (on - off)"
    foreach ($s in $scen) {
        $a = $res[$s.tag + "/off"]; $b = $res[$s.tag + "/on"]
        if ($a -and $b) {
            Say ("  {0,-16} priv {1,7:N2} -> {2,7:N2} ({3,7:N2})   thr {4} -> {5}   hnd {6} -> {7}   idleCPU {8} -> {9} ms" -f `
                    $s.tag, $a.private, $b.private, ($b.private - $a.private), $a.threads, $b.threads, $a.handles, $b.handles, $a.idlecpu, $b.idlecpu)
        }
    }
}

# ── E. 릭 주기 ───────────────────────────────────────────────────────────────
if ($stageList -contains "leak") {
    Say ""
    Say "== E. leak cycles (뒤 절반의 기울기 = MB/주기)"
    $cycles = @(
        @{ tag = "L1.file2m"; first = ""; cmd = ("open:" + $sql2m + ";file.close_tab"); n = 10; period = 4000 },
        @{ tag = "L2.rows100k"; first = ("open:" + $rowsSql); cmd = "run.all"; n = 10; period = 6000 },
        @{ tag = "L3.logwin"; first = ""; cmd = "view.log;view.log"; n = 10; period = 2000 },
        @{ tag = "L4.project"; first = ("project.load:" + $projFile); cmd = "view.project;view.project"; n = 10; period = 2500 }
    )
    foreach ($c in $cycles) {
        Say ("  -- " + $c.tag)
        & (Join-Path $PSScriptRoot "win-leak-cycle.ps1") -Exe $Exe -HomeDir $HomeDir -First $c.first -CycleCmds $c.cmd `
            -Cycles $c.n -PeriodMs $c.period -ArgList "Local" -Tag $c.tag 2>&1 | ForEach-Object { Say ("    " + $_) }
    }
}

# ── F. 벤치(편집기 · 되돌리기 · 변수) ────────────────────────────────────────
if ($stageList -contains "bench") {
    Say ""
    Say "== F. bench (nexa-ui bench_editor/bench_undo · nsql-run bench_vars)"
    $ui = Join-Path (Split-Path -Parent $root) "nexa-ui"
    if (Test-Path -LiteralPath $ui) {
        foreach ($n in @(700000, 40000)) {
            & cargo run --release -p nexa-ctl --example bench_editor --manifest-path (Join-Path $ui "Cargo.toml") -- $n all 2>&1 |
            Select-String -Pattern "ms|us" | ForEach-Object { Say ("  editor[$n] " + $_.Line.Trim()) }
        }
        & cargo run --release -p nexa-ctl --example bench_undo --manifest-path (Join-Path $ui "Cargo.toml") 2>&1 |
        Select-String -Pattern "ms" | ForEach-Object { Say ("  undo " + $_.Line.Trim()) }
    }
    else { Say "  (../nexa-ui not found - skipped)" }
    & cargo run --release -p nsql-run --example bench_vars 2>&1 |
    Select-String -Pattern "us|ms" | ForEach-Object { Say ("  vars " + $_.Line.Trim()) }
}

Say ""
Say ("-> " + $report)
