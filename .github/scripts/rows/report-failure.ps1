param(
    # The issue title, one per build or per job, so a failure that repeats lands on one issue.
    [Parameter(Mandatory = $true)][string]$Title,
    # A file of `step: reason` lines the steps wrote when they failed. It may be missing.
    [string]$Reasons = '',
    # The failing capture of the row proof. It may be missing.
    [string]$Capture = '',
    # Where the job's files are kept, for the person who looks into it.
    [string]$Files = ''
)

$ErrorActionPreference = 'Stop'
# Opens or updates the issue of a failed row job, so a failure is never silent. A person is told
# which step failed and why, and sees the capture of the failing window.

$run = "$env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"
$lines = @()
if ($Reasons -and (Test-Path $Reasons)) { $lines = @(Get-Content $Reasons | Where-Object { $_.Trim() }) }
if ($lines.Count -eq 0) { $lines = @('A step failed without recording its reason. The job log has it.') }
$body = @("Run: $run", '', 'Reason:', '') + ($lines | ForEach-Object { "- $_" })
if ($Files) { $body += @('', "Files of this job, kept for 30 days: $Files") }

# An issue cannot hold an upload made through the API, so the capture is a file of a prerelease that
# only holds failure captures. The table release stays as Rivet downloads it.
if ($Capture -and (Test-Path $Capture)) {
    $tag = 'row-failures'
    gh release view $tag 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
        gh release create $tag --title 'Row failure captures' --notes 'Captures of failed row proofs, linked from their issues.' --prerelease --latest=false | Out-Null
    }
    $slug = ($Title -replace '[^A-Za-z0-9.]+', '-').Trim('-').ToLower()
    $name = "$slug-$env:GITHUB_RUN_ID.png"
    $copy = Join-Path (Split-Path $Capture) $name
    Copy-Item $Capture $copy -Force
    gh release upload $tag $copy --clobber | Out-Null
    if ($LASTEXITCODE -eq 0) {
        $body += @('', 'Capture of the failing window:', '', "![failing capture]($env:GITHUB_SERVER_URL/$env:GITHUB_REPOSITORY/releases/download/$tag/$name)")
    } else {
        $body += @('', 'The capture could not be uploaded. It is in the job files.')
    }
}
$text = $body -join "`n"

gh label create browser-rows --color B60205 --description 'A failure of the browser rows job' --force | Out-Null
# The search matches words, so the exact title is checked here.
$open = @(gh issue list --state open --label browser-rows --limit 100 --json number,title | ConvertFrom-Json |
    Where-Object title -eq $Title)
if ($open.Count -gt 0) {
    gh issue comment $open[0].number --body $text
} else {
    gh issue create --title $Title --label browser-rows --body $text
}
if ($LASTEXITCODE -ne 0) { throw "Could not open or update the issue '$Title'" }
