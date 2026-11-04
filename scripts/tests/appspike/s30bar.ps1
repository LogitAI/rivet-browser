# FR-12 top bar: rest, a click on the address, typing, Escape, Enter with an address and with a search,
# the security level and the page info bubble. Run through appspike-drv.ps1 -Script.
. (Join-Path $PSScriptRoot 's30-helpers.ps1')
"click at $x,$y (dpi $($ctx.Dpi))"
$null = Get-Capture $ctx (Join-Path $shots 'bar-0-rest.png')
Urls 'start'
Send-Flag $ctx 'rivet-tab.txt' 'barsecurity' | Out-Null; Log-Tail 'address bar security'
Post-Client $ctx 0x201 1 $x $y; Start-Sleep -Milliseconds 150; Post-Client $ctx 0x202 0 $x $y; Start-Sleep 1
Bar-Text 'after click'
$null = Get-Capture $ctx (Join-Path $shots 'bar-1-focused.png')
Post-Text $ctx 'abc'
Bar-Text 'after typing abc'
$null = Get-Capture $ctx (Join-Path $shots 'bar-2-typed.png')
# Ctrl+Z through the engine's own undo.
[void][FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr]0x1A, [IntPtr]1); Start-Sleep -Milliseconds 400
Bar-Text 'after ctrl+z char'
Post-Key $ctx 0x1B
Bar-Text "after escape"
Post-Key $ctx 0x1B
$null = Get-Capture $ctx (Join-Path $shots 'bar-3-escape.png')
# Open page B by its address.
Post-Client $ctx 0x201 1 $x $y; Start-Sleep -Milliseconds 150; Post-Client $ctx 0x202 0 $x $y; Start-Sleep 1
Post-Text $ctx $urlB
Bar-Text "before enter"
Post-Key $ctx 0x0D
Start-Sleep 3
Urls 'after enter address'
Log-Tail 'address bar: open|omnibox:'
$null = Get-Capture $ctx (Join-Path $shots 'bar-4-address.png')
Send-Flag $ctx 'rivet-tab.txt' 'barclassify rivet search words' | Out-Null; Log-Tail 'address bar classify'
$cx = [int](($panelX + 8 + 90 + 8 + 6 + 11) * $f)
Post-Client $ctx 0x201 1 $cx $y; Start-Sleep -Milliseconds 150; Post-Client $ctx 0x202 0 $cx $y; Start-Sleep 2
Log-Tail 'address bar: page info'
$wins = Show-Windows $ctx; $wins
$null = Get-Capture $ctx (Join-Path $shots "bar-5-info.png")
# The bubble is a widget of its own, owned by the browser window. Capture each one beside the window.
$n = 0
foreach ($line in $wins) { $h = [IntPtr][Convert]::ToInt64(($line -split " ")[0], 16); if ($h -ne $ctx.Hwnd) { $n++; $null = Get-Capture ([pscustomobject]@{ Hwnd = $h }) (Join-Path $shots "bar-5-bubble-$n.png") } }
