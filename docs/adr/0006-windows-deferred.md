# ADR-0006: Windows targets deferred

- **Status:** Accepted
- **Date:** 2026-07-10
- **Deciders:** indexzero (plan review cycle)

## Context

The selector's terminal layer is built on `rustix::termios` (raw mode on
stderr, winsize ioctls), which has no Windows support. A native Windows build
requires a crossterm-based terminal layer — a real architecture decision with
byte-parity implications, not a target flag.

## Decision

Ship no Windows targets. `dist` builds darwin (arm64/x86_64) + linux-musl
(arm64/x86_64) only; the powershell installer is omitted (dist skips it with
no Windows artifacts). The PowerShell *wrapper* emitted by `tryme install`
remains, for pwsh-on-unix users.

## Consequences

- **Positive:** no cfg-gated terminal code, no untested platform claims.
- **Negative:** Windows users are limited to WSL.
- **Mitigations:** revisit with a dedicated crossterm ADR if demand appears.

## Revisit when

A contributor wants Windows enough to own the crossterm terminal layer and
its parity story.
