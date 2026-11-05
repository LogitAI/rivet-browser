# Held build contract for the Rivet launcher. When the installed browser's build has no usable row,
# a Rivet window opens from the build Rivet kept, shows the security warning, and stops opening
# after seven days. A plain browser window never appears in between.
#
#   pwsh -NoProfile -File scripts/tests/windows-held-build.ps1 [-Browser chrome] [-OutDir <dir>] [-NoBuild]
#
# The installed browser is simulated by a folder with a launcher stub and a module whose version
# no row knows. Rivet only reads that folder and then opens the kept copy, so the real install and
# the browsers running on this machine are never started, stopped or changed. The kept copy is made
# by `rivet hold refresh` from the real install, which only reads it.
#
# Safety: the test window sits off-screen and is captured with PrintWindow. No window is activated
# and the cursor never moves. The profile is fresh inside this checkout with sync and sign-in off.
# Only processes whose command line holds that profile are stopped. A browser starts only when the
# build exited 0.
param(
  [string]$Browser = 'chrome',
  [string]$OutDir,
  [switch]$NoBuild
)
$ErrorActionPreference = 'Continue'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$root = Join-Path $repo 'rivet'
$debug = Join-Path $root 'target\debug'
if (-not $OutDir) { $OutDir = Join-Path $repo '.scratch\held' }
New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
# Cargo comes from the caller's toolchain: CARGO_HOME when it is set, else cargo on the PATH.
$cargo = if ($env:CARGO_HOME -and (Test-Path (Join-Path $env:CARGO_HOME 'bin\cargo.exe'))) { Join-Path $env:CARGO_HOME 'bin\cargo.exe' } else { 'cargo' }

$installs = @{
  chrome = @('C:\Program Files\Google\Chrome\Application\chrome.exe', 'chrome.exe')
  edge   = @('C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe', 'msedge.exe')
}
$realExe, $exeName = $installs[$Browser]
$key = [IO.Path]::GetFileNameWithoutExtension($exeName)

. (Join-Path $PSScriptRoot 'windows-processes.ps1')
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class HeldWin{
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int ht,uint f);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr GetProp(IntPtr h,string n);
 public struct R{public int L,T,Ri,B;}
}
'@
[HeldWin]::SetThreadDpiAwarenessContext([IntPtr]-4) | Out-Null

$results = New-Object System.Collections.ArrayList
function Add-Result([string]$Name, [bool]$Pass, [string]$Detail) {
  [void]$results.Add([pscustomobject]@{ name = $Name; pass = $Pass; detail = $Detail })
  "{0} {1}: {2}" -f $(if ($Pass) { 'PASS' } else { 'FAIL' }), $Name, $Detail | Write-Host
}

