#!/usr/bin/env bash
# scripts/gate-carina.sh — ORACLE-FREE textual tier of the Carina CI contour
# (task 140). Runs on the windows-latest job of .github/workflows/carina-gate.yml
# after the seed bootstrap; NOT wired into the required push-gate list (that is
# a separate integrator decision, per the task bounds).
#
# WHY A SEPARATE SCRIPT instead of a NOVAC_TIER of gate-novac.sh: the split is
# not by cost (loop/push/full) but by PROVENANCE — every guard below judges
# text or the tree and needs neither the Rust Oracle nor a Carina binary built
# by it. A tier of gate-novac.sh would still be the same file carrying the
# oracle-dependent steps; a separate script lets check-carina-gate-ci.py prove
# by construction that the contour cannot call the Oracle at all.
#
# WHAT IS DELIBERATELY NOT HERE (anti-drift: check-carina-gate-ci.py requires
# every guard invoked in gate-novac.sh to be named in one of these lists or in
# the run lists below, so a new guard in gate-novac.sh reddens CI until it is
# consciously classified). The lists are DATA, not prose: the checker reads
# them.
#
# EXCLUDED_ORACLE — needs the Oracle binary; lives in
# .github/workflows/carina-oracle-compat.yml (advisory contour):
EXCLUDED_ORACLE="
check-novac-oracle-fresh.sh
check-novac-differential.sh
check-novac-module-tests.sh
check-novac-lint.sh
check-novac-build-clean.sh
check-novac-cli-surface.sh
check-novac-shell-freshness.sh
"
# EXCLUDED_CORPUS — corpus guards that CANNOT run in this contour even in the
# pinned world. check-novac-fuzz-zero-panic.sh needs the built fuzz harness
# (novac/target/novac-fuzz), which the oracle-free contour does not build;
# it stays with gate-novac.sh (measured 2026-11-04: red in the pinned world
# on the missing harness, not on a crash). The other nine corpus guards moved
# INTO this contour as the CORPUS tier below (owner decision 2026-11-04,
# owner-decisions.md section 1: the module level of Carina is judged without
# the Oracle — self-accepted plus the corpus guards).
EXCLUDED_CORPUS="
check-novac-fuzz-zero-panic.sh
"
# EXCLUDED_FULL — full-tier of gate-novac.sh (nightly measurement/mutation,
# not correctness checks of the tree):
EXCLUDED_FULL="
check-novac-iteration-cost.sh
check-novac-selftest-proves-red.sh
"
#
# CONTRACT per guard (registry #645): exit 0 without a line containing `ok:`
# is "did not fall", not "checked" — counted as a failure.
#
# $1 — repository root (default: pwd).
set -u
ROOT="${1:-$(pwd)}"
GUARDS="$ROOT/scripts/guards"
GATE_T0=$(date +%s)
FAILS=""
FAIL_N=0

note_fail() {
    echo "CARINA-GATE FAIL: $1" >&2
    FAILS="$FAILS
  * $1"
    FAIL_N=$((FAIL_N + 1))
}

judge() {
    # $1 = display name, $2 = rc file, $3 = out file
    local name="$1" rc="" out=""
    [ -f "$2" ] && read -r rc < "$2"
    [ -f "$3" ] && out=$(cat "$3")
    [ -n "$out" ] && printf '%s\n' "$out"
    if [ -z "$rc" ]; then
        note_fail "$name: no exit code recorded — the guard was not judged"
        return 0
    fi
    if [ "$rc" -ne 0 ]; then
        note_fail "$name"
        return 0
    fi
    case "$out" in
        *ok:*) : ;;
        *)
            echo "STEP PROVED NOTHING: $name exited 0 without an ok: line (registry #645)" >&2
            note_fail "$name: no ok: line"
            ;;
    esac
}

