#!/bin/sh
# Registry #1588 -- a slice of an array of closures, `fs[0..0]`, does not link: the runtime has
# no `nova_array_slice_void_p` (the closure arrays get push/get/pop, not slice).
# Found 2026-10-01 by the assistant (nova-88) while narrowing #1571.
#
# Run from the repository root:  sh docs/plans/repro/1588-slice-of-closure-array/cmd.sh
# Measured 2026-10-01 on main cd84734ce: lld-link: error: undefined symbol: nova_array_slice_void_p
set -e
D="$(mktemp -d)"
cp "$(dirname "$0")/main.nv.txt" "$D/main.nv"
"${NOVA:-./nova-cli/target/release/nova}" test "$D/main.nv"
