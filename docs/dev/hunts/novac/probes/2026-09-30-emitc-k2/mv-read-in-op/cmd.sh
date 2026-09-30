#!/bin/sh
# Probe mv-read-in-op: A MODULE VALUE read from a handler op body, with no local of that name:
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
