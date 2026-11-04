. scripts/tests/appspike/spike-common.ps1
# Hypothesis: a page whose tab belongs to another window stays in Rivet's view when clicked, and takes text.
function Titles { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; (Read-Log | Where-Object { $_ -match 'tab command' } | Select-Object -Last 1) -replace '^.*tabs ', '' }
$f = $ctx.Dpi / 96.0
$urlProbe = New-Probe
$p = Start-Process -FilePath $ctx.ExePath -ArgumentList @("--user-data-dir=`"$($ctx.Profile)`"", $urlProbe) -PassThru -WindowStyle Hidden
$null = $p.WaitForExit(20000); Start-Sleep 6
"tabs of the app window before: $(Titles)"
Send-Flag $ctx 'rivet-tab.txt' 'view 0 40 bv other' | Out-Null; Start-Sleep 5
$c1 = Get-Capture $ctx (Join-Path $shots 's20-1-shown.png')
$sx = [int]($c1.Width * 0.79)
$mainRed = Count-Colour $c1 ([int]($c1.Width * 0.2)) ([int]($c1.Width * 0.7)) ([int](130 * $f)) $script:isRed
$viewBar = (Count-Colour $c1 $sx $c1.Width ([int](130 * $f)) $script:isOrange) + (Count-Colour $c1 $sx $c1.Width ([int](130 * $f)) $script:isPurple)
"shown: main view red samples $mainRed (page-a), second view bar samples $viewBar (the probe page)"
Start-Sleep 1
$c1b = Get-Capture $ctx (Join-Path $shots 's20-1b-tick.png')
$o1 = Count-Colour $c1 $sx $c1.Width ([int](130 * $f)) $script:isOrange; $o2 = Count-Colour $c1b $sx $c1b.Width ([int](130 * $f)) $script:isOrange
"timer: orange samples $o1 then $o2 (a change means the page keeps repainting)"
# Click into the second view's input and type.
if ($env:VIEW_FOCUS) { [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x86, [IntPtr]1, [IntPtr]0); [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x6, [IntPtr]1, [IntPtr]0); [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x7, [IntPtr]0, [IntPtr]0); Start-Sleep 1; Send-Flag $ctx 'rivet-tab.txt' 'viewfocus' | Out-Null; Start-Sleep 1 }
$r = New-Object FeatureWin+R; [FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r) | Out-Null
$x = $r.L + $sx + [int](40 * $f); $y = $r.T + [int](245 * $f)
if ($env:FOCUS_AFTER) { Send-ClickAndType $ctx 'focus' $x $y ''; Send-Flag $ctx 'rivet-tab.txt' 'viewfocus' | Out-Null; foreach ($ch in 'ab'.ToCharArray()) { [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr][int]$ch, [IntPtr]1); Start-Sleep -Milliseconds 200 } } else { Send-ClickAndType $ctx $(if ($env:TYPE_METHOD) { $env:TYPE_METHOD } else { 'focus' }) $x $y 'ab' }
Start-Sleep 2
$c2 = Get-Capture $ctx (Join-Path $shots 's20-2-typed.png')
$green = Count-Colour $c2 $sx $c2.Width ([int](130 * $f)) $script:isGreen
$mainRed2 = Count-Colour $c2 ([int]($c2.Width * 0.2)) ([int]($c2.Width * 0.7)) ([int](130 * $f)) $script:isRed
$click = Count-Colour $c2 $sx $c2.Width ([int](130 * $f)) $script:isCyan; $focus = Count-Colour $c2 $sx $c2.Width ([int](130 * $f)) $script:isBlue
$still = (Count-Colour $c2 $sx $c2.Width ([int](130 * $f)) $script:isOrange) + (Count-Colour $c2 $sx $c2.Width ([int](130 * $f)) $script:isPurple)
"after click and typing ab: mouse-down seen by the page $click, focus seen $focus, second view green samples $green (the probe page reacted), bar samples $still, main view red samples $mainRed2 (was $mainRed)"
"tabs of the app window after: $(Titles)"
Read-Log | Where-Object { $_ -match 'second view|exception' } | Select-Object -Last 4 | ForEach-Object { $_.Substring(0, [Math]::Min(220, $_.Length)) }




