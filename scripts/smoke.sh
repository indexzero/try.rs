#!/usr/bin/env bash
# Hermetic smoke test: build the real binary, drive it, assert the
# stream/exit contracts. No network, no git remotes, no AI.
# (Selector-driving --and-keys flows join in M2 with the TUI.)
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> building (debug)"
cargo build --quiet --all

BIN=target/debug/tryme
fail() { echo "FAIL: $1" >&2; exit 1; }

echo "==> bare invocation: help on stderr, empty stdout, exit 2"
set +e
out=$("$BIN" 2>/dev/null); code=$?
err=$("$BIN" 2>&1 >/dev/null)
set -e
[ "$code" -eq 2 ] || fail "expected exit 2, got $code"
[ -z "$out" ] || fail "stdout must be empty on bare invocation, got: $out"
[[ "$err" == "try v"* ]] || fail "expected help on stderr, got: ${err:0:60}"

echo "==> --version: stderr only, exit 0"
set +e
out=$("$BIN" --version 2>/dev/null); code=$?
err=$("$BIN" --version 2>&1 >/dev/null)
set -e
[ "$code" -eq 0 ] || fail "--version expected exit 0, got $code"
[ -z "$out" ] || fail "--version stdout must be empty"
[[ "$err" == "try "* ]] || fail "--version format, got: $err"

echo "==> init: wrapper on stdout, explicit path quoted"
out=$(SHELL=/bin/bash "$BIN" init /tmp/smoke-tries 2>/dev/null)
grep -q "try() {" <<<"$out" || fail "init should emit bash function"
grep -qF -- "--path '/tmp/smoke-tries'" <<<"$out" || fail "init explicit --path form"

echo "==> clone (dry): script bytes on stdout, exit 0"
out=$("$BIN" --path /tmp/smoke-tries clone https://github.com/user/repo 2>/dev/null)
head -1 <<<"$out" | grep -q "^# if you can read this" || fail "script warning first line"
grep -q "git clone 'https://github.com/user/repo'" <<<"$out" || fail "clone command bytes"

echo "==> selector path (test mode): Cancelled. on stdout, exit 1"
set +e
out=$("$BIN" --path /tmp/smoke-tries --and-exit exec 2>/dev/null); code=$?
set -e
[ "$code" -eq 1 ] || fail "cancel expected exit 1, got $code"
[ "$out" = "Cancelled." ] || fail "expected Cancelled. on stdout, got: $out"

echo "✅ smoke: stream + exit contracts hold"
