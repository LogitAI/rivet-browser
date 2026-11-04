. scripts/tests/appspike/spike-common.ps1
# The page of a separate window of the same browser shown in Rivet's view of this window (no tab moves).
$p = Start-Process -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $urlB) -PassThru -WindowStyle Hidden
$null = $p.WaitForExit(20000); Start-Sleep 6
"windows before: " + ((Show-Windows $ctx | ForEach-Object { ($_ -split ' ', 3)[2] -replace ' \d+,\d+,\d+,\d+$', '' }) -join ' || ')
$null = Get-Capture $ctx (Join-Path $shots 's16-0-before.png')
Send-Flag $ctx 'rivet-tab.txt' 'view 0 40 bv other' | Out-Null; Start-Sleep 5
$c = Get-Capture $ctx (Join-Path $shots 's16-1-view.png')
$f = $ctx.Dpi / 96.0
$red = 0; $blue = 0
for ($x = 0; $x -lt $c.Width; $x += 4) { for ($y = [int](100 * $f); $y -lt $c.Height - 10; $y += 8) {
  $px = $c.Pixels[$y * $c.Width + $x]; $r = ($px -shr 16) -band 255; $g = ($px -shr 8) -band 255; $b = $px -band 255
  if ($r -gt 200 -and $g -lt 60 -and $b -lt 60) { $red++ }; if ($b -gt 200 -and $r -lt 60) { $blue++ } } }
"samples red $red (the main page, page-a) blue $blue (the other window's page-b)"
"windows after: " + ((Show-Windows $ctx | ForEach-Object { ($_ -split ' ', 3)[2] -replace ' \d+,\d+,\d+,\d+$', '' }) -join ' || ')
Read-Log | Where-Object { $_ -match 'second view|exception' } | Select-Object -Last 8 | ForEach-Object { $_.Substring(0, [Math]::Min(260, $_.Length)) }
