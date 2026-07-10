//! Core library for `try-me-maybe` — a Rust port of [tobi/try](https://github.com/tobi/try).
//!
//! `try` is a script emitter with an interactive picker attached: a child
//! process cannot `cd` its parent shell, so the TUI renders to **stderr**,
//! the chosen action is emitted as a shell script on **stdout**, and the
//! wrapper function (emitted by `tryme init`) `eval`s stdout on exit 0.
//!
//! Everything test-observable lives in this crate; the `tryme` binary is a
//! thin wrapper. Conformance target: tobi/try v1.9.3, pinned by the adopted
//! suite at `spec/tests/` (see `spec/UPSTREAM`).
//!
//! ## Provenance
//!
//! Decisions are recorded in `docs/adr/`. Behavior authority order
//! (ADR-0003): upstream `try.rb` code > shipped `spec/tests` > prose specs.

pub mod argv;
pub mod dispatch;
pub mod emit;
pub mod env;
pub mod fuzzy;
pub mod giturl;
pub mod help;
pub mod naming;
pub mod scan;
pub mod scripts;
pub mod selector;
pub mod testkeys;
pub mod tui;
pub mod wrappers;

pub use dispatch::{run, Ctx};
pub use emit::ScriptOut;
pub use env::Env;
