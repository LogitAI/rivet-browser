# Pixel checks and verdicts for windows-features.ps1. Nothing here touches a browser, so
# windows-features.selftest.ps1 can run it on synthetic images anywhere pwsh runs.
#
# A verdict is PASS, FAIL or NOT_DRIVEN. PASS needs pixels that show the feature. NOT_DRIVEN means
# the check could not set the scene up, so it says nothing about the feature. A blank, white or
# single-colour capture never passes.

Add-Type -TypeDefinition @'
using System;
public static class FeaturePixels {
    static bool Near(int p, int rgb, int tol) {
        return Math.Abs(((p >> 16) & 255) - ((rgb >> 16) & 255)) <= tol
            && Math.Abs(((p >> 8) & 255) - ((rgb >> 8) & 255)) <= tol
            && Math.Abs((p & 255) - (rgb & 255)) <= tol;
    }

    // The widest run of columns in which at least colFrac of the rows between bandTop and bandBottom
    // have the colour. Returns x0, x1 (exclusive), y0, y1 (exclusive) and the matching pixel count,
    // or zeros when no column qualifies.
    public static int[] Region(int[] px, int w, int h, int rgb, int tol, int bandTop, int bandBottom, double colFrac) {
        int need = (int)Math.Ceiling((bandBottom - bandTop) * colFrac);
        bool[] ok = new bool[w];
        for (int x = 0; x < w; x++) {
            int n = 0;
            for (int y = bandTop; y < bandBottom; y++) if (Near(px[y * w + x], rgb, tol)) n++;
            ok[x] = n >= need && n > 0;
        }
        int bestStart = 0, bestEnd = 0, start = -1, lastOk = -1;
        for (int x = 0; x <= w; x++) {
            bool on = x < w && ok[x];
            if (on) {
                if (start < 0) start = x;
                lastOk = x;
            } else if (start >= 0 && (x == w || x - lastOk > 3)) {
                if (lastOk + 1 - start > bestEnd - bestStart) { bestStart = start; bestEnd = lastOk + 1; }
                start = -1;
            }
        }
        if (bestEnd <= bestStart) return new int[] { 0, 0, 0, 0, 0 };
        int width = bestEnd - bestStart, top = -1, bottom = -1, count = 0;
        for (int y = 0; y < h; y++) {
            int n = 0;
            for (int x = bestStart; x < bestEnd; x++) if (Near(px[y * w + x], rgb, tol)) n++;
            count += n;
            if (n >= width * 0.3) { if (top < 0) top = y; bottom = y + 1; }
        }
        if (top < 0) return new int[] { 0, 0, 0, 0, 0 };
        return new int[] { bestStart, bestEnd, top, bottom, count };
    }

    public static double Fraction(int[] px, int w, int x0, int y0, int x1, int y1, int rgb, int tol) {
        long n = 0, total = 0;
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) { total++; if (Near(px[y * w + x], rgb, tol)) n++; }
        return total == 0 ? 0 : (double)n / total;
    }

    // Share of the rectangle that is not its most common colour (colours grouped in steps of 8).
    public static double NonDominant(int[] px, int w, int x0, int y0, int x1, int y1) {
        var counts = new System.Collections.Generic.Dictionary<int, int>();
        long total = 0;
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) {
            int key = px[y * w + x] & 0xF8F8F8;
            int c; counts.TryGetValue(key, out c); counts[key] = c + 1; total++;
        }
        if (total == 0) return 0;
        int best = 0;
        foreach (var c in counts.Values) if (c > best) best = c;
        return 1.0 - (double)best / total;
    }

    // Mean colour of the pixels of a rectangle whose saturation is at least minSat (0 for all).
    // Returns r, g, b and the pixel count.
    public static double[] Mean(int[] px, int w, int x0, int y0, int x1, int y1, double minSat) {
        double r = 0, g = 0, b = 0; long n = 0;
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) {
            int p = px[y * w + x];
            int pr = (p >> 16) & 255, pg = (p >> 8) & 255, pb = p & 255;
            int max = Math.Max(pr, Math.Max(pg, pb)), min = Math.Min(pr, Math.Min(pg, pb));
            double sat = max == 0 ? 0 : (double)(max - min) / max;
            if (sat < minSat) continue;
            r += pr; g += pg; b += pb; n++;
        }
        if (n == 0) return new double[] { 0, 0, 0, 0 };
        return new double[] { r / n, g / n, b / n, n };
    }
}
'@

