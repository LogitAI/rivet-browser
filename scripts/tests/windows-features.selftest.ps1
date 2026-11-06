# Runs the pixel checks of windows-features-lib.ps1 on synthetic windows: no browser is started.
# Each case states the verdict it must give, and the checks must reject what they should reject.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-features-lib.ps1')

$W = 900; $H = 560
function New-Window([int]$Ground = 0xF0F0F0) {
  $px = New-Object 'int[]' ($W * $H)
  $c = $Ground -bor 0xFF000000
  for ($i = 0; $i -lt $px.Length; $i++) { $px[$i] = $c }
  [pscustomobject]@{ Pixels = $px; Width = $W; Height = $H }
}
function Fill($Cap, [int]$X0, [int]$Y0, [int]$X1, [int]$Y1, [int]$Rgb) {
  $c = $Rgb -bor 0xFF000000
  for ($y = $Y0; $y -lt $Y1; $y++) { for ($x = $X0; $x -lt $X1; $x++) { $Cap.Pixels[$y * $W + $x] = $c } }
  $Cap
}
$failures = 0
function Expect([string]$Case, $Verdict, [string]$Want) {
  if ($Verdict.Verdict -ne $Want) { Write-Host "FAIL $Case : wanted $Want, got $($Verdict.Verdict) ($($Verdict.Reason))"; $script:failures++ }
  else { Write-Host "ok   $Case : $($Verdict.Verdict)" }
}
$A = $script:ColourA; $B = $script:ColourB

# Two pages side by side, with a gap, below a title row and right of an appbar.
$two = Fill (Fill (New-Window) 160 60 520 540 $A) 530 60 890 540 $B
Expect 'fr_split_two_side_by_side_passes' (Test-TwoPages $two $A $B) 'PASS'
# Two pages stacked.
$stack = Fill (Fill (New-Window) 160 60 890 290 $A) 160 300 890 540 $B
Expect 'fr_split_two_stacked_passes' (Test-TwoPages $stack $A $B) 'PASS'
# One page only: the known failure, a window that has no split.
$one = Fill (New-Window) 160 60 890 540 $B
Expect 'fr_split_fails_on_a_window_without_a_split' (Test-TwoPages $one $A $B) 'FAIL'
# The page in a different colour must not count.
$other = Fill (New-Window) 160 60 890 540 0x20A040
Expect 'fr_split_fails_on_a_third_colour' (Test-TwoPages $other $A $B) 'FAIL'
# A second page that only has a thin strip is not a split.
$thin = Fill (Fill (New-Window) 160 60 860 540 $A) 866 60 890 540 $B
Expect 'fr_split_fails_on_a_sliver' (Test-TwoPages $thin $A $B) 'FAIL'
# A second page that is painted only in part is not a painted page.
$holes = Fill (New-Window) 160 60 520 540 $A
$holes = Fill $holes 530 60 890 540 $B
for ($y = 60; $y -lt 540; $y += 4) { $holes = Fill $holes 530 $y 890 ($y + 3) 0xF0F0F0 }
Expect 'fr_split_fails_on_a_page_that_is_not_painted' (Test-TwoPages $holes $A $B) 'FAIL'
# White, black and transparent captures never pass anything.
Expect 'fr_split_white_capture_is_not_driven' (Test-TwoPages (New-Window 0xFFFFFF) $A $B) 'NOT_DRIVEN'
Expect 'fr_split_black_capture_is_not_driven' (Test-TwoPages (New-Window 0x000000) $A $B) 'NOT_DRIVEN'
Expect 'fr_one_page_white_capture_is_not_driven' (Test-OnePage (New-Window 0xFFFFFF) $A $B) 'NOT_DRIVEN'

