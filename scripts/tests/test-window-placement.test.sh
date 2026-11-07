#!/usr/bin/env bash
# nfr5: fails if a test runner places or sizes a browser window from the command line.
#
# Chromium gives a window placed or sized by --window-position or --window-size the normal show
# state, and shows a normal window with SW_SHOWNORMAL followed by SetForegroundWindow, whatever
# show command the browser process was started with. A test window would then take the foreground
# from the person using the machine. Test runs keep the window off every monitor through
# RIVET_KEEP_OFFSCREEN instead. A headless browser shows no window, so its size is allowed.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

hits="$(git grep -n -I -e '--window-position=' -e '--window-size=' -- 'scripts/tests/*.ps1' \
    | grep -v -e '--headless' || true)"

if [[ -n "$hits" ]]; then
    echo "error: a test runner places or sizes a browser window; Chromium then activates it" >&2
    echo "$hits" >&2
    exit 1
fi
echo "ok    no test runner places or sizes a browser window"
