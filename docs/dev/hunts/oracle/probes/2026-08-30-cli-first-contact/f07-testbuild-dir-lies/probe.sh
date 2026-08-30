#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe>
# "nova test-build <existing directory>" reports "file not found" for a path that exists.
# "nova build" on the SAME input gets it right -- so the two disagree about the same question.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe>"; exit 3; }
mkdir -p sub
echo "-- the argument exists:"
ls -d sub
echo "-- nova test-build sub:"
NO_COLOR=1 "$NOVA" test-build sub > tb.out 2>&1
echo "   rc=$?"
head -3 tb.out
echo "-- nova build sub (same input, honest message):"
NO_COLOR=1 "$NOVA" build sub > b.out 2>&1
echo "   rc=$?"
head -3 b.out
