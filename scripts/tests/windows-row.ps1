param(
    [Parameter(Mandatory = $true)][string]$Image,
    [Parameter(Mandatory = $true)][int]$Dpi
)

$ErrorActionPreference = 'Stop'
# Measures the top row of a captured Rivet window: the height down to its hairline and where its
# buttons sit. A generated row reaches people only when these match the design, since no symbol
# shows a member offset, a colour id or a layout constant to be right.
Add-Type -AssemblyName System.Drawing
$scale = $Dpi / 96.0
$bitmap = [Drawing.Bitmap]::FromFile((Resolve-Path $Image).Path)
try {
    function Near($pixel, $r, $g, $b, $tolerance) {
        [math]::Abs($pixel.R - $r) -le $tolerance -and [math]::Abs($pixel.G - $g) -le $tolerance -and [math]::Abs($pixel.B - $b) -le $tolerance
    }
    # The page is the large white area, and the panel's left edge is where it starts.
    $middle = [int]($bitmap.Height / 2)
    $left = -1
    for ($x = 0; $x -lt $bitmap.Width; $x++) {
        if (Near $bitmap.GetPixel($x, $middle) 255 255 255 2) { $left = $x; break }
    }
    if ($left -lt 0) { throw 'NFR-3: The capture shows no page.' }
    # Down the middle of the panel: the hairline is the first run with the design's rule colour,
    # and the row starts where the row's paper does.
    $column = $left + [int]($bitmap.Width - $left) / 2
    $hairline = -1
    for ($y = 0; $y -lt [int](120 * $scale); $y++) {
        if (Near $bitmap.GetPixel($column, $y) 233 233 230 4) { $hairline = $y; break }
    }
    if ($hairline -lt 0) { throw 'NFR-3: The capture shows no hairline below the row.' }
    $paper = $bitmap.GetPixel($column, $hairline - [int](4 * $scale))
    # Some browsers blend one pixel above the rule, so the walk starts on the paper itself.
    $top = $hairline - [int](4 * $scale)
    while ($top -gt 0 -and (Near $bitmap.GetPixel($column, $top - 1) $paper.R $paper.G $paper.B 3)) { $top-- }
    $height = ($hairline + 1 - $top) / $scale
    # Across the row: runs of pixels unlike the paper are the icons. A gap of 4 DIPs ends a run.
    $middleRow = [int](($top + $hairline) / 2)
    $runs = [Collections.Generic.List[object]]::new()
    $start = -1; $last = -1
    for ($x = $left; $x -lt $left + [int](240 * $scale); $x++) {
        $ink = $false
        for ($y = $middleRow - [int](8 * $scale); $y -le $middleRow + [int](8 * $scale); $y++) {
            if (-not (Near $bitmap.GetPixel($x, $y) $paper.R $paper.G $paper.B 40)) { $ink = $true; break }
        }
        if ($ink) {
            if ($start -lt 0 -or $x - $last -gt 4 * $scale) {
                if ($start -ge 0) { $runs.Add([math]::Round((($start + $last) / 2 - $left) / $scale, 1)) }
                $start = $x
            }
            $last = $x
        }
    }
    if ($start -ge 0) { $runs.Add([math]::Round((($start + $last) / 2 - $left) / $scale, 1)) }
} finally {
    $bitmap.Dispose()
}
"row height $height DIP, icon centres from the panel's left edge $($runs -join ', ') DIP"
if ([math]::Abs($height - 44) -gt 1) { throw "NFR-3: The row is $height DIPs high, not 44." }
# The toggle, back, forward and reload buttons of the design.
$design = 22, 51, 81, 111
if ($runs.Count -lt $design.Count) { throw "NFR-3: The row shows $($runs.Count) icons." }
for ($index = 0; $index -lt $design.Count; $index++) {
    if ([math]::Abs($runs[$index] - $design[$index]) -gt 2.5) {
        throw "NFR-3: Icon $index sits at $($runs[$index]) DIPs, not $($design[$index])."
    }
}
'NFR-3: The row matches the design.'