# Colours of the two test pages, as 0xRRGGBB. Far from each other and from Rivet's own tints.
$script:ColourA = 0xE61E1E
$script:ColourB = 0x1E3CE6
$script:Tolerance = 24

function ConvertTo-Colour([int]$Rgb) { [pscustomobject]@{ R = ($Rgb -shr 16) -band 255; G = ($Rgb -shr 8) -band 255; B = $Rgb -band 255 } }

function New-Verdict([string]$Verdict, [string]$Reason) { [pscustomobject]@{ Verdict = $Verdict; Reason = $Reason } }

# A capture: Pixels (int[] ARGB), Width, Height.
function Get-PageRegion($Capture, [int]$Rgb) {
    $w = $Capture.Width; $h = $Capture.Height
    $r = [FeaturePixels]::Region($Capture.Pixels, $w, $h, $Rgb, $script:Tolerance, [int]($h * 0.10), [int]($h * 0.95), 0.30)
    [pscustomobject]@{ X0 = $r[0]; X1 = $r[1]; Y0 = $r[2]; Y1 = $r[3]; Count = $r[4]; Width = $r[1] - $r[0]; Height = $r[3] - $r[2]; Area = ($r[1] - $r[0]) * ($r[3] - $r[2]) }
}

# True when the capture is empty: one colour fills nearly all of it. PrintWindow of a window it
# cannot paint gives a black or white image, and that must never count as evidence.
function Test-Blank($Capture) {
    [FeaturePixels]::NonDominant($Capture.Pixels, $Capture.Width, 0, 0, $Capture.Width, $Capture.Height) -lt 0.02
}

function Get-Coverage($Region, $Capture) { if ($Region.Area -eq 0) { 0 } else { $Region.Count / ($Capture.Width * $Capture.Height) } }

function Test-Overlap($A, $B) {
    $A.X0 -lt $B.X1 -and $B.X0 -lt $A.X1 -and $A.Y0 -lt $B.Y1 -and $B.Y0 -lt $A.Y1
}

# One page is shown: `Shown` is its colour, `Hidden` the colour of the page that must not appear.
function Test-OnePage($Capture, [int]$Shown, [int]$Hidden) {
    if (Test-Blank $Capture) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
    $window = $Capture.Width * $Capture.Height
    $s = Get-PageRegion $Capture $Shown
    $o = Get-PageRegion $Capture $Hidden
    if ((Get-Coverage $s $Capture) -lt 0.30) { return New-Verdict 'FAIL' ('the page of the active tab fills {0:P0} of the window, not one whole page' -f (Get-Coverage $s $Capture)) }
    if ($o.Count -gt $window * 0.005) { return New-Verdict 'FAIL' ('the other page still shows ({0:P1} of the window)' -f ($o.Count / $window)) }
    New-Verdict 'PASS' ('one page, {0}x{1} px' -f $s.Width, $s.Height)
}

# Two pages side by side or stacked: both colours, each with its own solid area, none over the other.
function Test-TwoPages($Capture, [int]$ColourOne, [int]$ColourTwo) {
    if (Test-Blank $Capture) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
    $window = $Capture.Width * $Capture.Height
    $a = Get-PageRegion $Capture $ColourOne
    $b = Get-PageRegion $Capture $ColourTwo
    foreach ($pair in @(@('first', $a), @('second', $b))) {
        $name, $r = $pair
        if ($r.Count -lt $window * 0.08) { return New-Verdict 'FAIL' ('the {0} page does not show: {1:P1} of the window has its colour' -f $name, ($r.Count / $window)) }
        if ($r.Count -lt $r.Area * 0.85) { return New-Verdict 'FAIL' ('the {0} page is not painted solidly (fill {1:P0})' -f $name, ($r.Count / $r.Area)) }
    }
    if (Test-Overlap $a $b) { return New-Verdict 'FAIL' 'the two areas overlap' }
    New-Verdict 'PASS' ('first {0}x{1} at {2},{3}; second {4}x{5} at {6},{7}' -f $a.Width, $a.Height, $a.X0, $a.Y0, $b.Width, $b.Height, $b.X0, $b.Y0)
}

