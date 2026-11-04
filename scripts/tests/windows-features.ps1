# Per-browser feature contract for the Rivet window. A feature is called working on a browser only
# when this passes on a launch like the installed build's. Unit tests and build rows do not count.
#
#   pwsh -NoProfile -File scripts/tests/windows-features.ps1 [-Browsers chrome,edge,...]
#       [-Features split,sidebar,newtab,space,split-grid-4,bar-layout,command-bar,link-preview] [-OutDir <dir>] [-NoBuild]
#
# One row per browser and feature: PASS, FAIL or NOT_DRIVEN. NOT_DRIVEN means the scene could not
# be set up, so the feature was not exercised. Nothing is decided from build rows or `supported`.
#
# Safety: test windows sit off-screen and are captured with PrintWindow. No window is activated and
# the cursor never moves. Each run uses a fresh profile inside this checkout with sync and sign-in
# off, and only processes whose command line holds that profile are stopped. Nothing is copied out
# of a profile. A browser is started only when the build exited 0.
param(
  [string[]]$Browsers = @('chrome', 'edge', 'whale', 'brave', 'samsung'),
  [string[]]$Features = @('split', 'sidebar', 'newtab', 'space', 'split-start', 'split-menu', 'split-menu-start', 'split-drag', 'split-drag-start', 'split-shortcut', 'split-one-tab', 'split-maximised', 'split-space', 'split-click', 'split-rows', 'split-drop-top', 'split-drop-bottom', 'split-grid-3', 'split-grid-4', 'bar-layout', 'mini-pop-out', 'mini-move', 'mini-return', 'mini-close', 'tab-sections', 'tab-close-select', 'tab-reopen', 'tab-archive', 'ctrl-t-setting', 'link-new-tab', 'command-bar', 'link-preview'),
  [string]$OutDir,
  [switch]$NoBuild
)
$ErrorActionPreference = 'Continue'
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

