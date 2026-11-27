#!/usr/bin/env bash
# WHY (docs/plans/231-bug-cycle-exit.md §4в): ordinary Cargo builds can
# succeed while unused Rust code accumulates across the compiler and drivers.
#
# CHECK: build the four Rust crates in release mode with rustc warnings
# denied and fail on Cargo warnings as well. Diagnostics retain source
# paths. The isolated target directory lets Cargo reuse dependencies
# without replacing binaries used by other gate steps.
#
# RUN: bash scripts/guards/check-cargo-no-warnings.sh <repository root>
# With no Cargo on PATH, print an explicit no-verdict result.
# Self-test: scripts/guards/selftest/test-check-cargo-no-warnings.sh.
set -eu
export LC_ALL=C

NAME=check-cargo-no-warnings
ROOT=$(cd "${1:-$(dirname "$0")/../..}" && pwd)
if ! command -v cargo >/dev/null 2>&1; then
    echo "$NAME ok: cargo unavailable — no verdict"
    exit 0
fi

# Cargo gives the encoded form precedence over RUSTFLAGS.
unset CARGO_ENCODED_RUSTFLAGS
export RUSTFLAGS="-D warnings"
export CARGO_TARGET_DIR="$ROOT/nova-cli/target/cargo-no-warnings"

for crate in nova-cli compiler-codegen nova-driver-support nova-lsp; do
    echo "$NAME: checking $crate"
    if ! output=$(cd "$ROOT/$crate" && cargo build --release --color never 2>&1); then
        printf '%s\n' "$output" >&2
        echo "$NAME: FAIL — build failed in $crate" >&2
        exit 1
    fi
    # Build scripts can emit Cargo warnings outside rustc's lint machinery.
    if [[ "$output" == warning:* || "$output" == *$'\n'warning:* ]]; then
        printf '%s\n' "$output" >&2
        echo "$NAME: FAIL — Cargo emitted a warning in $crate" >&2
        exit 1
    fi
done
echo "$NAME ok: four Rust crates built without warnings"
