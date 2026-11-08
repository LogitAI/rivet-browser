# Per-browser feature contract for the Rivet window. A feature is called working on a browser only
# when this passes on a launch like the installed build's. Unit tests and build rows do not count.
#
#   pwsh -NoProfile -File scripts/tests/windows-features.ps1 [-Browsers chrome,edge,...]
#       [-Features split,sidebar,newtab,space,split-grid-4,bar-layout,command-bar,link-preview,link-rules,address-suggest,appbar-width,appbar-peek,open-link-running,open-link-start,credentials-browser,credentials-system,view-ime,tab-moves,first-frame,cli-control,cli-input,cli-hidden] [-OutDir <dir>] [-NoBuild]
#
# One row per browser and feature: PASS, FAIL or NOT_DRIVEN. NOT_DRIVEN means the scene could not
# be set up, so the feature was not exercised. Nothing is decided from build rows or `supported`.
#
# Safety: test windows sit off-screen and are captured with PrintWindow. No window is activated and
# the cursor never moves. Each run uses a fresh profile inside this checkout with sync and sign-in
# off, and only processes whose command line holds that profile are stopped. Nothing is copied out
# of a profile. A browser is started only when the build exited 0. Every row runs under
# RIVET_KEEP_OFFSCREEN with Rivet's own files in this run's folder, and the visible guard stops the
# run when a test window shows on a monitor, takes the foreground or holds the mouse. The open-link
# rows run only when no other run is active (windows-test-safety.ps1).
param(
  [string[]]$Browsers = @('chrome', 'edge', 'whale', 'brave', 'samsung'),
  [string[]]$Features = @('split', 'sidebar', 'newtab', 'space', 'split-start', 'split-menu', 'split-menu-start', 'split-drag', 'split-drag-start', 'split-shortcut', 'split-one-tab', 'split-maximised', 'split-space', 'split-click', 'split-rows', 'split-drop-top', 'split-drop-bottom', 'split-grid-3', 'split-grid-4', 'bar-layout', 'mini-pop-out', 'mini-move', 'mini-return', 'mini-close', 'tab-sections', 'tab-close-select', 'tab-reopen', 'tab-archive', 'ctrl-t-setting', 'link-new-tab', 'command-bar', 'link-preview', 'rivet-settings', 'link-rules', 'address-suggest', 'appbar-width', 'appbar-peek', 'open-link-running', 'open-link-start', 'credentials-browser', 'credentials-system', 'view-click', 'view-click-other', 'view-keys', 'view-ime', 'tab-moves', 'first-frame', 'fullscreen-sidebar', 'taskbar-identity', 'taskbar-relaunch', 'sidebar-scale-monitor', 'split-panel-look', 'sidebar-floats', 'sidebar-open-button', 'dev-row', 'address-editing', 'security-chip', 'permission-bubble', 'password-bubble', 'typing-keeps-active', 'cli-control', 'cli-input', 'cli-hidden'),
  [string]$OutDir,
  [switch]$NoBuild,
  # A browser build outside its usual folder, such as one a row job downloaded. Needs one -Browsers.
  [string]$Executable,
  # The run stops before a row when the output drive has less free space than this.
  [double]$MinFreeGigabytes = 20
)
$ErrorActionPreference = 'Continue'
# The whole run, its build and every browser, runs on a desktop of its own.
. (Join-Path $PSScriptRoot 'windows-test-desktop.ps1')
$desktopCode = Invoke-OnOwnDesktop $PSCommandPath $PSBoundParameters
if ($null -ne $desktopCode) { exit $desktopCode }
# Under -File a comma list arrives as one string.
$Browsers = @($Browsers | ForEach-Object { $_ -split ',' })
$Features = @($Features | ForEach-Object { $_ -split ',' })
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$root = Join-Path $repo 'rivet'
$debug = Join-Path $root 'target\debug'

# Debug switches and the trace of one launch live in its own test profile, `rivet-debug`, which only
# that browser process reads. Tests of other checkouts on this machine never take its commands.
$flagDir = $null; $logFile = $null
if (-not $OutDir) { $OutDir = Join-Path $repo '.scratch\features' }
New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
. (Join-Path $PSScriptRoot 'windows-features-lib.ps1')
. (Join-Path $PSScriptRoot 'windows-processes.ps1')

# Cargo comes from the caller's toolchain: CARGO_HOME when it is set, else cargo on the PATH.
$cargo = if ($env:CARGO_HOME -and (Test-Path (Join-Path $env:CARGO_HOME 'bin\cargo.exe'))) { Join-Path $env:CARGO_HOME 'bin\cargo.exe' } else { 'cargo' }

