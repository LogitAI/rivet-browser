. scripts/tests/appspike/spike-common.ps1
# Side by side: the window's page narrows and the second view takes the rest. The divider moves,
# the window is resized and minimised, and the view closes. The second page counts in its title.
Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public static class Win3{[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int c);}'
$f = $ctx.Dpi / 96.0
$path = Join-Path $pageDir 'tick.html'
[IO.File]::WriteAllText($path, '<!doctype html><title>tick0</title><body style="margin:0;background:#80f"><div id=bar style="height:200px;background:#f80"></div><script>var n=0;setInterval(function(){n++;document.title="tick"+n;var b=document.getElementById("bar");b.style.background=n%2?"#80f":"#f80"},300)</script>')
$url = 'file:///' + ($path -replace '\\', '/')
$p = Start-TestBrowser -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $url)
$null = $p.WaitForExit(20000); Start-Sleep 6
function OtherTitle { $own = [Convert]::ToString([int64]$ctx.Hwnd, 16); $l = Show-Windows $ctx | Where-Object { $_ -notmatch "^$own " -and $_ -match '\[tick' } | Select-Object -First 1; if ($l) { [int]([regex]::Match($l, 'tick(\d+)')).Groups[1].Value } else { -1 } }
# The x where page-a's red ends and where the second page (purple or orange) starts, on one row.
function Edges($cap) {
  $y = [int]($cap.Height * 0.6); $redEnd = -1; $pageStart = -1
  for ($x = [int]($cap.Width * 0.2); $x -lt $cap.Width - 4; $x++) {
    $px = $cap.Pixels[$y * $cap.Width + $x]; $r = ($px -shr 16) -band 255; $g = ($px -shr 8) -band 255; $b = $px -band 255
    if ($redEnd -lt 0 -and -not (& $script:isRed $r $g $b)) { $redEnd = $x }
    if ($redEnd -ge 0 -and ((& $script:isPurple $r $g $b) -or (& $script:isOrange $r $g $b))) { $pageStart = $x; break }
  }
  "red ends at $redEnd, second page starts at $pageStart, width $($cap.Width) (gap $($pageStart - $redEnd))"
}
$c0 = Get-Capture $ctx (Join-Path $shots 's32-0-before.png'); "before: $(Edges $c0)"
Send-Flag $ctx 'rivet-tab.txt' 'view' | Out-Null; Start-Sleep 4
$c1 = Get-Capture $ctx (Join-Path $shots 's32-1-split.png'); "split 600: $(Edges $c1)"
Send-Flag $ctx 'rivet-tab.txt' 'share 300' | Out-Null; Start-Sleep 2
$c2 = Get-Capture $ctx (Join-Path $shots 's32-2-share300.png'); "split 300: $(Edges $c2)"
$r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
$w0 = $r.Ri - $r.L; $h0 = $r.B - $r.T
[void][FeatureWin]::SetWindowPos($ctx.Hwnd, [IntPtr]::Zero, 0, 0, [int]($w0 * 0.7), [int]($h0 * 0.8), 0x16); Start-Sleep 3
$c3 = Get-Capture $ctx (Join-Path $shots 's32-3-smaller.png'); "smaller: $(Edges $c3)"
[void][FeatureWin]::SetWindowPos($ctx.Hwnd, [IntPtr]::Zero, 0, 0, $w0, $h0, 0x16); Start-Sleep 3
$c4 = Get-Capture $ctx (Join-Path $shots 's32-4-restored.png'); "back to full size: $(Edges $c4)"
$a = OtherTitle; [void][Win3]::ShowWindow($ctx.Hwnd, 7); Start-Sleep 4; $b = OtherTitle
"minimised: the second page's counter $a -> $b"
[void][Win3]::ShowWindow($ctx.Hwnd, 4); Start-Sleep 3
$seen = @{}; foreach ($i in 1..6) { $c = Get-Capture $ctx (Join-Path $shots 's32-5-restored.png'); $o = Count-Colour $c ([int]($c.Width * 0.6)) $c.Width ([int](130 * $f)) $script:isOrange; $q = Count-Colour $c ([int]($c.Width * 0.6)) $c.Width ([int](130 * $f)) $script:isPurple; $seen["o$($o -gt 0)p$($q -gt 0)"] = 1; Start-Sleep -Milliseconds 330 }
"restored: colours seen in the second view over six captures: $($seen.Keys -join ', ')"
Send-Flag $ctx 'rivet-tab.txt' 'viewclose' | Out-Null; Start-Sleep 3
$c6 = Get-Capture $ctx (Join-Path $shots 's32-6-closed.png'); "closed: $(Edges $c6)"
$t1 = OtherTitle; Start-Sleep 2; $t2 = OtherTitle
"after close: owner window counter $t1 -> $t2; alive $(@(Get-MyProcesses $ctx.ExeName $ctx.Profile).Count)"
$own = [Convert]::ToString([int64]$ctx.Hwnd, 16)
$ow = Show-Windows $ctx | Where-Object { $_ -notmatch "^$own " } | Select-Object -First 1
if ($ow) { $oh = [IntPtr][Convert]::ToInt64(($ow -split ' ')[0], 16); [void][FeatureWin]::SetWindowPos($oh, [IntPtr]::Zero, 9000, 1000, 1200, 800, 0x14); Start-Sleep 2
  $co = Get-Capture ([pscustomobject]@{ Hwnd = $oh }) (Join-Path $shots 's32-7-owner.png'); $q = (Count-Colour $co 0 $co.Width 100 $script:isPurple) + (Count-Colour $co 0 $co.Width 100 $script:isOrange); "owner window shows its page again: samples $q" }
Read-Log | Where-Object { $_ -match 'second view: (open|closed)|exception' } | Select-Object -Last 3 | ForEach-Object { $_.Substring(0, [Math]::Min(220, $_.Length)) }
