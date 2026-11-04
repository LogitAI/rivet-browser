param(
    [Parameter(Mandatory = $true)][string]$Browser,
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Channel = 'stable',
    [string]$Download = '',
    [string]$Out = '',
    [string]$Output = '',
    # Prints the installer's address without fetching it.
    [switch]$ResolveOnly,
    [int]$Tries = 60
)

$ErrorActionPreference = 'Stop'
# Gets one exact browser build without touching what is installed: its files are unpacked into a
# folder, since a runner's own browser may be newer and would refuse a lower version.

# Asks Google's public update service for an exact version. A pinned request reaches what everyone
# is offered, while a build in a rollout goes to a random group of ordinary update checks, so those
# are repeated until one is offered it. Few enough tries stay polite, and a build in a very small
# group waits for a later run.
function Resolve-Chrome {
    $app = '{8A69D345-D564-463C-AFF1-A69D9E530F96}'
    $ap = 'x64-stable-statsdef_1'
    if ($Channel -eq 'beta') { $app = '{8237E44A-0054-442C-B6B6-EA0509993955}'; $ap = 'x64-beta-statsdef_1' }
    $ask = {
        param($source, $installed, $check)
        $request = @"
<?xml version="1.0" encoding="UTF-8"?><request protocol="3.0" updater="Omaha" updaterversion="1.3.36.372" shell_version="1.3.36.372" ismachine="1" installsource="$source" dedup="cr"><hw sse2="1" sse3="1" ssse3="1" sse41="1" sse42="1" avx="1" physmemory="16"/><os platform="win" version="10.0.26100.0" arch="x64"/><app appid="$app" ap="$ap" version="$installed" lang="en" brand="GCEA"><updatecheck $check/></app></request>
"@
        ([xml](Invoke-WebRequest -Method Post -ContentType 'application/xml' -Body $request -Uri 'https://tools.google.com/service/update2' -UseBasicParsing).Content).response.app.updatecheck
    }
    $answer = & $ask 'ondemand' '' "targetversionprefix=`"$Version`""
    # An installed version of the previous milestone is one every rollout group may update from.
    $parts = $Version -split '\.'
    $earlier = "$([int]$parts[0] - 1).0.0.0"
    for ($try = 0; $try -lt $Tries -and $answer.manifest.version -ne $Version; $try++) {
        Start-Sleep -Milliseconds 200
        $answer = & $ask 'scheduler' $earlier ''
    }
    if ($answer.manifest.version -ne $Version) {
        throw "Google's update service did not offer Chrome $Version in $Tries tries. It may be in a very small rollout group."
    }
    $base = @($answer.urls.url.codebase) | Where-Object { $_ -like 'https://dl.google.com/*' } | Select-Object -First 1
    "$base$($answer.manifest.packages.package.name)"
}

$url = switch ($Browser) {
    'Google Chrome' { Resolve-Chrome }
    default {
        if (-not $Download) { throw "The feed names no installer for $Browser $Version yet." }
        $Download
    }
}
if ($ResolveOnly) { $url; return }

New-Item -ItemType Directory -Force $Out | Out-Null
$package = Join-Path $Out (Split-Path $url -Leaf)
curl.exe -fsSL -o $package $url
if ($LASTEXITCODE -ne 0) { throw "Could not download $url" }
$unpacked = Join-Path $Out 'files'
if ($Browser -eq 'Microsoft Edge') {
    Start-Process msiexec.exe -Wait -ArgumentList @('/a', "`"$package`"", '/qn', "TARGETDIR=`"$unpacked`"")
} else {
    7z x $package "-o$unpacked" -y | Out-Null
}
# Installers keep the browser in an archive within the archive.
foreach ($inner in Get-ChildItem $unpacked -Recurse -File | Where-Object Extension -in '.7z', '.xz') {
    7z x $inner.FullName "-o$unpacked" -y | Out-Null
}
$moduleName = if ($Browser -eq 'Microsoft Edge') { 'msedge.dll' } else { 'chrome.dll' }
$executableName = @{ 'Google Chrome' = 'chrome.exe'; 'Microsoft Edge' = 'msedge.exe'; 'Brave' = 'brave.exe' }[$Browser]
$module = Get-ChildItem $unpacked -Recurse -File -Filter $moduleName | Where-Object { $_.Directory.Name -eq $Version } | Select-Object -First 1
if (-not $module) { throw "$moduleName of version $Version is not in the unpacked files of $url" }
$executable = Join-Path $module.Directory.Parent.FullName $executableName
if (-not (Test-Path $executable)) { throw "$executableName is not beside the version folder" }
if ($module.VersionInfo.FileVersion -notlike "*$Version*" -and (($module.VersionInfo.FileMajorPart, $module.VersionInfo.FileMinorPart, $module.VersionInfo.FileBuildPart, $module.VersionInfo.FilePrivatePart) -join '.') -ne $Version) {
    throw "$($module.FullName) reports version $($module.VersionInfo.FileVersion), not $Version"
}
if ($Output) {
    "module=$($module.FullName)" >> $Output
    "executable=$executable" >> $Output
}
"$Browser $Version is at $($module.FullName)"
