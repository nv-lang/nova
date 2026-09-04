#!/bin/sh
# Run from the repository root. Expected today: three diagnostics, of which two
# are echoes; expected after the wave: one honest refusal ("signature read, no
# body compiled") and no unknown-name echoes.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
cd "$R" || exit 2
./novac/target/novac.exe check "$D/prog/p.nv" --std "$D/decls"
echo "rc=$?"
