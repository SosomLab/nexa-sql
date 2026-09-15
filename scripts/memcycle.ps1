# 파일 대화상자 열기/닫기 N회 반복 · 프로세스 자원(Private · 핸들 · GDI · USER · 스레드) 표 — docs/37 §6-5 (09-15 누수 점검).
# 사용: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/memcycle.ps1 [-Exe path] [-Cycles 5] [-SettleMs 2500]
# 주의: 실행 중 다른 창을 만지지 말 것(SendKeys는 활성 창에 간다 · AppActivate 실패 시 중단).
param([string]$Exe = (Join-Path $PSScriptRoot "..\target\release\nexa-sql.exe"), [int]$Cycles = 5, [int]$SettleMs = 2500)
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Gui { [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr h, uint flags); }
"@
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 6
function Snap($tag) {
  $q = Get-Process -Id $p.Id
  $gdi = [Gui]::GetGuiResources($q.Handle, 0)
  $usr = [Gui]::GetGuiResources($q.Handle, 1)
  "{0,-10} priv={1,8:N0} KB  ws={2,8:N0} KB  handles={3,4}  gdi={4,3}  user={5,3}  threads={6,2}" -f $tag, ($q.PrivateMemorySize64/1KB), ($q.WorkingSet64/1KB), $q.HandleCount, $gdi, $usr, $q.Threads.Count
}
$wsh = New-Object -ComObject WScript.Shell
$null = $wsh.AppActivate($p.Id)
Start-Sleep -Milliseconds 800
Snap "start"
for ($i = 1; $i -le $Cycles; $i++) {
  if (-not $wsh.AppActivate($p.Id)) { "activate failed at cycle $i - abort"; break }
  Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait("^o")
  Start-Sleep -Milliseconds $SettleMs
  Snap "open$i"
  [System.Windows.Forms.SendKeys]::SendWait("{ESC}")
  Start-Sleep -Milliseconds $SettleMs
  Snap "closed$i"
}
Start-Sleep -Seconds 3
Snap "final"
Stop-Process -Id $p.Id -Force
