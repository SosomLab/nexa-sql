# win-burst-capture.ps1 — 격리 설정 폴더(NSQL_HOME) + 기동 명령(NSQL_STARTUP_CMD)으로 앱을 띄워 메인 창을 일정 간격으로
#   여러 장 캡처한다. **PrintWindow만 쓴다 — 키·마우스 입력을 주입하지 않고 포커스도 빼앗지 않는다**(docs/61 §2-4).
#   적재 진행 막처럼 "잠깐 보이는 화면"이나 "명령 뒤의 상태"를 확인할 때 쓴다.
#
# 사용:
#   pwsh -NoProfile -File scripts/win-burst-capture.ps1 -HomeDir C:\tmp\nsql-home `
#        -Cmd "open:C:\tmp\big.sql,@after:3000:edit.duplicate_line" -Name dup -WaitMs 6000 -Count 1 -ArgList Local
#
# ★ -HomeDir는 **이미 있는 폴더**여야 한다(없으면 멈춘다). 09-20에 셸 인자 실수("$SW\\$1" → 글자 그대로 `$1`)로 격리 폴더가
#   엉뚱한 새 폴더로 잡혀 "Not connected" 화면을 디버깅한 적이 있다 — 경로는 미리 변수에 조립해 넘긴다.
param(
    [string]$Exe = "",
    [string]$Out = "",
    [Parameter(Mandatory = $true)][string]$HomeDir,
    [string]$Cmd = "",
    [string]$Name = "burst",
    [int]$WaitMs = 3000,
    [int]$Count = 8,
    [int]$IntervalMs = 400,
    [string]$ArgList = ""
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root "target\debug\nexa-sql.exe" }
if (-not $Out) { $Out = Join-Path $root "target\capture" }
if ($HomeDir -match '\$' -or -not (Test-Path -LiteralPath $HomeDir -PathType Container)) {
    # 메시지는 영어로(콘솔 코드 페이지에 따라 한글이 깨진다).
    throw "HomeDir does not exist or looks wrong: '$HomeDir' - create the sandbox folder first and check the path."
}
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class NxBurst {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  // 그 프로세스의 보이는 창 가운데 가장 큰 것(= 메인 창 · 로그인 창이 같이 떠 있어도).
  public static IntPtr Biggest(uint pid) {
    IntPtr best = IntPtr.Zero; long area = 0;
    EnumWindows((h, x) => { if (!IsWindowVisible(h)) return true; uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid) return true; RECT r; GetWindowRect(h, out r); long a = (long)(r.R - r.L) * (r.B - r.T);
      if (a > area) { area = a; best = h; } return true; }, IntPtr.Zero);
    return best;
  }
}
"@
[NxBurst]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null
$env:NSQL_HOME = $HomeDir
$env:NSQL_NO_ACTIVATE = "1"  # 시험 창이 사용자의 전경 포커스를 가져가지 않게(docs/61 §4)
$env:NSQL_STARTUP_CMD = $Cmd
if ($ArgList) { $p = Start-Process -FilePath $Exe -ArgumentList $ArgList -WorkingDirectory (Split-Path $Exe) -PassThru }
else { $p = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru }
try {
    Start-Sleep -Milliseconds $WaitMs
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    foreach ($i in 0..($Count - 1)) {
        $h = [NxBurst]::Biggest($p.Id)
        if ($h -ne [IntPtr]::Zero) {
            $r = New-Object NxBurst+RECT
            [NxBurst]::GetWindowRect($h, [ref]$r) | Out-Null
            $w = $r.R - $r.L; $hh = $r.B - $r.T
            if ($w -ge 50 -and $hh -ge 50) {
                $bmp = New-Object System.Drawing.Bitmap $w, $hh
                $g = [System.Drawing.Graphics]::FromImage($bmp)
                $hdc = $g.GetHdc()
                [NxBurst]::PrintWindow($h, $hdc, 2) | Out-Null
                $g.ReleaseHdc($hdc)
                $file = Join-Path $Out ("{0}_{1}.png" -f $Name, $i)
                $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
                $bmp.Dispose()
                "saved $file at +$($sw.ElapsedMilliseconds)ms"
            }
        }
        if ($i -lt $Count - 1) { Start-Sleep -Milliseconds $IntervalMs }
    }
}
finally {
    # 내가 띄운 프로세스만 끝낸다(이름으로 고르지 않는다 — 사용자의 nexa-sql이 같이 떠 있을 수 있다).
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
