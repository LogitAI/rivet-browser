. scripts/tests/appspike/spike-common.ps1
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class Cmd2{
 [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr r);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h,System.Text.StringBuilder s,int n);
}
'@
function Cmd-Id([int]$Id) { $r = [IntPtr]::Zero; [void][Cmd2]::SendMessageTimeout($ctx.Hwnd, 0x111, [IntPtr]$Id, [IntPtr]::Zero, 2, 3000, [ref]$r) }
function AppTitle { $buf = New-Object Text.StringBuilder 200; [void][Cmd2]::GetWindowText($ctx.Hwnd, $buf, 200); $buf.ToString() }
function Others { (Show-Windows $ctx | Where-Object { $_ -notmatch "^$([Convert]::ToString([int64]$ctx.Hwnd,16)) " } | ForEach-Object { ($_ -split ' ', 3)[2] -replace ' \d+,\d+,\d+,\d+$', '' }) -join ' || ' }
$a = Join-Path $pageDir 'nav-a.html'; $b = Join-Path $pageDir 'nav-b.html'
[IO.File]::WriteAllText($a, '<!doctype html><title>nav-a</title><script>if(!sessionStorage.moved){sessionStorage.moved=1;setTimeout(()=>{location.href="nav-b.html"},1200)}</script>')
[IO.File]::WriteAllText($b, '<!doctype html><title>nav-b</title><script>document.title="nav-b "+(Date.now()%100000)</script>')
Send-Flag $ctx 'rivet-tab.txt' ('navigate file:///' + ($a -replace '\\', '/')) | Out-Null
Start-Sleep 6
"after nav-a -> nav-b link: app title [$(AppTitle)]"
Cmd-Id 33000; Start-Sleep 3; "IDC_BACK 33000: [$(AppTitle)]"
Cmd-Id 33001; Start-Sleep 3; "IDC_FORWARD 33001: [$(AppTitle)]"
Cmd-Id 33002; Start-Sleep 3; "IDC_RELOAD 33002: [$(AppTitle)]"
Cmd-Id 34014; Start-Sleep 4; "IDC_NEW_TAB 34014: app [$(AppTitle)] other windows [$(Others)]"
if (Test-Path (Join-Path $flagDir 'rivet-spike.txt')) {
  Send-Flag $ctx 'rivet-tab.txt' 'move' | Out-Null; Start-Sleep 3
  Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null
  "after moving the new tab in: tabs $((Get-TabUrls) -join ' | '); app [$(AppTitle)]; others [$(Others)]"
  Cmd-Id 34018; Start-Sleep 2; "IDC_SELECT_TAB_0 34018: [$(AppTitle)]"
  Cmd-Id 34019; Start-Sleep 2; "IDC_SELECT_TAB_1 34019: [$(AppTitle)]"
  Cmd-Id 34017; Start-Sleep 2; "IDC_SELECT_PREVIOUS_TAB 34017: [$(AppTitle)]"
  Cmd-Id 34016; Start-Sleep 2; "IDC_SELECT_NEXT_TAB 34016: [$(AppTitle)]"
  Cmd-Id 34015; Start-Sleep 3
  Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null
  "IDC_CLOSE_TAB 34015: tabs $((Get-TabUrls) -join ' | '); app [$(AppTitle)]"
} else { "no second tab can be placed in this browser's app window (no tab strip model addresses)" }

