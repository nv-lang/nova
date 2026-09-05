#!/bin/sh
# Registry #959 -- a function's body is never checked against its DECLARED
# RETURN TYPE. Two of the three doors that enforce numeric compatibility work
# (argument, annotated binding); the return door does not exist.
#
# Run from the repository root:  sh docs/plans/repro/959-return-type-never-checked/cmd.sh
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
NOVA="$R/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"
W="${TMPDIR:-/tmp}/repro959.$$"
mkdir -p "$W"

for f in ret_str_literal ret_narrowing_u8 ret_f64_to_int ret_int_to_bool \
         ret_var_not_literal ret_block_body ctrl_arg_narrowing ctrl_bind_narrowing; do
    cp "$D/$f.nv.txt" "$W/$f.nv"
    printf '%-22s ' "$f"
    "$NOVA" check "$W/$f.nv" > "$W/c.txt" 2>&1
    if grep -qE '\[E[0-9_A-Z]+\]' "$W/c.txt"; then
        printf 'check=%-22s ' "$(grep -oE '\[E[0-9_A-Z]+\]' "$W/c.txt" | head -1)"
    else
        printf 'check=%-22s ' "ok"
    fi
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