# The pages of a split grid. Every colour shows as a solid page, no two overlap, and they sit as
# `Shape` says: 'above' puts the first page above the second in one column; 'three' puts the first
# page beside the other two, which are stacked; 'four' is two columns of two, given as the left
# column top to bottom, then the right column.
function Test-GridPages($Capture, [int[]]$Colours, [string]$Shape) {
    if (Test-Blank $Capture) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
    $window = $Capture.Width * $Capture.Height
    $r = @($Colours | ForEach-Object { Get-PageRegion $Capture $_ })
    for ($i = 0; $i -lt $r.Count; $i++) {
        if ($r[$i].Count -lt $window * 0.04) { return New-Verdict 'FAIL' ('page {0} does not show: {1:P1} of the window has its colour' -f ($i + 1), ($r[$i].Count / $window)) }
        if ($r[$i].Count -lt $r[$i].Area * 0.85) { return New-Verdict 'FAIL' ('page {0} is not painted solidly (fill {1:P0})' -f ($i + 1), ($r[$i].Count / $r[$i].Area)) }
        for ($j = $i + 1; $j -lt $r.Count; $j++) {
            if (Test-Overlap $r[$i] $r[$j]) { return New-Verdict 'FAIL' ('pages {0} and {1} overlap' -f ($i + 1), ($j + 1)) }
        }
    }
    # Two pages share a column when their middles are close, and one is above the other when it
    # ends before the other starts.
    $tol = $Capture.Width * 0.06
    $column = { param($a, $b) [Math]::Abs(($a.X0 + $a.X1) / 2 - ($b.X0 + $b.X1) / 2) -lt $tol }
    $above = { param($a, $b) $a.Y1 -le $b.Y0 -and $a.X0 -lt $b.X1 -and $b.X0 -lt $a.X1 }
    $beside = { param($a, $b) $a.X1 -le $b.X0 -or $b.X1 -le $a.X0 }
    $ok = switch ($Shape) {
        'above' { $r.Count -eq 2 -and (& $above $r[0] $r[1]) }
        'three' { $r.Count -eq 3 -and (& $beside $r[0] $r[1]) -and (& $beside $r[0] $r[2]) -and (& $column $r[1] $r[2]) -and (& $above $r[1] $r[2]) -and $r[0].Height -gt 1.5 * $r[1].Height }
        'four' { $r.Count -eq 4 -and (& $column $r[0] $r[1]) -and (& $column $r[2] $r[3]) -and (& $above $r[0] $r[1]) -and (& $above $r[2] $r[3]) -and (& $beside $r[0] $r[2]) -and [Math]::Abs($r[0].Y0 - $r[2].Y0) -lt $Capture.Height * 0.06 }
        default { $false }
    }
    $where = ($r | ForEach-Object { '{0}x{1} at {2},{3}' -f $_.Width, $_.Height, $_.X0, $_.Y0 }) -join '; '
    if (-not $ok) { return New-Verdict 'FAIL' "the pages are not arranged as '$Shape': $where" }
    New-Verdict 'PASS' ('{0}: {1}' -f $Shape, $where)
}

# Page narrower and a rail beside it, then a panel in front of the page.
# The rail alone: the page gets narrower when it opens. The sidebar page itself is then shown in the
# second page view docked at the right, which the caller checks as two pages.
function Test-SidebarRail($Before, $Rail, [int]$Colour, [double]$Scale) {
    foreach ($c in @($Before, $Rail)) { if (Test-Blank $c) { return New-Verdict 'NOT_DRIVEN' 'a capture is blank' } }
    $r0 = Get-PageRegion $Before $Colour
    if ((Get-Coverage $r0 $Before) -lt 0.30) { return New-Verdict 'NOT_DRIVEN' 'the test page is not shown before the sidebar opens' }
    $r1 = Get-PageRegion $Rail $Colour
    $narrow = $r0.X1 - $r1.X1
    if ($narrow -lt 20 * $Scale) { return New-Verdict 'FAIL' ('the page did not get narrower when the rail opened (right edge moved {0} px)' -f $narrow) }
    New-Verdict 'PASS' ('rail narrowed the page by {0} px' -f $narrow)
}

