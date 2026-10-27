param(
    [Parameter(Mandatory = $true)]
    [string]$Browser,
    [string]$Url = "https://example.com/",
    [string]$StatePath,
    [string]$CargoPath
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Browser -PathType Leaf)) {
    throw "Browser executable was not found: $Browser"
}

$rivet = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
if (-not $StatePath) {
    $StatePath = Join-Path $rivet ".scratch\rivet-ui-demo\sessions.json"
}
if (-not $CargoPath) {
    if ($env:CARGO_HOME) {
        $CargoPath = Join-Path $env:CARGO_HOME "bin\cargo.exe"
    } else {
        $CargoPath = "cargo"
    }
}

$stateDirectory = Split-Path -Parent $StatePath
New-Item -ItemType Directory -Force -Path $stateDirectory | Out-Null

Push-Location $rivet
try {
    & $CargoPath build -p rivet-runtime-windows
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & $CargoPath build -p rivet
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $runtime = Join-Path $rivet "target\debug\rivet_runtime_windows.dll"
    # The installer puts the multi-size icon beside the runtime. The taskbar reads its sharp
    # frames from that file, so the demo places it the same way.
    Copy-Item (Join-Path $rivet "installer\windows\setup-ui\assets\logo.ico") (Join-Path $rivet "target\debug\rivet.ico")
    & $CargoPath run -p rivet -- session start --browser $Browser --url $Url --runtime $runtime --state $StatePath
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
