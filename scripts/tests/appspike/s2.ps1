. scripts/tests/appspike/spike-common.ps1
function Tabs($label) { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; "$label tabs: " + ((Get-TabUrls) -join ' | ') ; "$label windows:"; Show-Windows $ctx }
Tabs 'start'
Send-Flag $ctx "rivet-tab.txt" "add $urlB" | Out-Null
Start-Sleep 3
Send-Flag $ctx "rivet-tab.txt" "move" | Out-Null
Start-Sleep 4
Read-Log | Where-Object { $_ -match 'app window' } | Select-Object -Last 3 | ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }
Tabs 'after add'
$null = Get-Capture $ctx (Join-Path $shots 's2-add.png')
Send-Flag $ctx 'rivet-tab.txt' "add $urlSubB" | Out-Null
Start-Sleep 4
Tabs 'after add 2'
$null = Get-Capture $ctx (Join-Path $shots 's2-add2.png')
Send-Flag $ctx 'rivet-tab.txt' "activate 0" | Out-Null
Start-Sleep 3
Tabs 'after activate 0'
$null = Get-Capture $ctx (Join-Path $shots 's2-act0.png')
Send-Flag $ctx 'rivet-tab.txt' "activate 1" | Out-Null
Start-Sleep 3
Tabs 'after activate 1'
$null = Get-Capture $ctx (Join-Path $shots 's2-act1.png')
Send-Flag $ctx 'rivet-tab.txt' "close 1" | Out-Null
Start-Sleep 3
Tabs 'after close 1'
$null = Get-Capture $ctx (Join-Path $shots 's2-close.png')
