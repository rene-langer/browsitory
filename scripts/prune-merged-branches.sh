#!/usr/bin/env bash
# Lists, or with --delete removes, local branches that are already merged into main.
#
# Pull requests are squash-merged, so a merged branch's commits never appear in main's history
# and `git branch --merged` / `git fetch --prune` miss them. A branch counts as merged when
# either
#   - GitHub has a merged pull request whose head was exactly this branch tip, or
#   - the branch tip is already part of origin/main.
# A branch with commits after its merged pull request is kept. So are main, the branch you are
# on, and any branch checked out in a worktree (remove the worktree first).
#
# Usage: scripts/prune-merged-branches.sh [--delete]
# Needs git and an authenticated gh. Without --delete it only prints what it would delete.
set -euo pipefail

delete=false
case "${1:-}" in
  "") ;;
  --delete) delete=true ;;
  -h | --help)
    sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    echo "unknown argument: $1 (use --delete or --help)" >&2
    exit 2
    ;;
esac

git fetch --quiet --prune origin

# "<branch> <head commit>" for every merged pull request.
merged_heads=$(gh pr list --state merged --limit 1000 --json headRefName,headRefOid \
  --jq '.[] | "\(.headRefName) \(.headRefOid)"')

# Branches checked out in any worktree, this one included.
checked_out=$(git worktree list --porcelain | sed -n 's|^branch refs/heads/||p')

candidates=()
while read -r branch tip; do
  [ "$branch" = main ] && continue
  if grep -qxF "$branch" <<<"$checked_out"; then
    continue
  fi
  if grep -qxF "$branch $tip" <<<"$merged_heads" ||
    git merge-base --is-ancestor "$tip" origin/main; then
    candidates+=("$branch")
  fi
done < <(git for-each-ref --format='%(refname:short) %(objectname)' refs/heads)

if [ "${#candidates[@]}" -eq 0 ]; then
  echo "No merged local branches."
  exit 0
fi

printf '%s\n' "${candidates[@]}"
if $delete; then
  git branch --quiet -D "${candidates[@]}"
  echo "Deleted ${#candidates[@]} merged branch(es)."
else
  echo "${#candidates[@]} merged branch(es). Run again with --delete to remove them."
fi
