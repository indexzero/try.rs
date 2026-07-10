#!/usr/bin/env bash
# Re-adopt the conformance suite from a named upstream tag.
#
# Usage: scripts/sync-upstream-spec.sh v1.9.4
#
# The suite (spec/tests/) only ever changes through this script so every
# change is provenance-tracked against an upstream tag. PRs touching
# spec/tests/ must carry the `spec-sync` label (enforced in CI).
set -euo pipefail

TAG="${1:?usage: $0 <upstream-tag>}"
REPO_URL="https://github.com/tobi/try"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

git clone --quiet --depth 1 --branch "$TAG" "$REPO_URL" "$TMP/try"
COMMIT="$(git -C "$TMP/try" rev-parse HEAD)"
DATE="$(git -C "$TMP/try" log -1 --format=%cs)"

rm -rf "$ROOT/spec/tests"
cp -R "$TMP/try/spec/tests" "$ROOT/spec/tests"

sed -i.bak \
  -e "s/^# Tag:.*/# Tag:       $TAG/" \
  -e "s/^# Commit:.*/# Commit:    $COMMIT/" \
  -e "s/^# Date:.*/# Date:      $DATE/" \
  -e "s/^# Adopted:.*/# Adopted:   $(date +%Y-%m-%d)/" \
  -e "s/^tag=.*/tag=$TAG/" \
  -e "s/^commit=.*/commit=$COMMIT/" \
  "$ROOT/spec/UPSTREAM"
rm -f "$ROOT/spec/UPSTREAM.bak"

echo "spec/tests re-adopted from $REPO_URL@$TAG ($COMMIT)"
echo "Review the diff, then commit on a PR labeled 'spec-sync'."
