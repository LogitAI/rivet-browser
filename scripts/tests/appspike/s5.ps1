. scripts/tests/appspike/spike-common.ps1
function Tabs($label) { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; "$label tabs: " + ((Get-TabUrls) -join ' | ') ; "$label windows:"; Show-Windows $ctx }
# A second page by the browser's own hand-off lands in a window of its own. Its tab is then moved
# into the app window with the tab strip functions.
$p = Start-TestBrowser -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $urlB)
$null = $p.WaitForExit(20000)
Start-Sleep 6
Tabs 'after hand-off'
Send-Flag $ctx "rivet-tab.txt" "move" | Out-Null
Start-Sleep 3
Tabs 'two tabs'
$one = Get-Capture $ctx (Join-Path $shots 's5-two-tabs.png')
Send-RivetMessage $ctx 'Rivet.ToggleSplit'
Start-Sleep 6
Tabs 'after split'
$sp = Get-Capture $ctx (Join-Path $shots 's5-split.png')
$v = Test-TwoPages $sp $script:ColourA $script:ColourB
"split check: $($v.Verdict) $($v.Reason)"
Read-Log | Where-Object { $_ -match 'app window|split' } | Select-Object -Last 6 | ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }
