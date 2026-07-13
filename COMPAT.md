# Compatibility

**Conformance target: [tobi/try](https://github.com/tobi/try) v1.9.3**
(commit `08ca3e7abc5e0015c9af38d9a2ff65d4fc3c3950`).

The bar is byte-level: upstream's own 37-test conformance suite — adopted
verbatim into [`spec/tests/`](spec/tests/) — passes **unmodified** (387/387
assertions), the fuzzy scorer matches upstream Ruby **bit-for-bit**
(`f64::to_bits` equality over a 180-row oracle generated from upstream's
actual `lib/fuzzy.rb`), and rendered frames byte-match committed goldens
captured from upstream. The tmux interaction suite scores **36/37 — identical
to upstream Ruby's own result** (see the ruling log below).

## What "compatible" means here

- The emitted shell scripts (stdout), all TUI frames (stderr), exit codes
  (0 = eval, 1 = cancel/error, 2 = bare help), env vars (`TRY_PATH`,
  `TRY_PROJECTS`, `NO_COLOR`, `NO_COLORS`, `TRY_WIDTH`, `TRY_HEIGHT`), the
  wrapper functions emitted by `init`/`install`, directory naming and
  collision versioning, and the fuzzy ranking math are all upstream's,
  quirks included.
- Where upstream's prose specs contradict its code, **code wins** — every
  such ruling is logged in
  [ADR-0003](docs/adr/0003-divergence-authority.md), including:
  - vim-nav Ctrl-J/K does not exist in the shipped code (test_13 passes
    vacuously; the quirk is ported, not "fixed")
  - the fuzzy density/length multipliers apply to the *entire* score, and an
    empty query returns the raw base score before both
  - the fuzzy result limit reads the controlling tty and ignores
    `TRY_HEIGHT`
  - the tmux rename test asserts 📝 while the code renders ✏️ — upstream
    fails its own test; so do we, identically
- Deliberate, recorded divergences (interactive-only, not observable by the
  suite): the port degrades gracefully where Ruby crashes (symlink cycles,
  non-UTF-8 names), and delete-error status lines carry Rust's OS error text.

## Names

Upstream's binary is `try` (Homebrew core). This port ships as package
`try-me-maybe`, binary `tryme` — zero PATH collision — and the shell
function you actually type is still `try` (emitted by `tryme init`).
