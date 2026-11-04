#!/usr/bin/env bash
# Usage: ./.github/scripts/release/new-repo-init.sh [--push] [--source BRANCH] TARGET_REMOTE
#
# Starts a new repository from this one's develop tree, with no history.
#
#   1. One orphan commit holds develop's tree exactly as it is now. It becomes the
#      new repository's `develop`.
#   2. sync-release.sh, taken from that same tree, builds `release` from it in the
#      usual way (planning documents removed). `main` starts at the same commit, so
#      the first release -> main pull request has nothing to merge.
#   3. With --push, the three branches are pushed to TARGET_REMOTE, which must be
#      an empty repository. Without it, this is a dry run that builds everything in
#      a scratch repository, prints what it would push, and pushes nothing.
#
#   TARGET_REMOTE      URL or path of the new, empty repository
#   --source BRANCH    branch of this repository to take the tree from (default: develop)
#   --push             push develop, release and main to TARGET_REMOTE
#
# The work happens in a scratch repository under $TMPDIR. This repository's refs,
# index and working tree are never changed, and nothing of its history is copied:
# the scratch repository receives only the objects of one tree.

set -euo pipefail

push=0
source_branch="develop"
target=""

while (( $# )); do
    case "$1" in
        --push) push=1 ;;
        --source) shift; source_branch="${1:-}" ;;
        -h|--help) sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "usage: $0 [--push] [--source BRANCH] TARGET_REMOTE" >&2; exit 2 ;;
        *) [[ -z "$target" ]] || { echo "error: only one TARGET_REMOTE" >&2; exit 2; }; target="$1" ;;
    esac
    shift
done
[[ -n "$target" && -n "$source_branch" ]] \
    || { echo "usage: $0 [--push] [--source BRANCH] TARGET_REMOTE" >&2; exit 2; }

git rev-parse --git-dir >/dev/null 2>&1 \
    || { echo "error: not inside a git repository" >&2; exit 1; }
git rev-parse --verify -q "refs/heads/$source_branch" >/dev/null \
    || { echo "error: no such branch: $source_branch" >&2; exit 1; }

source_repo="$(git rev-parse --absolute-git-dir)"
source_commit="$(git rev-parse "refs/heads/$source_branch")"
short_source="$(git rev-parse --short "$source_commit")"
tree="$(git rev-parse "$source_commit^{tree}")"

if (( push )) && [[ -n "$(git ls-remote --heads "$target" 2>/dev/null)" ]]; then
    echo "error: $target already has branches; it must be an empty repository" >&2
    exit 1
fi

scratch="$(mktemp -d -t rivet-new-repo.XXXXXX)"
trap 'rm -rf "$scratch"' EXIT

git init -q "$scratch"
cd "$scratch"
git config user.name "$(git -C "$source_repo" config user.name || echo Rivet)"
git config user.email "$(git -C "$source_repo" config user.email || echo rivet@users.noreply.github.com)"

# Fetch the source commit into a temporary ref, write the orphan commit from its tree,
# then drop the ref and prune, so only the objects of that one tree stay here.
git fetch -q --no-tags "$source_repo" "+$source_commit:refs/rivet/source"
message="Start Rivet Browser"
message+=$'\n\n'
message+="The develop tree, without its earlier history."
orphan="$(git commit-tree "$tree" -m "$message")"
git update-ref refs/heads/develop "$orphan"
git update-ref -d refs/rivet/source
git reflog expire --expire=now --all
git gc -q --prune=now

# HEAD points at develop with an empty index and no files on disk. That reads as
# every file deleted, so point HEAD elsewhere for sync-release.sh's clean check.
git symbolic-ref HEAD refs/heads/rivet-unborn

# Use the release script of the tree being published, so the new repository starts
# with exactly the rules its own develop carries.
git show "develop:.github/scripts/release/sync-release.sh" > "$scratch.sync-release.sh"
trap 'rm -rf "$scratch" "$scratch.sync-release.sh"' EXIT
bash "$scratch.sync-release.sh" --write --source develop --target release >/dev/null
git update-ref refs/heads/main "$(git rev-parse refs/heads/release)"

echo "source:  $source_branch ($short_source)"
echo "develop: $(git rev-parse --short develop) (orphan, $(git ls-tree -r --name-only develop | wc -l | tr -d ' ') files)"
echo "release: $(git rev-parse --short release) ($(git ls-tree -r --name-only release | wc -l | tr -d ' ') files)"
echo "main:    $(git rev-parse --short main) (same commit as release)"
echo "commits reachable: $(git rev-list --all | wc -l | tr -d ' ')"

if (( ! push )); then
    echo
    echo "This was a dry run. Nothing was pushed."
    echo "Re-run with --push to create develop, release and main in $target."
    exit 0
fi

git push -q "$target" develop release main
echo
echo "Pushed develop, release and main to $target."
echo "Next: make develop the default branch, protect main, and add the secrets the workflows name."
