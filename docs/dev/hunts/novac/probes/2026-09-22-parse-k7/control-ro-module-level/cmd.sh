#!/bin/sh
# usage: sh cmd.sh <repo-root>
# CONTROL: the sibling form (ro) already reads correctly at module level.
# novac's remaining complaint must point PAST the binding itself (name
# resolution inside main, not the parse of the `ro` line) -- proving the
# `@let_decl` door itself is not the thing failing for mut/consume.
set -u
ROOT="${1:-.}"
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT" || exit 1
echo "=== ORACLE (nova check) ==="
./nova-cli/target/release/nova.exe check "$HERE/probe.nv"
echo "=== NOVAC (novac check) ==="
./novac/target/novac.exe check "$HERE/probe.nv"
