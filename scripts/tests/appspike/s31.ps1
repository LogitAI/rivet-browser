. scripts/tests/appspike/spike-common.ps1
function Send-Text($ctx, [string]$Text) {
  # Key down, character and key up for each letter, posted to the browser window as a keyboard would.
  foreach ($ch in $Text.ToCharArray()) { $vk = [int][char]::ToUpper($ch)
    [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x100, [IntPtr]$vk, [IntPtr]1); [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr][int]$ch, [IntPtr]1)
    [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x101, [IntPtr]$vk, [IntPtr](0xC0000001 -as [int])); Start-Sleep -Milliseconds 200 }
}
# The second view: typing after a click and a RequestFocus, then closing the view.
$f = $ctx.Dpi / 96.0
$urlProbe = New-Probe
$p = Start-Process -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $urlProbe) -PassThru -WindowStyle Hidden
$null = $p.WaitForExit(20000); Start-Sleep 6
Send-Flag $ctx 'rivet-tab.txt' 'view 0 40 bv other' | Out-Null; Start-Sleep 5
$c1 = Get-Capture $ctx (Join-Path $shots 's31-1-shown.png')
$sx = [int]($c1.Width * 0.79)
$mainRed = Count-Colour $c1 ([int]($c1.Width * 0.2)) ([int]($c1.Width * 0.7)) ([int](130 * $f)) $script:isRed
$viewBar = (Count-Colour $c1 $sx $c1.Width ([int](130 * $f)) $script:isOrange) + (Count-Colour $c1 $sx $c1.Width ([int](130 * $f)) $script:isPurple)
"shown: main view red samples $mainRed (page-a), second view bar samples $viewBar (the probe page)"
$r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
$x = $r.L + $sx + [int](40 * $f); $y = $r.T + [int](245 * $f)
Send-ClickAndType $ctx 'focus' $x $y ''
if (-not $env:NO_VIEWFOCUS) { Send-Flag $ctx 'rivet-tab.txt' 'viewfocus' | Out-Null }
Send-Text $ctx 'ab'
Start-Sleep 2
$c2 = Get-Capture $ctx (Join-Path $shots 's31-2-typed.png')
$green = Count-Colour $c2 $sx $c2.Width ([int](130 * $f)) $script:isGreen
"typed: second view green samples $green, main red $(Count-Colour $c2 ([int]($c2.Width * 0.2)) ([int]($c2.Width * 0.7)) ([int](130 * $f)) $script:isRed)"
Send-Flag $ctx 'rivet-tab.txt' 'viewclose' | Out-Null; Start-Sleep 3
$c4 = Get-Capture $ctx (Join-Path $shots 's31-3-closed.png')
$bar4 = (Count-Colour $c4 0 $c4.Width ([int](130 * $f)) $script:isOrange) + (Count-Colour $c4 0 $c4.Width ([int](130 * $f)) $script:isPurple)
"closed: second view bar samples $bar4, main red samples $(Count-Colour $c4 ([int]($c4.Width * 0.2)) ([int]($c4.Width * 0.9)) ([int](130 * $f)) $script:isRed); windows $(@(Show-Windows $ctx).Count); alive $(@(Get-MyProcesses $ctx.ExeName $ctx.Profile).Count)"
Start-Sleep 3
"after 3 s: alive $(@(Get-MyProcesses $ctx.ExeName $ctx.Profile).Count), window $([FeatureWin]::IsWindowVisible($ctx.Hwnd))"
Read-Log | Where-Object { $_ -match 'second view: (RequestFocus|closed|no )|exception' } | Select-Object -Last 4 | ForEach-Object { $_.Substring(0, [Math]::Min(220, $_.Length)) }
