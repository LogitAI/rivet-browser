. scripts/tests/appspike/spike-common.ps1
# The window's own only page is moved into Rivet's view: tests the view, its bounds and SetWebContents without a second page.
$null = Get-Capture $ctx (Join-Path $shots 's17-0-before.png')
Send-Flag $ctx 'rivet-tab.txt' 'view 0 40 bv' | Out-Null; Start-Sleep 5
$c = Get-Capture $ctx (Join-Path $shots 's17-1-view.png')
$f = $ctx.Dpi / 96.0
$red = 0; $left = 0; $right = 0
for ($x = 0; $x -lt $c.Width; $x += 4) { for ($y = [int](130 * $f); $y -lt $c.Height - 10; $y += 8) {
  $px = $c.Pixels[$y * $c.Width + $x]; $r = ($px -shr 16) -band 255; $g = ($px -shr 8) -band 255; $b = $px -band 255
  if ($r -gt 200 -and $g -lt 60 -and $b -lt 60) { $red++; if ($x -gt $c.Width * 0.7) { $right++ } else { $left++ } } } }
"red samples $red, in the right 30 percent $right, elsewhere $left"
Read-Log | Where-Object { $_ -match 'second view|exception' } | Select-Object -Last 8 | ForEach-Object { $_.Substring(0, [Math]::Min(260, $_.Length)) }
