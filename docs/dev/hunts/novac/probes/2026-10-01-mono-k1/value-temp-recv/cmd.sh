#!/bin/sh
# Probe value-temp-recv (a value TEMPORARY as the receiver of a by-pointer `value` method (D32: hoisted, `&temp`; D488 R3): a call result `mk().bump()`, a record literal `Pv { n: 4 }.bump()`, a result nobody reads, and a `-> @` method on a named receiver. Oracle 2 5 5. Refused on main: "does not spill it to a place yet"; and every `-> @` method on a by-pointer receiver returned the pointer where its signature promised the value (C type error))
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_set'
export PROBE GREP
. "$PROBE/../run-probe.sh"
