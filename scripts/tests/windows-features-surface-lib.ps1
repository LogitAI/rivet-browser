# Pixel checks for the window surface rows of windows-features.ps1: split panel looks, the Sidebar's
# floating panel and open button, the developer row and the address text. Nothing here touches a
# browser, so windows-features.selftest.ps1 runs them on synthetic images. Needs
# windows-features-lib.ps1 first.

Add-Type -TypeDefinition @'
using System;
public static class SurfacePixels {
    static int Lum(int p) { return (((p >> 16) & 255) + ((p >> 8) & 255) + (p & 255)) / 3; }
    static int Dist(int p, int q) {
        return Math.Abs(((p >> 16) & 255) - ((q >> 16) & 255)) + Math.Abs(((p >> 8) & 255) - ((q >> 8) & 255)) + Math.Abs((p & 255) - (q & 255));
    }

    // Mean brightness of a rectangle, clipped to the image. -1 when nothing is left.
    public static double MeanLum(int[] px, int w, int h, int x0, int y0, int x1, int y1) {
        x0 = Math.Max(0, x0); y0 = Math.Max(0, y0); x1 = Math.Min(w, x1); y1 = Math.Min(h, y1);
        long s = 0, n = 0;
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) { s += Lum(px[y * w + x]); n++; }
        return n == 0 ? -1 : (double)s / n;
    }

    // Share of a rectangle's pixels that are strongly coloured: an accent line, not a grey shadow.
    public static double Saturated(int[] px, int w, int h, int x0, int y0, int x1, int y1) {
        x0 = Math.Max(0, x0); y0 = Math.Max(0, y0); x1 = Math.Min(w, x1); y1 = Math.Min(h, y1);
        long hit = 0, n = 0;
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) {
            int p = px[y * w + x];
            int r = (p >> 16) & 255, g = (p >> 8) & 255, b = p & 255;
            int max = Math.Max(r, Math.Max(g, b)), min = Math.Min(r, Math.Min(g, b));
            n++;
            if (max >= 80 && (max - min) * 2 >= max) hit++;
        }
        return n == 0 ? 0 : (double)hit / n;
    }

    // Mean per-pixel colour distance of a rectangle between two captures of one size.
    public static double MeanDiff(int[] a, int[] b, int w, int h, int x0, int y0, int x1, int y1) {
        x0 = Math.Max(0, x0); y0 = Math.Max(0, y0); x1 = Math.Min(w, x1); y1 = Math.Min(h, y1);
        long s = 0, n = 0;
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) { s += Dist(a[y * w + x], b[y * w + x]); n++; }
        return n == 0 ? 0 : (double)s / n;
    }

    // Columns of a band that hold ink: a pixel at least `contrast` from the band's most common
    // colour. Returns the number of runs of ink columns separated by `gap` or more, and the first
    // and last ink column (or -1).
    public static int[] InkRuns(int[] px, int w, int h, int x0, int y0, int x1, int y1, int contrast, int gap) {
        x0 = Math.Max(0, x0); y0 = Math.Max(0, y0); x1 = Math.Min(w, x1); y1 = Math.Min(h, y1);
        var counts = new System.Collections.Generic.Dictionary<int, int>();
        for (int y = y0; y < y1; y++) for (int x = x0; x < x1; x++) {
            int k = px[y * w + x] & 0xF8F8F8; int c; counts.TryGetValue(k, out c); counts[k] = c + 1;
        }
        int ground = 0, best = -1;
        foreach (var kv in counts) if (kv.Value > best) { best = kv.Value; ground = kv.Key; }
        int runs = 0, first = -1, last = -1, lastInk = -1000000;
        for (int x = x0; x < x1; x++) {
            bool ink = false;
            for (int y = y0; y < y1 && !ink; y++) if (Dist(px[y * w + x] & 0xF8F8F8, ground) >= contrast) ink = true;
            if (!ink) continue;
            if (x - lastInk > gap) runs++;
            if (first < 0) first = x;
            last = x; lastInk = x;
        }
        return new int[] { runs, first, last };
    }

    // The first column of a row between x0 and x1 that is near black.
    public static int DarkColumn(int[] px, int w, int y, int x0, int x1) {
        for (int x = Math.Max(0, x0); x < Math.Min(w, x1); x++) { int p = px[y * w + x]; if (((p >> 16) & 255) < 50 && ((p >> 8) & 255) < 50 && (p & 255) < 50) return x; }
        return -1;
    }
}
'@

