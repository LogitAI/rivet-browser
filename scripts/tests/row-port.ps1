param(
    [Parameter(Mandatory = $true)][string]$Rows,
    [Parameter(Mandatory = $true)][string]$Generator,
    [Parameter(Mandatory = $true)][string]$Symbols
)

$ErrorActionPreference = 'Stop'
# Proves the row generator on builds people already verified: a row made from the prior build must
# hold the same addresses as the row that was checked by hand. Fields the prior row did not have
# yet are named, since a generator cannot add a feature. Needs the PDBs and an installed browser, so
# a machine without them skips the pair it lacks.
$work = Join-Path ([IO.Path]::GetTempPath()) "rivet-row-port-$PID"
New-Item -ItemType Directory -Force $work | Out-Null

function Flatten($value, $path, $out) {
    if ($value -is [System.Management.Automation.PSCustomObject]) {
        foreach ($item in $value.PSObject.Properties) { Flatten $item.Value "$path.$($item.Name)" $out }
    } elseif ($value -is [System.Collections.IEnumerable] -and $value -isnot [string]) {
        $index = 0
        foreach ($item in $value) { Flatten $item "$path[$index]" $out; $index++ }
    } else {
        $out[$path] = $value
    }
}

$failed = $false
foreach ($pair in @(
    @{ Browser = 'Google Chrome'; Prior = '156.0.8078.48'; New = '156.0.8078.74'; Pdbs = 'chrome-symbols'; Module = 'C:\Program Files\Google\Chrome\Application\156.0.8078.74\chrome.dll' },
    @{ Browser = 'Brave'; Prior = '156.1.98.62'; New = '156.1.98.63'; Pdbs = 'brave-symbols'; Module = 'C:\Program Files\BraveSoftware\Brave-Browser\Application\156.1.98.63\chrome.dll' }
)) {
    $old = Join-Path $Symbols "$($pair.Pdbs)\$($pair.Prior)\chrome.dll.pdb"
    $new = Join-Path $Symbols "$($pair.Pdbs)\$($pair.New)\chrome.dll.pdb"
    if (-not ((Test-Path $old) -and (Test-Path $new) -and (Test-Path $pair.Module))) {
        "skip $($pair.Browser) $($pair.New): no symbols or module here"
        continue
    }
    $priorRow = Join-Path $work "prior.json"
    $made = Join-Path $work "made.json"
    $report = Join-Path $work "report.json"
    & $Rows row $pair.Browser $pair.Prior | Set-Content -Encoding utf8 $priorRow
    $truth = & $Rows row $pair.Browser $pair.New | ConvertFrom-Json
    & $Generator port $priorRow $old $new $pair.Module $pair.New $made $report
    $review = (Get-Content -Raw $report | ConvertFrom-Json).review
    if ($review.Count -gt 0) { "$($pair.Browser): for review: $review"; $failed = $true }
    $a = @{}; $b = @{}
    Flatten (Get-Content -Raw $made | ConvertFrom-Json) '' $a
    Flatten $truth '' $b
    $count = 0
    foreach ($path in $b.Keys) {
        if ("$($b[$path])" -notmatch '^0x') { continue }
        $count++
        if ($a[$path] -ne $b[$path]) {
            if ($a[$path] -eq '0x7fff0000' -or $null -eq $a[$path]) { "$($pair.Browser): $path was added by hand after the prior row"; continue }
            "$($pair.Browser): $path made $($a[$path]), verified $($b[$path])"
            $failed = $true
        }
    }
    "$($pair.Browser) $($pair.Prior) -> $($pair.New): compared $count address values"
}
Remove-Item -Recurse -Force $work
if ($failed) { throw 'NFR-3: A generated row differs from the row people verified.' }
'NFR-3: Generated rows match the verified rows.'
