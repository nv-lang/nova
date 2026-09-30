#!/bin/sh
# Probe capture-mut-record-field-write: A captured `mut` RECORD whose FIELD is written in the op body: the write
P="$(cd "$(dirname "$0")" && pwd)/probe.nv"
export P
. "$(dirname "$0")/../run-probe.sh"
