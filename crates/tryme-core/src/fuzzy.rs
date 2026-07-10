//! Fuzzy matcher — exact transliteration of upstream `lib/fuzzy.rb`.
//!
//! Parity notes (each pinned by the oracle fixture and differential fuzz):
//! - `text.index(qc, pos)` operates on **chars** of the *lowered* text.
//! - Lowercasing is Ruby `downcase`: full mappings, **expanding** (`İ` →
//!   `i` + U+0307, 2 chars). Positions index the expanded lowered text.
//! - Empty query early-returns the raw `base_score` **before** the density
//!   and length multipliers (`fuzzy.rb:94`).
//! - The word-boundary regex `/[^a-z0-9]/` is ASCII-only — combining marks
//!   and any non-ASCII char count as boundaries.
//! - Both multipliers apply to the **entire** score (base included), and the
//!   length penalty divides by the **original** text's char count, not the
//!   lowered one (`fuzzy.rb:129-132`).
//! - Ruby's sort is unstable; tie order is documented as not guaranteed
//!   (ADR-0003).

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "1:1 port of Ruby integer/float arithmetic; widths, cursor positions, and \
            list lengths are bounded by terminal size and directory counts"
)]

/// A scored, matchable entry.
pub struct Entry<T> {
    /// Caller payload.
    pub data: T,
    text_char_len: usize,
    text_lower: Vec<char>,
    base_score: f64,
}

impl<T> Entry<T> {
    /// Build an entry from its display text and pre-computed base score.
    pub fn new(data: T, text: &str, base_score: f64) -> Self {
        // Ruby String#downcase: full Unicode mapping, may expand char count.
        let text_lower: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
        Self {
            data,
            text_char_len: text.chars().count(),
            text_lower,
            base_score,
        }
    }
}

/// One match: payload reference, highlight positions (char indices into the
/// lowered text), score.
pub struct Match<'a, T> {
    /// Payload of the matched entry.
    pub data: &'a T,
    /// Highlight char positions.
    pub positions: Vec<usize>,
    /// Final score.
    pub score: f64,
}

/// Port of `calculate_match` (`fuzzy.rb:89-135`). `None` = filtered out.
fn calculate_match<T>(
    entry: &Entry<T>,
    query: &str,
    query_chars: &[char],
) -> Option<(f64, Vec<usize>)> {
    let mut positions = Vec::new();
    let mut score = entry.base_score;

    // Empty query = match all with base score only — BEFORE both multipliers.
    if query.is_empty() {
        return Some((score, positions));
    }

    let text = &entry.text_lower;
    let query_len = query_chars.len();
    let mut last_pos: i64 = -1;
    let mut pos: usize = 0;

    for &qc in query_chars {
        // text.index(qc, pos) over chars
        let found = text[pos.min(text.len())..]
            .iter()
            .position(|&c| c == qc)
            .map(|i| i + pos)?;

        positions.push(found);
        score += 1.0;

        // Word boundary: start, or previous char outside ASCII [a-z0-9]
        if found == 0 || {
            let prev = text[found - 1];
            !(prev.is_ascii_lowercase() || prev.is_ascii_digit())
        } {
            score += 1.0;
        }

        // Proximity bonus (SQRT_TABLE is just a cache of the same formula)
        if last_pos >= 0 {
            #[allow(clippy::cast_sign_loss, reason = "found > last_pos >= 0 here")]
            let gap = (found as i64 - last_pos - 1) as u64;
            #[allow(clippy::cast_precision_loss, reason = "gap is tiny in practice")]
            {
                score += 2.0 / ((gap + 1) as f64).sqrt();
            }
        }

        last_pos = found as i64;
        pos = found + 1;
    }

    // Density bonus — multiplies the ENTIRE running score (upstream quirk)
    if last_pos >= 0 {
        #[allow(clippy::cast_precision_loss, reason = "lengths are tiny")]
        {
            score *= query_len as f64 / (last_pos + 1) as f64;
        }
    }

    // Length penalty — original text char count, not the lowered one
    #[allow(clippy::cast_precision_loss, reason = "lengths are tiny")]
    {
        score *= 10.0 / (entry.text_char_len as f64 + 10.0);
    }

    Some((score, positions))
}

