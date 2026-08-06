#!/usr/bin/env bash
# One-session: overlay + build + directly run the kept exe (ARMED M:N) with and
# without NOVA_GC_STACK_SCAN_KB. Keeps everything in one WSL session so the
# /tmp artifact is not cleaned between calls.
set -u
TREE=/home/craft/nova-appeffect-wsl
NOVA=/home/craft/nova-appeffect-target/release/nova
SRC=/mnt/d/Sources/nv-lang/nova-boehmret
RT="$TREE/compiler-codegen/nova_rt"

cp "$SRC/compiler-codegen/nova_rt/fiber_arena.c" "$RT/fiber_arena.c"
mkdir -p "$TREE/docs/plans/wip/boehmret-repro"
cp "$SRC/docs/plans/wip/boehmret-repro/boehmret_slope.nv" "$TREE/docs/plans/wip/boehmret-repro/"
cd "$TREE"
export NOVA_RT_DIR="$RT"

echo "=== BUILD (keep artifacts) ==="
"$NOVA" test docs/plans/wip/boehmret-repro/boehmret_slope.nv --keep-artifacts >/tmp/boehmret_build.log 2>&1
echo "build exit=$? ; tail:"; tail -3 /tmp/boehmret_build.log
EXE=$(find /tmp -name 'boehmret_slope' -type f 2>/dev/null | head -1)
echo "EXE=$EXE"
if [ -z "$EXE" ] || [ ! -x "$EXE" ]; then echo "NO EXE"; exit 1; fi

echo "=== sanity: one raw run (full stdout) ==="
timeout 120 "$EXE" 2>&1 | grep -E '\[boehmret\]|PASS|FAIL' | head

echo "############ BASELINE (whole-stack push) ############"
for r in 1 2 3; do
  echo "--- baseline $r ---"
  timeout 120 "$EXE" 2>&1 | grep -E '\[boehmret\]' || echo "(crash/none)"
done
echo "############ TIGHT NOVA_GC_STACK_SCAN_KB=64 ############"
for r in 1 2 3; do
  echo "--- tight $r ---"
  NOVA_GC_STACK_SCAN_KB=64 timeout 120 "$EXE" 2>&1 | grep -E '\[boehmret\]' || echo "(crash/none)"
done
