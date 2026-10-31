param(
    [Parameter(Mandatory = $true)][string]$Rivet,
    [Parameter(Mandatory = $true)][string]$Runtime,
    [Parameter(Mandatory = $true)][string]$Browser,
    [Parameter(Mandatory = $true)][string]$Work
)

$ErrorActionPreference = 'Stop'
# Opens a real Rivet window of a browser build the way an installed Rivet does, then measures it.
# The window is never activated and keeps its off-screen place only on a machine with no monitor
# to take it from, so this runs on build machines, not on a person's desktop.
New-Item -ItemType Directory -Force $Work | Out-Null
$state = Join-Path $Work 'sessions.json'
& $Rivet session start --browser $Browser --url about:blank --runtime (Resolve-Path $Runtime) `
    --state $state --user-data-dir (Join-Path $Work 'profile')
if ($LASTEXITCODE -ne 0) { throw 'NFR-3: The session did not start.' }
Start-Sleep -Seconds 12
& (Join-Path $PSScriptRoot 'windows-frame.ps1') -State $state

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;using System.Runtime.InteropServices;
public static class RivetRowProof{
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int ht,uint f);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out R r);
 public struct R{public int L,T,Ri,B;}
}
'@
[RivetRowProof]::SetThreadDpiAwarenessContext([IntPtr]-4) | Out-Null
$session = @((Get-Content -Raw $state | ConvertFrom-Json).sessions.PSObject.Properties.Value)[0]
$handle = (Get-Process -Id $session.pid).MainWindowHandle
if ($handle -eq [IntPtr]::Zero) { throw 'NFR-3: The browser has no window.' }
# SWP_NOACTIVATE | SWP_NOZORDER.
[RivetRowProof]::SetWindowPos($handle, [IntPtr]::Zero, 40, 40, 1280, 800, 0x14) | Out-Null
Start-Sleep -Seconds 2
$rect = New-Object RivetRowProof+R
[RivetRowProof]::GetWindowRect($handle, [ref]$rect) | Out-Null
$bitmap = New-Object Drawing.Bitmap ($rect.Ri - $rect.L), ($rect.B - $rect.T)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$dc = $graphics.GetHdc()
[RivetRowProof]::PrintWindow($handle, $dc, 2) | Out-Null
$graphics.ReleaseHdc($dc)
$graphics.Dispose()
$image = Join-Path $Work 'row.png'
$bitmap.Save($image)
$bitmap.Dispose()
$dpi = [RivetRowProof]::GetDpiForWindow($handle)
try {
    & (Join-Path $PSScriptRoot 'windows-row.ps1') -Image $image -Dpi $dpi
} catch {
    # Leave no image behind: the publish step takes an image as the proof that this row passed.
    Rename-Item $image 'row-failed.png'
    throw
} finally {
    Stop-Process -Id $session.pid -Force -ErrorAction SilentlyContinue
}
