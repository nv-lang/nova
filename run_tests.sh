#!/usr/bin/env bash
# Plan 24 + Plan 26: cross-platform thin wrapper над `nova-codegen test-all`.
#
# Логика runner'а в compiler-codegen/src/test_runner.rs. Этот скрипт
# только устанавливает пути и прокидывает аргументы в `nova-codegen test-all`.
#
# Usage:
#   ./run_tests.sh                                  # auto-detect Clang/GCC, dev mode
#   ./run_tests.sh --filter basics --mode release   # subset тестов, release
#   ./run_tests.sh --include-stdlib --toolchain gcc # std/ + GCC
#   ./run_tests.sh --jobs 8 --timeout 30            # parallel + timeout
#   ./run_tests.sh --format json                    # CI-grade JSON output
#   ./run_tests.sh --rerun-failed                   # только бывшие fail'ы
#
# Environment:
#   NOVA_CODEGEN          — путь к nova-codegen binary (default target/debug)
#   NOVA_CLANG            — путь к clang
#   NOVA_GCC              — путь к gcc
#   NOVA_MARCH_NATIVE=1   — release с -march=native (вместо x86-64-v3)

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
CODEGEN="${NOVA_CODEGEN:-$HERE/compiler-codegen/target/debug/nova-codegen}"

if [ ! -x "$CODEGEN" ]; then
    echo "ERROR: nova-codegen not found at $CODEGEN" >&2
    echo "Run: cd compiler-codegen && cargo build" >&2
    exit 1
fi

# Передаём фиксированные --tests-dir / --stdlib-dir / --cg-include / --rt-dir;
# все остальные аргументы прокидываются как-есть через "$@".
# Принимаемые flags Plan 26: --timeout, --jobs, --format, --verbose, --quiet,
# --results-file, --rerun-failed, --filter, --mode, --toolchain, --include-stdlib,
# --keep-artifacts.
exec "$CODEGEN" test-all \
    --tests-dir "$HERE/nova_tests" \
    --stdlib-dir "$HERE/std" \
    --cg-include "$HERE/compiler-codegen" \
    --rt-dir "$HERE/compiler-codegen/nova_rt" \
    "$@"
