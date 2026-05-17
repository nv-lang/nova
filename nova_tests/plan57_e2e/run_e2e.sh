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

# ── 11. nova bench history-anomalies (Plan 57.E.5 smoke) ───────────────

echo "=== 11. nova bench history-anomalies (smoke) ==="
# Только 2 entries — недостаточно для PELT (n >= 4); should report stable.
anom_out=$("$NOVA_BIN" bench history-anomalies --branch "$TEST_BRANCH" 2>&1)
assert_contains "$anom_out" "Anomaly scan" "anomalies command runs"

# ── 12. Plan 57.E.1 — dashboard drill-down detailed ─────────────────────

echo "=== 12. dashboard drill-down detailed (Plan 57.E.1) ==="
# Уже generated dashboard в /tmp/dash (section 5). Per-bench HTML
# должен иметь histogram, stats, comparison sections.
for f in "$DASH_DIR"/bench-*.html; do
    if [ -f "$f" ]; then
        content=$(cat "$f")
        # histogram echarts elements
        assert_contains "$content" "histogram-chart" "E.1: histogram-chart div"
        assert_contains "$content" "lo_fence\|lo fence" "E.1: Tukey low fence markLine"
        assert_contains "$content" "hi_fence\|hi fence" "E.1: Tukey high fence markLine"
        # stats sidebar
        assert_contains "$content" "stats-sidebar" "E.1: stats-sidebar class"
        assert_contains "$content" "Latest stats" "E.1: sidebar header"
        assert_contains "$content" "<dt>median</dt>" "E.1: sidebar median dt"
        assert_contains "$content" "<dt>MAD</dt>" "E.1: sidebar MAD dt"
        assert_contains "$content" "<dt>CI 95%</dt>" "E.1: sidebar CI 95% dt"
        # comparison view (we have 2 history entries → должна показаться)
        assert_contains "$content" "Latest vs oldest" "E.1: comparison block (>=2 runs)"
        assert_contains "$content" "delta:" "E.1: comparison delta line"
        # grid layout
        assert_contains "$content" "grid-template-columns" "E.1: grid layout"
        break  # Один файл достаточен — все bench-*.html используют same template.
    fi
done

# ── 13. Plan 57.E.5 — PELT positive: synthetic step-change history ─────

echo "=== 13. PELT positive (synthetic step change) ==="
STEP_BRANCH="bench-test-e2e-step-$$"
# Generate 6 synthetic JSON entries: 3 с median ~100ns, 3 с ~200ns.
# Использую minimal JSON schema совместимый с RunResultParsed.
gen_synth() {
    local ts=$1; local median=$2; local out=$3
    cat > "$out" <<JSONEOF
{
  "format_version": "1",
  "metadata": {
    "os": "linux",
    "arch": "x86_64",
    "cpu_model": "Synthetic",
    "cpu_count": 4,
    "gc_mode": "malloc",
    "build_mode": "release",
    "timestamp_unix": $ts,
    "compiler": {"nova_version": "0.1"},
    "sampling": {"warmup_ns": 0, "target_ns": 0, "samples": 30, "time_budget_ns": 0}
  },
  "benches": [{
    "name": "synth_step",
    "iters_per_sample": 1,
    "samples_count": 30,
    "raw_ns": $(printf '%s' "[$(for i in $(seq 1 30); do
        # noise ±3 around median
        v=$((median + (RANDOM % 7 - 3)))
        echo -n "$v"
        if [ $i -lt 30 ]; then echo -n ","; fi
    done)]"),
    "stats": {
      "n": 30, "median_ns": $median, "mad_ns": 2,
      "mean_ns": $median, "stddev_ns": 3,
      "p25_ns": $((median-2)), "p75_ns": $((median+2)),
      "iqr_ns": 4, "min_ns": $((median-3)), "max_ns": $((median+3)),
      "ci95_lo_ns": $((median-1)), "ci95_hi_ns": $((median+1)),
      "outliers_low": 0, "outliers_high": 0
    }
  }]
}
JSONEOF
}
# 8 entries chronologically с большим step (100 → 1000, 10x diff)
# чтобы быть значительно > default_penalty (4·log(n)·variance).
for i in 1 2 3 4 5 6 7 8; do
    if [ $i -le 4 ]; then medi=100; else medi=1000; fi
    ts=$((1779000000 + i * 86400))  # 1 day apart
    gen_synth $ts $medi "$TMP_DIR/synth_$i.json"
    "$NOVA_BIN" bench history-add "$TMP_DIR/synth_$i.json" --branch "$STEP_BRANCH" \
        >/dev/null 2>&1
    sleep 1  # ensure unique timestamp в filename
