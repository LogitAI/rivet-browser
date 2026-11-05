# Guards the owner's screen while browser tests run. Every visible top-level window of a test
# process counts: browser windows, dialogs, message boxes and owned popups alike. A test process is
# one whose command line names the test folder `-Mark`, or a descendant of such a process. When any
# such window touches a monitor, the guard logs it, stops every test process and every runner whose
# command line names the mark, writes `-FailFile` and exits 1.
#
#   pwsh -NoProfile -File scripts/tests/windows-visible-guard.ps1 -Mark <folder> -Log <file> -FailFile <file>
#   pwsh -NoProfile -File scripts/tests/windows-visible-guard.ps1 -SelfTest
#
# The owner's own browsers never match: their command lines do not name the test folder.
param(
  [string]$Mark,
  [string]$Log,
  [string]$FailFile,
  [switch]$SelfTest,
  # A dry run reports what it sees and stops nothing.
  [switch]$DryRun,
  [int]$Seconds = 0
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using System.Collections.Generic;
public static class GuardWin{
 public delegate bool EW(IntPtr h,IntPtr l);
 public delegate bool EM(IntPtr m,IntPtr dc,ref R r,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumWindows(EW f,IntPtr l);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc,IntPtr clip,EM f,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,System.Text.StringBuilder s,int n);
 public struct R{public int L,T,Ri,B;}
 // Every visible top-level window: handle, process, rectangle and class.
 public static List<string> All(){var o=new List<string>();EnumWindows((h,l)=>{if(IsWindowVisible(h)){uint q;GetWindowThreadProcessId(h,out q);R r;GetWindowRect(h,out r);var b=new System.Text.StringBuilder(128);GetClassName(h,b,128);o.Add(h+"|"+q+"|"+r.L+"|"+r.T+"|"+r.Ri+"|"+r.B+"|"+b);}return true;},IntPtr.Zero);return o;}
 // Every monitor in physical pixels, as the thread is per-monitor DPI aware.
 public static List<R> Monitors(){var o=new List<R>();EnumDisplayMonitors(IntPtr.Zero,IntPtr.Zero,(IntPtr m,IntPtr dc,ref R r,IntPtr l)=>{o.Add(r);return true;},IntPtr.Zero);return o;}
}
'@
[GuardWin]::SetThreadDpiAwarenessContext([IntPtr]-4) | Out-Null

function Test-OnMonitor([int[]]$Rect, $Monitors) {
  $l, $t, $r, $b = $Rect
  if ($r - $l -le 0 -or $b - $t -le 0) { return $false }
  foreach ($m in $Monitors) {
    if ($l -lt $m.Ri -and $r -gt $m.L -and $t -lt $m.B -and $b -gt $m.T) { return $true }
  }
  $false
}

if ($SelfTest) {
  $monitors = @([pscustomobject]@{ L = 0; T = 0; Ri = 1920; B = 1080 }, [pscustomobject]@{ L = 1920; T = 0; Ri = 3840; B = 1080 })
  $cases = @(
    @(@(498, 368, 900, 560), $true, 'a dialog in the middle of the first monitor'),
    @(@(9000, 1000, 10600, 2000), $false, 'a test window at the off-screen spot'),
    @(@(3800, 500, 4200, 700), $true, 'a window that only overlaps the second monitor edge'),
    @(@(-50, -50, 1, 1), $true, 'a window that touches the top left corner'),
    @(@(100, 100, 100, 300), $false, 'an empty window')
  )
  $failed = 0
  foreach ($case in $cases) {
    $got = Test-OnMonitor $case[0] $monitors
    $ok = $got -eq $case[1]
    if (-not $ok) { $failed++ }
    '{0} {1}: {2}' -f ($(if ($ok) { 'ok  ' } else { 'FAIL' })), $case[2], $got
  }
  $real = [GuardWin]::Monitors()
  "monitors on this machine: $($real.Count)"
  if ($real.Count -lt 1) { $failed++ }
  if ($failed) { "visible guard selftest: $failed failed"; exit 1 }
  'visible guard selftest: all passed'
  exit 0
}

if (-not $Mark -or -not $Log) { throw 'give -Mark and -Log' }
$markLower = $Mark.ToLowerInvariant().Replace('/', '\')
# Command lines name the folder with either slash and in any case.
function Test-Names([string]$CommandLine) { $CommandLine -and $CommandLine.ToLowerInvariant().Replace('/', '\').Contains($markLower) }
New-Item -ItemType File -Force $Log | Out-Null

# The test processes: every process whose command line names the mark, and their descendants.
function Get-TestProcesses {
  $all = @(Get-CimInstance Win32_Process)
  $test = New-Object 'System.Collections.Generic.HashSet[uint32]'
  foreach ($p in $all) {
    if ((Test-Names $p.CommandLine) -and $p.ProcessId -ne $PID) { [void]$test.Add([uint32]$p.ProcessId) }
  }
  # Children of test processes, such as a dialog helper started without the profile argument.
  do {
    $grew = $false
    foreach ($p in $all) {
      if (-not $test.Contains([uint32]$p.ProcessId) -and $test.Contains([uint32]$p.ParentProcessId) -and $p.Name -ne 'pwsh.exe' -and $p.Name -ne 'conhost.exe') {
        [void]$test.Add([uint32]$p.ProcessId); $grew = $true
      }
    }
  } while ($grew)
  @{ Ids = $test; All = $all }
}

function Stop-Tests($all) {
  foreach ($p in $all) {
    if ($p.ProcessId -eq $PID) { continue }
    if (Test-Names $p.CommandLine) { Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue }
  }
}

$start = Get-Date
$monitors = [GuardWin]::Monitors()
$tick = 0
$lastSeen = -1
$known = @{ Ids = (New-Object 'System.Collections.Generic.HashSet[uint32]'); All = @() }
while ($true) {
  # The process list is refreshed often, so a process started a moment ago is covered too.
  if ($tick % 5 -eq 0) { $known = Get-TestProcesses }
  $tick++
  $seen = 0
  foreach ($line in [GuardWin]::All()) {
    $p = $line -split '\|'
    if (-not $known.Ids.Contains([uint32]$p[1])) { continue }
    $seen++
    $rect = @([int]$p[2], [int]$p[3], [int]$p[4], [int]$p[5])
    if (Test-OnMonitor $rect $monitors) {
      Add-Content $Log "$(Get-Date -Format o) ONSCREEN window $($p[0]) pid $($p[1]) class $($p[6]) at $($rect -join ',')"
      if (-not $DryRun) {
        Stop-Tests (Get-CimInstance Win32_Process)
        if ($FailFile) { Set-Content $FailFile "a test window showed on a monitor: $line" }
        exit 1
      }
    }
  }
  if ($seen -ne $lastSeen) {
    Add-Content $Log "$(Get-Date -Format o) watching $($known.Ids.Count) test processes, $seen visible windows, none on a monitor"
    $lastSeen = $seen
  }
  if ($Seconds -gt 0 -and ((Get-Date) - $start).TotalSeconds -ge $Seconds) { exit 0 }
  Start-Sleep -Milliseconds 20
}
