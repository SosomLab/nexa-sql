# win-func-check.ps1 — **기능 점검 자동화**(사용자 09-22 "오늘 요청한 전체 내용에 대해 기능 점검을 자동화") —
#   시나리오마다 앱을 격리 홈으로 띄우고(`NSQL_STARTUP_CMD`로만 몬다 · OS 키·마우스 주입 0 · docs/61 §4) 창을 PrintWindow로
#   캡처한 뒤 **자동 판정**(프로세스 생존 · 창 존재 · stderr 패닉 없음)과 **캡처 파일**을 남긴다. 눈으로 봐야 하는 항목(메뉴 위치 ·
#   색 · 툴팁)은 캡처를 보고 표에 적는다(docs/71 §6과 같은 산출물 규칙 · 결과 = journal).
#   키 입력이 있어야만 보이는 것(Ctrl+K,Ctrl+D 조합 · 파일 창 드래그)은 `@after:ms:<명령 id>`로 **명령을 직접** 부르거나
#   단위 테스트에 맡기고 여기서는 다루지 않는다(사용자 실기 U-표).
#
# 사용:
#   pwsh -NoProfile -File scripts/win-func-check.ps1 -HomeDir C:\tmp\nsql-fc-home -DataDir C:\tmp\nsql-fc-data -Out target\func-check
#   -Only "S05,S06"  = 일부만 · -Exe = 기본 Release
param(
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [Parameter(Mandatory = $true)][string]$DataDir,
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Exe = "",
    [string]$Cli = "",
    [string]$Only = ""
)
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\release\nexa-sql.exe" }
if (-not $Cli) { $Cli = Join-Path $root "target\release\nsql.exe" }
foreach ($d in @($HomeDir, $DataDir, $Out)) { if (-not (Test-Path -LiteralPath $d)) { New-Item -ItemType Directory -Path $d -Force | Out-Null } }
$report = Join-Path $Out "func-check.txt"
if (Test-Path -LiteralPath $report) { Clear-Content -LiteralPath $report }
function Say($s) { Write-Output $s; Add-Content -LiteralPath $report -Value $s -Encoding UTF8 }

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public class NxFc {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static IntPtr Biggest(uint pid) {
    IntPtr best = IntPtr.Zero; long area = 0;
    EnumWindows((h, x) => { if (!IsWindowVisible(h)) return true; uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true; RECT r; GetWindowRect(h, out r); long a = (long)(r.R - r.L) * (r.B - r.T);
      if (a > area) { area = a; best = h; } return true; }, IntPtr.Zero);
    return best;
  }
  public static long[] Windows(uint pid) {
    var v = new System.Collections.Generic.List<long>();
    EnumWindows((h, x) => { if (!IsWindowVisible(h)) return true; uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid) { RECT r; GetWindowRect(h, out r); if (r.R - r.L >= 50 && r.B - r.T >= 50) v.Add((long)h); } return true; }, IntPtr.Zero);
    return v.ToArray();
  }
  public static int CountVisible(uint pid) {
    int n = 0;
    EnumWindows((h, x) => { if (!IsWindowVisible(h)) return true; uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid) { RECT r; GetWindowRect(h, out r); if (r.R - r.L >= 50 && r.B - r.T >= 50) n++; } return true; }, IntPtr.Zero);
    return n;
  }
}
"@ -ErrorAction SilentlyContinue
[NxFc]::SetProcessDPIAware() | Out-Null

