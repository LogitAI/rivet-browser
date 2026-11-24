#!/usr/bin/env bash
# The scheduled job tests the row finder on each new browser build, and it only runs from the
# default branch, so nothing shows a broken one until a build is already missed. These are the
# promises it must keep.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repo_root/.github/workflows/browser-rows.yml"
failed=0

expect() {
    if ! grep -qF -- "$2" "$workflow"; then
        echo "browser-rows.yml: $1: no '$2'" >&2
        failed=1
    fi
}

expect "runs on a schedule" "schedule:"
expect "watches Chrome's version history" "versionhistory.googleapis.com/v1/chrome/platforms/win64/channels"
expect "reads versions at any rollout fraction" "filter=endtime=none"
expect "watches Edge" "edgeupdates.microsoft.com/api/products"
expect "watches Brave" "repos/brave/brave-browser/releases"
expect "runs the finder on one build" 'chrome-builds.exe find "${{ steps.build.outputs.module }}"'
expect "uploads the finder's report" 'finder-${{ matrix.build.browser }}'
expect "scores the finder against verified rows" "scripts/tests/row-score.ps1"
expect "only reads the repository" "contents: read"

# The job tests the finder. It makes, signs and publishes no row, and opens no pull request.
for forbidden in "table-sign" "table-add" "gh pr create" "gh release upload" "RIVET_TABLE_SIGNING_KEY" "contents: write" "pull-requests: write" "issues: write"; do
    if grep -qF -- "$forbidden" "$workflow"; then
        echo "browser-rows.yml: the test job must not use '$forbidden'" >&2
        failed=1
    fi
done

if [[ -e "$repo_root/.github/workflows/chrome-support.yml" ]]; then
    echo "chrome-support.yml covers Chrome only and is replaced by browser-rows.yml" >&2
    failed=1
fi
for script in fetch-browser; do
    if [[ ! -f "$repo_root/.github/scripts/rows/$script.ps1" ]]; then
        echo "the job needs .github/scripts/rows/$script.ps1" >&2
        failed=1
    fi
done
build="$repo_root/rivet/installer/windows/build.ps1"
for promise in RIVET_TABLE_PUBLIC_KEY RIVET_TABLE_URL "releases/download/builds-table/table.bin"; do
    if ! grep -qF -- "$promise" "$build"; then
        echo "build.ps1: a release must trust the published table: no '$promise'" >&2
        failed=1
    fi
done
if grep -qF -- "-AsArray -InputObject" "$workflow"; then
    echo "browser-rows.yml: -AsArray -InputObject makes a list inside a list, and the find job then has no build" >&2
    failed=1
fi
exit "$failed"
