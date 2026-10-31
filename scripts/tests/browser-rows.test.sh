#!/usr/bin/env bash
# The scheduled job is what keeps Rivet ahead of browser updates, and it only runs from the default
# branch, so nothing shows a broken one until a build is already missed. These are the promises it
# must keep.
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
expect "watches Chrome beta for the next milestone" "/beta/versions"
expect "watches Edge" "edgeupdates.microsoft.com/api/products"
expect "watches Brave" "repos/brave/brave-browser/releases"
expect "proves a row on the real browser" "windows-row-proof.ps1"
expect "does not publish a row that needs review" "steps.made.outputs.for_review == 'false'"
expect "publishes a signed table" "table-sign"
expect "signs with a secret" "secrets.RIVET_TABLE_SIGNING_KEY"

if [[ -e "$repo_root/.github/workflows/chrome-support.yml" ]]; then
    echo "chrome-support.yml covers Chrome only and is replaced by browser-rows.yml" >&2
    failed=1
fi
for script in fetch-browser fetch-symbols; do
    if [[ ! -f "$repo_root/.github/scripts/rows/$script.ps1" ]]; then
        echo "the job needs .github/scripts/rows/$script.ps1" >&2
        failed=1
    fi
done
exit "$failed"
