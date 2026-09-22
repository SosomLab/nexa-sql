# win-mem-reclaim.ps1 — **메모리 회수 시험 C-2(R1~R6)**(docs/71 §C-2 · 사용자 09-22 "편집기·결과 그리드·대용량·다중 결과 그리드·커서 회수 시험") —
#   항목마다 앱을 격리 홈으로 띄우고 `@after:<ms>:<명령>` 시차 명령으로 ① 기준선(기동+접속) ② 올림 ③ 놓음(즉시) ④ memtrim 뒤
#   네 점의 Private/WS를 찍는다(입력 주입 없음 · docs/61 §4). memtrim 유휴 주기는 격리 설정 `mem.trim_secs`로 짧게(기본 300초 → 15초).
#   R5(Oracle/PG refcursor)는 실서버·자격이 필요해 이 스크립트 밖(사용자 환경에서 71 §C-2 표대로 손으로).
#
# 사용:
#   pwsh -NoProfile -File scripts/win-mem-reclaim.ps1 -HomeDir C:\tmp\nsql-home -DataDir C:\tmp\nsql-data -Out target\perf-win
#   -Only R1,R3  · -Exe 기본 Release · 결과 = <Out>\reclaim.txt
param(
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [Parameter(Mandatory = $true)][string]$DataDir,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Exe = "",
    [string]$Cli = "",
    [string]$Only = "",
    [int]$TrimSecs = 15
)
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\nexa-sql.exe" }
if (-not $Cli) { $Cli = Join-Path $root "target\release\nsql.exe" }
foreach ($d in @($HomeDir, $DataDir, $Out)) { if (-not (Test-Path -LiteralPath $d)) { New-Item -ItemType Directory -Path $d -Force | Out-Null } }
$report = Join-Path $Out "reclaim.txt"
if (Test-Path -LiteralPath $report) { Clear-Content -LiteralPath $report }
function Say($s) { Write-Output $s; Add-Content -LiteralPath $report -Value $s -Encoding UTF8 }

$env:NSQL_HOME = $HomeDir
$env:NSQL_NO_ACTIVATE = "1"
$sqlite = Join-Path $HomeDir "local.sqlite"
if (-not (Test-Path -LiteralPath (Join-Path $HomeDir "profiles"))) {
    & $Cli conn add Local ("sqlite:" + $sqlite) -d sqlite --no-prompt 2>&1 | Out-Null
}
# 격리 설정: 결과 탭 바 항상(× 클릭으로 닫기) · 데모 안내 끔 · 10만 행 전부 · 큰 파일 팝업 끔 · memtrim 유휴 주기 짧게.
# grid.max_rows = 10만 행을 다 가져오게(기본 200 · perf-all과 같음) · file.large_ask_mb=0 = 큰 파일 열기 선택 팝업을 묻지 않음(기동 명령은 팔레트를 못 고른다).
Set-Content -LiteralPath (Join-Path $HomeDir "settings.conf") -Encoding UTF8 -Value ("lang=en`ngrid.result_tabbar_single=on`ndemo.prompted=on`ngrid.max_rows=100000`nfile.large_ask_mb=0`nmem.trim_secs=" + $TrimSecs + "`n")

