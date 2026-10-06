#!/bin/sh
# run from anywhere inside the repository; novac/target/novac.exe must be built
cd "$(git rev-parse --show-toplevel)" || exit 2
NOVAC_SELF_PATH=docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p7-handed-plain-sum-tags/lib novac/target/novac.exe emit docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p7-handed-plain-sum-tags/app/main.nv | grep -n 'Figure'; NOVAC_SELF_PATH=docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p7-handed-plain-sum-tags/lib novac/target/novac.exe emit docs/dev/hunts/novac/probes/2026-10-07-sem-k3/p7-handed-plain-sum-tags/lib/shapes.nv | grep -n 'Figure'
