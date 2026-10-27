# win-restart-debug.ps1 — "빌드 및 재시작"의 Windows판(10-27 · 140차 win · T-436 · mac-restart-debug.sh / linux-restart-debug.sh와 같은 절차):
#   Debug(full · 내장 드라이버) 빌드 → 이 저장소 target\debug\nexa-sql.exe 종료(누가 띄웠든 · docs/61 §2-4 예외) → 실제 설정으로 재기동.
#   -Release = Release도 함께 = ★ lite 판(D-363 · docs/61 §3): target\lite에 `--no-default-features --features …oracle-oci`로 빌드해
#   target\release로 복사 · 종전 full 판은 *.full로 보존(판별은 빌드 전 · mac S1 교훈) · 기본 `cargo build --release`는 쓰지 않는다.
#   빌드 실패($LASTEXITCODE ≠ 0)면 복사·재기동을 하지 않는다(S2 교훈). ★ Windows는 실행 중인 exe가 링크를 막으므로 **Debug 인스턴스는
#   빌드 전에** 끈다(mac/linux는 빌드 뒤 · 협업 10-27 관찰 = PID 16440이 살아 있어 `failed to remove file …nexa-sql.exe` exit 101).
# 사용:  pwsh scripts/win-restart-debug.ps1              # Debug 빌드 + 재기동
#        pwsh scripts/win-restart-debug.ps1 -NoBuild     # 재기동만
#        pwsh scripts/win-restart-debug.ps1 -Release     # Debug + Release lite
#        -PidFile <파일>(기본 = $env:TEMP\nexa-sql-debug.pid) · -NsqlHome <폴더>(NSQL_HOME 격리 · 기본 = 실제 설정 ·
#        이름이 $HOME 자동 변수와 겹치면 param 바인딩에서 실패한다 · 협업 10-27)
#   띄운 창은 NSQL_NO_ACTIVATE=1(포커스를 빼앗지 않는다 · 61 §2-4) — 사용자가 작업 표시줄에서 올린다.
param(
    [switch]$NoBuild,
    [switch]$Release,
    [string]$PidFile = (Join-Path $env:TEMP 'nexa-sql-debug.pid'),
    [string]$NsqlHome = ''
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$bins = @('nexa-sql.exe', 'nsql.exe')

function Build-Filtered([string[]]$cargoArgs, [string]$tag) {
    # cargo 출력은 warning/error/Finished 줄만 · 종료 코드는 cargo 것($LASTEXITCODE는 파이프 안 네이티브 명령 것이 남는다).
    & cargo @cargoArgs 2>&1 | Where-Object { $_ -match '^(warning|error)|Finished' } | Select-Object -First 5 | ForEach-Object { "[$tag] $_" }
    if ($LASTEXITCODE -ne 0) { throw "[$tag] cargo build 실패(exit $LASTEXITCODE) — 종료·복사·재기동하지 않음" }
}

function Stop-RepoInstances([string]$sub) {
    # 이 저장소 target\<sub>\nexa-sql.exe만(개발 중 누가 띄웠든 · 61 §2-4) · 다른 경로의 nexa-sql은 건드리지 않는다.
    $dir = (Join-Path $root "target\$sub").ToLowerInvariant()
    $victims = Get-Process -Name 'nexa-sql' -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and $_.Path.ToLowerInvariant().StartsWith($dir)
    }
    foreach ($p in $victims) {
        Write-Output "[stop] $sub PID $($p.Id)"
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    }
    if ($victims) { Start-Sleep -Milliseconds 800 }
}

if (-not $NoBuild) {
    Stop-RepoInstances 'debug'   # 실행 중인 exe는 링크를 막는다(Windows) — 빌드가 실패하면 재기동 없이 멈춘다(옛 exe는 그대로 남는다)
    Build-Filtered @('build', '--workspace') 'debug'
    if ($Release) {
        # ★ full 판 판별은 빌드 전: 마지막으로 복사한 lite의 해시를 부속 파일 `<exe>.lite-sha`에 적어 두고, 지금 target\release가 그 해시와
        #   다르면 full로 보고 *.full 보존(부속 파일이 없으면(첫 실행) 직전 target\lite\release와 비교 · 3-OS 같은 규칙 · 10-27).
        $isFull = @{}
        foreach ($b in $bins) {
            $cur = Join-Path $root "target\release\$b"; $lite = Join-Path $root "target\lite\release\$b"
            if (Test-Path $cur) {
                $curHash = (Get-FileHash $cur).Hash
                $isFull[$b] = if (Test-Path "$cur.lite-sha") {
                    (Get-Content "$cur.lite-sha" -Raw).Trim() -ne $curHash
                } else {
                    (-not (Test-Path $lite)) -or ($curHash -ne (Get-FileHash $lite).Hash)
                }
            }
        }
        $env:CARGO_TARGET_DIR = Join-Path $root 'target\lite'
        try {
            Build-Filtered @('build', '--release', '-p', 'nexa-sql', '-p', 'nsql-cli', '--no-default-features',
                '--features', 'nexa-sql/driver-oracle-oci,nsql-cli/driver-oracle-oci') 'release-lite'
        } finally { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
        Stop-RepoInstances 'release'   # 실행 중인 exe는 복사를 막는다
        New-Item -ItemType Directory -Force (Join-Path $root 'target\release') | Out-Null
        foreach ($b in $bins) {
            $cur = Join-Path $root "target\release\$b"; $lite = Join-Path $root "target\lite\release\$b"
            if (-not (Test-Path $lite)) { continue }
            if ($isFull[$b]) { Copy-Item $cur "$cur.full" -Force; Write-Output "[release] $b → $b.full 보존" }
            # 덮어쓰기가 아니라 삭제 뒤 복사: cargo 산출물은 deps\ 쪽과 하드링크라 제자리 덮기가 그쪽까지 바꾼다(협업 10-26 관찰).
            Remove-Item $cur -Force -ErrorAction SilentlyContinue
            Copy-Item $lite $cur
            Set-Content -Path "$cur.lite-sha" -Value (Get-FileHash $cur).Hash
        }
        Write-Output "[release] lite = target\release\nexa-sql.exe · nsql.exe (full 판은 *.full · 확장 드라이버 = NSQL_DRIVER_DIR 또는 설정 폴더 drivers\)"
    }
}

Stop-RepoInstances 'debug'
$env:NSQL_NO_ACTIVATE = '1'
if ($NsqlHome) { $env:NSQL_HOME = $NsqlHome } else { Remove-Item Env:NSQL_HOME -ErrorAction SilentlyContinue }
$exe = Join-Path $root 'target\debug\nexa-sql.exe'
# ★ 표준 출력을 리다이렉트하지 않는다(협업 10-27 관찰): `-RedirectStandardOutput`은 핸들 상속으로 띄워 호출자의 stdout 파이프까지
#   앱에 상속되고, 출력을 받는 호출(`pwsh … | Out-String` · 도구 · CI)이 앱이 끝날 때까지 멈춘다. 리다이렉트 없는 Start-Process는
#   ShellExecute 경로라 상속이 없다. GUI 판은 콘솔 출력이 없으므로 로그는 두지 않는다(추적은 NSQL_TRACE_FRAMES로 콘솔에서 직접).
$p = Start-Process -FilePath $exe -WorkingDirectory $root -PassThru
Set-Content -Path $PidFile -Value "PID=$($p.Id)"
Write-Output "PID=$($p.Id) (pid file: $PidFile · home: $(if ($NsqlHome) { $NsqlHome } else { '실제 설정' }))"
