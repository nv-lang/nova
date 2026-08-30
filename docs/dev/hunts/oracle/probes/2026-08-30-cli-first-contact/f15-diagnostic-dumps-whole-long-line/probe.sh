#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe>
# A diagnostic on a long source line renders the WHOLE line plus a caret run of the
# same order into the terminal: no column window, no truncation, no ellipsis.
# rustc/clang/gcc all window a long line; here the first error a newcomer sees can be
# tens of kilobytes wide. Generates its own input; writes only into this directory.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe>"; exit 3; }
n=10000
{ printf 'module deep\nfn main() Io -> () { ro x int = '
  i=0; while [ $i -lt $n ]; do printf '('; i=$((i+1)); done
  printf '1'
  i=0; while [ $i -lt $n ]; do printf ')'; i=$((i+1)); done
  printf ' }\n'
} > deep.nv
echo "source: $(wc -c < deep.nv) bytes, longest line $(awk '{if(length($0)>m)m=length($0)}END{print m}' deep.nv) chars"
NO_COLOR=1 "$NOVA" check deep.nv > out.txt 2>&1
echo "rc=$?  diagnostic output: $(wc -c < out.txt) bytes"
echo "per-line widths of the rendered diagnostic:"
awk 'NR<=6{print "  line "NR": "length($0)" chars"}' out.txt
