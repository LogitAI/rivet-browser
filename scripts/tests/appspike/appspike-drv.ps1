# Drives one browser in an app window and runs a scenario script: pwsh -File scripts/tests/appspike/appspike-drv.ps1 -Name chrome -Script scripts/tests/appspike/s4.ps1
# SPIKE_FILE='detach RVA;insert RVA' writes the spike addresses, SPIKE_EARLY=1 loads the runtime before the first window.
param([string]$Name = 'chrome', [string]$Script = '', [string]$Url = '')
# The whole run, and every browser it starts, runs on a desktop of its own.
. (Join-Path $PSScriptRoot '..\windows-test-desktop.ps1')
$desktopCode = Invoke-OnOwnDesktop $PSCommandPath $PSBoundParameters
if ($null -ne $desktopCode) { exit $desktopCode }
$env:SPIKE_OUT = Join-Path (Get-Location) '.scratch\spike'
. (Join-Path $PSScriptRoot 'appspike-lib.ps1')
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using System.Text;using System.Collections.Generic;
public struct RR{public int L,T,Ri,B;}
public static class SpikeWin{
 public delegate bool EW(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumWindows(EW f,IntPtr l);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out RR r);
 public static List<string> All(HashSet<uint> pids){
  var o=new List<string>();
  EnumWindows((h,l)=>{uint q;GetWindowThreadProcessId(h,out q);if(pids.Contains(q)&&IsWindowVisible(h)){var b=new StringBuilder(64);GetClassName(h,b,64);var t=new StringBuilder(128);GetWindowText(h,t,128);RR r;GetWindowRect(h,out r);o.Add(h.ToInt64().ToString("x")+" "+b+" ["+t+"] "+r.L+","+r.T+","+r.Ri+","+r.B);}return true;},IntPtr.Zero);
  return o;
 }
}
'@
function Show-Windows($ctx) {
  $pids = New-Object 'System.Collections.Generic.HashSet[uint32]'
  foreach ($q in Get-MyProcesses $ctx.ExeName $ctx.Profile) { [void]$pids.Add([uint32]$q.ProcessId) }
  [SpikeWin]::All($pids) | Where-Object { $_ -match 'Chrome_WidgetWin_1' }
}
function Log-Tail([string]$Pattern) { Read-Log | Where-Object { $_ -match $Pattern } | Select-Object -Last 3 | ForEach-Object { $_.Substring(0, [Math]::Min(300, $_.Length)) } }
$ctx, $state = Start-Scene $Name 'drv' $urlA
$shots = Join-Path $OutDir $Name
New-Item -ItemType Directory -Force $shots | Out-Null
try {
  $u = if ($Url) { $Url } else { $urlA }
  if ($env:SPIKE_FILE) { Set-Content (Join-Path $flagDir 'rivet-spike.txt') ($env:SPIKE_FILE -split ';') }
  if ($env:SPIKE_EARLY) { $env:RIVET_SPIKE_EARLY = '1' } else { Remove-Item Env:RIVET_SPIKE_EARLY -ErrorAction SilentlyContinue }
  Open-Scene $ctx $state $u $null
  "error: $($ctx.Error)  dpi: $($ctx.Dpi)"
  Show-Windows $ctx
  $null = Get-Capture $ctx (Join-Path $shots 'drv-0.png')
  if ($Script) { . $Script }
} finally { Stop-Scene $ctx }

