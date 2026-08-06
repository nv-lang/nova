#!/usr/bin/env bash
# Overlay patched fiber_arena.c + repro onto the WSL appeffect tree (which
# already has the spawnctx-fix runtime + populated libuv) and build the repro
# test exe ONCE. Then the same exe is re-run with/without NOVA_GC_STACK_SCAN_KB
# for the discriminator (identical binary, env-only difference).
set -u
TREE=/home/craft/nova-appeffect-wsl
NOVA=/home/craft/nova-appeffect-target/release/nova
SRC=/mnt/d/Sources/nv-lang/nova-boehmret
RT="$TREE/compiler-codegen/nova_rt"

cp "$SRC/compiler-codegen/nova_rt/fiber_arena.c" "$RT/fiber_arena.c" || { echo "cp arena FAIL"; exit 1; }
mkdir -p "$TREE/docs/plans/wip/boehmret-repro"
cp "$SRC/docs/plans/wip/boehmret-repro/boehmret_slope.nv" "$TREE/docs/plans/wip/boehmret-repro/" || { echo "cp repro FAIL"; exit 1; }

echo -n "patched arena has discriminator: "
grep -c NOVA_GC_STACK_SCAN_KB "$RT/fiber_arena.c"

cd "$TREE"
export NOVA_RT_DIR="$RT"
echo "=== BUILD+RUN (baseline, no knob) ==="
"$NOVA" test docs/plans/wip/boehmret-repro/boehmret_slope.nv --keep-artifacts 2>&1 | tail -50
echo "=== locate kept exe ==="
find /tmp -name 'boehmret_slope*' -type f -executable -newermt '-10 minutes' 2>/dev/null | head
find "$TREE" -name 'boehmret_slope*' -type f -executable 2>/dev/null | head
