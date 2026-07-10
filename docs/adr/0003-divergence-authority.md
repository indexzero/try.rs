# ADR-0003: Divergence authority — code > shipped tests > prose specs

- **Status:** Accepted
- **Date:** 2026-07-10
- **Deciders:** indexzero (plan judge panel + review cycle)

## Context

Upstream's prose specs (`spec/*.md`, `AGENTS.md`) describe design intent that its code
contradicts in verified places. The port needs a standing rule for which source wins,
and a log of each ruling.

## Decision

Authority order: **(1) upstream `try.rb`/`lib/*` code at the pinned tag, (2) the shipped
`spec/tests` suite, (3) prose specs.** Port the code's behavior, quirks included.

## Ruling log

- **vim-nav:** README/tests imply Ctrl-J/K navigation; in code Ctrl-J is unmapped and
  Ctrl-K is `\x0B` kill-line. test_13 passes vacuously (unknown `--and-keys` tokens are
  silently dropped). Port the quirk; do not implement vim nav.
- **Token system:** `UI::TOKEN_MAP`/`expand_tokens` (AGENTS.md) do not exist in code.
  `--no-expand-tokens` is a bare alias of `--no-colors`. Follow code.
- **delete_spec.md** describes a realpath-based `sh -c` delete script that does not
  exist; the real emitted script uses basenames + `test -d` guards. Follow code.
- **Fuzzy scoring:** the density and length multipliers apply to the ENTIRE score
  (base included), not the fuzzy part alone as prose claims; empty query early-returns
  the raw base score BEFORE both multipliers. ctime bonus described in AGENTS.md is not
  in the shipped scorer (mtime only). Follow code.
- **Lowercasing:** Ruby `downcase` expands (`İ` → 2 chars); match positions index the
  expanded string and upstream applies them to original chars, shifting highlights on
  expanding-case input. Port the expansion verbatim.
- **Sort ties:** Ruby's `sort_by!` is unstable; tie order is documented as
  not-guaranteed rather than replicated.
- **Fuzzy result limit** reads the controlling tty directly and ignores `TRY_HEIGHT`
  (`try.rb:167`); only the render viewport honors the env var. Port both as-is.
- **Error-path divergences (recorded, not replicated):** Ruby crashes with a
  traceback where the port degrades gracefully — `ELOOP` symlink cycles in the
  scan (Ruby rescues only ENOENT/EACCES), `File.realpath` failures, non-UTF-8
  directory names (Ruby raises `ArgumentError` on the date regex), and
  `mkdir_p` over an existing file. Deliberate: a selector that survives beats
  byte-parity on crash messages nobody greps.
- **Line-anchor regexes on newline-containing names:** Ruby's `/^…/` matches
  after embedded `\n` in a dirname; the port anchors at string start only.
  Pathological input; recorded rather than replicated.
- **Delete error strings:** the "Error: …" status line renders Rust's OS error
  text, not Ruby's `rb_check_realpath_internal` phrasing. Interactive-only.
- **tmux rename-emoji test:** `test_21_tmux_rename.sh:31` asserts 📝 but the shipped
  code renders ✏️ (`try.rb:597`) — upstream Ruby fails its own test (36/37). The port
  matches the code and reproduces upstream's exact 36/37 result. Broken shipped test;
  code wins.

## Consequences

- **Positive:** every divergence dispute has a mechanical answer; the ruling log doubles
  as COMPAT.md source material.
- **Negative:** we knowingly ship upstream bugs (that is the product: byte parity).
- **Mitigations:** post-1.0, fixes can be proposed upstream first, then synced.

## Revisit when

Upstream fixes a logged quirk (spec-sync the new behavior), or 1.0 ships and deliberate
divergence becomes eligible.
