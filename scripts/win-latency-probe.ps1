# win-latency-probe.ps1 — 편집기 **입력→화면 지연 실측**(Windows · 사용자 09-19 "↑로 1열 이동이 오래 걸린다" · docs/55 §6).
#   GUI를 `NSQL_TRACE_FRAMES=1`로 Demo 프로필에 접속해 띄우고(로그인 창 없이) 편집기를 클릭한 뒤 SendKeys로
#   본문 입력 → End → ↑ …를 보내 stderr의 `[frames] input→present … · key · route · editor on_event · request_redraw ·
#   RedrawRequested · paint begin` 줄(지점별 ms)을 읽는다.
# 사용:  pwsh -File scripts\win-latency-probe.ps1 [-Exe target\release\nexa-sql.exe]   (기본 Debug exe · 로그 = %TEMP%\nsql-frames.log)
#   ★ 지연 계측은 **메모리에 모아 present 뒤 한 줄**로 본다 — 지점마다 eprintln을 하면 파이프 쓰기가 지점당 1~4 ms를 만들어
#     결과를 왜곡한다(09-19 실측: 같은 경로가 10 ms → 0.01 ms).
#   ★ 로그인 창(모달)이 열려 있으면 메인 창 입력이 버려져 500 ms(깜빡임 타이머)마다만 그려진 것처럼 보인다 — 그래서 `Demo` 인자로 띄운다.
param([string]$Exe = "target\debug\nexa-sql.exe", [string]$Log = "$env:TEMP\nsql-frames.log")
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public class NxWin {
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
if (Test-Path $Log) { Remove-Item $Log }
$Exe = (Resolve-Path $Exe).Path
$env:NSQL_TRACE_FRAMES = "1"
$p = Start-Process -FilePath $Exe -ArgumentList "Demo" -WorkingDirectory (Split-Path $Exe) -PassThru -RedirectStandardError $Log
Start-Sleep -Seconds 4
$h = [NxWin]::FindByPid($p.Id)
if ($h -eq [IntPtr]::Zero) { "no window"; exit 1 }
[NxWin]::SetProcessDPIAware() | Out-Null
[NxWin]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 800
$r = New-Object NxWin+RECT
[NxWin]::GetWindowRect($h, [ref]$r) | Out-Null
"window $($r.L),$($r.T)-$($r.R),$($r.B)"
# 편집기 본문 클릭(탐색기 오른쪽 · 툴바 아래)
[NxWin]::Click($r.L + [int](($r.R - $r.L) * 0.6), $r.T + 200)
Start-Sleep -Milliseconds 500
# 새 탭에 본문 입력(괄호 자동 닫기 고려: 여는 괄호만 치면 닫힘이 생기므로 End로 넘어간다)
[System.Windows.Forms.SendKeys]::SendWait("BEGIN RAISE_APPLICATION_ERROR{(}-20000, 'Invalid execution'")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("{END}{ENTER}/")
Start-Sleep -Milliseconds 800
"--- probe: Up x1 (line2 -> line1)"
[System.Windows.Forms.SendKeys]::SendWait("{UP}")
Start-Sleep -Milliseconds 700
[System.Windows.Forms.SendKeys]::SendWait("{END}")
Start-Sleep -Milliseconds 700
"--- probe: Up at line1 end -> col1"
[System.Windows.Forms.SendKeys]::SendWait("{UP}")
Start-Sleep -Milliseconds 700
[System.Windows.Forms.SendKeys]::SendWait("{END}")
Start-Sleep -Milliseconds 700
[System.Windows.Forms.SendKeys]::SendWait("{UP}")
Start-Sleep -Milliseconds 700
[System.Windows.Forms.SendKeys]::SendWait("{RIGHT}{RIGHT}{LEFT}")
Start-Sleep -Seconds 1
Stop-Process -Id $p.Id -Force
Start-Sleep -Milliseconds 500
Get-Content $Log | Select-String "input→present" | Select-Object -Last 20
