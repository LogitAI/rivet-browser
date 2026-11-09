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
    # The page is the large white area. The panel is the window's own surface: the sidebar and the
    # row above the page. A capture can hold a margin of the desktop around the window, and the
    # panel's left edge is where that margin ends.
    $middle = [int]($bitmap.Height / 2)
    $page = -1
    for ($x = 0; $x -lt $bitmap.Width; $x++) {
        if (Near $bitmap.GetPixel($x, $middle) 255 255 255 2) { $page = $x; break }
    }
    if ($page -lt 0) { throw 'NFR-3: The capture shows no page.' }
    $margin = $bitmap.GetPixel(0, $middle)
    $left = 0
    while ($left -lt $page -and (Near $bitmap.GetPixel($left, $middle) $margin.R $margin.G $margin.B 3)) { $left++ }
    if ($left -ge $page) { $left = 0 }    # Up the middle of the panel from the empty page: the hairline is the first thing above it. Going
    # down instead would meet the window's shadow and, in a narrow window, the address box. The start
    # is above any bar a browser puts at the top of the page, and a taller row fails the check.
    $column = $page + [int]($bitmap.Width - $page) / 2
    $hairline = [int](70 * $scale)
    while ($hairline -gt 0 -and (Near $bitmap.GetPixel($column, $hairline) 255 255 255 2)) { $hairline-- }
    # The page's own edge can blend one pixel below the hairline, so the hairline may be the pixel above.
    if (-not (Near $bitmap.GetPixel($column, $hairline) 233 233 230 4) -and $hairline -gt 0 -and (Near $bitmap.GetPixel($column, $hairline - 1) 233 233 230 4)) { $hairline-- }
    if (-not (Near $bitmap.GetPixel($column, $hairline) 233 233 230 4)) { throw 'NFR-3: The capture shows no hairline below the row.' }
    # The row's top is measured in the empty place between the reload button and the address box.
    $gap = $left + [int](125 * $scale)
    # Some browsers blend one pixel above the rule, so the walk starts on the paper itself.
    $top = $hairline - [int](4 * $scale)
    $paper = $bitmap.GetPixel($gap, $top)
    while ($top -gt 0 -and (Near $bitmap.GetPixel($gap, $top - 1) $paper.R $paper.G $paper.B 3)) { $top-- }
    $height = ($hairline + 1 - $top) / $scale
    # Across the row: runs of pixels unlike the paper are the icons. A gap of 4 DIPs ends a run.
    $middleRow = [int](($top + $hairline) / 2)
    $runs = [Collections.Generic.List[object]]::new()
    $start = -1; $last = -1
    for ($x = $left; $x -lt $left + [int](240 * $scale); $x++) {
        $ink = $false
        for ($y = $middleRow - [int](8 * $scale); $y -le $middleRow + [int](8 * $scale); $y++) {
            # Low enough to take all of a disabled button's faint glyph, or its centre is off.
            if (-not (Near $bitmap.GetPixel($x, $y) $paper.R $paper.G $paper.B 12)) { $ink = $true; break }
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