/// Port of `Fuzzy#match(...).limit(n).each`: score all entries, drop
/// non-matches, sort descending, apply the limit. Both Ruby branches
/// (`max_by(n)` and full sort) yield descending order; ties unspecified.
pub fn match_entries<'a, T>(
    entries: &'a [Entry<T>],
    query: &str,
    limit: Option<usize>,
) -> Vec<Match<'a, T>> {
    let query_lower: String = query.chars().flat_map(char::to_lowercase).collect();
    let query_chars: Vec<char> = query_lower.chars().collect();

    let mut results: Vec<Match<'a, T>> = entries
        .iter()
        .filter_map(|e| {
            calculate_match(e, query, &query_chars).map(|(score, positions)| Match {
                data: &e.data,
                positions,
                score,
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    if let Some(n) = limit {
        results.truncate(n);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str, base: f64) -> Entry<String> {
        Entry::new(text.to_string(), text, base)
    }

    fn score_of(text: &str, base: f64, query: &str) -> Option<(f64, Vec<usize>)> {
        let e = entry(text, base);
        let ql: String = query.chars().flat_map(char::to_lowercase).collect();
        let qc: Vec<char> = ql.chars().collect();
        calculate_match(&e, query, &qc)
    }

    #[test]
    fn empty_query_returns_raw_base_score_before_multipliers() {
        // fuzzy.rb:94 — neither density nor length penalty applies.
        let (score, positions) = score_of("a-very-long-name", 3.25, "").unwrap();
        assert_eq!(score.to_bits(), 3.25f64.to_bits());
        assert!(positions.is_empty());
    }

    #[test]
    fn all_query_chars_must_match_in_order() {
        assert!(score_of("abc", 0.0, "ac").is_some());
        assert!(score_of("abc", 0.0, "ca").is_none());
        assert!(score_of("abc", 0.0, "abd").is_none());
    }

    #[test]
    fn hand_computed_exact_score() {
        // "ab" in "ab": a: +1 +1(boundary at 0); b: +1, gap 0 -> +2/sqrt(1)=2
        // = 0 + 5.0; density *= 2/2 = 1; length *= 10/12
        let (score, positions) = score_of("ab", 0.0, "ab").unwrap();
        let expected = 5.0f64 * (2.0 / 2.0) * (10.0 / 12.0);
        assert_eq!(score.to_bits(), expected.to_bits());
        assert_eq!(positions, vec![0, 1]);
    }

    #[test]
    fn expanding_downcase_matches_ruby() {
        // "İstanbul".downcase => "i" + U+0307 + "stanbul" (9 chars).
        // Query "ist": i at 0 (+1+1); s at 2 (+1, prev U+0307 is a boundary +1,
        // gap 1 -> +2/sqrt(2)); t at 3 (+1, gap 0 -> +2).
        // Sum = 1+1+1+1+2/sqrt(2)+1+2 = 7 + 2/sqrt(2)
        // density *= 3/4; length penalty uses ORIGINAL 8 chars: *= 10/18.
        let (score, positions) = score_of("İstanbul", 1.0, "ist").unwrap();
        let expected = (1.0f64 + 7.0 + 2.0 / 2.0f64.sqrt()) * (3.0 / 4.0) * (10.0 / 18.0);
        assert_eq!(score.to_bits(), expected.to_bits());
        assert_eq!(positions, vec![0, 2, 3]);
    }

    #[test]
    fn sort_descending_and_limit() {
        let entries = vec![entry("aaa", 1.0), entry("aab", 5.0), entry("abc", 3.0)];
        let m = match_entries(&entries, "a", Some(2));
        assert_eq!(m.len(), 2);
        assert!(m[0].score >= m[1].score);
    }

    #[test]
    fn word_boundary_is_ascii_only() {
        // prev char 'é' is not [a-z0-9] -> boundary bonus applies
        let (with_boundary, _) = score_of("éa", 0.0, "a").unwrap();
        let (without, _) = score_of("ba", 0.0, "a").unwrap();
        assert!(with_boundary > without);
    }
}
