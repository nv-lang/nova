#!/bin/sh
# Registry #1545 -- an anonymous record literal as an ELEMENT of `[]Outcome`, where
# `Outcome` is a program record named like the prelude's generic `Outcome[T]`
# (D455), declared in a module other than the entry one. Found while fixing #1533.
#
# Run from the repository root:  sh docs/plans/repro/1545-anon-elem-generic-name/cmd.sh
# Measured 2026-10-01:
#   main (before #1533):          stdout " 0| 0" -- silently wrong (expected "a 1|b 2")
#   p1532-1533-regressions:       CODEGEN-FAIL "cannot infer type argument `T` for
#                                 generic function `copy_n_nonoverlapping`"
#   the same files with the record renamed `Outcomez`: "a 1|b 2" on both.
# The two files must sit side by side (`import outcome.{listed}`).
set -e
D="$(mktemp -d)"
cp "$(dirname "$0")/outcome.nv.txt" "$D/outcome.nv"
cp "$(dirname "$0")/main.nv.txt" "$D/main.nv"
"${NOVA:-./nova-cli/target/release/nova}" test "$D/main.nv"
