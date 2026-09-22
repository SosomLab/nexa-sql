# win-inventory.ps1 — **배포 인벤토리**(docs/71 §3 A단계 · 입력 주입 없음): 실행 파일 용량·PE 섹션 · **동적 라이브러리**(정적 import +
#   지연 import) · **정적 라이브러리**(링크된 crate 수) · **구성 파일**(격리 홈을 한 번 돌려 실제로 생기는 것) · 설정 키 수 ·
#   기동 뒤 스레드/핸들을 한 번에 찍는다. 성능 점검의 첫 단계 — 용량·의존이 늘었는지를 수치로 먼저 본다.
#
# 사용:
#   pwsh -NoProfile -File scripts/win-inventory.ps1 -HomeDir C:\tmp\nsql-inv
#   pwsh -NoProfile -File scripts/win-inventory.ps1 -HomeDir C:\tmp\nsql-inv -Out target\inventory.txt -SkipRun
#
# 읽는 법: `dynamic(import)`은 프로세스가 **기동 때 무조건 올리는** DLL — 늘면 기동이 느려진다.
#          `dynamic(delay)`는 처음 쓸 때 올린다(지금 0 = 지연 로드를 쓰지 않는다 · Oracle OCI는 ODPI-C가 `LoadLibrary`로 직접 연다).
#          `crates`는 정적으로 링크되는 Rust 패키지 수 = 용량·컴파일 시간의 원천(DR-3 원장과 대조).
param(
    [string]$Exe = "",
    [string]$Cli = "",
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [string]$Out = "",
    [switch]$SkipRun,
    [switch]$SkipCargo
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\nexa-sql.exe" }
if (-not $Cli) { $Cli = Join-Path $root "target\release\nsql.exe" }
if ($HomeDir -match '\$') { throw "HomeDir looks wrong: '$HomeDir'" }
if (-not (Test-Path -LiteralPath $HomeDir -PathType Container)) { New-Item -ItemType Directory -Path $HomeDir -Force | Out-Null }
$lines = New-Object System.Collections.Generic.List[string]
function Say($s) { $lines.Add($s); Write-Output $s }

# ── PE 읽기(의존 0 · dumpbin 없이) ────────────────────────────────────────────
function Get-PeInfo([string]$path) {
    $b = [IO.File]::ReadAllBytes($path)
    $u16 = { param($o) [BitConverter]::ToUInt16($b, $o) }
    $u32 = { param($o) [BitConverter]::ToUInt32($b, $o) }
    $pe = & $u32 0x3c
    $nSec = & $u16 ($pe + 6)
    $optOff = $pe + 24
    $is64 = ((& $u16 $optOff) -eq 0x20b)
    $ddOff = if ($is64) { $optOff + 112 } else { $optOff + 96 }
    $secOff = $optOff + (& $u16 ($pe + 20))
    $secs = @()
    for ($i = 0; $i -lt $nSec; $i++) {
        $s = $secOff + $i * 40
        $name = ([Text.Encoding]::ASCII.GetString($b, $s, 8)).Trim([char]0)
        $secs += [pscustomobject]@{ Name = $name; VA = (& $u32 ($s + 12)); Size = (& $u32 ($s + 8)); Raw = (& $u32 ($s + 20)); RawSize = (& $u32 ($s + 16)) }
    }
    function RVA2Off($rva) {
        foreach ($s in $secs) { if ($rva -ge $s.VA -and $rva -lt ($s.VA + $s.Size)) { return $s.Raw + ($rva - $s.VA) } }
        return 0
    }
    function ReadStr($off) {
        $sb = New-Object Text.StringBuilder
        while ($b[$off] -ne 0) { [void]$sb.Append([char]$b[$off]); $off++ }
        $sb.ToString()
    }
    $imports = @(); $delay = @()
    foreach ($d in @(1, 13)) {
        $rva = & $u32 ($ddOff + $d * 8)
        if ($rva -eq 0) { continue }
        $off = RVA2Off $rva
        if ($off -eq 0) { continue }
        $stride = if ($d -eq 1) { 20 } else { 32 }
        $nameField = if ($d -eq 1) { 12 } else { 4 }
        while ($true) {
            $nr = & $u32 ($off + $nameField)
            if ($nr -eq 0) { break }
            $o = RVA2Off $nr
            if ($o -eq 0) { break }
            $n = ReadStr $o
            if ($n -eq '') { break }
            if ($d -eq 1) { $imports += $n } else { $delay += $n }
            $off += $stride
        }
    }
    [pscustomobject]@{
        Size    = $b.Length
        Is64    = $is64
        Secs    = $secs
        Imports = ($imports | Sort-Object -Unique)
        Delay   = ($delay | Sort-Object -Unique)
    }
}

Say ("== inventory " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss"))
$commit = (& git -C $root rev-parse --short HEAD 2>$null)
Say ("commit=" + $commit + "  dirty=" + (((& git -C $root status --porcelain 2>$null) | Measure-Object -Line).Lines))

# ── 1. 용량 · 섹션 · 동적 라이브러리 ──────────────────────────────────────────
foreach ($pair in @(@{ n = "gui"; p = $Exe }, @{ n = "cli"; p = $Cli })) {
    if (-not (Test-Path -LiteralPath $pair.p)) { Say ($pair.n + ": MISSING " + $pair.p); continue }
    $pe = Get-PeInfo $pair.p
    Say ""
    Say ("-- " + $pair.n + " " + (Split-Path -Leaf $pair.p))
    Say ("size          = {0:N0} bytes ({1:N2} MiB)  arch={2}" -f $pe.Size, ($pe.Size / 1MB), $(if ($pe.Is64) { "x64" } else { "x86" }))
    foreach ($s in ($pe.Secs | Sort-Object -Property RawSize -Descending)) {
        if ($s.RawSize -gt 0) { Say ("  section {0,-8} {1,12:N0} bytes" -f $s.Name, $s.RawSize) }
    }
    Say ("dynamic(import) = " + $pe.Imports.Count + "  " + ($pe.Imports -join " "))
    Say ("dynamic(delay)  = " + $pe.Delay.Count + "  " + ($pe.Delay -join " "))
}

# ── 2. 정적 라이브러리(링크되는 crate) ────────────────────────────────────────
if (-not $SkipCargo) {
    foreach ($p in @("nexa-sql", "nsql-cli")) {
        $t = & cargo tree -p $p --edges normal --prefix none 2>$null
        if ($LASTEXITCODE -eq 0 -and $t) {
            $names = $t | ForEach-Object { ($_ -split '\s+')[0] } | Where-Object { $_ } | Sort-Object -Unique
            $ws = $names | Where-Object { $_ -like "nsql-*" -or $_ -like "nexa-*" }
            Say ("crates " + $p + " = " + $names.Count + " (workspace/sibling " + $ws.Count + ", external " + ($names.Count - $ws.Count) + ")")
        }
    }
}

# ── 3. 설정 키 · 구성 파일 ────────────────────────────────────────────────────
$env:NSQL_HOME = $HomeDir
$env:NSQL_NO_ACTIVATE = "1"
if (Test-Path -LiteralPath $Cli) {
    $keys = & $Cli config list all 2>$null
    # 출력 = 카테고리 머리줄 `[General ▸ Log]` + 키 줄 `키  값  (default)  [형식]  설명`
    $n = ($keys | Where-Object { $_ -match '^[a-z][a-z0-9_]*\.[a-z0-9_.]+\s' } | Measure-Object).Count
    $cats = ($keys | Where-Object { $_ -match '^\[' } | Measure-Object).Count
    Say ("settings keys = " + $n + " in " + $cats + " categories (nsql config list all)")
    $perf = & $Cli config list perf 2>$null
    $np = ($perf | Where-Object { $_ -match '^[a-z][a-z0-9_]*\.[a-z0-9_.]+\s' } | Measure-Object).Count
    if ($np -gt 0) { Say ("load-source keys (perf ledger) = " + $np + " (nsql config list perf)") }
}
if (-not $SkipRun -and (Test-Path -LiteralPath $Exe)) {
    Get-Process nexa-sql -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$root*" } | Stop-Process -Force
    Start-Sleep -Milliseconds 300
    $env:NSQL_STARTUP_CMD = ""
    $p = Start-Process -FilePath $Exe -ArgumentList "Local" -WorkingDirectory (Split-Path $Exe) -PassThru
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt 20000) { Start-Sleep -Milliseconds 20; $p.Refresh(); if ($p.MainWindowHandle -ne 0) { break } }
    Start-Sleep -Seconds 4
    $p.Refresh()
    Say ("startup threads = " + $p.Threads.Count + "  handles = " + $p.HandleCount + "  private = {0:N2} MB" -f ($p.PrivateMemorySize64 / 1MB))
    # 지금 이 프로세스가 실제로 올려 둔 모듈(= 기동 import + 런타임 LoadLibrary)
    try {
        $mods = @($p.Modules | ForEach-Object { Split-Path -Leaf $_.FileName } | Sort-Object -Unique)
        Say ("loaded modules  = " + $mods.Count)
        Say ("  " + ($mods -join " "))
    }
    catch { Say "loaded modules  = (denied)" }
    Stop-Process -Id $p.Id -Force
    Start-Sleep -Milliseconds 500
}
$files = @(Get-ChildItem -LiteralPath $HomeDir -Recurse -File -Force -ErrorAction SilentlyContinue)
Say ("config files in NSQL_HOME = " + $files.Count + "  total {0:N0} bytes" -f (($files | Measure-Object -Property Length -Sum).Sum))
foreach ($f in ($files | Sort-Object FullName)) {
    Say ("  {0,-42} {1,10:N0}" -f $f.FullName.Substring($HomeDir.Length).TrimStart('\'), $f.Length)
}
if ($Out) {
    $dir = Split-Path -Parent $Out
    if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    Set-Content -LiteralPath $Out -Value $lines -Encoding UTF8
    Write-Output ("-> " + $Out)
}
