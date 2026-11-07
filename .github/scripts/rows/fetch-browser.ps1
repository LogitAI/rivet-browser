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

# Edge's feed offers only an MSI, which wraps an update client whose payload 7-Zip cannot unpack. Edge's
# own update service also lists the full installer of each exact version, and that one is a plain
# Chromium installer with the browser in an archive.
function Resolve-Edge {
    $names = 'https://msedge.api.cdp.microsoft.com/api/v1.1/internal/contents/Browser/namespaces/Default/names'
    $body = '{"targetingAttributes":{"IsInternalUser":false,"Updater":"MicrosoftEdgeUpdate","UpdaterVersion":"1.3.195.31"}}'
    $files = Invoke-RestMethod -Method Post -ContentType 'application/json' -Body $body `
        -Uri "$names/msedge-$Channel-win-x64/versions/$Version/files?action=GenerateDownloadInfo&foregroundPriority=true"
    $full = @($files) | Where-Object FileId -eq "MicrosoftEdge_X64_$Version.exe" | Select-Object -First 1
    if (-not $full) { throw "Edge's update service lists no full installer for $Version yet." }
    $full.Url
}

$url = switch ($Browser) {
    'Google Chrome' { Resolve-Chrome }
    'Microsoft Edge' { Resolve-Edge }
    default {
        if (-not $Download) { throw "The feed names no installer for $Browser $Version yet." }
        $Download
    }
}
if ($ResolveOnly) { $url; return }

New-Item -ItemType Directory -Force $Out | Out-Null
# Edge's address names a file by its id and carries a query, so the package gets a plain name.
$leaf = if ($Browser -eq 'Microsoft Edge') { "MicrosoftEdge_X64_$Version.exe" } else { Split-Path ([uri]$url).AbsolutePath -Leaf }
$package = Join-Path $Out $leaf
curl.exe -fsSL -o $package $url
if ($LASTEXITCODE -ne 0) { throw "Could not download $url" }
$unpacked = Join-Path $Out 'files'
if ($Browser -eq 'Brave') {
    # Brave publishes only update-client installers. Their payload is split into several branch
    # streams that 7-Zip reads as one, so the files come out damaged. A runner is thrown away after
    # the job and has no Brave of its own, so the silent installer installs this exact build.
    $installed = Join-Path $env:ProgramFiles 'BraveSoftware\Brave-Browser\Application'
    if (Test-Path $installed) { throw "This runner already has Brave at $installed, so the build would not be the one asked for." }
    $setup = Start-Process $package -Wait -PassThru
    if ($setup.ExitCode -ne 0) { throw "Brave's installer exited with $($setup.ExitCode)" }
    $unpacked = $installed
} else {
    7z x $package "-o$unpacked" -y | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "7-Zip could not unpack $url" }
    # Installers keep the browser in an archive within the archive.
    foreach ($inner in Get-ChildItem $unpacked -Recurse -File | Where-Object Extension -in '.7z', '.xz') {
        7z x $inner.FullName "-o$unpacked" -y | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "7-Zip could not unpack $($inner.Name) from $url" }
    }
}
$moduleName = if ($Browser -eq 'Microsoft Edge') { 'msedge.dll' } else { 'chrome.dll' }
$executableName = @{ 'Google Chrome' = 'chrome.exe'; 'Microsoft Edge' = 'msedge.exe'; 'Brave' = 'brave.exe' }[$Browser]
$module = Get-ChildItem $unpacked -Recurse -File -Filter $moduleName | Where-Object { $_.Directory.Name -eq $Version } | Select-Object -First 1
if (-not $module) { throw "$moduleName of version $Version is not in the unpacked files of $url" }
$executable = Join-Path $module.Directory.Parent.FullName $executableName
# Edge's full installer keeps its executable in the version folder, and its setup copies it up beside
# the folder when it installs. The copy here gives the same layout as an installed Edge.
$versioned = Join-Path $module.Directory.FullName $executableName
if (-not (Test-Path $executable) -and (Test-Path $versioned)) { Copy-Item $versioned $executable }
if (-not (Test-Path $executable)) { throw "$executableName is not beside the version folder" }
if ($module.VersionInfo.FileVersion -notlike "*$Version*" -and (($module.VersionInfo.FileMajorPart, $module.VersionInfo.FileMinorPart, $module.VersionInfo.FileBuildPart, $module.VersionInfo.FilePrivatePart) -join '.') -ne $Version) {
    throw "$($module.FullName) reports version $($module.VersionInfo.FileVersion), not $Version"
}
if ($Output) {
    "module=$($module.FullName)" >> $Output
    "executable=$executable" >> $Output
}
"$Browser $Version is at $($module.FullName)"
