#!/bin/sh
# Body shared by every cmd.sh: P must be set to the probe's absolute path.
# 1) novac check; 2) novac emit, the handler/capture/module-value lines of the
# C shown; 3) the behaviour differential: scripts/tools/novac-e1-smoke.sh
# (oracle binary vs novac's C linked with the oracle's flags).
[ -f "$P" ] || { echo "MISSING $P"; exit 2; }
. "$(dirname "$P")/../env.sh"
cd "$ROOT" || exit 1
echo "=== SUBJECT: novac check"
timeout 120 "$NOVAC" check "$P"; echo "novac check rc=$?"
T=$(mktemp -d) || exit 1
echo "=== SUBJECT: novac emit (handler / capture / module-value / slot lines)"
timeout 120 "$NOVAC" emit "$P" > "$T/e.c" 2>"$T/e.err"; echo "novac emit rc=$?"
head -c 600 "$T/e.err"
grep -n '"code"' "$T/e.c" | head -5
grep -nE 'novac_hop_|_novac_cap|_cx->|novac_hctx_|novac_mv_|->ctx =|nova_eff_slot|_slot_|NovaVtable_[A-Za-z0-9_]*\* _novac' "$T/e.c" | grep -v '^[0-9]*:#' | head -60
echo "=== DIFFERENTIAL: novac-e1-smoke (oracle build+run vs novac C)"
# the smoke finds its oracle through its cache: pin it to $NOVA first.
mkdir -p "${NOVAC_SMOKE_CACHE:-$TMPDIR/novac-smoke-cache}"; printf '%s\n' "$NOVA" > "${NOVAC_SMOKE_CACHE:-$TMPDIR/novac-smoke-cache}/oracle.path"
NOVAC_BIN="$NOVAC" timeout 300 sh "$ROOT/scripts/tools/novac-e1-smoke.sh" "$P" 2>&1 | head -30
echo "=== SUBJECT ALONE: novac C compiled and run with the smoke's cached oracle flags"
C="${NOVAC_SMOKE_CACHE:-$TMPDIR/novac-smoke-cache}"
CF=$(ls -t "$C"/cflags-*.argv 2>/dev/null | head -1); LK=$(ls -t "$C"/link-*.argv 2>/dev/null | head -1); PC=$(ls -t "$C"/prelude-*.pch 2>/dev/null | head -1)
CL="${NOVA_CLANG:-C:/Program Files/LLVM/bin/clang.exe}"
if [ -s "$T/e.c" ] && [ -f "$CF" ] && [ -f "$PC" ] && ! grep -q '"code"' "$T/e.c"; then
  sed '0,/^#include "nova_rt\/nova_rt.h"$/{//d}' "$T/e.c" > "$T/body.c"
  eval "\"$CL\" $(tr '\n' ' ' < "$CF") -include-pch \"$PC\" -c \"$T/body.c\" -o \"$T/body.o\"" > "$T/cc.out" 2>&1; echo "novac C compile rc=$?"
  grep -E 'error' "$T/cc.out" | head -6
  if [ -f "$T/body.o" ]; then
    eval "\"$CL\" $(tr '\n' ' ' < "$LK") -o \"$T/n.exe\" \"$T/body.o\"" > "$T/ln.out" 2>&1; echo "novac link rc=$?"; head -3 "$T/ln.out"
    [ -f "$T/n.exe" ] && { timeout 60 "$T/n.exe"; echo "novac run rc=$?"; }
  fi
else
  echo "(skipped: no emission, or no smoke cache yet)"
fi
rm -rf "$T"
