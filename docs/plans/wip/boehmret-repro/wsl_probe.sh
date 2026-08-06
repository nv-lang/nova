#!/usr/bin/env bash
# Probe WSL nova checkouts: locate a tree with the spawnctx-fix fiber_arena.c
# (register_native_stack symbol) + populated libuv + a usable nova binary.
set -u
echo "HOME=$HOME"
for p in nova-work nova-211sc-work nova-appeffect-wsl nova-wedge-m nova-work-fix nova-work-cc nova-http; do
  d="/home/craft/$p"
  arena="$d/compiler-codegen/nova_rt/fiber_arena.c"
  if [ -f "$arena" ]; then
    reg=$(grep -c register_native_stack "$arena")
  else
    reg="NOFILE"
  fi
  libuv=$(test -f "$d/compiler-codegen/nova_rt/libuv/include/uv.h" && echo Y || echo N)
  head=$(git -C "$d" rev-parse --short HEAD 2>/dev/null || echo nogit)
  bin=$(test -f "$d/nova-cli/target/release/nova" && echo Y || echo N)
  echo "$p reg=$reg libuv=$libuv head=$head localbin=$bin"
done
echo "--- shared target binaries ---"
ls -la /home/craft/nova-target/release/nova 2>/dev/null
ls -la /home/craft/nova-211sc-target/release/nova 2>/dev/null
ls -la /home/craft/nova-appeffect-target/release/nova 2>/dev/null
ls -la /home/craft/nova-wedge-m-target/release/nova 2>/dev/null
echo "--- CARGO configs (target-dir) ---"
grep -rh target-dir /home/craft/nova-work/.cargo/config* /home/craft/nova-211sc-work/.cargo/config* 2>/dev/null || echo "(none)"
