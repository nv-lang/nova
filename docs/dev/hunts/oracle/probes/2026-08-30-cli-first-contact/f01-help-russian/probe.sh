#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe>
# Exit 0 = help is English-only. Exit 1 = Cyrillic bytes found in "nova --help".
# Rule: AGENTS.md, section "Language" -- diagnostic texts in English.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe>"; exit 3; }
"$NOVA" --help > help.out 2>&1
LC_ALL=C grep -n -P '[\xd0-\xd1][\x80-\xbf]' help.out > hits.txt
n=$(wc -l < hits.txt)
echo "cyrillic lines in 'nova --help': $n"
cut -c1-120 hits.txt
[ "$n" -eq 0 ] || exit 1
