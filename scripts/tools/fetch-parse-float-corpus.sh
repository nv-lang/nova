#!/bin/bash
#
# Fetch parse-number-fxx test data corpus from upstream repository.
#
# Purpose: Clone or update a pinned version of the parse-number-fxx-test-data
# repository (approximately 5 million lines) without including it in the main
# repository. This script manages the external corpus cache for Plan 283
# (docs/plans/283-float-parse-own-rounding.md, phase 5).
#
# Usage: bash scripts/tools/fetch-parse-float-corpus.sh [DEST]
#   DEST defaults to $NOVA_PARSE_FLOAT_CORPUS if set, otherwise
#   <repo-root>/nova_tests/.cache/parse-number-fxx
#
# Exit codes:
#   0 = success (corpus is at pinned hash)
#   1 = failure (checkout mismatch, network error, or verification failed)
#
# Reference: docs/plans/283-float-parse-own-rounding.md
#

set -u
export LC_ALL=C

readonly PINNED="55d79b184b7d8fac2e143e89dc19b766ec4e54b8"
readonly UPSTREAM="https://github.com/nigeltao/parse-number-fxx-test-data"

# Determine repo root: two levels above this script
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Determine destination
DEST="${1:-${NOVA_PARSE_FLOAT_CORPUS:-${REPO_ROOT}/nova_tests/.cache/parse-number-fxx}}"

# If destination exists and is at pinned hash, report success and exit
if [ -d "$DEST" ]; then
  if actual_hash=$(git -C "$DEST" rev-parse HEAD 2>/dev/null); then
    if [ "$actual_hash" = "$PINNED" ]; then
      echo "fetch-parse-float-corpus ok: already at $PINNED in $DEST"
      exit 0
    fi
  fi
fi

# Clone repository (no checkout, we will checkout manually).
# `core.autocrlf=false` and `core.eol=lf` are NOT decoration: this machine has
# core.autocrlf=true globally, and under it git rewrites every line ending on checkout.
# A CRLF corpus is worse than a missing one -- the last field of each line becomes "1\r",
# the float grammar refuses it, and the runner reports millions of SKIPPED lines with zero
# checks, which reads exactly like a passing run. Measured 2026-09-07: 60 CR bytes in the
# 60 lines of more-test-cases.txt after the first clone.
if ! git -c core.autocrlf=false -c core.eol=lf clone --no-checkout "$UPSTREAM" "$DEST" 2>&1; then
  echo "fetch-parse-float-corpus FAIL: git clone failed" >&2
  exit 1
fi

# The clone's own config, so a later `git -C "$DEST" checkout` cannot reintroduce CRLF.
git -C "$DEST" config core.autocrlf false
git -C "$DEST" config core.eol lf

# Checkout the pinned commit (detached HEAD)
if ! git -C "$DEST" checkout --detach "$PINNED" 2>&1; then
  echo "fetch-parse-float-corpus FAIL: git checkout $PINNED failed" >&2
  exit 1
fi

# Verify the hash after checkout
actual_hash=$(git -C "$DEST" rev-parse HEAD)
if [ "$actual_hash" != "$PINNED" ]; then
  echo "fetch-parse-float-corpus FAIL: hash mismatch after checkout. Expected $PINNED, got $actual_hash" >&2
  exit 1
fi

# VERIFY THE LINE ENDINGS. The whole point of the two flags above, asserted rather than
# trusted: a single CR in the data means the runner will judge nothing while looking green.
cr_count=$(tr -cd '\r' < "$DEST/data/more-test-cases.txt" 2>/dev/null | wc -c)
if [ "${cr_count:-0}" -ne 0 ]; then
  echo "fetch-parse-float-corpus FAIL: the corpus checked out with CR bytes ($cr_count in" >&2
  echo "  more-test-cases.txt). Every line would end in CR, the float grammar would refuse" >&2
  echo "  the last field, and the slow-lane runner would report millions of skipped lines" >&2
  echo "  with zero checks. Remove $DEST and re-run; if it persists, check git core.autocrlf." >&2
  exit 1
fi

# Count files and lines
file_count=$(find "$DEST/data" -name "*.txt" 2>/dev/null | wc -l)
line_count=$(cat "$DEST/data"/*.txt 2>/dev/null | wc -l)

# Read first line of first file to verify corpus format
first_line=$(head -1 "$DEST/data"/*.txt 2>/dev/null | head -1)

echo "fetch-parse-float-corpus ok: $PINNED, files=$file_count, lines=$line_count, LF-only, at $DEST"
echo "Sample: $first_line"

exit 0
