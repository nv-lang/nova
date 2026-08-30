#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe>
# Run from a directory that has NO nova.toml anywhere above it (this one qualifies).
# docs/guide/nova-cli.md:185 -- "If no nova.toml is found -- exit 2:"
# docs/guide/nova-cli.md:161 -- exit 2 = Usage error (..., missing nova.toml)
# Exit 0 = all three exit 2 as documented. Exit 1 = at least one exits 1.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe>"; exit 3; }
fail=0
NO_COLOR=1 "$NOVA" check                 > o1 2>&1; r1=$?
NO_COLOR=1 "$NOVA" test .                > o2 2>&1; r2=$?
NO_COLOR=1 "$NOVA" regen-runtime --check > o3 2>&1; r3=$?
echo "nova check              -> rc=$r1 (doc says 2) | $(head -1 o1)"
echo "nova test .             -> rc=$r2 (doc says 2) | $(head -1 o2)"
echo "nova regen-runtime      -> rc=$r3 (doc says 2) | $(head -1 o3)"
for r in "$r1" "$r2" "$r3"; do [ "$r" = "2" ] || fail=1; done
exit $fail
