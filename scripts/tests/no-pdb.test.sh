#!/usr/bin/env bash
# Fails if code, scripts or workflows download or read a browser's debug symbols. Rivet's row finder
# finds every value in the build's own binary by descriptions written from public source: a maker
# can ship a build long before its symbols appear, or never publish them, so nothing may wait on
# them.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

# Searches the tracked code, scripts and workflows. A search that cannot run fails, so a broken
# search never passes as a clean one.
search() {
    local found status
    found="$(git grep -inIE -e "$1" -- rivet scripts .github ':!scripts/tests/no-pdb.test.sh')"
    status=$?
    if (( status > 1 )); then
        echo "error: git grep could not search for $1" >&2
        return 2
    fi
    # The script that runs this test names it, and the name is not a reader.
    printf '%s' "$found" | grep -v 'no-pdb\.test\.sh'
    return 0
}

# A symbol server, the variable that names one, or the path a PDB is asked for by.
servers='symsrv|msdl\.microsoft\.com|brave-symbols|symbols\.mozilla|/download/symbols|_NT_SYMBOL_PATH|srv\*|\.pdb/|\.dll\.pdb'
found="$(search "$servers")" || exit 2
if [[ -n "$found" ]]; then
    echo "error: no code, script or workflow may name a symbol server:" >&2
    echo "$found" >&2
    exit 1
fi
echo "ok    no code, script or workflow names a symbol server"

# DbgHelp and DIA read PDBs, a CodeView record names the PDB of a module, and the `pdb` crate parses
# one. Any of them, or a file named after a PDB, is code that reads symbols.
readers='dbghelp|SymInitialize|SymLoadModule|SymEnumSymbols|SymFromName|SymFromAddr|SymSetOptions|msdia|IDiaDataSource|RSDS|CodeView|pdb_key|pdb-key|(^|[^a-z0-9_])pdb([^a-z0-9_]|$)|fetch-symbols'
found="$(search "$readers")" || exit 2
if [[ -n "$found" ]]; then
    echo "error: no code, script or workflow may read a PDB:" >&2
    echo "$found" >&2
    exit 1
fi
echo "ok    no code, script or workflow reads a PDB"
