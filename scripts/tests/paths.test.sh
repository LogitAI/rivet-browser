#!/usr/bin/env bash
# Fails if a source file other than the paths module finds Rivet's data folder or the OS temp
# folder on its own. One module builds every path Rivet writes, so the layout cannot drift.
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
