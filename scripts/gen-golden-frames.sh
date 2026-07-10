#!/usr/bin/env bash
# Regenerate testdata/golden/*.bin — byte-exact stderr frames captured from
# upstream Ruby at the tag pinned in spec/UPSTREAM, at 80x24, with fixture
# mtimes set RELATIVE to now so re-runs at any date produce identical bytes.
# The Rust golden test recreates the same fixtures and byte-compares.
#
# TRY_UPSTREAM_URL overrides the clone source.
set -euo pipefail
cd "$(dirname "$0")/.."

TAG=$(grep '^tag=' spec/UPSTREAM | cut -d= -f2)
URL="${TRY_UPSTREAM_URL:-https://github.com/tobi/try}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
git clone --quiet --depth 1 --branch "$TAG" "$URL" "$TMP/try" 2>/dev/null

FX="$TMP/fixtures"
source scripts/golden-fixtures.sh   # defines setup_golden_fixtures FX_DIR
setup_golden_fixtures "$FX"

run_ruby() { # name, args...
  local name=$1; shift
  ( cd "$TMP/try" && TRY_WIDTH=80 TRY_HEIGHT=24 NO_COLOR= NO_COLORS= \
      ruby try.rb "$@" --path "$FX" >/dev/null 2>"$TMP/out.bin" ) || true
  # Template out generation-day dates (the create-new row embeds "today"),
  # binary-safe; the Rust golden test applies the same substitution.
  python3 - "$TMP/out.bin" "testdata/golden/$name.bin" <<'PY'
import sys, time
data = open(sys.argv[1], "rb").read()
data = data.replace(time.strftime("%Y-%m-%d").encode(), b"{TODAY}")
open(sys.argv[2], "wb").write(data)
PY
  echo "  golden: $name.bin ($(wc -c < testdata/golden/$name.bin | tr -d ' ') bytes)"
}

run_ruby empty-query --and-exit exec
run_ruby filtered-beta --and-type beta --and-exit exec
run_ruby no-colors --no-colors --and-exit exec
run_ruby nav-keys --and-keys "DOWN, DOWN, ESC" exec
echo "done (from $URL@$TAG)"
