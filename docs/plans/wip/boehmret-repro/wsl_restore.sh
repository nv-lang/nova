#!/usr/bin/env bash
# Restore the borrowed WSL appeffect build-host tree's runtime files that this
# investigation overlaid, so the tree is left as found.
set -u
TREE=/home/craft/nova-appeffect-wsl
cd "$TREE" || exit 1
echo "git status of tree:"; git -C "$TREE" rev-parse --is-inside-work-tree 2>&1
git -C "$TREE" checkout -- compiler-codegen/nova_rt/fiber_arena.c \
                           compiler-codegen/nova_rt/alloc_boehm.c \
                           compiler-codegen/nova_rt/net.c 2>&1 && echo "restored via git" || echo "git checkout failed"
echo "residual repro file (harmless, untracked):"
ls "$TREE/docs/plans/wip/boehmret-repro/boehmret_slope.nv" 2>/dev/null
rm -f "$TREE/docs/plans/wip/boehmret-repro/boehmret_slope.nv"
rmdir "$TREE/docs/plans/wip/boehmret-repro" 2>/dev/null
echo "post-restore diff stat:"; git -C "$TREE" diff --stat -- compiler-codegen/nova_rt/ 2>&1 | tail -5
