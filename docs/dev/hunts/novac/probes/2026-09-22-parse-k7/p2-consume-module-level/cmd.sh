#!/bin/sh
# usage: sh cmd.sh <repo-root>
set -u
ROOT="${1:-.}"
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT" || exit 1
echo "=== ORACLE (nova check) ==="
./nova-cli/target/release/nova.exe check "$HERE/probe.nv"
echo "=== NOVAC (novac check) ==="
./novac/target/novac.exe check "$HERE/probe.nv"
