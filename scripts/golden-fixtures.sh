#!/usr/bin/env bash
# Shared fixture layout for golden-frame generation AND verification.
# Mtimes are relative to "now" (set via python3 os.utime — epoch-based, no
# local-time/DST round-trip) so rendered relative-times and recency scores
# are stable across runs. Offsets sit away from %.1f rounding boundaries:
#   alpha  2h   -> 3/sqrt(3)+2   = 3.732 -> "3.7", "2h ago"
#   beta   48h  -> 3/sqrt(49)+2  = 2.429 -> "2.4", "2d ago"
#   gamma  14d  -> 3/sqrt(337)+2 = 2.163 -> "2.2", "2w ago"
#   nodate ~8d  -> 3/sqrt(195)+0 = 0.215 -> "0.2", "1w ago"
setup_golden_fixtures() {
  local fx=$1
  rm -rf "$fx"
  mkdir -p "$fx/2025-11-01-alpha" "$fx/2025-11-15-beta" \
           "$fx/2025-11-20-gamma" "$fx/no-date-prefix"
  python3 - "$fx" <<'PY'
import os, sys, time
fx = sys.argv[1]
now = time.time()
for name, ago in [("2025-11-01-alpha", 7200), ("2025-11-15-beta", 172800),
                  ("2025-11-20-gamma", 1209600), ("no-date-prefix", 700000)]:
    t = now - ago
    os.utime(os.path.join(fx, name), (t, t))
PY
}