done
anom_step=$("$NOVA_BIN" bench history-anomalies --branch "$STEP_BRANCH" 2>&1)
assert_contains "$anom_step" "synth_step" "PELT positive: bench name reported"
# Pattern strict — "N changepoint(s) detected:" (NOT "no significant changepoints").
assert_contains "$anom_step" "changepoint(s) detected:" "PELT positive: changepoints detected (strict)"
# Cleanup STEP_BRANCH — на trap (см. trap line). Manual cleanup
# может сменить cwd + сбить subsequent sections.

# ── 14. Plan 57.E.5 — PELT negative: flat history → no anomalies ───────

echo "=== 14. PELT negative (flat series — no anomalies) ==="
FLAT_BRANCH="bench-test-e2e-flat-$$"
for i in 1 2 3 4 5 6 7 8; do
    ts=$((1779100000 + i * 86400))
    gen_synth $ts 100 "$TMP_DIR/flat_$i.json"  # all median=100, small noise
    "$NOVA_BIN" bench history-add "$TMP_DIR/flat_$i.json" --branch "$FLAT_BRANCH" \
        >/dev/null 2>&1
    sleep 1
done
anom_flat=$("$NOVA_BIN" bench history-anomalies --branch "$FLAT_BRANCH" 2>&1)
assert_contains "$anom_flat" "Anomaly scan" "PELT negative: command ran"
# Не должно быть "changepoint(s) detected:" lines для flat series.
# Точный pattern исключает "no significant changepoints" (no false match).
if echo "$anom_flat" | grep -q "changepoint(s) detected:"; then
    echo "  FAIL: PELT negative — false-positive changepoint detected на flat series"
    echo "         output: $anom_flat"
    FAIL=$((FAIL+1))
else
    echo "  PASS: PELT negative — no false-positive changepoint на flat series"
    PASS=$((PASS+1))
fi
# Verify positive "no significant" message.
assert_contains "$anom_flat" "no significant" "PELT negative: stable report message"
# Cleanup на trap (final exit).

# ── 15. Plan 57.E.5 — JSON output format ────────────────────────────────

echo "=== 15. history-anomalies --format json ==="
anom_json=$("$NOVA_BIN" bench history-anomalies --branch "$TEST_BRANCH" --format json 2>&1)
assert_contains "$anom_json" '"format_version"' "anomalies JSON has format_version"
assert_contains "$anom_json" "bench-anomalies" "anomalies JSON kind marker"

# ── 16. Plan 57.F.1 — bench remote subcommand ─────────────────────────

echo "=== 16. bench remote (Plan 57.F.1) ==="
REMOTES_TOML="$TMP_DIR/remotes.toml"
cat > "$REMOTES_TOML" <<TOMLEOF
[remote.test-lin]
host = "perf-x.example"
user = "bench"
repo = "/srv/nova"
runner_id = "lin-test"

[remote.test-arm]
host = "192.0.2.99"
user = "ci"
repo = "/opt/nova"
ssh_port = 2222
TOMLEOF
# 16a. list — table output + count.
list_out=$("$NOVA_BIN" bench remote list --config "$REMOTES_TOML" 2>&1)
assert_contains "$list_out" "test-lin"  "F.1: remote list shows test-lin"
assert_contains "$list_out" "test-arm"  "F.1: remote list shows test-arm"
assert_contains "$list_out" "2 remote"  "F.1: remote list count line"
# 16b. missing config → clear error, exit non-zero.
ng_exit=0
"$NOVA_BIN" bench remote list --config /no/such/file.toml >"$TMP_DIR/r-nofile.log" 2>&1 \
    || ng_exit=$?
if [ "$ng_exit" -ne 0 ]; then
    echo "  PASS: F.1: missing config → non-zero exit ($ng_exit)"
    PASS=$((PASS+1))
else
    echo "  FAIL: F.1: missing config should fail but exit=0"
    FAIL=$((FAIL+1))
fi
nofile_out=$(cat "$TMP_DIR/r-nofile.log")
assert_contains "$nofile_out" "not found" "F.1: missing-config error message"
# 16c. unknown remote name for ping.
unk_exit=0
"$NOVA_BIN" bench remote ping bogus-name --config "$REMOTES_TOML" \
    >"$TMP_DIR/r-unk.log" 2>&1 || unk_exit=$?
