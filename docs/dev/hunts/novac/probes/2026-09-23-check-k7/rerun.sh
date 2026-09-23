#!/bin/sh
# Re-run every probe of the 2026-09-23 check x K7 hunt and rewrite its run.out.
# Run: sh rerun.sh from THIS directory. Each probe cds to the repo root itself
# (novac check finds std/src only from there). One line per probe on stdout.
here=$(pwd)
for d in "$here"/*/; do
    n=$(basename "$d")
    if [ ! -f "$d/cmd.sh" ] || [ ! -f "$d/probe.nv" ]; then
        echo "$n: MISSING cmd.sh or probe.nv"
        continue
    fi
    (cd "$d" && sh cmd.sh) > "$d/run.out" 2>&1
    if [ ! -s "$d/run.out" ]; then
        echo "$n: EMPTY run.out"
        continue
    fi
    echo "$n: $(grep -oE 'novac rc=[0-9]+|oracle check rc=[0-9]+|oracle build rc=[0-9]+' "$d/run.out" | tr '\n' ' ')"
done
