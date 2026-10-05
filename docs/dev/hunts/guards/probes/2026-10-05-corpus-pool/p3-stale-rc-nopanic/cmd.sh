#!/bin/sh
# p3: no-panic. Fake novac kills its pool thread (its parent subshell) on
# pos_n2, so lines 2..3 of the single thread (J=1) never get a .rc.
#  run A (control): fresh $T                       -> expected red "no result"
#  run B: $T = $TMPDIR/novac-no-panic.<pid> already exists (left by a run the
#         gate killed: trap 0 does not fire on SIGKILL) with r/2.rc EMPTY and
#         r/3.rc = "0" from that run. PID is pinned by exec in sh -c.
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}"
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/tmp"
M="$W/root"; mkdir -p "$M/scripts/guards/lib" "$M/novac/fixtures"
cp "$ROOT/scripts/guards/lib/novac.sh" "$M/scripts/guards/lib/" || exit 2
cp "$ROOT/scripts/guards/check-novac-no-panic.sh" "$M/scripts/guards/" || exit 2
for k in 1 2 3; do echo x > "$M/novac/fixtures/pos_n$k.nv"; done
printf '#!/bin/sh\ncase "$2" in *pos_n2.nv) echo "killing pool thread $PPID at $2" >> "%s/kill.log"; kill -9 $PPID;; esac\nexit 0\n' "$W" > "$W/bin.sh"; chmod +x "$W/bin.sh"
G="$M/scripts/guards/check-novac-no-panic.sh"
export TMPDIR="$W/tmp" NOVAC_POOL_JOBS=1
echo "== run A: fresh T =="
sh "$G" "$M" "$W/bin.sh" > "$W/outA" 2>&1; echo "rc=$?"; head -4 "$W/outA"
echo "== run B: stale T with r/2.rc empty, r/3.rc=0 =="
sh -c 'T="$TMPDIR/novac-no-panic.$$"; mkdir -p "$T/r"; : > "$T/r/2.rc"; echo 0 > "$T/r/3.rc"; echo "pre-created $T" >&2; exec sh "$0" "$1" "$2"' "$G" "$M" "$W/bin.sh" > "$W/outB" 2>&1; echo "rc=$?"; cat "$W/outB"
echo "== kill.log (proves the thread was killed in both runs) =="; cat "$W/kill.log" 2>/dev/null || echo "NO kill.log - probe broken"
echo "== leftover dirs in TMPDIR after B (empty = guard used and removed the pre-created dir) =="; ls "$W/tmp"
