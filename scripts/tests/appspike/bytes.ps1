param([string]$Dll, [string[]]$Rvas, [int]$Count = 64)
$fs = [IO.File]::Open($Dll, 'Open', 'Read', 'ReadWrite')
$br = New-Object IO.BinaryReader $fs
$fs.Position = 0x3c; $pe = $br.ReadInt32()
$fs.Position = $pe + 6; $n = $br.ReadUInt16()
$fs.Position = $pe + 20; $opt = $br.ReadUInt16()
$sec = $pe + 24 + $opt
$secs = @()
for ($i = 0; $i -lt $n; $i++) { $fs.Position = $sec + 40 * $i + 8; $vs = $br.ReadUInt32(); $va = $br.ReadUInt32(); $rs = $br.ReadUInt32(); $ro = $br.ReadUInt32(); $secs += [pscustomobject]@{ va = $va; vs = $vs; ro = $ro } }
foreach ($r in $Rvas) {
  $rva = [Convert]::ToUInt32($r, 16)
  $s = $secs | Where-Object { $rva -ge $_.va -and $rva -lt $_.va + $_.vs } | Select-Object -First 1
  $fs.Position = $s.ro + ($rva - $s.va)
  $b = $br.ReadBytes($Count)
  "$r : " + (($b | ForEach-Object { '{0:X2}' -f $_ }) -join ' ')
}
$fs.Close()
