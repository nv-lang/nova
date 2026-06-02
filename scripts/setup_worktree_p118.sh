#!/usr/bin/env bash
# Plan 118 worktree setup script — automates libuv submodule + GC env vars
# для release nova test runs в nova-p118 worktree.
#
# Per memory project-worktree-nova-test-setup. Usage:
#   bash scripts/setup_worktree_p118.sh
#
# After running, env vars NOVA_GC_INCLUDE_DIR / NOVA_GC_LIB_DIR are set
# для current shell session pointing к main repo vcpkg_installed.
# libuv submodule copied + .git removed; prebuilt libuv.lib placed в
# target/libuv-cache (avoids ~30sec libuv rebuild).

set -euo pipefail

WORKTREE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAIN_REPO="D:/Sources/nv-lang/nova"

if [ ! -d "$MAIN_REPO/compiler-codegen/nova_rt/libuv" ]; then
    echo "FATAL: main repo libuv не found at $MAIN_REPO"
    echo "Update MAIN_REPO path в script."
    exit 1
fi

echo "[1/4] Copying libuv submodule from main repo ..."
LIBUV_DST="$WORKTREE_ROOT/compiler-codegen/nova_rt/libuv"
if [ -d "$LIBUV_DST" ] && [ -n "$(ls -A "$LIBUV_DST" 2>/dev/null)" ]; then
    echo "      libuv уже populated; skip"
else
    rm -rf "$LIBUV_DST"
    cp -r "$MAIN_REPO/compiler-codegen/nova_rt/libuv" "$WORKTREE_ROOT/compiler-codegen/nova_rt/"
    rm -rf "$LIBUV_DST/.git"
    echo "      libuv copied + .git removed"
fi

echo "[2/4] Copying prebuilt libuv.lib (avoids ~30sec libuv rebuild) ..."
mkdir -p "$WORKTREE_ROOT/target/libuv-cache" "$WORKTREE_ROOT/compiler-codegen/target/libuv-cache"
if [ -f "$MAIN_REPO/target/libuv-cache/libuv.lib" ]; then
    cp "$MAIN_REPO/target/libuv-cache/libuv.lib" "$WORKTREE_ROOT/target/libuv-cache/" 2>/dev/null || true
fi
if [ -f "$MAIN_REPO/compiler-codegen/target/libuv-cache/libuv.lib" ]; then
    cp "$MAIN_REPO/compiler-codegen/target/libuv-cache/libuv.lib" "$WORKTREE_ROOT/compiler-codegen/target/libuv-cache/" 2>/dev/null || true
fi
echo "      prebuilt libuv.lib copied (if available в main)"

echo "[3/4] Setup GC env vars (vcpkg_installed paths)..."
GC_INC="$MAIN_REPO/compiler-codegen/vcpkg_installed/x64-windows-static/include"
GC_LIB="$MAIN_REPO/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
if [ ! -f "$GC_INC/gc.h" ]; then
    echo "      WARNING: gc.h не found at $GC_INC. vcpkg install required в main repo:"
    echo "        cd $MAIN_REPO/compiler-codegen && vcpkg install bdwgc:x64-windows-static"
fi

cat <<EOF

[4/4] Done. Set env vars в current shell для test runs:

    export NOVA_GC_INCLUDE_DIR="$GC_INC"
    export NOVA_GC_LIB_DIR="$GC_LIB"

Then test plan118 (release build, clang toolchain):

    cd "$WORKTREE_ROOT"
    cargo build --release --bin nova-codegen
    ./target/release/nova-codegen test-all --tests-dir nova_tests/plan118

Or quick debug build:

    cargo build --bin nova-codegen
    ./compiler-codegen/target/debug/nova-codegen test-all --tests-dir nova_tests/plan118
EOF
