#!/usr/bin/env bash
# Exercise the existing production-driver dependency guard through the gate's
# standard shell selftest discovery, rather than leaving its Python entry point
# callable only by a separate workflow. This does not build a Nova compiler.
# Plan: docs/plans/274.16-carina-all-examples-and-packages.md, driver boundary.
# The Python --self-test checks the real Cargo graph and a forbidden edge.
# Usage: bash scripts/guards/selftest/test-check-nova-driver-no-codegen.sh
# Any Cargo/guard/selftest failure is propagated unchanged to the gate.

set -eu
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec python "$ROOT/scripts/guards/check-nova-driver-no-codegen.py" "$ROOT" --self-test
