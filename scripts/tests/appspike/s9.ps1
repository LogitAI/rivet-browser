. scripts/tests/appspike/spike-common.ps1
Send-Flag $ctx 'rivet-sidepanel.flag' 'sim open' | Out-Null
Start-Sleep 2
Send-Flag $ctx 'rivet-sidepanel.flag' 'sim entry 0' | Out-Null
Start-Sleep 3
Read-Log | Where-Object { $_ -match '(?i)side|rail|panel' -and $_ -notmatch '^colour' } | Select-Object -First 25 | ForEach-Object { $_.Substring(0, [Math]::Min(260, $_.Length)) }
