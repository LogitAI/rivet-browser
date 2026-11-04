. scripts/tests/appspike/spike-common.ps1
# Zero-address route: WM_COMMAND ids to the app window, window titles for the readout. No row value is read.
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class Cmd{
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr r);
}
'@
function Cmd-Id([int]$Id) { $r = [IntPtr]::Zero; [void][Cmd]::SendMessageTimeout($ctx.Hwnd, 0x111, [IntPtr]$Id, [IntPtr]::Zero, 2, 3000, [ref]$r) }
function Titles { (Show-Windows $ctx | ForEach-Object { ($_ -split ' ', 3)[2] -replace ' \d+,\d+,\d+,\d+$', '' }) -join ' || ' }
"start: $(Titles)"
$p = Start-Process -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $urlB) -PassThru -WindowStyle Hidden
$null = $p.WaitForExit(20000); Start-Sleep 5
"after hand-off to page-b: $(Titles)"
Cmd-Id 33000; Start-Sleep 3     # IDC_BACK
"IDC_BACK (33000): $(Titles)"
Cmd-Id 33001; Start-Sleep 3     # IDC_FORWARD
"IDC_FORWARD (33001): $(Titles)"
Cmd-Id 33002; Start-Sleep 3     # IDC_RELOAD
"IDC_RELOAD (33002): $(Titles)"
Cmd-Id 34014; Start-Sleep 4     # IDC_NEW_TAB
"IDC_NEW_TAB (34014): $(Titles)"
$null = Get-Capture $ctx (Join-Path $shots 's7-newtab.png')
Cmd-Id 34016; Start-Sleep 2     # IDC_SELECT_NEXT_TAB
"IDC_SELECT_NEXT_TAB (34016): $(Titles)"
Cmd-Id 34018; Start-Sleep 2     # IDC_SELECT_TAB_0
"IDC_SELECT_TAB_0 (34018): $(Titles)"
$enabled = @()
foreach ($id in 34014, 34015, 34016, 34017, 34018, 33000, 33001, 33002) { $enabled += $id }
"tab count by row-free route: not readable (an app window shows only the active tab)"
