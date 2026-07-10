# ADR-0002: Version stream restarts at 0.1.0; conformance claim lives in COMPAT.md

- **Status:** Accepted
- **Date:** 2026-07-10
- **Deciders:** indexzero (plan judge panel + review cycle)

## Context

Upstream is at v1.9.3. A draft plan proposed inheriting that lineage (shipping the port
as 1.9.3) to make the version a conformance claim. Review found this couples our patch
cadence to upstream's and crates.io build-metadata cannot distinguish published versions,
leaving no room for port-local fixes between upstream releases. The conformance suite
greps `try <semver>` format only, never the number.

## Decision

Our release stream starts at **0.1.0** (M3). The conformance claim ("conformance target:
tobi/try v1.9.3") lives in `COMPAT.md` and the README badge, not in semver. The help
text is byte-exact to upstream **except** the first-line version token, which renders our
`CARGO_PKG_VERSION` (golden regeneration templates that token). **1.0.0 gate:** 37/37 +
tmux green on Linux + dist installers live + nix flake parity.

## Consequences

- **Positive:** independent patch cadence; honest semver; placeholder 0.0.0 → 0.1.0 →
  1.0.0 tells the true maturity story.
- **Negative:** version number alone doesn't advertise which upstream we match.
- **Mitigations:** COMPAT.md + README badge carry the target; `spec/UPSTREAM` pins it
  mechanically.

## Revisit when

1.0.0 ships, or upstream's release cadence makes the COMPAT indirection confusing.
