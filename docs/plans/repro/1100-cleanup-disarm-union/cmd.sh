#!/bin/sh
# Run from anywhere; the script finds the repository root itself.
#
# Registry row #1100: an auto-`@cleanup` is DISARMED by an overload that is
# never called. The two files differ by exactly ONE declaration and behave
# oppositely, so both halves must be run -- a single half proves nothing.
#
# These probes are RUN (they are built and executed), which is why they keep
# the bare `.nv` suffix (rule #695 item 2a) and why this file exists: the
# suffix guard takes a recorded command as the mechanical sign of it.
#
# Expected today:
#   control-one-overload  -> stderr "panic: cleanup fired", exit 101  -- CORRECT
#   leak-two-overloads    -> stdout "1" and "reached the end without cleanup",
#                            stderr empty, exit 0                     -- THE DEFECT
#
# Binaries are written into this script's own temporary directory and removed
# at the end: the repository root stays clean (owner's rule, 2026-09-09).
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
cd "$R" || exit 2
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
NOVA=./nova-cli/target/release/nova
for f in control-one-overload leak-two-overloads; do
    echo "=== $f"
    "$NOVA" build "$D/$f.nv" -o "$T/$f.exe" || { echo "build rc=$?"; continue; }
    "$T/$f.exe"
    echo "exit=$?"
done
