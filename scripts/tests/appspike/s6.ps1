. scripts/tests/appspike/spike-common.ps1
$port = 18471
$server = Start-Job -ArgumentList $port -ScriptBlock {
  param($port)
  $l = New-Object Net.HttpListener; $l.Prefixes.Add("http://127.0.0.1:$port/"); $l.Start()
  while ($l.IsListening) {
    $c = $l.GetContext(); $p = $c.Request.Url.AbsolutePath; $type = 'text/html'
    $body = switch ($p) {
      '/login' { '<!doctype html><title>login</title><form id=f method=post action=/welcome><input name=user autocomplete=username value=fakeuser@example.test><input name=pass type=password autocomplete=current-password value=Fake-Pass-123><button>go</button></form><script>setTimeout(()=>document.getElementById("f").submit(),1500)</script>' }
      '/welcome' { '<!doctype html><title>welcome</title><body style="background:#2a8"><h1>welcome</h1>' }
      '/perm' { '<!doctype html><title>perm</title><body style="background:#a82"><script>navigator.geolocation.getCurrentPosition(()=>{document.title="geo-allowed"},()=>{document.title="geo-denied"})</script>' }
      '/dl' { '<!doctype html><title>dl</title><body style="background:#82a"><a id=a download=rivet-spike.txt href="/file.txt">x</a><script>setTimeout(()=>document.getElementById("a").click(),1000)</script>' }
      '/file.txt' { $type = 'text/plain'; 'rivet spike download' }
      default { '<!doctype html><title>x</title>' }
    }
    $b = [Text.Encoding]::UTF8.GetBytes($body); $c.Response.ContentType = $type; $c.Response.ContentLength64 = $b.Length
    $c.Response.OutputStream.Write($b, 0, $b.Length); $c.Response.Close()
  }
}
Start-Sleep 2
function Tabs($label) { Send-Flag $ctx 'rivet-tab.txt' 'list' | Out-Null; "$label tabs: " + ((Get-TabUrls) -join ' | ') }
function Shoot-All($tag) {
  $i = 0
  foreach ($line in (Show-Windows $ctx)) {
    $h = [IntPtr][Convert]::ToInt64(($line -split ' ')[0], 16)
    $c2 = [pscustomobject]@{ Hwnd = $h }
    $cap = Get-Capture $c2 (Join-Path $shots "s6-$tag-$i.png")
    "  $tag window $i : $line"
    $i++
  }
}
try {
  foreach ($case in 'login', 'perm', 'dl') {
    "== $case"
    Send-Flag $ctx 'rivet-tab.txt' "navigate http://127.0.0.1:$port/$case" | Out-Null
    Start-Sleep 7
    Tabs $case
    Shoot-All $case
  }
  "== after login submit"
  Send-Flag $ctx 'rivet-tab.txt' "navigate http://127.0.0.1:$port/login" | Out-Null
  Start-Sleep 9
  Tabs 'login-after'
  Shoot-All 'login-after'
  Get-ChildItem "$env:USERPROFILE\Downloads\rivet-spike*.txt" -ErrorAction SilentlyContinue | ForEach-Object { "download landed: $($_.Name)"; Remove-Item $_.FullName }
} finally { Stop-Job $server -ErrorAction SilentlyContinue; Remove-Job $server -Force -ErrorAction SilentlyContinue }
