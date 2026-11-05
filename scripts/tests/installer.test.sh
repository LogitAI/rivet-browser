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

# FR-8: the package offers Rivet as a browser for http, https, .htm and .html, with an open command
# that hands the address to the launcher. It never makes Rivet the default: only the person does.
wxs="$repo_root/rivet/installer/windows/rivet.wxs"
for needle in 'SOFTWARE\Clients\StartMenuInternet\RivetBrowser' 'SOFTWARE\RegisteredApplications' \
    'Key="URLAssociations"' 'Name="http"' 'Name="https"' 'Name=".htm"' 'Name=".html"' \
    'rivet-launcher.exe&quot; --open-link &quot;%1&quot;'; do
    if ! grep -qF "$needle" "$wxs"; then
        echo "fr8: the package does not register $needle" >&2
        exit 1
    fi
done
if grep -niE 'UserChoice|SetDefault|DefaultBrowser' "$wxs"; then
    echo "fr8: the package must not choose the default browser for the person" >&2
    exit 1
fi
if [ "$(grep -c 'ForceDeleteOnUninstall="yes"' "$wxs")" -lt 3 ]; then
    echo "fr8: the browser registration keys are not removed on uninstall" >&2
    exit 1
fi
echo "ok    fr8 Rivet is offered as a browser and never made the default by setup"
