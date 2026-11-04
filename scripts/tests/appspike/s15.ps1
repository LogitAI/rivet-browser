. scripts/tests/appspike/spike-common.ps1
# Traces the browser view's descendants with their vtables, and the ones that hold the active page.
Start-Sleep 3
Send-Flag $ctx 'rivet-tab.txt' 'tree' | Out-Null
Start-Sleep 2
Read-Log | Where-Object { $_ -match 'view tree' } | ForEach-Object { $_.Substring(0, [Math]::Min(230, $_.Length)) }
