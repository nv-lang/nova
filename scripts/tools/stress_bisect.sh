#!/usr/bin/env bash
# stress_bisect.sh — git-bisect-compatible stress test harness for stochastic
# concurrency bugs in Nova test fixtures.
#
# Born during Plan 83.11 §12.27 to identify the commit that introduced
# [M-83.11-supervised-spawn-cancel-memcpy-segv]. Bisect converged in 3
# iterations across 10 commits. Retained as reusable tool — works for ANY
# Nova test that exhibits stochastic SEGV / hang under load.
#
# ## Usage
#
#   tools/stress_bisect.sh <test.nv> [stress_n=15]
#
#   <test.nv>   Path to Nova test file. Must contain a `test` block. Relative
#               to repo root.
#   stress_n    Number of times to run the built .exe (default 15).
#
# Example for manual verification:
#   $ tools/stress_bisect.sh nova_tests/concurrency/cancel_semantics_test.nv 30
#
# Example for git bisect run:
#   $ git bisect start
#   $ git bisect bad HEAD
#   $ git bisect good <known-good-sha>
#   $ git bisect run tools/stress_bisect.sh nova_tests/concurrency/_my_test.nv
#
# ## Exit codes (git bisect protocol)
#
#   0    All stress_n iterations PASSED (GOOD)
#   1    At least one iteration FAILED — SEGV, abort, non-zero exit (BAD)
#   125  Build/compile failure — bisect should SKIP this commit
#
# ## Required env vars (Windows Boehm build)
#
#   NOVA_GC_LIB_DIR      Path to libgc.lib (vcpkg)
#   NOVA_GC_INCLUDE_DIR  Path to gc.h (vcpkg)
#
# If unset, script falls back to known main worktree path.
#
# ## How stress_n is chosen
#
# At repro rate p, probability of false-GOOD with n runs is (1-p)^n.
# Plan 83.11 §12.27 bug: p≈0.4 → n=15 gives P(false-good)=0.0047. For lower
# p, increase n. Heuristic: n ≥ ceil(7 / p).
#
# ## Why no `set -e`
#
# We WANT to count non-zero exits (SEGV → 139, abort → 134, etc.) without
# bailing. Iteration loop catches exit codes explicitly via $?.
#
# Bash on Windows/MSYS can propagate child-SIGSEGV to parent when stdin/stdout
# are piped — we run the child in background (& wait) to isolate signal
# handling.

set +e

TEST_FILE="${1:-}"
STRESS_N="${2:-15}"

if [ -z "$TEST_FILE" ]; then
    echo "usage: $0 <test.nv> [stress_n=15]" >&2
    exit 2
fi
if [ ! -f "$TEST_FILE" ]; then
    echo "[stress-bisect] test file not found: $TEST_FILE" >&2
    exit 2
fi

# Boehm GC paths — fallback to canonical main-worktree location if unset.
: "${NOVA_GC_LIB_DIR:=D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib}"
: "${NOVA_GC_INCLUDE_DIR:=D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include}"
export NOVA_GC_LIB_DIR NOVA_GC_INCLUDE_DIR

echo "[stress-bisect] cargo build --release..." >&2
(cd nova-cli && cargo build --release --quiet 2>&1 | tail -3) >&2
if [ $? -ne 0 ]; then
    echo "[stress-bisect] BUILD FAIL → skip (exit 125)" >&2
    exit 125
fi

# Derive test exe basename (matches nova test output naming).
TEST_BASE=$(basename "$TEST_FILE" .nv)

echo "[stress-bisect] nova test --keep-artifacts $TEST_FILE..." >&2
NOVA_LOG=$(mktemp)
./nova-cli/target/release/nova.exe test "$TEST_FILE" --keep-artifacts > "$NOVA_LOG" 2>&1
NOVA_EC=$?

# Locate fresh .exe (mtime within last 2 minutes — accounts for cached builds).
EXE=$(find /tmp/nova_tests -name "${TEST_BASE}.exe" -mmin -2 2>/dev/null | head -1)
if [ -z "$EXE" ]; then
    echo "[stress-bisect] EXE not found (nova test exit=$NOVA_EC) → skip" >&2
    tail -20 "$NOVA_LOG" >&2
    rm -f "$NOVA_LOG"
    exit 125
fi
rm -f "$NOVA_LOG"

# Stress loop — count PASS / FAIL.
# Child runs in background (& wait) to prevent SIGSEGV signal propagation
# killing this script before the iteration counter increments.
p=0
f=0
for i in $(seq 1 "$STRESS_N"); do
    "$EXE" > /dev/null 2>&1 &
    pid=$!
    wait $pid 2>/dev/null
    ec=$?
    if [ "$ec" -eq 0 ]; then
        p=$((p+1))
    else
        f=$((f+1))
    fi
done

echo "[stress-bisect] PASS=$p FAIL=$f / $STRESS_N" >&2

if [ "$f" -eq 0 ]; then
    exit 0
else
    exit 1
fi
