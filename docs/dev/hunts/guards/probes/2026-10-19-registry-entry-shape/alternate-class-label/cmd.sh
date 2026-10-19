#!/usr/bin/env bash
set -eu
: "${NOVA_REPO_ROOT:?set NOVA_REPO_ROOT to the checkout root}"
TREE="$(cd "$(dirname "$0")/tree" && pwd)"
NOVA_REGSHAPE_BASELINE="$TREE/scripts/guards/registry-entry-shape.baseline" \
  bash "$NOVA_REPO_ROOT/scripts/guards/check-registry-entry-shape.sh" "$TREE"
