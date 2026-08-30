#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe>
# The published D107 JSON Schema (its own $id is https://nova-lang.org/schemas/nova-doc-v1.json)
# carries Russian prose inside "description" fields -- it ships to every external consumer.
# Exit 0 = schema is English-only. Exit 1 = Cyrillic present.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe>"; exit 3; }
"$NOVA" doc --json-schema > schema.json 2>&1
echo "rc=$?  bytes=$(wc -c < schema.json)"
LC_ALL=C grep -n -P '[\xd0-\xd1][\x80-\xbf]' schema.json > hits.txt
n=$(wc -l < hits.txt)
echo "russian lines in the published JSON Schema: $n"
cut -c1-160 hits.txt
[ "$n" -eq 0 ] || exit 1
