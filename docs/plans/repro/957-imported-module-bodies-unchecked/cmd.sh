#!/bin/sh
# Registry #957 -- an entry-driven driver (`nova build <entry>`, `nova check <file>`)
# does not run the ro-launder / provenance return check over the bodies of an
# IMPORTED module. The same file checked directly, or `nova check <dir>`, refuses.
#
# It is NOT about reachability (an uncalled function in a peer file IS judged)
# and NOT about "imports are skipped" (the argument-position check DOES reach
# them -- see the control). It is one check that does not walk imported bodies.
#
# Run from the repository root:  sh docs/plans/repro/957-imported-module-bodies-unchecked/cmd.sh
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
NOVA="$R/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"
P="$D/pkg"
W="${TMPDIR:-/tmp}/repro957.$$"

setup() { # $1 entry file, $2 helper file, $3 optional peer file
    rm -rf "$W"; mkdir -p "$W/src/helper"
    cp "$P/nova.toml.txt" "$W/nova.toml"
    cp "$P/$1" "$W/src/main.nv"
    cp "$P/$2" "$W/src/helper/helper.nv"
    [ -n "$3" ] && cp "$P/$3" "$W/src/peer.nv"
    return 0
}
verdict() { # $1 label, $2.. command
    printf '%-42s ' "$1"; shift
    "$@" > "$W/o.txt" 2>&1
    if grep -qE '\[E[0-9_A-Z]+\]' "$W/o.txt"; then grep -oE '\[E[0-9_A-Z]+\]' "$W/o.txt" | head -1
    elif grep -q 'built:' "$W/o.txt"; then echo "BUILT -- missed"
    elif grep -q 'FAIL: 0' "$W/o.txt"; then echo "ok -- missed"
    else echo "other"; fi
}

echo "== the #760 error lives in an IMPORTED module =="
setup main_plain.nv.txt helper_ro_bad.nv.txt
verdict "build <entry>, bad() NOT called"  "$NOVA" build "$W/src/main.nv" -o "$W/a.exe"
verdict "check <entry>"                    "$NOVA" check "$W/src/main.nv"
verdict "check that file directly"         "$NOVA" check "$W/src/helper/helper.nv"
verdict "check <dir>"                      "$NOVA" check "$W/src"
setup main_calls_bad.nv.txt helper_ro_bad.nv.txt
verdict "build <entry>, bad() IS called"   "$NOVA" build "$W/src/main.nv" -o "$W/b.exe"

echo
echo "== the SAME error, not in an imported module =="
setup main_ro_bad_inline.nv.txt helper_clean.nv.txt
verdict "in the ENTRY file, never called"  "$NOVA" build "$W/src/main.nv" -o "$W/c.exe"
setup main_plain.nv.txt helper_ro_bad.nv.txt peer_ro_bad.nv.txt
verdict "in a PEER FILE, never called"     "$NOVA" build "$W/src/main.nv" -o "$W/d.exe"

echo
echo "== CONTROL: another check, same imported module =="
setup main_plain.nv.txt helper_arg_bad.nv.txt
verdict "argument-position error, uncalled" "$NOVA" build "$W/src/main.nv" -o "$W/e.exe"

rm -rf "$W"
