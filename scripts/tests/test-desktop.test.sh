#!/usr/bin/env bash
# nfr5: fails if a test runner can start a browser on the person's own desktop.
#
# A runner that starts a browser, through `rivet session start`, the launcher or
# Start-TestBrowser, must first move itself onto a desktop of its own with Invoke-OnOwnDesktop
# (windows-test-desktop.ps1). There no test window can show on the person's screen, take the
# foreground or hold the mouse. Libraries that only define how to start a browser, and appspike
# scenarios that run inside appspike-drv.ps1, are not runners.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

libraries=(
    scripts/tests/windows-test-safety.ps1
    scripts/tests/windows-test-desktop.ps1
    scripts/tests/windows-processes.ps1
    scripts/tests/windows-features.selftest.ps1
    scripts/tests/appspike/appspike-lib.ps1
)

failed=0
while IFS= read -r file; do
    for library in "${libraries[@]}"; do
        [[ "$file" == "$library" ]] && continue 2
    done
    # A scenario that appspike-drv.ps1 runs inside its own desktop.
    if grep -q 'spike-common.ps1' "$file"; then
        continue
    fi
    if ! grep -q 'Invoke-OnOwnDesktop' "$file"; then
        echo "error: $file starts a browser without moving onto a desktop of its own" >&2
        failed=1
    fi
done < <(git grep -l -I -E 'Start-TestBrowser|session start|rivet-launcher' -- 'scripts/tests/*.ps1' | sort -u)

# On a desktop of its own every process gets an input indicator window, which .NET may report as the
# browser's main window. Runners find the browser frame with Get-BrowserWindow instead.
if hits="$(git grep -n -I 'MainWindowHandle' -- 'scripts/tests/*.ps1')"; then
    echo "error: a runner reads MainWindowHandle, which can be the input indicator window" >&2
    echo "$hits" >&2
    failed=1
fi

if (( failed )); then
    exit 1
fi
echo "ok    every test runner that starts a browser runs on a desktop of its own"
