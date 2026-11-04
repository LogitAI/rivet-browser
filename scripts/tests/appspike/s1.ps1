. scripts/tests/appspike/spike-common.ps1
function Tabs($label) { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; "$label tabs: " + ((Get-TabUrls) -join ' | ') ; "$label windows:"; Show-Windows $ctx }
Tabs 'start'
Send-RivetMessage $ctx 'Rivet.NewStartPage'; Start-Sleep 6
Tabs 'after NewStartPage'
$null = Get-Capture $ctx (Join-Path $shots 's1-newstart.png')
$r = Open-SecondPage $ctx $urlB
"second page ok=$r"
Tabs 'after second page'
$null = Get-Capture $ctx (Join-Path $shots 's1-second.png')