# ── 시험 자료 ─────────────────────────────────────────────────────────────
$sql2m = Join-Path $DataDir "big2m.sql"
$rowsSql = Join-Path $DataDir "rows100k.sql"
$sql65m = Join-Path $DataDir "big65m.sql"
$eightSql = Join-Path $DataDir "rows8x10k.sql"
if (-not (Test-Path -LiteralPath $sql2m)) {
    $sb = New-Object Text.StringBuilder; $i = 0
    while ($sb.Length -lt 2MB) { $i++; [void]$sb.AppendLine("-- statement $i"); [void]$sb.AppendLine("SELECT a.id, a.name, a.amount, a.created_at FROM orders a WHERE a.project_cd = 'SEBANG' AND a.id > $i ORDER BY a.id;") }
    Set-Content -LiteralPath $sql2m -Value $sb.ToString() -Encoding UTF8
}
if (-not (Test-Path -LiteralPath $rowsSql)) {
    Set-Content -LiteralPath $rowsSql -Encoding UTF8 -Value @"
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 100000)
SELECT i AS id, 'name_' || i AS name, i * 1.5 AS amount, 'SEBANG' AS project_cd, 'memo ' || i AS memo FROM n;
"@
}
if (-not (Test-Path -LiteralPath $sql65m)) {
    # 65 MB = 2 MB 본문을 33번 이어 붙인다(줄 구조는 같다 · 59 §5의 84차 표본과 같은 크기).
    $body = Get-Content -LiteralPath $sql2m -Raw
    $fs = [System.IO.File]::Create($sql65m); $sw = New-Object System.IO.StreamWriter($fs, [System.Text.Encoding]::UTF8)
    for ($k = 0; $k -lt 33; $k++) { $sw.Write($body) }
    $sw.Close()
}
if (-not (Test-Path -LiteralPath $eightSql)) {
    $sb = New-Object Text.StringBuilder
    for ($k = 1; $k -le 8; $k++) {
        [void]$sb.AppendLine("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 10000)")
        [void]$sb.AppendLine("SELECT i AS id, 'q$k' || i AS name, i * 1.5 AS amount, 'SEBANG' AS project_cd, 'memo ' || i AS memo FROM n;")
    }
    Set-Content -LiteralPath $eightSql -Value $sb.ToString() -Encoding UTF8
}

function Sample($p) {
    try { $p.Refresh(); return @{ priv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2); ws = [math]::Round($p.WorkingSet64 / 1MB, 1); hnd = $p.HandleCount; thr = $p.Threads.Count } }
    catch { return @{ priv = -1; ws = -1; hnd = -1; thr = -1 } }
}

# 한 항목 = 시차 명령 문자열 + 표본 시각 넷(ms · 기동 기준) → 네 점을 찍고 허용치와 비교.
function Run-Item {
    param([string]$Id, [string]$Title, [string]$Cmd, [int]$TBase, [int]$TRaise, [int]$TRelease, [double]$Allow, [string]$Note = "")
    if ($Only -and (($Only.Split(",") | ForEach-Object { $_.Trim() }) -notcontains $Id)) { return }
    $TTrim = $TRelease + ($TrimSecs + 6) * 1000
    $env:NSQL_STARTUP_CMD = $Cmd
    $p = Start-Process -FilePath $Exe -ArgumentList "Local" -WorkingDirectory (Split-Path $Exe) -PassThru
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $pts = @{}
    foreach ($pair in @(@("base", $TBase), @("raise", $TRaise), @("release", $TRelease), @("trim", $TTrim))) {
        $name = $pair[0]; $at = $pair[1]
        $wait = $at - $sw.ElapsedMilliseconds
        if ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
        $pts[$name] = Sample $p
    }
    try { $p.Kill() } catch {}
    try { $p.WaitForExit(8000) | Out-Null } catch {}
    $env:NSQL_STARTUP_CMD = $null
    $b = $pts["base"]; $r = $pts["raise"]; $l = $pts["release"]; $t = $pts["trim"]
    $left = [math]::Round($t.priv - $b.priv, 2)
    $verdict = if ($left -le $Allow) { "OK" } else { "OVER(+$left > +$Allow)" }
    Say ("| {0} | {1} | {2} | {3} | {4} | {5} | {6} | {7} | +{8} | {9} | {10}" -f $Id, $Title, $b.priv, $r.priv, $l.priv, $t.priv, ("{0}/{1}" -f $b.hnd, $t.hnd), ("{0}/{1}" -f $b.thr, $t.thr), $Allow, $verdict, $Note)
}

