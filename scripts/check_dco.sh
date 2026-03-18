#!/usr/bin/env bash
set -euo pipefail

BASE=${1:-origin/main}

echo "Checking DCO from $BASE to HEAD"

commits=$(git log "$BASE"..HEAD --no-merges --pretty=format:"%H")

if [ -z "$commits" ]; then
  echo "No commits to check"
  exit 0
fi

for c in $commits; do
  if ! git log -1 --pretty=%B "$c" | grep -qi "^Signed-off-by:"; then
    echo "Commit $c is missing Signed-off-by"
    git log -1 --pretty=full "$c"
    exit 1
  fi
done

echo "All commits have Signed-off-by"
