. scripts/tests/appspike/spike-common.ps1
# Tab model check: Rivet's new tab, a page's own new window link and the tab commands stay in the app window.
function Step([string]$Label) {
  Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null
  $tabs = (Get-TabUrls) -join ' | '
  $wins = @(Show-Windows $ctx)
  "$Label : tabs [$tabs] windows $($wins.Count)"
  $own = [Convert]::ToString([int64]$ctx.Hwnd, 16); $i = 0
  foreach ($w in $wins) { "   $w"; $h = ($w -split ' ')[0]; if ($h -ne $own) { $null = Get-Capture ([pscustomobject]@{ Hwnd = [IntPtr][Convert]::ToInt64($h, 16) }) (Join-Path $shots "s30-$Label-w$i.png") }; $i++ }
  $null = Get-Capture $ctx (Join-Path $shots "s30-$Label.png")
}
Step 'start'
Send-RivetMessage $ctx 'Rivet.NewStartPage'; Start-Sleep 5
Step 'newstart'
Send-Flag $ctx 'rivet-tab.txt' "add $urlB" | Out-Null; Start-Sleep 4
Step 'add'
Send-Flag $ctx 'rivet-tab.txt' 'activate 0' | Out-Null; Start-Sleep 3
Step 'activate0'
Send-Flag $ctx 'rivet-tab.txt' 'close 1' | Out-Null; Start-Sleep 3
Step 'close1'
"log: " + ((Read-Log | Where-Object { $_ -match 'check 1' } | Select-Object -Last 3) -join ' / ')
# The address bar's view of the tabs, and its commands.
Send-Flag $ctx 'rivet-tab.txt' "navigate $urlB" | Out-Null; Start-Sleep 3
Send-Flag $ctx 'rivet-tab.txt' 'pages' | Out-Null
Send-Flag $ctx 'rivet-tab.txt' 'back' | Out-Null; Start-Sleep 3
Send-Flag $ctx 'rivet-tab.txt' 'pages' | Out-Null
Send-Flag $ctx 'rivet-tab.txt' 'forward' | Out-Null; Start-Sleep 3
Send-Flag $ctx 'rivet-tab.txt' 'reload' | Out-Null; Start-Sleep 3
Send-Flag $ctx 'rivet-tab.txt' 'pages' | Out-Null
Read-Log | Where-Object { $_ -match '^page \d' } | ForEach-Object { "  $_" }
Step 'nav'
