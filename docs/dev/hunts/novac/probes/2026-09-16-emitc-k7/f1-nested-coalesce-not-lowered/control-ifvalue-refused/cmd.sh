#!/bin/sh
# Run from the repository root.
export LC_ALL=C
D=docs/dev/hunts/novac/probes/2026-09-16-emitc-k7/f1-nested-coalesce-not-lowered/control-ifvalue-refused
./novac/target/novac.exe check "$D/m.nv"; echo "novac check rc=$?"
./nova-cli/target/release/nova.exe check "$D/m.nv"; echo "oracle check rc=$?"