# Cargo comes from the caller's toolchain: CARGO_HOME when it is set, else cargo on the PATH.
$cargo = if ($env:CARGO_HOME -and (Test-Path (Join-Path $env:CARGO_HOME 'bin\cargo.exe'))) { Join-Path $env:CARGO_HOME 'bin\cargo.exe' } else { 'cargo' }

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
  # A launcher without the off-screen flags would open a browser window on the owner's monitor.
  if ($built -and -not ([Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes((Join-Path $debug 'rivet.exe'))).Contains('--window-position=9000,1000'))) {
    $built = $false; $buildNote = 'rivet.exe was not built with the off-screen test flags'
  }
} else {
  # The window flags exist for tests only: off-screen, no sync, no sign-in. CalculateNativeWinOcclusion
  # is off because Chrome treats an off-screen window as covered and stops painting its pages, and a
  # pixel check on an unpainted page proves nothing. It is the one flag beyond the installed launch
  # that the usual off-screen test tool does not add. The launcher file is
  # restored right after the build and must never be committed.
  $f = Join-Path $root 'core\src\lib.rs'
  $orig = [IO.File]::ReadAllText($f)
  $flags = @('--disable-sync', '--no-first-run', '--no-default-browser-check', '--disable-features=msImplicitSignin,msEdgeSignIn,CalculateNativeWinOcclusion,Translate', '--lang=en-US', '--window-position=9000,1000', '--window-size=1600,1000')
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
    $h = (Get-Process -Id $mine.ProcessId -ErrorAction SilentlyContinue).MainWindowHandle
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
  $dir = Join-Path (Split-Path -Parent $OutDir) "demo-render\$hash"
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
  try {
    $shot = Start-Process -FilePath $ExePath -ArgumentList ($common + @("--screenshot=`"$png`"", $url)) -PassThru -WindowStyle Hidden
    if (-not $shot.WaitForExit(60000)) { return $null }
    $dump = Start-Process -FilePath $ExePath -ArgumentList ($common + @('--dump-dom', $url)) -PassThru -WindowStyle Hidden -RedirectStandardOutput $dom
    if (-not $dump.WaitForExit(60000)) { return $null }
  } finally { Stop-Mine $ExeName $profile_ }
  $text = [IO.File]::ReadAllText($dom)
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
# protocol's: 1 pop out, 2 move (x | y << 16 in DIPs), 4 return (0 left, 1 right, 2 top, 3 bottom), 5 close.
function Send-MiniCommand($ctx, [int]$Code, [int]$Low = 0, [int]$High = 0) {
  $l = [IntPtr](($High -shl 16) -bor ($Low -band 0xffff))
  [FeatureWin]::PostMessage($ctx.Hwnd, [FeatureWin]::RegisterWindowMessage('Rivet.MiniWindow'), [IntPtr]$Code, $l) | Out-Null
}

# Whether page B floats as a Mini Window over page A: B is a smaller rectangle that page A's region
# surrounds, so A still fills the page area around it.
function Test-MiniShown($capture) {
  # The focused pane pops out. A posted click may not move the focus, so either page may float.
  $one = Test-MiniOver $capture $script:ColourA $script:ColourB
  if ($one.Verdict -eq 'PASS') { return $one }
  $two = Test-MiniOver $capture $script:ColourB $script:ColourA
  if ($two.Verdict -eq 'PASS') { $two.Reason = "page A floats, not the focused page B; $($two.Reason)"; return $two }
  $one
}

# Whether the page of colour $Float floats over the page of colour $Behind. A and B in the result
# are the behind and floating regions.
function Test-MiniOver($capture, [int]$Behind, [int]$Float) {
  $a = Get-PageRegion $capture $Behind
  $b = Get-PageRegion $capture $Float
  if ($b.Area -le 0) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = 'page B is not shown'; A = $a; B = $b } }
  if ($a.Area -le 0) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = 'page A is not shown behind the Mini Window'; A = $a; B = $b } }
  $inside = $b.X0 -gt $a.X0 -and $b.X1 -lt $a.X1 -and $b.Y0 -gt $a.Y0 -and $b.Y1 -lt $a.Y1
  if (-not $inside) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = "page B ($($b.X0),$($b.Y0))-($($b.X1),$($b.Y1)) does not float inside page A ($($a.X0),$($a.Y0))-($($a.X1),$($a.Y1))"; A = $a; B = $b } }
  if ($b.Area * 4 -gt $a.Area) { return [pscustomobject]@{ Verdict = 'FAIL'; Reason = 'page B is too large for a Mini Window'; A = $a; B = $b } }
  [pscustomobject]@{ Verdict = 'PASS'; Reason = "page B floats at ($($b.X0),$($b.Y0)) $($b.Width)x$($b.Height) over page A"; A = $a; B = $b }
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
    # The checker must reject a split: both pages side by side are no Mini Window.
    if ((Test-MiniShown $split).Verdict -ne 'FAIL') { return @('NOT_DRIVEN', $split.Path, 'the Mini Window check did not fail on a split') }
    # Page B is the second pane, so it is focused first and then popped out.
    $before = @(Read-Log).Count
    $b = Get-PageRegion $split $script:ColourB
    $p = New-Object FeatureWin+P; $p.X = [int](($b.X0 + $b.X1) / 2); $p.Y = [int](($b.Y0 + $b.Y1) / 2)
    Send-ClientMouse $ctx 0x201 $p.X $p.Y 1
    Send-ClientMouse $ctx 0x202 $p.X $p.Y 0
    Start-Sleep 2
    Send-MiniCommand $ctx 1
    Start-Sleep 4
    $mini = Get-Capture $ctx (Join-Path $shots "mini-$Step-2-popped.png")
    $log = @(Read-Log | Select-Object -Skip $before)
    if (-not ($log | Where-Object { $_ -match '^mini window: open tab' })) { return @('FAIL', $mini.Path, 'the runtime did not open a Mini Window for the pop-out command') }
    $shown = Test-MiniShown $mini
    if ($shown.Verdict -ne 'PASS') { return @('FAIL', $mini.Path, "pop out: $($shown.Reason)") }
    if ($Step -eq 'pop-out') { return @('PASS', $mini.Path, $shown.Reason) }
    $s = $ctx.Dpi / 96.0
    switch ($Step) {
      'move' {
        $x = [int]($shown.A.X0 / $s) + 40; $y = [int]($shown.A.Y0 / $s) + 40
        Send-MiniCommand $ctx 2 $x $y
        Start-Sleep 3
        $moved = Get-Capture $ctx (Join-Path $shots 'mini-move-3-moved.png')
        $after = Test-MiniShown $moved
        if ($after.Verdict -ne 'PASS') { return @('FAIL', $moved.Path, "after the move: $($after.Reason)") }
        if ([Math]::Abs($after.B.X0 - $shown.B.X0) -lt 20 * $s -or $after.B.X0 -gt $shown.B.X0) { return @('FAIL', $moved.Path, "the Mini Window did not move to the upper left (from $($shown.B.X0),$($shown.B.Y0) to $($after.B.X0),$($after.B.Y0))") }
        return @('PASS', $moved.Path, "moved from ($($shown.B.X0),$($shown.B.Y0)) to ($($after.B.X0),$($after.B.Y0))")
      }
      'return' {
        Send-MiniCommand $ctx 4 0
        Start-Sleep 6
        $back = Get-Capture $ctx (Join-Path $shots 'mini-return-3-split.png')
        $two = Test-TwoPages $back $script:ColourA $script:ColourB
        if ($two.Verdict -ne 'PASS') { return @('FAIL', $back.Path, "after the return: $($two.Reason)") }
        $floated, $stayed = if ($shown.Reason -match '^page A floats') { $script:ColourA, $script:ColourB } else { $script:ColourB, $script:ColourA }
        $a = Get-PageRegion $back $stayed; $bb = Get-PageRegion $back $floated
        if ($bb.X0 -ge $a.X0) { return @('FAIL', $back.Path, 'the floating page came back as a split, but not on the left edge') }
        if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $back.Path, 'the runtime did not take the tab list command') }
        $count = (Get-TabUrls).Count
        if ($count -ne 2) { return @('FAIL', $back.Path, "the window has $count tabs after the return, not the same two") }
        return @('PASS', $back.Path, "page B is back on the left as a split of the same tab; $($two.Reason)")
      }
      'close' {
        Send-MiniCommand $ctx 5
        Start-Sleep 3
        $closed = Get-Capture $ctx (Join-Path $shots 'mini-close-3-closed.png')
        $one = if ($shown.Reason -match '^page A floats') { Test-OnePage $closed $script:ColourB $script:ColourA } else { Test-OnePage $closed $script:ColourA $script:ColourB }
        if ($one.Verdict -ne 'PASS') { return @('FAIL', $closed.Path, "after the close: $($one.Reason)") }
        if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $closed.Path, 'the runtime did not take the tab list command') }
        $count = (Get-TabUrls).Count
        if ($count -ne 2) { return @('FAIL', $closed.Path, "the window has $count tabs after the close; the page should stay in its tab") }
        return @('PASS', $closed.Path, 'the Mini Window closed and page B stays in its tab')
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
    $extraSeen = $false; $onMonitor = $false
    foreach ($i in 1..600) {
      Start-Sleep -Milliseconds 10
      foreach ($t in @(Get-MyTops $ctx)) {
        if ($t -ne $ctx.Hwnd -and $before -notcontains $t) { $extraSeen = $true }
        if ([FeatureMonitor]::OnMonitor($t)) {
          $onMonitor = $true
          [FeatureWin]::SetWindowPos($t, [IntPtr]::Zero, 9000, 1000, 800, 600, 0x14) | Out-Null
          if ($t -ne $ctx.Hwnd) { [FeatureWin]::PostMessage($t, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
        }
      }
    }
    if ($onMonitor) { $shot = Get-Capture $ctx (Join-Path $shots 'link-new-tab-1.png'); return @('FAIL', $shot.Path, 'a window of the test appeared on a visible monitor') }
    Start-Sleep 3
    $left = @(Get-MyTops $ctx | Where-Object { $_ -ne $ctx.Hwnd -and $before -notcontains $_ })
    if (-not (Send-Flag $ctx 'rivet-tab.txt' 'list')) { return @('NOT_DRIVEN', $null, 'list was not taken') }
    $urls = Get-TabUrls
    $shot = Get-Capture $ctx (Join-Path $shots 'link-new-tab-1.png')
    if (-not ($urls | Where-Object { $_ -match 'page-c' }) -and -not $extraSeen) { return @('NOT_DRIVEN', $shot.Path, 'the click did not open the link') }
    if ($left.Count -gt 0) { foreach ($t in $left) { [FeatureWin]::PostMessage($t, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }; return @('FAIL', $shot.Path, 'the link stayed in a window of its own') }
    if (-not ($urls | Where-Object { $_ -match 'page-c' })) { return @('FAIL', $shot.Path, "page-c is not a tab of the Rivet window: $($urls -join ' ')") }
    @('PASS', $shot.Path, "the new-window link is a tab of the Rivet window; a window of its own showed first: $extraSeen")
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

# --- Run ---------------------------------------------------------------------------------
foreach ($name in $Browsers) {
  $exePath, $exeName = $table[$name]
  $version = if (Test-Path $exePath) { (Get-Item $exePath).VersionInfo.ProductVersion } else { 'not installed' }
  $shots = Join-Path $OutDir $name
  New-Item -ItemType Directory -Force $shots | Out-Null
  foreach ($feature in $Features) {
    if (-not $built) { Add-Row $name $version $feature 'NOT_DRIVEN' $null "no browser was started: $buildNote"; continue }
    if (-not (Test-Path $exePath)) { Add-Row $name $version $feature 'NOT_DRIVEN' $null 'the browser is not installed'; continue }
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
$bad = @($rows | Where-Object { $_.result -ne 'PASS' }).Count
if ($bad) { exit 1 }
