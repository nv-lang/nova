#!/bin/sh
# Probe capture-method-receiver: A captured `mut` vector used as a method RECEIVER in an op body
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
