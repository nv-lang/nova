#!/usr/bin/env bash
# Plan 40 Ф.1 Этап 6: build + run pthread stress tests with sanitizers.
#
# Designed для запуска внутри docker контейнера (см. docker/Dockerfile).
# $NOVA_SANITIZER picked up from environment (set by Dockerfile build-arg).

set -euo pipefail

cd /nova
SANITIZER="${NOVA_SANITIZER:-none}"

if [ "$SANITIZER" = "none" ]; then
    echo "Skipping pthread stress tests (SANITIZER=none — these are for sanitizer validation)"
    exit 0
fi

echo "=== Plan 40 pthread stress tests (SANITIZER=$SANITIZER) ==="

SANITIZER_FLAG=""
case "$SANITIZER" in
    tsan)  SANITIZER_FLAG="-fsanitize=thread" ;;
    asan)  SANITIZER_FLAG="-fsanitize=address -fno-omit-frame-pointer" ;;
    ubsan) SANITIZER_FLAG="-fsanitize=undefined -fno-sanitize=signed-integer-overflow" ;;
esac

CFLAGS="-O1 -g -pthread $SANITIZER_FLAG"
INCLUDE="-I/nova/compiler-codegen/nova_rt"
LIBS="-lgc -lpthread"

TESTS=(
    b1_mutex_stress
    b2_selectdone_cas
    t2_waiter_churn
)

cd nova_tests/plan40_sanitizers

for t in "${TESTS[@]}"; do
    echo ""
    echo "--- build $t ---"
    clang $CFLAGS $INCLUDE "$t.c" $LIBS -o "$t"
    echo "--- run $t ---"
    if ! ./"$t"; then
        echo "FAIL: $t"
        exit 1
    fi
done

echo ""
echo "=== Plan 40 pthread stress tests PASS ($SANITIZER) ==="
