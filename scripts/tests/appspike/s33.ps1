# FR-12 top bar: typing shows the browser's suggestions under the address, Down picks one and Enter opens it.
. (Join-Path $PSScriptRoot 's30-helpers.ps1')
Send-Flag $ctx 'rivet-tab.txt' "navigate $urlB" | Out-Null; Start-Sleep 2
Send-Flag $ctx 'rivet-tab.txt' "navigate $urlA" | Out-Null; Start-Sleep 2
Click-Address
Post-Text $ctx 'page'
$null = Get-Capture $ctx (Join-Path $shots 'suggest-1-list.png')
Post-Key $ctx 0x28; Post-Key $ctx 0x28
$null = Get-Capture $ctx (Join-Path $shots 'suggest-2-picked.png')
Post-Key $ctx 0x0D; Start-Sleep 3
Log-Tail 'address bar: open suggestion'
Urls 'after enter'
$null = Get-Capture $ctx (Join-Path $shots 'suggest-3-opened.png')
