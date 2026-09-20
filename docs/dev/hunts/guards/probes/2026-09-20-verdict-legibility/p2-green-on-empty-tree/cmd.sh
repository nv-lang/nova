#!/usr/bin/env bash
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
S="$HERE/skel"
# minimal tree: every directory the guards read EXISTS and is EMPTY
rm -rf "$S"
mkdir -p "$S/scripts/guards/selftest" "$S/scripts/claude-hooks/selftest" "$S/scripts/tools" \
  "$S/compiler-codegen/src" "$S/compiler-codegen/nova_rt" "$S/nova-cli/src" "$S/std/src" \
  "$S/spec_tests/conformance/neg" "$S/spec_tests/conformance/standalone" \
  "$S/docs/dev" "$S/docs/plans" "$S/docs/guide" "$S/spec/decisions" "$S/examples"
# python cores + baselines are the guards' own machinery, not the subject
cp "$ROOT"/scripts/guards/*.py "$S/scripts/guards/" 2>/dev/null
cp "$ROOT"/scripts/guards/*.baseline "$S/scripts/guards/" 2>/dev/null
cp -r "$ROOT"/scripts/guards/lib "$S/scripts/guards/" 2>/dev/null
for g in check-generic-static.sh check-test-env-races.sh check-no-control-chars.sh \
         check-no-handwritten-plan-index.sh check-invariant-discipline.sh check-expect-markers.sh; do
  a=$(bash "$ROOT/scripts/guards/$g" "$ROOT" 2>&1); ra=$?
  b=$(bash "$ROOT/scripts/guards/$g" "$S"    2>&1); rb=$?
  echo "### $g"
  echo "  A real tree  rc=$ra : $a"
  echo "  B empty tree rc=$rb : $b"
  [ "$a" = "$b" ] && echo "  -> VERDICT BYTE-IDENTICAL" || echo "  -> differs"
done
