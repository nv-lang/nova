#!/bin/sh
# Run from the repository root.
export LC_ALL=C
D=docs/dev/hunts/novac/probes/2026-09-16-emitc-k7/f3-printer-gate-literal-half/expr-char
./novac/target/novac.exe check "$D/m.nv"; echo "novac check rc=$?"
./novac/target/novac.exe emit  "$D/m.nv" | tail -c 400; echo; echo "novac emit  rc=$?"
./nova-cli/target/release/nova.exe check "$D/m.nv"; echo "oracle check rc=$?"
