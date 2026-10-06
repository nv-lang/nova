#!/bin/sh
# run from anywhere inside the repository; novac/target/novac.exe must be built
cd "$(git rev-parse --show-toplevel)" || exit 2
novac/target/novac.exe emit docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p2-vec-of-paren-record/probe.nv | grep -n 'Pt' | head -8
