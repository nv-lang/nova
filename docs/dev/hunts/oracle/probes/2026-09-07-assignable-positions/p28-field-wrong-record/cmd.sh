#!/bin/sh
# Run from the nova worktree root (the directory that holds `std/` and `nova-cli/`).
D=$(dirname "$0")
NOVA=nova-cli/target/release/nova.exe
NOVA_STD_PATH="$PWD/std"; export NOVA_STD_PATH
rm -f "$D/probe.exe"
"$NOVA" build "$D/probe.nv" -o "$D/probe.exe" 2>&1 | grep -v vcpkg | grep -E 'error|warning:' | head -8
if [ -x "$D/probe.exe" ]; then
    echo "--- ACCEPTED by the checker; running it:"
    "$D/probe.exe"
    rc=$?
    echo "[exit=$rc]"
    rm -f "$D/probe.exe"
else
    echo "--- REFUSED (no binary produced)"
fi