Say ("== win-mem-reclaim " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss") + "  commit=" + (& git -C $root rev-parse --short HEAD 2>$null) + "  trim_secs=" + $TrimSecs)
Say "| # | 대상 | ① 기준 MB | ② 올림 | ③ 놓음 | ④ trim 뒤 | 핸들 ①/④ | 스레드 ①/④ | 허용 | 판정 | 비고"
Say "|---|---|---:|---:|---:|---:|---|---|---|---|---|"

# 결과 탭 × = 결과 탭 바 첫 탭의 닫기 상자(클라이언트 377/507 · 1375×945 · 배율 1.0 · 활성 탭이 첫 자리일 때).
$closeRes = "ui.click:377/507"

# R1 편집기 탭 2 MB: 올림 = open · 놓음 = file.close_tab.
Run-Item -Id R1 -Title "편집기 탭(2 MB)" -Cmd ("@after:6000:open:" + $sql2m + ",@after:14000:file.close_tab") -TBase 5500 -TRaise 13500 -TRelease 16000 -Allow 2
# R2 결과 그리드 10만 행: 올림 = 실행 · 놓음 = 결과 탭 ×.
Run-Item -Id R2 -Title "결과 그리드(10만 행 × 5열)" -Cmd ("open:" + $rowsSql + ",@after:6000:run.all,@after:16000:" + $closeRes) -TBase 5500 -TRaise 15500 -TRelease 18500 -Allow 3
# R3 대용량 65 MB: 올림 = open(적재 완료 대기 20 s) · 놓음 = file.close_tab.
Run-Item -Id R3 -Title "대용량 파일(65 MB · 닫기)" -Cmd ("@after:6000:open:" + $sql65m + ",@after:28000:file.close_tab") -TBase 5500 -TRaise 27000 -TRelease 31000 -Allow 8
# R3b 대용량 적재 중 취소(Esc = file.load_cancel · 열고 150 ms 뒤).
Run-Item -Id R3b -Title "대용량 파일(65 MB · 적재 중 취소)" -Cmd ("@after:6000:open:" + $sql65m + ",@after:6150:file.load_cancel") -TBase 5500 -TRaise 6100 -TRelease 12000 -Allow 8 -Note "②는 취소 직전 표본(적재 중)"
# R4 다중 결과 그리드 8탭 × 1만 행: 올림 = 8문장 실행 · 놓음 = × 8번(400 ms 간격).
$closes = ""; for ($k = 0; $k -lt 8; $k++) { $closes += (",@after:" + (17000 + $k * 400) + ":" + $closeRes) }
Run-Item -Id R4 -Title "다중 결과 그리드(8탭 × 1만 행)" -Cmd ("open:" + $eightSql + ",@after:6000:run.all" + $closes) -TBase 5500 -TRaise 16000 -TRelease 22000 -Allow 3
# R6 결과 + 편집기 번갈아 5회: [2 MB 열기 · 10만 행 실행 · 둘 다 닫기] × 5.
$seq = ""; $t = 6000
for ($k = 0; $k -lt 5; $k++) {
    $seq += (",@after:" + $t + ":open:" + $sql2m)
    $seq += (",@after:" + ($t + 3000) + ":open:" + $rowsSql)
    $seq += (",@after:" + ($t + 3500) + ":run.all")
    $seq += (",@after:" + ($t + 9000) + ":file.close_tab")
    $seq += (",@after:" + ($t + 9500) + ":file.close_tab")
    $seq += (",@after:" + ($t + 10000) + ":" + $closeRes)
    $t += 11000
}
Run-Item -Id R6 -Title "결과 + 편집기 동시(R1+R2 × 5)" -Cmd $seq.TrimStart(",") -TBase 5500 -TRaise ($t - 11000 + 8500) -TRelease ($t + 1500) -Allow 4
Say ("== done " + (Get-Date -Format "HH:mm:ss") + "  (R5 refcursor = 실서버 필요 · 여기서 안 잼)")
