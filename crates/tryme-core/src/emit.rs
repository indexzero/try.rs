//! Script emission — the ONLY module that writes to stdout.
//!
//! Port of upstream's shell-script helpers (`try.rb:1388-1409`). The emitted
//! script is the product: the wrapper function captures stdout and `eval`s it
//! on exit 0. Byte format is pinned by `test_05_script_format.sh`.

use std::io::Write;

/// First line of every emitted script (`try.rb:1389`), byte-exact.
pub const SCRIPT_WARNING: &str =
    "# if you can read this, you didn't launch try from an alias. run try --help.";

/// Single-quote shell quoting, port of `q()` (`try.rb:1391-1393`):
/// wrap in `'…'`, escaping embedded `'` as `'"'"'`.
#[must_use]
pub fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', r#"'"'"'"#))
}

/// Newtype over the real stdout handle. Constructed once in `main` and handed
/// only to emission call sites, so writing script bytes to the wrong stream is
/// unrepresentable.
pub struct ScriptOut<W: Write>(W);

impl<W: Write> ScriptOut<W> {
    /// Wrap the process stdout (or a test buffer).
    pub fn new(w: W) -> Self {
        Self(w)
    }

    /// Port of `emit_script` (`try.rb:1395-1409`): warning comment first,
    /// commands chained `&& \` with 2-space continuation indent, final
    /// newline, no trailing `&&`.
    ///
    /// # Errors
    /// Propagates I/O errors from the underlying writer.
    pub fn emit_script(&mut self, cmds: &[String]) -> std::io::Result<()> {
        writeln!(self.0, "{SCRIPT_WARNING}")?;
        let last = cmds.len().saturating_sub(1);
        for (i, cmd) in cmds.iter().enumerate() {
            if i == 0 {
                write!(self.0, "{cmd}")?;
            } else {
                write!(self.0, "  {cmd}")?;
            }
            if i < last {
                writeln!(self.0, " && \\")?;
            } else {
                writeln!(self.0)?;
            }
        }
        Ok(())
    }

    /// Upstream's `puts "Cancelled."` on cancel goes to STDOUT
    /// (`try.rb:1557,1566,1584`) — a sanctioned non-script stdout emission.
    ///
    /// # Errors
    /// Propagates I/O errors from the underlying writer.
    pub fn cancelled(&mut self) -> std::io::Result<()> {
        writeln!(self.0, "Cancelled.")
    }

    /// `init` output (the wrapper function) also goes to stdout — it is what
    /// the user's `eval "$(tryme init …)"` consumes (`try.rb:1181`).
    ///
    /// # Errors
    /// Propagates I/O errors from the underlying writer.
    pub fn raw(&mut self, text: &str) -> std::io::Result<()> {
        write!(self.0, "{text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emit(cmds: &[&str]) -> String {
        let mut buf = Vec::new();
        ScriptOut::new(&mut buf)
            .emit_script(&cmds.iter().map(ToString::to_string).collect::<Vec<_>>())
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn quotes_plain_and_embedded_single_quotes() {
        assert_eq!(q("abc"), "'abc'");
        assert_eq!(q("a'b"), r#"'a'"'"'b'"#);
    }

    #[test]
    fn script_format_matches_upstream() {
        // Pinned by test_05_script_format.sh: warning first, `&& \` chaining,
        // two-space continuation indent, no trailing chain on the last line.
        let out = emit(&["touch '/a'", "echo '/a'", "cd '/a'"]);
        assert_eq!(
            out,
            format!("{SCRIPT_WARNING}\ntouch '/a' && \\\n  echo '/a' && \\\n  cd '/a'\n")
        );
    }

    #[test]
    fn single_command_has_no_chain() {
        let out = emit(&["cd '/x'"]);
        assert_eq!(out, format!("{SCRIPT_WARNING}\ncd '/x'\n"));
    }
}