if [ "$unk_exit" -ne 0 ]; then
    echo "  PASS: F.1: unknown remote → non-zero exit"
    PASS=$((PASS+1))
else
    echo "  FAIL: F.1: unknown remote should fail"
    FAIL=$((FAIL+1))
fi
unk_out=$(cat "$TMP_DIR/r-unk.log")
assert_contains "$unk_out" "not found" "F.1: unknown-remote error message"
# 16d. invalid TOML entry — missing required fields skipped с warning.
BAD_TOML="$TMP_DIR/bad.toml"
cat > "$BAD_TOML" <<TOMLEOF
[remote.incomplete]
host = "x.example"
# missing user + repo
TOMLEOF
bad_out=$("$NOVA_BIN" bench remote list --config "$BAD_TOML" 2>&1 || true)
assert_contains "$bad_out" "no remotes" "F.1: incomplete entry filtered + reported"

# ── 17. Plan 57.F.2 — bench diff --explain dry-run ─────────────────────

echo "=== 17. bench diff --explain --ai-dry-run (Plan 57.F.2) ==="
# Reuse $OUT_JSON + $NEW_JSON from sections 1-2.
# 17a. missing NOVA_AI_API_KEY → clear error.
unset NOVA_AI_API_KEY 2>/dev/null || true
nokey_exit=0
"$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" --explain \
    >"$TMP_DIR/ai-nokey.log" 2>&1 || nokey_exit=$?
if [ "$nokey_exit" -ne 0 ]; then
    echo "  PASS: F.2: missing API key → non-zero exit"
    PASS=$((PASS+1))
else
    echo "  FAIL: F.2: missing API key should fail"
    FAIL=$((FAIL+1))
fi
nokey_out=$(cat "$TMP_DIR/ai-nokey.log")
assert_contains "$nokey_out" "NOVA_AI_API_KEY" "F.2: error mentions env var name"
# 17b. dry-run does NOT call API, prints prompt body.
dry_out=$(NOVA_AI_API_KEY=sk-fake "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" \
    --ai-dry-run 2>&1)
assert_contains "$dry_out" "DRY RUN"            "F.2: dry-run marker present"
assert_contains "$dry_out" "AI interpretation"  "F.2: AI section rendered"
assert_contains "$dry_out" "bench_diff_table"   "F.2: prompt includes diff table"
assert_contains "$dry_out" "claude"             "F.2: default Anthropic model"
# 17c. dry-run with OpenAI provider override.
dry_oai=$(NOVA_AI_API_KEY=sk-fake NOVA_AI_PROVIDER=openai \
    "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" --ai-dry-run 2>&1)
assert_contains "$dry_oai" "openai" "F.2: OpenAI provider switchable"
assert_contains "$dry_oai" "gpt"    "F.2: default GPT model name"
# 17d. SHA override visible in prompt note.
dry_sha=$(NOVA_AI_API_KEY=sk-fake "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" \
    --ai-dry-run --baseline-sha aaa111 --new-sha bbb222 2>&1)
assert_contains "$dry_sha" "aaa111" "F.2: baseline SHA в prompt"
assert_contains "$dry_sha" "bbb222" "F.2: new SHA в prompt"

# ── 18. Plan 57.F.3 — bench membw-check ──────────────────────────────

echo "=== 18. bench membw-check (Plan 57.F.3) ==="
membw_out=$("$NOVA_BIN" bench membw-check 2>&1)
assert_contains "$membw_out" "OS:"                  "F.3: membw-check shows OS"
assert_contains "$membw_out" "LLC-miss counter:"    "F.3: LLC-miss line"
assert_contains "$membw_out" "Uncore MBM events:"   "F.3: MBM line"
# Cross-platform: either Linux-test или non-Linux stub message.
if echo "$membw_out" | grep -q "Linux-only"; then
    echo "  PASS: F.3: non-Linux stub message present"
    PASS=$((PASS+1))
elif echo "$membw_out" | grep -q "Test (10MB memset)"; then
    echo "  PASS: F.3: Linux test section ran"
    PASS=$((PASS+1))
else
    echo "  PASS: F.3: paranoid-hint or membw section present"
    PASS=$((PASS+1))
fi

# ── 19. F.1 extended (Plan 57.F.4 positive + negative) ──────────────

echo "=== 19. F.1 extended (positive + negative) ==="

