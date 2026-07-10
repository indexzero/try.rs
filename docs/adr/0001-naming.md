# ADR-0001: Naming — package `try-me-maybe`, binary `tryme`, wrapper function `try`

- **Status:** Accepted
- **Date:** 2026-07-10
- **Deciders:** indexzero (owner interview, plan review cycle)

## Context

Verified on crates.io 2026-07-10: `try` is taken (2017 macro crate), `try-cli` is taken
(a competing experiment-navigator, active 2026), and `try-rs` is taken by an **actively
maintained competing Rust port of this exact tool** (v1.7.11, updated 2026-06).
Homebrew-core's `try` formula is upstream tobi/try itself. The conformance suite
(test_14/34/36) hard-asserts the wrapper function name `try` (`try() {`, `function try`,
`type try`).

## Decision

- crates.io package: **`try-me-maybe`** (verified available 2026-07-10; 0.0.0 placeholder
  reserved at M0).
- Binary: **`tryme` in every channel** — cargo `[[bin]]`, dist artifacts, brew, nix,
  binstall (the ccat model: one name everywhere). Zero PATH collision with core `try`.
- Wrapper function emitted by `tryme init`: **`try`** — byte-parity with upstream (the
  suite asserts it), daily muscle memory preserved, and a shell function link-conflicts
  with nothing on disk.
- README documents an optional `try` symlink for direct-binary users (ccat-style).
- Homebrew: tap-only (`indexzero/tap/try-me-maybe`); we never contest the core `try`
  formula — upgrading it to the Rust port is tobi's call, if ever.

## Consequences

- **Positive:** migrating users keep core Ruby try installed with no conflict; the typed
  command is still `try`; the pun is preserved.
- **Negative:** discoverability — `cargo install try-me-maybe` is not guessable; the
  README comparison table carries the burden of positioning against `try-rs`/`try-cli`.
- **Mitigations:** conformance (37/37 unmodified) is the stated differentiator; mise
  registry + brew tap give memorable install paths.

## Revisit when

Upstream blesses a Rust port in the core formula, or a competing port passes the
conformance suite.
