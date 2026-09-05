#!/bin/sh
# Registry #944 -- `print`/`println` applied DIRECTLY to a `char` print the
# codepoint; every other display path prints the character.
#
# One program, eight lines of output. The same value `'A'` goes out four ways and
# disagrees with itself:
#
#   println(a)        -> 65      WRONG
#   print(a)          -> 65      WRONG
#   a.to_str()        -> A       right
#   "${a}"            -> A       right
#   through a `char` parameter, printed inside the callee -> 65   WRONG
#   'я' interpolated  -> я       right
#   'я' printed       -> 1103    WRONG
#   Vec[char] interpolated -> Vec[A, B]   right
#
# So it is not "char has no display" -- three of four paths have one and agree.
# It is the direct `print`/`println` argument path, and only that one, treating a
# `char` as its integer.
#
# WHY THIS IS WORSE THAN IT LOOKS: `println(c)` is the first line anyone writes.
# The wrong answer is not a crash and not a diagnostic -- it is a plausible number
# where a letter belongs, and the reader will suspect their own data first.
D="$(cd "$(dirname "$0")" && pwd)"
R="$(cd "$D/../../../.." && pwd)"
cd "$R" || exit 2
NOVA="$R/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$R/nova-cli/target/release/nova"

W="${TMPDIR:-/tmp}/repro944.$$"
mkdir -p "$W"
cp "$D/charmatrix.nv.txt" "$W/charmatrix.nv"
"$NOVA" build "$W/charmatrix.nv" -o "$W/charmatrix.exe" 2>&1 | tail -1
"$W/charmatrix.exe"
echo "rc=$?"
rm -rf "$W"
