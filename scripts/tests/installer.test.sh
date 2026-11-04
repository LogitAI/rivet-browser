#!/usr/bin/env bash
# Setup must never restart the computer: a button or a command line option there closes everything
# a person has open. A file in use is replaced at their next restart, whenever that is.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
setup="$repo_root/rivet/installer/windows/setup-ui"

if grep -rnE 'RestartButton|AcceptRestart|OnRestart|ACTION\.Restart|RebootInitiated' "$setup" \
    --include='*.cs' --include='*.xaml'; then
    echo "setup can still restart the computer" >&2
    exit 1
fi
echo "ok    setup cannot restart the computer"