$table = [ordered]@{
  chrome  = @('C:\Program Files\Google\Chrome\Application\chrome.exe', 'chrome.exe')
  edge    = @('C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe', 'msedge.exe')
  whale   = @('C:\Program Files\Naver\Naver Whale\Application\whale.exe', 'whale.exe')
  brave   = @('C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe', 'brave.exe')
  samsung = @('C:\Program Files\Samsung\Internet\Application\samsunginternet.exe', 'samsunginternet.exe')
}
if ($Executable) {
  if ($Browsers.Count -ne 1) { throw '-Executable needs exactly one browser in -Browsers' }
  $table[$Browsers[0]] = @($Executable, (Split-Path -Leaf $Executable))
}

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;using System.Text;using System.Collections.Generic;
public static class FeatureWin{
 public delegate bool EW(IntPtr h,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int ht,uint f);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
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
 [DllImport("user32.dll")] public static extern IntPtr ChildWindowFromPointEx(IntPtr h,P p,uint f);
 // The deepest visible window of `top` at a screen point. Windows of other test browsers can lie
 // at the same off-screen place, so the point is looked up inside this window only.
 public static IntPtr Inside(IntPtr top,P s){var h=top;for(int i=0;i<16;i++){var c=s;ScreenToClient(h,ref c);var n=ChildWindowFromPointEx(h,c,1);if(n==IntPtr.Zero||n==h)break;h=n;}return h;}
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
$urlC = New-TestPage 'page-c' $script:ColourC
$urlD = New-TestPage 'page-d' $script:ColourD

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

# A page with a top stripe of its own colour over its body, so a bar drawn over the top of the page
# hides the stripe. A page of one colour looks the same whether the bar covers it or not.
function New-StripePage {
  $path = Join-Path $pageDir 'page-top.html'
  $hexA = '#{0:X6}' -f $script:ColourA; $hexC = '#{0:X6}' -f $script:ColourC
  [IO.File]::WriteAllText($path, "<!doctype html><meta charset=utf-8><title>page-top</title><style>html,body{margin:0;height:100%;background:$hexA}body{border-top:12px solid $hexC;box-sizing:border-box}</style>")
  'file:///' + ($path -replace '\\', '/')
}
$urlTop = New-StripePage

# The app window's Chromium addresses come from the build rows, as in an installed build. A test
# profile names only measuring aids here.
$appWindowRvas = @{ whale = @() }

$rows = New-Object System.Collections.ArrayList
function Add-Row($Browser, $Version, $Feature, $Result, $Evidence, $Reason) {
  [void]$rows.Add([pscustomobject]@{ browser = $Browser; version = $Version; feature = $Feature; result = $Result; evidence = $Evidence; reason = $Reason })
  "{0} {1} {2}: {3} {4}" -f $Browser, $Version, $Feature, $Result, $Reason | Write-Host
}

# --- Build, gated on the exit code ---------------------------------------------------------
$built = $false; $buildNote = ''
if ($NoBuild) {
  $built = (Test-Path (Join-Path $debug 'rivet.exe')) -and (Test-Path (Join-Path $debug 'rivet_runtime_windows.dll'))
  $buildNote = if ($built) { '' } else { 'no build output and -NoBuild given' }
  if ($built) { Write-Host 'build skipped (-NoBuild): the binaries are whatever the last build left' }
  # A launcher without the test flags would start a browser with sync and sign-in on.
  if ($built -and -not ([Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes((Join-Path $debug 'rivet.exe'))).Contains('msImplicitSignin,msEdgeSignIn,CalculateNativeWinOcclusion'))) {
    $built = $false; $buildNote = 'rivet.exe was not built with the test flags'
  }
} else {
  # The flags exist for tests only: no sync, no sign-in. CalculateNativeWinOcclusion is off because
  # Chrome treats an off-screen window as covered and stops painting its pages, and a pixel check on
  # an unpainted page proves nothing. No flag places or sizes the window: --window-position and
  # --window-size give it the normal show state, which Chromium shows with activation. Under
  # RIVET_KEEP_OFFSCREEN the browser starts minimized and the runtime restores the window off every
  # monitor at 1600x1000. The launcher file is restored right after the build and must never be
  # committed.
  $f = Join-Path $root 'core\src\lib.rs'
  $orig = [IO.File]::ReadAllText($f)
  $flags = @('--disable-sync', '--no-first-run', '--no-default-browser-check', '--disable-features=msImplicitSignin,msEdgeSignIn,CalculateNativeWinOcclusion,Translate', '--lang=en-US')
  $push = ($flags | ForEach-Object { "arguments.push(`"$_`".to_owned());" }) -join ' '
  # The visible launch is an app window now, so the test flags go in front of that push.
  $marker = 'arguments.push(format!("--app={}", request.url));'
  if (-not $orig.Contains($marker)) { throw 'the visible launch push was not found in core/src/lib.rs' }
  [IO.File]::WriteAllText($f, $orig.Replace($marker, "$push $marker"))
  Push-Location $root
  try {
    $output = & $cargo build -q 2>&1
    $code = $LASTEXITCODE
    if ($code -ne 0) { $output | Select-Object -Last 40 | ForEach-Object { Write-Host $_ } }
  } finally { Pop-Location; [IO.File]::WriteAllText($f, $orig) }
  $built = ($code -eq 0)
  $buildNote = if ($built) { '' } else { "the build exited $code" }
}

# --- Helpers for one launch -----------------------------------------------------------------
function Stop-Mine($ExeName, $Profile) {
  Get-ProcessList $ExeName | Where-Object { $_.CommandLine -like "*$Profile*" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

function Get-MyProcesses($ExeName, $Profile) {
  @(Get-ProcessList $ExeName | Where-Object { $_.CommandLine -like "*$Profile*" })
}

function Read-Log { Get-Content $logFile -ErrorAction SilentlyContinue }

function Start-Scene($Name, $Feature, $Url) {
  # No browser starts after the visible guard stopped the run.
  Assert-RunSafe $script:safety
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
  # App window route: Chromium's own tab strip model functions that put a second tab into the app
  # window, by public name. A browser with no symbols has none, so its second page cannot be placed.
  $spike = Join-Path $flagDir 'rivet-spike.txt'
  Remove-Item $spike -ErrorAction SilentlyContinue
  if ($appWindowRvas.Contains($Name)) { Set-Content $spike $appWindowRvas[$Name] }
  $state = Join-Path $OutDir "state-$Name-$Feature.json"
  Remove-Item $state -ErrorAction SilentlyContinue
  $ctx = [pscustomobject]@{ Name = $Name; ExeName = $exeName; ExePath = $exePath; Profile = $profile_; Hwnd = [IntPtr]::Zero; Error = $null; Dpi = 96; Feature = $Feature }
  return $ctx, $state
}

function Open-Scene($ctx, $state, $Url, $Seed, [string[]]$Extra = @()) {
  if ($Seed) { & $Seed }
  $session = $null; $raw = ''
  foreach ($try in 1..3) {
    $lines = & (Join-Path $debug 'rivet.exe') session start --browser $ctx.ExePath --url $Url --user-data-dir $ctx.Profile --runtime (Join-Path $debug 'rivet_runtime_windows.dll') --state $state @Extra --json 2>&1 | ForEach-Object { [string]$_ }
    $session = $lines | Where-Object { $_.Contains('runtime_loaded') } | Select-Object -First 1
    if ($session) { break }
    $raw = (($lines | Select-Object -First 2) -join ' ')
    Start-Sleep 2
  }
  if (-not $session) { $ctx.Error = "rivet session start did not report the runtime loaded: $raw"; return }
  $ctx | Add-Member -Force SessionId (($session | ConvertFrom-Json).session_id)
  $deadline = (Get-Date).AddSeconds(45)
  $attached = $false
  while ((Get-Date) -lt $deadline) {
    # A browser can run more than one process without a --type switch, such as Whale's tray
    # process, so the frame window is looked for in each of them.
    $mine = @(Get-MyProcesses $ctx.ExeName $ctx.Profile | Where-Object { $_.CommandLine -notlike '*--type=*' })
    if (-not $mine.Count) { $ctx.Error = 'the browser exited after launch'; return }
    $h = Get-BrowserWindow ($mine | ForEach-Object { [int]$_.ProcessId })
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
  Start-Sleep 8
  $ctx.Dpi = Get-LayoutDpi $ctx
}

# The scale the browser laid the window out at, as the runtime reports it, which every part of
# Rivet's look follows. Windows can report another DPI for the window, as it does on a test desktop
# and while a window crosses monitors, so the checks measure at the browser's scale. Without a
# report yet, the window's own DPI.
function Get-LayoutDpi($ctx) {
  $hex = '{0:x}' -f $ctx.Hwnd.ToInt64()
  $line = @(Read-Log | Where-Object { $_ -match "^layout dpi: window 0x$hex at (\d+)$" }) | Select-Object -Last 1
  if ($line -and $line -match ' at (\d+)$') { return [int]$Matches[1] }
  [FeatureWin]::GetDpiForWindow($ctx.Hwnd)
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

# Stops the scene's browser, keeps its trace under the run's `traces` folder and deletes its test
# profile, so a long run does not fill the disk. Captures are kept where they were written.
function Stop-Scene($ctx) {
  Stop-Mine $ctx.ExeName $ctx.Profile
  for ($i = 0; $i -lt 20 -and @(Get-MyProcesses $ctx.ExeName $ctx.Profile).Count -gt 0; $i++) { Start-Sleep -Milliseconds 250 }
  if (-not $ctx.Profile -or -not (Test-Path $ctx.Profile)) { return }
  $traces = Join-Path $OutDir 'traces'
  $debugDir = Join-Path $ctx.Profile 'rivet-debug'
  if (Test-Path $debugDir) {
    $keep = Join-Path $traces (Split-Path -Leaf $ctx.Profile)
    New-Item -ItemType Directory -Force $keep | Out-Null
    Copy-Item -Path (Join-Path $debugDir '*') -Destination $keep -Recurse -Force -ErrorAction SilentlyContinue
  }
  Remove-Item -Recurse -Force $ctx.Profile -ErrorAction SilentlyContinue
}

# The visible browser windows of the test profile's processes.
function Get-MyTops($ctx) {
  $pids = New-Object 'System.Collections.Generic.HashSet[uint32]'
  foreach ($q in Get-MyProcesses $ctx.ExeName $ctx.Profile) { [void]$pids.Add([uint32]$q.ProcessId) }
  @([FeatureWin]::Tops($pids))
}

# Opens a second page in the running window through Rivet's own new-tab command and then navigates
# that tab. The browser's single-instance hand-off is never used: it can open a normal window on
# screen that takes the keyboard. Any window other than the off-screen app window fails the step, and
# such a window is moved off-screen without activation and closed at once.
function Open-SecondPage($ctx, [string]$Url) {
  $before = Get-MyTops $ctx
  $flag = Join-Path $flagDir 'rivet-tab.txt'
  # The tab opens on the page at once. The browser's own new tab page is never shown first,
  # since Samsung Internet's crashed beside Rivet's second view.
  [IO.File]::WriteAllText($flag, "new $Url")
  # Watched every 50 ms, so a window the command should not have made is moved away at once.
  $extra = @()
  foreach ($i in 1..200) {
    Start-Sleep -Milliseconds 50
    $extra = @(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $before -notcontains $_ })
    if ($extra.Count -gt 0) { break }
  }
  if ($extra.Count -gt 0) {
    foreach ($t in $extra) {
      [FeatureWin]::SetWindowPos($t, [IntPtr]::Zero, -3000, -3000, 1600, 1000, 0x14) | Out-Null
      [FeatureWin]::PostMessage($t, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    }
    Remove-Item $flag -ErrorAction SilentlyContinue
    return $false
  }
  if (Test-Path $flag) { Remove-Item $flag -ErrorAction SilentlyContinue; return $false }
  Start-Sleep 4
  if (@(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $before -notcontains $_ }).Count -gt 0) { return $false }
  # The page must be a tab of the app window itself.
  (Send-Flag $ctx 'rivet-tab.txt' 'list') -and ((Get-TabUrls) -contains $Url)
}

# --- Features -----------------------------------------------------------------------------
function Invoke-Split($Name, $version, $shots) {
  $ctx, $state = Start-Scene $Name 'split' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots 'split-1-page-a.png')
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    $v = Test-OnePage $one $script:ColourA $script:ColourB
    if ($v.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $one.Path, "the first test page was not shown: $($v.Reason)") }
    if (-not (Open-SecondPage $ctx $urlB)) { return @('NOT_DRIVEN', $one.Path, 'the second page did not open as a tab of the app window alone') }
    $two = Get-Capture $ctx (Join-Path $shots 'split-2-page-b.png')
    $v = Test-OnePage $two $script:ColourB $script:ColourA
    if ($v.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $two.Path, "the second test page was not shown alone: $($v.Reason)") }
    # The checker must reject a window that has no split: this capture has two tabs and no split.
    $control = Test-TwoPages $two $script:ColourA $script:ColourB
    if ($control.Verdict -ne 'FAIL') { return @('NOT_DRIVEN', $two.Path, "the split check did not fail on a window without a split ($($control.Verdict))") }
    $opens = Get-ViewOpens 'Split'
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $split = Get-Capture $ctx (Join-Path $shots 'split-3-on.png')
    $on = Test-TwoPages $split $script:ColourA $script:ColourB
    if ($on.Verdict -ne 'PASS') { return @($on.Verdict, $split.Path, "split on: $($on.Reason)") }
    if (-not (Test-ViewOwns $opens 'Split')) { return @('FAIL', $split.Path, 'two pages are shown, but the second page is not in the second page view') }
    # Each page carries its own row, as the demo's split panels do, and both start at one top.
    $rows = Test-PaneRows $split $script:ColourA $script:ColourB ($ctx.Dpi / 96.0)
    if ($rows.Verdict -ne 'PASS') { return @($rows.Verdict, $split.Path, "split on: $($rows.Reason)") }
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $off = Get-Capture $ctx (Join-Path $shots 'split-4-off.png')
    # The active tab keeps the window after the split ends, so either colour may remain.
    $left = Test-OnePage $off $script:ColourA $script:ColourB
    if ($left.Verdict -ne 'PASS') { $left = Test-OnePage $off $script:ColourB $script:ColourA }
    if ($left.Verdict -ne 'PASS') { return @($left.Verdict, $off.Path, "split off: $($left.Reason)") }
    return @('PASS', $split.Path, "on: $($on.Reason); off: $($left.Reason)")
  } finally { Stop-Scene $ctx }
}

function Invoke-Sidebar($Name, $version, $shots) {
  $ctx, $state = Start-Scene $Name 'sidebar' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $before = Get-Capture $ctx (Join-Path $shots 'sidebar-1-before.png')
    if (-not $before) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $before.Path, 'Rivet did not attach to the window') }
    # The rail is the sidebar's closed state. `sim open` is the click on its open button and
    # `sim entry 0` the click on its first entry, both through the rail's own handler.
    if (-not (Send-Flag $ctx 'rivet-sidepanel.flag' 'sim open')) { return @('NOT_DRIVEN', $before.Path, 'the runtime did not take the open command') }
    Start-Sleep 2
    $rail = Get-Capture $ctx (Join-Path $shots 'sidebar-2-rail.png')
    # The browser's own side panel is not used: the sidebar page is Rivet's docked second view below.
    $v = Test-SidebarRail $before $rail $script:ColourA ($ctx.Dpi / 96.0)
    if ($v.Verdict -ne 'PASS') { return @($v.Verdict, $rail.Path, $v.Reason) }
    $panel = $rail
    # The sidebar page is shown in the second page view docked at the right: page-b is active and
    # page-a, tab 0, is docked beside it.
    if (-not (Open-SecondPage $ctx $urlB)) { return @('NOT_DRIVEN', $panel.Path, 'the second page did not open as a tab of the app window alone') }
    $opens = Get-ViewOpens 'Docked'
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'dock 0')) { return @('NOT_DRIVEN', $panel.Path, 'the runtime did not take the dock command') }
    Start-Sleep 3
    $docked = Get-Capture $ctx (Join-Path $shots 'sidebar-4-docked.png')
    $two = Test-TwoPages $docked $script:ColourB $script:ColourA
    if ($two.Verdict -ne 'PASS') { return @($two.Verdict, $docked.Path, "docked sidebar page: $($two.Reason)") }
    if (-not (Test-ViewOwns $opens 'Docked')) { return @('FAIL', $docked.Path, 'two pages are shown, but the sidebar page is not in the second page view') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'undock')) { return @('NOT_DRIVEN', $docked.Path, 'the runtime did not take the undock command') }
    Start-Sleep 3
    $back = Get-Capture $ctx (Join-Path $shots 'sidebar-5-undocked.png')
    $one = Test-OnePage $back $script:ColourB $script:ColourA
    if ($one.Verdict -ne 'PASS') { return @('FAIL', $back.Path, "after undock: $($one.Reason)") }
    return @('PASS', $docked.Path, "$($v.Reason); docked: $($two.Reason); undocked: one page")
  } finally { Stop-Scene $ctx }
}

# How many trace lines say the second page view opened as `Mode` (Split or Docked).
function Get-ViewOpens([string]$Mode) { @(Read-Log | Where-Object { $_ -match "second view: open $Mode" }).Count }

# The second page must be owned by Rivet's second view: a new open of that mode since `$Before`.
function Test-ViewOwns([int]$Before, [string]$Mode) { (Get-ViewOpens $Mode) -gt $Before }

function Get-TabUrls {
  $line = Read-Log | Where-Object { $_ -match 'tab urls \[' } | Select-Object -Last 1
  if (-not $line) { return $null }
  @([regex]::Matches($line, '"([^"]*)"') | ForEach-Object { $_.Groups[1].Value })
}

function Invoke-NewTab($Name, $version, $shots) {
  $ctx, $state = Start-Scene $Name 'newtab' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { $c = Get-Capture $ctx (Join-Path $shots 'newtab-1.png'); return @('FAIL', $c.Path, 'Rivet did not attach to the window') }
    # `list` is not a tab command, so the runtime changes nothing and only traces the tab list.
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $null, 'the runtime did not take the tab list command') }
    $first = Get-TabUrls
    if (-not $first -or $first.Count -ne 1) { return @('NOT_DRIVEN', $null, "the window did not start with one tab (saw $($first.Count))") }
    Send-RivetMessage $ctx 'Rivet.NewStartPage'
    Start-Sleep 6
    $shot = Get-Capture $ctx (Join-Path $shots 'newtab-1.png')
    $listed = @(Read-Log | Where-Object { $_ -match 'tab urls \[' }).Count
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $shot.Path, 'the runtime did not take the tab list command') }
    $urls = Get-TabUrls
    if (@(Read-Log | Where-Object { $_ -match 'tab urls \[' }).Count -le $listed) { $urls = $null }
    if (-not $urls) { return @('NOT_DRIVEN', $shot.Path, 'the runtime reported no tab list') }
    if (Test-Blank $shot) { return @('NOT_DRIVEN', $shot.Path, 'the capture is blank') }
    if ($urls.Count -ne 2) { return @('FAIL', $shot.Path, "expected two tabs after a new tab, the window has $($urls.Count): $($urls -join ' ')") }
    if ($urls[1] -notmatch 'start\.html') { return @('FAIL', $shot.Path, "the new tab is not the Rivet start page: $($urls[1])") }
    return @('PASS', $shot.Path, "two tabs, the new one is $($urls[1] -replace '\?.*','')")
  } finally { Stop-Scene $ctx }
}

function Invoke-Space($Name, $version, $shots) {
  $ctx, $state = Start-Scene $Name 'space' $urlA
  $seeded = $null
  try {
    Open-Scene $ctx $state $urlA { $script:seeded = Write-SpaceSeed $ctx.Profile $Name }
    $seeded = $script:seeded
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { $c = Get-Capture $ctx (Join-Path $shots 'space-1.png'); return @('FAIL', $c.Path, 'Rivet did not attach to the window') }
    Send-RivetMessage $ctx 'Rivet.NewStartPage'
    Start-Sleep 6
    $one = Get-Capture $ctx (Join-Path $shots 'space-1-first.png')
    # Click the second dot of the footer. The footer layout is the appbar's: 276 DIP wide, 10 DIP
    # inset, dots 32 DIP apart and centred between the settings and add buttons, 24 DIP above the bottom.
    $f = $ctx.Dpi / 96.0
    $dots = 2 * 32 - 4
    $x = 10 + 28 + (258 - 56 - $dots) / 2 + 32 + 14
    $cr = New-Object FeatureWin+R; [FeatureWin]::GetClientRect($ctx.Hwnd, [ref]$cr) | Out-Null
    Send-AppbarClick $ctx ([int]($x * $f)) ([int]($cr.B - 24 * $f))
    Start-Sleep 7
    $two = Get-Capture $ctx (Join-Path $shots 'space-2-second.png')
    $moved = Read-Log | Where-Object { $_ -match 'space: to FeatureB' } | Select-Object -First 1
    if (-not $moved) { return @('NOT_DRIVEN', $two.Path, 'the click did not switch to the second space, so the colours were not compared') }
    $v = Test-SpaceColours $one $two $f
    return @($v.Verdict, $two.Path, $v.Reason)
  } finally {
    Stop-Scene $ctx
    if ($seeded) { Remove-Item $seeded -ErrorAction SilentlyContinue }
  }
}

# --- Split, the way a person does it ---------------------------------------------------------
# Row geometry of the appbar in DIPs: the new-tab row starts at 138 and each row is 34 high, so
# tab `k` is centred at 189 + 34k. Its context menu opens below the row, and the first entry is 25
# DIPs under the row's bottom edge.
function Get-TabRowY([int]$Index) { 172 + 34 * $Index + 17 }

function Send-AppbarNc($ctx, [uint32]$Message, [int]$X, [int]$Y) {
  $p = New-Object FeatureWin+P; $p.X = $X; $p.Y = $Y
  [FeatureWin]::ClientToScreen($ctx.Hwnd, [ref]$p) | Out-Null
  $l = [IntPtr](([int]$p.Y -shl 16) -bor ([int]$p.X -band 0xffff))
  [FeatureWin]::PostMessage($ctx.Hwnd, $Message, [IntPtr]18, $l) | Out-Null
}

function Send-ClientMouse($ctx, [uint32]$Message, [int]$X, [int]$Y, [int]$Keys = 0) {
  [FeatureWin]::PostMessage($ctx.Hwnd, $Message, [IntPtr]$Keys, [IntPtr](($Y -shl 16) -bor ($X -band 0xffff))) | Out-Null
}

# The person's right click on a tab row, then a click on the menu's first entry.
function Invoke-TabMenu($ctx, [int]$Tab, $shotPath) {
  $f = $ctx.Dpi / 96.0
  $x = [int](100 * $f); $y = [int](2 + (Get-TabRowY $Tab) * $f)
  Send-AppbarNc $ctx 0xA4 $x $y
  Start-Sleep -Milliseconds 150
  Send-AppbarNc $ctx 0xA5 $x $y
  Start-Sleep 2
  if ($shotPath) { $null = Get-Capture $ctx $shotPath }
  $itemY = [int](2 + ((172 + 34 * $Tab + 34) + 25) * $f)
  Send-ClientMouse $ctx 0x200 $x $itemY
  Send-ClientMouse $ctx 0x201 $x $itemY 1
  Start-Sleep -Milliseconds 200
  Send-ClientMouse $ctx 0x202 $x $itemY
  Start-Sleep 4
}

# The person's drag of a tab row onto the left edge of the page, with the same messages: the press
# and the first moves arrive as non-client input, then the window captures the pointer.
function Invoke-TabDrag($ctx, [int]$Tab, $shotPath) {
  $f = $ctx.Dpi / 96.0
  $x = [int](100 * $f); $y = [int](2 + (Get-TabRowY $Tab) * $f)
  $cr = New-Object FeatureWin+R; [FeatureWin]::GetClientRect($ctx.Hwnd, [ref]$cr) | Out-Null
  $left = [int]((276 + 8) * $f); $right = $cr.Ri - [int](8 * $f)
  $dropX = $left + [int](($right - $left) * 0.05); $dropY = [int]($cr.B / 2)
  Send-AppbarNc $ctx 0xA1 $x $y
  Start-Sleep -Milliseconds 150
  foreach ($step in 1..4) { Send-AppbarNc $ctx 0xA0 ($x + 12 * $step) $y; Start-Sleep -Milliseconds 80 }
  foreach ($step in 1..8) {
    Send-ClientMouse $ctx 0x200 ([int]($x + ($dropX - $x) * $step / 8)) ([int]($y + ($dropY - $y) * $step / 8)) 1
    Start-Sleep -Milliseconds 80
  }
  Start-Sleep -Milliseconds 300
  if ($shotPath) { $null = Get-Capture $ctx $shotPath }
  Send-ClientMouse $ctx 0x202 $dropX $dropY
  Start-Sleep 4
}

function Get-ActiveTitle {
  $line = Read-Log | Where-Object { $_ -match 'tab command' } | Select-Object -Last 1
  if ($line -and $line -match '\*([^",\]]+)') { return $Matches[1] }
  $null
}

function Test-SplitShown($capture, $pages) {
  if ($pages -eq 'start') { Test-PaneAndStart $capture $script:ColourA } else { Test-TwoPages $capture $script:ColourA $script:ColourB }
}

# The window a person gets from the split paths. `Path` is message, menu or drag; `Pages` is colour
# (two coloured file pages) or start (a coloured page and the Rivet start page).
function Invoke-SplitPath($Name, $shots, [string]$Feature, [string]$Path, [string]$Pages, [switch]$ThenClick, [switch]$AfterSpace) {
  $ctx, $state = Start-Scene $Name $Feature $urlA
  $seeded = $null
  try {
    $seedBlock = $null
    if ($AfterSpace) { $seedBlock = { $script:seeded = Write-SpaceSeed $ctx.Profile $Name } }
    Open-Scene $ctx $state $urlA $seedBlock
    $seeded = $script:seeded
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots "$Feature-1-page-a.png")
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    if ($AfterSpace) {
      # One space switch first: the window keeps one start tab of the other space.
      $f = $ctx.Dpi / 96.0
      $cr = New-Object FeatureWin+R; [FeatureWin]::GetClientRect($ctx.Hwnd, [ref]$cr) | Out-Null
      $dx = 10 + 28 + (258 - 56 - 60) / 2 + 32 + 14
      Send-AppbarClick $ctx ([int]($dx * $f)) ([int]($cr.B - 24 * $f))
      Start-Sleep 7
      if (-not (Read-Log | Where-Object { $_ -match 'space: to FeatureB' })) { return @('NOT_DRIVEN', $one.Path, 'the click did not switch space') }
      if (-not (Open-SecondPage $ctx $urlA)) { return @('NOT_DRIVEN', $one.Path, 'a page did not open as a tab of the app window alone') }
      if (-not (Open-SecondPage $ctx $urlB)) { return @('NOT_DRIVEN', $one.Path, 'a page did not open as a tab of the app window alone') }
    } else {
      $v = Test-OnePage $one $script:ColourA $script:ColourB
      if ($v.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $one.Path, "the first test page was not shown: $($v.Reason)") }
      if ($Pages -eq 'start') {
        Send-RivetMessage $ctx 'Rivet.NewStartPage'
        Start-Sleep 6
      } elseif (-not (Open-SecondPage $ctx $urlB)) {
        return @('NOT_DRIVEN', $one.Path, 'the second page did not open as a tab of the app window alone')
      }
    }
    $shown = if ($AfterSpace) { 'colour' } else { $Pages }
    $two = Get-Capture $ctx (Join-Path $shots "$Feature-2-before.png")
    # The check must fail on this window, which has two tabs and no split.
    $control = Test-SplitShown $two $shown
    if ($control.Verdict -ne 'FAIL') { return @('NOT_DRIVEN', $two.Path, "the split check did not fail on a window without a split ($($control.Verdict)): $($control.Reason)") }
    $tabIndex = if ($AfterSpace) { 1 } else { 0 }
    $opens = Get-ViewOpens 'Split'
    switch ($Path) {
      'message' { Send-RivetMessage $ctx 'Rivet.ToggleSplit'; Start-Sleep 6 }
      'menu' { Invoke-TabMenu $ctx $tabIndex (Join-Path $shots "$Feature-3-menu.png") }
      'drag' { Invoke-TabDrag $ctx $tabIndex (Join-Path $shots "$Feature-3-dragging.png") }
    }
    $split = Get-Capture $ctx (Join-Path $shots "$Feature-4-after-$Path.png")
    $on = Test-SplitShown $split $shown
    if ($on.Verdict -ne 'PASS') {
      $log = (Read-Log | Where-Object { $_ -match 'tab menu|tab drag|split' } | Select-Object -Last 3) -join ' / '
      # A build with no recorded split view cannot split by the tab menu or a drag: its path is
      # not there to drive, which is not a fault of the path.
      $missing = Read-Log | Where-Object { $_ -match '^feature split: not in this build' } | Select-Object -First 1
      if ($missing -and $Path -ne 'message') { return @('NOT_DRIVEN', $split.Path, "this build has no recorded split view for the $Path path: $missing") }
      return @($on.Verdict, $split.Path, "after the $Path path: $($on.Reason). trace: $log")
    }
    if (-not (Test-ViewOwns $opens 'Split')) { return @('FAIL', $split.Path, "after the $Path path two pages are shown, but the second page is not in the second page view") }
    $detail = "after the $Path path: $($on.Reason)"
    if ($ThenClick) {
      # A click in the page that is not active, posted to the window under it as a real click would be.
      $r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
      $p = New-Object FeatureWin+P; $p.X = $r.L + 700; $p.Y = $r.T + 600
      $target = [FeatureWin]::Inside($ctx.Hwnd, $p)
      if ($target -eq [IntPtr]::Zero) { return @('NOT_DRIVEN', $split.Path, 'no window found at the first pane') }
      $c = New-Object FeatureWin+P; $c.X = $p.X; $c.Y = $p.Y
      [FeatureWin]::ScreenToClient($target, [ref]$c) | Out-Null
      $l = [IntPtr](([int]$c.Y -shl 16) -bor ([int]$c.X -band 0xffff))
      [FeatureWin]::PostMessage($target, 0x201, [IntPtr]1, $l) | Out-Null
      Start-Sleep -Milliseconds 150
      [FeatureWin]::PostMessage($target, 0x202, [IntPtr]0, $l) | Out-Null
      Start-Sleep 4
      $clicked = Get-Capture $ctx (Join-Path $shots "$Feature-5-after-click.png")
      $still = Test-SplitShown $clicked 'colour'
      if ($still.Verdict -ne 'PASS') { return @($still.Verdict, $clicked.Path, "after a click into the first pane the split is gone: $($still.Reason)") }
      if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $clicked.Path, 'the runtime did not take the tab list command') }
      $active = Get-ActiveTitle
      if ($active -ne 'page-a') { return @('NOT_DRIVEN', $clicked.Path, "both panes stay after the click, but it did not move the active tab (active: $active), so a posted click may not focus a pane") }
      return @('PASS', $clicked.Path, 'both panes stay after a click into the first pane and it became the active tab')
    }
    # Ending the split must leave one page.
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $off = Get-Capture $ctx (Join-Path $shots "$Feature-6-off.png")
    $ended = Test-SplitShown $off $shown
    if ($ended.Verdict -eq 'PASS') { return @('FAIL', $off.Path, "$detail; but turning it off left two pages") }
    if (Test-Blank $off) { return @('FAIL', $off.Path, "$detail; but turning it off left a blank window") }
    return @('PASS', $split.Path, "$detail; off: back to one page")
  } finally {
    Stop-Scene $ctx
    if ($seeded) { Remove-Item $seeded -ErrorAction SilentlyContinue }
  }
}

# A split beyond two pages beside each other: a tab dropped on the top or bottom of the page stacks
# the pages, and further drops make grids of three and four pages. The drops come from the drop
# flag, which names a side and the slot of the page to split, as a tab drop on that page's edge does.
# Every page must be one of Rivet's own page views.
function Invoke-SplitGrid($Name, $shots, [string]$Feature) {
  $ctx, $state = Start-Scene $Name $Feature $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots "$Feature-1-page-a.png")
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    $v = Test-OnePage $one $script:ColourA $script:ColourB
    if ($v.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $one.Path, "the first test page was not shown: $($v.Reason)") }
    # The pages, the drops and the arrangement they must give. The last page opened is active and
    # is the window's own page; each drop takes the first tab that is not shown yet.
    $plan = switch ($Feature) {
      'split-drop-top' { @{ Urls = @($urlB); Drops = @('top'); Colours = @($script:ColourA, $script:ColourB); Shape = 'above' } }
      'split-drop-bottom' { @{ Urls = @($urlB); Drops = @('bottom'); Colours = @($script:ColourB, $script:ColourA); Shape = 'above' } }
      'split-grid-3' { @{ Urls = @($urlB, $urlC); Drops = @('right', 'bottom 1'); Colours = @($script:ColourC, $script:ColourA, $script:ColourB); Shape = 'three' } }
      'split-grid-4' { @{ Urls = @($urlB, $urlC, $urlD); Drops = @('right', 'bottom', 'bottom 1'); Colours = @($script:ColourD, $script:ColourB, $script:ColourA, $script:ColourC); Shape = 'four' } }
    }
    foreach ($url in $plan.Urls) {
      if (-not (Open-SecondPage $ctx $url)) { return @('NOT_DRIVEN', $one.Path, 'a page did not open as a tab of the app window alone') }
    }
    $before = Get-Capture $ctx (Join-Path $shots "$Feature-2-before.png")
    # The check must fail on this window, which has its tabs and no split.
    $control = Test-GridPages $before $plan.Colours $plan.Shape
    if ($control.Verdict -ne 'FAIL') { return @('NOT_DRIVEN', $before.Path, "the grid check did not fail on a window without a split ($($control.Verdict)): $($control.Reason)") }
    $opens = Get-ViewOpens 'Split'
    $added = @(Read-Log | Where-Object { $_ -match 'second view: slot \d' }).Count
    foreach ($drop in $plan.Drops) {
      if (-not (Send-Flag $ctx 'rivet-drop.flag' $drop)) { return @('NOT_DRIVEN', $before.Path, "the runtime did not take the drop '$drop'") }
      Start-Sleep 3
    }
    Start-Sleep 3
    $grid = Get-Capture $ctx (Join-Path $shots "$Feature-3-grid.png")
    $on = Test-GridPages $grid $plan.Colours $plan.Shape
    if ($on.Verdict -ne 'PASS') {
      $log = (Read-Log | Where-Object { $_ -match 'drop flag|second view' } | Select-Object -Last 4) -join ' / '
      return @($on.Verdict, $grid.Path, "after the drops: $($on.Reason). trace: $log")
    }
    if (-not (Test-ViewOwns $opens 'Split')) { return @('FAIL', $grid.Path, 'the pages are shown, but the split did not open in a page view of Rivet') }
    $more = @(Read-Log | Where-Object { $_ -match 'second view: slot \d' }).Count - $added
    if ($more -lt $plan.Drops.Count - 1) { return @('FAIL', $grid.Path, "the pages are shown, but only $more of the $($plan.Drops.Count - 1) later pages are in Rivet's page views") }
    $pages = Read-Log | Where-Object { $_ -match 'drop flag: .* pages \d+' } | Select-Object -Last 1
    if (-not ($pages -match "pages $($plan.Colours.Count)\b")) { return @('FAIL', $grid.Path, "the runtime reports a split of another size: $pages") }
    # Ending the split must leave one page.
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $off = Get-Capture $ctx (Join-Path $shots "$Feature-4-off.png")
    if ((Test-GridPages $off $plan.Colours $plan.Shape).Verdict -eq 'PASS') { return @('FAIL', $off.Path, "$($on.Reason); but turning it off left the grid") }
    if (Test-Blank $off) { return @('FAIL', $off.Path, "$($on.Reason); but turning it off left a blank window") }
    @('PASS', $grid.Path, "$($on.Reason); off: the grid ended")
  } finally { Stop-Scene $ctx }
}

# A click on a row button, with the client messages a real click makes, posted without moving the
# pointer. `X` and `Y` are capture pixels, which start at the window's corner.
function Send-RowClick($ctx, [int]$X, [int]$Y) {
  $r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
  $o = New-Object FeatureWin+P; $o.X = 0; $o.Y = 0
  [FeatureWin]::ClientToScreen($ctx.Hwnd, [ref]$o) | Out-Null
  $cx = $X - ($o.X - $r.L); $cy = $Y - ($o.Y - $r.T)
  Send-ClientMouse $ctx 0x201 $cx $cy 1
  Start-Sleep -Milliseconds 120
  Send-ClientMouse $ctx 0x202 $cx $cy
}

# The window's hit test at a capture pixel, which starts at the window's corner. The window answers
# it synchronously and nothing is clicked, so a minimise or maximise button is checked without
# moving the window onto a monitor.
function Get-HitTest($ctx, [int]$X, [int]$Y) {
  $r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
  $sx = $r.L + $X; $sy = $r.T + $Y
  $l = [IntPtr]((($sy -band 0xffff) -shl 16) -bor ($sx -band 0xffff))
  [int][FeatureWin]::SendMessage($ctx.Hwnd, 0x84, [IntPtr]::Zero, $l)
}

# The rows of a split: the window buttons answer on the right page's row only, a click on the other
# page's address moves the focus and the address field there, and an address typed there opens in
# that page alone.
function Invoke-SplitRows($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'split-rows' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    if (-not (Open-SecondPage $ctx $urlB)) { return @('NOT_DRIVEN', $null, 'the second page did not open as a tab of the app window alone') }
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $split = Get-Capture $ctx (Join-Path $shots 'split-rows-1-on.png')
    $on = Test-TwoPages $split $script:ColourA $script:ColourB
    if ($on.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $split.Path, "no split to drive: $($on.Reason)") }
    $s = $ctx.Dpi / 96.0
    $a = Get-PageRegion $split $script:ColourA
    $b = Get-PageRegion $split $script:ColourB
    $left, $right = if ($a.X0 -lt $b.X0) { $a, $b } else { $b, $a }
    $leftColour = if ($a.X0 -lt $b.X0) { $script:ColourA } else { $script:ColourB }
    $rightColour = if ($a.X0 -lt $b.X0) { $script:ColourB } else { $script:ColourA }
    $rowY = [int]($right.Y0 - 22 * $s)
    $notes = @()
    # Close, maximise and minimise from the right end, 46 DIP each.
    $want = @(@('close', 20, 23), @('maximise', 9, 69), @('minimise', 8, 115))
    foreach ($w in $want) {
      $code = Get-HitTest $ctx ([int]($right.X1 - $w[2] * $s)) $rowY
      if ($code -ne $w[1]) { return @('FAIL', $split.Path, "the $($w[0]) button of the right page's row answers hit test $code, not $($w[1])") }
      $notes += "$($w[0]) $code"
    }
    # The left page's row carries no window buttons.
    $code = Get-HitTest $ctx ([int]($left.X1 - 23 * $s)) $rowY
    if (@(8, 9, 20) -contains $code) { return @('FAIL', $split.Path, "the left page's row answers window button hit test $code") }
    $notes += "left row end $code"
    # A click on the left page's address focuses that page and moves the field into its row.
    $before = @(Read-Log).Count
    $ax = [int](($left.X0 + $left.X1) / 2)
    Send-RowClick $ctx $ax $rowY
    Start-Sleep 2
    $focused = Get-Capture $ctx (Join-Path $shots 'split-rows-2-left-focused.png')
    $log = @(Read-Log | Select-Object -Skip $before)
    if (-not ($log | Where-Object { $_ -match 'split row: Address of page 0: true' })) { return @('FAIL', $focused.Path, 'the click on the left address did not reach the row') }
    $leftEdit = $log | Where-Object { $_ -match '^page row 0: .* editable true' } | Select-Object -Last 1
    $rightEdit = $log | Where-Object { $_ -match '^page row 1: .* editable (true|false)' } | Select-Object -Last 1
    if (-not $leftEdit -or ($rightEdit -and $rightEdit -match 'editable true')) { return @('FAIL', $focused.Path, 'the address field did not move into the left page''s row') }
    $notes += 'field moved to the left row'
    # Type an address there: it opens in the left page, and the right page keeps its own.
    $urlC = New-TestPage 'page-c' $script:ColourC
    Send-RowClick $ctx $ax $rowY
    Start-Sleep -Milliseconds 1200
    foreach ($ch in $urlC.ToCharArray()) { [FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr][int]$ch, [IntPtr]1) | Out-Null }
    Start-Sleep -Milliseconds 1500
    [FeatureWin]::PostMessage($ctx.Hwnd, 0x100, [IntPtr]0x0D, [IntPtr]1) | Out-Null
    [FeatureWin]::PostMessage($ctx.Hwnd, 0x101, [IntPtr]0x0D, [IntPtr]0xC0000001) | Out-Null
    # Every typed character refreshes the suggestions, so a busy machine takes a while to reach
    # the Enter, which is queued after them. The capture waits for the runtime to open the address.
    foreach ($i in 1..60) { if (@(Read-Log | Select-Object -Skip $before | Where-Object { $_ -match '^address bar: open ' }).Count -gt 0) { break }; Start-Sleep -Milliseconds 500 }
    Start-Sleep 4
    $typed = Get-Capture $ctx (Join-Path $shots 'split-rows-3-typed.png')
    $two = Test-TwoPages $typed $script:ColourC $rightColour
    if ($two.Verdict -ne 'PASS') { return @('FAIL', $typed.Path, "after typing an address in the left row: $($two.Reason)") }
    $c = Get-PageRegion $typed $script:ColourC
    if ([Math]::Abs($c.X0 - $left.X0) -gt 4 * $s) { return @('FAIL', $typed.Path, 'the typed address did not open in the left page') }
    $notes += 'typed address opened in the left page'
    return @('PASS', $typed.Path, ($notes -join '; '))
  } finally { Stop-Scene $ctx }
}

# One tab only: what each path does. Neither may lose the page or leave the window blank.
function Invoke-SplitOneTab($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'split-one-tab' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots 'split-one-tab-1.png')
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    $v = Test-OnePage $one $script:ColourA $script:ColourB
    if ($v.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $one.Path, "the test page was not shown: $($v.Reason)") }
    $notes = @()
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $m = Get-Capture $ctx (Join-Path $shots 'split-one-tab-2-message.png')
    $mv = Test-OnePage $m $script:ColourA $script:ColourB
    $notes += "message: $(if ($mv.Verdict -eq 'PASS') { 'no split, page intact' } else { 'the page changed: ' + $mv.Reason })"
    Invoke-TabMenu $ctx 0 (Join-Path $shots 'split-one-tab-3-menu-open.png')
    $n = Get-Capture $ctx (Join-Path $shots 'split-one-tab-4-menu.png')
    $nv = Test-OnePage $n $script:ColourA $script:ColourB
    $notes += "menu: $(if ($nv.Verdict -eq 'PASS') { 'no split, page intact' } else { 'the page changed: ' + $nv.Reason })"
    $notes += 'drag: a lone tab has no other tab to drop, not driven'
    if (Send-Flag $ctx 'rivet-tab.txt' 'list') { $notes += "tabs: $((Get-TabUrls).Count)" }
    $bad = ($mv.Verdict -ne 'PASS') -or ($nv.Verdict -ne 'PASS')
    if ($bad -and ((Test-Blank $m) -or (Test-Blank $n))) { return @('FAIL', $n.Path, ($notes -join '; ')) }
    # A split made with a new tab would be a fair outcome as well: it is reported, not judged.
    return @($(if ($bad) { 'NOT_DRIVEN' } else { 'PASS' }), $n.Path, ($notes -join '; '))
  } finally { Stop-Scene $ctx }
}

# The shell demo's layout, from one headless render of it at a device scale of one. Headless opens no
# window at all, so nothing appears on screen. The render runs in a profile of its own in OutDir
# that only this step uses, and only processes holding that profile are stopped. The PNG is kept as
# evidence beside the measured rectangles.
# An elevated browser otherwise relaunches itself de-elevated and the first process exits with no output.
$script:DemoGeometry = $null
# A file another process may still hold open for writing, read without asking for sole access. The
# read is tried again for a few seconds while the writer lets go. Null when it never could be read.
function Read-SharedText([string]$Path) {
  for ($try = 0; $try -lt 20; $try++) {
    try {
      $stream = [IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
      try { return (New-Object IO.StreamReader($stream)).ReadToEnd() } finally { $stream.Dispose() }
    } catch [IO.IOException] { Start-Sleep -Milliseconds 500 }
  }
  $null
}
# The demo is rendered once with Chrome, whose headless mode works, and the render is kept beside
# every browser's results, so each browser is compared with the same image. Brave's headless launch
# left no screenshot. A render is reused only while the demo file is unchanged.
function Get-DemoGeometry {
  if ($script:DemoGeometry) { return $script:DemoGeometry }
  $source = Join-Path $root 'runtime\windows\demo\rivet-shell-demo.html'
  if (-not (Test-Path $source)) { return $null }
  $ExePath, $ExeName = $table['chrome']
  if (-not (Test-Path $ExePath)) { return $null }
  $hash = (Get-FileHash -Algorithm SHA256 $source).Hash.Substring(0, 16)
  # Each run renders into its own folder, so runs in parallel never write the same file.
  $dir = Join-Path $OutDir "demo-render\$hash"
  New-Item -ItemType Directory -Force $dir | Out-Null
  $png = Join-Path $dir 'rivet-shell-demo.png'
  $kept = Join-Path $dir 'rivet-shell-demo.geometry.json'
  if ((Test-Path $png) -and (Test-Path $kept)) {
    $g = ConvertFrom-DemoGeometry ([IO.File]::ReadAllText($kept))
    $g | Add-Member Image $png
    $script:DemoGeometry = $g
    return $g
  }
  $measure = @'
<script>
(function () {
  function box(selector) { var e = document.querySelector(selector); if (!e) return null; var r = e.getBoundingClientRect(); return { left: r.left, top: r.top, width: r.width, height: r.height }; }
  function write() {
    var out = document.getElementById('rivet-geometry') || document.body.appendChild(Object.assign(document.createElement('pre'), { id: 'rivet-geometry' }));
    out.hidden = true;
    out.textContent = JSON.stringify({ window: box('.window'), panel: box('.panel'), toolbar: box('.panel .toolbar'), page: box('.panel .page') });
  }
  write();
  addEventListener('load', write);
})();
</script>
'@
  $html = [IO.File]::ReadAllText($source)
  $at = $html.LastIndexOf('</body>')
  $html = if ($at -ge 0) { $html.Insert($at, $measure) } else { $html + $measure }
  $page = Join-Path $dir 'rivet-shell-demo-measured.html'
  [IO.File]::WriteAllText($page, $html)
  $url = 'file:///' + ($page -replace '\\', '/')
  $profile_ = Join-Path $dir 'profile'
  Stop-Mine $ExeName $profile_
  $common = @('--headless=new', '--do-not-de-elevate', "--user-data-dir=`"$profile_`"", '--disable-sync', '--no-first-run', '--no-default-browser-check', '--disable-features=msImplicitSignin,msEdgeSignIn,Translate', '--lang=en-US', '--window-size=1600,1000', '--force-device-scale-factor=1', '--hide-scrollbars', '--virtual-time-budget=3000')
  $dom = Join-Path $dir 'rivet-shell-demo.dom.html'
  $dump = $null; $shot = $null
  try {
    $shot = Start-TestBrowser -FilePath $ExePath -ArgumentList ($common + @("--screenshot=`"$png`"", $url))
    if (-not $shot.WaitForExit(60000)) { return $null }
    $dump = Start-TestBrowser -FilePath $ExePath -ArgumentList ($common + @('--dump-dom', $url)) -RedirectStandardOutput $dom
    if (-not $dump.WaitForExit(60000)) { return $null }
  } finally {
    Stop-Mine $ExeName $profile_
    # The redirected output stays open while the process object or a child of the dump holds it.
    if ($dump) { $dump.Dispose() }
    if ($shot) { $shot.Dispose() }
  }
  $text = Read-SharedText $dom
  if ($null -eq $text) { return $null }
  $m = [regex]::Match($text, '<pre id="rivet-geometry"[^>]*>([^<]*)</pre>')
  if (-not $m.Success) { return $null }
  $json = [Net.WebUtility]::HtmlDecode($m.Groups[1].Value)
  if ($json -match ':null') { return $null }
  if (-not (Test-Path $png)) { return $null }
  [IO.File]::WriteAllText($kept, $json)
  $g = ConvertFrom-DemoGeometry $json
  $g | Add-Member Image $png
  $script:DemoGeometry = $g
  $g
}

# The address bar is at the top of the page area and the page starts below it, and the whole window
# matches the shell demo's regions: the sidebar's width, the bar row at the top, the page below it.
function Invoke-BarLayout($Name, $shots) {
  $demo = Get-DemoGeometry
  $ctx, $state = Start-Scene $Name 'bar-layout' $urlTop
  try {
    Open-Scene $ctx $state $urlTop $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots 'bar-layout-1.png')
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    $scale = $ctx.Dpi / 96.0
    $barDip = if ($demo) { $demo.BarHeight } else { 44 }
    $bar = Test-BarAtTop $one $script:ColourA $script:ColourC $scale $barDip
    if ($bar.Verdict -ne 'PASS') { return @($bar.Verdict, $one.Path, $bar.Reason) }
    # The page starts right at the bar's bottom. Only a browser infobar may sit between them.
    $under = Test-PageUnderBar $one $bar $scale $barDip
    if ($under.Verdict -ne 'PASS') { return @($under.Verdict, $one.Path, $under.Reason) }
    if (-not $demo) { return @('NOT_DRIVEN', $one.Path, "$($bar.Reason); the demo could not be rendered, so the window was not compared with it") }
    $v = Compare-ShellLayout $bar $demo $scale
    @($v.Verdict, $one.Path, "$($v.Reason); $($under.Reason); demo render $($demo.Image)")
  } finally { Stop-Scene $ctx }
}

# A Mini Window command, posted the way `rivet session mini-window` posts it. The codes are the
# protocol's: 1 pop out, 2 move (x | y << 16 in DIPs of the Rivet window's client area), 4 return (0 left, 1 right, 2 top, 3 bottom), 5 close.
function Send-MiniCommand($ctx, [int]$Code, [int]$Low = 0, [int]$High = 0) {
  $l = [IntPtr](($High -shl 16) -bor ($Low -band 0xffff))
  [FeatureWin]::PostMessage($ctx.Hwnd, [FeatureWin]::RegisterWindowMessage('Rivet.MiniWindow'), [IntPtr]$Code, $l) | Out-Null
}

# The Mini Window is a browser window of its own: a new visible top-level window of the same
# browser process, off every monitor like the test window, small, and placed near the Rivet window.
function Find-MiniWindow($ctx, $before) {
  $own = [uint32]0
  [FeatureWin]::GetWindowThreadProcessId($ctx.Hwnd, [ref]$own) | Out-Null
  foreach ($i in 1..80) {
    foreach ($t in @(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $before -notcontains $_ })) {
      $q = [uint32]0
      [FeatureWin]::GetWindowThreadProcessId($t, [ref]$q) | Out-Null
      if ($q -eq $own) { return $t }
      # A window of another process is no Mini Window. It is moved away and closed at once.
      [FeatureWin]::SetWindowPos($t, [IntPtr]::Zero, -3000, -3000, 1600, 1000, 0x14) | Out-Null
      [FeatureWin]::PostMessage($t, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    }
    Start-Sleep -Milliseconds 50
  }
  [IntPtr]::Zero
}

function Get-Rect([IntPtr]$h) {
  $r = New-Object FeatureWin+R
  [FeatureWin]::GetWindowRect($h, [ref]$r) | Out-Null
  $r
}

# Whether the Mini Window is a small window beside the Rivet window that shows page $Float.
function Test-MiniWindow($ctx, [IntPtr]$mini, [string]$Path, [int]$Float) {
  if ($mini -eq [IntPtr]::Zero) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = 'no new top-level window of the browser process appeared'; Capture = $null } }
  # A Mini Window on a monitor is moved off at once, so the test never shows a window.
  if ([FeatureMonitor]::OnMonitor($mini)) {
    [FeatureWin]::SetWindowPos($mini, [IntPtr]::Zero, -3000, -3000, 0, 0, 0x15) | Out-Null
    return [pscustomobject]@{ Verdict = 'FAIL'; Reason = 'the Mini Window opened on a monitor, not beside the off-screen Rivet window'; Capture = $null }
  }
  $m = Get-Rect $mini; $w = Get-Rect $ctx.Hwnd
  $s = $ctx.Dpi / 96.0
  $mw = ($m.Ri - $m.L) / $s; $mh = ($m.B - $m.T) / $s
  if ($mw -gt 640 -or $mh -gt 480 -or $mw -lt 200 -or $mh -lt 120) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = "the Mini Window is $([int]$mw)x$([int]$mh) DIP, not a small window"; Capture = $null } }
  $near = $m.L -lt $w.Ri + 200 * $s -and $m.Ri -gt $w.L - 200 * $s -and $m.T -lt $w.B + 200 * $s -and $m.B -gt $w.T - 200 * $s
  if (-not $near) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = "the Mini Window at ($($m.L),$($m.T)) is not near the Rivet window at ($($w.L),$($w.T))"; Capture = $null } }
  $shot = Get-Capture ([pscustomobject]@{ Hwnd = $mini }) $Path
  if (-not $shot) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = 'the Mini Window could not be captured'; Capture = $null } }
  $page = Get-PageRegion $shot $Float
  if ($page.Area -le 0) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = 'the Mini Window does not show the popped page'; Capture = $shot } }
  [pscustomobject]@{ Verdict = 'PASS'; Reason = "a $([int]$mw)x$([int]$mh) DIP window of the same process at ($($m.L),$($m.T)) beside the Rivet window shows the page"; Capture = $shot; Rect = $m }
}

# The Mini Window from a split: pop out, then one of move, return or close.
function Invoke-Mini($Name, $shots, [string]$Step) {
  $ctx, $state = Start-Scene $Name "mini-$Step" $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    if (-not (Open-SecondPage $ctx $urlB)) { return @('NOT_DRIVEN', $null, 'the second page did not open as a tab of the app window alone') }
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $split = Get-Capture $ctx (Join-Path $shots "mini-$Step-1-split.png")
    $on = Test-TwoPages $split $script:ColourA $script:ColourB
    if ($on.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $split.Path, "no split to pop out of: $($on.Reason)") }
    # Page B is the second pane, so it is focused first and then popped out.
    $before = @(Read-Log).Count
    $b = Get-PageRegion $split $script:ColourB
    $p = New-Object FeatureWin+P; $p.X = [int](($b.X0 + $b.X1) / 2); $p.Y = [int](($b.Y0 + $b.Y1) / 2)
    Send-ClientMouse $ctx 0x201 $p.X $p.Y 1
    Send-ClientMouse $ctx 0x202 $p.X $p.Y 0
    Start-Sleep 2
    $tops = Get-MyTops $ctx
    Send-MiniCommand $ctx 1
    # Rivet asks the browser for the Mini Window's own app window, which takes a moment to come.
    $log = @()
    foreach ($i in 1..60) {
      Start-Sleep -Milliseconds 250
      $log = @(Read-Log | Select-Object -Skip $before)
      if ($log | Where-Object { $_ -match '^mini window: (open tab|kept the tab|off, |this build cannot move a tab)' }) { break }
    }
    Start-Sleep 1
    $mini = Find-MiniWindow $ctx $tops
    $kept = @($log | Where-Object { $_ -match '^mini window: kept the tab, ' } | Select-Object -First 1)
    if ($kept) { return @('FAIL', $null, ('the Mini Window did not open and the page stayed: {0}' -f ($kept[0] -replace '^mini window: kept the tab, ', ''))) }
    if (-not ($log | Where-Object { $_ -match '^mini window: open tab' })) {
      $off = @($log | Where-Object { $_ -match '^mini window: off, ' } | Select-Object -First 1)
      if ($off) { return @('NOT_DRIVEN', $null, ('the Mini Window is off on this build: {0}' -f ($off[0] -replace '^mini window: off, ', ''))) }
      if ($log | Where-Object { $_ -match '^mini window: this build cannot move a tab' }) { return @('NOT_DRIVEN', $null, 'this build has no recorded tab detach or insert function') }
      return @('FAIL', $null, 'the runtime did not open a Mini Window for the pop-out command')
    }
    # The focused pane pops out. A posted click may not move the focus, so either page may float.
    $float, $stay = $script:ColourB, $script:ColourA
    $shown = Test-MiniWindow $ctx $mini (Join-Path $shots "mini-$Step-2-mini.png") $float
    if ($shown.Verdict -ne 'PASS') {
      $other = Test-MiniWindow $ctx $mini (Join-Path $shots "mini-$Step-2-mini.png") $script:ColourA
      if ($other.Verdict -eq 'PASS') { $shown = $other; $shown.Reason = "page A popped out, not the focused page B; $($other.Reason)"; $float, $stay = $script:ColourA, $script:ColourB }
    }
    if ($shown.Verdict -ne 'PASS') { return @('FAIL', $(if ($shown.Capture) { $shown.Capture.Path }), "pop out: $($shown.Reason)") }
    $main = Get-Capture $ctx (Join-Path $shots "mini-$Step-3-window.png")
    $left = Test-OnePage $main $stay $float
    if ($left.Verdict -ne 'PASS') { return @('FAIL', $main.Path, "the Rivet window after the pop-out: $($left.Reason)") }
    if ($Step -eq 'pop-out') { return @('PASS', $shown.Capture.Path, "$($shown.Reason); the Rivet window shows the other page alone") }
    $s = $ctx.Dpi / 96.0
    switch ($Step) {
      'move' {
        # 40 DIP right and down of the Rivet window's client origin, which is off every monitor.
        Send-MiniCommand $ctx 2 40 40
        Start-Sleep 2
        $o = New-Object FeatureWin+P; [FeatureWin]::ClientToScreen($ctx.Hwnd, [ref]$o) | Out-Null
        $r = Get-Rect $mini
        $moved = Get-Capture ([pscustomobject]@{ Hwnd = $mini }) (Join-Path $shots 'mini-move-4-moved.png')
        if ([FeatureMonitor]::OnMonitor($mini)) { return @('FAIL', $moved.Path, 'the move put the Mini Window on a monitor') }
        $want = @([int]($o.X + 40 * $s), [int]($o.Y + 40 * $s))
        if ([Math]::Abs($r.L - $want[0]) -gt 2 -or [Math]::Abs($r.T - $want[1]) -gt 2) { return @('FAIL', $moved.Path, "the Mini Window is at ($($r.L),$($r.T)), not at ($($want[0]),$($want[1]))") }
        return @('PASS', $moved.Path, "moved from ($($shown.Rect.L),$($shown.Rect.T)) to ($($r.L),$($r.T))")
      }
      'return' {
        Send-MiniCommand $ctx 4 0
        Start-Sleep 6
        $back = Get-Capture $ctx (Join-Path $shots 'mini-return-4-split.png')
        if (@(Get-MyTops $ctx) -contains $mini) { return @('FAIL', $back.Path, 'the Mini Window is still open after the return') }
        $two = Test-TwoPages $back $script:ColourA $script:ColourB
        if ($two.Verdict -ne 'PASS') { return @('FAIL', $back.Path, "after the return: $($two.Reason)") }
        $a = Get-PageRegion $back $stay; $bb = Get-PageRegion $back $float
        if ($bb.X0 -ge $a.X0) { return @('FAIL', $back.Path, 'the popped page came back as a split, but not on the left edge') }
        if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $back.Path, 'the runtime did not take the tab list command') }
        $count = (Get-TabUrls).Count
        if ($count -ne 2) { return @('FAIL', $back.Path, "the window has $count tabs after the return, not the same two") }
        return @('PASS', $back.Path, "the Mini Window closed and its page is back on the left as a split of the same tab; $($two.Reason)")
      }
      'close' {
        Send-MiniCommand $ctx 5
        Start-Sleep 3
        $closed = Get-Capture $ctx (Join-Path $shots 'mini-close-4-closed.png')
        if (@(Get-MyTops $ctx) -contains $mini) { return @('FAIL', $closed.Path, 'the Mini Window is still open after the close') }
        $one = Test-OnePage $closed $stay $float
        if ($one.Verdict -ne 'PASS') { return @('FAIL', $closed.Path, "after the close: $($one.Reason)") }
        if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $closed.Path, 'the runtime did not take the tab list command') }
        $count = (Get-TabUrls).Count
        if ($count -ne 2) { return @('FAIL', $closed.Path, "the window has $count tabs after the close; the page should be back in its tab") }
        return @('PASS', $closed.Path, 'the Mini Window closed and its page is back as a background tab of the Rivet window')
      }
    }
  } finally { Stop-Scene $ctx }
}

# --- FR-3 tab list ------------------------------------------------------------------------
# A page that is one link opening `page-c` in a new window. The window is asked for at the test
# window's off-screen spot, so it never shows on a monitor even before Rivet takes its tab.
$urlLink = & {
  $path = Join-Path $pageDir 'page-link.html'
  $hexA = '#{0:X6}' -f $script:ColourA
  [IO.File]::WriteAllText($path, "<!doctype html><meta charset=utf-8><title>page-link</title><style>html,body{margin:0;height:100%;background:$hexA}</style><a href=`"page-c.html`" onclick=`"window.open(this.href,'_blank','left=9000,top=1000,width=800,height=600');return false`" style=`"display:block;width:100%;height:100%`"></a>")
  'file:///' + ($path -replace '\\', '/')
}

Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class FeatureMonitor{
 [StructLayout(LayoutKind.Sequential)] public struct R{public int L,T,Ri,B;}
 [DllImport("user32.dll")] static extern IntPtr MonitorFromRect(ref R r,uint f);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out R r);
 // Whether any part of the window lies on a monitor. MONITOR_DEFAULTTONULL answers none for a
 // rectangle that touches no monitor.
 public static bool OnMonitor(IntPtr h){R r;if(!GetWindowRect(h,out r))return false;return MonitorFromRect(ref r,0)!=IntPtr.Zero;}
}
'@

# The last `tab sections` trace, split into its parts.
function Get-Sections {
  $line = Read-Log | Where-Object { $_ -match 'tab sections ' } | Select-Object -Last 1
  if (-not $line) { return $null }
  $part = { param($name) if ($line -match "$name (\[[^\]]*\])") { @([regex]::Matches($Matches[1], '"([^"]*)"') | ForEach-Object { $_.Groups[1].Value }) } else { @() } }
  [pscustomobject]@{
    Line = $line
    Pinned = & $part 'pinned'
    Today = & $part 'today'
    Order = & $part 'order'
    Closed = if ($line -match 'closed (\d+)') { [int]$Matches[1] } else { -1 }
    Archived = if ($line -match 'archived (\d+)') { [int]$Matches[1] } else { -1 }
    CtrlT = if ($line -match 'ctrl_t (\S+)') { $Matches[1] } else { $null }
  }
}

# A window with the pages a, b and c as its three tabs, or the reason it could not be made.
function Open-ThreeTabs($ctx, $state, $shots, [string]$Prefix) {
  Open-Scene $ctx $state $urlA $null
  if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return $ctx.Error }
  if (-not (Test-Attached $ctx)) { return 'Rivet did not attach to the window' }
  if (-not (Open-SecondPage $ctx $urlB)) { return 'the second page did not open as a tab of the app window alone' }
  if (-not (Open-SecondPage $ctx $urlC)) { return 'the third page did not open as a tab of the app window alone' }
  $null
}

function Invoke-TabSections($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'tab-sections' $urlA
  try {
    $why = Open-ThreeTabs $ctx $state $shots 'tab-sections'
    if ($why) { return @('NOT_DRIVEN', $null, $why) }
    foreach ($command in 'pin 2', 'folder 1 Launch Atlas', 'into 0', 'sections') {
      if (-not (Send-Flag $ctx 'rivet-tab.txt' $command)) { return @('NOT_DRIVEN', $null, "the runtime did not take '$command'") }
    }
    $shot = Get-Capture $ctx (Join-Path $shots 'tab-sections-1.png')
    $s = Get-Sections
    if (-not $s) { return @('NOT_DRIVEN', $shot.Path, 'the runtime reported no sections') }
    if ($s.Pinned -notcontains 'page-c') { return @('FAIL', $shot.Path, "page-c is not pinned: $($s.Line)") }
    if ($s.Line -notmatch 'Launch Atlas:2:open') { return @('FAIL', $shot.Path, "the folder does not hold two tabs: $($s.Line)") }
    if (($s.Order -join ',') -ne 'page-c,page-b,page-a') { return @('FAIL', $shot.Path, "the order is not pinned, folder, today: $($s.Order -join ',')") }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'foldertoggle') -or -not (Send-Flag $ctx 'rivet-tab.txt' 'sections')) { return @('NOT_DRIVEN', $shot.Path, 'the folder toggle was not taken') }
    if ((Get-Sections).Line -notmatch 'Launch Atlas:2:collapsed') { return @('FAIL', $shot.Path, 'the folder did not collapse') }
    $closed = Get-Capture $ctx (Join-Path $shots 'tab-sections-2.png')
    @('PASS', $closed.Path, "pinned page-c, folder Launch Atlas with 2 tabs collapsed, check the captures for the count and rows")
  } finally { Stop-Scene $ctx }
}

function Invoke-TabCloseSelect($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'tab-close-select' $urlA
  try {
    $why = Open-ThreeTabs $ctx $state $shots 'tab-close-select'
    if ($why) { return @('NOT_DRIVEN', $null, $why) }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'activate 1')) { return @('NOT_DRIVEN', $null, 'activate was not taken') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'closesel 1')) { return @('NOT_DRIVEN', $null, 'closesel was not taken') }
    $below = Get-ActiveTitle
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'closesel 1')) { return @('NOT_DRIVEN', $null, 'the second closesel was not taken') }
    $above = Get-ActiveTitle
    $shot = Get-Capture $ctx (Join-Path $shots 'tab-close-select-1.png')
    if ($below -ne 'page-c') { return @('FAIL', $shot.Path, "closing page-b selected $below, not the tab below") }
    if ($above -ne 'page-a') { return @('FAIL', $shot.Path, "closing the last tab selected $above, not the tab above") }
    @('PASS', $shot.Path, 'below, then above')
  } finally { Stop-Scene $ctx }
}

function Invoke-TabReopen($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'tab-reopen' $urlA
  try {
    $why = Open-ThreeTabs $ctx $state $shots 'tab-reopen'
    if ($why) { return @('NOT_DRIVEN', $null, $why) }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'closesel 1')) { return @('NOT_DRIVEN', $null, 'closesel was not taken') }
    # Ctrl+Shift+T posts this message; posted keys do not change GetKeyState, so the message is sent.
    Send-RivetMessage $ctx 'Rivet.ReopenTab'
    Start-Sleep 5
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'sections')) { return @('NOT_DRIVEN', $null, 'sections was not taken') }
    $shot = Get-Capture $ctx (Join-Path $shots 'tab-reopen-1.png')
    $s = Get-Sections
    if (($s.Order -join ',') -ne 'page-a,page-b,page-c') { return @('FAIL', $shot.Path, "the reopened tab is not where it was: $($s.Order -join ',')") }
    @('PASS', $shot.Path, 'page-b came back between page-a and page-c')
  } finally { Stop-Scene $ctx }
}

function Invoke-TabArchive($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'tab-archive' $urlA
  try {
    $why = Open-ThreeTabs $ctx $state $shots 'tab-archive'
    if ($why) { return @('NOT_DRIVEN', $null, $why) }
    foreach ($command in 'sections', 'pin 0', 'archive 2', 'age 1', 'sections') { if (-not (Send-Flag $ctx 'rivet-tab.txt' $command)) { return @('NOT_DRIVEN', $null, "'$command' was not taken") } }
    if ((Get-Sections).Archived -ne 0) { return @('FAIL', $null, 'tabs archived before the period') }
    if ((Get-Sections).Line -notmatch 'archive_after Some\(7200\)') { return @('FAIL', $null, "the period setting was not kept: $((Get-Sections).Line)") }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'age 2')) { return @('NOT_DRIVEN', $null, 'age was not taken') }
    Start-Sleep 3
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'sections')) { return @('NOT_DRIVEN', $null, 'sections was not taken') }
    $shot = Get-Capture $ctx (Join-Path $shots 'tab-archive-1.png')
    $s = Get-Sections
    # page-c is active and page-a pinned, so only page-b is archived.
    if ($s.Archived -ne 1 -or $s.Order -contains 'page-b') { return @('FAIL', $shot.Path, "expected page-b alone archived: $($s.Line)") }
    @('PASS', $shot.Path, 'page-b archived after the 2 hour period, pinned and active tabs stayed')
  } finally { Stop-Scene $ctx }
}

function Invoke-CtrlTSetting($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'ctrl-t-setting' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    foreach ($command in 'ctrlt newtab', 'sections') { if (-not (Send-Flag $ctx 'rivet-tab.txt' $command)) { return @('NOT_DRIVEN', $null, "'$command' was not taken") } }
    if ((Get-Sections).CtrlT -ne 'new_tab_page') { return @('FAIL', $null, 'the setting did not change') }
    Send-RivetMessage $ctx 'Rivet.CtrlT'
    Start-Sleep 6
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $null, 'list was not taken') }
    $urls = Get-TabUrls
    $shot = Get-Capture $ctx (Join-Path $shots 'ctrl-t-setting-1.png')
    if ($urls.Count -ne 2 -or $urls[1] -notmatch 'start\.html') { return @('FAIL', $shot.Path, "Ctrl+T with the new tab page setting did not open the start page: $($urls -join ' ')") }
    @('PASS', $shot.Path, 'the new tab page setting opens the start page; the command bar route is checked with the command bar')
  } finally { Stop-Scene $ctx }
}

function Invoke-LinkNewTab($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'link-new-tab' $urlLink
  try {
    Open-Scene $ctx $state $urlLink $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    $before = Get-MyTops $ctx
    $cr = New-Object FeatureWin+R; [FeatureWin]::GetClientRect($ctx.Hwnd, [ref]$cr) | Out-Null
    $page = [FeatureWin]::Inside($ctx.Hwnd, (& { $p = New-Object FeatureWin+P; $p.X = [int]($cr.Ri * 0.7); $p.Y = [int]($cr.B / 2); [FeatureWin]::ClientToScreen($ctx.Hwnd, [ref]$p) | Out-Null; $p }))
    $l = [IntPtr](([int]($cr.B / 2) -shl 16) -bor ([int]($cr.Ri * 0.7) -band 0xffff))
    $target = if ($page -ne [IntPtr]::Zero) { $page } else { $ctx.Hwnd }
    [FeatureWin]::PostMessage($target, 0x201, [IntPtr]1, $l) | Out-Null
    [FeatureWin]::PostMessage($target, 0x202, [IntPtr]0, $l) | Out-Null
    # No window of the test may touch a visible monitor at any time. The popup is asked for
    # off-screen; if any window still lands on a monitor the check fails, and the window is closed.
    $extraSeen = $false; $onMonitor = $false; $landed = @()
    foreach ($i in 1..600) {
      Start-Sleep -Milliseconds 10
      foreach ($t in @(Get-MyTops $ctx)) {
        if ($t -ne $ctx.Hwnd -and $before -notcontains $t) { $extraSeen = $true }
        if ([FeatureMonitor]::OnMonitor($t)) {
          $onMonitor = $true
          $wr = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($t, [ref]$wr) | Out-Null
          $wp = [uint32]0; [FeatureWin]::GetWindowThreadProcessId($t, [ref]$wp) | Out-Null
          $cls = New-Object Text.StringBuilder 64; [FeatureWin]::GetClassName($t, $cls, 64) | Out-Null
          $landed += ('{0} pid {1} class {2} at {3},{4},{5},{6}' -f $t, $wp, $cls, $wr.L, $wr.T, $wr.Ri, $wr.B)
          [FeatureWin]::SetWindowPos($t, [IntPtr]::Zero, 9000, 1000, 800, 600, 0x14) | Out-Null
          if ($t -ne $ctx.Hwnd) { [FeatureWin]::PostMessage($t, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
        }
      }
    }
    # On a test desktop of its own no monitor shows anything, and the guard watches the person's
    # desktop. A window that lands on a monitor there is noted, not a breach of anyone's screen.
    $placed = ($landed | Select-Object -Unique) -join '; '
    if (Test-VisibleBreach $onMonitor $env:RIVET_TEST_DESKTOP) { $shot = Get-Capture $ctx (Join-Path $shots 'link-new-tab-1.png'); return @('FAIL', $shot.Path, "a window of the test appeared on a visible monitor: $placed") }
    Start-Sleep 3
    $left = @(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $before -notcontains $_ })
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $null, 'list was not taken') }
    $urls = Get-TabUrls
    $shot = Get-Capture $ctx (Join-Path $shots 'link-new-tab-1.png')
    if (-not ($urls | Where-Object { $_ -match 'page-c' }) -and -not $extraSeen) { return @('NOT_DRIVEN', $shot.Path, 'the click did not open the link') }
    if ($left.Count -gt 0) { foreach ($t in $left) { [FeatureWin]::PostMessage($t, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }; return @('FAIL', $shot.Path, 'the link stayed in a window of its own') }
    if (-not ($urls | Where-Object { $_ -match 'page-c' })) { return @('FAIL', $shot.Path, "page-c is not a tab of the Rivet window: $($urls -join ' ')") }
    @('PASS', $shot.Path, "the new-window link is a tab of the Rivet window; a window of its own showed first: $extraSeen$(if ($placed) { "; on a monitor of the test desktop: $placed" })")
  } finally { Stop-Scene $ctx }
}

# The command bar opens over the page, finds what is typed, and opens it in a new tab of the window.
function Invoke-CommandBar($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'command-bar' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots 'command-bar-1-before.png')
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'command')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the command bar command') }
    Start-Sleep 1
    if (-not (Read-Log | Where-Object { $_ -match 'command bar: open: true' })) { return @('FAIL', $one.Path, 'the command bar did not open') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' "commandtype $urlB")) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the typing command') }
    Start-Sleep 1
    $open = Get-Capture $ctx (Join-Path $shots 'command-bar-2-open.png')
    if (-not (Read-Log | Where-Object { $_ -match 'command bar rows \[".+' })) { return @('FAIL', $open.Path, 'the command bar listed no rows for the typed address') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'commandrun 0')) { return @('NOT_DRIVEN', $open.Path, 'the runtime did not take the run command') }
    Start-Sleep 4
    $after = Get-Capture $ctx (Join-Path $shots 'command-bar-3-opened.png')
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $after.Path, 'the runtime did not take the tab list command') }
    $urls = Get-TabUrls
    if (-not $urls -or $urls.Count -ne 2) { return @('FAIL', $after.Path, "expected two tabs after Enter, the window has $($urls.Count)") }
    $v = Test-OnePage $after $script:ColourB $script:ColourA
    if ($v.Verdict -ne 'PASS') { return @($v.Verdict, $after.Path, "the new tab does not show the typed page: $($v.Reason)") }
    @('PASS', $open.Path, "bar open, Enter opened a second tab on page-b; $($v.Reason)")
  } finally { Stop-Scene $ctx }
}

# Rivet settings open as a card over the page without a window of their own, are walked with the
# keyboard, and write the store that owns each setting: here Ctrl+T, which the tab list file keeps.
function Invoke-RivetSettings($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'rivet-settings' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    $tops = @(Get-MyTops $ctx)
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'settings')) { return @('NOT_DRIVEN', $null, 'the runtime did not take the settings command') }
    Start-Sleep 1
    $open = Get-Capture $ctx (Join-Path $shots 'rivet-settings-1-open.png')
    if (-not (Read-Log | Where-Object { $_ -match 'settings: open: true' })) { return @('FAIL', $open.Path, 'the settings card did not open') }
    if (@(Get-MyTops $ctx | Where-Object { $tops -notcontains $_ }).Count -gt 0) { return @('FAIL', $open.Path, 'the settings card made a window of its own') }
    foreach ($command in 'settingskey down', 'settingskey right') {
      if (-not (Send-Flag $ctx 'rivet-tab.txt' $command)) { return @('NOT_DRIVEN', $open.Path, "'$command' was not taken") }
    }
    Start-Sleep 1
    $changed = Get-Capture $ctx (Join-Path $shots 'rivet-settings-2-changed.png')
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'sections')) { return @('NOT_DRIVEN', $changed.Path, 'sections was not taken') }
    if ((Get-Sections).CtrlT -ne 'new_tab_page') { return @('FAIL', $changed.Path, 'the keyboard did not change what Ctrl+T opens') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'settingskey esc')) { return @('NOT_DRIVEN', $changed.Path, 'esc was not taken') }
    Start-Sleep 1
    $closed = Get-Capture $ctx (Join-Path $shots 'rivet-settings-3-closed.png')
    if (-not (Read-Log | Where-Object { $_ -match 'settings: closed' })) { return @('FAIL', $closed.Path, 'Esc did not close the card') }
    @('PASS', $changed.Path, 'card opened without a window of its own, Down and Right set Ctrl+T to the new tab page, Esc closed it')
  } finally { Stop-Scene $ctx }
}

# Link rules: links sent with `rivet session open` follow the rules in the test profile's own
# rules file. A link no rule matches opens as a new tab of the current space, a link of a Mini
# Window rule floats, and the panel opens over the page and changes a rule's target. The addresses
# are on 127.0.0.1 port 9, which refuses them, so nothing leaves the machine; the tab keeps the
# address it was asked to open.
function Invoke-LinkRules($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'link-rules' $urlA
  try {
    $rulesFile = Join-Path $flagDir 'link-rules.json'
    [IO.File]::WriteAllText($rulesFile, '{"rules":[{"pattern":{"kind":"prefix","value":"http://127.0.0.1:9/mini"},"target":{"kind":"mini-window"}}]}')
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    if (-not $ctx.SessionId) { return @('NOT_DRIVEN', $null, 'the session reported no id') }
    $rivet = Join-Path $debug 'rivet.exe'
    & $rivet session open --id $ctx.SessionId --state $state 'http://127.0.0.1:9/plain' 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { return @('FAIL', $null, "rivet session open exited $LASTEXITCODE") }
    Start-Sleep 3
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $null, 'list was not taken') }
    $urls = @(Get-TabUrls)
    $plain = Get-Capture $ctx (Join-Path $shots 'link-rules-1-plain.png')
    if (-not ($urls | Where-Object { $_ -match '127\.0\.0\.1:9/plain' })) { return @('FAIL', $plain.Path, "a link no rule matches is not a tab of the window: $($urls -join ' ')") }
    & $rivet session open --id $ctx.SessionId --state $state 'http://127.0.0.1:9/mini/1' 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { return @('FAIL', $plain.Path, "rivet session open exited $LASTEXITCODE for the Mini Window link") }
    Start-Sleep 4
    $mini = Get-Capture $ctx (Join-Path $shots 'link-rules-2-mini.png')
    if (-not (Read-Log | Where-Object { $_ -match 'link rules: open http://127\.0\.0\.1:9/mini/1 to MiniWindow: true' })) { return @('FAIL', $mini.Path, 'the Mini Window rule did not float the link') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'linkrules')) { return @('NOT_DRIVEN', $mini.Path, 'the runtime did not take the panel command') }
    Start-Sleep 1
    $panel = Get-Capture $ctx (Join-Path $shots 'link-rules-3-panel.png')
    if (-not (Read-Log | Where-Object { $_ -match 'link rules: open$' })) { return @('FAIL', $panel.Path, 'the panel did not open') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'linkrulestarget 0')) { return @('NOT_DRIVEN', $panel.Path, 'the runtime did not take the target click') }
    $changed = Get-Capture $ctx (Join-Path $shots 'link-rules-4-changed.png')
    $saved = Get-Content $rulesFile -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
    if ($saved.rules[0].target.kind -ne 'current-space') { return @('FAIL', $changed.Path, "a click on the target did not move it to the current space: $($saved.rules[0].target.kind)") }
    @('PASS', $panel.Path, 'an unmatched link opened as a tab, the Mini Window rule floated its link, the panel opened and a click changed the saved target; check the panel capture against the study')
  } finally { Stop-Scene $ctx }
}

# A tab's link preview floats over the page being read, then opens as a tab or goes away.
function Invoke-LinkPreview($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'link-preview' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots 'link-preview-1.png')
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    if (-not (Open-SecondPage $ctx $urlB)) { return @('NOT_DRIVEN', $one.Path, 'the second page did not open as a tab of the app window alone') }
    $opens = Get-ViewOpens 'Floating'
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'preview 0')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the preview command') }
    Start-Sleep 3
    $shown = Get-Capture $ctx (Join-Path $shots 'link-preview-2-shown.png')
    $v = Test-FloatingPage $shown $script:ColourB $script:ColourA
    if ($v.Verdict -ne 'PASS') { return @($v.Verdict, $shown.Path, $v.Reason) }
    if (-not (Test-ViewOwns $opens 'Floating')) { return @('FAIL', $shown.Path, 'the preview page is not in the second page view') }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'previewesc')) { return @('NOT_DRIVEN', $shown.Path, 'the runtime did not take the dismiss command') }
    Start-Sleep 3
    $gone = Get-Capture $ctx (Join-Path $shots 'link-preview-3-dismissed.png')
    $back = Test-OnePage $gone $script:ColourB $script:ColourA
    if ($back.Verdict -ne 'PASS') { return @('FAIL', $gone.Path, "after Esc: $($back.Reason)") }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'preview 0')) { return @('NOT_DRIVEN', $gone.Path, 'the runtime did not take the preview command') }
    Start-Sleep 3
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'previewopen')) { return @('NOT_DRIVEN', $gone.Path, 'the runtime did not take the open as tab command') }
    Start-Sleep 3
    $tab = Get-Capture $ctx (Join-Path $shots 'link-preview-4-tab.png')
    $opened = Test-OnePage $tab $script:ColourA $script:ColourB
    if ($opened.Verdict -ne 'PASS') { return @('FAIL', $tab.Path, "after Open as tab: $($opened.Reason)") }
    @('PASS', $shown.Path, "$($v.Reason); Esc and Open as tab both leave one page")
  } finally { Stop-Scene $ctx }
}

# Posts each character of `$Text` to the window as typed keys, which the focused address field takes.
function Send-Chars($ctx, [string]$Text) {
  foreach ($ch in $Text.ToCharArray()) { [FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr][int]$ch, [IntPtr]1) | Out-Null }
}

function Send-Key($ctx, [int]$Key) {
  [FeatureWin]::PostMessage($ctx.Hwnd, 0x100, [IntPtr]$Key, [IntPtr]1) | Out-Null
  [FeatureWin]::PostMessage($ctx.Hwnd, 0x101, [IntPtr]$Key, [IntPtr]0xC0000001) | Out-Null
}

# Waits until the runtime log, after line `$Skip`, has a line matching `$Pattern`, and returns it.
function Wait-Log([int]$Skip, [string]$Pattern, [int]$Seconds = 30) {
  foreach ($i in 1..($Seconds * 2)) {
    $hit = @(Read-Log | Select-Object -Skip $Skip | Where-Object { $_ -match $Pattern }) | Select-Object -Last 1
    if ($hit) { return $hit }
    Start-Sleep -Milliseconds 500
  }
  $null
}

# The address bar lists the browser's own suggestions: page-a, typed and visited first, is suggested
# for a prefix of its address and completed inline after the typed text, and Enter opens it. The
# pages are served over loopback, since the browser's history suggestions leave file pages out.
function Invoke-AddressSuggest($Name, $shots) {
  $dir = Join-Path $pageDir 'suggest'
  New-Item -ItemType Directory -Force $dir | Out-Null
  foreach ($p in @(@('alpha', $script:ColourA), @('beta', $script:ColourB))) {
    [IO.File]::WriteAllText((Join-Path $dir "$($p[0]).html"), ("<!doctype html><meta charset=utf-8><title>$($p[0])</title><style>html,body{margin:0;height:100%;background:#" + ('{0:X6}' -f $p[1]) + "}</style>"))
  }
  $site = Start-LocalSite $dir
  $sa = "$($site.Base)/alpha.html"; $sb = "$($site.Base)/beta.html"
  # page-a is typed into the bar, as a person who visited it would have. A typed address is what
  # the browser completes inline from its history at once; a page only opened by a link or at start
  # is indexed for suggestions later and not completed inline.
  $ctx, $state = Start-Scene $Name 'address-suggest' $sb
  try {
    Open-Scene $ctx $state $sb $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots 'address-suggest-1-page-a.png')
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    $before = @(Read-Log).Count
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'bar')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the bar command') }
    Send-Chars $ctx $sa
    Start-Sleep -Milliseconds 800
    Send-Key $ctx 0x0D
    if (-not (Wait-Log $before '^address bar: open')) { return @('FAIL', $one.Path, 'the typed address of page-a did not open') }
    Start-Sleep 6
    # page-b is typed into the bar and opened in the same tab, so page-a is history, not the page.
    $before = @(Read-Log).Count
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'bar')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the bar command') }
    Send-Chars $ctx $sb
    Start-Sleep -Milliseconds 800
    Send-Key $ctx 0x0D
    if (-not (Wait-Log $before '^address bar: open')) { return @('FAIL', $one.Path, 'the typed address of page-b did not open') }
    Start-Sleep 4
    $b = Get-Capture $ctx (Join-Path $shots 'address-suggest-2-page-b.png')
    $onB = Test-OnePage $b $script:ColourB $script:ColourA
    if ($onB.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $b.Path, "page-b did not open from the bar: $($onB.Reason)") }
    # Type page-a's address up to the middle of its name.
    # The prefix ends inside a word, as a person types it: the history providers match whole
    # words of the address and its title, and no other page starts with it.
    $prefix = $sa.Substring(0, $sa.Length - 7)
    $rest = $sa.Substring($sa.Length - 7)
    $before = @(Read-Log).Count
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'bar')) { return @('NOT_DRIVEN', $b.Path, 'the runtime did not take the bar command') }
    Send-Chars $ctx $prefix
    $quoted = [regex]::Escape('"' + $prefix + '"')
    $line = Wait-Log $before "^address bar: suggestions for $quoted"
    # The history providers' slower pass answers after the first list; the bar shows it as it comes.
    Start-Sleep 2
    $late = @(Read-Log | Select-Object -Skip $before | Where-Object { $_ -match "^address bar: suggestions for $quoted later" }) | Select-Object -Last 1
    if ($late) { $line = $late }
    $list = Get-Capture $ctx (Join-Path $shots 'address-suggest-3-list.png')
    if (-not $line) { return @('FAIL', $list.Path, 'the bar asked the browser for no suggestions for the typed prefix') }
    $page = $sa
    if ($line -notmatch [regex]::Escape($page)) { return @('FAIL', $list.Path, "page-a is not suggested: $line") }
    $notes = @("page-a suggested$(if ($late) { ' once the slower providers answered' })")
    $inline = $line -match ('inline ' + [regex]::Escape('"' + $rest + '"') + '$')
    $rows = @([regex]::Matches((($line -replace '^[^\[]*\[', '') -replace ' inline .*$', ''), '"([^"]*)"') | ForEach-Object { $_.Groups[1].Value })
    $index = [array]::IndexOf($rows, $page)
    $before = @(Read-Log).Count
    if ($inline) {
      $notes += "completed inline with '$rest'"
    } else {
      # Not the default match, so it is picked from the list with the arrow keys, as a person would.
      $notes += "picked as row $index of the list"
      foreach ($k in 0..$index) { Send-Key $ctx 0x28; Start-Sleep -Milliseconds 150 }
    }
    Send-Key $ctx 0x0D
    $want = if ($inline) { 0 } else { $index }
    if (-not (Wait-Log $before "^address bar: open suggestion $want")) { return @('FAIL', $list.Path, "Enter did not open suggestion $want") }
    Start-Sleep 4
    $after = Get-Capture $ctx (Join-Path $shots 'address-suggest-4-opened.png')
    $v = Test-OnePage $after $script:ColourA $script:ColourB
    if ($v.Verdict -ne 'PASS') { return @('FAIL', $after.Path, "Enter did not show page-a: $($v.Reason)") }
    $notes += 'Enter opened page-a'
    @('PASS', $list.Path, ($notes -join '; '))
  } finally { Stop-Scene $ctx; Stop-LocalSite $site }
}

# --- FR-3 appbar width, hide and peek, and links from other apps ----------------------------
# The docked appbar starts 276 DIP wide. Its inner 6 DIP are the resizer, and a drag sets the
# width to the pointer's x, kept between 200 and 420 DIP.
$appbarWidth = 276

function Invoke-AppbarWidth($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'appbar-width' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    $f = $ctx.Dpi / 96.0
    $y = [int](300 * $f)
    $start = @(Read-Log).Count
    # The press lands on the resizer as non-client input, then the window holds the pointer and
    # takes client moves, as with a person's drag.
    Send-AppbarNc $ctx 0xA1 ([int](($appbarWidth - 3) * $f)) $y
    Start-Sleep -Milliseconds 300
    $seen = @()
    $shot = $null
    foreach ($step in @(@(150, 200, 'narrow'), @(600, 420, 'wide'), @(320, 320, 'middle'))) {
      $x, $want, $label = $step
      $skip = @(Read-Log).Count
      Send-ClientMouse $ctx 0x200 ([int]($x * $f)) $y 1
      $hit = Wait-Log $skip "appbar: width $want$" 10
      $shot = Get-Capture $ctx (Join-Path $shots "appbar-width-$label.png")
      if (-not $hit) {
        Send-ClientMouse $ctx 0x202 ([int]($x * $f)) $y
        $last = @(Read-Log | Select-Object -Skip $start | Where-Object { $_ -match 'appbar: width ' }) | Select-Object -Last 1
        if ($seen.Count -eq 0 -and -not $last) { return @('NOT_DRIVEN', $shot.Path, 'the drag on the resizer changed no width') }
        return @('FAIL', $shot.Path, "a drag to $x DIP did not give $want DIP; last: $last")
      }
      $seen += $want
    }
    Send-ClientMouse $ctx 0x202 ([int](320 * $f)) $y
    @('PASS', $shot.Path, "the drag gave $($seen -join ', ') DIP for 150, 600 and 320; check the captures for the page edge")
  } finally { Stop-Scene $ctx }
}

function Invoke-AppbarPeek($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'appbar-peek' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    $f = $ctx.Dpi / 96.0
    # The real pointer is never over the off-screen window, which would end a peek at once. This
    # flag keeps the peek until the posted pointer leaves.
    $hold = Join-Path $flagDir 'rivet-hold-peek.flag'
    [IO.File]::WriteAllText($hold, '')
    # The toggle is the 28 DIP button 8 DIP right of the page panel's leading edge, 16 DIP down.
    $skip = @(Read-Log).Count
    Send-AppbarClick $ctx ([int](($appbarWidth + 8 + 14) * $f)) ([int]((16 + 14) * $f))
    $hidden = Wait-Log $skip 'appbar: hidden true' 10
    $one = Get-Capture $ctx (Join-Path $shots 'appbar-peek-1-hidden.png')
    if (-not $hidden) { return @('NOT_DRIVEN', $one.Path, 'the toggle click did not hide the appbar') }
    $skip = @(Read-Log).Count
    Send-ClientMouse $ctx 0x200 ([int](4 * $f)) ([int](300 * $f))
    $peek = Wait-Log $skip 'appbar: peek true' 10
    Start-Sleep 1
    $two = Get-Capture $ctx (Join-Path $shots 'appbar-peek-2-peek.png')
    if (-not $peek) { return @('FAIL', $two.Path, 'the pointer at the left edge did not show the hidden appbar') }
    # A point inside the floating appbar keeps it.
    $skip = @(Read-Log).Count
    Send-ClientMouse $ctx 0x200 ([int](120 * $f)) ([int](300 * $f))
    if (Wait-Log $skip 'appbar: peek false' 2) { return @('FAIL', $two.Path, 'the peek left while the pointer was over the floating appbar') }
    Remove-Item $hold -ErrorAction SilentlyContinue
    $skip = @(Read-Log).Count
    Send-ClientMouse $ctx 0x200 ([int](($appbarWidth + 300) * $f)) ([int](300 * $f))
    $gone = Wait-Log $skip 'appbar: peek false' 10
    $three = Get-Capture $ctx (Join-Path $shots 'appbar-peek-3-left.png')
    if (-not $gone) { return @('FAIL', $three.Path, 'the peek stayed after the pointer left it') }
    @('PASS', $two.Path, 'the toggle hid the appbar, the left edge showed it, and it went when the pointer left; check the captures against the mockup')
  } finally { Stop-Scene $ctx }
}

# Windows runs `rivet-launcher.exe --open-link <address>` when Rivet is the default browser. The
# launcher reads the saved setup and the session list below its data folder, so
# this row gives it a folder of its own in the output and never the owner's.
function New-LinkRoot($Name, [string]$Mode, [string]$ExePath) {
  $root = Join-Path $OutDir "local-$Name-open-link-$Mode"
  Remove-HeldBrowsers $root
  Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
  $config = Join-Path $root 'Rivet\config'
  New-Item -ItemType Directory -Force $config | Out-Null
  $setup = @{ version = 1; browser = $ExePath; profile = 'Link' } | ConvertTo-Json -Compress
  [IO.File]::WriteAllText((Join-Path $config 'setup.json'), $setup)
  Add-HeldBrowsers $root
  $root
}

# Runs the launcher for one link with `$Root` in place of the local application data folder for
# Rivet's files, and returns its exit code, or $null when it did not finish.
function Invoke-Launcher([string]$Root, [string]$Url) {
  $old = $env:RIVET_DATA_ROOT
  $env:RIVET_DATA_ROOT = $Root
  try {
    # Started like a test browser, so a window the launcher itself shows first is not activated.
    $p = Start-TestBrowser -FilePath (Join-Path $debug 'rivet-launcher.exe') -ArgumentList '--open-link', $Url
    # The launcher waits at most 30 seconds for a new window to take the link.
    if (-not $p.WaitForExit(45000)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue; $p.Dispose(); return $null }
    $code = $p.ExitCode
    $p.Dispose()
    $code
  } finally { $env:RIVET_DATA_ROOT = $old }
}

function Invoke-OpenLink($Name, $shots, [string]$Mode) {
  $feature = "open-link-$Mode"
  $exePath, $exeName = $table[$Name]
  if (-not (Test-Path (Join-Path $debug 'rivet-launcher.exe'))) { return @('NOT_DRIVEN', $null, 'rivet-launcher.exe was not built') }
  $root = New-LinkRoot $Name $Mode $exePath
  $link = "http://127.0.0.1:9/handoff-$Mode"
  $ctx = $null
  $started = $null
  try {
    if ($Mode -eq 'running') {
      $ctx, $state = Start-Scene $Name $feature $urlA
      # The window registers in the session list the launcher reads.
      $state = Join-Path $root 'Rivet\state\sessions.json'
      New-Item -ItemType Directory -Force (Split-Path $state) | Out-Null
      $old = $env:RIVET_DATA_ROOT; $env:RIVET_DATA_ROOT = $root
      try { Open-Scene $ctx $state $urlA $null } finally { $env:RIVET_DATA_ROOT = $old }
      if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
      if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
      $code = Invoke-Launcher $root $link
      if ($null -eq $code) { return @('FAIL', $null, 'the launcher did not finish') }
      Start-Sleep 3
      if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $null, 'list was not taken') }
      $urls = @(Get-TabUrls)
      $shot = Get-Capture $ctx (Join-Path $shots "$feature.png")
      $fresh = @(Get-ProcessList $exeName | Where-Object { $_.CommandLine -like "*$root\Rivet\profiles*" })
      if ($fresh.Count) { return @('FAIL', $shot.Path, 'the launcher started another window instead of using the running one') }
      if ($code -ne 0) { return @('FAIL', $shot.Path, "the launcher exited $code") }
      if (-not ($urls | Where-Object { $_ -match [regex]::Escape($link) })) { return @('FAIL', $shot.Path, "the link is not a tab of the running window: $($urls -join ' ')") }
      return @('PASS', $shot.Path, "the running window took the link as a tab: $($urls.Count) tabs")
    }
    # No Rivet window: the launcher starts the saved session. Its profile folder is where the
    # setup names it, so the trace and debug switches can be put there before the launch.
    $key = switch ($exeName) { 'msedge.exe' { 'edge' } 'samsunginternet.exe' { 'samsung' } default { [IO.Path]::GetFileNameWithoutExtension($exeName).ToLowerInvariant() } }
    $started = Join-Path $root "Rivet\profiles\$key\Link"
    $script:flagDir = Join-Path $started 'rivet-debug'
    New-Item -ItemType Directory -Force $script:flagDir | Out-Null
    $script:logFile = Join-Path $script:flagDir 'rivet-vertical-tabs.log'
    [IO.File]::WriteAllText($logFile, '')
    if ($appWindowRvas.Contains($Name)) { Set-Content (Join-Path $flagDir 'rivet-spike.txt') $appWindowRvas[$Name] }
    $code = Invoke-Launcher $root $link
    $ctx = [pscustomobject]@{ Name = $Name; ExeName = $exeName; ExePath = $exePath; Profile = $started; Hwnd = [IntPtr]::Zero; Error = $null; Dpi = 96; Feature = $feature }
    $mine = @(Get-MyProcesses $exeName $started | Where-Object { $_.CommandLine -notlike '*--type=*' })
    if (-not $mine.Count) { return @('FAIL', $null, "the launcher started no browser on the saved profile (exit $code)") }
    $ctx.Hwnd = Get-BrowserWindow ($mine | ForEach-Object { [int]$_.ProcessId })
    if ($ctx.Hwnd -and $ctx.Hwnd -ne [IntPtr]::Zero) { $ctx.Dpi = Get-LayoutDpi $ctx }
    if ($null -eq $code) { return @('FAIL', $null, 'the launcher did not finish') }
    if ($code -ne 0) { return @('FAIL', $null, "the launcher exited $code") }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $null, 'list was not taken') }
    $urls = @(Get-TabUrls)
    $shot = if ($ctx.Hwnd -and $ctx.Hwnd -ne [IntPtr]::Zero) { Get-Capture $ctx (Join-Path $shots "$feature.png") } else { $null }
    $path = if ($shot) { $shot.Path } else { $null }
    if (-not ($urls | Where-Object { $_ -match [regex]::Escape($link) })) { return @('FAIL', $path, "the link is not a tab of the started window: $($urls -join ' ')") }
    @('PASS', $path, "the saved session started and its window took the link: $($urls -join ' ')")
  } finally {
    if ($ctx) { Stop-Scene $ctx }
    if ($started) { Stop-Mine $exeName $started }
    Remove-HeldBrowsers $root
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
  }
}

# A fake account form, served over loopback from files in the run's page folder, because a
# browser's password manager does not save or fill on file:// pages. The form's script fills a fake
# account and submits it, since posted keys do not reach the page on every browser. The page turns
# green when the browser itself fills its fields (`:autofill`), and stays red otherwise. Nothing
# real is typed and nothing is read from the profile's password store.
function New-LoginPages {
  $hexA = '#{0:X6}' -f $script:ColourA; $hexC = '#{0:X6}' -f $script:ColourC
  $style = "<style>html,body{margin:0;height:100%;background:$hexA}body:has(input:autofill),body:has(input:-webkit-autofill){background:$hexC}input{font-size:20px;margin:20px}</style>"
  $form = "<form method=get action=done.html autocomplete=on><input id=u name=username autocomplete=username autofocus><input id=p name=password type=password autocomplete=current-password><button>Sign in</button></form>"
  $js = "<script>if(location.hash==='#submit'){setTimeout(function(){u.value='rivet-test-user';p.value='not-a-real-password-1';document.forms[0].requestSubmit()},1500)}</script>"
  [IO.File]::WriteAllText((Join-Path $pageDir 'login.html'), "<!doctype html><meta charset=utf-8><title>login</title>$style$form$js")
  $hexB = '#{0:X6}' -f $script:ColourB
  # The page after signing in holds a line of text. Chromium takes a page with an empty body for a
  # redirect and does not report it, so the sign in never counted as done and no save was offered.
  [IO.File]::WriteAllText((Join-Path $pageDir 'done.html'), "<!doctype html><meta charset=utf-8><title>signed in</title><style>html,body{margin:0;height:100%;background:$hexB}p{margin:0;color:$hexB}</style><p>Signed in.</p>")
}

# Serves the page folder on 127.0.0.1 from a runspace of this process, on a port of its own.
function Start-LoginServer {
  New-LoginPages
  $probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0); $probe.Start()
  $port = $probe.LocalEndpoint.Port; $probe.Stop()
  $listener = [Net.HttpListener]::new()
  $listener.Prefixes.Add("http://127.0.0.1:$port/")
  $listener.Start()
  $ps = [PowerShell]::Create()
  [void]$ps.AddScript({
      param($listener, $dir)
      while ($listener.IsListening) {
        try { $c = $listener.GetContext() } catch { break }
        $name = [IO.Path]::GetFileName($c.Request.Url.AbsolutePath)
        $path = Join-Path $dir $name
        if ($name -and (Test-Path $path)) {
          $bytes = [IO.File]::ReadAllBytes($path); $c.Response.ContentType = 'text/html; charset=utf-8'
          $c.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        } else { $c.Response.StatusCode = 404 }
        $c.Response.Close()
      }
    }).AddArgument($listener).AddArgument($pageDir)
  [void]$ps.BeginInvoke()
  [pscustomobject]@{ Listener = $listener; Shell = $ps; Url = "http://127.0.0.1:$port/login.html" }
}

function Stop-LoginServer($server) {
  if (-not $server) { return }
  try { $server.Listener.Stop(); $server.Listener.Close() } catch {}
  try { $server.Shell.Stop(); $server.Shell.Dispose() } catch {}
}

# Reads only the two password manager switches from the test profile's Preferences.
function Get-ProviderPrefs([string]$Profile) {
  $path = Join-Path $Profile 'Default\Preferences'
  try { $p = Get-Content -Raw $path | ConvertFrom-Json } catch { return $null }
  [pscustomobject]@{ Save = $p.credentials_enable_service; AutoSignIn = $p.credentials_enable_autosignin }
}

# NFR-8: the chosen provider takes effect. `browser`: the browser's password manager offers to save
# the fake account after the form is sent, its Save is accepted, and a fresh load of the form is
# filled by the browser. `system`: the browser's password manager stays off, so it neither offers
# to save nor fills; the save through Windows passkeys needs Windows Hello, which only a person at
# the machine can answer, so that half is reported as not driven.
function Invoke-Credentials($Name, $shots, [string]$Provider) {
  $feature = "credentials-$Provider"
  $server = $null
  $ctx, $state = Start-Scene $Name $feature $urlA
  try {
    $server = Start-LoginServer
    Open-Scene $ctx $state $server.Url $null @('--credentials', $Provider)
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    $prefs = Get-ProviderPrefs $ctx.Profile
    $want = ($Provider -eq 'browser')
    if (-not $prefs -or $prefs.Save -ne $want -or $prefs.AutoSignIn -ne $want) {
      return @('FAIL', $null, "the profile does not start with the $Provider provider: save=$($prefs.Save) autosignin=$($prefs.AutoSignIn)")
    }
    $tops = @(Get-MyTops $ctx)
    # Only now is the form sent, so the save offer is a window that was not there before. Sent while
    # the scene still settled, the offer was already among the windows taken as the scene's own.
    # The browser offers to save a password only in an active window. On a test desktop of its own
    # the runtime may activate the window, which reaches no person's screen or keyboard.
    if ($env:RIVET_TEST_DESKTOP) {
      # The browser saves what a person typed, so the fake account is typed into the focused form,
      # as keys the focused window takes, and sent with Enter. A posted key down already becomes its
      # character in the browser's message loop, so Tab and Enter get no second character.
      if (-not (Send-Flag $ctx 'rivet-tab.txt' 'focuswindow')) { return @('NOT_DRIVEN', $null, 'the window could not be focused on the test desktop') }
      Start-Sleep 1
      Send-Chars $ctx 'rivet-test-user'
      Send-Key $ctx 0x09
      Start-Sleep -Milliseconds 300
      Send-Chars $ctx 'not-a-real-password-1'
      Start-Sleep -Milliseconds 300
      Send-Key $ctx 0x0D
    } else {
      $go = Invoke-Rivet $ctx $state @('nav', 'go', ($server.Url + '?send#submit'))
      if ($go.Code -ne 0) { return @('NOT_DRIVEN', $null, "the login page could not be opened to send the form: $($go.Text)") }
    }
    # The form sends itself 1.5 s after load. The browser's save offer is a window of its own; it is
    # moved off-screen without activation as soon as it appears, then its default button is pressed.
    $bubble = $null
    foreach ($i in 1..200) {
      Start-Sleep -Milliseconds 50
      $bubble = @(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $tops -notcontains $_ }) | Select-Object -First 1
      if ($bubble) { [FeatureWin]::SetWindowPos($bubble, [IntPtr]::Zero, 9000, 2200, 0, 0, 0x15) | Out-Null; break }
    }
    $sent = Get-Capture $ctx (Join-Path $shots "$feature-1-sent.png")
    if ($Provider -eq 'system') {
      if ($bubble) {
        [FeatureWin]::PostMessage($bubble, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        return @('FAIL', $sent.Path, 'the browser offered to save although the Windows provider is chosen')
      }
      if (-not (Open-SecondPage $ctx $server.Url)) { return @('NOT_DRIVEN', $sent.Path, 'the form could not be opened again in a tab of the window') }
      Start-Sleep 3
      $again = Get-Capture $ctx (Join-Path $shots "$feature-2-again.png")
      $v = Test-OnePage $again $script:ColourA $script:ColourC
      if ($v.Verdict -ne 'PASS') { return @('FAIL', $again.Path, "the browser filled the form although its manager is off: $($v.Reason)") }
      return @('NOT_DRIVEN', $again.Path, 'browser manager off as chosen (no save offer, no fill); the Windows passkey save needs Windows Hello and was not driven')
    }
    if (-not $bubble) { return @('FAIL', $sent.Path, 'the browser did not offer to save the fake account') }
    $capBubble = [pscustomobject]@{ Hwnd = $bubble }
    $offer = Get-Capture $capBubble (Join-Path $shots "$feature-2-offer.png")
    # An offer shown without activation has no focused view, so Enter reaches no button. On a test
    # desktop of its own the runtime focuses the offer first, which reaches no person's keyboard.
    if ($env:RIVET_TEST_DESKTOP -and -not (Send-Flag $ctx 'rivet-tab.txt' 'focuswindow popup')) { return @('NOT_DRIVEN', $offer.Path, 'the save offer could not be focused on the test desktop') }
    Start-Sleep -Milliseconds 500
    Send-Key $capBubble 0x0D
    Start-Sleep 2
    if (-not (Open-SecondPage $ctx $server.Url)) { return @('NOT_DRIVEN', $offer.Path, 'the form could not be opened again in a tab of the window') }
    Start-Sleep 3
    $filled = Get-Capture $ctx (Join-Path $shots "$feature-3-filled.png")
    $v = Test-OnePage $filled $script:ColourC $script:ColourA
    if ($v.Verdict -ne 'PASS') { return @('FAIL', $filled.Path, "the saved fake account was not filled: $($v.Reason)") }
    @('PASS', $filled.Path, 'the browser offered to save the fake account, Save was accepted, and a fresh load of the form was filled by the browser')
  } finally {
    Stop-Scene $ctx
    Stop-LoginServer $server
  }
}

# A page that is one text field over the whole page, blue until the field holds `$Expected`, then
# green. A click anywhere in it focuses the field, so the colour tells whether typing reached it.
function New-TypingPage([string]$Name, [string]$Expected) {
  $path = Join-Path $pageDir "$Name.html"
  $hexB = '#{0:X6}' -f $script:ColourB; $hexC = '#{0:X6}' -f $script:ColourC
  $want = ($Expected.ToCharArray() | ForEach-Object { '\u{0:x4}' -f [int]$_ }) -join ''
  [IO.File]::WriteAllText($path, "<!doctype html><meta charset=utf-8><title>$Name</title><style>html,body{margin:0;height:100%;background:$hexB}input{all:unset;display:block;width:100%;height:100%;color:transparent;caret-color:transparent}</style><input id=f autofocus><script>const f=document.getElementById('f');f.addEventListener('input',()=>{if(f.value==='$want'){document.body.style.background='$hexC';}});</script>")
  'file:///' + ($path -replace '\\', '/')
}

# The screen point at the middle of the page region of `$Rgb` in a capture of the window.
function Get-RegionCentre($ctx, $Capture, [int]$Rgb) {
  $region = Get-PageRegion $Capture $Rgb
  if ($region.Count -eq 0) { return $null }
  $r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
  $p = New-Object FeatureWin+P
  $p.X = $r.L + [int](($region.X0 + $region.X1) / 2); $p.Y = $r.T + [int](($region.Y0 + $region.Y1) / 2)
  [pscustomobject]@{ Point = $p; Region = $region }
}

# A press and release at a screen point, posted to the window under it as a real click would be.
function Send-PointClick($ctx, $Point) {
  $target = [FeatureWin]::Inside($ctx.Hwnd, $Point)
  if ($target -eq [IntPtr]::Zero) { return $false }
  $c = New-Object FeatureWin+P; $c.X = $Point.X; $c.Y = $Point.Y
  [FeatureWin]::ScreenToClient($target, [ref]$c) | Out-Null
  $l = [IntPtr](([int]$c.Y -shl 16) -bor ([int]$c.X -band 0xffff))
  [FeatureWin]::PostMessage($target, 0x201, [IntPtr]1, $l) | Out-Null
  Start-Sleep -Milliseconds 150
  [FeatureWin]::PostMessage($target, 0x202, [IntPtr]0, $l) | Out-Null
  $true
}

# Committed Korean IME input, as the IME delivers it: a composition starts, each syllable arrives
# as WM_IME_CHAR, and the composition ends. The candidate window and the composing preedit come
# from a real IME only, so this checks that committed IME text reaches the page.
function Send-ImeText($ctx, [string]$Text) {
  [FeatureWin]::PostMessage($ctx.Hwnd, 0x10D, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  foreach ($ch in $Text.ToCharArray()) { [FeatureWin]::PostMessage($ctx.Hwnd, 0x286, [IntPtr][int]$ch, [IntPtr]1) | Out-Null }
  [FeatureWin]::PostMessage($ctx.Hwnd, 0x10E, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
}

# A split of page-a and a second page of the same window, then a click into the second view's page.
# The click must leave both pages where they were: the second page is not pulled back into the
# window's page view. `$Typing` then types into it: none, keys or ime.
function Invoke-ViewInput($Name, $shots, [string]$Feature, [string]$Typing) {
  $second = switch ($Typing) { 'keys' { $urlType } 'ime' { $urlIme } default { $urlB } }
  $ctx, $state = Start-Scene $Name $Feature $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots "$Feature-1-page-a.png")
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    if (-not (Open-SecondPage $ctx $second)) { return @('NOT_DRIVEN', $one.Path, 'the second page did not open as a tab of the app window alone') }
    $opens = Get-ViewOpens 'Split'
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $split = Get-Capture $ctx (Join-Path $shots "$Feature-2-split.png")
    $on = Test-TwoPages $split $script:ColourA $script:ColourB
    if ($on.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $split.Path, "the split did not show both pages: $($on.Reason)") }
    if (-not (Test-ViewOwns $opens 'Split')) { return @('NOT_DRIVEN', $split.Path, 'the second page is not in the second page view') }
    $at = Get-RegionCentre $ctx $split $script:ColourB
    if (-not $at) { return @('NOT_DRIVEN', $split.Path, 'the second page was not found in the capture') }
    $before = $at.Region
    if (-not (Send-PointClick $ctx $at.Point)) { return @('NOT_DRIVEN', $split.Path, 'no window found at the second page') }
    Start-Sleep 4
    $clicked = Get-Capture $ctx (Join-Path $shots "$Feature-3-after-click.png")
    $still = Test-TwoPages $clicked $script:ColourA $script:ColourB
    if ($still.Verdict -ne 'PASS') { return @('FAIL', $clicked.Path, "after a click into the second view the pages changed: $($still.Reason)") }
    $after = Get-PageRegion $clicked $script:ColourB
    if ([Math]::Abs($after.X0 - $before.X0) -gt 4 -or [Math]::Abs($after.Y0 - $before.Y0) -gt 4) {
      return @('FAIL', $clicked.Path, "the clicked page moved from $($before.X0),$($before.Y0) to $($after.X0),$($after.Y0): it was pulled into the other view")
    }
    if ($Typing -eq 'none') { return @('PASS', $clicked.Path, "both pages stay in their views after a click into the second view: $($still.Reason)") }
    if ($Typing -eq 'keys') { Send-Chars $ctx 'rivet' } else { Send-ImeText $ctx ([string][char]0xD55C + [char]0xAE00) }
    Start-Sleep 3
    $typed = Get-Capture $ctx (Join-Path $shots "$Feature-4-typed.png")
    $v = Test-TwoPages $typed $script:ColourA $script:ColourC
    $what = if ($Typing -eq 'keys') { 'typed keys' } else { 'committed Korean IME text' }
    if ($v.Verdict -ne 'PASS') { return @('FAIL', $typed.Path, "the $what did not reach the second view's page: $($v.Reason)") }
    @('PASS', $typed.Path, "the $what reached the second view's page and both pages stayed")
  } finally { Stop-Scene $ctx }
}

# Tabs moved into the app window through the tab strip model never leave a normal window behind,
# and the browser keeps running with the app window as its only window, also once tabs close.
function Invoke-TabMoves($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'tab-moves' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $one = Get-Capture $ctx (Join-Path $shots 'tab-moves-1.png')
    if (-not $one) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $one.Path, 'Rivet did not attach to the window') }
    foreach ($url in @($urlB, $urlC, $urlD)) {
      # Open-SecondPage fails, and closes it at once, when any other window shows up.
      if (-not (Open-SecondPage $ctx $url)) { return @('FAIL', $one.Path, "opening $([IO.Path]::GetFileName($url)) made another window or did not land in the app window") }
    }
    foreach ($i in 1..3) {
      if (-not (Send-Flag $ctx 'rivet-tab.txt' 'close 1')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the close command') }
    }
    Start-Sleep 4
    $tops = @(Get-MyTops $ctx)
    $alive = @(Get-MyProcesses $ctx.ExeName $ctx.Profile | Where-Object { $_.CommandLine -notlike '*--type=*' }).Count -gt 0
    $last = Get-Capture $ctx (Join-Path $shots 'tab-moves-2-after.png')
    if (-not $alive) { return @('FAIL', $one.Path, 'the browser exited') }
    if ($tops.Count -ne 1 -or $tops[0] -ne $ctx.Hwnd) { return @('FAIL', $last.Path, "the browser shows $($tops.Count) windows, not the app window alone") }
    $v = Test-OnePage $last $script:ColourA $script:ColourB
    if ($v.Verdict -ne 'PASS') { return @('FAIL', $last.Path, "after the tabs closed: $($v.Reason)") }
    @('PASS', $last.Path, 'three tabs moved into the app window and closed again; no other window showed and the browser kept running')
  } finally { Stop-Scene $ctx }
}

# The Rivet look from the first frame the window shows: a watcher captures every new look of the
# window from launch, and none may show the browser's own title strip above the page.
function Invoke-FirstFrame($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'first-frame' $urlA
  $exe = [IO.Path]::GetFileNameWithoutExtension($ctx.ExeName)
  $watch = [powershell]::Create()
  try {
    # The watcher waits for the browser process, then captures for fifteen seconds.
    [void]$watch.AddScript({
      param($exe, $profile_)
      $pids = New-Object 'System.Collections.Generic.HashSet[uint32]'
      foreach ($i in 1..400) {
        foreach ($q in Get-CimInstance Win32_Process -Filter "Name = '$exe.exe'" -ErrorAction SilentlyContinue) { if ($q.CommandLine -like "*$profile_*") { [void]$pids.Add([uint32]$q.ProcessId) } }
        if ($pids.Count) { break }
        Start-Sleep -Milliseconds 25
      }
      if (-not $pids.Count) { return , @() }
      , [FirstFrames]::Watch($pids, 8000, 15000)
    }).AddArgument($exe).AddArgument($ctx.Profile)
    $running = $watch.BeginInvoke()
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    $frames = @($watch.EndInvoke($running) | ForEach-Object { $_ })
    $steady = Get-Capture $ctx (Join-Path $shots 'first-frame-settled.png')
    if (-not $steady) { return @('NOT_DRIVEN', $null, 'no capture') }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $steady.Path, 'Rivet did not attach to the window') }
    $caps = @($frames | ForEach-Object { [pscustomobject]@{ Pixels = $_[3]; Width = $_[1]; Height = $_[2]; Ms = $_[0] } })
    if (-not $caps.Count) { return @('NOT_DRIVEN', $steady.Path, 'the watcher caught no frame of the window') }
    # The band right of the appbar, as high as the top row and a vendor title strip below it.
    $f = $ctx.Dpi / 96.0
    $v = Test-FirstFrames $caps $steady ([int](268 * $f)) ([int](668 * $f)) ([int](60 * $f))
    if ($v.Verdict -eq 'FAIL' -and $v.Reason -match '^frame (\d+)') {
      $bad = $caps[[int]$Matches[1] - 1]
      $bmp = New-Object Drawing.Bitmap $bad.Width, $bad.Height, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
      $data = $bmp.LockBits((New-Object Drawing.Rectangle 0, 0, $bad.Width, $bad.Height), [Drawing.Imaging.ImageLockMode]::WriteOnly, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
      [Runtime.InteropServices.Marshal]::Copy([int[]]$bad.Pixels, 0, $data.Scan0, $bad.Pixels.Length)
      $bmp.UnlockBits($data)
      $path = Join-Path $shots "first-frame-bad-$($bad.Ms)ms.png"
      $bmp.Save($path); $bmp.Dispose()
      return @('FAIL', $path, "$($v.Reason), $($bad.Ms) ms after the browser started")
    }
    @($v.Verdict, $steady.Path, $v.Reason)
  } finally { $watch.Dispose(); Stop-Scene $ctx }
}

$urlType = New-TypingPage 'page-type' 'rivet'
$urlIme = New-TypingPage 'page-ime' ([string][char]0xD55C + [char]0xAE00)

Add-Type -TypeDefinition @'
using System;using System.Collections.Generic;using System.Diagnostics;using System.Runtime.InteropServices;using System.Text;
public static class FirstFrames{
 delegate bool EW(IntPtr h,IntPtr l);
 struct R{public int L,T,Ri,B;}
 [StructLayout(LayoutKind.Sequential)] struct BIH{public int Size,Width,Height;public short Planes,Bits;public int Compression,SizeImage,X,Y,Used,Important;}
 [DllImport("user32.dll")] static extern bool EnumWindows(EW f,IntPtr l);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("gdi32.dll")] static extern IntPtr CreateCompatibleDC(IntPtr dc);
 [DllImport("gdi32.dll")] static extern IntPtr CreateDIBSection(IntPtr dc,ref BIH info,uint usage,out IntPtr bits,IntPtr section,uint offset);
 [DllImport("gdi32.dll")] static extern IntPtr SelectObject(IntPtr dc,IntPtr o);
 [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);
 [DllImport("gdi32.dll")] static extern bool DeleteDC(IntPtr dc);
 [DllImport("kernel32.dll")] static extern void Sleep(uint ms);
 // One PrintWindow capture as top-down ARGB pixels, or null.
 static int[] Capture(IntPtr h,int w,int ht){
  var dc=CreateCompatibleDC(IntPtr.Zero); if(dc==IntPtr.Zero) return null;
  var info=new BIH{Size=40,Width=w,Height=-ht,Planes=1,Bits=32};
  IntPtr bits; var dib=CreateDIBSection(dc,ref info,0,out bits,IntPtr.Zero,0);
  if(dib==IntPtr.Zero){DeleteDC(dc);return null;}
  var old=SelectObject(dc,dib);
  int[] px=null;
  if(PrintWindow(h,dc,2)){px=new int[w*ht];Marshal.Copy(bits,px,0,px.Length);for(int i=0;i<px.Length;i++)px[i]|=unchecked((int)0xFF000000);}
  SelectObject(dc,old);DeleteObject(dib);DeleteDC(dc);
  return px;
 }
 // Captures each new look of the visible browser windows of the given processes that sit at or
 // beyond `minX`, the test windows' off-screen place, until `ms` pass.
 public static List<object[]> Watch(HashSet<uint> pids,int minX,int ms){
  SetThreadDpiAwarenessContext(new IntPtr(-4));
  var o=new List<object[]>(); var sw=Stopwatch.StartNew(); long last=0;
  while(sw.ElapsedMilliseconds<ms){
   var tops=new List<IntPtr>();
   EnumWindows((h,l)=>{uint q;GetWindowThreadProcessId(h,out q);if(pids.Contains(q)&&IsWindowVisible(h)){var b=new StringBuilder(64);GetClassName(h,b,64);if(b.ToString()=="Chrome_WidgetWin_1")tops.Add(h);}return true;},IntPtr.Zero);
   foreach(var h in tops){
    R r; GetWindowRect(h,out r);
    int w=r.Ri-r.L, ht=r.B-r.T;
    if(r.L<minX||w<=0||ht<=0) continue;
    var px=Capture(h,w,ht); if(px==null) continue;
    long hash=w*31+ht; for(int i=0;i<px.Length;i+=97) hash=hash*1000003+px[i];
    if(hash!=last){last=hash;o.Add(new object[]{sw.ElapsedMilliseconds,w,ht,px});}
   }
   Sleep(8);
  }
  return o;
 }
}
'@

# --- Window surface rows: FR-2 frame, FR-9 panels, FR-10 Sidebar, FR-11 developer row, FR-12 bar,
# NFR-5 focus. Kept as one block so other rows merge around it. --------------------------------
. (Join-Path $PSScriptRoot 'windows-features-surface-lib.ps1')
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class SurfaceWin{
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll")] public static extern bool IsChild(IntPtr parent,IntPtr child);
 [DllImport("user32.dll",EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetLong(IntPtr h,int i);
 [DllImport("user32.dll",EntryPoint="SetWindowLongPtrW")] public static extern IntPtr SetLong(IntPtr h,int i,IntPtr v);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr r);
 [StructLayout(LayoutKind.Sequential)] public struct RC{public int L,T,R,B;}
 [StructLayout(LayoutKind.Sequential)] public struct GTI{public int cbSize;public int flags;public IntPtr hwndActive,hwndFocus,hwndCapture,hwndMenuOwner,hwndMoveSize,hwndCaret;public RC rcCaret;}
 [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread,ref GTI info);
 // The focus and active window of the thread that owns `h`: active, focus.
 public static IntPtr[] Focus(IntPtr h){uint p;var t=GetWindowThreadProcessId(h,out p);var g=new GTI();g.cbSize=Marshal.SizeOf(typeof(GTI));if(!GetGUIThreadInfo(t,ref g))return new IntPtr[]{IntPtr.Zero,IntPtr.Zero};return new IntPtr[]{g.hwndActive,g.hwndFocus};}
 [StructLayout(LayoutKind.Sequential)] public struct PK{public Guid fmtid;public uint pid;}
 [StructLayout(LayoutKind.Sequential)] public struct PV{public ushort vt,r1,r2,r3;public IntPtr p,p2;}
 [ComImport,Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 public interface IPS{int GetCount(out uint c);int GetAt(uint i,out PK k);int GetValue(ref PK k,out PV v);int SetValue(ref PK k,ref PV v);int Commit();}
 [DllImport("shell32.dll")] public static extern int SHGetPropertyStoreForWindow(IntPtr h,ref Guid iid,[MarshalAs(UnmanagedType.Interface)] out IPS store);
 [DllImport("ole32.dll")] public static extern int PropVariantClear(ref PV v);
 // A string property of the window's shell property store: 2 relaunch command, 3 relaunch icon,
 // 4 relaunch name and 5 the AppUserModelID, all of the AppUserModel property set.
 public static string Property(IntPtr h,uint pid){
  var iid=new Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99");IPS s;
  if(SHGetPropertyStoreForWindow(h,ref iid,out s)!=0||s==null)return null;
  var k=new PK();k.fmtid=new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3");k.pid=pid;PV v;
  string o=null;if(s.GetValue(ref k,out v)==0){if(v.vt==31&&v.p!=IntPtr.Zero)o=Marshal.PtrToStringUni(v.p);PropVariantClear(ref v);}
  Marshal.ReleaseComObject(s);return o;
 }
}
'@

# A loopback web server for pages a file address cannot stand in for: the browser saves no password
# for a file and `127.0.0.1` pages start in developer mode. It answers on a free port of 127.0.0.1
# only and stops by itself after a few minutes.
function Start-LocalSite([string]$Dir) {
  $probe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0); $probe.Start()
  $port = ([Net.IPEndPoint]$probe.LocalEndpoint).Port; $probe.Stop()
  $job = Start-ThreadJob -ArgumentList $Dir, $port -ScriptBlock {
    param($dir, $port)
    $l = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $port); $l.Start()
    $until = (Get-Date).AddMinutes(5)
    while ((Get-Date) -lt $until) {
      if (-not $l.Pending()) { Start-Sleep -Milliseconds 50; continue }
      $c = $l.AcceptTcpClient()
      try {
        $s = $c.GetStream(); $s.ReadTimeout = 3000
        $buf = New-Object byte[] 16384; $n = $s.Read($buf, 0, $buf.Length)
        $path = ([Text.Encoding]::ASCII.GetString($buf, 0, $n) -split ' ')[1]
        $name = [IO.Path]::GetFileName((($path -split '\?')[0])); if (-not $name) { $name = 'index.html' }
        $file = Join-Path $dir $name
        $found = Test-Path -LiteralPath $file -PathType Leaf
        $body = if ($found) { [IO.File]::ReadAllBytes($file) } else { [Text.Encoding]::UTF8.GetBytes('missing') }
        $code = if ($found) { '200 OK' } else { '404 Not Found' }
        $head = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 $code`r`nContent-Type: text/html; charset=utf-8`r`nContent-Length: $($body.Length)`r`nCache-Control: no-store`r`nConnection: close`r`n`r`n")
        $s.Write($head, 0, $head.Length); $s.Write($body, 0, $body.Length)
      } catch {} finally { $c.Close() }
    }
    $l.Stop()
  }
  [pscustomobject]@{ Job = $job; Base = "http://127.0.0.1:$port" }
}
function Stop-LocalSite($Site) { if ($Site) { Stop-Job $Site.Job -ErrorAction SilentlyContinue; Remove-Job $Site.Job -Force -ErrorAction SilentlyContinue } }

$siteDir = Join-Path $pageDir 'site'
New-Item -ItemType Directory -Force $siteDir | Out-Null
$hexA = '#{0:X6}' -f $script:ColourA; $hexB = '#{0:X6}' -f $script:ColourB; $hexC = '#{0:X6}' -f $script:ColourC
# The developer pages: one title, and addresses of one length in narrow and in wide letters.
$devNarrow = 'i' * 24; $devWide = 'w' * 24
foreach ($n in $devNarrow, $devWide) {
  [IO.File]::WriteAllText((Join-Path $siteDir $n), "<!doctype html><meta charset=utf-8><title>dev</title><style>html,body{margin:0;height:100%;background:$hexA}body{border-top:12px solid $hexC;box-sizing:border-box}</style>")
}
# A fake login form, focused on load. Typing that reaches it turns the page green, so a missing
# bubble is told apart from typing that never arrived.
[IO.File]::WriteAllText((Join-Path $siteDir 'login.html'), "<!doctype html><meta charset=utf-8><title>fake login</title><style>html,body{margin:0;height:100%;background:$hexA}input{display:block;width:400px;height:40px;margin:20px}</style><form action=done.html method=post><input id=u name=username autocomplete=username autofocus><input id=p name=password type=password autocomplete=current-password><button>Sign in</button></form><script>document.addEventListener('input',()=>{if(u.value&&p.value)document.documentElement.style.background='$hexC'})</script>")
[IO.File]::WriteAllText((Join-Path $siteDir 'done.html'), "<!doctype html><meta charset=utf-8><title>signed in</title><style>html,body{margin:0;height:100%;background:$hexB}p{margin:0;color:$hexB}</style><p>Signed in.</p>")
# A page that asks for the location as it loads, which makes the browser's permission bubble.
[IO.File]::WriteAllText((Join-Path $siteDir 'geo.html'), "<!doctype html><meta charset=utf-8><title>location</title><style>html,body{margin:0;height:100%;background:$hexA}</style><script>navigator.geolocation.getCurrentPosition(()=>{},()=>{})</script>")
# A page with a dark marker at half its width over its top 40 per cent, which moves when the page
# is laid out narrower and stays when something floats over it.
$markPath = Join-Path $pageDir 'page-mark.html'
[IO.File]::WriteAllText($markPath, "<!doctype html><meta charset=utf-8><title>page-mark</title><style>html,body{margin:0;height:100%;background:$hexA}i{position:fixed;left:50%;top:0;width:4px;height:40vh;background:#101010}</style><i></i>")
$urlMark = 'file:///' + ($markPath -replace '\\', '/')

# Opens a scene on `$Url` and returns ctx, state, a first capture and an early verdict when the
# scene could not be set up.
function Open-SurfaceScene($Name, [string]$Feature, [string]$Url, $shots) {
  $ctx, $state = Start-Scene $Name $Feature $Url
  Open-Scene $ctx $state $Url $null
  if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return $ctx, $null, @('NOT_DRIVEN', $null, $ctx.Error) }
  $one = Get-Capture $ctx (Join-Path $shots "$Feature-1-start.png")
  if (-not $one) { return $ctx, $null, @('NOT_DRIVEN', $null, 'no capture') }
  if (-not (Test-Attached $ctx)) { return $ctx, $one, @('FAIL', $one.Path, 'Rivet did not attach to the window') }
  $ctx, $one, $null
}

# Capture pixels of the client origin: the capture starts at the window's corner.
function Get-ClientOffset($ctx) {
  $r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
  $o = New-Object FeatureWin+P; [FeatureWin]::ClientToScreen($ctx.Hwnd, [ref]$o) | Out-Null
  [pscustomobject]@{ X = $o.X - $r.L; Y = $o.Y - $r.T; ScreenX = $o.X; ScreenY = $o.Y }
}

# The address and the security chip in client DIPs, and the layout DPI, from the runtime's trace.
function Get-BarAnchors($ctx) {
  $before = @(Read-Log).Count
  if (-not (Send-Flag $ctx 'rivet-tab.txt' 'baranchors')) { return $null }
  $line = Wait-Log $before '^address bar anchors Some' 5
  if (-not $line) { return $null }
  $n = @([regex]::Matches($line, '-?\d+(\.\d+)?') | ForEach-Object { [double]$_.Value })
  if ($n.Count -lt 9) { return $null }
  [pscustomobject]@{ Address = $n[0..3]; Chip = $n[4..7]; Dpi = $n[8] }
}

# Windows of the test profile that appeared since `$Before`. One that lies outside the off-screen
# test window, where a misplaced bubble could reach the owner's monitor, is moved off-screen at once
# after its place is noted.
function Wait-NewTops($ctx, $Before, [int]$Seconds = 10) {
  $main = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$main) | Out-Null
  foreach ($i in 1..($Seconds * 20)) {
    $extra = @(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $Before -notcontains $_ })
    if ($extra.Count -gt 0) {
      $found = foreach ($t in $extra) {
        $r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($t, [ref]$r) | Out-Null
        $inside = $r.L -ge $main.L -and $r.Ri -le $main.Ri -and $r.T -ge $main.T -and $r.B -le $main.B
        if (-not $inside) { [FeatureWin]::SetWindowPos($t, [IntPtr]::Zero, -3000, -3000, 0, 0, 0x15) | Out-Null }
        [pscustomobject]@{ Hwnd = $t; L = $r.L; T = $r.T; R = $r.Ri; B = $r.B; Inside = $inside }
      }
      return @($found)
    }
    Start-Sleep -Milliseconds 50
  }
  @()
}

# A browser bubble belongs inside the Rivet window near the address row: its top no more than 48
# DIPs below the address and its left between the row's start and the address's end.
if (-not ('BubbleScreen' -as [type])) {
  Add-Type -TypeDefinition @'
using System;using System.Collections.Generic;using System.Runtime.InteropServices;
public sealed class BubbleMonitor { public int[] Rect; public double Scale; public bool Primary; }
public static class BubbleScreen{
 public delegate bool EM(IntPtr m,IntPtr dc,IntPtr r,IntPtr l);
 [DllImport("user32.dll")] static extern bool EnumDisplayMonitors(IntPtr dc,IntPtr clip,EM f,IntPtr l);
 [DllImport("shcore.dll")] static extern int GetDpiForMonitor(IntPtr m,int type,out uint x,out uint y);
 [StructLayout(LayoutKind.Sequential)] struct MI{public int cb;public int L,T,R,B,WL,WT,WR,WB;public uint flags;}
 [DllImport("user32.dll")] static extern bool GetMonitorInfoW(IntPtr m,ref MI info);
 // Every monitor in physical pixels, with its scale; the caller is per-monitor DPI aware.
 public static List<BubbleMonitor> Monitors(){
  var o=new List<BubbleMonitor>();
  EnumDisplayMonitors(IntPtr.Zero,IntPtr.Zero,(m,dc,r,l)=>{uint x,y;if(GetDpiForMonitor(m,0,out x,out y)!=0)x=96;var i=new MI();i.cb=Marshal.SizeOf(typeof(MI));GetMonitorInfoW(m,ref i);
   o.Add(new BubbleMonitor{Rect=new[]{i.L,i.T,i.R,i.B},Scale=Math.Max(96u,x)/96.0,Primary=(i.flags&1)!=0});return true;},IntPtr.Zero);
  return o;
 }
}
'@
}

# Where Chromium puts a screen pixel in screen DIPs, as display::win::ScreenWin lays displays out:
# the primary monitor at zero, a monitor touching it at the primary's DIP edge with its offset at the
# primary's scale. `Near` is the monitor of the window's middle. A second implementation of the
# runtime's own mapping, so the check does not trust the code it checks.
function ConvertTo-ScreenDips([double]$X, [double]$Y, $Near, $Primary) {
  $l, $t, $r, $b = $Near.Rect; $pl, $pt, $pr, $pb = $Primary.Rect
  $s = $Near.Scale; $ps = $Primary.Scale
  if ($l -eq $pl -and $t -eq $pt -and $r -eq $pr -and $b -eq $pb) { $ox = 0; $oy = 0 }
  elseif ($l -eq $pr) { $ox = ($pr - $pl) / $ps; $oy = ($t - $pt) / $ps }
  elseif ($r -eq $pl) { $ox = - ($r - $l) / $s; $oy = ($t - $pt) / $ps }
  elseif ($t -eq $pb) { $ox = ($l - $pl) / $ps; $oy = ($pb - $pt) / $ps }
  elseif ($b -eq $pt) { $ox = ($l - $pl) / $ps; $oy = - ($b - $t) / $s }
  else { $ox = $l / $s; $oy = $t / $s }
  @(($ox + ($X - $l) / $s), ($oy + ($Y - $t) / $s))
}

# The last anchor the runtime handed Chromium, from its trace, as x, y, width and height in screen DIPs.
function Get-ChosenAnchor {
  $line = @(Read-Log | Where-Object { $_ -match '^bubble anchor: chose \[' }) | Select-Object -Last 1
  if (-not $line) { return $null }
  @([regex]::Matches($line, '-?\d+') | ForEach-Object { [int]$_.Value })
}

function Test-BubbleNearBar($ctx, $Bubble, $Anchors) {
  $o = Get-ClientOffset $ctx; $s = $Anchors.Dpi / 96.0
  $w = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$w) | Out-Null
  $mons = @([BubbleScreen]::Monitors())
  $onMonitor = @($mons | Where-Object { $_.Rect[0] -lt $w.Ri -and $_.Rect[2] -gt $w.L -and $_.Rect[1] -lt $w.B -and $_.Rect[3] -gt $w.T }).Count -gt 0
  if (-not $onMonitor) {
    # Off every monitor Chromium fits a bubble to a display the window is not on, so where the
    # bubble lands sideways says nothing. The anchor Rivet hands Chromium is checked exactly
    # instead, and the bubble's top by the row.
    $chosen = Get-ChosenAnchor
    if (-not $chosen) { return New-Verdict 'FAIL' 'the runtime traced no anchor for the bubble' }
    $mx = ($w.L + $w.Ri) / 2; $my = ($w.T + $w.B) / 2
    $near = $mons | Sort-Object { [Math]::Max(0, [Math]::Max($_.Rect[0] - $mx, $mx - $_.Rect[2])) + [Math]::Max(0, [Math]::Max($_.Rect[1] - $my, $my - $_.Rect[3])) } | Select-Object -First 1
    $primary = $mons | Where-Object { $_.Primary } | Select-Object -First 1
    $target = if ($Bubble.PageInfo) { $Anchors.Chip } else { $Anchors.Address }
    $want = ConvertTo-ScreenDips ($o.ScreenX + $target[0] * $s) ($o.ScreenY + $target[1] * $s) ([pscustomobject]@{ Rect = $near.Rect; Scale = $s }) $primary
    if ([Math]::Abs($chosen[0] - $want[0]) -gt 1 -or [Math]::Abs($chosen[1] - $want[1]) -gt 1) {
      return New-Verdict 'FAIL' ('the runtime handed Chromium the anchor {0},{1}, not the {2} at {3:N0},{4:N0} DIPs' -f $chosen[0], $chosen[1], $(if ($Bubble.PageInfo) { 'chip' } else { 'address' }), $want[0], $want[1])
    }
    $top = $o.ScreenY + $Anchors.Address[1] * $s; $bottom = $o.ScreenY + ($Anchors.Address[1] + $Anchors.Address[3]) * $s
    if ($Bubble.T -lt $top - 8 * $s -or $Bubble.T -gt $bottom + 48 * $s) { return New-Verdict 'FAIL' ('the bubble top {0} is not by the address row ({1:N0} to {2:N0})' -f $Bubble.T, $top, $bottom) }
    return New-Verdict 'PASS' ('anchor {0},{1} DIPs is the {2} exactly; bubble top {3} by the row; its left edge is not checked: the window is off every monitor, where Chromium fits a bubble to another display' -f $chosen[0], $chosen[1], $(if ($Bubble.PageInfo) { 'chip' } else { 'address' }), ($Bubble.T - $o.ScreenY))
  }
  if (-not $Bubble.Inside) { return New-Verdict 'FAIL' ('the bubble opened outside the Rivet window at {0},{1}' -f $Bubble.L, $Bubble.T) }
  $addrLeft = $o.ScreenX + $Anchors.Chip[0] * $s
  $addrRight = $o.ScreenX + ($Anchors.Address[0] + $Anchors.Address[2]) * $s
  $addrTop = $o.ScreenY + $Anchors.Address[1] * $s
  $addrBottom = $o.ScreenY + ($Anchors.Address[1] + $Anchors.Address[3]) * $s
  if ($Bubble.T -lt $addrTop - 8 * $s -or $Bubble.T -gt $addrBottom + 48 * $s) { return New-Verdict 'FAIL' ('the bubble top {0} is not by the address row ({1:N0} to {2:N0})' -f $Bubble.T, $addrTop, $addrBottom) }
  if ($Bubble.L -lt $addrLeft - 80 * $s -or $Bubble.L -gt $addrRight) { return New-Verdict 'FAIL' ('the bubble left {0} is not under the address ({1:N0} to {2:N0})' -f $Bubble.L, $addrLeft, $addrRight) }
  New-Verdict 'PASS' ('bubble at {0},{1} under the address row' -f ($Bubble.L - $o.ScreenX), ($Bubble.T - $o.ScreenY))
}

# FR-2: fullscreen takes the sidebar away and leaving brings it back. Real fullscreen covers a
# monitor, which the off-screen rule forbids, so this sets the frame style Chromium sets for
# fullscreen (no sizing frame) and nudges the size so the browser lays out again.
function Invoke-FullscreenSidebar($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'fullscreen-sidebar' $urlA $shots
  try {
    if ($early) { return $early }
    $a = Get-PageRegion $one $script:ColourA
    if ($a.Width -lt 200) { return @('NOT_DRIVEN', $one.Path, 'the test page was not shown') }
    $s = $ctx.Dpi / 96.0
    $style = [SurfaceWin]::GetLong($ctx.Hwnd, -16)
    $bare = [IntPtr]([int64]$style -band (-bnot [int64]0x40000))
    [SurfaceWin]::SetLong($ctx.Hwnd, -16, $bare) | Out-Null
    [FeatureWin]::SetWindowPos($ctx.Hwnd, [IntPtr]::Zero, 0, 0, 1801, 1067, 0x36) | Out-Null
    Start-Sleep 4
    $full = Get-Capture $ctx (Join-Path $shots 'fullscreen-sidebar-2-full.png')
    [SurfaceWin]::SetLong($ctx.Hwnd, -16, $style) | Out-Null
    [FeatureWin]::SetWindowPos($ctx.Hwnd, [IntPtr]::Zero, 0, 0, 1800, 1067, 0x36) | Out-Null
    Start-Sleep 4
    $back = Get-Capture $ctx (Join-Path $shots 'fullscreen-sidebar-3-back.png')
    $f = Get-PageRegion $full $script:ColourA; $b = Get-PageRegion $back $script:ColourA
    if ($f.X0 -gt $a.X0 - 100 * $s) { return @('FAIL', $full.Path, ('the sidebar stayed without the sizing frame: the page starts at {0}, it started at {1}' -f $f.X0, $a.X0)) }
    if ([Math]::Abs($b.X0 - $a.X0) -gt 2) { return @('FAIL', $back.Path, ('the sidebar did not come back: the page starts at {0}, it started at {1}' -f $b.X0, $a.X0)) }
    @('PASS', $full.Path, ('page starts at {0} with the sidebar, {1} without the sizing frame, {2} after; the frame style was set by the test, not by real fullscreen' -f $a.X0, $f.X0, $b.X0))
  } finally { Stop-Scene $ctx }
}

# FR-2: the window carries Rivet's AppUserModelID, so the taskbar groups it apart from the browser,
# and its icon is Rivet's, compared with the installer's icon and against the browser's own.
function Invoke-TaskbarIdentity($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'taskbar-identity' $urlA $shots
  try {
    if ($early) { return $early }
    $id = [SurfaceWin]::Property($ctx.Hwnd, 5)
    if ($id -ne 'LogitAI.Rivet.Browser') { return @('FAIL', $one.Path, "the window's AppUserModelID is '$id'") }
    $r = [IntPtr]::Zero
    [SurfaceWin]::SendMessageTimeout($ctx.Hwnd, 0x7F, [IntPtr]1, [IntPtr]::Zero, 2, 2000, [ref]$r) | Out-Null
    if ($r -eq [IntPtr]::Zero) { return @('FAIL', $one.Path, 'the window has no big icon') }
    $logo = Join-Path $root 'installer\windows\setup-ui\assets\logo.ico'
    $toBitmap = { param($icon) $b = New-Object Drawing.Bitmap 32, 32; $g = [Drawing.Graphics]::FromImage($b); $g.DrawIcon($icon, (New-Object Drawing.Rectangle 0, 0, 32, 32)); $g.Dispose(); $b }
    $diff = { param($x, $y) $t = 0; for ($i = 0; $i -lt 32; $i++) { for ($j = 0; $j -lt 32; $j++) { $p = $x.GetPixel($i, $j); $q = $y.GetPixel($i, $j); $t += [Math]::Abs($p.R - $q.R) + [Math]::Abs($p.G - $q.G) + [Math]::Abs($p.B - $q.B) + [Math]::Abs($p.A - $q.A) } }; $t / 1024 }
    $window = & $toBitmap ([Drawing.Icon]::FromHandle($r))
    $rivet = & $toBitmap (New-Object Drawing.Icon $logo, 32, 32)
    $browser = & $toBitmap ([Drawing.Icon]::ExtractAssociatedIcon($ctx.ExePath))
    $window.Save((Join-Path $shots 'taskbar-identity-icon.png'))
    $toRivet = & $diff $window $rivet; $toBrowser = & $diff $window $browser
    if ($toRivet -ge $toBrowser -or $toRivet -gt 40) { return @('FAIL', (Join-Path $shots 'taskbar-identity-icon.png'), ('the icon is not Rivet''s: distance {0:N0} to Rivet, {1:N0} to the browser' -f $toRivet, $toBrowser)) }
    @('PASS', (Join-Path $shots 'taskbar-identity-icon.png'), ('AppUserModelID {0}; icon distance {1:N0} to Rivet, {2:N0} to the browser' -f $id, $toRivet, $toBrowser))
  } finally { Stop-Scene $ctx }
}

# FR-2: a pinned window relaunches Rivet with Rivet's icon. Those properties are set only for an
# installed Rivet, whose folder holds the launcher and the icon.
function Invoke-TaskbarRelaunch($Name, $shots) {
  if (-not ((Test-Path (Join-Path $debug 'rivet-launcher.exe')) -and (Test-Path (Join-Path $debug 'rivet.ico')))) {
    return @('NOT_DRIVEN', $null, 'the relaunch command and icon are set only when the runtime folder holds rivet-launcher.exe and rivet.ico, as an installed build does; target\debug does not')
  }
  $ctx, $one, $early = Open-SurfaceScene $Name 'taskbar-relaunch' $urlA $shots
  try {
    if ($early) { return $early }
    $command = [SurfaceWin]::Property($ctx.Hwnd, 2); $icon = [SurfaceWin]::Property($ctx.Hwnd, 3)
    if ($command -notmatch 'rivet-launcher\.exe') { return @('FAIL', $one.Path, "relaunch command '$command'") }
    if ($icon -notmatch 'rivet\.ico') { return @('FAIL', $one.Path, "relaunch icon '$icon'") }
    @('PASS', $one.Path, "relaunch $command with $icon")
  } finally { Stop-Scene $ctx }
}

# FR-9: split panels have rounded corners and the page being worked in is marked by its shadow.
function Invoke-SplitPanelLook($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'split-panel-look' $urlA $shots
  try {
    if ($early) { return $early }
    if (-not (Open-SecondPage $ctx $urlB)) { return @('NOT_DRIVEN', $one.Path, 'the second page did not open as a tab of the app window alone') }
    Send-RivetMessage $ctx 'Rivet.ToggleSplit'
    Start-Sleep 6
    $split = Get-Capture $ctx (Join-Path $shots 'split-panel-look-2-split.png')
    $two = Test-TwoPages $split $script:ColourA $script:ColourB
    if ($two.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $split.Path, "no split to look at: $($two.Reason)") }
    $v = Test-PanelLook $split $script:ColourA $script:ColourB ($ctx.Dpi / 96.0)
    @($v.Verdict, $split.Path, $v.Reason)
  } finally { Stop-Scene $ctx }
}

# FR-10: the rail takes its own width, and the open Sidebar page floats over the page without
# changing the page's width.
function Invoke-SidebarFloats($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'sidebar-floats' $urlB $shots
  try {
    if ($early) { return $early }
    if (-not (Open-SecondPage $ctx $urlMark)) { return @('NOT_DRIVEN', $one.Path, 'the marked page did not open as a tab of the app window alone') }
    $plain = Get-Capture $ctx (Join-Path $shots 'sidebar-floats-2-page.png')
    if (-not (Send-Flag $ctx 'rivet-sidepanel.flag' 'sim open')) { return @('NOT_DRIVEN', $plain.Path, 'the runtime did not take the open command') }
    Start-Sleep 3
    $rail = Get-Capture $ctx (Join-Path $shots 'sidebar-floats-3-rail.png')
    $width = Test-RailWidth $plain $rail $script:ColourA ($ctx.Dpi / 96.0)
    if ($width.Verdict -ne 'PASS') { return @($width.Verdict, $rail.Path, "rail: $($width.Reason)") }
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'dock 0')) { return @('NOT_DRIVEN', $rail.Path, 'the runtime did not take the dock command') }
    Start-Sleep 3
    $docked = Get-Capture $ctx (Join-Path $shots 'sidebar-floats-4-panel.png')
    $v = Test-PanelFloats $rail $docked $script:ColourA $script:ColourB ($ctx.Dpi / 96.0)
    @($v.Verdict, $docked.Path, "$($width.Reason); $($v.Reason)")
  } finally { Stop-Scene $ctx }
}

# FR-10: the Sidebar starts folded with no open button in sight, and the button fades in, faint,
# when the pointer reaches the lower right corner. The pointer is held there by the runtime's test
# command, since a test never moves the real pointer.
function Invoke-SidebarOpenButton($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'sidebar-open-button' $urlA $shots
  try {
    if ($early) { return $early }
    $s = $ctx.Dpi / 96.0
    $a = Get-PageRegion $one $script:ColourA
    if ($a.Width -lt 200) { return @('NOT_DRIVEN', $one.Path, 'the test page was not shown') }
    $box = @([int]($a.X1 - 72 * $s), [int]($a.Y1 - 72 * $s), $a.X1, $a.Y1)
    if (-not (Send-Flag $ctx 'rivet-sidepanel.flag' 'sim near')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the pointer command') }
    Start-Sleep 2
    $near = Get-Capture $ctx (Join-Path $shots 'sidebar-open-button-2-near.png')
    if (-not (Send-Flag $ctx 'rivet-sidepanel.flag' 'sim over')) { return @('NOT_DRIVEN', $near.Path, 'the runtime did not take the pointer command') }
    Start-Sleep 2
    $over = Get-Capture $ctx (Join-Path $shots 'sidebar-open-button-3-over.png')
    Send-Flag $ctx 'rivet-sidepanel.flag' 'sim away' | Out-Null
    # The page reaching the window's right edge, with no rail beside it, is the folded start.
    $o = Get-ClientOffset $ctx
    $c = New-Object FeatureWin+R; [FeatureWin]::GetClientRect($ctx.Hwnd, [ref]$c) | Out-Null
    $rightGap = ($o.X + $c.Ri) - $a.X1
    if ($rightGap -gt 30 * $s) { return @('FAIL', $one.Path, ('the Sidebar did not start folded: {0} px between the page and the window edge' -f $rightGap)) }
    $v = Test-OpenButtonFade $one $near $over $script:ColourA $box
    @($v.Verdict, $near.Path, "folded at start; $($v.Reason)")
  } finally { Stop-Scene $ctx }
}

# FR-11: a 127.0.0.1 page starts in developer mode; its row has its own background, a monospace
# address and seven buttons left of the caption buttons.
function Invoke-DevRow($Name, $shots) {
  $site = Start-LocalSite $siteDir
  $ctx = $null
  try {
    $ctx, $one, $early = Open-SurfaceScene $Name 'dev-row' "$($site.Base)/$devNarrow" $shots
    if ($early) { return $early }
    $s = $ctx.Dpi / 96.0
    $before = @(Read-Log).Count
    Send-Flag $ctx 'rivet-tab.txt' 'devactions' | Out-Null
    $line = Wait-Log $before '^dev mode ' 5
    if ($line -notmatch '^dev mode true') { return @('FAIL', $one.Path, "the 127.0.0.1 page did not start in developer mode: $line") }
    $count = ([regex]::Matches(($line -replace '^.*actions', ''), '\w+')).Count
    if (-not (Send-Flag $ctx 'rivet-tab.txt' "navigate $($site.Base)/$devWide")) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the navigate command') }
    Start-Sleep 4
    $wide = Get-Capture $ctx (Join-Path $shots 'dev-row-2-wide.png')
    Send-RivetMessage $ctx 'Rivet.ToggleDevMode'
    Start-Sleep 3
    $plain = Get-Capture $ctx (Join-Path $shots 'dev-row-3-plain.png')
    $bar = Test-BarAtTop $plain $script:ColourA $script:ColourC $s
    if ($bar.Verdict -ne 'PASS') { return @('NOT_DRIVEN', $plain.Path, "the row could not be found: $($bar.Reason)") }
    # The row from the runtime's own place for the address, since a browser infobar (Brave's analytics
    # notice on a new profile) can sit between the row and the page.
    $anc = Get-BarAnchors $ctx
    if ($anc) {
      $co = Get-ClientOffset $ctx
      $rowTop = [int]($co.Y + ($anc.Address[1] - (44 - $anc.Address[3]) / 2) * $s); $rowBottom = [int]($rowTop + 44 * $s)
    } else {
      $rowTop = [int]($bar.PageTop - 44 * $s); $rowBottom = $bar.PageTop
    }
    $capLeft = [int]($bar.PageRight - 3 * 46 * $s)
    $span = @([int]($capLeft - (6 + 7 * 30) * $s), $capLeft)
    $row = Test-DevRow $plain $wide @($rowTop, $rowBottom) $span ([Math]::Min(7, $count))
    if ($row.Verdict -ne 'PASS') { return @($row.Verdict, $wide.Path, "$($row.Reason); the build offers $count buttons") }
    $notes = @($row.Reason)
    if ($count -lt 7) { return @('FAIL', $wide.Path, "$($notes -join '; '); the build offers $count of the seven buttons") }
    $text = @([int]($bar.PageLeft + 130 * $s), [int]($rowTop + 10 * $s), ($span[0] - [int](8 * $s)), [int]($rowBottom - 10 * $s))
    $narrow = Get-InkWidth $one $text; $wideInk = Get-InkWidth $wide $text
    $mono = Test-Monospace $narrow $wideInk $s ($text[2] - $text[0])
    @($mono.Verdict, $wide.Path, "$($notes -join '; '); address: $($mono.Reason)")
  } finally { if ($ctx) { Stop-Scene $ctx }; Stop-LocalSite $site }
}

# FR-12: a Korean composition is drawn in the address field with the input method's windows at the
# caret, and copy, paste, select all and undo run through the windowless text engine.
function Invoke-AddressEditing($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'address-editing' $urlA $shots
  $saved = $null
  try {
    if ($early) { return $early }
    $s = $ctx.Dpi / 96.0
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'bar')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the bar command') }
    $before = @(Read-Log).Count
    Send-Flag $ctx 'rivet-tab.txt' 'baredit selectall' | Out-Null
    Send-Chars $ctx 'rivet'
    Start-Sleep 1
    Send-Flag $ctx 'rivet-tab.txt' 'barime 한' | Out-Null
    $ime = Wait-Log $before '^address bar ime:' 5
    if (-not $ime -or $ime -match ' set 0 ') { return @('NOT_DRIVEN', $one.Path, "the Korean input method took no composition: $ime") }
    Start-Sleep 1
    $composing = Get-Capture $ctx (Join-Path $shots 'address-editing-2-composing.png')
    $m = [regex]::Match($ime, 'composition window \d+ at (-?\d+),(-?\d+).*candidate \d+ at (-?\d+),(-?\d+).*caret Some\(\((-?\d+), (-?\d+), (-?\d+)\)\)')
    if (-not $m.Success) { return @('FAIL', $composing.Path, "no caret or input method windows in: $ime") }
    $cx, $cy, $kx, $ky, $left, $top, $bottom = 1..7 | ForEach-Object { [int]$m.Groups[$_].Value }
    $near = 40 * $s
    if ([Math]::Abs($cx - $left) -gt $near -or $cy -lt $top - $near -or $cy -gt $bottom + $near) { return @('FAIL', $composing.Path, "the composition window sits at $cx,$cy, the caret at $left,$top-$bottom") }
    if ([Math]::Abs($kx - $left) -gt $near -or $ky -lt $top - $near -or $ky -gt $bottom + $near) { return @('FAIL', $composing.Path, "the candidate window sits at $kx,$ky, the caret at $left,$top-$bottom") }
    Send-Flag $ctx 'rivet-tab.txt' 'barimedone' | Out-Null
    $before = @(Read-Log).Count
    Send-Flag $ctx 'rivet-tab.txt' 'bartext' | Out-Null
    $t = Wait-Log $before '^address bar text' 5
    if ($t -notmatch 'rivet한') { return @('FAIL', $composing.Path, "the finished composition is not in the field: $t") }
    $notes = @('composition at the caret, candidates beside it, finished as rivet한')
    # The clipboard is the person's: it is kept and put back afterwards.
    $saved = Get-Clipboard -Raw -ErrorAction SilentlyContinue
    $edit = {
      param([string]$Word)
      $b = @(Read-Log).Count
      Send-Flag $ctx 'rivet-tab.txt' "baredit $Word" | Out-Null
      Wait-Log $b "^address bar edit ${Word}:" 5
    }
    & $edit 'selectall' | Out-Null
    $copied = & $edit 'copy'
    if ((Get-Clipboard -Raw) -ne 'rivet한') { return @('FAIL', $composing.Path, "copy put '$(Get-Clipboard -Raw)' on the clipboard: $copied") }
    $notes += 'copy'
    Set-Clipboard -Value 'pasted-rivet'
    & $edit 'selectall' | Out-Null
    $pasted = & $edit 'paste'
    if ($pasted -notmatch 'text Some\("pasted-rivet"\)') { return @('FAIL', $composing.Path, "paste: $pasted") }
    $notes += 'paste'
    $undone = & $edit 'undo'
    if ($undone -notmatch 'text Some\("rivet한"\)') { return @('FAIL', $composing.Path, "undo did not bring back the text before the paste: $undone") }
    $notes += 'undo'
    $after = Get-Capture $ctx (Join-Path $shots 'address-editing-3-after.png')
    @('PASS', $composing.Path, ($notes -join '; '))
  } finally {
    if ($null -ne $saved) { Set-Clipboard -Value $saved }
    Stop-Scene $ctx
  }
}

# FR-12: a click on the security chip opens the browser's page info bubble near the address row,
# inside the Rivet window. What the bubble lists is not read: a test page has no certificate.
function Invoke-SecurityChip($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'security-chip' $urlA $shots
  try {
    if ($early) { return $early }
    $anchors = Get-BarAnchors $ctx
    if (-not $anchors) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not trace where the chip is') }
    $s = $anchors.Dpi / 96.0
    $x = [int](($anchors.Chip[0] + $anchors.Chip[2] / 2) * $s); $y = [int](($anchors.Chip[1] + $anchors.Chip[3] / 2) * $s)
    $tops = Get-MyTops $ctx
    $before = @(Read-Log).Count
    Send-ClientMouse $ctx 0x200 $x $y
    Send-ClientMouse $ctx 0x201 $x $y 1
    Start-Sleep -Milliseconds 120
    Send-ClientMouse $ctx 0x202 $x $y
    $traced = Wait-Log $before '^address bar: page info' 5
    if (-not $traced) { return @('FAIL', $one.Path, 'the click on the chip did not reach it') }
    if ($traced -notmatch 'true') { return @('FAIL', $one.Path, "the chip did not open page info: $traced") }
    $found = Wait-NewTops $ctx $tops 8
    $shot = Get-Capture $ctx (Join-Path $shots 'security-chip-2-open.png')
    if ($found.Count -eq 0) { return @('FAIL', $shot.Path, 'no page info bubble appeared') }
    Get-Capture ([pscustomobject]@{ Hwnd = $found[0].Hwnd }) (Join-Path $shots 'security-chip-3-bubble.png') | Out-Null
    $found[0] | Add-Member -Force PageInfo $true
    $v = Test-BubbleNearBar $ctx $found[0] $anchors
    @($v.Verdict, $shot.Path, "page info opened; $($v.Reason)")
  } finally { Stop-Scene $ctx }
}

# FR-12: the browser's permission bubble is anchored by the address row inside the Rivet window.
function Invoke-PermissionBubble($Name, $shots) {
  $site = Start-LocalSite $siteDir
  $ctx = $null
  try {
    $ctx, $one, $early = Open-SurfaceScene $Name 'permission-bubble' $urlA $shots
    if ($early) { return $early }
    $anchors = Get-BarAnchors $ctx
    if (-not $anchors) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not trace where the address is') }
    $tops = Get-MyTops $ctx
    if (-not (Send-Flag $ctx 'rivet-tab.txt' "navigate $($site.Base)/geo.html")) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the navigate command') }
    $found = Wait-NewTops $ctx $tops 12
    $shot = Get-Capture $ctx (Join-Path $shots 'permission-bubble-2-asked.png')
    if ($found.Count -eq 0) { return @('FAIL', $shot.Path, 'the location request showed no permission bubble') }
    Get-Capture ([pscustomobject]@{ Hwnd = $found[0].Hwnd }) (Join-Path $shots 'permission-bubble-3-bubble.png') | Out-Null
    $anchors = Get-BarAnchors $ctx
    $v = Test-BubbleNearBar $ctx $found[0] $anchors
    @($v.Verdict, $shot.Path, $v.Reason)
  } finally { if ($ctx) { Stop-Scene $ctx }; Stop-LocalSite $site }
}

# FR-12: the browser's password save bubble, after a fake sign in on a loopback page, is anchored
# by the address row inside the Rivet window. Fake credentials only, on a fresh test profile.
function Invoke-PasswordBubble($Name, $shots) {
  $site = Start-LocalSite $siteDir
  $ctx = $null
  try {
    $ctx, $one, $early = Open-SurfaceScene $Name 'password-bubble' "$($site.Base)/login.html" $shots
    if ($early) { return $early }
    Send-Chars $ctx 'rivet-test-user'
    Send-Key $ctx 0x09; [FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr]9, [IntPtr]1) | Out-Null
    Send-Chars $ctx 'fake-pass-1234'
    Start-Sleep 1
    $typed = Get-Capture $ctx (Join-Path $shots 'password-bubble-2-typed.png')
    if ((Get-PageRegion $typed $script:ColourC).Width -lt 200) { return @('NOT_DRIVEN', $typed.Path, 'the posted typing did not reach the page form') }
    $anchors = Get-BarAnchors $ctx
    if (-not $anchors) { return @('NOT_DRIVEN', $typed.Path, 'the runtime did not trace where the address is') }
    $tops = Get-MyTops $ctx
    [FeatureWin]::PostMessage($ctx.Hwnd, 0x100, [IntPtr]0x0D, [IntPtr]1) | Out-Null
    [FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr]0x0D, [IntPtr]1) | Out-Null
    [FeatureWin]::PostMessage($ctx.Hwnd, 0x101, [IntPtr]0x0D, [IntPtr](0xC0000001 -as [int])) | Out-Null
    $found = Wait-NewTops $ctx $tops 12
    $shot = Get-Capture $ctx (Join-Path $shots 'password-bubble-3-signed-in.png')
    if ($found.Count -eq 0) { return @('FAIL', $shot.Path, 'the sign in showed no password bubble') }
    Get-Capture ([pscustomobject]@{ Hwnd = $found[0].Hwnd }) (Join-Path $shots 'password-bubble-4-bubble.png') | Out-Null
    $v = Test-BubbleNearBar $ctx $found[0] $anchors
    @($v.Verdict, $shot.Path, $v.Reason)
  } finally { if ($ctx) { Stop-Scene $ctx }; Stop-LocalSite $site }
}

# NFR-5: typing in the address field leaves the browser window active with the focus on it. No
# Rivet window takes the keyboard, the foreground stays where it was, and the caption buttons keep
# their active look.
function Invoke-TypingKeepsActive($Name, $shots) {
  $ctx, $one, $early = Open-SurfaceScene $Name 'typing-keeps-active' $urlA $shots
  try {
    if ($early) { return $early }
    $s = $ctx.Dpi / 96.0
    $tops = Get-MyTops $ctx
    $foreground = [SurfaceWin]::GetForegroundWindow()
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'bar')) { return @('NOT_DRIVEN', $one.Path, 'the runtime did not take the bar command') }
    Start-Sleep 1
    $first = [SurfaceWin]::Focus($ctx.Hwnd)
    $idle = Get-Capture $ctx (Join-Path $shots 'typing-keeps-active-2-focused.png')
    Send-Chars $ctx 'typing in rivet'
    Start-Sleep 2
    $typing = Get-Capture $ctx (Join-Path $shots 'typing-keeps-active-3-typed.png')
    $then = [SurfaceWin]::Focus($ctx.Hwnd)
    $before = @(Read-Log).Count
    Send-Flag $ctx 'rivet-tab.txt' 'bartext' | Out-Null
    $t = Wait-Log $before '^address bar text' 5
    if ($t -notmatch 'typing in rivet') { return @('NOT_DRIVEN', $typing.Path, "the typing did not reach the field: $t") }
    $extra = @(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $tops -notcontains $_ })
    if ($extra.Count -gt 0) { return @('FAIL', $typing.Path, "typing opened $($extra.Count) other window(s)") }
    if ([SurfaceWin]::GetForegroundWindow() -ne $foreground) { return @('FAIL', $typing.Path, 'the foreground window changed while typing') }
    $mine = { param($h) $h -eq $ctx.Hwnd -or [SurfaceWin]::IsChild($ctx.Hwnd, $h) }
    if ($then[1] -ne [IntPtr]::Zero -and -not (& $mine $then[1])) { return @('FAIL', $typing.Path, ('the focus moved to window {0:X} while typing' -f [int64]$then[1])) }
    if ($then[0] -ne $first[0]) { return @('FAIL', $typing.Path, ('the active window changed from {0:X} to {1:X}' -f [int64]$first[0], [int64]$then[0])) }
    $o = Get-ClientOffset $ctx
    $c = New-Object FeatureWin+R; [FeatureWin]::GetClientRect($ctx.Hwnd, [ref]$c) | Out-Null
    $cx = [int]($o.X + $c.Ri - 23 * $s); $cy = [int]($o.Y + 22 * $s)
    $inkIdle = Get-ButtonInk $idle $cx $cy ([int](6 * $s)); $inkTyping = Get-ButtonInk $typing $cx $cy ([int](6 * $s))
    if ($null -eq $inkIdle -or [Math]::Abs($inkIdle - $inkTyping) -gt 6) { return @('FAIL', $typing.Path, ('the close button changed its look while typing ({0:N0} to {1:N0})' -f $inkIdle, $inkTyping)) }
    $focusNote = if ($then[1] -eq [IntPtr]::Zero) { 'the thread reports no focus window, as the off-screen test window is never made foreground' } else { 'focus on the browser window' }
    @('PASS', $typing.Path, ('text reached the field; no new window, foreground unchanged, active window unchanged; {0}; close button look {1:N0} and {2:N0}' -f $focusNote, $inkIdle, $inkTyping))
  } finally { Stop-Scene $ctx }
}
# --- End of the window surface rows -----------------------------------------------------------

# Runs one `rivet` control command of the scene's session with --json and returns the exit code and
# the parsed answer.
function Invoke-Rivet($ctx, $state, [string[]]$Words) {
  $out = & (Join-Path $debug 'rivet.exe') @Words --id $ctx.SessionId --state $state --json 2>&1 | ForEach-Object { [string]$_ }
  $code = $LASTEXITCODE
  $line = $out | Where-Object { $_.StartsWith('{') } | Select-Object -Last 1
  $value = if ($line) { try { $line | ConvertFrom-Json } catch { $null } } else { $null }
  [pscustomobject]@{ Code = $code; Value = $value; Text = ($out -join ' ') }
}

# Pages, navigation, reads and workspaces through `rivet`, checked against what the browser reports.
function Invoke-CliControl($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'cli-control' $urlA
  try {
    Open-Scene $ctx $state $urlA $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    if (-not $ctx.SessionId) { return @('NOT_DRIVEN', $null, 'the session reported no id') }
    $notes = @()
    $r = Invoke-Rivet $ctx $state @('page', 'open', $urlB)
    if ($r.Code -ne 0) { return @('FAIL', $null, "page open exited $($r.Code): $($r.Text)") }
    Start-Sleep 3
    $r = Invoke-Rivet $ctx $state @('page', 'list')
    $urls = @($r.Value.pages | ForEach-Object { $_.url })
    if (-not ($urls | Where-Object { $_ -like '*page-a.html' }) -or -not ($urls | Where-Object { $_ -like '*page-b.html' })) { return @('FAIL', $null, "page list does not show the frame page and the CLI page: $($urls -join ' ')") }
    $notes += "page list shows $($urls.Count) pages"
    $r = Invoke-Rivet $ctx $state @('nav', 'go', $urlC)
    if ($r.Code -ne 0) { return @('FAIL', $null, "nav go exited $($r.Code): $($r.Text)") }
    Start-Sleep 3
    $r = Invoke-Rivet $ctx $state @('read', 'url')
    if ($r.Value.value -notlike '*page-c.html') { return @('FAIL', $null, "read url after nav go: $($r.Text)") }
    $r = Invoke-Rivet $ctx $state @('read', 'title')
    if ($r.Value.value -ne 'page-c') { return @('FAIL', $null, "read title after nav go: $($r.Text)") }
    $r = Invoke-Rivet $ctx $state @('nav', 'back')
    Start-Sleep 3
    $r = Invoke-Rivet $ctx $state @('read', 'url')
    if ($r.Value.value -notlike '*page-b.html') { return @('FAIL', $null, "read url after nav back: $($r.Text)") }
    $notes += 'nav go, read url and title, nav back worked'
    $r = Invoke-Rivet $ctx $state @('workspace', 'list')
    if ($r.Code -ne 0 -or @($r.Value.workspaces).Count -lt 1) { return @('FAIL', $null, "workspace list: $($r.Text)") }
    $notes += "workspace list shows $(@($r.Value.workspaces).Count)"
    $r = Invoke-Rivet $ctx $state @('page', 'close', '1')
    Start-Sleep 2
    $r = Invoke-Rivet $ctx $state @('page', 'list')
    if (@($r.Value.pages).Count -ne ($urls.Count - 1)) { return @('FAIL', $null, "page close left $(@($r.Value.pages).Count) pages") }
    $cap = Get-Capture $ctx (Join-Path $shots 'cli-control.png')
    @('PASS', $cap.Path, (($notes + 'page close removed the page') -join '; '))
  } finally { Stop-Scene $ctx }
}

# Typing and a key through `rivet input`: the page copies its field into its title.
function Invoke-CliInput($Name, $shots) {
  $path = Join-Path $pageDir 'input.html'
  [IO.File]::WriteAllText($path, "<!doctype html><meta charset=utf-8><title>empty</title><input id=f autofocus style='font-size:40px' oninput='document.title=this.value' onkeydown=`"if(event.key==='Enter')document.title='enter:'+this.value`">")
  $url = 'file:///' + ($path -replace '\\', '/')
  $ctx, $state = Start-Scene $Name 'cli-input' $url
  try {
    Open-Scene $ctx $state $url $null
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    $r = Invoke-Rivet $ctx $state @('input', 'click', '200', '200')
    $r = Invoke-Rivet $ctx $state @('input', 'type', 'rivet')
    if ($r.Code -ne 0) { return @('FAIL', $null, "input type exited $($r.Code): $($r.Text)") }
    Start-Sleep 2
    $typed = (Invoke-Rivet $ctx $state @('read', 'title')).Value.value
    $r = Invoke-Rivet $ctx $state @('input', 'key', 'enter')
    Start-Sleep 2
    $entered = (Invoke-Rivet $ctx $state @('read', 'title')).Value.value
    $cap = Get-Capture $ctx (Join-Path $shots 'cli-input.png')
    if ($typed -ne 'rivet') { return @('FAIL', $cap.Path, "typed text did not reach the page field: title $typed") }
    if ($entered -ne 'enter:rivet') { return @('FAIL', $cap.Path, "Enter did not reach the page: title $entered") }
    @('PASS', $cap.Path, 'typed text and Enter reached the page field')
  } finally { Stop-Scene $ctx }
}

# A hidden session: cloaked, still navigating; a frame read is refused under hidden and shows the
# window under auto. The window stays off-screen because the whole run sets RIVET_KEEP_OFFSCREEN.
function Invoke-CliHidden($Name, $shots) {
  $ctx, $state = Start-Scene $Name 'cli-hidden' $urlA
  try {
    Open-Scene $ctx $state $urlA $null @('--display', 'hidden')
    if ($ctx.Error -and $ctx.Error -ne 'attach-failed') { return @('NOT_DRIVEN', $null, $ctx.Error) }
    if (-not (Test-Attached $ctx)) { return @('FAIL', $null, 'Rivet did not attach to the window') }
    if (-not $ctx.SessionId) { return @('NOT_DRIVEN', $null, 'the hidden session reported no id') }
    if (-not [CloakWin]::Cloaked($ctx.Hwnd)) { return @('FAIL', $null, 'the hidden session window is not cloaked') }
    $r = Invoke-Rivet $ctx $state @('nav', 'go', $urlB, '--display', 'hidden')
    Start-Sleep 3
    $r = Invoke-Rivet $ctx $state @('read', 'url', '--display', 'hidden')
    if ($r.Value.value -notlike '*page-b.html') { return @('FAIL', $null, "a hidden session did not navigate: $($r.Text)") }
    if (-not [CloakWin]::Cloaked($ctx.Hwnd)) { return @('FAIL', $null, 'navigation showed the hidden window') }
    $bmp = Join-Path $shots 'cli-hidden-frame.bmp'
    $r = Invoke-Rivet $ctx $state @('read', 'frame', '--out', $bmp, '--display', 'hidden')
    if ($r.Code -eq 0 -or $r.Value.error -ne 'needs_visible') { return @('FAIL', $null, "a hidden frame read was not refused with a reason: $($r.Text)") }
    if (-not [CloakWin]::Cloaked($ctx.Hwnd)) { return @('FAIL', $null, 'the refused frame read showed the window') }
    $r = Invoke-Rivet $ctx $state @('read', 'frame', '--out', $bmp, '--display', 'auto')
    if ($r.Code -ne 0 -or -not $r.Value.promoted -or -not (Test-Path $bmp)) { return @('FAIL', $null, "auto did not show the window for a frame read: $($r.Text)") }
    if ([CloakWin]::Cloaked($ctx.Hwnd)) { return @('FAIL', $bmp, 'auto reported a promotion but the window is still cloaked') }
    @('PASS', $bmp, 'hidden window cloaked and navigating; hidden frame read refused with needs_visible; auto showed it and wrote the capture; open the bitmap to check it')
  } finally { Stop-Scene $ctx }
}

# --- CLI control channel -------------------------------------------------------------------
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class CloakWin{
 [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h,int a,out int v,int n);
 // DWMWA_CLOAKED: non-zero while the window manager keeps the window off every screen.
 public static bool Cloaked(IntPtr h){int v;return DwmGetWindowAttribute(h,14,out v,4)==0&&v!=0;}
}
'@

# --- Run safety -----------------------------------------------------------------------------
# Every row runs under RIVET_KEEP_OFFSCREEN with its Rivet files in this run's folder, and the
# visible guard stops the run when a test window shows on a monitor, takes the foreground or holds
# the mouse. The open-link rows hand a link to a running window and so run alone: while another
# run is active they are not driven, and no other run starts while they are.
. (Join-Path $PSScriptRoot 'windows-test-safety.ps1')
$aloneRows = @($Features | Where-Object { $_ -like 'open-link-*' })
$safety = $null
if ($built) {
  $safety = Start-RunSafety -OutDir $OutDir -Name 'features' -Alone:($aloneRows.Count -gt 0) -DropAloneRows
}

# --- Run ---------------------------------------------------------------------------------
foreach ($name in $Browsers) {
  $exePath, $exeName = $table[$name]
  $version = if (Test-Path $exePath) { (Get-Item $exePath).VersionInfo.ProductVersion } else { 'not installed' }
  $shots = Join-Path $OutDir $name
  New-Item -ItemType Directory -Force $shots | Out-Null
  foreach ($feature in $Features) {
    # A full disk leaves a fresh profile half written: no key in Local State, an empty Web Data and
    # a browser profile error dialog on the owner's screen. Stop the run before that can happen.
    $free = Get-FreeGigabytes $OutDir
    if ($free -lt $MinFreeGigabytes) {
      Write-Host "run stopped before $name $feature`: $free GB free on the output drive, below $MinFreeGigabytes GB"
      exit 2
    }
    if (-not $built) { Add-Row $name $version $feature 'NOT_DRIVEN' $null "no browser was started: $buildNote"; continue }
    if (-not (Test-Path $exePath)) { Add-Row $name $version $feature 'NOT_DRIVEN' $null 'the browser is not installed'; continue }
    Assert-RunSafe $safety
    if ($aloneRows -contains $feature -and -not $safety.Alone) { Add-Row $name $version $feature 'NOT_DRIVEN' $null "the open-link rows run alone: $($safety.AloneRefused)"; continue }
    try {
      $result = switch ($feature) {
        'split' { Invoke-Split $name $version $shots }
        'sidebar' { Invoke-Sidebar $name $version $shots }
        'newtab' { Invoke-NewTab $name $version $shots }
        'space' { Invoke-Space $name $version $shots }
        'split-start' { Invoke-SplitPath $name $shots 'split-start' 'message' 'start' }
        'split-menu' { Invoke-SplitPath $name $shots 'split-menu' 'menu' 'colour' }
        'split-menu-start' { Invoke-SplitPath $name $shots 'split-menu-start' 'menu' 'start' }
        'split-drag' { Invoke-SplitPath $name $shots 'split-drag' 'drag' 'colour' }
        'split-drag-start' { Invoke-SplitPath $name $shots 'split-drag-start' 'drag' 'start' }
        'split-shortcut' { @('NOT_DRIVEN', $null, 'the shortcut hook reads the real key state with GetKeyState, which posted keys do not change; only a real key press with focus reaches it') }
        'split-one-tab' { Invoke-SplitOneTab $name $shots }
        'split-maximised' { @('NOT_DRIVEN', $null, 'a maximised window moves onto a visible monitor, which the off-screen rule forbids') }
        'split-space' { Invoke-SplitPath $name $shots 'split-space' 'message' 'colour' -AfterSpace }
        'split-click' { Invoke-SplitPath $name $shots 'split-click' 'message' 'colour' -ThenClick }
        'split-rows' { Invoke-SplitRows $name $shots }
        'split-drop-top' { Invoke-SplitGrid $name $shots 'split-drop-top' }
        'split-drop-bottom' { Invoke-SplitGrid $name $shots 'split-drop-bottom' }
        'split-grid-3' { Invoke-SplitGrid $name $shots 'split-grid-3' }
        'split-grid-4' { Invoke-SplitGrid $name $shots 'split-grid-4' }
        'bar-layout' { Invoke-BarLayout $name $shots }
        'mini-pop-out' { Invoke-Mini $name $shots 'pop-out' }
        'mini-move' { Invoke-Mini $name $shots 'move' }
        'mini-return' { Invoke-Mini $name $shots 'return' }
        'mini-close' { Invoke-Mini $name $shots 'close' }
        'tab-sections' { Invoke-TabSections $name $shots }
        'tab-close-select' { Invoke-TabCloseSelect $name $shots }
        'tab-reopen' { Invoke-TabReopen $name $shots }
        'tab-archive' { Invoke-TabArchive $name $shots }
        'ctrl-t-setting' { Invoke-CtrlTSetting $name $shots }
        'link-new-tab' { Invoke-LinkNewTab $name $shots }
        'command-bar' { Invoke-CommandBar $name $shots }
        'link-preview' { Invoke-LinkPreview $name $shots }
        'rivet-settings' { Invoke-RivetSettings $name $shots }
        'link-rules' { Invoke-LinkRules $name $shots }
        'address-suggest' { Invoke-AddressSuggest $name $shots }
        'appbar-width' { Invoke-AppbarWidth $name $shots }
        'appbar-peek' { Invoke-AppbarPeek $name $shots }
        'open-link-running' { Invoke-OpenLink $name $shots 'running' }
        'open-link-start' { Invoke-OpenLink $name $shots 'start' }
        'credentials-browser' { Invoke-Credentials $name $shots 'browser' }
        'credentials-system' { Invoke-Credentials $name $shots 'system' }
        'view-click' { Invoke-ViewInput $name $shots 'view-click' 'none' }
        'view-click-other' { @('NOT_DRIVEN', $null, 'a page of another window needs a second browser window, which opens a normal window that may show on screen and take the keyboard') }
        'view-keys' { Invoke-ViewInput $name $shots 'view-keys' 'keys' }
        'view-ime' { if ($env:RIVET_TEST_DESKTOP) { @('NOT_DRIVEN', $null, 'the input method composes only on the desktop that has the keyboard, and a test desktop never has it') } else { Invoke-ViewInput $name $shots 'view-ime' 'ime' } }
        'tab-moves' { Invoke-TabMoves $name $shots }
        'first-frame' { Invoke-FirstFrame $name $shots }
        'fullscreen-sidebar' { Invoke-FullscreenSidebar $name $shots }
        'taskbar-identity' { if ($env:RIVET_TEST_DESKTOP) { @('NOT_DRIVEN', $null, 'a test desktop has no taskbar to read the window icon from') } else { Invoke-TaskbarIdentity $name $shots } }
        'taskbar-relaunch' { Invoke-TaskbarRelaunch $name $shots }
        'sidebar-scale-monitor' { @('NOT_DRIVEN', $null, 'a move to a monitor of another scale needs the window on a real monitor, which the off-screen rule forbids; the scale of the current monitor is checked by windows-frame.ps1') }
        'split-panel-look' { Invoke-SplitPanelLook $name $shots }
        'sidebar-floats' { Invoke-SidebarFloats $name $shots }
        'sidebar-open-button' { Invoke-SidebarOpenButton $name $shots }
        'dev-row' { Invoke-DevRow $name $shots }
        'address-editing' { if ($env:RIVET_TEST_DESKTOP) { @('NOT_DRIVEN', $null, 'the input method composes only on the desktop that has the keyboard, and a test desktop never has it') } else { Invoke-AddressEditing $name $shots } }
        'security-chip' { Invoke-SecurityChip $name $shots }
        'permission-bubble' { Invoke-PermissionBubble $name $shots }
        'password-bubble' { Invoke-PasswordBubble $name $shots }
        'typing-keeps-active' { Invoke-TypingKeepsActive $name $shots }
        'cli-control' { Invoke-CliControl $name $shots }
        'cli-input' { if ($env:RIVET_TEST_DESKTOP) { @('NOT_DRIVEN', $null, 'a page takes typed text only in an active window, and no test window is ever activated') } else { Invoke-CliInput $name $shots } }
        'cli-hidden' { if ($env:RIVET_TEST_DESKTOP) { @('NOT_DRIVEN', $null, 'the window manager reports no cloak for windows of a desktop it does not show') } else { Invoke-CliHidden $name $shots } }
      }
      $verdict, $evidence, $reason = $result
      Add-Row $name $version $feature $verdict $evidence $reason
    } catch {
      Add-Row $name $version $feature 'NOT_DRIVEN' $null "the check raised: $($_.Exception.Message)"
    }
  }
}

$json = Join-Path $OutDir 'results.json'
$text = Join-Path $OutDir 'results.txt'
$rows | ConvertTo-Json -Depth 4 | Set-Content -Path $json -Encoding utf8
Format-Rows $rows | Set-Content -Path $text -Encoding utf8
''
Format-Rows $rows
"json: $json"
Stop-RunSafety $safety
$bad = @($rows | Where-Object { $_.result -ne 'PASS' }).Count
if ($bad) { exit 1 }