# A split grid: stacked, three and four pages, and the windows the checks must reject.
$C = $script:ColourC; $D = $script:ColourD
Expect 'fr9_grid_page_above_passes' (Test-GridPages $stack @($A, $B) 'above') 'PASS'
Expect 'fr9_grid_page_above_fails_side_by_side' (Test-GridPages $two @($A, $B) 'above') 'FAIL'
Expect 'fr9_grid_page_above_fails_upside_down' (Test-GridPages $stack @($B, $A) 'above') 'FAIL'
$three = Fill (Fill (Fill (New-Window) 160 60 520 540 $C) 530 60 890 290 $A) 530 300 890 540 $B
Expect 'fr9_grid_three_pages_pass' (Test-GridPages $three @($C, $A, $B) 'three') 'PASS'
Expect 'fr9_grid_three_fails_with_two_pages' (Test-GridPages $two @($A, $B, $C) 'three') 'FAIL'
$four = Fill (Fill (Fill (Fill (New-Window) 160 60 520 290 $D) 160 300 520 540 $B) 530 60 890 290 $A) 530 300 890 540 $C
Expect 'fr9_grid_four_pages_pass' (Test-GridPages $four @($D, $B, $A, $C) 'four') 'PASS'
Expect 'fr9_grid_four_fails_on_three_pages' (Test-GridPages $three @($C, $A, $B, $D) 'four') 'FAIL'
$row4 = Fill (Fill (Fill (Fill (New-Window) 160 60 330 540 $D) 340 60 520 540 $B) 530 60 700 540 $A) 710 60 890 540 $C
Expect 'fr9_grid_four_fails_on_a_row_of_four' (Test-GridPages $row4 @($D, $B, $A, $C) 'four') 'FAIL'
Expect 'fr9_grid_blank_capture_is_not_driven' (Test-GridPages (New-Window 0xFFFFFF) @($A, $B) 'above') 'NOT_DRIVEN'

# Ending the split leaves one page.
Expect 'fr_unsplit_one_page_passes' (Test-OnePage $one $B $A) 'PASS'
Expect 'fr_unsplit_fails_while_two_pages_show' (Test-OnePage $two $A $B) 'FAIL'

# Sidebar: the page gets narrower with the rail, then a panel takes more of it and is painted.
$before = Fill (New-Window) 160 60 890 540 $A
$rail = Fill (Fill (New-Window) 160 60 855 540 $A) 860 60 890 540 0xD0D0E0
$rail = Fill $rail 866 100 880 114 0x303030
$panel = Fill (Fill (New-Window) 160 60 600 540 $A) 600 60 855 540 0xFFFFFF
$panel = Fill $panel 620 100 800 112 0x202020
$panel = Fill $panel 860 60 890 540 0xD0D0E0
$panel = Fill $panel 866 100 880 114 0x303030
Expect 'fr10_sidebar_rail_narrows_the_page_passes' (Test-SidebarRail $before $rail $A 1.0) 'PASS'
Expect 'fr10_sidebar_rail_fails_when_the_page_stays_whole' (Test-SidebarRail $before $before $A 1.0) 'FAIL'
Expect 'fr_sidebar_narrower_page_and_painted_panel_passes' (Test-Sidebar $before $rail $panel $A 1.0) 'PASS'
Expect 'fr_sidebar_fails_when_the_page_stays_whole' (Test-Sidebar $before $before $before $A 1.0) 'FAIL'
Expect 'fr_sidebar_fails_without_a_panel' (Test-Sidebar $before $rail $rail $A 1.0) 'FAIL'
Expect 'fr_sidebar_fails_on_an_empty_panel_area' (Test-Sidebar $before $rail (Fill (Fill (New-Window) 160 60 600 540 $A) 605 60 855 540 0xF0F0F0) $A 1.0) 'FAIL'
# The page shows through a band of the panel column, as it did under a browser bar the panel did not cover.
$gap = Fill (Fill $panel 606 80 850 130 $A) 620 100 800 112 0x202020
Expect 'fr_sidebar_fails_when_the_page_shows_through_the_panel' (Test-Sidebar $before $rail $gap $A 1.0) 'FAIL'
Expect 'fr_sidebar_blank_capture_is_not_driven' (Test-Sidebar (New-Window 0xFFFFFF) $rail $panel $A 1.0) 'NOT_DRIVEN'

