#!/usr/bin/env bash
# Compile a real warning fixture, then clear the warning and retry.
set -eu
GUARD="$(cd "$(dirname "$0")/.." && pwd)/check-cargo-no-warnings.sh"
if ! command -v cargo >/dev/null 2>&1; then
    echo "selftest check-cargo-no-warnings: cargo unavailable — no verdict"
    exit 0
fi

FIXTURE=$(mktemp -d)
trap 'rm -rf -- "$FIXTURE"' EXIT
for crate in nova-cli compiler-codegen nova-driver-support nova-lsp; do
    mkdir -p "$FIXTURE/$crate/src"
    printf '[package]\nname = "%s"\nversion = "0.0.0"\nedition = "2021"\n' "$crate" \
        > "$FIXTURE/$crate/Cargo.toml"
    printf 'fn main() {}\n' > "$FIXTURE/$crate/src/main.rs"
done

SOURCE="$FIXTURE/nova-lsp/src/main.rs"
printf 'fn main() { let unused = 1; }\n' > "$SOURCE"
if output=$(bash "$GUARD" "$FIXTURE" 2>&1); then
    printf 'FAIL: accepted an unused variable\n%s\n' "$output" >&2
    exit 1
fi
if [[ "$output" != *'unused variable'* || "$output" != *'src/main.rs:1:'* || "$output" != *'FAIL — build failed in nova-lsp'* ]]; then
    printf 'FAIL: expected a diagnostic with a source file\n%s\n' "$output" >&2
    exit 1
fi
echo "RED: unused variable rejected with source location"
printf 'fn main() {}\n' > "$SOURCE"
output=$(bash "$GUARD" "$FIXTURE")
[[ "$output" == *'ok: four Rust crates built without warnings'* ]]
echo "GREEN: all four clean fixtures accepted"