function Test-PixelIs($Capture, [int]$X, [int]$Y, [int]$Rgb) {
  if ($X -lt 0 -or $Y -lt 0 -or $X -ge $Capture.Width -or $Y -ge $Capture.Height) { return $false }
  [FeaturePixels]::Fraction($Capture.Pixels, $Capture.Width, $X, $Y, $X + 1, $Y + 1, $Rgb, $script:Tolerance) -eq 1
}

# Split panels: each page is a panel with rounded lower corners, the page being worked in is raised
# by a deeper shadow below it, and no panel carries an accent coloured border.
function Test-PanelLook($Capture, [int]$ColourOne, [int]$ColourTwo, [double]$Scale) {
  if (Test-Blank $Capture) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
  $w = $Capture.Width; $h = $Capture.Height; $px = $Capture.Pixels
  $panels = @()
  foreach ($pair in @(@('first', $ColourOne), @('second', $ColourTwo))) {
    $r = Get-PageRegion $Capture $pair[1]
    if ($r.Width -lt 100 -or $r.Height -lt 100) { return New-Verdict 'FAIL' "the $($pair[0]) panel was not found" }
    $panels += [pscustomobject]@{ Name = $pair[0]; R = $r; Rgb = $pair[1] }
  }
  $in = [int][Math]::Ceiling(6 * $Scale)
  $notes = @()
  foreach ($p in $panels) {
    $r = $p.R
    # One pixel in from a corner lies outside a 12 DIP arc and six in lies inside it.
    foreach ($corner in @(@('lower left', ($r.X0 + 1), ($r.Y1 - 2), ($r.X0 + $in), ($r.Y1 - 1 - $in)), @('lower right', ($r.X1 - 2), ($r.Y1 - 2), ($r.X1 - 1 - $in), ($r.Y1 - 1 - $in)))) {
      $name, $ox, $oy, $ix, $iy = $corner
      if (-not (Test-PixelIs $Capture $ix $iy $p.Rgb)) { return New-Verdict 'FAIL' "the $($p.Name) panel's $name corner area is not the page" }
      if (Test-PixelIs $Capture $ox $oy $p.Rgb) { return New-Verdict 'FAIL' "the $($p.Name) panel's $name corner is square" }
    }
    # An accent border would show as a strongly coloured line just outside the panel.
    $o = [int][Math]::Ceiling(2 * $Scale); $b = [int][Math]::Ceiling(4 * $Scale)
    foreach ($band in @(@('left', ($r.X0 - $o - $b), ($r.Y0 + 20), ($r.X0 - $o), ($r.Y1 - 20)), @('right', ($r.X1 + $o), ($r.Y0 + 20), ($r.X1 + $o + $b), ($r.Y1 - 20)), @('below', ($r.X0 + 20), ($r.Y1 + $o), ($r.X1 - 20), ($r.Y1 + $o + $b)))) {
      $side, $x0, $y0, $x1, $y1 = $band
      $s = [SurfacePixels]::Saturated($px, $w, $h, $x0, $y0, $x1, $y1)
      if ($s -gt 0.10) { return New-Verdict 'FAIL' ('the {0} panel has a coloured border on the {1} ({2:P0} of the band)' -f $p.Name, $side, $s) }
    }
    $shadow = [SurfacePixels]::MeanLum($px, $w, $h, ($r.X0 + 20), ($r.Y1 + $o), ($r.X1 - 20), ($r.Y1 + $o + [int](8 * $Scale)))
    $p | Add-Member Shadow $shadow
    $notes += ('{0} {1}x{2} shadow {3:N0}' -f $p.Name, $r.Width, $r.Height, $shadow)
  }
  if ($panels[0].Shadow -lt 0 -or $panels[1].Shadow -lt 0) { return New-Verdict 'FAIL' 'no room below the panels for their shadows' }
  $gap = [Math]::Abs($panels[0].Shadow - $panels[1].Shadow)
  if ($gap -lt 3) { return New-Verdict 'FAIL' ('no panel is marked by a deeper shadow: {0}' -f ($notes -join ', ')) }
  $active = if ($panels[0].Shadow -lt $panels[1].Shadow) { $panels[0].Name } else { $panels[1].Name }
  New-Verdict 'PASS' ('rounded corners, no coloured border, the {0} panel raised by its shadow; {1}' -f $active, ($notes -join ', '))
}