# ── Textual guards invoked in gate-novac.sh, minus the excluded lists above.
# Python guards go through run-guards.py in ONE process (convention G3:
# interpreter start ~73ms vs 40-60ms of work per guard).
# check-carina-gate-ci.py is this contour's own anti-drift guard (task 140):
# it is not invoked by gate-novac.sh, it is listed here on top of the
# mirrored set. check-carina-seed-drift.py (owner decision 2026-11-04,
# mechanism 2) publishes and ratchets the HEAD-vs-seed drift.
PY_GUARDS="
check-carina-gate-ci.py
check-carina-seed-drift.py
check-novac-arch-class-proofs.py
check-novac-arch-invariants.py
check-novac-keyword-parity.py
check-novac-no-naked-panic.py
check-novac-no-crutch.py
check-novac-no-unwrap-compare.py
check-novac-cursor-is-range.py
check-novac-wrapper-is-stored.py
check-novac-import-exists.py
check-novac-export-has-caller.py
check-novac-acceptance-donor.py
check-novac-selftest-interpreter.py
check-novac-legacy-workarounds.py
check-novac-oracle-tax-link.py
check-guard-honesty.py
check-novac-bin-door.py
check-diag-paths.py
check-lint-rule-covered.py
check-novac-plan-liveline.py
check-novac-time-ledger.py
check-novac-deps.py
check-novac-file-size.py
check-novac-atomics-door.py
check-novac-no-string-keys.py
check-novac-no-global-state.py
check-novac-frontend-shape.py
check-novac-type-field-docs.py
check-novac-doc-language.py
check-novac-grammar-kinds.py
check-novac-no-name-hardcode.py
check-novac-no-prelude-shadow.py
check-novac-ctx-tables.py
check-novac-row-fields.py
check-novac-ref-field-names.py
check-novac-no-alloc-in-lookup.py
check-novac-ice-messages.py
check-novac-no-default-branch.py
check-novac-mangling-one-way.py
check-novac-second-door.py
check-novac-lowering-one-door.py
check-novac-seams-listed.py
check-novac-empty-str-door.py
check-novac-one-door-export.py
check-novac-edge-payload.py
check-novac-surface.py
check-novac-temp-edges.py
check-novac-module-donor.py
check-novac-plan-donor.py
check-novac-resolve-discipline.py
check-novac-channel-one-writer.py
check-novac-invented-name-not-named.py
check-novac-match-exhaustive.py
check-novac-tuple-no-second-door.py
check-novac-no-silent-skip.py
check-novac-pch.py
check-novac-line-length.py
check-novac-precondition.py
check-novac-emitted-names.py
check-novac-table-is-match.py
check-novac-no-grammar-excuse.py
check-novac-no-undeclared-simplification.py
check-novac-no-copy-loop.py
check-novac-string-build-door.py
check-novac-required-token-door.py
check-novac-recovery-closer.py
check-novac-prim-id-compare.py
check-novac-one-branch-head.py
check-novac-table-one-filler.py
check-novac-deferral-address.py
check-novac-module-decl-uniform.py
check-novac-subset-debt-dated.py
check-novac-tyid-door.py
check-novac-file-decls-door.py
check-novac-branch-complete.py
check-novac-conventions-coverage.py
check-gate-budget.py
check-guard-external-caller.py
check-novac-guard-registry.py
"

SH_GUARDS="
check-ratchet-moves.sh
check-novac-smoke-wrapper.sh
check-novac-local-only-work.sh
check-novac-effects-at-door.sh
check-tbd-branch-freshness.sh
check-novac-batch.sh
check-novac-registry-counts.sh
"

# Guards gate-novac.sh calls with a fixed /dev/null first argument (they judge
# the form of commit input, not the tree); they cannot ride run-guards.py,
# which passes exactly one argument (the root).
DEVNULL_GUARDS="
check-novac-commit-donor.sh
check-novac-commit-forms.sh
check-novac-commit-no-simplification.py
"

echo "carina-gate :: tree $ROOT — oracle-free textual tier (task 140)"

PAR_DIR=$(mktemp -d "${TMPDIR:-/tmp}/carina-gate.XXXXXX")
trap 'rm -rf "$PAR_DIR"' EXIT

# ── Python guards: one process. ──
i=0
IDX=""
for g in $PY_GUARDS; do
    i=$((i + 1))
    printf '%s\n' "$GUARDS/$g" > "$PAR_DIR/$i.cmd"
    IDX="$IDX $i"
done
echo "carina-gate :: $i python guards, one process (run-guards.py)"
python "$GUARDS/run-guards.py" "$ROOT" "$PAR_DIR" $IDX
PY_N=$i
j=0
for g in $PY_GUARDS; do
    j=$((j + 1))
    judge "$g" "$PAR_DIR/$j.rc" "$PAR_DIR/$j.out"