# Space colours: both the top strip and the page colours must move.
function New-SpaceWindow([int]$Top, [int]$Accent) {
  $c = Fill (New-Window 0xFFFFFF) 0 0 $W 40 $Top
  $c = Fill $c 250 150 650 200 $Accent
  Fill $c 20 100 120 500 0x202020
}
$s1 = New-SpaceWindow 0xD08A52 0xC06020
$s2 = New-SpaceWindow 0x5B8BAB 0x2060C0
Expect 'fr_space_both_parts_change_passes' (Test-SpaceColours $s1 $s2 1.0) 'PASS'
Expect 'fr_space_fails_when_nothing_changes' (Test-SpaceColours $s1 $s1 1.0) 'FAIL'
Expect 'fr_space_fails_when_only_the_page_changes' (Test-SpaceColours $s1 (New-SpaceWindow 0xD08A52 0x2060C0) 1.0) 'FAIL'
Expect 'fr_space_fails_when_only_the_top_changes' (Test-SpaceColours $s1 (New-SpaceWindow 0x5B8BAB 0xC06020) 1.0) 'FAIL'
Expect 'fr_space_blank_capture_is_not_driven' (Test-SpaceColours (New-Window 0xFFFFFF) $s2 1.0) 'NOT_DRIVEN'

# Colour page beside the start page.
function New-Start { Fill (Fill (New-Window 0xFBF8F3) 400 300 800 340 0xFFFFFF) 500 100 560 160 0xF0C8B0 }
$start = New-Start
$pair = Fill (New-Start) 160 60 520 540 $A
Expect 'fr_split_colour_and_start_page_passes' (Test-PaneAndStart $pair $A) 'PASS'
Expect 'fr_split_colour_and_start_fails_on_the_colour_page_alone' (Test-PaneAndStart $one $A) 'FAIL'
Expect 'fr_split_colour_and_start_fails_on_the_start_page_alone' (Test-PaneAndStart $start $A) 'FAIL'
Expect 'fr_split_colour_and_start_fails_on_an_empty_second_page' (Test-PaneAndStart (Fill (New-Window 0xFBF8F3) 160 60 520 540 $A) $A) 'FAIL'
Expect 'fr_split_colour_and_start_blank_capture_is_not_driven' (Test-PaneAndStart (New-Window 0xFFFFFF) $A) 'NOT_DRIVEN'
# Sub page: the main page keeps its colour and the sub page shows the wanted one and no other.
$C = $script:ColourC; $D = $script:ColourD
$subC = Fill (Fill (New-Window) 160 60 520 540 $A) 530 60 890 540 $C
Expect 'fr9_sub_pane_wanted_colour_passes' (Test-SubPane $subC $A $C @($B, $D)) 'PASS'
$subB = Fill (Fill (New-Window) 160 60 520 540 $A) 530 60 890 540 $B
Expect 'fr9_sub_pane_fails_when_the_button_did_nothing' (Test-SubPane $subB $A $C @($B, $D)) 'FAIL'
Expect 'fr9_sub_pane_fails_when_the_wrong_page_shows' (Test-SubPane (Fill (Fill (New-Window) 160 60 520 540 $A) 530 60 890 540 $D) $A $C @($B, $D)) 'FAIL'
Expect 'fr9_sub_pane_fails_when_the_main_page_is_gone' (Test-SubPane (Fill (New-Window) 160 60 890 540 $C) $A $C @($B, $D)) 'FAIL'
Expect 'fr9_sub_pane_blank_capture_is_not_driven' (Test-SubPane (New-Window 0xFFFFFF) $A $C @($B)) 'NOT_DRIVEN'
# A faded icon is lighter than a drawn one.
$faded = Fill (New-Window 0xFFFFFF) 100 100 112 112 0xD3D1D6
$drawn = Fill (New-Window 0xFFFFFF) 100 100 112 112 0x4A4850
$fadedInk = Get-ButtonInk $faded 106 106 14; $drawnInk = Get-ButtonInk $drawn 106 106 14
if ($drawnInk -ge $fadedInk - 3) { Write-Host "FAIL fr9_button_ink_tells_faded_from_drawn ($drawnInk vs $fadedInk)"; $failures++ } else { Write-Host 'ok   fr9_button_ink_tells_faded_from_drawn' }
# The address bar row is at the top of the page area: a 44 px bar, then the page's 12 px top stripe.
$C = $script:ColourC
$barOk = Fill (Fill (Fill (New-Window) 160 60 890 104 0xFBFAF7) 160 104 890 116 $C) 160 116 890 540 $A
$barOk = Fill $barOk 200 70 240 94 0x4A4850
Expect 'fr12_bar_at_the_top_of_the_page_area_passes' (Test-BarAtTop $barOk $A $C 1.0) 'PASS'
# The bar drawn over the page: the page starts at the panel's top and its stripe is hidden.
$covered = Fill (Fill (New-Window) 160 60 890 540 $A) 160 60 890 104 0xFBFAF7
Expect 'fr12_bar_over_the_top_of_the_page_fails' (Test-BarAtTop $covered $A $C 1.0) 'FAIL'
# The bar covering only part of the stripe still fails.
$part = Fill (Fill (Fill (New-Window) 160 60 890 108 0xFBFAF7) 160 108 890 116 $C) 160 116 890 540 $A
Expect 'fr12_bar_over_part_of_the_stripe_fails' (Test-BarAtTop $part $A $C 1.0) 'FAIL'
# The page at the very top with no bar row above it fails.
$noBar = Fill (Fill (New-Window) 160 0 890 12 $C) 160 12 890 540 $A
Expect 'fr12_a_page_without_a_bar_row_above_it_fails' (Test-BarAtTop $noBar $A $C 1.0) 'FAIL'
Expect 'fr12_bar_check_on_a_blank_capture_is_not_driven' (Test-BarAtTop (New-Window 0xFFFFFF) $A $C 1.0) 'NOT_DRIVEN'
# The whole window against the demo's regions.
$demo = ConvertFrom-DemoGeometry '{"window":{"left":110,"top":28},"panel":{"left":270,"top":36},"toolbar":{"top":36,"height":44},"page":{"top":80}}'
if ($demo.Side -ne 160 -or $demo.BarHeight -ne 44 -or $demo.BarTop -ne 0 -or $demo.PageGap -ne 0) { Write-Host "FAIL fr12_demo_geometry_is_read ($($demo | ConvertTo-Json -Compress))"; $failures++ } else { Write-Host 'ok   fr12_demo_geometry_is_read' }
Expect 'fr12_window_matches_the_demo_layout' (Compare-ShellLayout (Test-BarAtTop $barOk $A $C 1.0) $demo 1.0) 'PASS'
$wide = [pscustomobject]@{ Side = 268; BarTop = 0; BarHeight = 44; PageGap = 0 }
Expect 'fr12_a_sidebar_of_another_width_fails' (Compare-ShellLayout (Test-BarAtTop $barOk $A $C 1.0) $wide 1.0) 'FAIL'
Expect 'fr12_a_covered_page_fails_the_demo_comparison' (Compare-ShellLayout (Test-BarAtTop $covered $A $C 1.0) $demo 1.0) 'FAIL'
# The page starts right at the bar's bottom.
Expect 'fr12_page_right_under_the_bar_passes' (Test-PageUnderBar $barOk (Test-BarAtTop $barOk $A $C 1.0) 1.0) 'PASS'
# A plain grey band between the bar and the page, as Samsung Internet 30.0.4.29 showed, fails.
$band = Fill (Fill (Fill (Fill (New-Window) 160 60 890 104 0xFBFAF7) 160 104 890 137 0xE6E6E6) 160 137 890 149 $C) 160 149 890 540 $A
Expect 'fr12_a_plain_band_under_the_bar_fails' (Test-PageUnderBar $band (Test-BarAtTop $band $A $C 1.0) 1.0) 'FAIL'
# A browser infobar with text between them passes and is named.
$info = Fill (Fill (Fill (Fill (New-Window) 160 60 890 104 0xFBFAF7) 160 104 890 140 0xFFF4CE) 160 140 890 152 $C) 160 152 890 540 $A
for ($x = 180; $x -lt 600; $x += 9) { $info = Fill $info $x 116 ($x + 5) 128 0x202020 }
$infoVerdict = Test-PageUnderBar $info (Test-BarAtTop $info $A $C 1.0) 1.0
Expect 'fr12_an_infobar_between_the_bar_and_the_page_passes' $infoVerdict 'PASS'
if ($infoVerdict.Reason -notmatch 'infobar') { Write-Host "FAIL fr12_an_infobar_is_named_in_the_result ($($infoVerdict.Reason))"; $failures++ } else { Write-Host 'ok   fr12_an_infobar_is_named_in_the_result' }
# A thin band that is too short for an infobar fails even with marks in it.
$thinBand = Fill (Fill (Fill (Fill (New-Window) 160 60 890 104 0xFBFAF7) 160 104 890 112 0xE6E6E6) 160 112 890 124 $C) 160 124 890 540 $A
$thinBand = Fill $thinBand 300 106 400 110 0x202020
Expect 'fr12_a_short_band_under_the_bar_fails' (Test-PageUnderBar $thinBand (Test-BarAtTop $thinBand $A $C 1.0) 1.0) 'FAIL'

