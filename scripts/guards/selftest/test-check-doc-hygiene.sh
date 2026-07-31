#!/bin/sh
# Самотест check-doc-hygiene.sh: (1) ловит РОСТ, (2) не ложнит на равном,
# (3) храповик пропускает долг в пределах baseline. На временном дереве.
set -u
GUARD="$(cd "$(dirname "$0")/.." && pwd)/check-doc-hygiene.sh"
TMP="${TMPDIR:-/tmp}/dh_selftest_$$"
rm -rf "$TMP"; mkdir -p "$TMP/std/src" "$TMP/examples" "$TMP/scripts/guards" "$TMP/compiler-codegen/src"
cp "$GUARD" "$TMP/scripts/guards/"
: > "$TMP/compiler-codegen/src/lints.rs"
printf 'cyrillic_doc=0\ninternal_doc=0\ncyrillic_lint=0\n' > "$TMP/scripts/guards/doc-hygiene.baseline"

# (2) чистое дерево — зелёный
sh "$TMP/scripts/guards/check-doc-hygiene.sh" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: ложняк на чистом"; rm -rf "$TMP"; exit 1; }

# (1) рост кириллицы в /// — красный
printf '/// документация по-русски\nfn probe() -> int => 1\n' > "$TMP/std/src/p.nv"
sh "$TMP/scripts/guards/check-doc-hygiene.sh" "$TMP" >/dev/null 2>&1 && { echo "SELFTEST FAIL: не поймал рост"; rm -rf "$TMP"; exit 1; }

# (3) храповик: долг в пределах baseline — зелёный
printf 'cyrillic_doc=1\ninternal_doc=0\ncyrillic_lint=0\n' > "$TMP/scripts/guards/doc-hygiene.baseline"
sh "$TMP/scripts/guards/check-doc-hygiene.sh" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: храповик не пропускает baseline-долг"; rm -rf "$TMP"; exit 1; }

rm -rf "$TMP"
echo "selftest check-doc-hygiene: OK (ловит рост / без ложняка / храповик работает)"
