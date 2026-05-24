#!/usr/bin/env bash
# Plan 83.4.5.6 Ф.5 (2026-05-24): TSAN gate — clang `-fsanitize=thread`
# на concurrency tests; acceptance = 0 reported races.
#
# Linux-only — Windows clang не поддерживает TSAN native (требует
# WSL2 / Linux VM). MSVC не имеет TSAN equivalent (Application Verifier
# покрывает races частично, не строгий ThreadSanitizer).
#
# Usage:
#   ./scripts/tsan_concurrency.sh [filter]
#
# Example:
#   ./scripts/tsan_concurrency.sh concurrency/  # all concurrency
#   ./scripts/tsan_concurrency.sh fiber_throw   # single test
#
# Requirements:
#   - Linux host (Ubuntu 22.04+ либо WSL2).
#   - clang LLVM 15+ с TSAN runtime.
#   - Boehm GC compiled с GC_THREADS (vcpkg / system package).
#   - Nova cli built с release mode.
#
# Output: TSAN findings приходят на stderr через TSAN runtime; non-zero
# exit code = races detected.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
    echo "ERROR: TSAN gate Linux-only (Plan 83.4.5.6 §6.4 acceptance)."
    echo "Current platform: $(uname -s)"
    echo "Windows users: run from WSL2 либо Linux VM."
    exit 1
fi

FILTER="${1:-concurrency/}"
NOVA_BIN="${NOVA_BIN:-./nova-cli/target/release/nova}"

if [[ ! -x "$NOVA_BIN" ]]; then
    echo "ERROR: nova binary not found at $NOVA_BIN"
    echo "Build с: cd nova-cli && cargo build --release --features z3-backend"
    exit 1
fi

# TSAN flags для clang (приложение должно compile'иться с -fsanitize=thread
# + link). Nova test_runner build_command для clang добавляет CFLAGS
# через NOVA_CC_EXTRA env.
export NOVA_CC_EXTRA="-fsanitize=thread -g -O1"
export NOVA_LD_EXTRA="-fsanitize=thread"

# TSAN options.
export TSAN_OPTIONS="halt_on_error=1 second_deadlock_stack=1 history_size=7 \
    print_suppressions=0 report_thread_leaks=0 \
    suppressions=$(dirname "$0")/tsan_suppressions.txt"

echo "=== Plan 83.4.5.6 Ф.5: TSAN gate run ==="
echo "Filter:  $FILTER"
echo "Binary:  $NOVA_BIN"
echo "CFLAGS:  $NOVA_CC_EXTRA"

"$NOVA_BIN" test --toolchain clang --mode dev --filter "$FILTER" 2>&1 | \
    tee /tmp/nova-tsan-output.log

# Check for TSAN race reports.
if grep -q "WARNING: ThreadSanitizer" /tmp/nova-tsan-output.log; then
    echo ""
    echo "❌ TSAN: races detected — see output above."
    exit 1
fi

echo ""
echo "✅ TSAN gate: 0 races detected (acceptance MET per Plan 83.4.5.6 §6.4)."