# Split: each pane has its own row above it, and both pages start at one top.
function New-PaneRows([int]$LeftTop, [int]$RightTop, [switch]$Shared) {
  $c = New-Window 0xECE8E2
  if ($Shared) { $c = Fill $c 160 ($LeftTop - 44) 890 $LeftTop 0xFBFAF7 }
  else { $c = Fill (Fill $c 160 ($LeftTop - 44) 520 $LeftTop 0xFBFAF7) 530 ($RightTop - 44) 890 $RightTop 0xFBFAF7 }
  foreach ($x in 170, 200, 230, 540, 570, 600) { $c = Fill $c $x ($LeftTop - 30) ($x + 14) ($LeftTop - 16) 0x4A4850 }
  Fill (Fill $c 160 $LeftTop 520 540 $A) 530 $RightTop 890 540 $B
}
Expect 'fr9_split_panes_with_their_own_rows_pass' (Test-PaneRows (New-PaneRows 104 104) $A $B 1.0) 'PASS'
Expect 'fr9_split_with_one_bar_across_both_panes_fails' (Test-PaneRows (New-PaneRows 104 104 -Shared) $A $B 1.0) 'FAIL'
Expect 'fr9_split_panes_with_unequal_tops_fail' (Test-PaneRows (New-PaneRows 104 110) $A $B 1.0) 'FAIL'
$noRows = Fill (Fill (New-Window 0xECE8E2) 160 104 520 540 $A) 530 104 890 540 $B
Expect 'fr9_split_panes_without_rows_fail' (Test-PaneRows $noRows $A $B 1.0) 'FAIL'
Expect 'fr9_split_rows_blank_capture_is_not_driven' (Test-PaneRows (New-Window 0xFFFFFF) $A $B 1.0) 'NOT_DRIVEN'
# At 200% the same layout is twice the pixels.
# The table has one row per result.
$rows = @([pscustomobject]@{ browser = 'chrome'; version = '1.2'; feature = 'split'; result = 'PASS'; evidence = 'a.png'; reason = 'r' })
if ((Format-Rows $rows) -notmatch 'chrome\s+1\.2\s+split\s+PASS') { Write-Host 'FAIL table row'; $failures++ } else { Write-Host 'ok   table' }

