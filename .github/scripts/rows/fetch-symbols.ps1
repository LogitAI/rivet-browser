param(
    [Parameter(Mandatory = $true)][string]$Browser,
    [Parameter(Mandatory = $true)][string]$Key,
    [Parameter(Mandatory = $true)][string]$Out,
    # Prints the address without fetching it.
    [switch]$ResolveOnly
)

$ErrorActionPreference = 'Stop'
# Each maker publishes its symbols in its own place. Brave keeps them gzipped. These are only for
# making rows on the build machine: a Rivet on a person's computer never downloads a PDB.
$url, $packed = switch ($Browser) {
    'Google Chrome' { "https://chromium-browser-symsrv.commondatastorage.googleapis.com/chrome.dll.pdb/$Key/chrome.dll.pdb", $false }
    'Microsoft Edge' { "https://msdl.microsoft.com/download/symbols/msedge.dll.pdb/$Key/msedge.dll.pdb", $false }
    'Brave' { "https://brave-symbols.s3.brave.com/chrome.dll.pdb/$Key/chrome.dll.pdb", $true }
    default { throw "No public symbols are known for $Browser" }
}
if ($ResolveOnly) { $url; return }
$target = if ($packed) { "$Out.gz" } else { $Out }
curl.exe -fsSL --retry 3 -o $target $url
if ($LASTEXITCODE -ne 0) { throw "The symbol server has no PDB at $url yet" }
if ($packed) {
    $in = [IO.File]::OpenRead($target)
    $gzip = [IO.Compression.GZipStream]::new($in, [IO.Compression.CompressionMode]::Decompress)
    $to = [IO.File]::Create($Out)
    $gzip.CopyTo($to)
    $to.Dispose(); $gzip.Dispose(); $in.Dispose()
    Remove-Item $target
}
