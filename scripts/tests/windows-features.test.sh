#!/usr/bin/env bash
# The pixel checks behind scripts/tests/windows-features.ps1 run on synthetic windows here, so a
# check that passes on an empty or wrong capture is caught without a browser.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if ! command -v pwsh >/dev/null 2>&1; then
    echo "windows-features: pwsh is not installed, skipped"
    exit 0
fi
pwsh -NoProfile -File "$repo_root/scripts/tests/windows-features.selftest.ps1"
