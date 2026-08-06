#!/usr/bin/env bash
# Plan 197 — smoke test for the experimental `--strict-effects` CLI flag.
#
# The D89 EXPECT_* marker system driving `nova test` has no per-file CLI-flag
# support (parse_expect / test_runner.rs), so the pos/neg fixtures under
# spec_tests/strict_effects/ cannot be verified through the normal test
# runner. This script drives `nova check` directly instead, asserting the
# exact pass/fail pattern documented per-file in the fixtures' header
# comments: `pos_*.nv` must pass BOTH with and without the flag; `neg_*.nv`
# must pass WITHOUT the flag (byte-identical-behavior guarantee) and FAIL
# WITH the flag, with stderr containing the fixture's own `[E_...]` code.
#
# Usage: scripts/guards/strict_effects_smoke.sh [path/to/nova(.exe)]
# Default binary: nova-cli/target/debug/nova(.exe) relative to repo root.
#
# Plan 231 (docs/plans/231-bug-cycle-exit.md) treats this as one of the
# machine-enforcement guards for a norm the checker enforces only behind a
# flag (track Д — "machine enforcement of norms"). NOTE (verified
# 2026-07-27): NOT invoked by scripts/gate.sh or any .github/workflows/
# *.yml — run it by hand whenever spec_tests/strict_effects/ fixtures or
# the `--strict-effects` code path change.
set -euo pipefail
# Script lives in scripts/guards/ — repo root is two levels up.
cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.."

NOVA_BIN="${1:-nova-cli/target/debug/nova.exe}"
if [ ! -f "$NOVA_BIN" ]; then
    NOVA_BIN="nova-cli/target/debug/nova"
fi
if [ ! -f "$NOVA_BIN" ]; then
    echo "error: nova binary not found (looked for nova-cli/target/debug/nova[.exe])." >&2
    echo "Build it first: cd nova-cli && cargo build --bin nova" >&2
    exit 2
fi

FIXTURES_DIR="spec_tests/strict_effects"
fail_count=0
pass_count=0

check_pass() {
    local file="$1"; shift
    local label="$1"; shift
    if "$NOVA_BIN" "$@" check "$file" >/tmp/strict_effects_smoke.out 2>&1; then
        echo "PASS  $label"
        pass_count=$((pass_count + 1))
    else
        echo "FAIL  $label (expected PASS, got FAIL)"
        sed 's/^/      /' /tmp/strict_effects_smoke.out
        fail_count=$((fail_count + 1))
    fi
}

check_fail_with_code() {
    local file="$1"; local code="$2"
    if "$NOVA_BIN" --strict-effects check "$file" >/tmp/strict_effects_smoke.out 2>&1; then
        echo "FAIL  $file --strict-effects (expected FAIL with $code, got PASS)"
        fail_count=$((fail_count + 1))
    elif grep -q "\[$code\]" /tmp/strict_effects_smoke.out; then
        echo "PASS  $file --strict-effects (failed with $code, as expected)"
        pass_count=$((pass_count + 1))
    else
        echo "FAIL  $file --strict-effects (failed, but not with $code)"
        sed 's/^/      /' /tmp/strict_effects_smoke.out
        fail_count=$((fail_count + 1))
    fi
}

for f in "$FIXTURES_DIR"/pos_*.nv; do
    check_pass "$f" "$f (no flag)"
    check_pass "$f" "$f (--strict-effects)" --strict-effects
done

check_pass "$FIXTURES_DIR/neg_transitive_undeclared.nv" "neg_transitive_undeclared.nv (no flag)"
check_fail_with_code "$FIXTURES_DIR/neg_transitive_undeclared.nv" "E_UNDECLARED_TRANSITIVE_EFFECT"

check_pass "$FIXTURES_DIR/neg_erasure_fn_type.nv" "neg_erasure_fn_type.nv (no flag)"
check_fail_with_code "$FIXTURES_DIR/neg_erasure_fn_type.nv" "E_EFFECT_ERASED_IN_FN_TYPE"

echo
echo "===== strict_effects_smoke: $pass_count passed, $fail_count failed ====="
rm -f /tmp/strict_effects_smoke.out
[ "$fail_count" -eq 0 ]
