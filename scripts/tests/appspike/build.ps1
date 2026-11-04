$root = Join-Path (Get-Location) 'rivet'
$f = Join-Path $root 'core\src\lib.rs'
$orig = [IO.File]::ReadAllText($f)
$flags = @('--disable-sync', '--no-first-run', '--no-default-browser-check', '--disable-features=msImplicitSignin,msEdgeSignIn,CalculateNativeWinOcclusion,Translate', '--lang=en-US', '--window-position=9000,1000', '--window-size=1600,1000')
$push = ($flags | ForEach-Object { "arguments.push(`"$_`".to_owned());" }) -join ' '
$marker = 'arguments.push(format!("--app={}", request.url));'
[IO.File]::WriteAllText($f, $orig.Replace($marker, "$push $marker"))
Push-Location $root
try { $out = & cargo build -q 2>&1; $code = $LASTEXITCODE; if ($code -ne 0) { $out | Where-Object { $_ -match 'error' -or $_ -match '^\s+-->' } | Select-Object -First 40 | ForEach-Object { Write-Host $_ } } } finally { Pop-Location; [IO.File]::WriteAllText($f, $orig) }
"build exit $code"
