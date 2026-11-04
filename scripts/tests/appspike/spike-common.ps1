function Get-TabUrls { $line = Read-Log | Where-Object { $_ -match 'tab urls \[' } | Select-Object -Last 1; if (-not $line) { return $null }; @([regex]::Matches($line, '"([^"]*)"') | ForEach-Object { $_.Groups[1].Value -replace '^.*/','' }) }


# A probe page: an orange/purple bar that flips every 400 ms, and an input whose page turns green after two characters.
function New-Probe {
  $path = Join-Path $pageDir 'probe.html'
  [IO.File]::WriteAllText($path, '<!doctype html><title>probe</title><body style="margin:0;background:#fff" onmousedown="document.getElementById(''m'').style.background=''#0ff''"><div id=bar style="height:90px;background:#f80"></div><input id=i style="width:80%;height:90px;font-size:50px;margin:20px" onfocus="document.getElementById(''f'').style.background=''#00f''" oninput="document.body.style.background=this.value.length>=2?''#0c0'':''#fff''"><div id=m style="height:40px;background:#eee"></div><div id=f style="height:40px;background:#eee"></div><script>setInterval(function(){var b=document.getElementById("bar");b.style.background=(b.style.background=="rgb(255, 136, 0)")?"#80f":"#f80"},400)</script>')
  'file:///' + ($path -replace '\\', '/')
}
function Count-Colour($cap, [int]$x0, [int]$x1, [int]$y0, [scriptblock]$test) {
  $n = 0
  for ($x = $x0; $x -lt $x1; $x += 4) { for ($y = $y0; $y -lt $cap.Height - 10; $y += 6) {
    $px = $cap.Pixels[$y * $cap.Width + $x]; $r = ($px -shr 16) -band 255; $g = ($px -shr 8) -band 255; $b = $px -band 255
    if (& $test $r $g $b) { $n++ } } }
  $n
}
$script:isGreen = { param($r, $g, $b) $r -lt 60 -and $g -gt 170 -and $b -lt 60 }
$script:isOrange = { param($r, $g, $b) $r -gt 220 -and $g -gt 110 -and $g -lt 160 -and $b -lt 40 }
$script:isPurple = { param($r, $g, $b) $r -gt 110 -and $r -lt 150 -and $g -lt 40 -and $b -gt 220 }
$script:isRed = { param($r, $g, $b) $r -gt 200 -and $g -lt 60 -and $b -lt 60 }
# Posts a click and then text. `Method`: child (to the window under the point), top (to the browser window, client
# coordinates), keys (top, with key down and up around each character). Nothing is activated and the cursor stays put.
function Send-ClickAndType($ctx, [string]$Method, [int]$ScreenX, [int]$ScreenY, [string]$Text) {
  if ($Method -eq 'focus' -or $Method -eq 'mixed') {
    # Only messages: the browser is told its window is active and focused. The system's foreground window does not change.
    [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x86, [IntPtr]1, [IntPtr]0)
    [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x6, [IntPtr]1, [IntPtr]0)
    [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x7, [IntPtr]0, [IntPtr]0)
    Start-Sleep -Milliseconds 500
  }
  $pt = New-Object FeatureWin+P; $pt.X = $ScreenX; $pt.Y = $ScreenY
  $under = [FeatureWin]::WindowFromPoint($pt)
  $target = if ($Method -eq 'child' -or $Method -eq 'mixed') { $under } else { $ctx.Hwnd }
  $cl = New-Object FeatureWin+P; $cl.X = $ScreenX; $cl.Y = $ScreenY; [void][FeatureWin]::ScreenToClient($target, [ref]$cl)
  $l = [IntPtr](([int]$cl.Y -shl 16) -bor ([int]$cl.X -band 0xffff))
  [void][FeatureWin]::PostMessage($target, 0x200, [IntPtr]0, $l)
  [void][FeatureWin]::PostMessage($target, 0x201, [IntPtr]1, $l); Start-Sleep -Milliseconds 150; [void][FeatureWin]::PostMessage($target, 0x202, [IntPtr]0, $l)
  Start-Sleep 1
  foreach ($ch in $Text.ToCharArray()) {
    $code = [int][char]$ch; $vk = [int][char]::ToUpper($ch)
    if ($Method -eq 'keys') { [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x100, [IntPtr]$vk, [IntPtr]1) }
    [void][FeatureWin]::PostMessage($(if ($Method -eq 'child') { $under } else { $ctx.Hwnd }), 0x102, [IntPtr]$code, [IntPtr]1)
    if ($Method -eq 'keys') { [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x101, [IntPtr]$vk, [IntPtr](0xC0000001 -as [int])) }
    Start-Sleep -Milliseconds 200
  }
}



$script:isCyan = { param($r, $g, $b) $r -lt 40 -and $g -gt 220 -and $b -gt 220 }
$script:isBlue = { param($r, $g, $b) $r -lt 40 -and $g -lt 40 -and $b -gt 220 }
