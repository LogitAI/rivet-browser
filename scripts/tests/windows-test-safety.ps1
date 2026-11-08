# The safety of one browser test run, shared by every runner that starts a browser.
#
#   . scripts/tests/windows-test-safety.ps1
#   $safety = Start-RunSafety -OutDir $OutDir -Name features [-Alone]
#   Assert-RunSafe $safety      # before each step; exits when the guard stopped the run
#   Stop-RunSafety $safety      # at the end
#
# Start-RunSafety does four things before any browser starts:
#
# 1. Sets RIVET_KEEP_OFFSCREEN for the run and everything it starts. Under it the Rivet runtime keeps
#    every top-level window of the browser off every monitor and never activates one, the setup
#    window shows off-screen without activation, and nothing brings a window forward.
# 2. Sets RIVET_DATA_ROOT to the run's own folder, so the sessions, spaces and tab lists the run
#    makes never land in the person's own Rivet folder. The applied signed build table is copied in,
#    so the runtime sees the same rows an installed build would.
# 3. Registers the run below .scratch\runs. A run that must be alone, such as one with the
#    open-link rows, is refused while another run is active, and no run starts beside it.
# 4. Starts windows-visible-guard.ps1 on the run's folder. The guard stops this run, and only this
#    run, when a test window shows on a monitor, takes the foreground or holds the mouse. It also
#    takes the foreground lock for the run, when Windows lets it, and gives it back as it ends.
#
# Under RIVET_KEEP_OFFSCREEN `rivet session start` starts the browser with STARTF_USESHOWWINDOW and
# SW_SHOWMINNOACTIVE and without --window-position or --window-size. Those switches give the window
# Chromium's normal show state, which it shows with SW_SHOWNORMAL and then activates whatever the
# startup show command says. With the default show state Chromium shows its first window with the
# startup command, minimized and inactive, so it lies on no monitor until the runtime attaches and
# restores it off every monitor at 1600x1000. No test runner may add either switch to a browser that
# shows a window. A runner that starts a browser itself, for a hand-off to a running test browser or
# a headless capture, uses Start-TestBrowser below, which starts it without activation:
# Start-Process with -WindowStyle Hidden hands the browser SW_HIDE, which Chromium turns into
# SW_SHOWNORMAL followed by SetForegroundWindow. A hand-off's window is made by the running browser,
# which shows it with its own startup command.
#
# Not done: starting test browsers on a desktop of their own (CreateDesktop and
# STARTUPINFO.lpDesktop) would keep them from the person's desktop entirely, but the guard sees only
# the input desktop, and whether PrintWindow with PW_RENDERFULLCONTENT returns the pages of a
# GPU-composited Chromium window on a desktop that is never shown is not known. It stays an option.

$script:SafetyRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Get-RunRegistry { Join-Path $script:SafetyRepo '.scratch\runs' }

. (Join-Path $PSScriptRoot 'windows-test-desktop.ps1')

# The held copies of earlier browser builds are Rivet's own. An installed Rivet opens a browser
# whose new build has no row yet from its held copy, so a run's Rivet folder `$Root` sees the same
# copies, through a junction rather than a copy of whole browsers.
function Add-HeldBrowsers([string]$Root) {
  if (-not $env:LOCALAPPDATA) { return }
  $held = Join-Path $env:LOCALAPPDATA 'Rivet\browsers'
  $link = Join-Path $Root 'Rivet\browsers'
  if ((Test-Path $held) -and -not (Test-Path $link)) {
    New-Item -ItemType Directory -Force (Split-Path $link) | Out-Null
    New-Item -ItemType Junction -Path $link -Target $held | Out-Null
  }
}

# Removes the junction Add-HeldBrowsers made in `$Root`, and only the junction, before the folder
# is removed, so no removal can reach the held browsers themselves.
function Remove-HeldBrowsers([string]$Root) {
  $link = Join-Path $Root 'Rivet\browsers'
  $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
  if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { [IO.Directory]::Delete($link, $false) }
}

# The runs registered and still running, each with its process id, start time, folder and whether
# it must be alone. A record whose process is gone, or whose id another process took, is removed.
function Get-ActiveRuns {
  $dir = Get-RunRegistry
  if (-not (Test-Path $dir)) { return @() }
  $live = @()
  foreach ($file in Get-ChildItem $dir -Filter '*.json' -ErrorAction SilentlyContinue) {
    $record = $null
    try { $record = Get-Content -Raw $file.FullName | ConvertFrom-Json } catch { }
    $proc = if ($record) { Get-Process -Id ([int]$record.pid) -ErrorAction SilentlyContinue } else { $null }
    if ($proc -and $proc.StartTime.ToUniversalTime().Ticks -eq [long]$record.start) { $live += $record }
    else { Remove-Item $file.FullName -ErrorAction SilentlyContinue }
  }
  $live
}