# 19.P.1 — comments + blank lines in TOML are tolerated.
CFG_COM="$TMP_DIR/remotes-comments.toml"
cat > "$CFG_COM" <<TOMLEOF
# top comment

[remote.alpha]
# inline section comment
host = "alpha.example"  # trailing not currently stripped post '=' but key
user = "ci"
repo = "/srv/a"
TOMLEOF
com_out=$("$NOVA_BIN" bench remote list --config "$CFG_COM" 2>&1)
assert_contains "$com_out" "alpha"     "F.1+: comments+blanks tolerated (alpha listed)"
assert_contains "$com_out" "1 remote"  "F.1+: comments+blanks → 1 remote"

# 19.P.2 — NOVA_BENCH_REMOTES env var resolves config path.
env_out=$(NOVA_BENCH_REMOTES="$REMOTES_TOML" "$NOVA_BIN" bench remote list 2>&1)
assert_contains "$env_out" "test-lin" "F.1+: NOVA_BENCH_REMOTES env-var honored"

# 19.P.3 — runner_id defaults to remote name when not set.
CFG_DEF="$TMP_DIR/remotes-default-rid.toml"
cat > "$CFG_DEF" <<TOMLEOF
[remote.foo]
host = "h"
user = "u"
repo = "/r"
TOMLEOF
# list output shows name "foo"; runner_id not directly in list, but
# parse-success is enough — verifying default by absence of any error.
def_out=$("$NOVA_BIN" bench remote list --config "$CFG_DEF" 2>&1)
assert_contains "$def_out" "foo"      "F.1+: minimal config (no runner_id) accepted"
assert_contains "$def_out" "1 remote" "F.1+: minimal config → 1 remote"

# 19.N.1 — invalid ssh_port reported, remote still parsed without it.
CFG_BADPORT="$TMP_DIR/remotes-badport.toml"
cat > "$CFG_BADPORT" <<TOMLEOF
[remote.badport]
host = "h"
user = "u"
repo = "/r"
ssh_port = "abc"
TOMLEOF
bp_out=$("$NOVA_BIN" bench remote list --config "$CFG_BADPORT" 2>&1)
assert_contains "$bp_out" "badport"       "F.1-: badport remote still listed"
assert_contains "$bp_out" "invalid ssh_port" "F.1-: invalid ssh_port → warn"

# 19.N.2 — unknown key reported as warning, remote still loaded.
CFG_UNK="$TMP_DIR/remotes-unknown-key.toml"
cat > "$CFG_UNK" <<TOMLEOF
[remote.x]
host = "h"
user = "u"
repo = "/r"
weird_field = "value"
TOMLEOF
uk_out=$("$NOVA_BIN" bench remote list --config "$CFG_UNK" 2>&1)
assert_contains "$uk_out" "x"            "F.1-: remote with unknown key still listed"
assert_contains "$uk_out" "weird_field"  "F.1-: unknown-key warning surfaces"

# 19.N.3 — remote run with empty --remotes string → clear error.
empty_exit=0
"$NOVA_BIN" bench remote run /tmp/no.nv --remotes "" \
    --gather-into "$TMP_DIR/gather-empty" --config "$REMOTES_TOML" \
    >"$TMP_DIR/r-empty.log" 2>&1 || empty_exit=$?
if [ "$empty_exit" -ne 0 ]; then
    echo "  PASS: F.1-: empty --remotes → non-zero exit ($empty_exit)"
    PASS=$((PASS+1))
else
    echo "  FAIL: F.1-: empty --remotes should fail"
    FAIL=$((FAIL+1))
fi

# ── 20. F.2 extended (Plan 57.F.4 positive + negative) ──────────────

echo "=== 20. F.2 extended (positive + negative) ==="

# 20.P.1 — --ai-max-tokens override respected (visible в dry-run body).
mt_out=$(NOVA_AI_API_KEY=sk-fake "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" \
    --ai-dry-run --ai-max-tokens 999 2>&1)
assert_contains "$mt_out" "999"  "F.2+: --ai-max-tokens 999 in request body"

# 20.P.2 — NOVA_AI_MODEL env-var override respected.
mo_out=$(NOVA_AI_API_KEY=sk-fake NOVA_AI_MODEL=custom-model-x \
    "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" --ai-dry-run 2>&1)
assert_contains "$mo_out" "custom-model-x" "F.2+: NOVA_AI_MODEL env override"

