# ADR-0005: tmux interaction suite — local + Linux-required at 1.0, macOS advisory

- **Status:** Accepted
- **Date:** 2026-07-10
- **Deciders:** indexzero (owner interview)

## Context

The tmux suite drives a real pty and catches bugs the grep-based conformance
suite structurally cannot (it caught the buffered-stdin escape-sequence bug in
M2). But tmux on macOS GitHub runners is notoriously flaky, and the suite's
own rename test fails against upstream itself (ADR-0003 ruling).

## Decision

- Pre-1.0: run locally as part of milestone gates; expected score is **36/37 —
  identical to upstream Ruby's own result**.
- At 1.0: a required Linux CI job; macOS stays advisory (non-blocking).

## Revisit when

GitHub macOS runners stop flaking on tmux, or upstream fixes its rename test.