# A link preview covers most of the page and leaves a margin of it on three sides.
$preview = Fill (Fill (Fill (New-Window) 160 60 890 540 $B) 200 70 850 90 0xFBFAF7) 200 90 850 515 $A
Expect 'fr_link_preview_over_the_page_passes' (Test-FloatingPage $preview $B $A) 'PASS'
# Pages side by side are a split, not a preview.
Expect 'fr_link_preview_fails_beside_the_page' (Test-FloatingPage $two $B $A) 'FAIL'
# A preview that fills the whole page area hides the page under it.
Expect 'fr_link_preview_fails_when_no_page_shows_under_it' (Test-FloatingPage (Fill (New-Window) 160 60 890 540 $A) $B $A) 'FAIL'
# The page under the preview gone, with the window ground around it.
Expect 'fr_link_preview_fails_on_the_window_ground' (Test-FloatingPage (Fill (New-Window) 200 90 850 515 $A) $B $A) 'FAIL'

# --- Window surface rows: split panel looks, Sidebar panel and open button, developer row ---------
. (Join-Path $PSScriptRoot 'windows-features-surface-lib.ps1')
function Round-Lower($Cap, [int]$X0, [int]$X1, [int]$Y1, [int]$Ground = 0xF0F0F0) {
  foreach ($d in @(@(0, 1), @(1, 1), @(0, 2), @(1, 2), @(2, 1), @(0, 3), @(3, 1))) {
    Fill (Fill $Cap ($X0 + $d[0]) ($Y1 - $d[1]) ($X0 + $d[0] + 1) ($Y1 - $d[1] + 1) $Ground) ($X1 - 1 - $d[0]) ($Y1 - $d[1]) ($X1 - $d[0]) ($Y1 - $d[1] + 1) $Ground | Out-Null
  }
  $Cap
}
function New-Panels([int]$ShadowA = 0xC8C8C8, [int]$ShadowB = 0xE6E6E6, [switch]$Square) {
  $c = Fill (Fill (New-Window) 160 60 520 400 $A) 530 60 890 400 $B
  $c = Fill (Fill $c 180 400 500 410 $ShadowA) 550 400 870 410 $ShadowB
  if (-not $Square) { $c = Round-Lower (Round-Lower $c 160 520 400) 530 890 400 }
  $c
}
Expect 'fr9_panels_with_round_corners_and_one_raised_by_shadow_pass' (Test-PanelLook (New-Panels) $A $B 1) 'PASS'
Expect 'fr9_square_panels_fail' (Test-PanelLook (New-Panels -Square) $A $B 1) 'FAIL'
Expect 'fr9_panels_with_equal_shadows_fail' (Test-PanelLook (New-Panels 0xD8D8D8 0xD8D8D8) $A $B 1) 'FAIL'
Expect 'fr9_an_accent_border_on_the_active_panel_fails' (Test-PanelLook (Fill (New-Panels) 524 80 528 380 0x1E78FF) $A $B 1) 'FAIL'

