# 실서버 SSH 터널 시험(161차 · 10-28 · docs/125) — 사용자의 로컬 ssh 설정으로 DB에 붙는지 세 단계로 본다.
#   1) ssh 선검사: BatchMode로 점프 대상에 로그인되는지(키·known_hosts·ProxyCommand=cloudflared 포함) — 여기서 막히면 터널도 막힌다.
#   2) 허브 시험: cargo test … ssh_tunnel_real_server (NSQL_SSH_URL 또는 NSQL_SSH_PROFILE).
#   3) CLI 왕복: nsql run -c <대상> 으로 확인 조회 1건(사용자가 실제로 쓰는 경로).
# 사용:
#   scripts/win-ssh-tunnel-test.ps1 -Target "oracle://BISCM:…@127.0.0.1:1521/BISCM?ssh=oracle@192.168.0.58&driver=thin"
#   scripts/win-ssh-tunnel-test.ps1 -ProfileName biscm-ssh      # 볼트 프로필(비밀번호를 명령줄에 안 남긴다)
#   -Jump user@host[:port] 를 주면 1)에서 그 대상으로 로그인 검사(없으면 Target의 ssh= 에서 뽑는다).
#   -AddKnownHost 는 점프 호스트의 공개 키를 ~/.ssh/known_hosts에 추가한다(사용자 파일 수정 = 명시적으로 켤 때만).
param(
    [string]$Target = "",
    [Alias("Profile")][string]$ProfileName = "",
    [string]$Jump = "",
    [switch]$AddKnownHost,
    [switch]$SkipCargo
)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
if (-not $Target -and -not $ProfileName) { Write-Host "Target 또는 ProfileName 하나는 필요"; exit 2 }

# ── 점프 대상 추출(Target의 ?ssh= · 프로필은 nsql conn show)
if (-not $Jump) {
    if ($Target -match '[?&]ssh=([^&]+)') { $Jump = [uri]::UnescapeDataString($Matches[1]) }
    elseif ($ProfileName) {
        $show = & target/debug/nsql.exe conn show $ProfileName 2>&1 | Out-String
        if ($show -match 'via ssh (\S+)') { $Jump = $Matches[1].Trim() }
    }
}
if (-not $Jump) { Write-Host "점프 대상(ssh=)을 찾지 못함"; exit 2 }
$jumpHost = $Jump; $jumpPort = ""
if ($Jump -match '^(.*):(\d+)$') { $jumpHost = $Matches[1]; $jumpPort = $Matches[2] }
$hostOnly = ($jumpHost -split '@')[-1]
Write-Host "== 1) ssh 선검사: $Jump"
if ($AddKnownHost) {
    $scan = if ($jumpPort) { ssh-keyscan -p $jumpPort -T 5 $hostOnly 2>$null } else { ssh-keyscan -T 5 $hostOnly 2>$null }
    if ($scan) { Add-Content -Path "$HOME/.ssh/known_hosts" -Value $scan; Write-Host "   known_hosts에 $hostOnly 추가" }
    else { Write-Host "   ssh-keyscan 실패(ProxyCommand 대상은 터미널에서 한 번 접속해 등록)" }
}
$sshArgs = @("-o", "BatchMode=yes", "-o", "ConnectTimeout=10")
if ($jumpPort) { $sshArgs += @("-p", $jumpPort) }
$sshArgs += @($jumpHost, "echo", "ssh-ok")
$pre = & ssh @sshArgs 2>&1 | Out-String
if ($pre -match "ssh-ok") { Write-Host "   로그인 OK" }
else {
    Write-Host "   로그인 실패 — 터널도 같은 이유로 막힌다. ssh 원문:"
    Write-Host ($pre.Trim() -replace '(?m)^', '   | ')
    Write-Host "   힌트: 'Permission denied' = 공개 키 미등록(ssh-copy-id 또는 서버 authorized_keys) · 'Host key verification failed' = -AddKnownHost 또는 터미널에서 한 번 접속"
    exit 1
}

# ── 2) 허브 시험
if (-not $SkipCargo) {
    Write-Host "== 2) cargo test ssh_tunnel_real_server"
    if ($Target) { $env:NSQL_SSH_URL = $Target; Remove-Item Env:NSQL_SSH_PROFILE -ErrorAction SilentlyContinue }
    else { $env:NSQL_SSH_PROFILE = $ProfileName; Remove-Item Env:NSQL_SSH_URL -ErrorAction SilentlyContinue }
    & cargo test -p nsql-drivers --test integration ssh_tunnel_real_server -- --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { Write-Host "   허브 시험 실패"; exit 1 }
}

# ── 3) CLI 왕복
Write-Host "== 3) nsql run 왕복"
$conn = if ($Target) { $Target } else { $ProfileName }
$sql = "SELECT 1 AS via_ssh FROM dual;"
if ($conn -match '^(postgres|mysql|mariadb|mssql|sqlserver)://') { $sql = "SELECT 1 AS via_ssh;" }
$out = $sql | & target/debug/nsql.exe run -c $conn --no-prompt -f csv - 2>&1 | Out-String
Write-Host ($out.Trim() -replace '(?m)^', '   | ')
if ($LASTEXITCODE -ne 0) { Write-Host "   CLI 왕복 실패(종료 $LASTEXITCODE)"; exit 1 }
Write-Host "== 전부 통과"
