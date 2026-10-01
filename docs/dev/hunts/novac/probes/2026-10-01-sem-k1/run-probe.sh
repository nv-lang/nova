#!/bin/sh
# Hunt 2026-10-01 sem x K1 -- body shared by every cmd.sh.
#
# A probe is a PACKAGE: its files live under <probe>/pkg/ with the suffixes
# `.nv.txt` and `nova.toml.txt` (evidence is not a fixture), and this body
# copies them to a temporary package with the real names, then:
#   1) ORACLE: `nova build <pkg>/src/main.nv` and run;
#   2) SUBJECT: `novac check` and `novac emit` of the same entry with
#      NOVAC_SELF_PATH=<pkg>/src -- the program's modules handed to Carina,
#      the way the E.10 unit self-build hands novac/src (274.11 E.10 step 2a);
#      lines of the C matching $GREP are shown;
#   3) the C of Carina compiled and linked with the oracle's own flags
#      (the cache of scripts/tools/novac-e1-smoke.sh, warmed on hello.nv
#      when empty), run, and stdout compared byte for byte.
# Optional: EMIT_ALSO=<path inside pkg> -- emit that file too (the OWNER's
# unit) and show its $GREP lines, for a name two units must agree on.
#
# ROOT is found by walking up to AGENTS.md (or pass ROOT=<repo>). NOVAC and
# NOVA override the binaries (default: the repository's). Every artefact goes
# to TMPDIR.
[ -n "$PROBE" ] || { echo "run-probe.sh: PROBE (the probe dir) is not set"; exit 2; }
[ -d "$PROBE/pkg" ] || { echo "MISSING $PROBE/pkg"; exit 2; }
[ -n "$TMPDIR" ] || TMPDIR=/tmp
export TMPDIR
if [ -z "$ROOT" ]; then
    d="$PROBE"
    while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    if [ -f "$d/AGENTS.md" ]; then ROOT="$d"; else echo "run-probe.sh: no repository root above the probe -- set ROOT=<repo>"; exit 1; fi
fi
[ -n "$NOVAC" ] || NOVAC="$ROOT/novac/target/novac.exe"
[ -n "$NOVA" ]  || NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVAC" ] || { echo "run-probe.sh: no novac at $NOVAC (pass NOVAC=)"; exit 1; }
[ -x "$NOVA" ]  || { echo "run-probe.sh: no oracle at $NOVA (pass NOVA=)"; exit 1; }
CACHE="${NOVAC_SMOKE_CACHE:-$TMPDIR/novac-smoke-cache}"
CL="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
cd "$ROOT" || exit 1
T=$(mktemp -d) || exit 1
# ---- the package, with its real names ------------------------------------
( cd "$PROBE/pkg" && find . -type f ) | while IFS= read -r f; do
    dst="$T/pkg/$(printf '%s' "$f" | sed 's/\.nv\.txt$/.nv/; s/nova\.toml\.txt$/nova.toml/')"
    mkdir -p "$(dirname "$dst")"; cp "$PROBE/pkg/$f" "$dst"
done
[ -f "$T/pkg/src/main.nv" ] || { echo "run-probe.sh: the package has no src/main.nv"; rm -rf "$T"; exit 2; }
echo "=== PACKAGE"
( cd "$T/pkg" && find . -name '*.nv' | sort )
echo "=== ORACLE: nova build src/main.nv, run"
"$NOVA" build "$T/pkg/src/main.nv" -o "$T/o.exe" > "$T/o.out" 2>&1; echo "oracle build rc=$?"
grep -E "error" "$T/o.out" | sed "s|$T|<tmp>|g" | head -6
if [ -f "$T/o.exe" ]; then "$T/o.exe" > "$T/o.run" 2>&1; echo "oracle run rc=$?"; cat "$T/o.run"; fi
echo "=== SUBJECT: novac check (NOVAC_SELF_PATH=<pkg>/src)"
NOVAC_SELF_PATH="$T/pkg/src" "$NOVAC" check "$T/pkg/src/main.nv" > "$T/c.out" 2>&1; rc=$?
sed "s|$T|<tmp>|g" "$T/c.out"; echo "novac check rc=$rc"
echo "=== SUBJECT: novac emit"
NOVAC_SELF_PATH="$T/pkg/src" "$NOVAC" emit "$T/pkg/src/main.nv" > "$T/e.c" 2>"$T/e.err"; echo "novac emit rc=$?"
grep '"code"' "$T/e.c" | sed "s|$T|<tmp>|g" | head -5
[ -n "$GREP" ] && { echo "--- C lines matching: $GREP"; grep -nE "$GREP" "$T/e.c" | head -20; }
if [ -n "$EMIT_ALSO" ]; then
    echo "=== SUBJECT: novac emit of the owner unit $EMIT_ALSO"
    NOVAC_SELF_PATH="$T/pkg/src" "$NOVAC" emit "$T/pkg/$EMIT_ALSO" > "$T/e2.c" 2>&1; echo "novac emit rc=$?"
    [ -n "$GREP" ] && grep -nE "$GREP" "$T/e2.c" | head -12
fi
# ---- the flags: the smoke's cache, warmed once ----------------------------
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
        if [ -f "$T/n.exe" ]; then "$T/n.exe" > "$T/n.run" 2>&1; echo "novac run rc=$?"; cat "$T/n.run"; fi
    fi
else
    echo "(skipped: refused by novac, or no smoke cache)"
fi
if [ -f "$T/o.run" ] && [ -f "$T/n.run" ]; then
    if cmp -s "$T/o.run" "$T/n.run"; then echo "=== VERDICT: SAME stdout"
    else echo "=== VERDICT: DIFFERENT stdout (< oracle, > Carina)"; diff "$T/o.run" "$T/n.run" | head -12; fi
fi
rm -rf "$T"
