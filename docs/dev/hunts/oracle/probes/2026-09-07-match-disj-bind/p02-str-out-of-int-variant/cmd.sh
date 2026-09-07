#!/bin/sh
# Run from the nova worktree root. `type Tagged enum P(int, str) | Q(int)` printing `b`: P's second field is a POINTER, so reading
# it out of a `Q` value shows a pointer that was never stored there -- the sharper half of p01.
# Expected of a FIXED compiler: REFUSED at check time, naming the missing binder and the
# alternative that lacks it. Measured 2026-09-07 on the unfixed one: accepted, built, answered
# `real` for P(1, "real") and an EMPTY LINE for Q(9) -- a string pointer read out of Q.
P=docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p02-str-out-of-int-variant/probe.nv
NOVA=nova-cli/target/release/nova.exe
echo "=== does the oracle accept the form?"
"$NOVA" check "$P" 2>&1 | grep -v vcpkg | grep -E 'ok:|FAIL:|error' | head -3
echo "=== build and run:"
"$NOVA" build "$P" -o "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p02-str-out-of-int-variant/probe.exe" 2>&1 | grep -v vcpkg | grep -E 'built:|error' | head -2
[ -x "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p02-str-out-of-int-variant/probe.exe" ] && "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p02-str-out-of-int-variant/probe.exe" | head -5
rm -f "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p02-str-out-of-int-variant/probe.exe"
