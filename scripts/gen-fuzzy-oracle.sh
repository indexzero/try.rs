#!/usr/bin/env bash
# Regenerate testdata/fuzzy_oracle.json from upstream's actual lib/fuzzy.rb
# at the tag pinned in spec/UPSTREAM. The oracle pins bit-exact (f64) score
# parity; the Rust test compares via to_bits, no epsilon.
#
# TRY_UPSTREAM_URL overrides the clone source (e.g. a local mirror).
set -euo pipefail
cd "$(dirname "$0")/.."

TAG=$(grep '^tag=' spec/UPSTREAM | cut -d= -f2)
URL="${TRY_UPSTREAM_URL:-https://github.com/tobi/try}"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
git clone --quiet --depth 1 --branch "$TAG" "$URL" "$TMP/try"

ruby -r json -r "$TMP/try/lib/fuzzy" -e '
  # Case matrix: unicode expansion, boundaries, non-matches, date prefixes,
  # emoji, base-score interplay, empty query.
  texts = [
    ["2025-11-01-alpha", 3.7], ["2025-11-15-beta", 2.4],
    ["2025-11-20-gamma", 2.1], ["2025-11-25-project-with-long-name", 2.0],
    ["no-date-prefix", 0.4], ["İstanbul", 1.0], ["café-experiment", 2.2],
    ["🚀-rocket-try", 1.5], ["a", 0.0], ["ab", 0.0],
    ["2024-01-15-feature1", 1.1], ["straße-test", 0.9],
  ]
  queries = ["", "a", "alpha", "beta", "ist", "İST", "pro", "z-x", "é",
             "rocket", "AB", "2025", "long name", "ss", "-"]
  rows = []
  texts.each do |text, base|
    fuzzy = Fuzzy.new([{text: text, base_score: base}])
    queries.each do |q|
      matched = false
      fuzzy.match(q).each do |_, positions, score|
        rows << { text: text, base_score: base, query: q,
                  score_bits: [score].pack("E").unpack1("Q<"),
                  score: score, positions: positions }
        matched = true
      end
      rows << { text: text, base_score: base, query: q, score_bits: nil,
                score: nil, positions: nil } unless matched
    end
  end
  puts JSON.pretty_generate(rows)
' > testdata/fuzzy_oracle.json
echo "wrote testdata/fuzzy_oracle.json ($(grep -c '"text"' testdata/fuzzy_oracle.json) rows) from $URL@$TAG"
