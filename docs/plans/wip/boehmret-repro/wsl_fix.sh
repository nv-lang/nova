#!/usr/bin/env bash
# Build WITH the net.c fix (clear stream's retained I/O-buffer pointers) and
# measure NET-LARGE slope 3x. Baseline (pre-fix) NET-LARGE ~= 86740 bytes/iter.
set -u
TREE=/home/craft/nova-appeffect-wsl
NOVA=/home/craft/nova-appeffect-target/release/nova
SRC=/mnt/d/Sources/nv-lang/nova-boehmret
RT="$TREE/compiler-codegen/nova_rt"
cp "$SRC/compiler-codegen/nova_rt/fiber_arena.c" "$RT/fiber_arena.c"
cp "$SRC/compiler-codegen/nova_rt/alloc_boehm.c" "$RT/alloc_boehm.c"
cp "$SRC/compiler-codegen/nova_rt/net.c"         "$RT/net.c"
cp "$SRC/docs/plans/wip/boehmret-repro/boehmret_slope.nv" "$TREE/docs/plans/wip/boehmret-repro/"
echo -n "net.c fix markers: "; grep -c 'M-boehm-large-buffer-retention' "$RT/net.c"
cd "$TREE"; export NOVA_RT_DIR="$RT"
"$NOVA" test docs/plans/wip/boehmret-repro/boehmret_slope.nv --keep-artifacts >/tmp/bld.log 2>&1
echo "build exit=$?"; grep -E 'SUMMARY|PASS:|FAIL:' /tmp/bld.log | tail -3
EXE=$(find /tmp -name 'boehmret_slope' -type f 2>/dev/null | head -1)
echo "EXE=$EXE"; [ -x "$EXE" ] || { echo NO_EXE; tail -25 /tmp/bld.log; exit 1; }
echo "===== POST-FIX (NET-LARGE baseline pre-fix ~86740) ====="
for r in 1 2 3; do
  out=$(timeout 120 "$EXE" 2>&1)
  C=$(echo "$out" | grep -oE 'COMPUTE-LARGE\([0-9]+\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$' | tail -1)
  L=$(echo "$out" | grep -oE '[^-]LARGE\(16384\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$' | tail -1)
  S=$(echo "$out" | grep -oE 'SMALL\(64\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$' | tail -1)
  cr=""; echo "$out" | grep -qE 'SIGSEGV|abort|Segmentation|cancel-throw|overflow' && cr=CRASH
  echo "run $r : COMPUTE=${C:-NA} NET=${L:-NA} SMALL=${S:-NA} $cr"
done
