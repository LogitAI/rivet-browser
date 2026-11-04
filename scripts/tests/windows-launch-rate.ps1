# Starts each browser repeatedly through `rivet session start` and records how often the runtime is
# reported loaded and attached, with the raw output of every failure. A plain window after launch
# is a failure of the product, so the rate matters more than any single run.
#
#   pwsh -NoProfile -File scripts/tests/windows-launch-rate.ps1 [-Browsers whale,brave] [-Runs 8] [-Seed]
#
# Uses the binaries of the last build, which carry the off-screen test flags, and refuses to run
# when they do not. Fresh test profiles inside this checkout, only their processes are stopped.
param(
  [string[]]$Browsers = @('whale', 'brave'),
  [int]$Runs = 8,
  [switch]$Seed,
  [int]$Gap = 1,
  [string]$Url = 'about:blank',
  [string]$OutDir
)
$Browsers = @($Browsers | ForEach-Object { $_ -split ',' })
. (Join-Path $PSScriptRoot 'windows-features-lib.ps1')
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$debug = Join-Path $repo 'rivet\target\debug'
if (-not $OutDir) { $OutDir = Join-Path $repo '.scratch\launch-rate' }
New-Item -ItemType Directory -Force $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
$rivet = Join-Path $debug 'rivet.exe'
$dll = Join-Path $debug 'rivet_runtime_windows.dll'
if (-not ([Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($rivet)).Contains('--window-position=9000,1000'))) {
  Write-Host 'refusing to launch: this rivet.exe was not built with the off-screen test flags'; exit 2
}
$table = @{
  chrome = @('C:\Program Files\Google\Chrome\Application\chrome.exe', 'chrome.exe')
  edge = @('C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe', 'msedge.exe')
  whale = @('C:\Program Files\Naver\Naver Whale\Application\whale.exe', 'whale.exe')
  brave = @('C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe', 'brave.exe')
  samsung = @('C:\Program Files\Samsung\Internet\Application\samsunginternet.exe', 'samsunginternet.exe')
}
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class LaunchWin{ [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr GetProp(IntPtr h,string n); }
'@
$report = @()
foreach ($name in $Browsers) {
  $exe, $exeName = $table[$name]
  $ok = 0
  foreach ($run in 1..$Runs) {
    $profile_ = Join-Path $OutDir "p-$name-$run"
    Get-CimInstance Win32_Process -Filter "name='$exeName'" | Where-Object { $_.CommandLine -like "*$OutDir*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep $Gap
    Remove-Item -Recurse -Force $profile_ -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $profile_ | Out-Null
    $state = Join-Path $OutDir "state-$name-$run.json"
    Remove-Item $state -ErrorAction SilentlyContinue
    $seedFile = if ($Seed) { Write-SpaceSeed $profile_ $name } else { $null }
    $started = Get-Date
    $out = & $rivet session start --browser $exe --url $Url --user-data-dir $profile_ --runtime $dll --state $state --json 2>&1 | ForEach-Object { [string]$_ }
    $reported = [bool]($out | Where-Object { $_.Contains('runtime_loaded') })
    $attached = $false; $alive = $false
    foreach ($i in 1..25) {
      $mine = Get-CimInstance Win32_Process -Filter "name='$exeName'" | Where-Object { $_.CommandLine -like "*$profile_*" -and $_.CommandLine -notlike '*--type=*' } | Select-Object -First 1
      if ($mine) {
        $alive = $true
        $h = (Get-Process -Id $mine.ProcessId -ErrorAction SilentlyContinue).MainWindowHandle
        if ($h -and $h -ne [IntPtr]::Zero -and [LaunchWin]::GetProp($h, 'Rivet.OriginalWindowProc') -ne [IntPtr]::Zero) { $attached = $true; break }
      }
      Start-Sleep 1
    }
    if ($reported -and $attached) { $ok++ }
    $row = [pscustomobject]@{ browser = $name; run = $run; reported = $reported; alive = $alive; attached = $attached; seconds = [int]((Get-Date) - $started).TotalSeconds; output = ($out -join ' | ') }
    $report += $row
    '{0} run {1}: reported={2} alive={3} attached={4} {5}' -f $name, $run, $reported, $alive, $attached, $(if (-not $reported) { $row.output } else { '' }) | Write-Host
    Get-CimInstance Win32_Process -Filter "name='$exeName'" | Where-Object { $_.CommandLine -like "*$profile_*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  }
  "{0}: {1} of {2} launches reported the runtime loaded and attached" -f $name, $ok, $Runs | Write-Host
}
$report | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $OutDir 'launch-rate.json') -Encoding utf8
