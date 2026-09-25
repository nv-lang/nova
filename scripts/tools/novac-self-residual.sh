#!/bin/sh
# scripts/tools/novac-self-residual.sh -- the TYPING residual of Carina's own
# source: how many diagnostics stand between novac/src and step 0.2, counted
# where typing actually ran (registry 221.1 №1350; plan 274.7 wave U, U.4a).
#
# WHY THIS AND NOT THE BATCH'S FILE COUNT. A unit of the module-mode batch is
# one text, and novac/src/check/run.nv types a text only on a CLEAN walk
# verdict ("A refused file is not typed"). One walk refusal in any file of a
# unit switches typing off for the whole unit, so the batch's per-file count
# ("rejected 29 of 108") says nothing about the files it calls accepted. This
# tool takes each unit, drops the files the walk refused, lets the rest type,
# and counts what typing says.
#
# IT IS A LOWER BOUND, and the reason is part of the number: the dropped files
# are not typed at all, so their own typing diagnostics are not in it. A unit
# with no walk-refused file is typed whole, and its count is exact.
#
# A MEASURE, NOT A RATCHET (the integrator's decision, 2026-09-25): no guard
# holds this number; it is quoted with its date and commit where it is used.
# An ICE aborts a run and hides everything behind it, so the ICE count is
# printed beside the total -- a total with ICE above zero is not a measure.
#
# Usage (from the repo root, novac built): sh scripts/tools/novac-self-residual.sh
#   NOVAC=<path> overrides novac/target/novac.exe.
# Verified: Windows (Git Bash), 2026-09-25 -- 4050 diagnostics, ICE 0, 109 files, on
# p274-novac after registry 1322's handed fields.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT" || exit 1
NOVAC="${NOVAC:-$ROOT/novac/target/novac.exe}"
if [ ! -x "$NOVAC" ]; then
    echo "novac-self-residual: FAIL -- no novac binary at $NOVAC (build it first)" >&2
    exit 1
fi
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT

# one unit check: the file list on argv, the raw diagnostics to $T/out
run_unit() {
    NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src "$NOVAC" check "$@" > "$T/out" 2>&1 </dev/null
}

grand=0
ice=0
: > "$T/msgs"
printf '%-22s %6s %6s %6s %8s\n' "unit" "files" "walk" "typed" "diags"
for dir in novac/src novac/src/*/; do
    dir=${dir%/}
    files=$(ls "$dir"/*.nv 2>/dev/null)
    [ -n "$files" ] || continue
    nfiles=$(printf '%s\n' "$files" | wc -l | tr -d '[:space:]')
    # shellcheck disable=SC2086
    run_unit $files
    grep -o '"file":"[^"]*"' "$T/out" | sed 's/^"file":"//; s/"$//; s#^.*novac/src/#novac/src/#' \
        | sort -u > "$T/refused"
    nref=$(wc -l < "$T/refused" | tr -d '[:space:]')
    rest=""
    for f in $files; do
        grep -qxF "$f" "$T/refused" || rest="$rest $f"
    done
    if [ "$nref" -gt 0 ] && [ -n "$rest" ]; then
        # shellcheck disable=SC2086
        run_unit $rest
    fi
    ntyped=$(printf '%s\n' $rest | grep -c . )
    n=$(grep -c '"code":' "$T/out")
    ni=$(grep -c 'E_NOVAC_ICE' "$T/out")
    grand=$((grand + n))
    ice=$((ice + ni))
    grep -o '"message":"[^"]*' "$T/out" | sed 's/^"message":"//' | cut -c1-90 >> "$T/msgs"
    printf '%-22s %6s %6s %6s %8s\n' "${dir#novac/}" "$nfiles" "$nref" "$ntyped" "$n"
done
echo "novac-self-residual: typing diagnostics $grand (LOWER BOUND -- walk-refused files are not typed), ICE $ice"
if [ "$ice" -gt 0 ]; then
    echo "novac-self-residual: WARNING -- an ICE aborts its run; the total hides what stood behind it" >&2
fi
echo "top causes:"
sort "$T/msgs" | uniq -c | sort -rn | head -15
