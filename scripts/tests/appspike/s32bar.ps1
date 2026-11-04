# FR-12 top bar: the browser's own suggestions for typed text, after two pages were visited.
. (Join-Path $PSScriptRoot 's30-helpers.ps1')
Send-Flag $ctx 'rivet-tab.txt' "navigate $urlB" | Out-Null; Start-Sleep 2
Send-Flag $ctx 'rivet-tab.txt' "navigate $urlA" | Out-Null; Start-Sleep 2
foreach ($q in 'page', 'pages/page-b', 'rivet words') {
  Send-Flag $ctx 'rivet-tab.txt' "barsuggest $q" | Out-Null; Start-Sleep 1
  Log-Tail 'address bar suggestions|omnibox: autocomplete'
}
