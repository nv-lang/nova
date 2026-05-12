#!/usr/bin/env bash
# Plan 40 Ф.1 Этап 5: run nova tests inside Linux Docker container.
#
# Usage (from container):
#   ./docker/run-tests.sh
#
# Honours $NOVA_SANITIZER env var (set by Dockerfile build-arg):
#   none  — runs полный 262/262 regression.
#   tsan  — runs sanitizers tests + Plan 40 channel stress.
#   asan  — runs sanitizers tests + Plan 40 channel stress.
#   ubsan — runs sanitizers tests + Plan 40 channel stress.

set -euo pipefail

cd /nova

NOVA_BIN=nova-cli/target/release/nova
if [ ! -x "$NOVA_BIN" ]; then
    echo "ERROR: nova binary not found at $NOVA_BIN"
    exit 1
fi

SANITIZER="${NOVA_SANITIZER:-none}"

echo "=== Plan 40 Linux tests (SANITIZER=$SANITIZER) ==="

case "$SANITIZER" in
    none)
        # Полный regression — 262/262.
        echo ""
        echo "--- Full regression ---"
        $NOVA_BIN test
        echo ""
        echo "--- std type-check ---"
        $NOVA_BIN check std/
        ;;
    tsan|asan|ubsan)
        # Plan 40 focused channel tests с sanitizer'ом.
        echo ""
        echo "--- Plan 40 channel tests under $SANITIZER ---"
        for f in nova_tests/concurrency/plan40_channel_hardening.nv \
                 nova_tests/concurrency/plan40_perf_bench.nv \
                 nova_tests/concurrency/select_many_arms.nv \
                 nova_tests/concurrency/select_timer_cleanup.nv \
                 nova_tests/concurrency/select_max_arms_boundary.nv \
                 nova_tests/expected_runtime/channel_zero_capacity_panic.nv; do
            echo ""
            echo "→ $f"
            $NOVA_BIN test "$f"
        done

        # Plan 40 Этап 6: pthread stress tests (если есть).
        if [ -d nova_tests/plan40_sanitizers ]; then
            echo ""
            echo "--- pthread stress tests ---"
            bash nova_tests/plan40_sanitizers/run_all.sh
        fi
        ;;
    *)
        echo "ERROR: unknown SANITIZER=$SANITIZER (expected: none|tsan|asan|ubsan)"
        exit 1
        ;;
esac

echo ""
echo "=== Plan 40 Linux tests OK ==="
