# ADR-0004: Staging — exemplar-altitude M1, corpus tooling M3, vetoes never staged

- **Status:** Accepted
- **Date:** 2026-07-10
- **Deciders:** indexzero (owner interview; cross-model review ruling)

## Context

The rust-cli-aspects corpus mandates a full tooling surface (usage spec, dist, hk,
nextest, machete, git-cliff…). The author's shipped exemplar (remarkable-mcp.rs v0.1)
demonstrates a leaner altitude. A judge panel found both extremes lose; the review cycle
surfaced one hard rule: **corpus veto_conditions are not stageable by ADR** (cross-model
reviewer ruling, accepted by owner 2026-07-10).

## Decision

- **M0 (now):** workspace lints, mise tasks, ci.yml gate+msrv, **cargo audit + cargo
  deny** (veto), `[package.metadata.binstall]` (exemplar ships it at v0.1), suite
  adoption into `spec/`, crates.io name reservation.
- **M1:** usage spec via `clap_usage` + committed `try.usage.kdl` + the clap-vs-normalizer
  inventory-diff test (veto; marginal cost ≈ 6 lines per WORKLOG-V2 §3).
- **M3:** dist + tap + attestations, nextest, machete, git-cliff, hk, man page +
  completions from the usage spec, vhs demo.
- **M4:** nix flake (flake-parts + crane) preserving `programs.try.{enable,package,path}`.
- MSRV: **1.95**, set empirically — the provisional 1.88 failed in CI because kdl 6.7.1
  (via clap_usage) requires 1.95; full workspace verified building on 1.95 (2026-07-10).
  Within the corpus N-2/12-month policy (stable is 1.96). `rust-toolchain.toml` arrives
  with dist (its release builds want it).

## Consequences

- **Positive:** vetoes stay meaningful; everything else lands when it pays for itself;
  matches demonstrated practice with recorded reasons.
- **Negative:** M0–M2 lacks nextest speed and hk hooks; contributors rely on `mise run ci`.
- **Mitigations:** `mise run ci` is documented as the pre-push gate; CI enforces it anyway.

## Revisit when

Any staged item blocks a milestone gate, or a new corpus veto lands.