# ── 격리 홈 · 시험 자료 ────────────────────────────────────────────────────
$sqlite = Join-Path $HomeDir "local.sqlite"
$env:NSQL_HOME = $HomeDir
$env:NSQL_NO_ACTIVATE = "1"
if (-not (Test-Path -LiteralPath (Join-Path $HomeDir "profiles"))) {
    & $Cli conn add Local ("sqlite:" + $sqlite) -d sqlite --no-prompt 2>&1 | Out-Null
}
$q500 = Join-Path $DataDir "q500.sql"
Set-Content -LiteralPath $q500 -Encoding UTF8 -Value @"
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 500)
SELECT i AS id, 'name_' || i AS name, CASE WHEN i % 3 = 0 THEN NULL ELSE i * 1.5 END AS amount, 'SEBANG' AS project_cd, 'memo ' || i AS memo FROM n;
"@
$qErr = Join-Path $DataDir "q_err.sql"
Set-Content -LiteralPath $qErr -Encoding UTF8 -Value "SELECT * FROM no_such_table_zz;"
$qTwo = Join-Path $DataDir "q_two.sql"
Set-Content -LiteralPath $qTwo -Encoding UTF8 -Value @"
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 300)
SELECT i AS id, 'a_' || i AS name FROM n;
WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i < 300)
SELECT i AS id, 'b_' || i AS name FROM n;
"@
$qWord = Join-Path $DataDir "q_word.sql"
Set-Content -LiteralPath $qWord -Encoding UTF8 -Value "select sum(x) from t where sum > 1 and sum < 9 order by sum;`n-- sum sum sum"
$qMany = Join-Path $DataDir "q_many.sql"
Set-Content -LiteralPath $qMany -Encoding UTF8 -Value (("sum " * 150).Trim())
$a = Join-Path $DataDir "a.sql"; Set-Content -LiteralPath $a -Encoding UTF8 -Value "-- file a`nSELECT 1;"
$b = Join-Path $DataDir "b.sql"; Set-Content -LiteralPath $b -Encoding UTF8 -Value "-- file b`nSELECT 2;"
$c = Join-Path $DataDir "c.sql"; Set-Content -LiteralPath $c -Encoding UTF8 -Value "-- file c`nSELECT 3;"
$longDir = Join-Path $DataDir "very_long_directory_name_for_ellipsis_check\another_level_of_nesting_here\and_one_more_level_to_make_it_long"
if (-not (Test-Path -LiteralPath $longDir)) { New-Item -ItemType Directory -Path $longDir -Force | Out-Null }
$longFile = Join-Path $longDir "the_script_with_a_fairly_long_file_name_2026-09-22.sql"
Set-Content -LiteralPath $longFile -Encoding UTF8 -Value "SELECT 'long';"
$projFile = Join-Path $DataDir "fc.nsql-project"
$dj = $DataDir.Replace('\', '/')
Set-Content -LiteralPath $projFile -Encoding UTF8 -Value "{ `"version`": 1, `"folders`": [ { `"path`": `"$dj`" } ] }"

# 기본 설정: 결과 탭 바 항상(결과 탭 우클릭 시나리오) · 영어 · 데모 안내 끔.
$baseConf = "lang=en`ngrid.result_tabbar_single=on`ndemo.prompted=on`n"   # demo.prompted = 최초 실행 데모 안내 팝업 끔(같은 status 팝업 자리를 뺏는다)

# ── 시나리오 실행기 ─────────────────────────────────────────────────────────
function Run-Scenario {
    param([string]$Id, [string]$Title, [string]$Cmd, [string]$ArgList = "Local", [int]$WaitMs = 4500,
          [string]$Conf = "", [int]$Shots = 1, [int]$ShotGapMs = 600, [string]$Expect = "", [switch]$NoProfileHome)
    if ($Only -and (($Only.Split(",") | ForEach-Object { $_.Trim() }) -notcontains $Id)) { return }
    # 프로필 없는 홈 = 기동 시 접속 창이 먼저 뜬다(접속 창 시나리오용 · 메인만 뜨는 홈에서는 conn.toggle이 기동 명령으로 안 닿는다).
    $home2 = if ($NoProfileHome) { Join-Path $HomeDir "noprof" } else { $HomeDir }
    if (-not (Test-Path -LiteralPath $home2)) { New-Item -ItemType Directory -Path $home2 -Force | Out-Null }
    $env:NSQL_HOME = $home2
    Set-Content -LiteralPath (Join-Path $home2 "settings.conf") -Value ($baseConf + $Conf) -Encoding UTF8
    $errLog = Join-Path $Out ("{0}.stderr.txt" -f $Id)
    $env:NSQL_STARTUP_CMD = $Cmd
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe; $psi.Arguments = $ArgList; $psi.WorkingDirectory = (Split-Path $Exe)
    $psi.UseShellExecute = $false; $psi.RedirectStandardError = $true; $psi.RedirectStandardOutput = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    $errTask = $p.StandardError.ReadToEndAsync(); $outTask = $p.StandardOutput.ReadToEndAsync()
    Start-Sleep -Milliseconds $WaitMs
    $alive = -not $p.HasExited
    $wins = 0; $files = @()
    if ($alive) {
        $wins = [NxFc]::CountVisible([uint32]$p.Id)
        foreach ($i in 0..($Shots - 1)) {
            # 그 프로세스의 보이는 창 전부(메인 + 접속 창·보조 창) — 큰 것부터 `_w0`, `_w1`, …
            $hs = [NxFc]::Windows([uint32]$p.Id)
            $k = 0
            foreach ($hl in $hs) {
                $h = [IntPtr]$hl
                $r = New-Object NxFc+RECT
                [NxFc]::GetWindowRect($h, [ref]$r) | Out-Null
                $w = $r.R - $r.L; $hh = $r.B - $r.T
                if ($w -ge 50 -and $hh -ge 50) {
                    $bmp = New-Object System.Drawing.Bitmap $w, $hh
                    $g = [System.Drawing.Graphics]::FromImage($bmp)
                    $hdc = $g.GetHdc(); [NxFc]::PrintWindow($h, $hdc, 2) | Out-Null; $g.ReleaseHdc($hdc)
                    $suffix = if ($hs.Count -gt 1) { "_w$k" } else { "" }
                    $file = Join-Path $Out ("{0}_{1}{2}.png" -f $Id, $i, $suffix)
                    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
                    $files += (Split-Path -Leaf $file)
                    $k++
                }
            }
            if ($i -lt $Shots - 1) { Start-Sleep -Milliseconds $ShotGapMs }
        }
        try { $p.Kill() } catch {}
    }
    try { $p.WaitForExit(5000) | Out-Null } catch {}
    $err = ""; try { $err = $errTask.Result } catch {}
    if ($err) { Set-Content -LiteralPath $errLog -Value $err -Encoding UTF8 }
    $panic = ($err -match "panicked|RUST_BACKTRACE|thread '.*' panicked")
    $verdict = if (-not $alive) { "FAIL(exited)" } elseif ($panic) { "FAIL(panic)" } elseif ($wins -lt 1) { "FAIL(no window)" } else { "auto-ok" }
    Say ("| {0} | {1} | {2} | {3} | {4} | {5} |" -f $Id, $Title, $verdict, $wins, ($files -join " "), $Expect)
    $env:NSQL_STARTUP_CMD = $null
    $env:NSQL_HOME = $HomeDir
}

Say ("== win-func-check " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss") + "  commit=" + (& git -C $root rev-parse --short HEAD 2>$null))
Say ("exe=" + $Exe + "  home=" + $HomeDir)
Say "| id | 시나리오 | 자동 판정 | 창 | 캡처 | 눈으로 볼 것 |"
Say "|---|---|---|---|---|---|"

# 좌표 = 창 클라이언트 · 장치 px(배율 1.0 · 기본 창 1375×945 기준 · 캡처 menu0_0.png에서 잰 값).
#   메뉴바: File(25,14) Project(79,14) Edit(133,14)  · 편집기 탭 Script_1(365,80) · 탭 + (428,80) · 편집기 본문(592,269)
#   결과 영역: 그리드 첫 셀(≈420,565) · 행번호(334,565) · 결과 탭 바(≈380,507 · tabbar_single=on일 때 · 그리드 헤더는 527) · 푸터 Σ(667,902) · 전체 조회(619,902)

# §29 동시 편집 탭 상단 줄: 파일 둘 열고 두 번째 탭 Shift+클릭 → 두 탭 모두 상단 accent 줄.
Run-Scenario -Id S01 -Title "동시 편집 탭 상단 줄(§29)" -Cmd ("open:" + $a + ",open:" + $b + ",@after:1500:ui.sclick:365/80") -Expect "탭 두 개 모두 상단 accent 줄 · 두 칸"
# §30·§45·§47 다중 열기 확인 = 중앙 모달 · 기본 항목(열기)에 accent 테두리.
Run-Scenario -Id S02 -Title "다중 열기 확인 팝업 = 중앙·기본 항목(§30·45·47)" -Cmd ("file.open_many:" + $a + ";" + $b + ";" + $c) -Expect "창 중앙 팝업 · 3개 목록 · 'Open all' 행에 accent 테두리"
# 그 팝업의 기본 항목을 고르면 3탭이 순차 적재.
Run-Scenario -Id S03 -Title "다중 열기 → 순차 적재(§30)" -Cmd ("file.open_many:" + $a + ";" + $b + ";" + $c + ",@after:1200:multi.open") -WaitMs 5500 -Expect "탭 a·b·c 셋 · 내용 채워짐"
# §31 시작 모드: 프로젝트 인자 = 프로젝트 모드(프로젝트 패널) · 파일 인자 = 파일 모드.
Run-Scenario -Id S04 -Title "프로젝트 인자 = 프로젝트 모드(§31)" -Cmd "@after:1200:view.project" -ArgList ("Local `"" + $projFile + "`"") -Expect "왼쪽 프로젝트 패널에 DataDir 트리"
Run-Scenario -Id S05 -Title "파일 인자 = 파일 모드(§31)" -ArgList ("Local `"" + $a + "`" `"" + $b + "`"") -Cmd "" -Expect "탭 a·b · 프로젝트 없음"
# §28·§34 미리보기 탭: 프로젝트 패널에서 파일 클릭 = ◦ 탭 · 더블클릭 = 정식 탭(좌표는 패널 첫 파일 행 추정).
Run-Scenario -Id S06 -Title "프로젝트 패널 클릭 = 미리보기 탭(§28)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2500:ui.click:150/175") -WaitMs 5500 -Shots 1 -Expect "탭 제목 앞 ◦(미리보기) · 패널에 파일 목록"
Run-Scenario -Id S07 -Title "프로젝트 패널 더블클릭 = 정식 탭(§34)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2500:ui.dclick:150/175") -WaitMs 5500 -Expect "◦ 없는 정식 탭"
# §34 IME 안내 = 비밀번호 칸 아래(접속 창 캡처 명령).
Run-Scenario -Id S08 -Title "IME 안내 = 입력란 아래(§34)" -Cmd "@after:1500:conn.ime_hint:0/0" -ArgList "" -WaitMs 4500 -NoProfileHome -Expect "접속 창 비밀번호 칸 바로 아래 '입력 언어: 가 KOR' 카드"
# §38 긴 경로 가운데 축약(File ▸ 최근 파일).
Run-Scenario -Id S09 -Title "긴 경로 가운데 축약(§38)" -Cmd "@after:1000:ui.click:25/14" -Conf ("file.recent=" + $longFile.Replace('\', '\\') + "`nui.menu_max_width=320`n") -Expect "File 메뉴 최근 항목이 가운데 … 축약 · 폭 320 안"
# §43 실행 Facade + §48 시계 + §50~52 카드 스택: 질의 둘 실행 → 카드 2장(최신 위 · 편집기 영역 아래 고정).
Run-Scenario -Id S10 -Title "실행 카드 스택 · 최신 위 · 아래 고정(§48·50~52)" -Cmd ("open:" + $q500 + ",@after:1500:run.all,@after:3000:run.all,@after:4200:run.all") -WaitMs 6500 -Shots 2 -ShotGapMs 400 -Expect "카드 3장 · 최신이 맨 위 · 편집기 아래에 붙음 · 시계 00:00:00.mmm · ■ 회색"
# §50 오류 5번 = 카드 5장(빨간).
Run-Scenario -Id S11 -Title "오류 5번 = 카드 5장(§50)" -Cmd ("open:" + $qErr + ",@after:1200:run.all,@after:1700:run.all,@after:2200:run.all,@after:2700:run.all,@after:3200:run.all") -WaitMs 5500 -Expect "오류 카드 5장 · 스택"
# §43 Σ 건수 = 카드 경로 · 전체 조회 = 카드(푸터 버튼 클릭).
Run-Scenario -Id S12 -Title "Σ 건수·전체 조회 = 실행 카드(§43)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.click:667/902,@after:4200:ui.click:619/902") -WaitMs 6500 -Expect "건수 카드 · 전체 조회 카드(제목이 Count/Fetch all)"
# §44 그리드 선택 표시: 셀 클릭 = 행 배경 + 행/열 헤더 · NULL 흐림 · §49 헤더 위선 1px.
Run-Scenario -Id S13 -Title "그리드 셀 선택 표시 · NULL 흐림 · 헤더 선 1px(§44·49)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.click:420/580") -WaitMs 5500 -Expect "셀 진한 선택 + 행 전체 연한 배경 · 행번호·열 헤더 같은 선택색 · amount NULL 더 흐림 · 헤더 위선 1px"
Run-Scenario -Id S14 -Title "행번호 선택 = 행 배경만(§47)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.click:334/580") -WaitMs 5500 -Expect "행 전체 연한 배경(셀 진한 선택 없음) · 행번호 진한 선택색"
# §53 카드 툴팁 = 카드 왼쪽(카드 SQL 줄 위 hover · 좌표 = S10 캡처로 보정).
Run-Scenario -Id S15 -Title "카드 SQL 툴팁 = 카드 왼쪽(§53)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.move:1150/432") -WaitMs 5500 -Expect "툴팁이 카드 왼쪽 · 카드 상단 정렬 · 잘림 없음"
# §54 우클릭 메뉴가 카드 위 + 글자 크기 UI 글꼴.
Run-Scenario -Id S16 -Title "편집기 우클릭 메뉴 = 카드 위 · UI 글꼴(§54)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.rclick:1000/450") -WaitMs 5500 -Expect "편집 메뉴가 카드를 덮음 · 메뉴 글자 = 메뉴바와 같은 크기"
# §55 배타: 풀다운 연 채 편집기 우클릭 = 풀다운 닫히고 편집 메뉴만.
Run-Scenario -Id S17 -Title "풀다운 → 편집기 우클릭 = 배타(§55)" -Cmd "@after:1000:ui.click:133/14,@after:1800:ui.rclick:592/269" -Expect "Edit 풀다운 없음 · 편집 메뉴만"
# §55·58 편집 탭 메뉴 → 결과 탭 우클릭 = 편집 탭 메뉴 닫히고 결과 탭 메뉴(한 번에).
Run-Scenario -Id S18 -Title "편집 탭 메뉴 → 결과 탭 우클릭(§58)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.rclick:365/80,@after:3800:ui.rclick:380/507") -WaitMs 5500 -Expect "편집 탭 메뉴 없음 · 결과 탭 메뉴만"
# §58 결과 탭 메뉴 → 편집기 본문 우클릭 = 편집 메뉴(한 번에).
Run-Scenario -Id S19 -Title "결과 탭 메뉴 → 편집기 우클릭(§58)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.rclick:380/507,@after:3800:ui.rclick:592/269") -WaitMs 5500 -Expect "결과 탭 메뉴 없음 · 편집 메뉴만"
# §58 그리드 메뉴 → 편집기 우클릭.
Run-Scenario -Id S20 -Title "그리드 메뉴 → 편집기 우클릭(§58)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:ui.rclick:420/580,@after:3800:ui.rclick:592/269") -WaitMs 5500 -Expect "그리드 메뉴 없음 · 편집 메뉴만"
# §58 편집 탭 메뉴 → 편집기 우클릭.
Run-Scenario -Id S21 -Title "편집 탭 메뉴 → 편집기 우클릭(§58)" -Cmd "@after:1000:ui.rclick:365/80,@after:1800:ui.rclick:592/269" -Expect "탭 메뉴 없음 · 편집 메뉴만"
# §59 Ctrl+D · Ctrl+K,Ctrl+D = 명령을 직접(키 조합 대신): sum에 캐럿 → expand ×2 → skip → 선택 3곳 중 마지막이 다음으로.
Run-Scenario -Id S22 -Title "Quick Skip Next(§59 · 명령 직접)" -Cmd ("open:" + $qWord + ",@after:1200:ui.click:390/141,@after:1800:edit.expand_selection,@after:2200:edit.expand_selection,@after:2600:edit.skip_occurrence") -WaitMs 4500 -Expect "sum 선택 2곳 = 1번째·3번째(2번째는 건너뜀) · 상태줄 '2 selection regions'"
# §60 Edit 메뉴 그룹 + 하위 메뉴(보통 창 · 좁은 창).
Run-Scenario -Id S23 -Title "Edit 메뉴 6그룹 · Selection ▸(§60)" -Cmd "@after:1000:ui.click:133/14,@after:1600:ui.move:133/215" -Expect "1레벨 12항목+그룹 6 · Selection 하위 9항목이 오른쪽"
Run-Scenario -Id S24 -Title "좁은 창에서 하위 메뉴 잘림 없음(§60)" -Cmd "@after:1000:ui.click:133/14,@after:1600:ui.move:133/215" -Conf "window.main_size=430,600`n" -Expect "하위 메뉴가 창 안(nudge)"
# §56 하위 메뉴 → 비활성 부모 행 hover = 닫힘(편집 메뉴의 확장 하위: 확장이 없으면 그룹 없음 → 참고용).
Run-Scenario -Id S25 -Title "우클릭 메뉴 → Cut(비활성) hover(§56)" -Cmd "@after:1000:ui.rclick:592/269,@after:1600:ui.move:640/300" -Expect "메뉴만(하위 없음 · 비활성 Cut 위 hover 표시 없음)"
# §38 결과 탭 우클릭 메뉴 · 실행 쿼리 복사 항목 존재(캡처 명령 result.menu).
Run-Scenario -Id S26 -Title "결과 탭 메뉴(result.menu)" -Cmd ("open:" + $q500 + ",@after:1200:run.all,@after:3000:result.menu") -WaitMs 5500 -Expect "결과 탭 메뉴 · 항목 정상"
# §51 run.toast_max 상한(3) · follow: 질의 5번 → 카드 3장만.
Run-Scenario -Id S27 -Title "run.toast_max=3 상한(§51)" -Cmd ("open:" + $qErr + ",@after:1200:run.all,@after:1700:run.all,@after:2200:run.all,@after:2700:run.all,@after:3200:run.all") -Conf "run.toast_max=3`n" -WaitMs 5500 -Expect "오류 카드 3장만"
# §43 다중 결과 탭(두 문장) → 결과 탭 2 · 탭 바.
Run-Scenario -Id S28 -Title "결과 탭 둘 + 탭 바(§43)" -Cmd ("open:" + $qTwo + ",@after:1200:run.all") -WaitMs 5500 -Expect "결과 탭 2개 · 두 번째 활성"