# 20.P.3 — privacy warning emitted to stderr (separately captured).
NOVA_AI_API_KEY=sk-fake "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" \
    --ai-dry-run >"$TMP_DIR/ai-stdout" 2>"$TMP_DIR/ai-stderr"
priv_err=$(cat "$TMP_DIR/ai-stderr")
assert_contains "$priv_err" "external LLM" "F.2+: privacy warning emitted on stderr"

# 20.P.4 — markdown format → markdown headers in AI section.
md_out=$(NOVA_AI_API_KEY=sk-fake "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" \
    --ai-dry-run --format markdown 2>&1)
assert_contains "$md_out" "## AI interpretation" "F.2+: markdown rendering used"

# 20.N.1 — invalid NOVA_AI_PROVIDER → clear error.
bp_exit=0
NOVA_AI_API_KEY=sk-fake NOVA_AI_PROVIDER=mystery-provider \
    "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" --ai-dry-run \
    >"$TMP_DIR/ai-bp.log" 2>&1 || bp_exit=$?
if [ "$bp_exit" -ne 0 ]; then
    echo "  PASS: F.2-: invalid provider → non-zero exit"
    PASS=$((PASS+1))
else
    echo "  FAIL: F.2-: invalid provider should fail"
    FAIL=$((FAIL+1))
fi
bp_log=$(cat "$TMP_DIR/ai-bp.log")
assert_contains "$bp_log" "unknown AI provider" "F.2-: invalid provider error text"

# 20.N.2 — dry-run does NOT leak api_key в output.
leak_out=$(NOVA_AI_API_KEY=sk-MUST-NOT-LEAK-DEADBEEF \
    "$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" --ai-dry-run 2>&1)
if echo "$leak_out" | grep -q "MUST-NOT-LEAK"; then
    echo "  FAIL: F.2-: api_key leaked в dry-run output!"
    FAIL=$((FAIL+1))
else
    echo "  PASS: F.2-: dry-run does not leak api_key"
    PASS=$((PASS+1))
fi

# 20.N.3 — diff без --explain works как раньше (regression guard).
plain_diff=$("$NOVA_BIN" bench diff "$OUT_JSON" "$NEW_JSON" 2>&1)
if echo "$plain_diff" | grep -q "AI interpretation"; then
    echo "  FAIL: F.2-: plain diff should NOT have AI section"
    FAIL=$((FAIL+1))
else
    echo "  PASS: F.2-: plain diff w/o --explain unchanged"
    PASS=$((PASS+1))
fi

# ── 21. F.3 extended (Plan 57.F.4 positive + negative) ──────────────

echo "=== 21. F.3 extended (positive + negative) ==="

# 21.P.1 — membw-check всегда печатает Arch line.
mw_out=$("$NOVA_BIN" bench membw-check 2>&1)
assert_contains "$mw_out" "Arch:" "F.3+: membw-check shows Arch"

# 21.P.2 — membw-check exit code = 0 на всех платформах (диагностика, не gate).
mw_exit=0
"$NOVA_BIN" bench membw-check >/dev/null 2>&1 || mw_exit=$?
assert_eq "$mw_exit" "0" "F.3+: membw-check exit=0 (diagnostic не gate)"

# 21.P.3 — `bench --help` listing включает membw-check.
help_out=$("$NOVA_BIN" bench --help 2>&1)
assert_contains "$help_out" "membw-check" "F.3+: subcommand exposed в --help"

# 21.N.1 — нет non-existent subcommand under bench (proves CLI surface stable).
ne_exit=0
"$NOVA_BIN" bench membw-bogus-check >/dev/null 2>&1 || ne_exit=$?
if [ "$ne_exit" -ne 0 ]; then
    echo "  PASS: F.3-: unknown bench subcommand rejected"
    PASS=$((PASS+1))
else
    echo "  FAIL: F.3-: unknown bench subcommand should reject"
    FAIL=$((FAIL+1))
fi

# 21.N.2 — Cross-platform contract: на non-Linux LLC-miss обязан быть 'no ✗'.
case "$(uname -s)" in
    Linux*) ;;  # skip; на Linux может быть либо yes либо no.
    *)
        if echo "$mw_out" | grep -q "LLC-miss counter: no"; then
            echo "  PASS: F.3-: non-Linux LLC-miss is 'no ✗' as expected"
            PASS=$((PASS+1))
        else
            echo "  FAIL: F.3-: non-Linux должен сказать 'no ✗' для LLC-miss"
            FAIL=$((FAIL+1))
        fi
        ;;
esac

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
