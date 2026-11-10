# The feature tests build Rivet with browser switches that turn sync and sign-in off. A package made
# from such a build would start a person's browser without sync, so build.ps1 refuses it. These are
# the switches only that test build adds; the browser start a person gets never carries them.
$script:TestOnlySwitches = @('--disable-sync', 'msImplicitSignin,msEdgeSignIn,CalculateNativeWinOcclusion')

# The files among `$Files` that carry a test-only browser switch.
function Get-TestFlagBinaries([string[]]$Files) {
  foreach ($file in $Files) {
    $text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($file))
    if (@($script:TestOnlySwitches | Where-Object { $text.Contains($_) }).Count -gt 0) { $file }
  }
}
