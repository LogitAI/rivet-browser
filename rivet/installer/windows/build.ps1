# Builds Rivet's Windows MSI from the DSA installer staging pattern.
param(
    [string]$Version = "0.1.0",
    [string]$CargoPath = ""
)
$ErrorActionPreference = "Stop"

$installer = $PSScriptRoot
$rivet = (Resolve-Path (Join-Path $installer "..\.."))
$repository = (Resolve-Path (Join-Path $rivet ".."))
$out = Join-Path $rivet "target\installer"
$stage = Join-Path $out "stage"

if (-not $CargoPath) {
    if ($env:CARGO_HOME) {
        $CargoPath = Join-Path $env:CARGO_HOME "bin\cargo.exe"
    } else {
        $CargoPath = "cargo"
    }
}
if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
    throw "WiX 5 is required. Install the same WiX CLI DSA uses, then run this script again."
}

Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null

Push-Location $rivet
try {
    & $CargoPath build --release -p rivet -p rivet-runtime-windows
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

Copy-Item (Join-Path $rivet "target\release\rivet.exe") $stage
Copy-Item (Join-Path $rivet "target\release\rivet_runtime_windows.dll") $stage
Copy-Item (Join-Path $repository "LICENSE") (Join-Path $stage "LICENSE")

$msi = Join-Path $out "rivet-$Version.msi"
Push-Location $installer
try {
    wix build -arch x64 -d "Version=$Version" -d "SourceDir=$stage" rivet.wxs -o $msi
    if ($LASTEXITCODE -ne 0) { throw "building the Rivet package failed" }
} finally {
    Pop-Location
}
Write-Host "built $msi"
