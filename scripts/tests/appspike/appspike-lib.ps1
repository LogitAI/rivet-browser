$ErrorActionPreference="Continue"; $OutDir=$env:SPIKE_OUT; $repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path; $root = Join-Path $repo "rivet"; $debug = Join-Path $root "target\debug"; 
# Debug switches and the trace of one launch live in its own test profile, `rivet-debug`, which only
# that browser process reads. Tests of other checkouts on this machine never take its commands.
$flagDir = $null; $logFile = $null

New-Item -ItemType Directory -Force $OutDir | Out-Null; $OutDir = (Resolve-Path $OutDir).Path
. (Join-Path $PSScriptRoot "..\windows-features-lib.ps1")
# Start-TestBrowser: a hand-off starts without activation, like the first launch.
. (Join-Path $PSScriptRoot "..\windows-test-safety.ps1")
# The spike build places no window, so the test switch keeps every window off every monitor:
# the browser starts minimized and the runtime restores it off-screen as it attaches.
$env:RIVET_KEEP_OFFSCREEN = '1'
$cargo = if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME 'bin\cargo.exe' } else { (Get-Command cargo).Source }

$table = [ordered]@{
  chrome  = @('C:\Program Files\Google\Chrome\Application\chrome.exe', 'chrome.exe')
  edge    = @('C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe', 'msedge.exe')
  whale   = @('C:\Program Files\Naver\Naver Whale\Application\whale.exe', 'whale.exe')
  brave   = @('C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe', 'brave.exe')
  samsung = @('C:\Program Files\Samsung\Internet\Application\samsunginternet.exe', 'samsunginternet.exe')
}

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using System.Text;using System.Collections.Generic;
public static class FeatureWin{
 public delegate bool EW(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int ht,uint f);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h,out R r);
 [DllImport("user32.dll")] public static extern bool EnumWindows(EW f,IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr GetProp(IntPtr h,string n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern uint RegisterWindowMessage(string n);
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h,ref P p);
 public struct R{public int L,T,Ri,B;}
 [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(P p);
 [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h,ref P p);
 public struct P{public int X,Y;}
 // Visible top-level browser windows of the given processes.
 public static List<IntPtr> Tops(HashSet<uint> pids){
  var o=new List<IntPtr>();
  EnumWindows((h,l)=>{uint q;GetWindowThreadProcessId(h,out q);if(pids.Contains(q)&&IsWindowVisible(h)){var b=new StringBuilder(64);GetClassName(h,b,64);if(b.ToString()=="Chrome_WidgetWin_1")o.Add(h);}return true;},IntPtr.Zero);
  return o;
 }
}
'@
[FeatureWin]::SetThreadDpiAwarenessContext([IntPtr]-4) | Out-Null

$pageDir = Join-Path $OutDir 'pages'
New-Item -ItemType Directory -Force $pageDir | Out-Null
function New-TestPage([string]$Name, [int]$Rgb) {
  $hex = '#{0:X6}' -f $Rgb
  $path = Join-Path $pageDir "$Name.html"
  [IO.File]::WriteAllText($path, "<!doctype html><meta charset=utf-8><title>$Name</title><style>html,body{margin:0;height:100%;background:$hex}</style>")
  'file:///' + ($path -replace '\\', '/')
}
$urlA = New-TestPage 'page-a' $script:ColourA
$urlB = New-TestPage 'page-b' $script:ColourB

# The sub page's pages. `sub-b` is blue and a click anywhere in it follows a link to `sub-c`, which is
# green, and yellow when it was loaded by a reload. Back and forward restore it green, so the colour
# tells which of the three buttons ran.
function New-SubPages {
  $b = Join-Path $pageDir 'sub-b.html'
  $c = Join-Path $pageDir 'sub-c.html'
  $hexB = '#{0:X6}' -f $script:ColourB; $hexC = '#{0:X6}' -f $script:ColourC; $hexD = '#{0:X6}' -f $script:ColourD
  [IO.File]::WriteAllText($b, "<!doctype html><meta charset=utf-8><title>sub-b</title><style>html,body{margin:0;height:100%;background:$hexB}</style><a href=`"sub-c.html`" style=`"display:block;width:100%;height:100%`"></a>")
  [IO.File]::WriteAllText($c, "<!doctype html><meta charset=utf-8><title>sub-c</title><style>html,body{margin:0;height:100%}html{background:$hexC}</style><script>if (performance.getEntriesByType('navigation')[0].type === 'reload') { document.documentElement.style.background = '$hexD'; }</script>")
  'file:///' + ($b -replace '\\', '/')
}
$urlSubB = New-SubPages

$rows = New-Object System.Collections.ArrayList
function Add-Row($Browser, $Version, $Feature, $Result, $Evidence, $Reason) {
  [void]$rows.Add([pscustomobject]@{ browser = $Browser; version = $Version; feature = $Feature; result = $Result; evidence = $Evidence; reason = $Reason })
  "{0} {1} {2}: {3} {4}" -f $Browser, $Version, $Feature, $Result, $Reason | Write-Host
}

$debug = Join-Path $root "target\debug"
# --- Helpers for one launch -----------------------------------------------------------------
function Stop-Mine($ExeName, $Profile) {
  Get-CimInstance Win32_Process -Filter "name='$ExeName'" | Where-Object { $_.CommandLine -like "*$Profile*" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

function Get-MyProcesses($ExeName, $Profile) {
  @(Get-CimInstance Win32_Process -Filter "name='$ExeName'" | Where-Object { $_.CommandLine -like "*$Profile*" })
}

function Read-Log { Get-Content $logFile -ErrorAction SilentlyContinue }

function Start-Scene($Name, $Feature, $Url) {
  $exePath, $exeName = $table[$Name]
  $profile_ = Join-Path $OutDir "profiles\$Name-$Feature"
  Stop-Mine $exeName $profile_
  Start-Sleep 1
  Remove-Item -Recurse -Force $profile_ -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force $profile_ | Out-Null
  $script:flagDir = Join-Path $profile_ 'rivet-debug'
  New-Item -ItemType Directory -Force $script:flagDir | Out-Null
  $script:logFile = Join-Path $script:flagDir 'rivet-vertical-tabs.log'
  # The log is how the runtime reports tab lists and space changes. It exists only when asked for.
  [IO.File]::WriteAllText($logFile, '')
  foreach ($flag in 'rivet-sidepanel.flag', 'rivet-tab.txt') { Remove-Item (Join-Path $script:flagDir $flag) -ErrorAction SilentlyContinue }
  $state = Join-Path $OutDir "state-$Name-$Feature.json"
  Remove-Item $state -ErrorAction SilentlyContinue
  $ctx = [pscustomobject]@{ Name = $Name; ExeName = $exeName; ExePath = $exePath; Profile = $profile_; Hwnd = [IntPtr]::Zero; Error = $null; Dpi = 96; Feature = $Feature }
  return $ctx, $state
}

function Open-Scene($ctx, $state, $Url, $Seed) {
  if ($Seed) { & $Seed }
  $session = $null; $raw = ''
  foreach ($try in 1..3) {
    $lines = & (Join-Path $debug 'rivet.exe') session start --browser $ctx.ExePath --url $Url --user-data-dir $ctx.Profile --runtime (Join-Path $debug 'rivet_runtime_windows.dll') --state $state --json 2>&1 | ForEach-Object { [string]$_ }
    $session = $lines | Where-Object { $_.Contains('runtime_loaded') } | Select-Object -First 1
    if ($session) { break }
    $raw = (($lines | Select-Object -First 2) -join ' ')
    Start-Sleep 2
  }
  if (-not $session) { $ctx.Error = "rivet session start did not report the runtime loaded: $raw"; return }
  $deadline = (Get-Date).AddSeconds(45)
  $attached = $false
  while ((Get-Date) -lt $deadline) {
    $mine = Get-MyProcesses $ctx.ExeName $ctx.Profile | Where-Object { $_.CommandLine -notlike '*--type=*' } | Select-Object -First 1
    if (-not $mine) { $ctx.Error = 'the browser exited after launch'; return }
    $h = (Get-BrowserWindow $mine.ProcessId)
    if ($h -and $h -ne [IntPtr]::Zero) {
      $ctx.Hwnd = $h
      if ([FeatureWin]::GetProp($h, 'Rivet.OriginalWindowProc') -ne [IntPtr]::Zero) { $attached = $true; break }
    }
    Start-Sleep 1
  }
  if ($ctx.Hwnd -eq [IntPtr]::Zero) { $ctx.Error = 'the browser opened no window'; return }
  if (-not $attached) { $ctx.Error = 'attach-failed' }
  # Park off-screen without activating, as the installed build's window would sit on the owner's monitor.
  [FeatureWin]::SetWindowPos($ctx.Hwnd, [IntPtr]::Zero, 9000, 1000, 1800, 1067, 0x14) | Out-Null
  [FeatureWin]::PostMessage($ctx.Hwnd, 0x86, [IntPtr]1, [IntPtr]0) | Out-Null
  $ctx.Dpi = [FeatureWin]::GetDpiForWindow($ctx.Hwnd)
  Start-Sleep 8
}

function Get-Capture($ctx, [string]$Path) {
  $r = New-Object FeatureWin+R
  [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
  $w = $r.Ri - $r.L; $h = $r.B - $r.T
  if ($w -le 0 -or $h -le 0) { return $null }
  $bmp = New-Object Drawing.Bitmap $w, $h, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [FeatureWin]::PrintWindow($ctx.Hwnd, $dc, 2) | Out-Null
  $g.ReleaseHdc($dc); $g.Dispose()
  $bmp.Save($Path)
  $data = $bmp.LockBits((New-Object Drawing.Rectangle 0, 0, $w, $h), [Drawing.Imaging.ImageLockMode]::ReadOnly, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $px = New-Object 'int[]' ($w * $h)
  [Runtime.InteropServices.Marshal]::Copy($data.Scan0, $px, 0, $px.Length)
  $bmp.UnlockBits($data); $bmp.Dispose()
  [pscustomobject]@{ Pixels = $px; Width = $w; Height = $h; Path = $Path }
}

function Send-RivetMessage($ctx, [string]$Name) {
  [FeatureWin]::PostMessage($ctx.Hwnd, [FeatureWin]::RegisterWindowMessage($Name), [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
}

# A click on the appbar. The appbar belongs to Rivet through the window's hit test, so Windows
# delivers its clicks as non-client messages with the border code and screen coordinates. These are
# the same messages a real click makes, posted to the window without moving the pointer.
function Send-AppbarClick($ctx, [int]$X, [int]$Y) {
  $p = New-Object FeatureWin+P; $p.X = $X; $p.Y = $Y
  [FeatureWin]::ClientToScreen($ctx.Hwnd, [ref]$p) | Out-Null
  $l = [IntPtr](([int]$p.Y -shl 16) -bor ([int]$p.X -band 0xffff))
  $htBorder = [IntPtr]18
  [FeatureWin]::PostMessage($ctx.Hwnd, 0xA1, $htBorder, $l) | Out-Null
  Start-Sleep -Milliseconds 150
  [FeatureWin]::PostMessage($ctx.Hwnd, 0xA2, $htBorder, $l) | Out-Null
}

# Writes a command file the runtime reads on its timer, and waits for the runtime to take it.
function Send-Flag($ctx, [string]$File, [string]$Text) {
  $path = Join-Path $flagDir $File
  [IO.File]::WriteAllText($path, $Text)
  foreach ($i in 1..20) { Start-Sleep -Milliseconds 500; if (-not (Test-Path $path)) { Start-Sleep 2; return $true } }
  Remove-Item $path -ErrorAction SilentlyContinue
  $false
}

# After a launch: a Rivet window that never attached is a failure of the browser build.
function Test-Attached($ctx) { $ctx.Error -ne 'attach-failed' }

function Stop-Scene($ctx) { Stop-Mine $ctx.ExeName $ctx.Profile }

# Opens a second page in the running window the way a link from outside would: the browser's own
# single-instance hand-off. Only after the first process is alive, so it cannot start a window.
function Open-SecondPage($ctx, [string]$Url) {
  $p = Start-TestBrowser -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $Url)
  $null = $p.WaitForExit(20000)
  Start-Sleep 5
  $pids = New-Object 'System.Collections.Generic.HashSet[uint32]'
  foreach ($q in Get-MyProcesses $ctx.ExeName $ctx.Profile) { [void]$pids.Add([uint32]$q.ProcessId) }
  $tops = [FeatureWin]::Tops($pids)
  if ($tops.Count -gt 1) {
    # This browser's hand-off opens a window of its own. The window belongs to the test profile's process,
    # so it is closed, and the page goes into a new tab of the first window through the runtime.
    foreach ($t in $tops) { if ($t -ne $ctx.Hwnd) { [FeatureWin]::PostMessage($t, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null } }
    Start-Sleep 3
    $left = New-Object 'System.Collections.Generic.HashSet[uint32]'
    foreach ($q in Get-MyProcesses $ctx.ExeName $ctx.Profile) { [void]$left.Add([uint32]$q.ProcessId) }
    if ([FeatureWin]::Tops($left).Count -gt 1) {
      foreach ($t in [FeatureWin]::Tops($left)) { if ($t -ne $ctx.Hwnd) { [FeatureWin]::SetWindowPos($t, [IntPtr]::Zero, -3000, -3000, 1600, 1000, 0x14) | Out-Null } }
      return $false
    }
    # The hand-off may already have put the page into a tab of the first window.
    if ((Send-Flag $ctx 'rivet-tab.txt' 'list') -and ((Get-TabUrls) -contains $Url)) { return $true }
    Send-RivetMessage $ctx 'Rivet.NewStartPage'
    Start-Sleep 6
    if (-not (Send-Flag $ctx 'rivet-tab.txt' "navigate $Url")) { return $false }
    Start-Sleep 4
    return $true
  }
  $true
}


