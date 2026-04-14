#!/bin/bash
# Apply release notes from CHANGELOG.md to GitHub releases.
# Usage: ./scripts/apply-releases.sh [changelog-file]
#
# Parses "# vX.Y.Z" headings and updates each GitHub release via `gh release edit`.
# Requires: gh CLI authenticated with repo access.

set -euo pipefail

FILE="${1:-CHANGELOG.md}"

if [[ ! -f "$FILE" ]]; then
  echo "File not found: $FILE" >&2
  exit 1
fi

current_tag=""
current_body=""

flush() {
  if [[ -n "$current_tag" && -n "$current_body" ]]; then
    echo "Updating $current_tag..."
    gh release edit "$current_tag" --notes "$current_body"
  fi
}

while IFS= read -r line; do
  if [[ "$line" =~ ^#\ v([0-9]+\.[0-9]+\.[0-9]+) ]]; then
    flush
    current_tag="v${BASH_REMATCH[1]}"
    current_body=""
  elif [[ "$line" == "---" ]]; then
    continue
  else
    current_body+="$line"$'\n'
  fi
done < "$FILE"

flush

echo "Done."