# The page has a dark vertical marker at half its width over its top 40 per cent. Returns its column on the row at `Y`
# between `X0` and `X1`, or -1.
function Get-MarkerColumn($Capture, [int]$Y, [int]$X0, [int]$X1) { [SurfacePixels]::DarkColumn($Capture.Pixels, $Capture.Width, $Y, $X0, $X1) }

# An open Sidebar panel floats over the page: the page's marker stays where it was and the panel's
# page covers the page's right part.
function Test-PanelFloats($Before, $After, [int]$Page, [int]$Panel, [double]$Scale) {
  if (Test-Blank $After) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
  $p0 = Get-PageRegion $Before $Page
  if ($p0.Width -lt 200) { return New-Verdict 'NOT_DRIVEN' 'the page was not shown before the panel opened' }
  # The marker covers the top 40 per cent of the page at a quarter of its width, left of a 360 DIP
  # docked page on any page wider than 480 DIPs; its columns still count as the page.
  $y = [int]($p0.Y0 + ($p0.Y1 - $p0.Y0) * 0.2)
  $m0 = Get-MarkerColumn $Before $y $p0.X0 $p0.X1
  if ($m0 -lt 0) { return New-Verdict 'NOT_DRIVEN' "the page's marker was not found before the panel opened" }
  $side = Get-PageRegion $After $Panel
  if ($side.Width -lt 60) { return New-Verdict 'FAIL' 'the panel page does not show' }
  if ($side.X0 -ge $p0.X1 - 4) { return New-Verdict 'FAIL' ('the panel sits beside the page, not over it: the page ended at {0}, the panel starts at {1}' -f $p0.X1, $side.X0) }
  $m1 = Get-MarkerColumn $After $y $p0.X0 $side.X0
  if ($m1 -lt 0) { return New-Verdict 'FAIL' 'the page marker is gone after the panel opened' }
  if ([Math]::Abs($m1 - $m0) -gt [Math]::Max(2, $Scale)) { return New-Verdict 'FAIL' ('the page changed width: its middle moved from {0} to {1}' -f $m0, $m1) }
  New-Verdict 'PASS' ('panel {0} px wide over the page, page middle stays at {1}' -f $side.Width, $m0)
}

# The rail takes its own width: the page ends `RailDip` DIPs further left with the rail shown.
function Test-RailWidth($Before, $Rail, [int]$Page, [double]$Scale, [int]$RailDip = 40) {
  $a = Get-PageRegion $Before $Page; $b = Get-PageRegion $Rail $Page
  if ($a.Width -lt 200 -or $b.Width -lt 200) { return New-Verdict 'NOT_DRIVEN' 'the page was not found in both captures' }
  $moved = $a.X1 - $b.X1; $want = $RailDip * $Scale
  if ([Math]::Abs($moved - $want) -gt [Math]::Max(2, 2 * $Scale)) { return New-Verdict 'FAIL' ('the page right edge moved {0} px with the rail, the rail is {1:N0} px' -f $moved, $want) }
  New-Verdict 'PASS' ('the rail takes {0} px' -f $moved)
}

# The Sidebar's open button: unseen with the pointer away, faint near the corner and solid over the
# button. `Box` is the corner area as x0, y0, x1, y1.
function Test-OpenButtonFade($Away, $Near, $Over, [int]$Page, [int[]]$Box) {
  $x0, $y0, $x1, $y1 = $Box
  if (Test-Blank $Away) { return New-Verdict 'NOT_DRIVEN' 'the capture is blank' }
  $plain = [FeaturePixels]::Fraction($Away.Pixels, $Away.Width, $x0, $y0, $x1, $y1, $Page, $script:Tolerance)
  if ($plain -lt 0.97) { return New-Verdict 'FAIL' ('something shows in the corner with the pointer away ({0:P0} page)' -f $plain) }
  $dn = [SurfacePixels]::MeanDiff($Away.Pixels, $Near.Pixels, $Away.Width, $Away.Height, $x0, $y0, $x1, $y1)
  $do = [SurfacePixels]::MeanDiff($Away.Pixels, $Over.Pixels, $Away.Width, $Away.Height, $x0, $y0, $x1, $y1)
  if ($dn -lt 2) { return New-Verdict 'FAIL' ('the open button does not appear near the corner (change {0:N1})' -f $dn) }
  # Near the corner the study's button is 60% of a light ground and soft ink; under the pointer it is
  # whole, on a 6% dark ground with ink. On a bright page the light ground changes more than the
  # dark one, so the two states are told apart from each other, not by which changes the page more.
  if ($do -lt 2) { return New-Verdict 'FAIL' ('the open button does not show under the pointer (change {0:N1})' -f $do) }
  $between = [SurfacePixels]::MeanDiff($Near.Pixels, $Over.Pixels, $Near.Width, $Near.Height, $x0, $y0, $x1, $y1)
  if ($between -lt 1.5) { return New-Verdict 'FAIL' ('the button looks the same near the corner and under the pointer (difference {0:N1})' -f $between) }
  New-Verdict 'PASS' ('hidden away, faint near the corner ({0:N1}), its hover look under the pointer ({1:N1}, {2:N1} apart)' -f $dn, $do, $between)
}

