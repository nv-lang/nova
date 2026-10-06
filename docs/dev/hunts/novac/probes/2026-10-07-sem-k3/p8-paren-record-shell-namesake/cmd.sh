#!/bin/sh
# run from anywhere inside the repository; novac/target/novac.exe must be built
cd "$(git rev-parse --show-toplevel)" || exit 2
novac/target/novac.exe emit docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p8-paren-record-shell-namesake/probe.nv | grep -c '^struct NovaTuple_Number {'
