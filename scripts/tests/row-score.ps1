param(
    # rivet-rows.exe, which prints the rows Rivet knows.
    [Parameter(Mandatory = $true)][string]$Rows,
    # chrome-builds.exe, the row generator.
    [Parameter(Mandatory = $true)][string]$Generator,
    # The checked-in table, whose rows count as verified like those built into rivet-rows.
    [Parameter(Mandatory = $true)][string]$Table,
    # Where the modules, rows and scores go. Removed and made again.
    [Parameter(Mandatory = $true)][string]$Work,
    # `browser@prior>new` pairs. Empty means the list below.
    [string[]]$Pairs = @(),
    # Fetches each build with .github/scripts/rows/fetch-browser.ps1. Without it, each module is
    # taken from `$Modules\<browser>\<version>\<chrome.dll or msedge.dll>`.
    [switch]$Fetch,
    [string]$Modules = ''
)

$ErrorActionPreference = 'Stop'
# Scores the row generator against answer keys: builds whose rows people verified, made from
# public symbols before Rivet stopped reading them. Each new row is made without symbols from the
# verified row of an older build, the way the row job makes rows, and compared value by value with
# the verified row of the new build. Per feature it reports values found and equal, values found
# but different, and features the made row turns off because their values were not found.
#
# A value found but different is a wrong value, which would reach people's browsers, so any fails
# this script. A feature turned off is not a failure: the runtime keeps it off. A pair whose builds
# cannot be fetched is reported and skipped, and a run that scores no pair fails, since it measured
# nothing. Nothing is published or proposed.
#
# A build newer than every verified row, such as Chrome 158 while the newest verified row is 157,
# has no answer key and cannot be scored here.
if (-not $Pairs -or $Pairs.Count -eq 0) {
    $Pairs = @(
        'Google Chrome@156.0.8078.82>157.0.8103.38',
        'Google Chrome@156.0.8078.82>157.0.8103.39',
        'Google Chrome@156.0.8078.74>156.0.8078.80',
        'Microsoft Edge@156.0.4314.50>156.0.4314.58',
        'Brave@156.1.98.63>157.1.99.55'
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
foreach ($pair in $Pairs) {
    $browser, $versions = $pair -split '@', 2
    $prior, $new = $versions -split '>', 2
    $slug = ($browser -replace ' ', '-') + "-$prior-to-$new"
    $priorRow = Join-Path $Work "$slug-prior.json"
    $verified = Join-Path $Work "$slug-verified.json"
    $score = Join-Path $Work "$slug-score.json"
    try {
        Get-VerifiedRow $browser $prior $priorRow
        Get-VerifiedRow $browser $new $verified
        $priorModule = Find-BuildModule $browser $prior
        $module = Find-BuildModule $browser $new
    } catch {
        "skip ${browser} $prior -> ${new}: $_"
        continue
    }
    & $Generator score $priorRow $priorModule $module $verified $score
    switch ($LASTEXITCODE) {
        0 { $scored++ }
        5 { $scored++; $wrong++ }
        default { $failed += "$browser $prior -> $new (chrome-builds exited with $LASTEXITCODE)" }
    }
}

"scored $scored of $($Pairs.Count) pairs; $wrong with a wrong value"
if ($failed.Count -gt 0) { throw "NFR-6: the generator failed on: $($failed -join '; ')" }
if ($wrong -gt 0) { throw 'NFR-6: a row made without symbols holds a value that differs from its verified row.' }
if ($scored -eq 0) { throw 'NFR-6: no pair could be scored, so nothing was measured.' }
'NFR-6: every value the generator found equals the verified row.'
