#!/usr/bin/env bash
# Runs every repository check under scripts/tests/. CI runs this; run it before pushing.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failed=()
tests=(
    scripts/tests/planning-docs.test.sh
    scripts/tests/readme-links.test.sh
    scripts/tests/no-em-dash.test.sh
    scripts/tests/sync-release.test.sh
)
for test in "${tests[@]}"; do
    echo "== $test"
    if ! bash "$test"; then
        failed+=("$test")
    fi
done

if (( ${#failed[@]} )); then
    echo
    echo "failed:" >&2
    printf '  %s\n' "${failed[@]}" >&2
    exit 1
fi
echo
echo "all checks passed"
