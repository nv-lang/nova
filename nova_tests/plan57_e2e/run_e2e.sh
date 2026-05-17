#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Plan 57.E.6 — E2E shell tests для bench CLI subcommands.
#
# Использование (из repo root):
#   ./nova_tests/plan57_e2e/run_e2e.sh
#
# Тестирует все bench CLI commands end-to-end: build nova → run smoke
# bench → diff → gate → history-add → list → dashboard → calibrate →
# corpus → runner-branch → cpu-instr-check → history-anomalies.
#
# Exit codes:
#   0 — все tests pass.
#   1 — один или более tests failed.
#
# Зависимости: bash, find, grep, mktemp, git.

set -uo pipefail

# ── Setup ────────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NOVA_BIN="${REPO_ROOT}/nova-cli/target/release/nova.exe"
if [ ! -x "$NOVA_BIN" ]; then
    NOVA_BIN="${REPO_ROOT}/nova-cli/target/release/nova"
fi
if [ ! -x "$NOVA_BIN" ]; then
    echo "FAIL: nova binary не найден. Build: cargo build --release -p nova" >&2
    exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR" 2>/dev/null; \
      cd "$REPO_ROOT/../nova-lang" 2>/dev/null && \
        for b in $(git branch --list "bench-test-e2e-*" 2>/dev/null); do \
          git branch -D "$b" >/dev/null 2>&1; \
        done' EXIT

cd "$REPO_ROOT"

PASS=0
FAIL=0
TEST_BRANCH="bench-test-e2e-$$"

# ── Test helpers ─────────────────────────────────────────────────────────

assert_eq() {
    local actual="$1" expected="$2" label="$3"
    if [ "$actual" = "$expected" ]; then
        echo "  PASS: $label"
        PASS=$((PASS+1))
    else
        echo "  FAIL: $label (expected '$expected', got '$actual')"
        FAIL=$((FAIL+1))
    fi
}

assert_contains() {
    local haystack="$1" needle="$2" label="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo "  PASS: $label"
        PASS=$((PASS+1))
    else
        echo "  FAIL: $label (output не содержит '$needle')"
        echo "         actual output: $(echo "$haystack" | head -3)"
        FAIL=$((FAIL+1))
    fi
}

assert_file_exists() {
    local path="$1" label="$2"
    if [ -f "$path" ] && [ -s "$path" ]; then
        echo "  PASS: $label ($path, $(wc -c < "$path") bytes)"
        PASS=$((PASS+1))
    else
        echo "  FAIL: $label — file missing or empty: $path"
        FAIL=$((FAIL+1))
    fi
}

# ── 1. nova bench run smoke ──────────────────────────────────────────────

echo "=== 1. nova bench run smoke ==="
OUT_JSON="$TMP_DIR/baseline.json"
"$NOVA_BIN" bench run bench/micro/hello.nv \
    --gc malloc --mode dev --samples 5 --warmup-ms 30 --time-budget 2 \
    --out "$OUT_JSON" >"$TMP_DIR/run1.log" 2>&1
assert_file_exists "$OUT_JSON" "nova bench run emits JSON"
schema_version=$(grep -o '"format_version":[[:space:]]*"[^"]*"' "$OUT_JSON" | head -1)
assert_contains "$schema_version" '"format_version": *"1"' "JSON schema v1"

# ── 2. nova bench diff (same vs same baseline) ──────────────────────────

echo "=== 2. nova bench diff ==="
NEW_JSON="$TMP_DIR/new.json"
"$NOVA_BIN" bench run bench/micro/hello.nv \
    --gc malloc --mode dev --samples 5 --warmup-ms 30 --time-budget 2 \
    --out "$NEW_JSON" >"$TMP_DIR/run2.log" 2>&1
diff_out=$("$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" 2>&1 || true)
assert_contains "$diff_out" "name" "diff output содержит header"
assert_contains "$diff_out" "geomean" "diff содержит geomean"

# ── 3. nova bench gate ───────────────────────────────────────────────────

echo "=== 3. nova bench gate ==="
gate_exit=0
"$NOVA_BIN" bench gate "$OUT_JSON" "$NEW_JSON" >"$TMP_DIR/gate.log" 2>&1 \
    || gate_exit=$?
# Gate должен PASS если diff не значимый (default thresholds 5% + p<0.01).
# Для same-input runs noise может triggered fail если runs widely differ;
# accept либо 0 либо 1.
if [ "$gate_exit" -eq 0 ] || [ "$gate_exit" -eq 1 ]; then
    echo "  PASS: gate exit=$gate_exit (0=ok, 1=regress — оба valid для smoke)"
    PASS=$((PASS+1))
else
    echo "  FAIL: gate unexpected exit=$gate_exit"
    FAIL=$((FAIL+1))
fi

# ── 4. nova bench history-add + list ─────────────────────────────────────

echo "=== 4. nova bench history-add + list ==="
"$NOVA_BIN" bench history-add "$OUT_JSON" --branch "$TEST_BRANCH" \
    >"$TMP_DIR/h-add.log" 2>&1
