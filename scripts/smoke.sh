#!/usr/bin/env bash
# Hermetic smoke test: build the real binary, drive it, assert the
# stream/exit contracts. No network, no git remotes, no AI.
# (Exemplar: remarkable-mcp.rs scripts/smoke.sh. Grows with each milestone —
# M1 adds init/clone-dry/help/version flows; selector flows join in M2.)
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> building (debug)"
cargo build --quiet --all

BIN=target/debug/tryme

echo "==> M0: bare invocation exits 2 with stderr-only output"
set +e
out=$("$BIN" 2>/dev/null)
code=$?
err=$("$BIN" 2>&1 >/dev/null)
set -e

if [ "$code" -ne 2 ]; then
  echo "FAIL: expected exit 2, got $code" >&2
  exit 1
fi
if [ -n "$out" ]; then
  echo "FAIL: stdout must be empty on bare invocation (stream contract), got: $out" >&2
  exit 1
fi
if [[ "$err" != tryme* ]]; then
  echo "FAIL: expected stderr to start with 'tryme', got: $err" >&2
  exit 1
fi

echo "✅ smoke: stream + exit contracts hold"
