# FR-12 top bar: Korean composition in the address, with the input method's windows at the caret.
. (Join-Path $PSScriptRoot 's30-helpers.ps1')
$null = Get-Capture $ctx (Join-Path $shots 'ime-0.png')
Click-Address
Post-Text $ctx 'ab'
Send-Flag $ctx 'rivet-tab.txt' 'barime 한' | Out-Null; Start-Sleep 1
Log-Tail 'address bar ime'
Bar-Text 'while composing'
$null = Get-Capture $ctx (Join-Path $shots 'ime-1-composing.png')
Send-Flag $ctx 'rivet-tab.txt' 'barimedone' | Out-Null; Start-Sleep 1
Bar-Text 'after composition'
$null = Get-Capture $ctx (Join-Path $shots 'ime-2-done.png')
Show-Windows $ctx
