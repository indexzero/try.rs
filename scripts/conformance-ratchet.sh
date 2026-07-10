#!/usr/bin/env bash
# Shrink-only conformance ratchet: the adopted suite's failure count may
# only go DOWN. Baseline lives in spec/conformance-baseline.txt; lower it
# in the same PR that makes more tests pass. M2's exit gate is 0.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-target/release/tryme}"
BASELINE_FILE=spec/conformance-baseline.txt
baseline=$(cat "$BASELINE_FILE")

# runner.sh exits non-zero while failures remain — expected until 37/37.
out=$( (bash spec/tests/runner.sh "$BIN" 2>&1 || true) | sed 's/\x1b\[[0-9;]*m//g' | tail -5)
failed=$(grep -oE '[0-9]+ tests failed' <<<"$out" | grep -oE '^[0-9]+' || echo 0)
passed_line=$(grep -E 'Results:' <<<"$out" || true)

echo "conformance: $passed_line (failures: $failed, baseline: $baseline)"
if [ "$failed" -gt "$baseline" ]; then
  echo "::error::conformance regressed: $failed failures > baseline $baseline" >&2
  exit 1
fi
if [ "$failed" -lt "$baseline" ]; then
  echo "NOTE: failures dropped to $failed — lower $BASELINE_FILE in this PR to ratchet."
fi