# Whether a run that `wantsAlone` may start beside the `active` runs, and why not.
function Test-RunMayStart([bool]$WantsAlone, $Active) {
  $others = @($Active | Where-Object { $_ })
  # A record written by an older runner holds the switch itself, {"IsPresent": ...}.
  $alone = @($others | Where-Object { $_.alone -eq $true -or $_.alone.IsPresent -eq $true })
  if ($alone.Count) { return @($false, "the run in $($alone[0].out) (pid $($alone[0].pid)) must run alone") }
  if ($WantsAlone -and $others.Count) { return @($false, "$($others.Count) other run(s) are active, the first in $($others[0].out) (pid $($others[0].pid))") }
  @($true, '')
}

function Start-RunSafety {
  param(
    [Parameter(Mandatory = $true)][string]$OutDir,
    [Parameter(Mandatory = $true)][string]$Name,
    # The run holds rows that must not run beside another run.
    [switch]$Alone,
    # When the run cannot be alone, it starts without those rows instead of not at all.
    [switch]$DropAloneRows
  )
  # A run on a test desktop keeps RIVET_KEEP_OFFSCREEN too: without it `rivet session start` waits
  # on that desktop and never returns, so the switch stays until that is understood.
  $env:RIVET_KEEP_OFFSCREEN = '1'
  $dataRoot = Join-Path $OutDir 'rivet-data'
  New-Item -ItemType Directory -Force $dataRoot | Out-Null
  $env:RIVET_DATA_ROOT = $dataRoot
  # The signed build table is Rivet's own cache, not browser data. An installed build would read it.
  if ($env:LOCALAPPDATA) {
    $table = Join-Path $env:LOCALAPPDATA 'Rivet\cache\builds\table.bin'
    $copy = Join-Path $dataRoot 'Rivet\cache\builds'
    if (Test-Path $table) {
      New-Item -ItemType Directory -Force $copy | Out-Null
      Copy-Item $table (Join-Path $copy 'table.bin') -Force
    }
    Add-HeldBrowsers $dataRoot
  }

  # The registry is read and written under one lock, so two runs starting together see each other.
  $lock = New-Object System.Threading.Mutex($false, 'Local\RivetTestRunRegistry')
  [void]$lock.WaitOne()
  # PowerShell names are not case sensitive: a variable named like the switch would stay a switch.
  $runAlone = [bool]$Alone
  $note = ''
  try {
    $ok, $why = Test-RunMayStart $runAlone (Get-ActiveRuns)
    if (-not $ok -and $runAlone -and $DropAloneRows) {
      $ok2, $why2 = Test-RunMayStart $false (Get-ActiveRuns)
      if ($ok2) { $note = $why; $runAlone = $false; $ok = $true }
      else { $why = $why2 }
    }
    if (-not $ok) {
      Write-Host "refusing to start the $Name run: $why"
      exit 2
    }
    $dir = Get-RunRegistry
    New-Item -ItemType Directory -Force $dir | Out-Null
    $me = Get-Process -Id $PID
    $record = Join-Path $dir "$PID.json"
    @{ pid = $PID; start = $me.StartTime.ToUniversalTime().Ticks; out = $OutDir; name = $Name; alone = $runAlone } |
      ConvertTo-Json -Compress | Set-Content -Path $record -Encoding utf8
  } finally { $lock.ReleaseMutex(); $lock.Dispose() }

  $log = Join-Path $OutDir 'guard.log'
  $fail = Join-Path $OutDir 'guard-failed.txt'
  $stop = Join-Path $OutDir 'guard-stop.txt'
  Remove-Item $log, $fail, $stop -ErrorAction SilentlyContinue
  $guardArgs = @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'windows-visible-guard.ps1'), '-Mark', $OutDir, '-Log', $log, '-FailFile', $fail, '-StopFile', $stop, '-RunnerPid', $PID)
  # A run on a test desktop of its own (windows-test-desktop.ps1) puts its guard on the person's
  # desktop, where no window of a test process may appear at all.
  $guard = if ($env:RIVET_TEST_DESKTOP) {
    Start-OnDefaultDesktop (Get-Process -Id $PID).Path ($guardArgs + '-TestDesktop')
  } else {
    Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList $guardArgs -WindowStyle Hidden -PassThru
  }
  # No browser starts before the guard watches.
  foreach ($i in 1..100) { if (Test-Path $log) { break }; Start-Sleep -Milliseconds 100 }
  if (-not (Test-Path $log)) {
    Stop-Process -Id $guard.Id -Force -ErrorAction SilentlyContinue
    Remove-Item $record -ErrorAction SilentlyContinue
    Write-Host "refusing to start the $Name run: the visible guard did not start"
    exit 2
  }
  [pscustomobject]@{ Guard = $guard; Log = $log; FailFile = $fail; StopFile = $stop; Record = $record; DataRoot = $dataRoot; Alone = $runAlone; AloneRefused = $note }
}

