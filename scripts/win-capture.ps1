# win-capture.ps1 — Windows 실기 캡처 자동화(사용자 09-16 "테스트 자동화"): Debug exe를 띄워 메인 창과 환경 설정 창을
#   PrintWindow로 PNG에 담고, 글자 선명도 판독용 영역을 ×3 최근접 확대 크롭으로 target\capture\ 에 남긴다.
#   CopyFromScreen·FindWindow는 비대화형 세션에서 실패해 PrintWindow(PW_RENDERFULLCONTENT) + EnumWindows 제목 매칭을 쓴다.
# 사용:  pwsh -File scripts\win-capture.ps1   (Windows PowerShell 5.1도 가능 — 파일은 UTF-8 BOM) [-Exe target\debug\nexa-sql.exe] [-Out target\capture] [-Keep]
#   결과: main.png · prefs.png · crop_*.png (menu · editor · status · prefs_close · prefs_desc). -Keep이면 앱을 닫지 않는다.
param(
    [string]$Exe = "target\debug\nexa-sql.exe",
    [string]$Out = "target\capture",
    [switch]$Keep
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public class NxCap {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static IntPtr Find(string part, bool exact) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { if (!IsWindowVisible(h)) return true; var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256);
      var t = sb.ToString(); if (exact ? t == part : t.Contains(part)) { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
  }
}
"@
[NxCap]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null

function Shot([IntPtr]$h, [string]$file) {
    [NxCap]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 400
    $r = New-Object NxCap+RECT
    [NxCap]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.R - $r.L; $hh = $r.B - $r.T
    $bmp = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [NxCap]::PrintWindow($h, $hdc, 2) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Output "saved $file ${w}x${hh}"
}

function Crop([string]$in, [string]$file, [int]$x, [int]$y, [int]$w, [int]$h, [int]$scale = 3) {
    $src = [System.Drawing.Bitmap]::FromFile($in)
    $dst = New-Object System.Drawing.Bitmap ($w * $scale), ($h * $scale)
    $g = [System.Drawing.Graphics]::FromImage($dst)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
    $g.DrawImage($src, (New-Object System.Drawing.Rectangle 0, 0, ($w * $scale), ($h * $scale)), (New-Object System.Drawing.Rectangle $x, $y, $w, $h), [System.Drawing.GraphicsUnit]::Pixel)
    $dst.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $src.Dispose(); $dst.Dispose()
    Write-Output "saved $file"
}

$started = $false
if (-not (Get-Process nexa-sql -ErrorAction SilentlyContinue)) {
    Start-Process -FilePath (Resolve-Path $Exe) -WorkingDirectory (Get-Location)
    $started = $true
    Start-Sleep -Seconds 5
}
$main = [NxCap]::Find("Nexa SQL", $true)
if ($main -eq [IntPtr]::Zero) { throw "메인 창을 찾지 못함" }
# 시작 때 뜨는 로그인 창(있으면)도 담는다 — 버튼 줄(굵기·한글) 판독.
$login = [NxCap]::Find("데이터베이스 로그인", $false)
if ($login -ne [IntPtr]::Zero) {
    Shot $login "$Out\login.png"
    Crop "$Out\login.png" "$Out\crop_login_buttons.png" 200 60 440 34
}
Shot $main "$Out\main.png"
# 메인 창 판독 영역(1116×759 기본 창 기준): 메뉴 · 편집기 첫 줄 · 상태줄/도구줄.
Crop "$Out\main.png" "$Out\crop_menu.png" 10 34 300 24
Crop "$Out\main.png" "$Out\crop_editor.png" 60 128 320 28
Crop "$Out\main.png" "$Out\crop_status.png" 780 695 336 58

# 환경 설정 창(Ctrl+,) — 닫기 버튼 · 설명문(굵은 제목 + 본문).
$ws = New-Object -ComObject WScript.Shell
$prefs = [IntPtr]::Zero
for ($try = 0; $try -lt 4 -and $prefs -eq [IntPtr]::Zero; $try++) {
    [NxCap]::SetForegroundWindow($main) | Out-Null
    Start-Sleep -Milliseconds 700
    $ws.SendKeys("^,")
    Start-Sleep -Seconds 2
    $prefs = [NxCap]::Find("환경 설정", $false)
    if ($prefs -eq [IntPtr]::Zero) { $prefs = [NxCap]::Find("Preferences", $false) }
}
if ($prefs -ne [IntPtr]::Zero) {
    Shot $prefs "$Out\prefs.png"
    Crop "$Out\prefs.png" "$Out\crop_prefs_close.png" 700 630 230 32 4
    Crop "$Out\prefs.png" "$Out\crop_prefs_desc.png" 268 62 330 52
} else {
    Write-Warning "환경 설정 창을 열지 못함(Ctrl+, 전달 실패)"
}
if ($started -and -not $Keep) { Stop-Process -Name nexa-sql -Force -ErrorAction SilentlyContinue }
