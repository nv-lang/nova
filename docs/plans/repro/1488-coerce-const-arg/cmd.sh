#!/bin/sh
# Registry #1488 -- a module-level `str` const at a `[]u8` position does not get
# its `.bytes()` view since #1452 (CC-FAIL), because the checker had no type for
# a const name at all. Flagship carrier: examples/net/echo_client.nv.
#
# Run from the repository root:  sh docs/plans/repro/1488-coerce-const-arg/cmd.sh
# Before the fix: min_method_arg / let_position -> build=C-COMPILER ERROR,
#                 const_type_mismatch -> check=ok.
# After the fix:  min_method_arg / let_position -> build=ok stdout=[2],
#                 const_type_mismatch -> check=[E7301].
# The "before" lines were taken on main e63d0167e (the both-ways proof passed
# with a temporary kill-switch on one binary, removed before the merge).
# neighbour_qualified_module_const: NOT this class -- `m.K` (a const read through
# a module prefix) is emitted as `m->K` even with no coercion at all; it stays
# C-COMPILER ERROR before and after.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
NOVA="$R/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"
W="${TMPDIR:-/tmp}/repro1488.$$"
mkdir -p "$W"

for f in min_method_arg let_position const_type_mismatch neighbour_qualified_module_const; do
    cp "$D/$f.nv.txt" "$W/$f.nv"
    printf '%-34s ' "$f"
    "$NOVA" check "$W/$f.nv" > "$W/c.txt" 2>&1
    if grep -qE '\[E[0-9_A-Z]+\]' "$W/c.txt"; then
        printf 'check=%-10s\n' "$(grep -oE '\[E[0-9_A-Z]+\]' "$W/c.txt" | head -1)"
        continue
    fi
    printf 'check=%-10s ' "ok"
    "$NOVA" build "$W/$f.nv" -o "$W/$f.exe" > "$W/b.txt" 2>&1
    if grep -q 'built:' "$W/b.txt"; then
        printf 'build=ok   stdout=[%s]\n' "$("$W/$f.exe" 2>&1 | head -1 | tr -d '\r')"
    elif grep -q 'compiler error' "$W/b.txt"; then
        printf 'build=C-COMPILER ERROR\n'
    else
        printf 'build=refused\n'
    fi
done
rm -rf "$W"