function New-Marked([int]$Right, [int]$Marker) { Fill (Fill (New-Window) 160 60 $Right 540 $A) $Marker 60 ($Marker + 4) 252 0x101010 }
$floatBefore = New-Marked 890 525
Expect 'fr10_panel_over_the_page_keeps_its_width_passes' (Test-PanelFloats $floatBefore (Fill (New-Marked 890 525) 700 60 890 540 $B) $A $B 1) 'PASS'
Expect 'fr10_panel_that_narrows_the_page_fails' (Test-PanelFloats $floatBefore (Fill (New-Marked 700 430) 710 60 890 540 $B) $A $B 1) 'FAIL'
Expect 'fr10_no_panel_fails' (Test-PanelFloats $floatBefore (New-Marked 890 525) $A $B 1) 'FAIL'
Expect 'fr10_rail_takes_its_own_width_passes' (Test-RailWidth (New-Marked 890 525) (New-Marked 850 505) $A 1) 'PASS'
Expect 'fr10_rail_of_the_wrong_width_fails' (Test-RailWidth (New-Marked 890 525) (New-Marked 870 515) $A 1) 'FAIL'

function Mix([int]$Over, [int]$Under, [double]$Alpha) {
  $o = ConvertTo-Colour $Over; $u = ConvertTo-Colour $Under
  ([int]($o.R * $Alpha + $u.R * (1 - $Alpha)) -shl 16) -bor ([int]($o.G * $Alpha + $u.G * (1 - $Alpha)) -shl 8) -bor [int]($o.B * $Alpha + $u.B * (1 - $Alpha))
}
$corner = @(818, 468, 890, 540)
$away = Fill (New-Window) 160 60 890 540 $A
$near = Fill (Fill (New-Window) 160 60 890 540 $A) 856 504 886 534 (Mix 0x404040 $A 0.6)
$over = Fill (Fill (New-Window) 160 60 890 540 $A) 856 504 886 534 0x404040
Expect 'fr10_open_button_hidden_faint_then_solid_passes' (Test-OpenButtonFade $away $near $over $A $corner) 'PASS'
Expect 'fr10_open_button_that_never_appears_fails' (Test-OpenButtonFade $away $away $over $A $corner) 'FAIL'
Expect 'fr10_open_button_solid_near_the_corner_fails' (Test-OpenButtonFade $away $over $over $A $corner) 'FAIL'
Expect 'fr10_open_button_shown_with_the_pointer_away_fails' (Test-OpenButtonFade $over $near $over $A $corner) 'FAIL'

