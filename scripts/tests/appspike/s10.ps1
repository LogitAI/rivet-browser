. scripts/tests/appspike/spike-common.ps1
function Tabs($label) { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; "$label tabs: " + ((Get-TabUrls) -join ' | ') ; "$label windows: " + ((Show-Windows $ctx | Measure-Object).Count) }
$p = Start-TestBrowser -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $urlB)
$null = $p.WaitForExit(20000); Start-Sleep 5
Send-Flag $ctx 'rivet-tab.txt' 'move' | Out-Null; Start-Sleep 3
Tabs 'two tabs'
$null = Get-Capture $ctx (Join-Path $shots 's10-1-two-tabs.png')
Send-Flag $ctx 'rivet-tab.txt' 'view 0 40 bv' | Out-Null; Start-Sleep 5
Read-Log | Where-Object { $_ -match 'second view' } | Select-Object -Last 8 | ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }
Tabs 'after view'
$c = Get-Capture $ctx (Join-Path $shots 's10-2-view.png')
$f = $ctx.Dpi / 96.0
# The right 40 percent of the page area should show page-a (red) beside page-b (blue).
$red = 0; $blue = 0
for ($x = 0; $x -lt $c.Width; $x += 4) { for ($y = [int](100 * $f); $y -lt $c.Height - 10; $y += 8) {
  $px = $c.Pixels[$y * $c.Width + $x]; $r = ($px -shr 16) -band 255; $g = ($px -shr 8) -band 255; $b = $px -band 255
  if ($r -gt 200 -and $g -lt 60 -and $b -lt 60) { $red++ }; if ($b -gt 200 -and $r -lt 60) { $blue++ } } }
"samples red $red blue $blue"
Send-Flag $ctx 'rivet-tab.txt' 'view bounds 30' | Out-Null; Start-Sleep 3
$c2 = Get-Capture $ctx (Join-Path $shots 's10-3-resized.png')
Send-Flag $ctx 'rivet-tab.txt' 'activate 0' | Out-Null; Start-Sleep 3
Tabs 'after activate 0 (the tab shown in the second view)'
$null = Get-Capture $ctx (Join-Path $shots 's10-4-activate.png')
Send-Flag $ctx 'rivet-tab.txt' 'close 1' | Out-Null; Start-Sleep 3
Tabs 'after close 1'
$null = Get-Capture $ctx (Join-Path $shots 's10-5-close.png')

