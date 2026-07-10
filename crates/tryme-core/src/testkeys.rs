//! `--and-keys` parser — port of `parse_test_keys` (`try.rb:1094-1150`).
//!
//! Two auto-detected modes. Token mode when the spec contains a comma OR is
//! purely `[A-Z-]+`; otherwise raw-character mode (3-byte `\e[X` sequences
//! kept whole). Unknown multi-char tokens are **silently dropped** — the
//! quirk that makes `test_13_vim_nav` pass vacuously (ADR-0003 ruling: CTRL-J
//! is intentionally absent from the token map; CTRL-K maps to kill-line).

/// Parse an `--and-keys` spec into individual key strings, or `None` for an
/// empty/missing spec. Each element is what one `read_key` call returns.
#[must_use]
pub fn parse_test_keys(spec: &str) -> Option<Vec<String>> {
    if spec.is_empty() {
        return None;
    }
    let token_mode = spec.contains(',')
        || (!spec.is_empty() && spec.chars().all(|c| c.is_ascii_uppercase() || c == '-'));

    let keys = if token_mode {
        // Ruby: spec.split(/,\s*/)
        let mut keys = Vec::new();
        for tok in split_comma_ws(spec) {
            let up = tok.to_uppercase();
            let mapped: Option<&str> = match up.as_str() {
                "UP" => Some("\x1b[A"),
                "DOWN" => Some("\x1b[B"),
                "LEFT" => Some("\x1b[D"),
                "RIGHT" => Some("\x1b[C"),
                "ENTER" => Some("\r"),
                "ESC" => Some("\x1b"),
                "BACKSPACE" => Some("\x7f"),
                "CTRL-A" | "CTRLA" => Some("\x01"),
                "CTRL-B" | "CTRLB" => Some("\x02"),
                "CTRL-D" | "CTRLD" => Some("\x04"),
                "CTRL-E" | "CTRLE" => Some("\x05"),
                "CTRL-F" | "CTRLF" => Some("\x06"),
                "CTRL-G" | "CTRLG" => Some("\x07"),
                "CTRL-H" | "CTRLH" => Some("\x08"),
                "CTRL-K" | "CTRLK" => Some("\x0b"),
                "CTRL-N" | "CTRLN" => Some("\x0e"),
                "CTRL-P" | "CTRLP" => Some("\x10"),
                "CTRL-R" | "CTRLR" => Some("\x12"),
                "CTRL-T" | "CTRLT" => Some("\x14"),
                "CTRL-W" | "CTRLW" => Some("\x17"),
                _ => None,
            };
            if let Some(k) = mapped {
                keys.push(k.to_string());
            } else if let Some(text) = strip_type_prefix(tok) {
                // TYPE=text → each char is a key (try.rb:1127-1128)
                keys.extend(text.chars().map(|c| c.to_string()));
            } else if tok.chars().count() == 1 {
                // Single-char token passes through (try.rb:1130)
                keys.push(tok.to_string());
            }
            // else: unknown token silently dropped (load-bearing quirk)
        }
        keys
    } else {
        // Raw mode: chars, keeping \e[X 3-char escape sequences whole
        // (try.rb:1136-1148; Ruby indexes chars and requires i+2 < len,
        // i.e. a strictly-inside third char).
        let chars: Vec<char> = spec.chars().collect();
        let mut keys = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\x1b' && i + 2 < chars.len() && chars[i + 1] == '[' {
                keys.push(chars[i..i + 3].iter().collect());
                i += 3;
            } else {
                keys.push(chars[i].to_string());
                i += 1;
            }
        }
        keys
    };
    Some(keys)
}

/// Ruby `split(/,\s*/)`: comma followed by any run of whitespace.
fn split_comma_ws(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(idx) = rest.find(',') {
        out.push(&rest[..idx]);
        rest = rest[idx + 1..].trim_start();
    }
    out.push(rest);
    out
}

/// Ruby `/^TYPE=/i` prefix strip.
fn strip_type_prefix(tok: &str) -> Option<&str> {
    let bytes = tok.as_bytes();
    if bytes.len() >= 5 && tok[..5].eq_ignore_ascii_case("TYPE=") {
        Some(&tok[5..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_mode_basics() {
        assert_eq!(
            parse_test_keys("DOWN, ENTER").unwrap(),
            vec!["\x1b[B".to_string(), "\r".to_string()]
        );
        // Purely [A-Z-]+ without comma is also token mode
        assert_eq!(parse_test_keys("ESC").unwrap(), vec!["\x1b".to_string()]);
    }

    #[test]
    fn unknown_tokens_silently_dropped_vim_nav_quirk() {
        // CTRL-J is NOT in the token map (kill-line CTRL-K is): test_13 relies
        // on unknown tokens vanishing rather than erroring.
        assert_eq!(parse_test_keys("CTRL-J").unwrap(), Vec::<String>::new());
        assert_eq!(parse_test_keys("CTRL-K").unwrap(), vec!["\x0b".to_string()]);
    }

    #[test]
    fn type_injection_and_single_chars() {
        assert_eq!(
            parse_test_keys("TYPE=ab, ENTER").unwrap(),
            vec!["a".to_string(), "b".to_string(), "\r".to_string()]
        );
        // single-char unknown token passes through
        assert_eq!(parse_test_keys("x, ENTER").unwrap()[0], "x");
    }

    #[test]
    fn raw_mode_keeps_escape_sequences_whole() {
        let keys = parse_test_keys("a\x1b[Ab").unwrap();
        assert_eq!(
            keys,
            vec!["a".to_string(), "\x1b[A".to_string(), "b".to_string()]
        );
        // "\e[A" alone: i+2 < len holds (the third char exists), so the
        // sequence stays whole; a bare "\e[" (len 2) splits.
        let keys = parse_test_keys("\x1b[A").unwrap();
        assert_eq!(keys, vec!["\x1b[A".to_string()]);
        let keys = parse_test_keys("\x1b[").unwrap();
        assert_eq!(keys, vec!["\x1b".to_string(), "[".to_string()]);
    }

    #[test]
    fn empty_spec_is_none() {
        assert_eq!(parse_test_keys(""), None);
    }
}
