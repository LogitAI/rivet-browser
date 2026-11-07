#!/usr/bin/env bash
# Keeps the feature layer of the Windows runtime off Windows calls and off Chromium internals.
#
# A file under rivet/runtime/windows/src/features/ may use only the traits of the operating
# system layer and the browser access layer. It must not name `windows_sys`, write `unsafe` or
# read a build row. Most features still do all three, so the current counts are recorded in
# scripts/tests/layers.baseline. A count may go down as features move behind the traits, but it
# may never go up, and a file that is not in the baseline must have none.
#
# Usage: layers.test.sh            check the counts against the baseline
#        layers.test.sh --update   write the current counts as the new baseline
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

features="rivet/runtime/windows/src/features"
baseline="scripts/tests/layers.baseline"

# One pattern per kind of violation. Comment lines are left out before matching.
kinds=(windows_sys unsafe rows)
pattern_windows_sys='windows_sys'
pattern_unsafe='\bunsafe\b'
pattern_rows='builds::|row_data|\bbv\(\)|\brow\(\)|\bBuild\b'

pattern_of() {
    local name="pattern_$1"
    printf '%s' "${!name}"
}

# Counts the lines of standard input that match one kind, ignoring comment lines.
count_kind() {
    grep -vE '^[[:space:]]*//' | grep -cE "$(pattern_of "$1")" || true
}

# Checks the counter itself on a small sample, so a broken pattern cannot pass every file.
self_check() {
    local sample
    sample=$'use windows_sys::Win32::Foundation::HWND;\n'
    sample+=$'unsafe fn f() {}\n'
    sample+=$'let x = unsafe { g() };\n'
    sample+=$'// unsafe in a comment does not count\n'
    sample+=$'fn bv() -> &\'static crate::builds::Tabs { todo!() }\n'
    sample+=$'let y = bv().tab_at;\n'
    sample+=$'fn needs(build: &Build) {}\n'
    sample+=$'let unsafely = 1; let Builder = 2;\n'
    local expected=(1 2 3)
    local index=0
    for kind in "${kinds[@]}"; do
        local got
        got="$(printf '%s' "$sample" | count_kind "$kind")"
        if [[ "$got" != "${expected[$index]}" ]]; then
            echo "error: the $kind counter found $got lines in the sample, expected ${expected[$index]}" >&2
            exit 1
        fi
        index=$((index + 1))
    done
}

# Prints "<file> <kind> <count>" for every non-zero count, sorted.
current_counts() {
    git ls-files --cached --others --exclude-standard -- "$features/*.rs" | sort -u | while read -r file; do
        [[ -f "$file" ]] || continue
        for kind in "${kinds[@]}"; do
            local count
            count="$(count_kind "$kind" <"$file")"
            if [[ "$count" != "0" ]]; then
                echo "$file $kind $count"
            fi
        done
    done | sort
}

self_check
counts="$(current_counts)"

if [[ "${1:-}" == "--update" ]]; then
    {
        echo "# Feature layer violations per file: <file> <kind> <count>."
        echo "# Written by scripts/tests/layers.test.sh --update. Counts may only go down."
        printf '%s\n' "$counts"
    } >"$baseline"
    echo "ok    wrote $baseline"
    exit 0
fi

if [[ ! -f "$baseline" ]]; then
    echo "error: $baseline is missing; run $0 --update" >&2
    exit 1
fi

failed=0
lowered=0
while read -r file kind count; do
    [[ -n "$file" ]] || continue
    allowed="$(awk -v f="$file" -v k="$kind" '$1 == f && $2 == k { print $3 }' "$baseline")"
    allowed="${allowed:-0}"
    if (( count > allowed )); then
        echo "error: $file has $count $kind lines, the baseline allows $allowed" >&2
        failed=1
    fi
done <<<"$counts"

while read -r file kind allowed; do
    [[ -n "$file" && "$file" != \#* ]] || continue
    count="$(awk -v f="$file" -v k="$kind" '$1 == f && $2 == k { print $3 }' <<<"$counts")"
    count="${count:-0}"
    if (( count < allowed )); then
        echo "note  $file has $count $kind lines, below its baseline of $allowed"
        lowered=1
    fi
done <"$baseline"

if (( failed )); then
    echo "error: the feature layer must use only the operating system and browser access traits" >&2
    exit 1
fi
if (( lowered )); then
    echo "note  run $0 --update to lower the baseline"
fi
for kind in "${kinds[@]}"; do
    total="$(awk -v k="$kind" '$2 == k { sum += $3 } END { print sum + 0 }' <<<"$counts")"
    echo "ok    $kind lines in features: $total, none above the baseline"
done
