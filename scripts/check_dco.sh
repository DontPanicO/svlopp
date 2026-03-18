#!/usr/bin/env bash
set -euo pipefail

BASE=${1:-origin/main}

echo "Checking DCO from $BASE to HEAD"

commits=$(git log "$BASE"..HEAD --pretty=format:"%H")

if [ -z "$commits" ]; then
  echo "No commits to check"
  exit 0
fi

for c in $commits; do
  # Skip merge commits
  if [ "$(git rev-list --parents -n 1 "$c" | wc -w)" -gt 2 ]; then
    echo "Skipping merge commit $c"
    continue
  fi

  # It appears that github actions "merge" commits are not really merge
  # commits and they have 1 parent only. I can't find a better solution
  # than match the messagge pattern
  if git log -1 --pretty=%B "$c" | grep -q "^Merge .* into .*"; then
    echo "Skipping synthetic merge commit $c"
    continue
  fi

  if ! git log -1 --pretty=%B "$c" | grep -qi "^Signed-off-by:"; then
    echo "Commit $c is missing Signed-off-by"
    git log -1 --pretty=full "$c"
    exit 1
  fi
done

echo "All commits have Signed-off-by"
