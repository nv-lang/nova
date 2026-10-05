#!/bin/sh
# p4: differential stage 1. Fake novac kills its pool thread on pos_n2 (J=1),
# so .v of lines 2..3 is never written in THIS run.
#  run A (control): fresh $T -> expected red "no outcome"
#  run B: pre-existing $T (same pid, exec) with r/2.v EMPTY and r/3.v = "x x"
#         (any two equal words) -> read as "outcomes matched"?
# NOVAC_SMOKE=0 + NOVAC_DIFF_TIER=push stop the guard after stage 1.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/tmp"
M="$W/root"; mkdir -p "$M/scripts/guards/lib" "$M/novac/fixtures" "$M/nova-cli/target/release"
cp "$ROOT/scripts/guards/lib/novac.sh" "$M/scripts/guards/lib/" || exit 2
cp "$ROOT/scripts/guards/check-novac-differential.sh" "$M/scripts/guards/" || exit 2
for k in 1 2 3; do echo x > "$M/novac/fixtures/pos_n$k.nv"; done
printf '#!/bin/sh\nexit 0\n' > "$M/nova-cli/target/release/nova.exe"; chmod +x "$M/nova-cli/target/release/nova.exe"
printf '#!/bin/sh\ncase "$2" in *pos_n2.nv) echo "killing pool thread $PPID at $2" >> "%s/kill.log"; kill -9 $PPID;; esac\nexit 0\n' "$W" > "$W/bin.sh"; chmod +x "$W/bin.sh"
G="$M/scripts/guards/check-novac-differential.sh"
export TMPDIR="$W/tmp" NOVAC_POOL_JOBS=1 NOVAC_SMOKE=0 NOVAC_DIFF_TIER=push
echo "== run A: fresh T =="
sh "$G" "$M" "$W/bin.sh" > "$W/outA" 2>&1; echo "rc=$?"; grep -v 'Killed\|_np_\|done <' "$W/outA"
echo "== run B: stale T with r/2.v empty, r/3.v='x x' =="
sh -c 'T="$TMPDIR/novac-differential.$$"; mkdir -p "$T/r"; : > "$T/r/2.v"; echo "x x" > "$T/r/3.v"; echo "pre-created $T" >&2; exec sh "$0" "$1" "$2"' "$G" "$M" "$W/bin.sh" > "$W/outB" 2>&1; echo "rc=$?"; grep -v 'Killed\|_np_\|done <' "$W/outB"
echo "== kill.log =="; cat "$W/kill.log" 2>/dev/null || echo "NO kill.log - probe broken"
echo "== leftover dirs in TMPDIR after B =="; ls "$W/tmp"
