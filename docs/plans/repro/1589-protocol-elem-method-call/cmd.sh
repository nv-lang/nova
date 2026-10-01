#!/bin/sh
# Registry #1589 -- a method called on an element of `Vec[Greeter]` (a protocol): `gs[j].greet()`
# passes the protocol box where the concrete receiver is expected.
# Found 2026-10-01 by the assistant (nova-88) while narrowing #1571.
#
# Run from the repository root:  sh docs/plans/repro/1589-protocol-elem-method-call/cmd.sh
# Measured 2026-10-01 on main cd84734ce: CC-FAIL: passing 'NovaBox_Greeter' to parameter of incompatible type 'Nova_Speaker *'
set -e
D="$(mktemp -d)"
cp "$(dirname "$0")/main.nv.txt" "$D/main.nv"
"${NOVA:-./nova-cli/target/release/nova}" test "$D/main.nv"
