# FR-12 top bar: the browser's own password save bubble on a local fake login form, where it is anchored,
# saving the fake password through the bubble, and the browser filling it on the next visit.
# The form is served on 127.0.0.1 by a listener of this script, since a browser saves no password for a file.
. (Join-Path $PSScriptRoot 's30-helpers.ps1')
$port = 8765
$site = Join-Path $OutDir 'login-site'
New-Item -ItemType Directory -Force $site | Out-Null
$css = '<style>body{margin:0;font:20px sans-serif}input{display:block;width:600px;height:60px;font-size:28px;margin:20px}input:-webkit-autofill,input:autofill{box-shadow:0 0 0 200px #00c000 inset!important}</style>'
[IO.File]::WriteAllText((Join-Path $site 'login.html'), "<!doctype html><meta charset=utf-8><title>fake login</title>$css<form action=done.html method=post><input id=u name=username autocomplete=username><input id=p name=password type=password autocomplete=current-password><button style='width:300px;height:60px;margin:20px'>Sign in</button></form>")
[IO.File]::WriteAllText((Join-Path $site 'done.html'), '<!doctype html><meta charset=utf-8><title>signed in</title><body style="background:#eef">signed in</body>')
$server = Start-ThreadJob -ArgumentList $site, $port -ScriptBlock {
  param($site, $port)
  $l = New-Object Net.HttpListener; $l.Prefixes.Add("http://127.0.0.1:$port/"); $l.Start()
  $until = (Get-Date).AddMinutes(4)
  while ((Get-Date) -lt $until) {
    $t = $l.GetContextAsync(); while (-not $t.Wait(500)) { if ((Get-Date) -ge $until) { break } }
    if (-not $t.IsCompleted) { break }
    $c = $t.Result; $name = [IO.Path]::GetFileName($c.Request.Url.AbsolutePath); if (-not $name) { $name = 'login.html' }
    $path = Join-Path $site $name
    $bytes = if (Test-Path $path) { [IO.File]::ReadAllBytes($path) } else { [Text.Encoding]::UTF8.GetBytes('missing') }
    $c.Response.ContentType = 'text/html; charset=utf-8'; $c.Response.OutputStream.Write($bytes, 0, $bytes.Length); $c.Response.Close()
  }
  $l.Stop()
}
try {
  $login = "http://127.0.0.1:$port/login.html"
  Send-Flag $ctx 'rivet-tab.txt' "navigate $login" | Out-Null; Start-Sleep 3
  $null = Get-Capture $ctx (Join-Path $shots 'pw-0-form.png')
  # The page starts below the bar: the panel's top plus the bar's row.
  $top = ($panelY + 44)
  $ux = [int](($panelX + 200) * $f); $uy = [int](($top + 20 + 30) * $f); $py = [int](($top + 20 + 60 + 20 + 30) * $f)
  foreach ($pair in @(@($uy, 'rivet-test-user'), @($py, 'fake-pass-1234'))) {
    Post-Client $ctx 0x201 1 $ux $pair[0]; Start-Sleep -Milliseconds 150; Post-Client $ctx 0x202 0 $ux $pair[0]; Start-Sleep 1
    Post-Text $ctx $pair[1]
  }
  $null = Get-Capture $ctx (Join-Path $shots 'pw-1-typed.png')
  # Enter in the password field submits the form: the key and its character, as a keyboard sends them.
  [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x100, [IntPtr]0x0D, [IntPtr]1); [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x102, [IntPtr]0x0D, [IntPtr]1); [void][FeatureWin]::PostMessage($ctx.Hwnd, 0x101, [IntPtr]0x0D, [IntPtr](0xC0000001 -as [int])); Start-Sleep 4
  Urls 'after sign in'
  $r = New-Object FeatureWin+R; [void][FeatureWin]::GetWindowRect($ctx.Hwnd, [ref]$r)
  "window at $($r.L),$($r.T) to $($r.Ri),$($r.B)"
  $wins = @(Show-Windows $ctx); $wins
  $null = Get-Capture $ctx (Join-Path $shots 'pw-2-after.png')
  $n = 0
  foreach ($line in $wins) { $h = [IntPtr][Convert]::ToInt64(($line -split ' ')[0], 16); if ($h -ne $ctx.Hwnd) { $n++; $null = Get-Capture ([pscustomobject]@{ Hwnd = $h }) (Join-Path $shots "pw-3-bubble-$n.png"); $script:bubble = $h } }
  if ($script:bubble) {
    # The bubble's Save button, at the place its capture shows it: a posted click, the pointer stays put.
    $save = (370 -shl 16) -bor 401
    [void][FeatureWin]::PostMessage($script:bubble, 0x200, [IntPtr]0, [IntPtr]$save); [void][FeatureWin]::PostMessage($script:bubble, 0x201, [IntPtr]1, [IntPtr]$save); Start-Sleep -Milliseconds 150; [void][FeatureWin]::PostMessage($script:bubble, 0x202, [IntPtr]0, [IntPtr]$save)
    Start-Sleep 2
    "after save windows:"; Show-Windows $ctx
  }
  Send-Flag $ctx 'rivet-tab.txt' "navigate $login" | Out-Null; Start-Sleep 4
  $cap = Get-Capture $ctx (Join-Path $shots 'pw-4-revisit.png')
  $green = 0; for ($i = 0; $i -lt $cap.Pixels.Length; $i += 7) { $px = $cap.Pixels[$i]; if (((($px -shr 16) -band 255) -lt 40) -and ((($px -shr 8) -band 255) -gt 170) -and (($px -band 255) -lt 40)) { $green++ } }
  "autofilled green pixels: $green"
} finally { Stop-Job $server -ErrorAction SilentlyContinue; Remove-Job $server -Force -ErrorAction SilentlyContinue }
