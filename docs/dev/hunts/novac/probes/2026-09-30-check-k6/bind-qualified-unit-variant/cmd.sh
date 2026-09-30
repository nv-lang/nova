#!/bin/sh
# Probe bind-qualified-unit-variant: E_REFUTABLE_BINDING spellings: bind-qualified-unit-variant
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