done

# ── Shell guards: sequential, each under a hang deadline.
for g in $SH_GUARDS; do
    echo "── $g"
    bash "$ROOT/scripts/tools/with-deadline.sh" 120 bash "$GUARDS/$g" "$ROOT" \
        > "$PAR_DIR/sh.out" 2>&1
    echo "$?" > "$PAR_DIR/sh.rc"
    judge "$g" "$PAR_DIR/sh.rc" "$PAR_DIR/sh.out"
done

# ── /dev/null-form guards (extra fixed argument).
for g in $DEVNULL_GUARDS; do
    echo "── $g"
    runner=bash
    case "$g" in *.py) runner=python ;; esac
    bash "$ROOT/scripts/tools/with-deadline.sh" 120 "$runner" "$GUARDS/$g" /dev/null "$ROOT" \
        > "$PAR_DIR/dn.out" 2>&1
    echo "$?" > "$PAR_DIR/dn.rc"
    judge "$g" "$PAR_DIR/dn.rc" "$PAR_DIR/dn.out"
done

# ── Corpus tier (owner decision 2026-11-04, owner-decisions.md section 1):
# the module-level judgement of Carina WITHOUT the Oracle. The contour's own
# b2 ($NOVAC) judges the PINNED tree ($CARINA_CORPUS_ROOT) it was built from —
# binary, corpus and guard scripts of ONE generation, so a red here is the
# compiler's own regression, never "the seed rejects post-seed drift".
# Measured 2026-11-04 in the pinned world (b1 of 3729abcc): all nine green
# (self-accepted 214/0; no-panic 569 fixtures; diag-schema/no-cascade 269;
# fixture-expect 232 pinned; emission-size; emitted-unique 269 units;
# mangle-fixed-point; grammar-coverage 20 forms). Needs the bootstrap
# artifacts; absent locally the tier is skipped with a notice, absent in CI
# it is a failure (fail-closed).
CORPUS_GUARDS="
check-novac-self-accepted.py
check-novac-no-panic.sh
check-novac-diag-schema.sh
check-novac-no-cascade.sh
check-novac-fixture-expect.sh
check-novac-emission-size.sh
check-novac-emitted-unique.py
check-novac-mangle-fixed-point.sh
check-novac-grammar-fixture-coverage.sh
"
CORPUS_N=0
if [ -n "${CARINA_CORPUS_ROOT:-}" ] && [ -n "${NOVAC:-}" ]; then
    CG="$CARINA_CORPUS_ROOT/scripts/guards"
    echo "carina-gate :: corpus tier — pinned world $CARINA_CORPUS_ROOT, binary $NOVAC"
    for g in $CORPUS_GUARDS; do
        CORPUS_N=$((CORPUS_N + 1))
        echo "── [corpus] $g"
        runner=bash
        case "$g" in *.py) runner=python ;; esac
        # 1200s: no-panic's deadline in gate-novac.sh (its 569 fixtures took
        # 147s on 16 threads; CI runners are slower).
        bash "$ROOT/scripts/tools/with-deadline.sh" 1200 "$runner" "$CG/$g" "$CARINA_CORPUS_ROOT" \
            > "$PAR_DIR/cg.out" 2>&1
        echo "$?" > "$PAR_DIR/cg.rc"
        judge "[corpus] $g" "$PAR_DIR/cg.rc" "$PAR_DIR/cg.out"
    done
elif [ "${CI:-}" = "true" ]; then
    note_fail "corpus tier unconfigured in CI: CARINA_CORPUS_ROOT/NOVAC unset — the module level is not judged"
else
    echo "carina-gate :: corpus tier SKIPPED (no CARINA_CORPUS_ROOT/NOVAC — set them to the bootstrap artifacts)"
fi

ELAPSED=$(( $(date +%s) - GATE_T0 ))
if [ "$FAIL_N" -gt 0 ]; then
    echo "" >&2
    echo "CARINA-GATE: $FAIL_N failure(s):$FAILS" >&2
    exit 1
fi
echo "CARINA-GATE OK (final): $PY_N python + $(echo $SH_GUARDS | wc -w) shell + $(echo $DEVNULL_GUARDS | wc -w) devnull + $CORPUS_N corpus guards, ${ELAPSED}s, no Oracle invoked"
exit 0