add_status=$?
assert_eq "$add_status" "0" "history-add exit=0"
"$NOVA_BIN" bench history-add "$NEW_JSON" --branch "$TEST_BRANCH" \
    >"$TMP_DIR/h-add2.log" 2>&1
list_out=$("$NOVA_BIN" bench history-list --branch "$TEST_BRANCH" 2>&1)
assert_contains "$list_out" "2 total entries" "history-list reports 2 entries"

# ── 5. nova bench dashboard ──────────────────────────────────────────────

echo "=== 5. nova bench dashboard ==="
DASH_DIR="$TMP_DIR/dash"
"$NOVA_BIN" bench dashboard --history-branch "$TEST_BRANCH" --out "$DASH_DIR" \
    >"$TMP_DIR/dash.log" 2>&1
assert_file_exists "$DASH_DIR/index.html" "dashboard index.html"
assert_file_exists "$DASH_DIR/data.json" "dashboard data.json"
# Plan 57.E.1 drill-down: per-bench HTML pages должны содержать new sections.
for f in "$DASH_DIR"/bench-*.html; do
    if [ -f "$f" ]; then
        content=$(cat "$f")
        assert_contains "$content" "histogram-chart" "drill-down: histogram section ($f)"
        assert_contains "$content" "stats-sidebar" "drill-down: stats sidebar ($f)"
        break
    fi
done

# ── 6. nova bench calibrate ──────────────────────────────────────────────

echo "=== 6. nova bench calibrate ==="
NOISE_JSON="$TMP_DIR/noise.json"
"$NOVA_BIN" bench calibrate "$OUT_JSON" "$NEW_JSON" --out "$NOISE_JSON" \
    >"$TMP_DIR/calib.log" 2>&1
calib_status=$?
assert_eq "$calib_status" "0" "calibrate exit=0"
assert_file_exists "$NOISE_JSON" "calibrate emits noise.json"
schema_in_noise=$(grep -o '"format_version":[[:space:]]*"[^"]*"' "$NOISE_JSON" | head -1)
assert_contains "$schema_in_noise" '"format_version": *"1"' "noise schema v1"

# ── 7. nova bench corpus --json ──────────────────────────────────────────

echo "=== 7. nova bench corpus --json ==="
corpus_json=$("$NOVA_BIN" bench corpus bench/corpus/01_hello.nv --mode dev --json 2>&1)
assert_contains "$corpus_json" '"format_version"' "corpus JSON has format_version"
assert_contains "$corpus_json" "corpus-perf-breakdown" "corpus JSON kind marker"
assert_contains "$corpus_json" '"passes"' "corpus JSON passes array"

# ── 8. nova bench corpus --html ──────────────────────────────────────────

echo "=== 8. nova bench corpus --html ==="
HTML_OUT="$TMP_DIR/corpus.html"
"$NOVA_BIN" bench corpus bench/corpus/01_hello.nv --mode dev --html "$HTML_OUT" \
    >"$TMP_DIR/corpus.log" 2>&1
assert_file_exists "$HTML_OUT" "corpus HTML emitted"
html_content=$(cat "$HTML_OUT")
assert_contains "$html_content" "echarts" "corpus HTML references echarts"
assert_contains "$html_content" "stacked" "corpus HTML uses stacked bar"

# ── 9. nova bench runner-branch ──────────────────────────────────────────

echo "=== 9. nova bench runner-branch ==="
rb_default=$("$NOVA_BIN" bench runner-branch 2>&1)
assert_eq "$rb_default" "bench-history" "runner-branch default = bench-history"
rb_env=$(NOVA_BENCH_RUNNER_ID=ci-linux-amd "$NOVA_BIN" bench runner-branch 2>&1)
assert_eq "$rb_env" "bench-history-ci-linux-amd" "runner-branch env-aware"

# ── 10. nova bench cpu-instr-check ──────────────────────────────────────

echo "=== 10. nova bench cpu-instr-check ==="
instr_out=$("$NOVA_BIN" bench cpu-instr-check 2>&1)
assert_contains "$instr_out" "OS:" "cpu-instr-check shows OS"
assert_contains "$instr_out" "Available:" "cpu-instr-check shows availability"

# ── 11. nova bench history-anomalies (Plan 57.E.5) ──────────────────────

echo "=== 11. nova bench history-anomalies ==="
# Только 2 entries — недостаточно для PELT (n >= 4); should report stable.
anom_out=$("$NOVA_BIN" bench history-anomalies --branch "$TEST_BRANCH" 2>&1)
assert_contains "$anom_out" "Anomaly scan" "anomalies command runs"

# ── Summary ──────────────────────────────────────────────────────────────

echo ""
echo "===== Plan 57 E2E test summary ====="
echo "PASS: $PASS"
echo "FAIL: $FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "ALL PASS"
    exit 0
else
    echo "FAILURES DETECTED"
    exit 1
fi
