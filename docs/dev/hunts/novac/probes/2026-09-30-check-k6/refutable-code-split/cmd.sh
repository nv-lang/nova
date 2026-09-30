#!/bin/sh
# Probe refutable-code-split: E_REFUTABLE_BINDING spellings: refutable-code-split
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
