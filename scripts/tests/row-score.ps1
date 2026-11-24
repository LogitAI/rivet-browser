param(
    # rivet-rows.exe, which prints the rows Rivet knows.
    [Parameter(Mandatory = $true)][string]$Rows,
    # chrome-builds.exe, which holds the row finder.
    [Parameter(Mandatory = $true)][string]$Finder,
    # The checked-in table, whose rows count as verified like those built into rivet-rows.
    [Parameter(Mandatory = $true)][string]$Table,
    # Where the modules, rows and scores go. Removed and made again.
    [Parameter(Mandatory = $true)][string]$Work,
    # `browser@version` builds. Empty means the list below.
    [string[]]$Builds = @(),
    # Fetches each build with .github/scripts/rows/fetch-browser.ps1. Without it, each module is
    # taken from `$Modules\<browser>\<version>\<chrome.dll or msedge.dll>`.
    [switch]$Fetch,
    [string]$Modules = ''
)

$ErrorActionPreference = 'Stop'
# Scores the row finder against builds whose rows people verified. The finder reads only the build
# at hand, by the checked-in descriptions, and each value it finds is compared with the verified
# row of the same build. Per feature it reports hits, wrong values and values not found.
#
# A wrong value would reach people's browsers, so any fails this script. A value not found is not a
# failure: its feature stays off. A build that cannot be fetched is reported and skipped, and a run
# that scores no build fails, since it measured nothing. Nothing is published or proposed.
if (-not $Builds -or $Builds.Count -eq 0) {
    $Builds = @(
        'Google Chrome@156.0.8078.81',
        'Google Chrome@156.0.8078.82',
        'Google Chrome@157.0.8103.39',
        'Microsoft Edge@156.0.4314.58'
    )
}

if (Test-Path $Work) { Remove-Item -Recurse -Force $Work }
New-Item -ItemType Directory -Force $Work | Out-Null

function Find-BuildModule($browser, $version) {
    $dll = if ($browser -eq 'Microsoft Edge') { 'msedge.dll' } else { 'chrome.dll' }
    if (-not $Fetch) {
        $path = Join-Path $Modules "$browser\$version\$dll"
        if (-not (Test-Path $path)) { throw "no module at $path" }
        return $path
    }
    $out = Join-Path $Work ("browser-" + ($browser -replace ' ', '-') + "-$version")
    $found = Join-Path $Work ("browser-" + ($browser -replace ' ', '-') + "-$version.txt")
    if (-not (Test-Path $found)) {
        & "$PSScriptRoot/../../.github/scripts/rows/fetch-browser.ps1" -Browser $browser -Version $version -Out $out -Output $found | Out-Host
    }
    (Get-Content $found | Where-Object { $_ -like 'module=*' } | Select-Object -First 1) -replace '^module=', ''
}

function Get-VerifiedRow($browser, $version, $file) {
    & $Rows prior $browser $version --table $Table | Set-Content -Encoding utf8 $file
    if ($LASTEXITCODE -ne 0) { throw "rivet-rows has no row for $browser at or below $version" }
    $row = Get-Content -Raw $file | ConvertFrom-Json
    if ($row.frame.version -ne $version) { throw "$browser $version has no verified row; the nearest is $($row.frame.version)" }
}

$scored = 0
$wrong = 0
$failed = @()
foreach ($build in $Builds) {
    $browser, $version = $build -split '@', 2
    $slug = ($browser -replace ' ', '-') + "-$version"
    $verified = Join-Path $Work "$slug-verified.json"
    $score = Join-Path $Work "$slug-score.json"
    try {
        Get-VerifiedRow $browser $version $verified
        $module = Find-BuildModule $browser $version
    } catch {
        "skip ${browser} ${version}: $_"
        continue
    }
    & $Finder score $module $verified $score
    switch ($LASTEXITCODE) {
        0 { $scored++ }
        5 { $scored++; $wrong++ }
        default { $failed += "$browser $version (chrome-builds exited with $LASTEXITCODE)" }
    }
}

"scored $scored of $($Builds.Count) builds; $wrong with a wrong value"
if ($failed.Count -gt 0) { throw "NFR-6: the finder failed on: $($failed -join '; ')" }
if ($wrong -gt 0) { throw 'NFR-6: the finder found a value that differs from its verified row.' }
if ($scored -eq 0) { throw 'NFR-6: no build could be scored, so nothing was measured.' }
'NFR-6: every value the finder found equals the verified row.'
