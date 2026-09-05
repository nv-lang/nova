#!/bin/sh
# Registry #942 — one peer with an EXPECT_STDOUT marker silences every OTHER
# peer's `test` blocks in the same folder-module CU.
#
# Run from anywhere. Two runs, and the whole finding is the difference between
# them:
#
#   run 1 (a_entry + b_peer)            -> RUN-FAIL, "deliberately FALSE"
#   run 2 (the same two + c_stdout)     -> PASS: 1  FAIL: 0
#
# Nothing about the false assertion changed between them. The third file only
# added `// EXPECT_STDOUT hello930`, and that flipped the CU's lane from
# "positive, run the test blocks" to "stdout, compare the output" — after which
# the failing assertion is compiled and never run.
#
# Measured 2026-09-05 on `spec_tests/conformance`, where six such peers exist
# (`p1_canonical_range.nv`, `p2_contract_real_bounds.nv`,
# `p3_single_and_ordered.nv`, ...) and 6207 `assert(...)` lines across 1168 peer
# files are silenced by them.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
cd "$R" || exit 2
NOVA="$R/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"

W="${TMPDIR:-/tmp}/repro942.$$"
rm -rf "$W"; mkdir -p "$W/fm930"
cp "$D/a_entry.nv.txt" "$W/fm930/a_entry.nv"
cp "$D/b_peer.nv.txt"  "$W/fm930/b_peer.nv"

echo "== run 1: two peers, one assertion deliberately false =="
"$NOVA" test --positive --compile-error "$W/fm930" 2>&1 | tail -4

cp "$D/c_stdout.nv.txt" "$W/fm930/c_stdout.nv"

echo
echo "== run 2: the SAME two peers, plus one EXPECT_STDOUT peer =="
"$NOVA" test --positive --compile-error "$W/fm930" 2>&1 | tail -4

rm -rf "$W"
