. scripts/tests/appspike/spike-common.ps1
# Tries each candidate offset of the browser type: the new tab stays in the app window when the right one is flipped.
foreach ($hex in ($env:CANDS -split ';')) {
  $n = (($hex -split ',') | ForEach-Object { [Convert]::ToInt32($_, 16) }) -join ' '
  Send-Flag $ctx 'rivet-tab.txt' "flip $n" | Out-Null
  Start-Sleep 5
  Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null
  $tabs = (Get-TabUrls) -join ' | '
  $wins = @(Show-Windows $ctx)
  "offset 0x$hex : tabs [$tabs] windows $($wins.Count): " + (($wins | ForEach-Object { ($_ -split ' ', 3)[2] -replace ' \d+,\d+,\d+,\d+$', '' }) -join ' || ')
  "   log: " + ((Read-Log | Where-Object { $_ -match 'app window flip' } | Select-Object -Last 1))
  $null = Get-Capture $ctx (Join-Path $shots "s12-$hex.png")
  if ($wins.Count -eq 0) { "the browser is gone after 0x$hex"; break }
}


