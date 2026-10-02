#!/bin/sh
# Probe expected-type-param (a type parameter fixed by the call's EXPECTED type (D489, the integrator's ruling of 2026-10-02): `-> Row => idf(if m { 1 } else { 0 })` -- free function, method, a bare literal argument; control: `ro c = idf(6)` fixes `T = int` from the argument. Base: E7301 "cannot return value of type `int`" x3)
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_free'
export PROBE GREP
. "$PROBE/../run-probe.sh"