# Stops the run when the guard found a breach or is no longer watching. The guard has already
# stopped the run's test processes; this ends the runner before it starts another browser.
function Assert-RunSafe($Safety) {
  if (-not $Safety) { return }
  $why = $null
  if (Test-Path $Safety.FailFile) { $why = Get-Content -Raw $Safety.FailFile }
  elseif ($Safety.Guard.HasExited) { $why = "the visible guard exited with $($Safety.Guard.ExitCode)" }
  if ($why) {
    Write-Host "run stopped by the visible guard: $why"
    Remove-Item $Safety.Record -ErrorAction SilentlyContinue
    exit 3
  }
}

function Stop-RunSafety($Safety) {
  if (-not $Safety) { return }
  Assert-RunSafe $Safety
  # The guard ends on its own when asked, so it gives the foreground lock back. It is stopped only
  # when it does not end in time.
  Set-Content -Path $Safety.StopFile -Value 'stop'
  if (-not $Safety.Guard.WaitForExit(3000)) { Stop-Process -Id $Safety.Guard.Id -Force -ErrorAction SilentlyContinue }
  Remove-Item $Safety.Record, $Safety.StopFile -ErrorAction SilentlyContinue
}

if (-not ('RivetTestStart' -as [type])) {
  Add-Type -TypeDefinition @'
using System;using System.IO;using System.Runtime.InteropServices;
public sealed class RivetTestProcess:IDisposable{
 [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h,uint ms);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 IntPtr handle;public readonly int Id;
 public RivetTestProcess(IntPtr h,int id){handle=h;Id=id;}
 // Whether the process ended within `ms` milliseconds.
 public bool WaitForExit(int ms){return handle==IntPtr.Zero||WaitForSingleObject(handle,(uint)ms)==0;}
 [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr h,out uint code);
 // The exit code once the process ended (259 while it runs).
 public int ExitCode{get{uint code=0;if(handle!=IntPtr.Zero)GetExitCodeProcess(handle,out code);return (int)code;}}
 public void Dispose(){if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
}
public static class RivetTestStart{
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct SI{public int cb;public string reserved,desktop,title;public int x,y,w,h,xChars,yChars,fill,flags;public short show,reserved2;public IntPtr reserved3,input,output,error;}
 [StructLayout(LayoutKind.Sequential)] struct PI{public IntPtr process,thread;public int id,threadId;}
 [DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)] static extern bool CreateProcessW(string app,System.Text.StringBuilder line,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string dir,ref SI si,out PI pi);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetHandleInformation(IntPtr h,uint mask,uint flags);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 // Starts `app` with the whole command line `line`: STARTF_USESHOWWINDOW (1) with
 // SW_SHOWNOACTIVATE (4), so its first window shows without activation. Standard output goes to
 // `output` when one is given; PowerShell passes a missing one as an empty string. No other handle
 // is inherited.
 public static RivetTestProcess Start(string app,string line,string output){
  var si=new SI();si.cb=Marshal.SizeOf(typeof(SI));si.flags=1;si.show=4;
  FileStream file=null;
  try{
   if(!string.IsNullOrEmpty(output)){
    file=new FileStream(output,FileMode.Create,FileAccess.Write,FileShare.ReadWrite|FileShare.Delete);
    IntPtr h=file.SafeFileHandle.DangerousGetHandle();
    if(!SetHandleInformation(h,1,1))throw new System.ComponentModel.Win32Exception();
    si.flags|=0x100;si.output=h; // STARTF_USESTDHANDLES
   }
   PI pi;
   if(!CreateProcessW(app,new System.Text.StringBuilder(line),IntPtr.Zero,IntPtr.Zero,!string.IsNullOrEmpty(output),0,IntPtr.Zero,null,ref si,out pi))throw new System.ComponentModel.Win32Exception();
   CloseHandle(pi.thread);
   return new RivetTestProcess(pi.process,pi.id);
  }finally{if(file!=null)file.Dispose();}
 }
}
'@
}

# Starts a browser for a test the way `rivet session start` does under a test run: its first window
# shows without activation. It takes the place of Start-Process -PassThru -WindowStyle Hidden, whose
# SW_HIDE Chromium shows as SW_SHOWNORMAL and then activates. The arguments are joined as
# Start-Process joins them, so each must be quoted already where it needs to be. The result has Id,
# WaitForExit(milliseconds) and Dispose().
function Start-TestBrowser {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$ArgumentList = @(),
    [string]$RedirectStandardOutput
  )
  $line = '"' + $FilePath + '"'
  if ($ArgumentList.Count) { $line += ' ' + ($ArgumentList -join ' ') }
  $output = if ($RedirectStandardOutput) { $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($RedirectStandardOutput) } else { $null }
  [RivetTestStart]::Start($FilePath, $line, $output)
}
