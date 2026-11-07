# Guards the owner's screen, keyboard and mouse while browser tests run. A test process is one whose
# command line names the test folder `-Mark`, a descendant of such a process, or a descendant of the
# runner `-RunnerPid`. The run is stopped when any of these happens:
#
# - a visible top-level window of a test process touches a monitor: browser windows, dialogs,
#   message boxes and owned popups alike;
# - the foreground window belongs to a test process, so a test window took the keyboard;
# - the foreground thread's mouse capture, or its move or size loop, belongs to a test process, so a
#   test window holds the person's mouse.
#
# Stopping means: the runner and every process whose command line names the mark are stopped, the
# reason is written to `-FailFile` in the run's folder, and the guard exits 1. Nothing else is
# touched: the owner's own browsers never name the test folder.
#
# Every 250 ms the guard also logs the foreground window's process, the cursor position and any
# capture a test thread holds in the background, when they change. A cursor that moves is logged
# only, since the guard cannot tell the person's hand from anything else.
#
#   pwsh -NoProfile -File scripts/tests/windows-visible-guard.ps1 -Mark <folder> -Log <file> -FailFile <file> [-RunnerPid <pid>]
#   pwsh -NoProfile -File scripts/tests/windows-visible-guard.ps1 -SelfTest
param(
  [string]$Mark,
  [string]$Log,
  [string]$FailFile,
  # The runner that started this guard. Its descendants are test processes, it is stopped with
  # them, and the guard ends when it ends.
  [int]$RunnerPid = 0,
  [switch]$SelfTest,
  # A dry run reports what it sees and stops nothing.
  [switch]$DryRun,
  [int]$Seconds = 0
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-processes.ps1')
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using System.Collections.Generic;
public static class GuardWin{
 public delegate bool EW(IntPtr h,IntPtr l);
 public delegate bool EM(IntPtr m,IntPtr dc,ref R r,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumWindows(EW f,IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc,IntPtr clip,EM f,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] static extern bool GetCursorPos(out P p);
 [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint t,ref GUI g);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,System.Text.StringBuilder s,int n);
 public struct R{public int L,T,Ri,B;}
 public struct P{public int X,Y;}
 [StructLayout(LayoutKind.Sequential)] public struct GUI{public int cbSize;public uint flags;public IntPtr hwndActive,hwndFocus,hwndCapture,hwndMenuOwner,hwndMoveSize,hwndCaret;public R rcCaret;}
 // Every visible top-level window: handle, process, rectangle, class and thread.
 public static List<string> All(){var o=new List<string>();EnumWindows((h,l)=>{if(IsWindowVisible(h)){uint q;uint t=GetWindowThreadProcessId(h,out q);R r;GetWindowRect(h,out r);var b=new System.Text.StringBuilder(128);GetClassName(h,b,128);o.Add(h+"|"+q+"|"+r.L+"|"+r.T+"|"+r.Ri+"|"+r.B+"|"+b+"|"+t);}return true;},IntPtr.Zero);return o;}
 // Every monitor in physical pixels, as the thread is per-monitor DPI aware.
 public static List<R> Monitors(){var o=new List<R>();EnumDisplayMonitors(IntPtr.Zero,IntPtr.Zero,(IntPtr m,IntPtr dc,ref R r,IntPtr l)=>{o.Add(r);return true;},IntPtr.Zero);return o;}
 public static uint ProcessOf(IntPtr h){uint p=0;if(h!=IntPtr.Zero)GetWindowThreadProcessId(h,out p);return p;}
 public static string Cursor(){P p;return GetCursorPos(out p)?p.X+","+p.Y:"?";}
 // The input state of a thread, 0 for the foreground thread: its capture window, its move or size
 // window and whether it runs a move or size loop (GUI_INMOVESIZE).
 public static string Thread(uint t){var g=new GUI();g.cbSize=Marshal.SizeOf(typeof(GUI));if(!GetGUIThreadInfo(t,ref g))return null;return g.hwndCapture+"|"+g.hwndMoveSize+"|"+((g.flags&2)!=0?1:0)+"|"+g.hwndActive;}
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

# Whether `child` was started by `parent`. Windows reuses process ids, so a process whose parent
# died long ago can name a test process as its parent; a real parent is older than its child.
function Test-Child($child, $parent) {
  $parent -and $child.CreationDate -and $parent.CreationDate -and $parent.CreationDate -le $child.CreationDate
}

# The reason to stop the run from the foreground state, or $null. `Foreground` is the process of
# the foreground window, `Capture` and `MoveSize` the processes of the foreground thread's capture
# and move or size windows, `InMoveSize` whether that thread runs a move or size loop, and `Ids` the
# test processes.
function Get-InputBreach([uint32]$Foreground, [uint32]$Capture, [uint32]$MoveSize, [bool]$InMoveSize, $Ids) {
  if ($Capture -and $Ids.Contains($Capture)) { return "test process $Capture holds the mouse capture" }
  if ($InMoveSize -and $MoveSize -and $Ids.Contains($MoveSize)) { return "test process $MoveSize runs a move or size loop on the mouse" }
  if ($Foreground -and $Ids.Contains($Foreground)) { return "the foreground window belongs to test process $Foreground" }
  $null
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
  $old = [pscustomobject]@{ CreationDate = [datetime]'2026-11-05 08:00' }
  $new = [pscustomobject]@{ CreationDate = [datetime]'2026-11-05 10:00' }
  foreach ($case in @(@($new, $old, $true, 'a child started by a test process'), @($old, $new, $false, 'a process whose dead parent id a test process took over'), @($new, $null, $false, 'a process whose parent is gone'))) {
    $got = [bool](Test-Child $case[0] $case[1])
    $ok = $got -eq $case[2]
    if (-not $ok) { $failed++ }
    '{0} {1}: {2}' -f ($(if ($ok) { 'ok  ' } else { 'FAIL' })), $case[3], $got
  }
  $ids = New-Object 'System.Collections.Generic.HashSet[uint32]'
  [void]$ids.Add(4242)
  $inputCases = @(
    @(4242, 0, 0, $false, $true, 'nfr5: a test window in the foreground stops the run'),
    @(77, 4242, 0, $false, $true, 'nfr5: a test window holding the mouse capture stops the run'),
    @(77, 0, 4242, $true, $true, 'nfr5: a test window in a move loop stops the run'),
    @(77, 0, 4242, $false, $false, 'a move window without a running loop is no breach'),
    @(77, 88, 0, $false, $false, 'the owner''s own foreground and capture are no breach'),
    @(0, 0, 0, $false, $false, 'no foreground window is no breach')
  )
  foreach ($case in $inputCases) {
    $got = [bool](Get-InputBreach $case[0] $case[1] $case[2] $case[3] $ids)
    $ok = $got -eq $case[4]
    if (-not $ok) { $failed++ }
    '{0} {1}: {2}' -f ($(if ($ok) { 'ok  ' } else { 'FAIL' })), $case[5], $got
  }
  # The process list without WMI must see this guard with its command line, start time and parent.
  $me = @(Get-ProcessList) | Where-Object { $_.ProcessId -eq $PID } | Select-Object -First 1
  $ok = [bool]($me -and $me.CommandLine -and $me.CommandLine.Contains('windows-visible-guard') -and $me.CreationDate -and $me.ParentProcessId)
  if (-not $ok) { $failed++ }
  '{0} the process list names this guard with its command line: {1}' -f ($(if ($ok) { 'ok  ' } else { 'FAIL' })), $ok
  $real = [GuardWin]::Monitors()
  "monitors on this machine: $($real.Count)"
  if ($real.Count -lt 1) { $failed++ }
  $ok = $null -ne [GuardWin]::Thread(0) -or $null -eq [GuardWin]::GetForegroundWindow()
  if (-not $ok) { $failed++ }
  '{0} the foreground thread''s input state can be read: {1}' -f ($(if ($ok) { 'ok  ' } else { 'FAIL' })), $ok
  if ($failed) { "visible guard selftest: $failed failed"; exit 1 }
  'visible guard selftest: all passed'
  exit 0
}

if (-not $Mark -or -not $Log) { throw 'give -Mark and -Log' }
$markLower = $Mark.ToLowerInvariant().Replace('/', '\')
# Command lines name the folder with either slash and in any case.
function Test-Names([string]$CommandLine) { $CommandLine -and $CommandLine.ToLowerInvariant().Replace('/', '\').Contains($markLower) }
New-Item -ItemType File -Force $Log | Out-Null
$runner = if ($RunnerPid) { Get-Process -Id $RunnerPid -ErrorAction SilentlyContinue } else { $null }
$runnerStart = if ($runner) { $runner.StartTime } else { $null }

# The test processes: every process whose command line names the mark, and their descendants, and
# the runner's descendants. Other consoles and this guard are not test processes.
function Get-TestProcesses {
  $all = @(Get-ProcessList)
  $test = New-Object 'System.Collections.Generic.HashSet[uint32]'
  $byId = @{}
  foreach ($p in $all) { $byId[[uint32]$p.ProcessId] = $p }
  # A runner script is a root but not itself a test process: a console window can report the shell
  # as its owner, and the person may be typing in that console.
  $roots = New-Object 'System.Collections.Generic.HashSet[uint32]'
  if ($RunnerPid) { [void]$roots.Add([uint32]$RunnerPid) }
  foreach ($p in $all) {
    if ((Test-Names $p.CommandLine) -and $p.ProcessId -ne $PID) {
      [void]$roots.Add([uint32]$p.ProcessId)
      if ($p.Name -ne 'pwsh.exe') { [void]$test.Add([uint32]$p.ProcessId) }
    }
  }
  # Children of test processes, such as a dialog helper started without the profile argument.
  do {
    $grew = $false
    foreach ($p in $all) {
      if (-not $test.Contains([uint32]$p.ProcessId) -and $p.ProcessId -ne $PID -and $roots.Contains([uint32]$p.ParentProcessId) -and $p.Name -ne 'pwsh.exe' -and $p.Name -ne 'conhost.exe' -and (Test-Child $p $byId[[uint32]$p.ParentProcessId])) {
        [void]$test.Add([uint32]$p.ProcessId); [void]$roots.Add([uint32]$p.ProcessId); $grew = $true
      }
    }
  } while ($grew)
  @{ Ids = $test; All = $all }
}

# Stops this run only: its runner, the processes that name its folder and the test processes found.
function Stop-Tests($ids) {
  if ($RunnerPid -and $runner -and -not $runner.HasExited) { Stop-Process -Id $RunnerPid -Force -ErrorAction SilentlyContinue }
  foreach ($p in Get-ProcessList) {
    if ($p.ProcessId -eq $PID) { continue }
    if ((Test-Names $p.CommandLine) -or ($ids -and $ids.Contains([uint32]$p.ProcessId))) { Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue }
  }
}

function Stop-Run([string]$Reason, $Ids) {
  Add-Content $Log "$(Get-Date -Format o) STOP $Reason"
  if ($DryRun) { return }
  Stop-Tests $Ids
  if ($FailFile) { Set-Content $FailFile $Reason }
  exit 1
}

$start = Get-Date
$monitors = [GuardWin]::Monitors()
$tick = 0
$lastSeen = -1
$lastState = ''
$lastCursor = [GuardWin]::Cursor()
$lastBackground = ''
$known = @{ Ids = (New-Object 'System.Collections.Generic.HashSet[uint32]'); All = @() }
while ($true) {
  # The process list is refreshed often, so a process started a moment ago is covered too.
  if ($tick % 5 -eq 0) { $known = Get-TestProcesses }
  $tick++
  $seen = 0
  $threads = New-Object 'System.Collections.Generic.HashSet[uint32]'
  foreach ($line in [GuardWin]::All()) {
    $p = $line -split '\|'
    if (-not $known.Ids.Contains([uint32]$p[1])) { continue }
    $seen++
    [void]$threads.Add([uint32]$p[7])
    $rect = @([int]$p[2], [int]$p[3], [int]$p[4], [int]$p[5])
    if (Test-OnMonitor $rect $monitors) {
      Stop-Run "a test window showed on a monitor: window $($p[0]) pid $($p[1]) class $($p[6]) at $($rect -join ',')" $known.Ids
    }
  }
  # The foreground thread's state: who has the keyboard and who holds the mouse.
  $foreground = [GuardWin]::GetForegroundWindow()
  $fgPid = [GuardWin]::ProcessOf($foreground)
  $fgThread = [GuardWin]::Thread(0)
  $capPid = 0; $movePid = 0; $inMove = $false
  if ($fgThread) {
    $f = $fgThread -split '\|'
    $capPid = [GuardWin]::ProcessOf([IntPtr][long]$f[0])
    $movePid = [GuardWin]::ProcessOf([IntPtr][long]$f[1])
    $inMove = $f[2] -eq '1'
  }
  $breach = Get-InputBreach $fgPid $capPid $movePid $inMove $known.Ids
  if ($breach) { Stop-Run "$breach (foreground window $foreground)" $known.Ids }
  if ($tick % 12 -eq 0) {
    # About every 250 ms: the foreground process, the cursor and any background capture of a test
    # thread, logged when they change.
    $state = "foreground pid $fgPid capture pid $capPid"
    if ($state -ne $lastState) { Add-Content $Log "$(Get-Date -Format o) $state"; $lastState = $state }
    $cursor = [GuardWin]::Cursor()
    if ($cursor -ne $lastCursor) { Add-Content $Log "$(Get-Date -Format o) cursor moved from $lastCursor to $cursor"; $lastCursor = $cursor }
    $held = @()
    foreach ($t in $threads) {
      $info = [GuardWin]::Thread($t)
      if ($info -and ($info -split '\|')[0] -ne '0') { $held += "thread $t capture $(($info -split '\|')[0])" }
    }
    $background = $held -join '; '
    if ($background -ne $lastBackground) {
      Add-Content $Log "$(Get-Date -Format o) background capture of test threads: $(if ($background) { $background } else { 'none' })"
      $lastBackground = $background
    }
    if ($runner -and ($runner.HasExited -or (Get-Process -Id $RunnerPid -ErrorAction SilentlyContinue).StartTime -ne $runnerStart)) {
      Add-Content $Log "$(Get-Date -Format o) the runner ended"
      exit 0
    }
  }
  if ($seen -ne $lastSeen) {
    Add-Content $Log "$(Get-Date -Format o) watching $($known.Ids.Count) test processes, $seen visible windows, none on a monitor"
    $lastSeen = $seen
  }
  if ($Seconds -gt 0 -and ((Get-Date) - $start).TotalSeconds -ge $Seconds) { exit 0 }
  Start-Sleep -Milliseconds 20
}