# The developer row: its background differs from the plain row and it carries at least `Buttons`
# buttons in the `Span` (x0, x1) left of the caption buttons. `Row` is the row's y0, y1.
function Test-DevRow($Plain, $Dev, [int[]]$Row, [int[]]$Span, [int]$Buttons) {
  $y0, $y1 = $Row; $x0, $x1 = $Span
  if ((Test-Blank $Plain) -or (Test-Blank $Dev)) { return New-Verdict 'NOT_DRIVEN' 'a capture is blank' }
  $w = $Dev.Width; $h = $Dev.Height
  $edge = [int][Math]::Max(1, ($y1 - $y0) / 12)
  $a = [FeaturePixels]::Mean($Plain.Pixels, $w, $x0, $y0 + 1, $x1, $y0 + 1 + $edge, 0)
  $b = [FeaturePixels]::Mean($Dev.Pixels, $w, $x0, $y0 + 1, $x1, $y0 + 1 + $edge, 0)
  $d = Get-Distance $a $b
  if ($d -lt 20) { return New-Verdict 'FAIL' ('the developer row has the plain row background (distance {0:N0})' -f $d) }
  $inkDev = [SurfacePixels]::InkRuns($Dev.Pixels, $w, $h, $x0, $y0 + $edge * 2, $x1, $y1 - $edge * 2, 90, 3)
  $inkPlain = [SurfacePixels]::InkRuns($Plain.Pixels, $w, $h, $x0, $y0 + $edge * 2, $x1, $y1 - $edge * 2, 90, 3)
  if ($inkDev[0] -lt $Buttons) { return New-Verdict 'FAIL' ('{0} buttons left of the caption buttons, {1} wanted' -f $inkDev[0], $Buttons) }
  if ($inkPlain[0] -ge $inkDev[0]) { return New-Verdict 'FAIL' ('the plain row shows as many marks there ({0})' -f $inkPlain[0]) }
  New-Verdict 'PASS' ('background moved {0:N0}, {1} buttons left of the caption buttons (plain row {2})' -f $d, $inkDev[0], $inkPlain[0])
}

# The width of the address text in a band: last ink column less the first.
function Get-InkWidth($Capture, [int[]]$Box) {
  $x0, $y0, $x1, $y1 = $Box
  $r = [SurfacePixels]::InkRuns($Capture.Pixels, $Capture.Width, $Capture.Height, $x0, $y0, $x1, $y1, 120, 1000)
  if ($r[1] -lt 0) { return -1 }
  $r[2] - $r[1] + 1
}

# A monospace address: two addresses of one length, one of narrow letters and one of wide ones,
# take one width. In a proportional face they differ by many pixels.
function Test-Monospace([int]$Narrow, [int]$Wide, [double]$Scale, [int]$Room) {
  if ($Narrow -le 0 -or $Wide -le 0) { return New-Verdict 'FAIL' 'no address text was found in the row' }
  if ($Narrow -ge $Room - 4 -or $Wide -ge $Room - 4) { return New-Verdict 'NOT_DRIVEN' 'the address fills the field, so its width says nothing' }
  if ([Math]::Abs($Narrow - $Wide) -gt [Math]::Max(3, 3 * $Scale)) { return New-Verdict 'FAIL' ('the address is not monospace: {0} px for narrow letters, {1} px for wide ones' -f $Narrow, $Wide) }
  New-Verdict 'PASS' ('both addresses {0} and {1} px wide' -f $Narrow, $Wide)
}
