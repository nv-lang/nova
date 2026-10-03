#!/bin/sh
# Hunt 2026-10-02 cast x K1 -- body shared by every cmd.sh.
#
# A probe is ONE program, <probe>/prog.nv.txt (evidence is not a fixture: the
# suffix keeps it out of every corpus). This body copies it to a temporary
# directory as prog.nv, then from the repository root:
#   1) ORACLE: `nova build prog.nv` and run;
#   2) SUBJECT: `novac emit prog.nv`, the C compiled and linked with the
#      oracle's own flags (the cache of scripts/tools/novac-e1-smoke.sh,
#      warmed on examples/basics/hello.nv when empty), and run;
#   3) both stdouts side by side, and the lines of Carina's C `main` body.
#
# ROOT is found by walking up to AGENTS.md (or pass ROOT=<repo>). NOVAC and
# NOVA override the binaries (default: the repository's; copy them first if
# another run holds them -- os error 5). Every artefact goes to TMPDIR.
[ -n "$PROBE" ] || { echo "run-probe.sh: PROBE (the probe dir) is not set"; exit 2; }
[ -f "$PROBE/prog.nv.txt" ] || { echo "MISSING $PROBE/prog.nv.txt"; exit 2; }
[ -n "$TMPDIR" ] || TMPDIR=/tmp
export TMPDIR
if [ -z "$ROOT" ]; then
    d="$PROBE"
    while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    if [ -f "$d/AGENTS.md" ]; then ROOT="$d"; else echo "run-probe.sh: no repository root above the probe -- set ROOT=<repo>"; exit 1; fi
fi
# Carina's binary through the door (registry 1607): the fresher of novac.exe and novac --
# a stale novac.exe from an older build won by name and judged the wrong compiler.
[ -n "$NOVAC" ] || { . "$ROOT/scripts/guards/lib/novac.sh"; NOVAC=$(novac_bin "$ROOT"); }
[ -n "$NOVA" ]  || NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVAC" ] || { echo "run-probe.sh: no novac at $NOVAC (pass NOVAC=)"; exit 1; }
[ -x "$NOVA" ]  || { echo "run-probe.sh: no oracle at $NOVA (pass NOVA=)"; exit 1; }
CACHE="${NOVAC_SMOKE_CACHE:-$TMPDIR/novac-smoke-cache}"
CL="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
cd "$ROOT" || exit 1
T=$(mktemp -d) || exit 1
cp "$PROBE/prog.nv.txt" "$T/prog.nv" || { echo "run-probe.sh: copy failed"; rm -rf "$T"; exit 2; }
echo "=== PROGRAM"
cat -n "$T/prog.nv"
echo "=== ORACLE: nova build prog.nv, run"
"$NOVA" build "$T/prog.nv" -o "$T/o.exe" > "$T/o.out" 2>&1; echo "oracle build rc=$?"
grep -E "error" "$T/o.out" | sed "s|$T|<tmp>|g" | head -6
if [ -f "$T/o.exe" ]; then "$T/o.exe" > "$T/o.run" 2>&1; echo "oracle run rc=$?"; else echo "(no oracle binary)"; fi
echo "=== SUBJECT: novac emit prog.nv"
"$NOVAC" emit "$T/prog.nv" > "$T/e.c" 2>"$T/e.err"; echo "novac emit rc=$?"
grep '"code"' "$T/e.c" | sed -E "s|$T|<tmp>|g; s|\"file\":\"[^\"]*/prog.nv\"|\"file\":\"<tmp>/prog.nv\"|g" | head -5
echo "--- Carina's C, the body of main"
awk '/nova_fn_main_impl\(void\) \{/{f=1} f{print} f&&/^}/{exit}' "$T/e.c" | head -60
CF=$(ls -t "$CACHE"/cflags-*.argv 2>/dev/null | head -1)
if [ -z "$CF" ]; then
    echo "(warming the smoke cache on examples/basics/hello.nv)"
    NOVAC_BIN="$NOVAC" NOVAC_SMOKE_CACHE="$CACHE" sh "$ROOT/scripts/tools/novac-e1-smoke.sh" examples/basics/hello.nv > /dev/null 2>&1
    CF=$(ls -t "$CACHE"/cflags-*.argv 2>/dev/null | head -1)
fi
LK=$(ls -t "$CACHE"/link-*.argv 2>/dev/null | head -1); PC=$(ls -t "$CACHE"/prelude-*.pch 2>/dev/null | head -1)
echo "=== SUBJECT: Carina's C compiled, linked with the oracle's flags, run"
if [ -s "$T/e.c" ] && ! grep -q '"code"' "$T/e.c" && [ -f "$CF" ] && [ -f "$PC" ]; then
    sed '0,/^#include "nova_rt\/nova_rt.h"$/{//d}' "$T/e.c" > "$T/body.c"
    eval "\"$CL\" $(tr '\n' ' ' < "$CF") -include-pch \"$PC\" -c \"$T/body.c\" -o \"$T/body.o\"" > "$T/cc.out" 2>&1; echo "C compile rc=$?"
    grep -E 'error' "$T/cc.out" | sed "s|$T|<tmp>|g" | head -6
    if [ -f "$T/body.o" ]; then
        eval "\"$CL\" $(tr '\n' ' ' < "$LK") -o \"$T/n.exe\" \"$T/body.o\"" > "$T/ln.out" 2>&1; echo "link rc=$?"
        grep -E "error" "$T/ln.out" | head -3
        if [ -f "$T/n.exe" ]; then "$T/n.exe" > "$T/n.run" 2>&1; echo "novac run rc=$?"; else echo "(no Carina binary)"; fi
    fi
else
    echo "(skipped: refused by novac, or no smoke cache)"
fi
if [ -f "$T/o.run" ] && [ -f "$T/n.run" ]; then
    echo "=== STDOUT, line by line: oracle | Carina"
    paste -d'|' "$T/o.run" "$T/n.run" | tr -d '\r' | awk -F'|' '{ printf "%3d  %-24s| %s\n", NR, $1, $2 }'
    if cmp -s "$T/o.run" "$T/n.run"; then echo "=== VERDICT: SAME stdout"
    else echo "=== VERDICT: DIFFERENT stdout"; fi
else
    echo "=== VERDICT: one side produced no run (see above)"
    [ -f "$T/o.run" ] && { echo "--- oracle stdout"; cat "$T/o.run"; }
    [ -f "$T/n.run" ] && { echo "--- Carina stdout"; cat "$T/n.run"; }
fi
rm -rf "$T"
