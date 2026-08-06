#!/usr/bin/env bash
# Restore the borrowed non-git WSL appeffect build tree by writing back the
# PRISTINE (pre-patch) runtime files from my worktree's base commit ce0ab9e00.
set -u
TREE=/home/craft/nova-appeffect-wsl
WT=/mnt/d/Sources/nv-lang/nova-boehmret
for f in fiber_arena.c alloc_boehm.c net.c; do
  if git -C "$WT" show "ce0ab9e00:compiler-codegen/nova_rt/$f" > "$TREE/compiler-codegen/nova_rt/$f" 2>/tmp/rst.err; then
    echo "restored $f from ce0ab9e00 ($(wc -l < "$TREE/compiler-codegen/nova_rt/$f") lines)"
  else
    echo "FAILED $f: $(cat /tmp/rst.err)"
  fi
done
echo -n "discriminator gone from arena: "; grep -c NOVA_GC_STACK_SCAN_KB "$TREE/compiler-codegen/nova_rt/fiber_arena.c"
echo -n "fix gone from net.c: "; grep -c 'M-boehm-large-buffer-retention' "$TREE/compiler-codegen/nova_rt/net.c"
rm -f "$TREE/docs/plans/wip/boehmret-repro/boehmret_slope.nv"
rmdir "$TREE/docs/plans/wip/boehmret-repro" 2>/dev/null
rmdir "$TREE/docs/plans/wip" 2>/dev/null
echo "cleanup done"