function Test-Sidebar($Before, $Rail, $Panel, [int]$Colour, [double]$Scale) {
    foreach ($c in @($Before, $Rail, $Panel)) { if (Test-Blank $c) { return New-Verdict 'NOT_DRIVEN' 'a capture is blank' } }
    $r0 = Get-PageRegion $Before $Colour
    if ((Get-Coverage $r0 $Before) -lt 0.30) { return New-Verdict 'NOT_DRIVEN' 'the test page is not shown before the sidebar opens' }
    $r1 = Get-PageRegion $Rail $Colour
    $narrowRail = $r0.X1 - $r1.X1
    if ($narrowRail -lt 20 * $Scale) { return New-Verdict 'FAIL' ('the page did not get narrower when the rail opened (right edge moved {0} px)' -f $narrowRail) }
    $strip = [FeaturePixels]::NonDominant($Rail.Pixels, $Rail.Width, $r1.X1 + 2, $r1.Y0, [Math]::Min($r0.X1 + 2, $Rail.Width), $r1.Y1)
    $r2 = Get-PageRegion $Panel $Colour
    $narrowPanel = $r1.X1 - $r2.X1
    if ($narrowPanel -lt 120 * $Scale) { return New-Verdict 'FAIL' ('the panel did not take room from the page (right edge moved {0} px)' -f $narrowPanel) }
    $content = [FeaturePixels]::NonDominant($Panel.Pixels, $Panel.Width, $r2.X1 + 2, $r2.Y0, $r1.X1, $r2.Y1)
    if ($content -lt 0.003) { return New-Verdict 'FAIL' 'the area beside the page is empty, so the panel is not painted' }
    # The panel is one piece with its header: from the top of the page down, none of the page shows
    # in the column between the page and the rail. A page that shows there is a gap in the panel.
    $margin = [int](6 * $Scale)
    $leak = [FeaturePixels]::Fraction($Panel.Pixels, $Panel.Width, $r2.X1 + $margin, $r0.Y0 + 2, $r1.X1 - $margin, $r0.Y1 - 2, $Colour, $script:Tolerance)
    if ($leak -gt 0.02) { return New-Verdict 'FAIL' ('the page shows through the panel column ({0:P1} of it), so the panel is not whole' -f $leak) }
    New-Verdict 'PASS' ('rail narrowed the page by {0} px, panel by {1} px more, panel content {2:P1}' -f $narrowRail, $narrowPanel, $content)
}

function Get-Distance($A, $B) { [Math]::Sqrt(($A[0] - $B[0]) * ($A[0] - $B[0]) + ($A[1] - $B[1]) * ($A[1] - $B[1]) + ($A[2] - $B[2]) * ($A[2] - $B[2])) }

# The frame at the top edge of the window and the tinted pixels of the start page, in both captures.\n# Tints are pale, so a change of 8 in RGB distance is already plainly visible.
function Test-SpaceColours($One, $Two, [double]$Scale) {
    foreach ($c in @($One, $Two)) { if (Test-Blank $c) { return New-Verdict 'NOT_DRIVEN' 'a capture is blank' } }
    $stripH = [int](4 * $Scale) + 4
    # The right half keeps clear of the appbar. The band is the frame above the toolbar row.
    $x0 = [int]($One.Width * 0.5); $x1 = $One.Width - [int](20 * $Scale)
    $topOne = [FeaturePixels]::Mean($One.Pixels, $One.Width, $x0, 2, $x1, $stripH, 0)
    $topTwo = [FeaturePixels]::Mean($Two.Pixels, $Two.Width, $x0, 2, $x1, $stripH, 0)
    $pageTop = [int](120 * $Scale)
    $pxOne = [FeaturePixels]::Mean($One.Pixels, $One.Width, [int]($One.Width * 0.25), $pageTop, $One.Width - 2, $One.Height - 2, 0.05)
    $pxTwo = [FeaturePixels]::Mean($Two.Pixels, $Two.Width, [int]($Two.Width * 0.25), $pageTop, $Two.Width - 2, $Two.Height - 2, 0.05)
    $top = Get-Distance $topOne $topTwo
    if ($pxOne[3] -lt 2000 -or $pxTwo[3] -lt 2000) { return New-Verdict 'NOT_DRIVEN' 'the start page shows no coloured pixels to compare' }
    $page = Get-Distance $pxOne $pxTwo
    $parts = @()
    if ($top -lt 8) { $parts += ('the top of the window kept its colour (distance {0:N0})' -f $top) }
    if ($page -lt 8) { $parts += ('the start page kept its colours (distance {0:N0})' -f $page) }
    if ($parts.Count) { return New-Verdict 'FAIL' ($parts -join '; ') }
    New-Verdict 'PASS' ('top of the window moved {0:N0}, start page colours moved {1:N0}' -f $top, $page)
}

