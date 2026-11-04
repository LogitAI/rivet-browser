. scripts/tests/appspike/spike-common.ps1
function Tabs($label) { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; "$label tabs: " + ((Get-TabUrls) -join ' | ') ; "$label windows:"; Show-Windows $ctx }
Send-Flag $ctx "rivet-tab.txt" "add $urlB" | Out-Null
Start-Sleep 3
Send-Flag $ctx "rivet-tab.txt" "move" | Out-Null
Start-Sleep 3
Tabs 'two tabs'
$one = Get-Capture $ctx (Join-Path $shots 's4-two-tabs.png')
Send-RivetMessage $ctx 'Rivet.ToggleSplit'
Start-Sleep 6
Tabs 'after split'
$sp = Get-Capture $ctx (Join-Path $shots 's4-split.png')
$v = Test-TwoPages $sp $script:ColourA $script:ColourB
"split check: $($v.Verdict) $($v.Reason)"
Read-Log | Where-Object { $_ -match 'split' } | Select-Object -Last 5 | ForEach-Object { $_.Substring(0, [Math]::Min(250, $_.Length)) }
