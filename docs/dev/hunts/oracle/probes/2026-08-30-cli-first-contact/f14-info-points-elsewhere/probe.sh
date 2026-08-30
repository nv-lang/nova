#!/bin/sh
# usage: sh probe.sh <path-to-nova.exe> [project-dir]
# Runs with cwd inside a Nova project (default: cwd). Writes nothing anywhere.
# "nova info nosuch.nv" answers a question nobody asked: it blames nova.toml,
# while the actual problem is that the argument does not exist.
NOVA="$1"
[ -x "$NOVA" ] || { echo "usage: sh probe.sh <path-to-nova.exe> [project-dir]"; exit 3; }
[ -n "$2" ] && cd "$2"
[ -f nova.toml ] || { echo "note: no nova.toml in $(pwd) -- cd into a Nova project or pass one"; exit 3; }
echo "-- nova info nosuch.nv:"
out=$(NO_COLOR=1 "$NOVA" info nosuch.nv 2>&1); echo "   rc=$?"; printf '%s\n' "$out" | head -3
echo "-- nova check nosuch.nv  (same tree, same missing file, honest message):"
out=$(NO_COLOR=1 "$NOVA" check nosuch.nv 2>&1); echo "   rc=$?"; printf '%s\n' "$out" | head -2
