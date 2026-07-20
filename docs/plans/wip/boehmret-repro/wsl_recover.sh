#!/usr/bin/env bash
# Recover the appeffect build tree's 3 runtime files (accidentally truncated)
# by copying the pristine post-spawnctx versions from the sibling nova-211sc-work
# tree (same generation, no boehmret patches).
set -u
DST=/home/craft/nova-appeffect-wsl/compiler-codegen/nova_rt
SRC=/home/craft/nova-211sc-work/compiler-codegen/nova_rt
for f in fiber_arena.c alloc_boehm.c net.c; do
  s="$SRC/$f"; d="$DST/$f"
  if [ -s "$s" ]; then
    cp "$s" "$d"
    echo "recovered $f: $(wc -l < "$d") lines (src reg/fix check: arena=$(grep -c register_native_stack "$s" 2>/dev/null))"
  else
    echo "SOURCE MISSING/EMPTY: $s"
  fi
done
echo "verify appeffect tree now valid + free of boehmret patches:"
echo -n "  arena lines: "; wc -l < "$DST/fiber_arena.c"
echo -n "  arena has spawnctx(register_native_stack): "; grep -c register_native_stack "$DST/fiber_arena.c"
echo -n "  arena has boehmret discriminator: "; grep -c NOVA_GC_STACK_SCAN_KB "$DST/fiber_arena.c"
echo -n "  net.c lines: "; wc -l < "$DST/net.c"
echo -n "  net.c has boehmret fix: "; grep -c 'M-boehm-large-buffer-retention' "$DST/net.c"
echo -n "  alloc_boehm lines: "; wc -l < "$DST/alloc_boehm.c"
