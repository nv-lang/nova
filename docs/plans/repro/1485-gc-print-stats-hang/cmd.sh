#!/bin/bash
# Registry 221.1 #1485: under GC_PRINT_STATS=1, PARALLEL processes of one Nova
# binary hang -- Boehm on Windows writes its log to `<exe>.gc.log` beside the
# executable, and the processes contend for that one file: exactly one runs
# to the end, the others block. Not the #1461 kind (GC roots) -- measured.
#
# Run from the repository root with novac built: novac/target/novac.exe
# (nova build novac/src/main.nv -o novac/target/novac.exe).
# Measured 2026-10-01 on Windows (Git Bash), main 932531589:
#   A  8 parallel `GC_PRINT_STATS=1 novac emit`, 2 rounds -> rc 0 x1, 124 x7 each round
#   B  the same without GC_PRINT_STATS (control)        -> rc 0 x8
#   C  the same with a per-process GC_LOG_FILE (reverse) -> rc 0 x8
#   a single run under GC_PRINT_STATS=1: 15 of 15 finish.
# Note: `timeout` in Git Bash may leave the native exe alive for a while; such a
# survivor holds the log, and then even a SINGLE later run hangs -- which is how
# this was first seen. Check `tasklist | grep novac` before reading a single hang.
N=./novac/target/novac.exe
F=novac/fixtures/display_generic/pos_1.nv
T="${TMPDIR:-/tmp}/p1485.$$"; mkdir -p "$T"; trap 'rm -rf "$T"' 0
TW="$T"; command -v cygpath >/dev/null 2>&1 && TW=$(cygpath -w "$T")
run8() {   # $1 = label, $2 = mode (shared | none | own)
    for k in 1 2 3 4 5 6 7 8; do
        (
            case "$2" in
                shared) GC_PRINT_STATS=1 timeout 40 "$N" emit "$F" > "$T/o$k" 2>&1 < /dev/null ;;
                none)   timeout 40 "$N" emit "$F" > "$T/o$k" 2>&1 < /dev/null ;;
                own)    GC_PRINT_STATS=1 GC_LOG_FILE="$TW\\gc_$k.log" timeout 40 "$N" emit "$F" > "$T/o$k" 2>&1 < /dev/null ;;
            esac
            echo $? > "$T/rc$k"
        ) &
    done
    wait
    echo "$1: rcs $(cat "$T"/rc? | tr '\n' ' ')"
}
run8 "A shared log (GC_PRINT_STATS=1)" shared
run8 "B no GC_PRINT_STATS (control)" none
run8 "C own GC_LOG_FILE per process" own
