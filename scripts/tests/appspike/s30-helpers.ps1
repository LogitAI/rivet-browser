# Helpers of the FR-12 top bar scenarios: messages posted to the window in client coordinates, never the pointer.
function Post-Client($ctx, [uint32]$Message, [int]$W, [int]$X, [int]$Y) {
  [void][FeatureWin]::PostMessage($ctx.Hwnd, $Message, [IntPtr]$W, [IntPtr](($Y -shl 16) -bor ($X -band 0xffff)))
}
function Post-Key($ctx, [int]$Vk) {
  [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x100, [IntPtr]$Vk, [IntPtr]1)
  [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x101, [IntPtr]$Vk, [IntPtr](0xC0000001 -as [int]))
  Start-Sleep -Milliseconds 300
}
function Post-Text($ctx, [string]$Text) {
  foreach ($ch in $Text.ToCharArray()) { [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr][int]$ch, [IntPtr]1); Start-Sleep -Milliseconds 60 }
  Start-Sleep -Milliseconds 400
}
function Bar-Text($label) { Send-Flag $ctx 'rivet-tab.txt' 'bartext' | Out-Null; "${label}: " + ((Read-Log | Where-Object { $_ -match 'address bar text' } | Select-Object -Last 1)) }
function Urls($label) { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; "${label} urls: " + ((Read-Log | Where-Object { $_ -match 'tab urls \[' } | Select-Object -Last 1)) }

$f = $ctx.Dpi / 96.0
# The address starts after the panel's left edge, the three buttons and its margin.
$panelX = 268; $panelY = 38
$x = [int](($panelX + 8 + 90 + 8 + 60) * $f); $y = [int](($panelY + 22) * $f)
function Click-Address { Post-Client $ctx 0x201 1 $x $y; Start-Sleep -Milliseconds 150; Post-Client $ctx 0x202 0 $x $y; Start-Sleep 1 }
