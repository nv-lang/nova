#!/bin/sh
# run from the nova worktree root
sh scripts/tools/novac-e1-smoke.sh "$(cd "$(dirname "$0")" && pwd)/vec_of_newtype.nv"