# §65 선택 되돌리기(Sublime soft undo · Ctrl+U): sum에 캐럿 → Ctrl+D ×3(선택 3) → soft_undo → 선택 2 · 다시 soft_undo → 1(상태줄 개수 표시는 2 이상일 때만).
Run-Scenario -Id S29 -Title "선택 되돌리기 soft undo(§65 · 명령 직접)" -Cmd ("open:" + $qWord + ",@after:1200:ui.click:390/141,@after:1800:edit.expand_selection,@after:2200:edit.expand_selection,@after:2600:edit.expand_selection,@after:3000:edit.soft_undo") -WaitMs 4500 -Expect "상태줄 '2 selection regions'(3 → soft undo → 2)"
Run-Scenario -Id S30 -Title "선택 다시 실행 soft redo(§65)" -Cmd ("open:" + $qWord + ",@after:1200:ui.click:390/141,@after:1800:edit.expand_selection,@after:2200:edit.expand_selection,@after:2600:edit.expand_selection,@after:3000:edit.soft_undo,@after:3400:edit.soft_undo,@after:3800:edit.soft_redo") -WaitMs 5000 -Expect "상태줄 '2 selection regions'(3 → 2 → 1 → redo → 2)"
# §66 다중 선택 상한 `editor.max_occurrences`(100 · docs/72): sum 150개에서 Alt+F3 → 100에서 멈추고 상태줄 안내.
Run-Scenario -Id S31 -Title "다중 선택 상한 editor.max_occurrences(§66 · 명령 직접)" -Cmd ("open:" + $qMany + ",@after:1200:ui.click:372/141,@after:1800:edit.select_all_occurrences") -Conf "editor.max_occurrences=100`n" -WaitMs 4500 -Expect "상태줄 'Stopped at 100 selections (editor.max_occurrences)'"
# §68 ① 다중 선택 상한 = 줄 나누기에도: 150줄 Ctrl+A → Split into Lines(상한 100) → 100에서 멈추고 상태줄 안내.
$qLines = Join-Path $DataDir "q_lines.sql"
Set-Content -LiteralPath $qLines -Encoding UTF8 -Value ((1..150 | ForEach-Object { "SELECT $_;" }) -join "`n")
Run-Scenario -Id S32 -Title "다중 선택 상한 = Split into Lines(§68 ①)" -Cmd ("open:" + $qLines + ",@after:1200:ui.click:400/141,@after:1800:edit.select_all,@after:2400:edit.split_lines") -Conf "editor.max_occurrences=100`n" -WaitMs 4500 -Expect "상태줄 'Stopped at 100 selections' · 선택 100개"
# §70 북마크(docs/69 · T-167): 워크스페이스 파일이 시나리오 사이에 남으므로 먼저 clear_doc · 파일 열고 캐럿 줄 토글 → 패널 열기 → 거터 마크 + 패널 행 + 상태줄 개수.
Run-Scenario -Id S33 -Title "북마크 토글 + 패널(§70)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle,@after:2800:view.bookmarks") -WaitMs 4500 -Expect "북마크 영역(줄 번호 왼쪽) 1·2줄 색 띠 · 패널 'q500.sql 2' + 두 행 · 상태줄 '북마크 2/2'"
Run-Scenario -Id S34 -Title "북마크 다음/이전 이동(§70)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle,@after:2800:ui.click:400/141,@after:3200:bookmark.next") -WaitMs 4500 -Expect "캐럿이 2줄로(상태줄 Ln 2)"
# §71 북마크 패널 우클릭 메뉴(항목 행 · 팝업 규칙 ④ 모서리 캡처 대신 패널 안).
Run-Scenario -Id S35 -Title "북마크 패널 우클릭 메뉴(§71)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle,@after:2800:view.bookmarks,@after:3400:ui.rclick:150/182") -WaitMs 5000 -Expect "그룹 → 문서 → 항목 트리 · 항목 우클릭 메뉴(열기·이름·니모닉 ▸·그룹 이동 ▸·제거)"
# §76 프로젝트 탐색기 = 셰브론 부품 · OS 파일/폴더 아이콘(project.icons) · 루트 우클릭 메뉴(Remove Folder from Project).
Run-Scenario -Id S38 -Title "프로젝트 탐색기 아이콘·셰브론·루트 우클릭 메뉴(§76)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2500:ui.click:60/132,@after:3200:ui.rclick:120/132") -WaitMs 5000 -Expect "루트 펼침(셰브론 ∨ · 하위 폴더 › · 파일 = OS 아이콘) · 루트 행 위 메뉴 'Remove Folder from Project / Add Folder to Project…'"
Run-Scenario -Id S39 -Title "프로젝트 탐색기 아이콘 끔(project.icons=off · 향상 모드 값)(§76)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2500:ui.click:60/132") -Conf "project.icons=off`n" -WaitMs 4500 -Expect "아이콘 없이 셰브론 + 이름만"
# §77 프로젝트 파일의 `selected` = 마지막 클릭 위치 복원(열면 그 행이 선택·보임).
$projSel = Join-Path $DataDir "fc-sel.nsql-project"
Set-Content -LiteralPath $projSel -Encoding UTF8 -Value "{ `"version`": 1, `"folders`": [ { `"path`": `".`" } ], `"selected`": `"q_lines.sql`" }"
Run-Scenario -Id S40 -Title "프로젝트 열기 = 마지막 선택 위치 복원(§77)" -Cmd ("project.load:" + $projSel + ",@after:1200:view.project") -WaitMs 4000 -Expect "패널에서 q_lines.sql 행이 선택(강조)된 채 열림"
# §78 편집기 탭 우클릭 ▸ Reveal in Project Explorer(프로젝트 폴더 안 파일만 활성) · 활성 탭 따라가기(project.auto_reveal).
Run-Scenario -Id S41 -Title "편집기 탭 우클릭 메뉴 = Reveal in Project Explorer(§78)" -Cmd ("project.load:" + $projFile + ",open:" + $qLines + ",@after:1500:ui.rclick:470/80") -WaitMs 4000 -Expect "탭 메뉴 맨 아래 'Reveal in Project Explorer' 활성(q_lines.sql은 프로젝트 폴더 안) · Script_1 탭에서는 비활성"
Run-Scenario -Id S42 -Title "활성 탭 따라가기 = 탐색기 펼침+선택+스크롤(§78)" -Cmd ("project.load:" + $projFile + ",@after:800:view.project,@after:1500:open:" + $qLines) -Conf "project.auto_reveal=on`n" -WaitMs 4500 -Expect "패널: 루트 펼침 · q_lines.sql 행 선택(강조) · 편집기 포커스 유지"
Run-Scenario -Id S43 -Title "활성 탭 따라가기 끔(기본) = 펼치지 않고 표시만(§78)" -Cmd ("project.load:" + $projFile + ",@after:800:view.project,@after:1500:open:" + $qLines) -WaitMs 4500 -Expect "패널: 루트 펼쳐진 채(루트 하나 = 자동) q_lines.sql 행 선택 표시 · 스크롤 없음"
# §79 프로젝트 탐색기 가로 스크롤(긴 이름) · 행 클립(스크롤한 첫 행이 필터 상자를 덮지 않음) · 루트 메뉴 = Remove만.
Run-Scenario -Id S44 -Title "프로젝트 탐색기 가로 스크롤(§79)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2500:ui.hwheel:180/300/360") -WaitMs 4500 -Expect "긴 폴더 이름의 뒷부분이 보이도록 트리가 왼쪽으로 밀림 · 아래 가로 스크롤바"
Run-Scenario -Id S45 -Title "프로젝트 탐색기 세로 스크롤 = 필터 상자와 겹치지 않음(§79)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2500:ui.wheel:180/300/-120") -Conf "window.main_size=1000,420`n" -WaitMs 4500 -Expect "첫 행이 필터 상자 아래에서 잘려 시작(상자 위로 안 올라감)"
# §80 필터 틀 부품(FilterBar): 프로젝트·북마크·확장 패널 = Aa·ab·(.*) 토글 · 프로젝트는 오른쪽에 숨김/점 파일 토글 · 위·아래 여백 8 · 빈 영역 클릭 = 선택 해제.
Run-Scenario -Id S46 -Title "프로젝트 탐색기 필터 틀 = 토글 셋 + 숨김/점 파일 토글 · 여백(§80)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2500:ui.click:150/175,@after:3200:ui.click:180/560") -WaitMs 5000 -Expect "필터 틀 안 오른쪽 Aa·ab·(.*) · 틀 오른쪽 눈·점 아이콘 둘(점 = on) · 헤더/트리와 8px 여백 · 빈 영역 클릭 뒤 선택 없음"
Run-Scenario -Id S47 -Title "북마크 패널 필터 틀 = 토글 셋(§80)" -Cmd ("open:" + $q500 + ",@after:1000:view.bookmarks") -WaitMs 3500 -Expect "필터 틀 안 오른쪽 Aa·ab·(.*) · 위·아래 여백 8"
Run-Scenario -Id S48 -Title "확장 패널 검색 틀 = 토글 셋(§80)" -Cmd "@after:1000:view.extensions" -Conf "extensions.enabled=on`n" -WaitMs 3500 -Expect "검색 틀 안 오른쪽 Aa·ab·(.*) · 아래 여백 8"
# §80 안내 글 겹침 없음: 일치 없음 + 상한 안내가 한 줄씩(scan_max=100 · 필터 = 없는 이름).
$projBig = Join-Path $DataDir "fc-big.nsql-project"
$rj = $root.Replace([char]92, '/')
Set-Content -LiteralPath $projBig -Encoding UTF8 -Value "{ `"version`": 1, `"folders`": [ { `"path`": `"$rj`" } ] }"
Run-Scenario -Id S49 -Title "필터 안내 글 두 줄(일치 없음 → 상한)(§80)" -Cmd ("project.load:" + $projBig + ",@after:1200:view.project,@after:2000:project.filter:zzqq") -Conf "project.scan_max=100`n" -WaitMs 4500 -Expect "'No match' 아래 줄에 'stopped at 100 (project.scan_max)' — 겹치지 않음"
# §81 필터 Path 토글(기본 끔): 켜면 루트부터의 상대 경로에 일반/정규식 매칭 · 상한 안내는 실제로 멈췄을 때만.
Run-Scenario -Id S50 -Title "필터 Path 토글 = 경로 일치(§81)" -Cmd ("project.load:" + $projFile + ",@after:1200:view.project,@after:2000:project.filter:p:check/another") -WaitMs 4500 -Expect "틀 안 넷째 토글(…/) 켜짐 · …ellipsis_check/another_level… 폴더 경로가 걸려 조상과 함께 보임 · 이름만이면 0"
# §89 작업 환경 복원: 프로젝트 파일의 tabs(파일 + 미저장 스크립트 본문 + 캐럿 앵커) · active · 북마크 → 로딩 때 복원 · OPEN FILES 섹션.
$projWs = Join-Path $DataDir "fc-ws.nsql-project"
$ql = $qLines.Replace('\', '/')
$projWsJson = ("{ `"version`": 1, `"folders`": [ { `"path`": `".`" } ], `"active`": 1, `"panel`": `"bookmarks`", `"profiles`": [ `"Local`" ], `"tabs`": [ { `"path`": `"" + $ql + "`", `"title`": `"q_lines.sql`", `"line`": 40, `"col`": 0, `"anchor`": `"SELECT 41;`", `"before`": `"SELECT 40;`", `"after`": `"SELECT 42;`" }, { `"title`": `"Notes`", `"line`": 1, `"col`": 0, `"text`": `"-- restored scratch\nSELECT 'ws';`" } ], `"bookmarks`": { `"version`": 1, `"next_id`": 2, `"next_group`": 2, `"groups`": [ { `"id`": 1, `"name`": `"`", `"default`": true, `"enabled`": true } ], `"items`": [ { `"id`": 1, `"doc`": { `"file`": `"" + $ql + "`" }, `"line`": 4, `"col`": 0, `"text`": `"SELECT 5;`", `"before`": `"SELECT 4;`", `"after`": `"SELECT 6;`", `"hash`": `"0`", `"lines`": 150, `"group`": 1, `"shared`": false, `"state`": `"live`", `"created`": 0, `"visited`": 0 } ] } }")
Set-Content -LiteralPath $projWs -Encoding UTF8 -Value $projWsJson
Run-Scenario -Id S51 -Title "프로젝트 로딩 = 작업 환경 복원 + OPEN FILES(§89)" -Cmd ("project.load:" + $projWs + ",@after:1500:view.project") -WaitMs 4500 -Expect "탭 = Script_1 · q_lines.sql · Notes(활성 · 본문 '-- restored scratch') · 패널 OPEN FILES 3줄(Notes 강조) · 상태줄 '탭 2개 복원' · q_lines 캐럿 41행(앵커) · 북마크 5행 띠"
# §75 🔧 풀다운 Project ▸ 새 프로젝트 저장… = 파일 창(메뉴바 픽 → menu_action → project_cmd · 사용자 09-22 "풀다운에서 아무 동작이 없다").
Run-Scenario -Id S37 -Title "풀다운 Project ▸ Save New Project… = 파일 저장 창(§75)" -Cmd ("open:" + $qLines + ",@after:1000:ui.click:75/14,@after:1600:ui.click:120/48") -WaitMs 4500 -Expect "창 2 = 메인 + 파일 저장 창(제목 Save Project · 이름 칸 .nsql-project)"
# §74 CLI ↔ GUI 공유(B8) + 미니맵 틱 · 줄 끝 라벨(U-2): 워크스페이스를 비운 뒤 CLI로 심고 GUI가 같은 파일을 읽는다.
if (-not $Only -or (($Only.Split(",") | ForEach-Object { $_.Trim() }) -contains "S36") -or (($Only.Split(",") | ForEach-Object { $_.Trim() }) -contains "S54") -or (($Only.Split(",") | ForEach-Object { $_.Trim() }) -contains "S55")) {
    $ws = Join-Path $HomeDir "workspaces\default.nsql-workspace"
    if (Test-Path -LiteralPath $ws) { Remove-Item -LiteralPath $ws -Force }
    $env:NSQL_HOME = $HomeDir
    & $Cli bookmark add $qLines 2 --label "monthly totals" 2>&1 | ForEach-Object { Say ("  cli: " + $_) }
    & $Cli bookmark add $qLines 5 2>&1 | ForEach-Object { Say ("  cli: " + $_) }
    & $Cli bookmark add $qLines 120 2>&1 | ForEach-Object { Say ("  cli: " + $_) }
    & $Cli bookmark list 2>$null | ForEach-Object { Say ("  cli: " + $_) }
}
Run-Scenario -Id S36 -Title "CLI로 심은 북마크 = 거터·미니맵 틱·줄 끝 라벨(§74)" -Cmd ("open:" + $qLines) -WaitMs 4000 -Expect "거터 2·5줄 색 띠 · 2줄 끝 '◆ monthly totals' 흐린 글 · 미니맵 오른쪽 가장자리 점 셋(120줄 = 화면 밖) · 상태줄 '북마크 3/3'"
# §92 프로젝트 저장 = 작업 환경을 담아서(탭 경로) · 저장을 거듭해도 탭이 늘지 않는다(사용자 09-23 "저장 누를 때마다 탭 추가").
$projSave = Join-Path $DataDir "fc-save.nsql-project"
Set-Content -LiteralPath $projSave -Encoding UTF8 -Value "{ `"version`": 1, `"folders`": [ { `"path`": `".`" } ] }"
Run-Scenario -Id S52 -Title "프로젝트 저장 3번 = 탭 그대로 · 파일에 탭 경로(§92)" -Cmd ("project.load:" + $projSave + ",open:" + $a + ",open:" + $b + ",@after:1000:view.project,@after:1300:bookmark.set_1,@after:1400:ui.click:400/161,@after:1500:bookmark.set_2,@after:1600:ui.click:460/80,@after:1700:bookmark.set_1,@after:1800:project.save,@after:2600:project.save,@after:3400:project.save") -WaitMs 5000 -Expect "탭 = Script_1 · a.sql · b.sql 셋뿐(저장 3번 뒤에도 추가 없음) · OPEN FILES 3줄 · 파일 검사 mn1x2/mn2x1 = 파일 단위 니모닉 보존"
if (-not $Only -or (($Only.Split(",") | ForEach-Object { $_.Trim() }) -contains "S52")) {
$savedTxt = if (Test-Path -LiteralPath $projSave) { Get-Content -LiteralPath $projSave -Raw } else { "" }
Say ("  file: tabs=" + ($savedTxt -match '"tabs"') + " a.sql=" + ($savedTxt -match '"path": "a\.sql"') + " b.sql=" + ($savedTxt -match '"path": "b\.sql"') + " expanded=" + ($savedTxt -match '"expanded"') + " panel=" + ($savedTxt -match '"panel": "project"') + " mn1x2=" + (([regex]::Matches($savedTxt, '"mn": 1')).Count -eq 2) + " mn2x1=" + (([regex]::Matches($savedTxt, '"mn": 2')).Count -eq 1) + " blob=" + ($savedTxt -match '%%NSQL-BLOBS%%') + " notext=" + (-not ($savedTxt -match '"title": "[^"]*"[^
]*"text":')))
}
# §92 프로젝트 로딩 = 작업 환경 **교체**(복원 목록에 없던 깨끗한 탭은 닫힘) + 보이던 패널(북마크) + 접속 표식 토스트.
Set-Content -LiteralPath $projWs -Encoding UTF8 -Value $projWsJson
Run-Scenario -Id S53 -Title "프로젝트 로딩 = 옛 탭 닫고 교체 · 패널·접속 표식 복원(§92)" -Cmd ("open:" + $a + ",@after:1200:project.load:" + $projWs) -WaitMs 3200 -Expect "탭 = q_lines.sql · Notes만(Script_1·a.sql 닫힘) · 북마크 패널 열림 · 토스트 '마지막 접속: Local' · 상태줄 '탭 2개 복원'"
# §92 북마크 패널: 한 번 클릭 = 미리보기 탭(◦) · 더블클릭 = 정식 탭(프로젝트 탐색기와 같은 규칙 · 사용자 09-23).
Run-Scenario -Id S54 -Title "북마크 패널 더블클릭 = 정식 탭(§92)" -Cmd "view.bookmarks,@after:1500:ui.dclick:150/182" -WaitMs 4000 -Expect "탭 q_lines.sql(◦ 없음 · 정식) · 캐럿 2줄"
Run-Scenario -Id S55 -Title "북마크 패널 한 번 클릭 = 미리보기 탭(§92)" -Cmd "view.bookmarks,@after:1500:ui.click:150/182" -WaitMs 4000 -Expect "탭 ◦ q_lines.sql(미리보기) · 캐럿 2줄"
# §95 줄 번호를 꺼도 북마크 띠·니모닉 상자는 슬림 거터에(사용자 09-23 "줄번호 해제하면 표시할 수 없다").
Run-Scenario -Id S56 -Title "줄 번호 끔 + 북마크·니모닉 = 슬림 거터(§95)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.set_3,@after:2000:ui.click:400/161,@after:2400:bookmark.toggle") -Conf "editor.line_numbers=off`n" -WaitMs 4000 -Expect "줄 번호 없음 · 맨 왼쪽 북마크 영역에 1줄 니모닉 3 상자 + 2줄 색 띠 · 그 오른쪽이 본문(배치 = 북마크 영역 → 줄 번호 → 편집 → 미니맵)"
# §98 프로젝트 닫기 = 전체 저장 → 처음 실행 상태(사용자 09-23) + 북마크 = 로컬 세트(프로젝트 것 잔존 없음).
Set-Content -LiteralPath $projWs -Encoding UTF8 -Value $projWsJson
Run-Scenario -Id S57 -Title "프로젝트 닫기 = 처음 실행 상태 + 북마크 초기화(§98)" -Cmd ("open:" + $a + ",@after:1000:project.load:" + $projWs + ",@after:2500:project.close,@after:3200:view.bookmarks") -WaitMs 4500 -Expect "탭 = 빈 Script_1 하나(q_lines·Notes·a.sql 전부 닫힘) · 프로젝트 패널 닫힘 · 북마크 패널 = 로컬 세트(S33/S36이 심은 q500·q_lines · 기존 프로젝트를 연 것이라 이관 없음 · 프로젝트 것 아님) · 상태줄 'Project closed'"
# §99 작업 모드 셋: 폴더 인자 = 폴더 모드 → 북마크 저장 위치 = <폴더>/.nsql/(전역 %APPDATA% 아님).
$folderWs = Join-Path $DataDir ".nsql\workspaces\default.nsql-workspace"
if (Test-Path -LiteralPath $folderWs) { Remove-Item -LiteralPath $folderWs -Force }
Run-Scenario -Id S58 -Title "폴더 모드 = 북마크가 <폴더>/.nsql/에(§99)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1600:bookmark.toggle,@after:2200:view.bookmarks") -ArgList ("Local `"" + $DataDir + "`"") -WaitMs 5000 -Expect "북마크 패널에 q500.sql 1줄 · 파일 검사 folder_ws=True(폴더 안 .nsql) · 기동 인자 = 폴더"
if (-not $Only -or (($Only.Split(",") | ForEach-Object { $_.Trim() }) -contains "S58")) {
Say ("  file: folder_ws=" + (Test-Path -LiteralPath $folderWs))
}
# §103 프로젝트가 없어도 OPEN FILES(사용자 09-23 "프로젝트 여부와 상관없이 열린 파일은 보여지도록").
Run-Scenario -Id S59 -Title "파일 모드 + 프로젝트 패널 = OPEN FILES 표시(§103)" -Cmd ("open:" + $a + ",open:" + $b + ",@after:1200:view.project") -WaitMs 3500 -Expect "머리글 '프로젝트 없음' 아래 OPEN FILES 3줄(Script_1 · a.sql · b.sql 활성) → 파일 필터 → 안내 글·링크 셋"
# §104 북마크 디바운스 저장이 스스로 깬다(사용자 09-23 "값이 바뀌어도 저장이 안 됨"): 토글 뒤 아무 사건 없이 기다려도 파일이 써진다.
$wsHome = Join-Path $HomeDir "workspaces\default.nsql-workspace"
if (Test-Path -LiteralPath $wsHome) { Remove-Item -LiteralPath $wsHome -Force }
Run-Scenario -Id S60 -Title "파일 모드 북마크 = 사건 없이도 디바운스 저장(§104)" -Cmd ("open:" + $q500 + ",@after:800:ui.click:400/141,@after:1000:bookmark.toggle") -WaitMs 4500 -Expect "거터 1줄 띠 · 파일 검사 ws_saved=True(토글 뒤 사건 없이 1~5초 안에 default.nsql-workspace 생성)"
if (-not $Only -or (($Only.Split(",") | ForEach-Object { $_.Trim() }) -contains "S60")) {
Say ("  file: ws_saved=" + (Test-Path -LiteralPath $wsHome))
}
# §107 거터(북마크/니모닉 영역) 우클릭 메뉴(사용자 09-23): 없으면 "추가" 활성 · 있으면 "제거"·"니모닉 해제" 활성 · 니모닉 1~9 하위.
Run-Scenario -Id S61 -Title "거터 우클릭 = 북마크 메뉴(없는 줄 · §107·§108)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1500:ui.rclick:335/141") -WaitMs 3500 -Expect "거터 우클릭(335/141 = 2줄) 좁은 메뉴(170px): 'Bookmarks'(보기) · 구분선 · 'Toggle Bookmark'(✓ 없음) · 'Mnemonic ▸' · 'Remove' 없음 · 캐럿 2줄"
Run-Scenario -Id S62 -Title "거터 우클릭 = 있는 줄 = 토글 ✓ · 니모닉 해제 활성(§107·§108)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1200:ui.click:400/141,@after:1500:bookmark.set_3,@after:2000:ui.rclick:335/141") -WaitMs 4000 -Expect "2줄 북마크 띠 + 니모닉 3 상자(클릭 400/141 = 2줄) · 메뉴: 'Bookmarks' · 'Toggle Bookmark'(✓) · Mnemonic ▸(3에 ✓ · 해제 활성) · 상태줄 'Mnemonic 3 set on line 2'"
Run-Scenario -Id S65 -Title "줄 변경 표시 = 복제한 줄에(줄 번호 켬 · §113)" -Cmd ("open:" + $q500 + ",@after:1000:ui.click:400/141,@after:1500:edit.duplicate_line") -WaitMs 3500 -Expect "2줄 복제 → 3줄에 초록 막대(줄 번호 오른쪽 띠) · 탭 미저장 표시"
Run-Scenario -Id S64 -Title "줄 번호 끔 + 줄 변경 표시(§113)" -Cmd ("open:" + $q500 + ",@after:1000:ui.click:400/141,@after:1500:edit.duplicate_line") -Conf "editor.line_numbers=off`n" -WaitMs 3500 -Expect "줄 번호 없이도 슬림 거터에 2줄(복제된 줄) 초록 막대 · 탭 미저장 표시"
# §117 IntelliSense · 아웃라인(docs/76): 심볼이 있는 스크립트(DEFINE · CTE · 패키지 본문) → Ctrl+Space 팝업 · Goto Symbol 팔레트 · 아웃라인 패널.
$qOutline = Join-Path $DataDir "q_outline.sql"
Set-Content -LiteralPath $qOutline -Encoding UTF8 -Value @"
DEFINE v_user = 'scott'
VARIABLE rc REFCURSOR
WITH recent AS (SELECT 1 AS id FROM dual), older AS (SELECT 2 AS id FROM dual)
SELECT r.id FROM recent r, older o WHERE r.id = o.id;
CREATE OR REPLACE PACKAGE BODY pkg_report AS
  g_count NUMBER := 0;
  CURSOR c_rows IS SELECT 1 FROM dual;
  PROCEDURE run_report(p_day IN DATE) IS
    v_total NUMBER;
  BEGIN
    NULL;
  END run_report;
  FUNCTION total_rows RETURN NUMBER IS
  BEGIN
    RETURN g_count;
  END total_rows;
END pkg_report;
/
"@
Run-Scenario -Id S66 -Title "코드 완성 팝업 = Ctrl+Space(§117)" -Cmd ("open:" + $qOutline + ",@after:1000:ui.click:392/201,@after:1600:edit.complete") -WaitMs 3500 -Expect "4줄 'SELECT r' 뒤(캐럿 = r 다음) Ctrl+Space → 캐럿 아래 팝업: recent(alias r · cte) · r(alias) · 키워드/문서 단어 … 오른쪽 열 = 종류"
Run-Scenario -Id S67 -Title "Goto Symbol 팔레트(§117)" -Cmd ("open:" + $qOutline + ",@after:1200:goto.symbol") -WaitMs 3500 -Expect "팔레트에 심볼 목록: DEFINE v_user · VARIABLE rc · 문장 머리 · CTE recent/older · package body pkg_report · declared g_count · cursor c_rows · procedure run_report(깊이 2 들여쓰기) · function total_rows · 각 행 끝 :줄"
Run-Scenario -Id S68 -Title "아웃라인 패널(§117)" -Cmd ("open:" + $qOutline + ",@after:1200:view.outline") -WaitMs 3500 -Expect "좌측 아웃라인 패널: 필터 틀 · 줄 번호 → 이름(문장 흐림 · 서브프로그램 굵게 강조색 · 깊이 들여쓰기) → 오른쪽 종류/타입 · 활동 막대 아웃라인 아이콘 활성"
Run-Scenario -Id S69 -Title "북마크 패널 문서 이름 = 탭 id로 지금 이름(§118)" -Cmd ("file.new,@after:800:ui.click:400/141,@after:1000:bookmark.toggle,@after:1400:view.bookmarks,@after:2000:tab.rename_to:NoName1") -WaitMs 3500 -Expect "새 탭 Script_2에 북마크 → 탭 이름을 NoName1로 바꾼 뒤 패널 문서 행이 'NoName1 1'(Script_2가 남지 않음) · 탭 제목도 NoName1"
$qQuote = Join-Path $DataDir "q_quote.sql"
Set-Content -LiteralPath $qQuote -Encoding UTF8 -Value "DEFINE who = 'O''Neil'`nSELECT 'it''s' AS v, `"a`"`"b`" AS q FROM DUAL;"
Run-Scenario -Id S70 -Title "SQL '' 이스케이프 = 쌍 대상 아님(§118)" -Cmd ("open:" + $qQuote + ",@after:1200:ui.click:474/141") -WaitMs 3500 -Expect "2줄 'it''s' 끝 ' 뒤 캐럿(474/141 = Ln 2 Col 15) → 짝 밑줄 = 첫 '(it 앞)와 마지막 '(s 뒤)에만 · 가운데 ''에는 밑줄 없음 · 'it''s'·'O''Neil'이 각각 한 문자열 색"
$skipDir = Join-Path $DataDir "skipdir"
if (-not (Test-Path -LiteralPath $skipDir)) { New-Item -ItemType Directory -Path $skipDir | Out-Null }
Set-Content -LiteralPath (Join-Path $skipDir "ok.sql") -Encoding UTF8 -Value "SELECT 1 FROM t;`nSELECT 2 FROM u;"
[System.IO.File]::WriteAllText((Join-Path $skipDir "big.sql"), ("SELECT big;`n" * 110000))
[System.IO.File]::WriteAllBytes((Join-Path $skipDir "bin.dat"), [byte[]](83,69,76,69,67,84,0,1,2,3))
Run-Scenario -Id S71 -Title "파일 검색 제외 로그 = 제외됨(N) + 이유(§120)" -Cmd ("view.search,@after:1000:search.run:SELECT,@after:3200:ui.click:120/271") -ArgList ("Local `"" + $skipDir + "`"") -WaitMs 4500 -Expect "폴더 모드(skipdir) 검색 SELECT → 상태 '1 files · 2 matches · 2 skipped' · ok.sql 2 일치 · 아래 'Skipped (2)' 행(클릭 120/271로 펼침) → big.sql '크기 초과: 1320 KB > 1024 KB' · bin.dat '이진 파일' · 상태줄 '… · 제외 2'"
Run-Scenario -Id S63 -Title "거터 우클릭 = 마지막 줄 아래 빈 영역 = 보기만(§108)" -Cmd ("open:" + $q500 + ",@after:1000:bookmark.clear_doc,@after:1500:ui.rclick:335/400") -WaitMs 3500 -Expect "빈 영역(335/400 · 2줄 파일) 우클릭 = 'Bookmarks' 한 항목뿐(토글·니모닉 없음) · 캐럿은 그대로 1줄"
Say ("== done " + (Get-Date -Format "HH:mm:ss"))
