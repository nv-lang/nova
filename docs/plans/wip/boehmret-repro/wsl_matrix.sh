#!/usr/bin/env bash
# Rebuild with the compute-only test + run an env matrix to LOCATE the channel:
#   - COMPUTE-LARGE vs NET-LARGE: is net/libuv required for the leak?
#   - NOVA_AUTOARM=0 (cooperative, no M:N workers/driver): M:N-specific?
#   - NOVA_MAXPROCS=1 vs default: worker-count dependence?
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
echo "build exit=$?"; grep -E 'SUMMARY|PASS:|FAIL:|error' /tmp/bld.log | tail -4
EXE=$(find /tmp -name 'boehmret_slope' -type f 2>/dev/null | head -1)
echo "EXE=$EXE"; [ -x "$EXE" ] || { echo NO_EXE; tail -20 /tmp/bld.log; exit 1; }
runone() {
  local label="$1"; shift
  local out; out=$(env "$@" timeout 120 "$EXE" 2>&1)
  local C=$(echo "$out" | grep -oE 'COMPUTE-LARGE\([0-9]+\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$' | tail -1)
  local L=$(echo "$out" | grep -oE '[^-]LARGE\(16384\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$' | tail -1)
  local S=$(echo "$out" | grep -oE 'SMALL\(64\) slope = -?[0-9]+' | grep -oE '\-?[0-9]+$' | tail -1)
  local crash=""; echo "$out" | grep -qE 'SIGSEGV|abort|Segmentation|cancel-throw|overflow' && crash="CRASH"
  echo "$label : COMPUTE=${C:-NA} NET=${L:-NA} SMALL=${S:-NA} $crash"
}
echo "===== MATRIX (COMPUTE/NET LARGE ~?/86740, SMALL plateau) ====="
runone "default        "
runone "AUTOARM=0      " NOVA_AUTOARM=0
runone "MAXPROCS=1     " NOVA_MAXPROCS=1
runone "MAXPROCS=8     " NOVA_MAXPROCS=8