function Stop-Mine([string]$Profile) {
  Get-ProcessList | Where-Object { $_.CommandLine -like "*$Profile*" -and $_.ProcessId -ne $PID } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

# --- Build, gated on the exit code ------------------------------------------------------------
$built = $false
if ($NoBuild) {
  $built = (Test-Path (Join-Path $debug 'rivet.exe')) -and (Test-Path (Join-Path $debug 'rivet_runtime_windows.dll'))
  if ($built -and -not ([Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes((Join-Path $debug 'rivet.exe'))).Contains('--window-position=9000,1000'))) {
    $built = $false; Write-Host 'rivet.exe was not built with the off-screen test flags'
  }
} else {
  # The window flags exist for tests only: off-screen, no sync, no sign-in, and no occlusion tracking
  # so that an off-screen page still paints. They are the same flags windows-features.ps1 uses. The
  # launcher file is restored right after the build and must never be committed.
  $f = Join-Path $root 'core\src\lib.rs'
  $orig = [IO.File]::ReadAllText($f)
  $flags = @('--disable-sync', '--no-first-run', '--no-default-browser-check', '--disable-features=msImplicitSignin,msEdgeSignIn,CalculateNativeWinOcclusion', '--window-position=9000,1000', '--window-size=1600,1000')
  $push = ($flags | ForEach-Object { "arguments.push(`"$_`".to_owned());" }) -join ' '
  [IO.File]::WriteAllText($f, $orig.Replace('arguments.push(request.url.clone());', "$push arguments.push(request.url.clone());"))
  Push-Location $root
  try {
    $output = & $cargo build -q 2>&1
    $code = $LASTEXITCODE
    if ($code -ne 0) { $output | Select-Object -Last 40 | ForEach-Object { Write-Host $_ } }
  } finally { Pop-Location; [IO.File]::WriteAllText($f, $orig) }
  $built = ($code -eq 0)
}
if (-not $built) { Write-Host 'NOT_DRIVEN: the build did not succeed, so no browser was started'; exit 3 }
$rivet = Join-Path $debug 'rivet.exe'
$runtime = Join-Path $debug 'rivet_runtime_windows.dll'

# --- The simulated installed browser: an unknown build ----------------------------------------
$simulated = Join-Path $OutDir 'installed'
$hold = Join-Path $OutDir 'hold\browsers'
$profile_ = Join-Path $OutDir "profile-$Browser"
Stop-Mine $profile_
Remove-Item -Recurse -Force $simulated, (Join-Path $OutDir 'hold'), $profile_ -ErrorAction SilentlyContinue
$inner = Join-Path $simulated 'Application\10.0.0.1'
New-Item -ItemType Directory -Force $inner, $profile_ | Out-Null
Set-Content (Join-Path $simulated "Application\$exeName") 'stub'
$module = if ($Browser -eq 'edge') { 'msedge.dll' } else { 'chrome.dll' }
Copy-Item (Join-Path $env:SystemRoot 'System32\kernel32.dll') (Join-Path $inner $module)
$fake = Join-Path $simulated "Application\$exeName"
Write-Host ("simulated installed build {0}" -f (Get-Item (Join-Path $inner $module)).VersionInfo.FileVersion)

# --- Keep the real build, as a launch of the verified build does ------------------------------
$sw = [Diagnostics.Stopwatch]::StartNew()
& $rivet hold refresh --browser $realExe --hold $hold | ForEach-Object { Write-Host "refresh: $_" }
$copyCode = $LASTEXITCODE
$kept = Get-ChildItem (Join-Path $hold $key) -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName 'rivet-held.json') }
Add-Result 'a verified build is kept' (($copyCode -eq 0) -and $kept) ("{0} s, {1}" -f [int]$sw.Elapsed.TotalSeconds, ($kept.Name -join ','))
if (-not $kept) { exit 1 }
$bytes = (Get-ChildItem $kept[0].FullName -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ("kept copy size: {0:N0} MB" -f ($bytes / 1MB))
$privateNames = Get-ChildItem $kept[0].FullName -Recurse -Force | Where-Object { $_.Name -in 'Cookies', 'Login Data', 'Local State', 'Web Data', 'History', 'Preferences', 'User Data', 'Default' }
Add-Result 'the kept copy holds no profile data' (-not $privateNames) (($privateNames | ForEach-Object { $_.FullName }) -join ';')

# --- Open a Rivet window while the installed build is unknown ---------------------------------
$page = Join-Path $OutDir 'page.html'
[IO.File]::WriteAllText($page, '<!doctype html><meta charset=utf-8><title>held</title><style>html,body{margin:0;height:100%;background:#2d7ff0}</style>')
$url = 'file:///' + ($page -replace '\\', '/')
$state = Join-Path $OutDir 'state.json'
Remove-Item $state -ErrorAction SilentlyContinue
$sessionLine = $null
foreach ($try in 1..3) {
  $lines = & $rivet session start --browser $fake --url $url --user-data-dir $profile_ --runtime $runtime --state $state --hold $hold --display visible --json 2>&1 | ForEach-Object { [string]$_ }
  $sessionLine = $lines | Where-Object { $_.Contains('runtime_loaded') } | Select-Object -First 1
  if ($sessionLine) { break }
  Start-Sleep 2
}
Add-Result 'the session reports the held build' ($sessionLine -and $sessionLine.Contains('"held_version":"156.')) "$sessionLine"
$hwnd = [IntPtr]::Zero; $attached = $false; $mine = $null
$deadline = (Get-Date).AddSeconds(45)
while ((Get-Date) -lt $deadline -and $sessionLine) {
  $mine = Get-ProcessList $exeName | Where-Object { $_.CommandLine -like "*$profile_*" -and $_.CommandLine -notlike '*--type=*' } | Select-Object -First 1
  if (-not $mine) { Start-Sleep 1; continue }
  $h = (Get-Process -Id $mine.ProcessId -ErrorAction SilentlyContinue).MainWindowHandle
  if ($h -and $h -ne [IntPtr]::Zero) {
    $hwnd = $h
    if ([HeldWin]::GetProp($h, 'Rivet.OriginalWindowProc') -ne [IntPtr]::Zero) { $attached = $true; break }
  }
  Start-Sleep 1
}
Add-Result 'the window opened from the kept copy' ($mine -and $mine.ExecutablePath.StartsWith($hold, [StringComparison]::OrdinalIgnoreCase)) "$($mine.ExecutablePath)"
Add-Result 'Rivet attached to the window' $attached 'Rivet.OriginalWindowProc is set on the browser window'
if ($mine) { Write-Host "command line: $($mine.CommandLine)" }
if ($attached) {
  [HeldWin]::SetWindowPos($hwnd, [IntPtr]::Zero, 9000, 1000, 1800, 1067, 0x14) | Out-Null
  [HeldWin]::PostMessage($hwnd, 0x86, [IntPtr]1, [IntPtr]0) | Out-Null
  Start-Sleep 8
  $r = New-Object HeldWin+R
  [HeldWin]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  $w = $r.Ri - $r.L; $h = $r.B - $r.T
  $bmp = New-Object Drawing.Bitmap $w, $h, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [HeldWin]::PrintWindow($hwnd, $dc, 2) | Out-Null
  $g.ReleaseHdc($dc); $g.Dispose()
  $png = Join-Path $OutDir "held-$Browser.png"
  $bmp.Save($png)
  $data = $bmp.LockBits((New-Object Drawing.Rectangle 0, 0, $w, $h), [Drawing.Imaging.ImageLockMode]::ReadOnly, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $px = New-Object 'int[]' ($w * $h)
  [Runtime.InteropServices.Marshal]::Copy($data.Scan0, $px, 0, $px.Length)
  $bmp.UnlockBits($data); $bmp.Dispose()
  # The warning's ground is #FDEAB8 and sits in the lower right corner. The page is #2D7FF0.
  $ground = 0xFFFDEAB8 -as [int]
  $warning = 0; $page_ = 0
  for ($i = 0; $i -lt $px.Length; $i++) {
    if ($px[$i] -eq $ground) { $warning++ } elseif ($px[$i] -eq (0xFF2D7FF0 -as [int])) { $page_++ }
  }
  Add-Result 'the warning is on the window' ($warning -gt 3000) "$warning px of the warning ground, capture $png"
  Add-Result 'the page below it is drawn' ($page_ -gt ($w * $h / 4)) "$page_ px of the page colour"
}

# The window belongs to the kept copy and to no other browser window. Nothing else was started.
$others = Get-ProcessList | Where-Object { $_.CommandLine -like "*$profile_*" -and $_.ExecutablePath -and -not $_.ExecutablePath.StartsWith($hold, [StringComparison]::OrdinalIgnoreCase) -and $_.ProcessId -ne $PID }
Add-Result 'every process on the test profile runs from the kept copy' (-not $others) (($others | ForEach-Object { $_.ExecutablePath }) -join ';')
Stop-Mine $profile_
Start-Sleep 2

# --- After seven days: nothing opens and the reason is said -----------------------------------
$clockFile = Join-Path $hold "$key\hold-state.json"
$clock = Get-Content $clockFile -Raw | ConvertFrom-Json
$old = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - 8 * 86400
$clock.first_detected = $old
[IO.File]::WriteAllText($clockFile, ($clock | ConvertTo-Json -Compress))
$before = @(Get-ProcessList | Where-Object { $_.CommandLine -like "*$profile_*" }).Count
$text = & $rivet session start --browser $fake --url $url --user-data-dir $profile_ --runtime $runtime --state $state --hold $hold --display visible 2>&1 | ForEach-Object { [string]$_ }
$exit = $LASTEXITCODE
Start-Sleep 3
$after = @(Get-ProcessList | Where-Object { $_.CommandLine -like "*$profile_*" }).Count
Add-Result 'after seven days no window opens and the reason is said' (($exit -ne 0) -and ($after -eq $before) -and ($text -join ' ').Contains('no longer opens new windows')) (($text | Select-Object -First 1) -join '')

# --- Never kept: nothing opens either ---------------------------------------------------------
Remove-Item -Recurse -Force (Join-Path $OutDir 'hold') -ErrorAction SilentlyContinue
$text = & $rivet session start --browser $fake --url $url --user-data-dir $profile_ --runtime $runtime --state $state --hold $hold --display visible 2>&1 | ForEach-Object { [string]$_ }
$exit = $LASTEXITCODE
Start-Sleep 2
$after = @(Get-ProcessList | Where-Object { $_.CommandLine -like "*$profile_*" }).Count
Add-Result 'with nothing kept no window opens and the reason is said' (($exit -ne 0) -and ($after -eq 0) -and ($text -join ' ').Contains('no verified copy')) (($text | Select-Object -First 1) -join '')

Stop-Mine $profile_
$failed = @($results | Where-Object { -not $_.pass }).Count
if ($failed) { Write-Host "$failed check(s) failed"; exit 1 }
Write-Host 'all held build checks passed'
