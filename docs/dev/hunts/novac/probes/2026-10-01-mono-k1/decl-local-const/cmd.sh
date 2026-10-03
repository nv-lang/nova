#!/bin/sh
# Probe decl-local-const (a literal and an untyped CONSTANT EXPRESSION under a DECLARED local type (D489: "a declaration with a type" is a position; registry 1679): `ro a u8 = 42`, `ro b u8 = 40 + 2`, `ro i Small = 40 + 2` with `type Small u8`. Oracle 42 42 42. Refused on main: "a declared local type is not compiled yet (E2-b3) -- only an initializer of exactly the written type")
PROBE="$(cd "$(dirname "$0")" && pwd)"
GREP='novac_fn_hp_main'
export PROBE GREP
. "$PROBE/../run-probe.sh"
