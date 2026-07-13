#!/usr/bin/env bash
# Regenerate shell completions + man page from the committed usage spec
# (try.usage.kdl). One spec, every artifact — never hand-edit the outputs.
# `--check` verifies freshness without writing (used by hk + CI).
set -euo pipefail
cd "$(dirname "$0")/.."

command -v usage >/dev/null || {
  echo "usage-cli not found — install with: mise use cargo:usage-cli" >&2
  exit 1
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p completions docs/man

# Completions fetch the spec from the installed binary at completion time
# (--usage-cmd) instead of embedding a copy of try.usage.kdl: the shims stay
# frozen across surface changes, and completions always match the binary
# version the user actually runs. The man page still renders from the
# committed spec (it must be buildable without the binary installed).
for shell in bash zsh fish; do
  usage g completion "$shell" tryme --usage-cmd 'tryme --usage-spec' > "$TMP/tryme.$shell"
done
usage g manpage -f try.usage.kdl -o "$TMP/tryme.1" >/dev/null

if [ "${1:-}" = "--check" ]; then
  for f in tryme.bash tryme.zsh tryme.fish; do
    diff -q "completions/$f" "$TMP/$f" >/dev/null || {
      echo "stale: completions/$f (run scripts/gen-cli-docs.sh)" >&2
      exit 1
    }
  done
  diff -q docs/man/tryme.1 "$TMP/tryme.1" >/dev/null || {
    echo "stale: docs/man/tryme.1 (run scripts/gen-cli-docs.sh)" >&2
    exit 1
  }
  echo "CLI docs fresh"
else
  for f in tryme.bash tryme.zsh tryme.fish; do cp "$TMP/$f" "completions/$f"; done
  cp "$TMP/tryme.1" docs/man/tryme.1
  echo "regenerated completions/ and docs/man/tryme.1"
fi
