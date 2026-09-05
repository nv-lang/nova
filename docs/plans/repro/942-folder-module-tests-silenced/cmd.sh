#!/bin/sh
# Registry #942 — a peer's `EXPECT_STDOUT` marker replaces the folder-module CU's
# verdict, and every peer's `test` block result is discarded with it.
#
# Four runs. The false assertion `assert(1 == 2)` in `b_peer` is IDENTICAL in all
# four; only the third file changes.
#
#   A  two peers, no marker, no main      -> RUN-FAIL          (the assertion is caught)
#   B  + a peer with `fn main`, no marker -> PASS: 1  FAIL: 0  (silenced, second cause)
#   C  + a peer with a marker that does
#      NOT match the harness output       -> NEG-WRONG-STDOUT  (tests DID run: the
#                                            output quotes "Running 3 tests...")
#   D  + a peer with a marker that DOES
#      match                              -> PASS: 1  FAIL: 0  (silenced, main cause)
#
# C is the run that settles the mechanism. The `test` blocks are NOT skipped — the
# runner's own error message prints their results back. What happens is that the
# verdict is taken from the stdout match instead of from them, so in D a passing
# pattern makes a failing assertion invisible.
#
# WHY A PEER'S MARKER IS READ AT ALL: `collect_marker_sources` (test_runner.rs)
# gathers header directives from the entry file AND from every same-module peer. It
# was widened to peers on purpose, so that a `// ENV ...` line living on the peer
# that declares the tests would reach the run step. `EXPECT_STDOUT` travels the same
# path and takes the verdict with it.
#
# ON THE REAL CORPUS (measured 2026-09-05): `spec_tests/conformance/` has six such
# peers, and five of them spell the marker `// EXPECT_STDOUT: ok` — the colon form
# AGENTS.md forbids in as many words ("the colon would become part of the matched
# substring"), so the pattern those five ask for is literally `: ok`. 6207
# `assert(...)` lines across 1168 peer files are judged by that. Carriers:
# `append_as_slice.nv` with `assert(a == [1, 2, 3])` on a five-element vector, and
# a deliberately false assert in a fixture of my own — mega-CU `PASS: 901 FAIL: 0`
# both times.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
cd "$R" || exit 2
NOVA="$R/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"

W="${TMPDIR:-/tmp}/repro942.$$"
rm -rf "$W"; mkdir -p "$W/fm"
cp "$D/a_entry.nv.txt" "$W/fm/a_entry.nv"
cp "$D/b_peer.nv.txt"  "$W/fm/b_peer.nv"

run() {
    echo
    echo "== $1 =="
    "$NOVA" test --positive --compile-error "$W/fm" 2>&1 | tail -5
}

run "A: two peers, one assertion deliberately false"

cp "$D/c_main.nv.txt" "$W/fm/c_third.nv"
run "B: + a peer with fn main and NO marker"

cp "$D/c_marker_nomatch.nv.txt" "$W/fm/c_third.nv"
run "C: + a peer whose marker does NOT match (tests ran -- see the quoted output)"

cp "$D/c_marker_match.nv.txt" "$W/fm/c_third.nv"
run "D: + a peer whose marker DOES match"

rm -rf "$W"
