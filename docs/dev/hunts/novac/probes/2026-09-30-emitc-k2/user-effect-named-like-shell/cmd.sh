#!/bin/sh
# Probe user-effect-named-like-shell: A file declares an effect named like a shell effect (`Random`). The
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
