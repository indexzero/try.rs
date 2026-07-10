# try.rs

A Rust port of [tobi/try](https://github.com/tobi/try) — instant, fuzzy-searchable,
dated experiment directories.

> **Status: pre-0.1, M0 skeleton.** Conformance target: **tobi/try v1.9.3**, pinned by
> upstream's own 37-test suite adopted verbatim at [`spec/tests/`](spec/tests/)
> (provenance: [`spec/UPSTREAM`](spec/UPSTREAM)). The port's bar is passing that suite
> **unmodified**, byte for byte.

- **Package:** `try-me-maybe` · **Binary:** `tryme` · **Your command:** `try` (the shell
  function emitted by `tryme init`) — see [ADR-0001](docs/adr/0001-naming.md)
- **Decisions:** [`docs/adr/`](docs/adr/)

## Development

```sh
mise run ci           # pre-push gate: fmt + clippy + test + smoke
mise run conformance  # adopted upstream suite against the release binary (from M1)
```

Milestones: M0 skeleton → M1 script core (non-TUI conformance green) → M2 selector +
fuzzy + TUI (37/37) → M3 ship 0.1.0 (dist/brew/binstall/crates.io) → M4 nix parity →
M5 1.0.0.

## License

[MIT](LICENSE). Upstream tobi/try is MIT; the adopted suite under `spec/tests/` retains
upstream's copyright.
