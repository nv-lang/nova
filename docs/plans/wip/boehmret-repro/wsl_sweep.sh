#!/usr/bin/env bash
# Rebuild with both discriminators (stack-scan + interior-pointer) and sweep:
#  1) NOVA_GC_STACK_SCAN_KB down to 1 (does tightening the whole-stack push help
#     at ANY window? does it crash = were live roots there?)
#  2) NOVA_GC_NO_INTERIOR=1 (how much of the leak is interior-pointer amplification?)
set -u
TREE=/home/craft/nova-appeffect-wsl
NOVA=/home/craft/nova-appeffect-target/release/nova
SRC=/mnt/d/Sources/nv-lang/nova-boehmret
RT="$TREE/compiler-codegen/nova_rt"
cp "$SRC/compiler-codegen/nova_rt/fiber_arena.c" "$RT/fiber_arena.c"
cp "$SRC/compiler-codegen/nova_rt/alloc_boehm.c" "$RT/alloc_boehm.c"
cp "$SRC/docs/plans/wip/boehmret-repro/boehmret_slope.nv" "$TREE/docs/plans/wip/boehmret-repro/"
cd "$TREE"; export NOVA_RT_DIR="$RT"
"$NOVA" test docs/plans/wip/boehmret-repro/boehmret_slope.nv --keep-artifacts >/tmp/bld.log 2>&1
echo "build exit=$?"; tail -2 /tmp/bld.log
EXE=$(find /tmp -name 'boehmret_slope' -type f 2>/dev/null | head -1)
echo "EXE=$EXE"; [ -x "$EXE" ] || { echo NO_EXE; exit 1; }
runone() {  # $1 = label, rest = env assignment(s)
  local label="$1"; shift
  local out
  out=$(env "$@" timeout 90 "$EXE" 2>&1)
  local L=$(echo "$out" | grep -oE 'LARGE\(16384\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$')
  local S=$(echo "$out" | grep -oE 'SMALL\(64\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$')
  local crash=""; echo "$out" | grep -qE 'SIGSEGV|abort|Segmentation|cancel-throw|overflow' && crash="CRASH!"
  echo "$label : LARGE=${L:-NA} SMALL=${S:-NA} $crash"
}
echo "===== STACK-SCAN SWEEP (LARGE baseline ~86740) ====="
runone "stack_kb=unset "
runone "stack_kb=2048  " NOVA_GC_STACK_SCAN_KB=2048
runone "stack_kb=256   " NOVA_GC_STACK_SCAN_KB=256
runone "stack_kb=32    " NOVA_GC_STACK_SCAN_KB=32
runone "stack_kb=8     " NOVA_GC_STACK_SCAN_KB=8
runone "stack_kb=1     " NOVA_GC_STACK_SCAN_KB=1
echo "===== INTERIOR-POINTER DISCRIMINATOR ====="
runone "no_interior=1  " NOVA_GC_NO_INTERIOR=1
runone "no_interior+kb8" NOVA_GC_NO_INTERIOR=1 NOVA_GC_STACK_SCAN_KB=8
