. scripts/tests/appspike/spike-common.ps1
# A normal window exists beside the app window after IDC_NEW_TAB. Their browser objects differ where the type sits.
Add-Type -TypeDefinition 'using System;using System.Runtime.InteropServices;public static class Cmd3{[DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out IntPtr r);}'
$r = [IntPtr]::Zero; [void][Cmd3]::SendMessageTimeout($ctx.Hwnd, 0x111, [IntPtr]34014, [IntPtr]::Zero, 2, 3000, [ref]$r)
Start-Sleep 5
Send-Flag $ctx 'rivet-tab.txt' 'dump' | Out-Null
$lines = Read-Log | Where-Object { $_ -match 'app window dump' }
$map = @{}
foreach ($l in $lines) {
  $own = $l -match 'rivet true'
  $vals = @{}; foreach ($m in [regex]::Matches(($l -replace '^.*: ', ''), '([0-9a-f]+)=(\d)')) { $vals[$m.Groups[1].Value] = [int]$m.Groups[2].Value }
  $map[$(if ($own) { 'app' } else { 'normal' + $map.Count })] = $vals
}
$map.Keys | ForEach-Object { "$_ : $($map[$_].Count) small values" }
$app = $map['app']
foreach ($k in $map.Keys | Where-Object { $_ -like 'normal*' }) {
  $diff = @(); foreach ($o in $app.Keys) { if ($app[$o] -ne 0 -and (-not $map[$k].ContainsKey($o) -or $map[$k][$o] -ne $app[$o])) { $diff += ('{0}:app={1},normal={2}' -f $o, $app[$o], $map[$k][$o]) } }
  "offsets where the app window differs from $k : " + ($diff | Sort-Object { [Convert]::ToInt32(($_ -split ':')[0], 16) }) -join ' '
}


