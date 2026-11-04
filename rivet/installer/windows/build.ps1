# Builds Rivet's package (rivet-<version>.msi) and setup.exe (setup-<version>.exe) into
# rivet\target\installer, the way DSA builds its installer. Needs Rust, the .NET SDK (for the
# setup screens in setup-ui\) and WiX 5:
#
#   dotnet tool install --global wix --version 5.0.2
#
# The bootstrapper extension is downloaded from NuGet into rivet\target\installer and passed
# to WiX by path. (In 5.0.2 its DLL is named WixToolset.BootstrapperApplications.wixext.dll,
# which WiX does not find under the package's name.) setup.exe needs it for the variables
# that the command line may set.
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

# The repository whose `builds-table` release holds the signed table: RIVET_REPO, else the one
# this checkout belongs to.
$tableRepository = if ($env:RIVET_REPO) { $env:RIVET_REPO } else { (& gh repo view --json nameWithOwner --jq .nameWithOwner) }

# A package without these trusts no published table, so it would stop working as soon as the
# browser updates past the rows compiled into it. Refuse to build one.
if (-not $env:RIVET_TABLE_PUBLIC_KEY) {
    $env:RIVET_TABLE_PUBLIC_KEY = (& gh variable get RIVET_TABLE_PUBLIC_KEY --repo $tableRepository)
}
if ($env:RIVET_TABLE_PUBLIC_KEY -notmatch '^[0-9a-fA-F]{128}$') {
    throw "RIVET_TABLE_PUBLIC_KEY must be the public key of the table signing key."
}
if (-not $env:RIVET_TABLE_URL) {
    $env:RIVET_TABLE_URL = "https://github.com/$tableRepository/releases/download/builds-table/table.bin"
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
Copy-Item (Join-Path $rivet "target\release\rivet-launcher.exe") $stage
Copy-Item (Join-Path $rivet "target\release\rivet_runtime_windows.dll") $stage
Copy-Item (Join-Path $rivet "target\release\start.html") $stage
Copy-Item (Join-Path $rivet "target\release\Manrope.ttf") $stage
Copy-Item (Join-Path $installer "setup-ui\assets\logo.ico") (Join-Path $stage "rivet.ico")
Copy-Item (Join-Path $repository "LICENSE") (Join-Path $stage "LICENSE")
Copy-Item (Join-Path $rivet "runtime\windows\fonts\OFL-*.txt") $stage

# The setup screens: a .NET Framework 4.7.2 program that setup.exe carries and starts.
$setupUi = Join-Path $out "setup-ui"
dotnet build (Join-Path $installer "setup-ui\RivetSetup.csproj") -c Release -o $setupUi -p:Version=$Version
if ($LASTEXITCODE -ne 0) { throw "building the setup screens failed" }

$balVersion = "5.0.2"
$balDir = Join-Path $out "ext\bal-$balVersion"
$balDll = Join-Path $balDir "wixext5\WixToolset.BootstrapperApplications.wixext.dll"
if (-not (Test-Path $balDll)) {
    New-Item -ItemType Directory -Force $balDir | Out-Null
    $nupkg = Join-Path $balDir "bal.zip"
    Invoke-WebRequest -Uri "https://api.nuget.org/v3-flatcontainer/wixtoolset.bal.wixext/$balVersion/wixtoolset.bal.wixext.$balVersion.nupkg" -OutFile $nupkg
    Expand-Archive -Path $nupkg -DestinationPath $balDir -Force
    if (-not (Test-Path $balDll)) { throw "the bootstrapper extension has no $balDll" }
}

$msi = Join-Path $out "rivet-$Version.msi"
$setup = Join-Path $out "setup-$Version.exe"
Push-Location $installer
try {
    wix build -arch x64 -d "Version=$Version" -d "SourceDir=$stage" rivet.wxs -o $msi
    if ($LASTEXITCODE -ne 0) { throw "building the Rivet package failed" }
    wix build -arch x64 -ext $balDll -d "Version=$Version" -d "MsiPath=$msi" -d "SetupUiDir=$setupUi" bundle.wxs -o $setup
    if ($LASTEXITCODE -ne 0) { throw "building setup.exe failed" }
} finally {
    Pop-Location
}
Write-Host "built $msi and $setup"
