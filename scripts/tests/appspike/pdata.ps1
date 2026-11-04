param([string]$Dll, [string]$From, [string]$To)
# Lists the function starts and sizes from the exception directory (.pdata) of a module, between two RVAs.
$b = [IO.File]::ReadAllBytes($Dll)
$pe = [BitConverter]::ToInt32($b, 0x3c)
$n = [BitConverter]::ToUInt16($b, $pe + 6); $opt = [BitConverter]::ToUInt16($b, $pe + 20)
$rvaTable = [BitConverter]::ToUInt32($b, $pe + 24 + 112 + 3 * 8); $size = [BitConverter]::ToUInt32($b, $pe + 24 + 112 + 3 * 8 + 4)
$sec = $pe + 24 + $opt
$off = $null
for ($i = 0; $i -lt $n; $i++) { $h = $sec + 40 * $i; $va = [BitConverter]::ToUInt32($b, $h + 12); $vs = [BitConverter]::ToUInt32($b, $h + 8); $ro = [BitConverter]::ToUInt32($b, $h + 20); if ($rvaTable -ge $va -and $rvaTable -lt $va + $vs) { $off = $ro + ($rvaTable - $va) } }
$lo = [Convert]::ToUInt32($From, 16); $hi = [Convert]::ToUInt32($To, 16)
for ($k = 0; $k -lt $size / 12; $k++) {
  $s = [BitConverter]::ToUInt32($b, $off + 12 * $k); $e = [BitConverter]::ToUInt32($b, $off + 12 * $k + 4)
  if ($s -ge $lo -and $s -le $hi) { '{0:X8} size {1:X}' -f $s, ($e - $s) }
}
