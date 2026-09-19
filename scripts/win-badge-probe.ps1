# win-badge-probe.ps1 — 툴바 Disconnect 배지 실기 자동화(사용자 09-19 "탭이 추가돼도 숫자가 안 는다"): Demo 접속으로 띄운 뒤 Ctrl+N으로 탭을 늘리며 툴바 Disconnect 배지를 캡처(×3 크롭).
param([string]$Exe = "target\debug\nexa-sql.exe", [string]$Out = "target\capture")
exa-sql.exe", [string]$Out = "targetpture")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public class NxB {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static void Click(int x, int y) { SetCursorPos(x, y); mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
  public static IntPtr FindByPid(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { if (!IsWindowVisible(h)) return true; uint p; GetWindowThreadProcessId(h, out p);
      var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256);
      if (p == pid && sb.ToString().Contains("Nexa SQL")) { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
  }
}
"@
[NxB]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null
function Shot([IntPtr]$h, [string]$file) {
    $r = New-Object NxB+RECT
    [NxB]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.R - $r.L; $hh = $r.B - $r.T
    $bmp = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [NxB]::PrintWindow($h, $hdc, 2) | Out-Null
    $g.ReleaseHdc($hdc)
    # 툴바 왼쪽 620×70 을 ×3 확대
    $dst = New-Object System.Drawing.Bitmap (620 * 3), (70 * 3)
    $g2 = [System.Drawing.Graphics]::FromImage($dst)
    $g2.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
    $g2.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
    $g2.DrawImage($bmp, (New-Object System.Drawing.Rectangle 0, 0, (620 * 3), (70 * 3)), (New-Object System.Drawing.Rectangle 0, 60, 620, 70), [System.Drawing.GraphicsUnit]::Pixel)
    $dst.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose(); $dst.Dispose()
    "saved $file"
}
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$p = Start-Process -FilePath $Exe -ArgumentList "Demo" -WorkingDirectory (Split-Path $Exe) -PassThru
Start-Sleep -Seconds 4
$h = [NxB]::FindByPid($p.Id)
if ($h -eq [IntPtr]::Zero) { "no window"; exit 1 }
[NxB]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 800
$r = New-Object NxB+RECT
[NxB]::GetWindowRect($h, [ref]$r) | Out-Null
[NxB]::Click($r.L + [int](($r.R - $r.L) * 0.6), $r.T + 200)
Start-Sleep -Milliseconds 500
Shot $h "$Out\badge_1tab.png"
[System.Windows.Forms.SendKeys]::SendWait("^n")
Start-Sleep -Milliseconds 700
Shot $h "$Out\badge_2tabs.png"
[System.Windows.Forms.SendKeys]::SendWait("^n")
Start-Sleep -Milliseconds 700
Shot $h "$Out\badge_3tabs.png"
[System.Windows.Forms.SendKeys]::SendWait("^w")
Start-Sleep -Milliseconds 700
Shot $h "$Out\badge_after_close.png"
Stop-Process -Id $p.Id -Force
