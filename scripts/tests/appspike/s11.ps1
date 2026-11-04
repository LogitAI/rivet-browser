. scripts/tests/appspike/spike-common.ps1
Send-Flag $ctx 'rivet-tab.txt' 'scan' | Out-Null
Read-Log | Where-Object { $_ -match 'type scan' } | Select-Object -Last 1 | ForEach-Object { $_.Substring(0, [Math]::Min(400, $_.Length)) }
