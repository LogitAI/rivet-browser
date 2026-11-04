param([string]$Dll, [string]$Pattern, [int]$Max = 10)
# Pattern: hex bytes separated by spaces, '??' is a wildcard. Reports the RVAs of the matches in executable sections.
$bytes = [IO.File]::ReadAllBytes($Dll)
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
$n = [BitConverter]::ToUInt16($bytes, $pe + 6); $opt = [BitConverter]::ToUInt16($bytes, $pe + 20)
$sec = $pe + 24 + $opt
$tokens = $Pattern -split ' '
$firstReal = 0
$pat = @($tokens | ForEach-Object { if ($_ -eq '??') { -1 } else { [Convert]::ToInt32($_, 16) } })
$first = [byte]$pat[0]
$found = 0
for ($i = 0; $i -lt $n; $i++) {
  $o = $sec + 40 * $i
  $vs = [BitConverter]::ToUInt32($bytes, $o + 8); $va = [BitConverter]::ToUInt32($bytes, $o + 12)
  $rs = [BitConverter]::ToUInt32($bytes, $o + 16); $ro = [BitConverter]::ToUInt32($bytes, $o + 20)
  $ch = [BitConverter]::ToUInt32($bytes, $o + 36)
  if (($ch -band 0x20000000) -eq 0) { continue }
  $start = [int]$ro; $end = [int]($ro + [Math]::Min($rs, $vs)) - $pat.Count
  $pos = $start
  while ($pos -lt $end) {
    $pos = [Array]::IndexOf($bytes, $first, $pos, $end - $pos)
    if ($pos -lt 0) { break }
    $ok = $true
    for ($k = 1; $k -lt $pat.Count; $k++) { if ($pat[$k] -ne -1 -and $bytes[$pos + $k] -ne $pat[$k]) { $ok = $false; break } }
    if ($ok) { '{0:X8}' -f ($va + ($pos - $ro)); $found++; if ($found -ge $Max) { return } }
    $pos++
  }
}
if ($found -eq 0) { 'none' }