function Format-Rows($Rows) {
    $widths = @(8, 8, 8, 10, 0, 0)
    $lines = @()
    $lines += ('{0,-8} {1,-16} {2,-8} {3,-10} {4,-46} {5}' -f 'browser', 'version', 'feature', 'result', 'evidence', 'reason')
    foreach ($r in $Rows) {
        $evidence = if ($r.evidence) { [string]$r.evidence } else { '-' }
        if ($evidence.Length -gt 46) { $evidence = '...' + $evidence.Substring($evidence.Length - 43) }
        $lines += ('{0,-8} {1,-16} {2,-8} {3,-10} {4,-46} {5}' -f $r.browser, $r.version, $r.feature, $r.result, $evidence, $r.reason)
    }
    $lines -join [Environment]::NewLine
}

# A seed for the spaces of this test profile: two spaces of the browser under test, so a switch
# stays in the window. Without it every other default space belongs to another browser, and a
# switch would start that browser.
function Write-SpaceSeed([string]$Profile, [string]$Key) {
  $local = $env:LOCALAPPDATA
  $normal = $Profile.TrimEnd('\', '/').Replace('/', '\').ToLowerInvariant()
  # FNV-1a over the lower-cased profile path, as the runtime keys its spaces file.
  $hash = [System.Numerics.BigInteger]::Parse('14695981039346656037')
  $prime = [System.Numerics.BigInteger]::Parse('1099511628211')
  $mod = [System.Numerics.BigInteger]::Pow(2, 64)
  foreach ($b in [Text.Encoding]::UTF8.GetBytes($normal)) {
    $low = [System.Numerics.BigInteger]($hash % 256)
    $hash = (($hash - $low + [System.Numerics.BigInteger]([int]$low -bxor [int]$b)) * $prime) % $mod
  }
  $file = Join-Path $local ('Rivet\state\spaces\' + $hash.ToString('x').PadLeft(16, '0').TrimStart('0').PadLeft(16, '0') + '.json')
  New-Item -ItemType Directory -Force (Split-Path $file) | Out-Null
  $json = @{
    version = 2; user_data_dir = $Profile; current = 0; appbar = @{ width = 276; hidden = $false }
    spaces = @(
      @{ name = 'FeatureA'; tint = 0xd08a52; browser = $Key; profile = 'FeatureA'; tabs = @() },
      @{ name = 'FeatureB'; tint = 0x5b8bab; browser = $Key; profile = 'FeatureB'; tabs = @() }
    )
  } | ConvertTo-Json -Depth 6
  [IO.File]::WriteAllText($file, $json)
  $file
}

# A colour page on the left and some other page, such as the start page, beside it. The other page
# has no known colour, so it must be wide, must not show the colour, and must have content.
function Test-PaneAndStart($Capture, [int]$Colour) {
  if (Test-Blank $Capture) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
  $window = $Capture.Width * $Capture.Height
  $r = Get-PageRegion $Capture $Colour
  if ($r.Count -lt $window * 0.08) { return New-Verdict 'FAIL' ('the colour page does not show: {0:P1} of the window' -f ($r.Count / $window)) }
  if ($r.Count -lt $r.Area * 0.85) { return New-Verdict 'FAIL' ('the colour page is not painted solidly (fill {0:P0})' -f ($r.Count / $r.Area)) }
  $x0 = $r.X1 + 4; $x1 = $Capture.Width - 4
  if (($x1 - $x0) -lt $Capture.Width * 0.15) { return New-Verdict 'FAIL' 'no second page beside the colour page' }
  if ([FeaturePixels]::Fraction($Capture.Pixels, $Capture.Width, $x0, $r.Y0, $x1, $r.Y1, $Colour, $script:Tolerance) -gt 0.01) { return New-Verdict 'FAIL' 'the colour page also fills the second area' }
  $content = [FeaturePixels]::NonDominant($Capture.Pixels, $Capture.Width, $x0, $r.Y0, $x1, $r.Y1)
  if ($content -lt 0.002) { return New-Verdict 'FAIL' 'the second page is empty, not painted' }
  New-Verdict 'PASS' ('colour page {0}x{1} at {2},{3}; second page {4} px wide with {5:P1} content' -f $r.Width, $r.Height, $r.X0, $r.Y0, ($x1 - $x0), $content)
}

# Two more page colours for the sub page tests: a page reached by a link, and the same page after a
# reload. Far from the first two and from each other.
$script:ColourC = 0x1EB43C
$script:ColourD = 0xE6C81E

# The main page keeps its colour and the sub page, which is no tab, shows `Sub`. None of the colours
# in `Others` may show, so a button that did nothing, or the wrong thing, is told apart from the right one.
function Test-SubPane($Capture, [int]$Main, [int]$Sub, [int[]]$Others) {
  $v = Test-TwoPages $Capture $Main $Sub
  if ($v.Verdict -ne 'PASS') { return $v }
  $window = $Capture.Width * $Capture.Height
  foreach ($other in $Others) {
    $r = Get-PageRegion $Capture $other
    if ($r.Count -gt $window * 0.005) { return New-Verdict 'FAIL' ('a page colour that must not show covers {0:P1} of the window' -f ($r.Count / $window)) }
  }
  $v
}

# How dark a row button's icon is: the mean brightness of a box of `Half` pixels around its centre. A
# faded icon is lighter than a drawn one, so the same button in two captures tells enabled from disabled.
function Get-ButtonInk($Capture, [int]$CentreX, [int]$CentreY, [int]$Half) {
  $m = [FeaturePixels]::Mean($Capture.Pixels, $Capture.Width, $CentreX - $Half, $CentreY - $Half, $CentreX + $Half, $CentreY + $Half, 0)
  if ($m[3] -eq 0) { return $null }
  ($m[0] + $m[1] + $m[2]) / 3
}
# The address bar sits at the top of the page area and the page starts right below it. The test page
# has a top stripe of `Stripe` that is `StripeDip` tall over a body of `Page`, so a bar drawn over the
# top of the page hides the stripe, which a page of one colour could never show. `BarDip` is the
# bar's height. Returns a verdict with the page's left, top and right in capture pixels.
function Test-BarAtTop($Capture, [int]$Page, [int]$Stripe, [double]$Scale, [int]$BarDip = 44, [int]$StripeDip = 12) {
    $none = { param($v, $r) $o = New-Verdict $v $r; $o | Add-Member PageLeft 0; $o | Add-Member PageTop 0; $o | Add-Member PageRight 0; $o }
    if (Test-Blank $Capture) { return & $none 'NOT_DRIVEN' 'the capture is blank' }
    $w = $Capture.Width; $h = $Capture.Height; $px = $Capture.Pixels; $tol = $script:Tolerance
    $body = Get-PageRegion $Capture $Page
    if ((Get-Coverage $body $Capture) -lt 0.20) { return & $none 'NOT_DRIVEN' 'the test page was not shown' }
    $x0 = $body.X0; $x1 = $body.X1
    $top = -1
    for ($y = 0; $y -lt $h; $y++) {
        if ([FeaturePixels]::Fraction($px, $w, $x0, $y, $x1, $y + 1, $Stripe, $tol) -ge 0.9) { $top = $y; break }
    }
    if ($top -lt 0) { return & $none 'FAIL' 'the top of the page is covered: its top stripe does not show' }
    $rows = 0
    while ($top + $rows -lt $h -and [FeaturePixels]::Fraction($px, $w, $x0, $top + $rows, $x1, $top + $rows + 1, $Stripe, $tol) -ge 0.9) { $rows++ }
    $wantRows = [int][Math]::Floor($StripeDip * $Scale) - 2
    if ($rows -lt $wantRows) { return & $none 'FAIL' ("only {0} of {1} rows of the page's top stripe show: the bar covers the top of the page" -f $rows, $wantRows) }
    $bar = [int][Math]::Round($BarDip * $Scale)
    if ($top - $bar -lt 0) { return & $none 'FAIL' ('the page starts {0} px from the window top, with no room for the {1} px bar above it' -f $top, $bar) }
    $inBar = [FeaturePixels]::Fraction($px, $w, $x0, $top - $bar, $x1, $top, $Page, $tol) + [FeaturePixels]::Fraction($px, $w, $x0, $top - $bar, $x1, $top, $Stripe, $tol)
    if ($inBar -gt 0.02) { return & $none 'FAIL' ('the bar row above the page shows the page ({0:P0}): the bar is not at the top of the page area' -f $inBar) }
    $above = [FeaturePixels]::Fraction($px, $w, $x0, 0, $x1, $top, $Page, $tol)
    if ($above -gt 0.01) { return & $none 'FAIL' ('the page shows above the bar ({0:P1})' -f $above) }
    $v = New-Verdict 'PASS' ('the bar ends and the page starts at y {0} px' -f $top)
    $v | Add-Member PageLeft $x0; $v | Add-Member PageTop $top; $v | Add-Member PageRight $x1
    $v
}

# Compares the window's layout with the shell demo's, in DIPs. `Demo` holds Side (the sidebar's
# width), BarTop (the bar's top below the panel's top), BarHeight and PageGap (the page's top below
# the bar's bottom), measured from the demo's own render. `Bar` is the verdict of Test-BarAtTop
# with the demo's bar height, so the bar row and the page below it are already proven.
function Compare-ShellLayout($Bar, $Demo, [double]$Scale, [double]$TolDip = 12) {
    if ($Bar.Verdict -ne 'PASS') { return New-Verdict $Bar.Verdict $Bar.Reason }
    if ($Demo.BarTop -ne 0 -or $Demo.PageGap -ne 0) { return New-Verdict 'NOT_DRIVEN' ('the demo itself has its bar at {0} and its page {1} below it' -f $Demo.BarTop, $Demo.PageGap) }
    $side = $Bar.PageLeft / $Scale
    if ([Math]::Abs($side - $Demo.Side) -gt $TolDip) { return New-Verdict 'FAIL' ('the sidebar is {0:N0} DIP wide, the demo {1} DIP' -f $side, $Demo.Side) }
    $pageTop = $Bar.PageTop / $Scale
    if ($pageTop -lt $Demo.BarHeight) { return New-Verdict 'FAIL' ('the page starts {0:N0} DIP from the top, above the end of a {1} DIP bar' -f $pageTop, $Demo.BarHeight) }
    New-Verdict 'PASS' ('sidebar {0:N0} DIP (demo {1}), bar {2} DIP at the top of the page area, page below it at {3:N0} DIP' -f $side, $Demo.Side, $Demo.BarHeight, $pageTop)
}

# The demo's layout from the JSON its measuring script wrote: rectangles of .window, .panel,
# .panel .toolbar and .panel .page in CSS pixels at a device scale of one.
function ConvertFrom-DemoGeometry([string]$Json) {
    $g = $Json | ConvertFrom-Json
    [pscustomobject]@{
        Side = [int][Math]::Round($g.panel.left - $g.window.left)
        BarTop = [int][Math]::Round($g.toolbar.top - $g.panel.top)
        BarHeight = [int][Math]::Round($g.toolbar.height)
        PageGap = [int][Math]::Round($g.page.top - ($g.toolbar.top + $g.toolbar.height))
    }
}

# Rivet's paper, the colour of the bar and of every page row.
$script:Paper = 0xFBFAF7

# The page starts right at the bar's bottom, within `TolDip`. `Bar` is a passing verdict of
# Test-BarAtTop. The bar's top is the first row above the page that is mostly paper. A band between
# the bar and the page passes only when it is a browser infobar: 24 to 64 DIP tall and carrying
# text or icons. The result then names it. A plain band, like a strip the browser keeps above its
# page area, fails.
function Test-PageUnderBar($Capture, $Bar, [double]$Scale, [int]$BarDip = 44, [double]$TolDip = 2) {
    if ($Bar.Verdict -ne 'PASS') { return New-Verdict $Bar.Verdict $Bar.Reason }
    $w = $Capture.Width; $px = $Capture.Pixels
    $span = $Bar.PageRight - $Bar.PageLeft
    $x0 = $Bar.PageLeft + [int]($span * 0.2); $x1 = $Bar.PageLeft + [int]($span * 0.8)
    $barTop = -1
    for ($y = 0; $y -lt $Bar.PageTop; $y++) {
        if ([FeaturePixels]::Fraction($px, $w, $x0, $y, $x1, $y + 1, $script:Paper, 6) -ge 0.6) { $barTop = $y; break }
    }
    if ($barTop -lt 0) { return New-Verdict 'FAIL' 'no bar row of Rivet paper shows above the page' }
    $barBottom = $barTop + [int][Math]::Round($BarDip * $Scale)
    $gap = $Bar.PageTop - $barBottom
    $gapDip = $gap / $Scale
    if ([Math]::Abs($gapDip) -le $TolDip) { return New-Verdict 'PASS' ('the page starts {0:N1} DIP from the bar bottom' -f $gapDip) }
    if ($gap -lt 0) { return New-Verdict 'FAIL' ('the page starts {0:N0} DIP above the bar bottom' -f (-$gapDip)) }
    $ink = [FeaturePixels]::NonDominant($px, $w, $Bar.PageLeft, $barBottom, $Bar.PageRight, $Bar.PageTop)
    if ($gapDip -ge 24 -and $gapDip -le 64 -and $ink -ge 0.01) {
        return New-Verdict 'PASS' ('a browser infobar of {0:N0} DIP sits between the bar and the page' -f $gapDip)
    }
    New-Verdict 'FAIL' ('a band of {0:N0} DIP with no infobar content separates the bar bottom from the page top' -f $gapDip)
}

# Each page of a split has its own row, as the shell demo's panels do: the two pages start at the
# same top within `TolDip`, a row of paper with buttons or text sits right above each page, and the
# gap between the pages runs up through the rows, so no one bar spans both pages.
function Test-PaneRows($Capture, [int]$ColourOne, [int]$ColourTwo, [double]$Scale, [int]$RowDip = 44, [double]$TolDip = 2) {
    if (Test-Blank $Capture) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
    $w = $Capture.Width; $px = $Capture.Pixels
    $pages = @((Get-PageRegion $Capture $ColourOne), (Get-PageRegion $Capture $ColourTwo)) | Sort-Object X0
    $left, $right = $pages
    if ($left.Area -eq 0 -or $right.Area -eq 0) { return New-Verdict 'FAIL' 'two pages do not show' }
    $tops = [Math]::Abs($left.Y0 - $right.Y0) / $Scale
    if ($tops -gt $TolDip) { return New-Verdict 'FAIL' ('the pane tops differ by {0:N1} DIP' -f $tops) }
    $top = [Math]::Min($left.Y0, $right.Y0)
    $y0 = $top - [int][Math]::Round($RowDip * $Scale) + 2; $y1 = $top - 3
    if ($y0 -lt 0 -or $y1 -le $y0) { return New-Verdict 'FAIL' 'no room for a row above the pages' }
    $index = 0
    foreach ($p in $pages) {
        $index++
        $a = $p.X0 + [int]($p.Width * 0.1); $b = $p.X1 - [int]($p.Width * 0.1)
        $paper = [FeaturePixels]::Fraction($px, $w, $a, $y0, $b, $y1, $script:Paper, 6)
        if ($paper -lt 0.5) { return New-Verdict 'FAIL' ('pane {0} has no row of its own above it (paper {1:P0})' -f $index, $paper) }
        if ([FeaturePixels]::NonDominant($px, $w, $p.X0, $y0, $p.X1, $y1) -lt 0.005) { return New-Verdict 'FAIL' ('the row of pane {0} is empty' -f $index) }
    }
    $g0 = $left.X1 + 1; $g1 = $right.X0 - 1
    if ($g1 -le $g0) { return New-Verdict 'FAIL' 'the pages have no gap between them' }
    $shared = [FeaturePixels]::Fraction($px, $w, $g0, $y0, $g1, $y1, $script:Paper, 4)
    if ($shared -ge 0.5) { return New-Verdict 'FAIL' ('one bar runs across both pages ({0:P0} of the gap above them is paper)' -f $shared) }
    New-Verdict 'PASS' ('each pane has its own row; tops at {0} and {1} px' -f $left.Y0, $right.Y0)
}
