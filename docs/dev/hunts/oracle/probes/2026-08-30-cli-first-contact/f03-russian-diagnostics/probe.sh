#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe> [path-to-a-real-.nv-file]
# Run from a directory with NO nova.toml above it (this one qualifies).
# Rule: AGENTS.md, section "Language" -- diagnostic texts in English.
# Exit 0 = everything English. Exit 1 = Cyrillic found in user-facing output.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe> [some.nv]"; exit 3; }
: > all.out
NO_COLOR=1 "$NOVA" info x                >> all.out 2>&1
NO_COLOR=1 "$NOVA" add foo --path ../bar >> all.out 2>&1
NO_COLOR=1 "$NOVA" update                >> all.out 2>&1
NO_COLOR=1 "$NOVA" bench hyperfine ""    >> all.out 2>&1
# Optional second half: the SUCCESS path of "nova info" on a real package file.
if [ -n "$2" ]; then NO_COLOR=1 "$NOVA" info "$2" >> all.out 2>&1; fi
LC_ALL=C grep -n -P '[\xd0-\xd1][\x80-\xbf]' all.out > hits.txt
n=$(wc -l < hits.txt)
echo "russian user-facing lines: $n"
cut -c1-120 hits.txt
[ "$n" -eq 0 ] || exit 1
