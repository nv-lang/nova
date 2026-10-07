#!/bin/sh
# run from anywhere inside the repository; novac/target/novac.exe must be built
cd "$(git rev-parse --show-toplevel)" || exit 2
novac/target/novac.exe check docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p4-abi-name-sum-eq/probe.nv; echo rc=$?; novac/target/novac.exe check docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p4-abi-name-sum-eq-control/probe.nv; echo control rc=$?
