. scripts/tests/appspike/spike-common.ps1
# Clicks, typing and focus in the second page view, then closing the tab whose page it shows.

function Titles { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; (Read-Log | Where-Object { $_ -match 'tab command' } | Select-Object -Last 1) -replace '^.*tabs ', '' }
$type = Join-Path $pageDir 'type.html'
[IO.File]::WriteAllText($type, '<!doctype html><title>t</title><body style="margin:0;background:#fff"><input id=i style="width:90%;height:120px;font-size:60px;margin:20px" oninput="document.title=''typed:''+this.value">')
$urlType = 'file:///' + ($type -replace '\\', '/')
$p = Start-Process -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $urlType) -PassThru -WindowStyle Hidden
$null = $p.WaitForExit(20000); Start-Sleep 5
Send-Flag $ctx 'rivet-tab.txt' 'move' | Out-Null; Start-Sleep 3
Send-Flag $ctx 'rivet-tab.txt' 'activate 0' | Out-Null; Start-Sleep 2
"before: $(Titles)"
Send-Flag $ctx 'rivet-tab.txt' 'view 1 40 bv' | Out-Null; Start-Sleep 4
$c = Get-Capture $ctx (Join-Path $shots 's14-1-shown.png')
"shown: $(Titles)"
# The second view's area: where the white page is on the right.
$f = $ctx.Dpi / 96.0
$x0 = [int]($c.Width * 0.82); $y0 = [int](140 * $f)
$r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
$pt = New-Object FeatureWin+P; $pt.X = $r.L + $x0; $pt.Y = $r.T + $y0
$target = [FeatureWin]::WindowFromPoint($pt)
$sb = New-Object Text.StringBuilder 64; [void][FeatureWin]::GetClassName($target, $sb, 64)
"window under the point in the second view: $($target.ToInt64().ToString('x')) class $sb (browser window $($ctx.Hwnd.ToInt64().ToString('x')))"
$cl = New-Object FeatureWin+P; $cl.X = $pt.X; $cl.Y = $pt.Y; [void][FeatureWin]::ScreenToClient($target, [ref]$cl)
$l = [IntPtr](([int]$cl.Y -shl 16) -bor ([int]$cl.X -band 0xffff))
[void][FeatureWin]::PostMessage($target, 0x201, [IntPtr]1, $l); Start-Sleep -Milliseconds 150; [void][FeatureWin]::PostMessage($target, 0x202, [IntPtr]0, $l)
Start-Sleep 2
foreach ($ch in 'a', 'b', 'c') { [void][FeatureWin]::PostMessage($target, 0x102, [IntPtr][int][char]$ch, [IntPtr]1); Start-Sleep -Milliseconds 150 }
Start-Sleep 2
"after click and typing abc: $(Titles)"
$null = Get-Capture $ctx (Join-Path $shots 's14-2-typed.png')
# Close the tab whose page the view shows.
Send-Flag $ctx 'rivet-tab.txt' 'close 1' | Out-Null; Start-Sleep 3
"after closing that tab: $(Titles); windows $(@(Show-Windows $ctx).Count)"
$null = Get-Capture $ctx (Join-Path $shots 's14-3-closed.png')

