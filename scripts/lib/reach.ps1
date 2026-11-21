# reach.ps1 — 통합 시험 프로필의 TCP 도달(verify.sh 통합 단계의 문턱 · 11-21 D-491). 프로필 이름은 인자로 받는다(저장소에 실제 이름을 쓰지 않는다).
#   주소는 출력하지 않는다 · 결과 줄 = "<프로필> OK|FAIL" · Git Bash /dev/tcp는 거짓 음성이라 TcpClient로(메모리 tcp-reach-check-powershell).
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Profiles)
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exe = Join-Path $root 'target\debug\nsql.exe'
if (-not (Test-Path $exe)) { $exe = 'nsql' }
$rows = & $exe conn list 2>$null
foreach ($name in $Profiles) {
  $line = $rows | Where-Object { $_ -match "^$([regex]::Escape($name))\s" } | Select-Object -First 1
  if ($line -match '@\[?([A-Za-z0-9\.\-]+)\]?:(\d+)') {
    $c = New-Object System.Net.Sockets.TcpClient
    try { $ok = $c.ConnectAsync($Matches[1], [int]$Matches[2]).Wait(3000) } catch { $ok = $false } finally { $c.Dispose() }
    "$name $(if ($ok) { 'OK' } else { 'FAIL' })"
  } else { "$name FAIL(주소 없음)" }
}
