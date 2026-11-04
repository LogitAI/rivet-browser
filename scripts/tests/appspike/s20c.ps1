. scripts/tests/appspike/spike-common.ps1
# Control: which way of typing reaches a page in the MAIN view. The probe page is the window's own page.
$f = $ctx.Dpi / 96.0
$r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
foreach ($method in 'focus', 'child') {
  $null = Send-Flag $ctx 'rivet-tab.txt' ('navigate ' + (New-Probe)); Start-Sleep 5
  $c0 = Get-Capture $ctx (Join-Path $shots "s20c-$method-0.png")
  $x = [int]($c0.Width * 0.5); $y = [int](250 * $f)
  Send-ClickAndType $ctx $method ($r.L + $x) ($r.T + $y) 'ab'
  Start-Sleep 2
  $c1 = Get-Capture $ctx (Join-Path $shots "s20c-$method-1.png")
  $green = Count-Colour $c1 ([int]($c1.Width * 0.2)) $c1.Width ([int](100 * $f)) $script:isGreen
  "control method $method : green samples after typing ab = $green"
}