function New-Row([int]$Ground, [int]$Buttons) {
  $c = Fill (Fill (New-Window) 160 64 890 540 $A) 160 20 890 64 $Ground
  for ($k = 0; $k -lt $Buttons; $k++) { $x = 646 + 30 * $k + 8; $c = Fill $c $x 35 ($x + 14) 49 0xFFFFFF }
  $c
}
$plainRow = New-Row 0xF7F5F0 0
Expect 'fr11_dev_row_with_its_background_and_seven_buttons_passes' (Test-DevRow $plainRow (New-Row 0x2A2B2E 7) @(20, 64) @(640, 856) 7) 'PASS'
Expect 'fr11_dev_row_with_the_plain_background_fails' (Test-DevRow $plainRow (Fill (New-Row 0xF7F5F0 0) 646 35 860 49 0x202020) @(20, 64) @(640, 856) 7) 'FAIL'
Expect 'fr11_dev_row_missing_buttons_fails' (Test-DevRow $plainRow (New-Row 0x2A2B2E 4) @(20, 64) @(640, 856) 7) 'FAIL'
$text = Fill (Fill (New-Window) 200 30 210 50 0x202020) 300 30 305 50 0x202020
$width = Get-InkWidth $text @(160, 25, 600, 55)
if ($width -ne 105) { Write-Host "FAIL fr11_ink_width_spans_the_text : wanted 105, got $width"; $failures++ } else { Write-Host 'ok   fr11_ink_width_spans_the_text : 105' }
Expect 'fr11_equal_widths_are_monospace' (Test-Monospace 200 201 1 400) 'PASS'
Expect 'fr11_unequal_widths_are_not_monospace' (Test-Monospace 200 260 1 400) 'FAIL'
Expect 'fr11_a_full_field_says_nothing' (Test-Monospace 398 398 1 400) 'NOT_DRIVEN'

if ($failures) { Write-Host "$failures failed"; exit 1 }
Write-Host 'windows-features selftest: all passed'
