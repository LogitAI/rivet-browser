#!/usr/bin/env bash
# Fails if a source file other than the paths module finds Rivet's data folder or the OS temp
# folder on its own. One module builds every path Rivet writes, so the layout cannot drift.
# Also fails if the runtime names a browser's own profile folder: it reads only the profile the
# session was started with, from `--user-data-dir`, and never guesses the person's own profile.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

allowed="rivet/core/src/paths.rs"
found="$(git ls-files 'rivet/*.rs' | grep -v "^$allowed\$" \
    | xargs grep -nE '"LOCALAPPDATA"|temp_dir\(\)' || true)"
if [[ -n "$found" ]]; then
    echo "error: only $allowed may read LOCALAPPDATA or call temp_dir():" >&2
    echo "$found" >&2
    exit 1
fi
echo "ok    every Rivet path is built by $allowed"

guessed="$(git ls-files 'rivet/runtime/*.rs' | xargs grep -nE 'User Data|APPDATA' || true)"
if [[ -n "$guessed" ]]; then
    echo "error: the runtime must read only the session's --user-data-dir:" >&2
    echo "$guessed" >&2
    exit 1
fi
echo "ok    the runtime reads only the session's --user-data-dir"
