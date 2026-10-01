#!/bin/sh
# Registry #1498 -- a source name written into C past the door `c_ident`
# (regression of #1440/#1446, 9712b306f): the declaration is escaped
# (`nv_ctx`), the use is not (`ctx`).
#
# Run from the repository root:  sh docs/plans/repro/1498-c-ident-bypass/cmd.sh
# Before the fix: g1, g3, g4 C-COMPILER ERROR, g2 (control) `g2 x-y`.
# After the fix:  `g1 x-y`, `g2 x-y`, `g3 y x | 5 2 3 7 | 14 | 50`, `g4 4 5`.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
NOVA="$R/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"
W="${TMPDIR:-/tmp}/repro1498.$$"
mkdir -p "$W"

for f in g1_param_ctx g2_param_other g3_other_sites g4_export_const; do
    cp "$D/$f.nv.txt" "$W/$f.nv"
    printf '%-16s ' "$f"
    "$NOVA" build "$W/$f.nv" -o "$W/$f.exe" > "$W/b.txt" 2>&1
    if [ -x "$W/$f.exe" ]; then
        printf 'build=ok   stdout=[%s]\n' "$("$W/$f.exe" 2>&1 | head -1 | tr -d '\r')"
    elif grep -q 'compiler error' "$W/b.txt"; then
        printf 'build=C-COMPILER ERROR: %s\n' "$(grep -m1 '\.c:[0-9]*:[0-9]*: error:' "$W/b.txt" | sed 's/.*error: //')"
    else
        printf 'build=refused\n'
    fi
done
rm -rf "$W"
