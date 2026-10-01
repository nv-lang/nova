#!/usr/bin/env bash
# p2 -- check-push-proven-by-ci.py: a CLOSED registry row is read as OPEN, so a
# stale exemption in ci-accepted-red.list is ACCEPTED instead of refused.
#
# Header promise (lines 21-24): "A closed or missing row makes the entry stale,
# and a stale entry is a refusal even when everything is green".
# Header promise (lines 32-34): "CLOSED ROW -- the same reading as
# registry-routes-scan.py ... the `**Статус:**` field of the row contains ЗАКРЫТ".
#
# Code (row_state): takes the FIRST occurrence of the literal bold `**Статус:**`
# anywhere in the row and reads 60 chars after it. A row whose real status is
# written `Статус: ЗАКРЫТ` (not bold) and which later QUOTES the bold form in
# prose is judged by the quote.
#
# A: control -- canonical bold closed field -> expected refusal "is closed".
# B: synthetic row of the same shape        -> stale exemption accepted.
# C: the REAL registry row 651 copied verbatim (registry-real-651.md) -> same.
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-push-proven-by-ci.py"
PY=python3; "$PY" -c 'print(1)' >/dev/null 2>&1 || PY=python
SHA=1111111111111111111111111111111111111111
[ -f "$G" ] || { echo "no guard at $G (run from the repository root)"; exit 2; }
for f in runs-red.json jobs-red.json registry-canonical.md registry-synthetic.md registry-real-651.md acc-9005.list acc-651.list; do
    [ -s "$HERE/$f" ] || { echo "MISSING probe file $f"; exit 2; }
done
run() { "$PY" "$G" "$SHA" --runs-json "$HERE/runs-red.json" --jobs-json "$HERE/jobs-red.json" \
          --registry "$HERE/$1" --accepted "$HERE/$2" 2>&1; echo "rc=$?"; }
echo "=== A. control: row 9005 closed in canonical bold field (expect FAIL 'is closed')"
run registry-canonical.md acc-9005.list
echo "=== B. row 9005 closed as 'Статус: ЗАКРЫТ', prose later quotes '**Статус:**'"
run registry-synthetic.md acc-9005.list
echo "=== C. real registry row 651, verbatim"
run registry-real-651.md acc-651.list
