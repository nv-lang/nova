#!/bin/sh
# Run from the nova worktree root. `type Pair enum P(int, int) | Q(int)` with the arm `P(a, b) | Q(a)`: what was `b` -- a binder
# Q does not introduce -- read as, when the value is `Q(7)` and the body computes `a + b`?
# Expected of a FIXED compiler: REFUSED at check time, naming the missing binder and the
# alternative that lacks it. Measured 2026-09-07 on the unfixed one: accepted, built, answered
# `3 / 7 / 7 / 7`: on Q(7) the sum came out 7, so `b` was read as 0 out of the union.
P=docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p01-arity-mismatch/probe.nv
NOVA=nova-cli/target/release/nova.exe
echo "=== does the oracle accept the form?"
"$NOVA" check "$P" 2>&1 | grep -v vcpkg | grep -E 'ok:|FAIL:|error' | head -3
echo "=== build and run:"
"$NOVA" build "$P" -o "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p01-arity-mismatch/probe.exe" 2>&1 | grep -v vcpkg | grep -E 'built:|error' | head -2
[ -x "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p01-arity-mismatch/probe.exe" ] && "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p01-arity-mismatch/probe.exe" | head -5
rm -f "docs/dev/hunts/oracle/probes/2026-09-07-match-disj-bind/p01-arity-mismatch/probe.exe"
