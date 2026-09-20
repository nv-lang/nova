#!/usr/bin/env bash
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # probe dir
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-single-mco-resume.sh"
echo "=== A. real tree (invariant truly holds, 60+ runtime files)"
bash "$G" "$ROOT"; echo "rc=$?"
echo "=== B. control 1: nova_rt exists but EMPTY (target gone)"
bash "$G" "$REPO/tree-empty"; echo "rc=$?"
echo "=== C. control 2: naked mco_resume in nova_rt/gc/gc_resume.c (subdir)"
bash "$G" "$REPO/tree-subdir"; echo "rc=$?"
