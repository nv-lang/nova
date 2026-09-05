#!/bin/sh
# Самотест check-expect-marker-colon.sh: (1) не ложнит на правильной форме,
# (2) ловит двоеточие, (3) ловит его и у других EXPECT_*, не только STDOUT,
# (4) НЕ судит прозу в docs/ — там форма цитируется, в том числе самим стражем.
set -u
GUARD="$(cd "$(dirname "$0")/.." && pwd)/check-expect-marker-colon.sh"
TMP="${TMPDIR:-/tmp}/emc_selftest_$$"
rm -rf "$TMP"; mkdir -p "$TMP/spec_tests/conformance" "$TMP/docs/dev" "$TMP/scripts/guards"
cp "$GUARD" "$TMP/scripts/guards/"
G="$TMP/scripts/guards/check-expect-marker-colon.sh"

printf '// EXPECT_STDOUT ok\nmodule spec_tests.conformance\n' > "$TMP/spec_tests/conformance/good.nv"
bash "$G" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: ложняк на правильной форме без двоеточия"; rm -rf "$TMP"; exit 1; }

printf '// EXPECT_STDOUT: ok\nmodule spec_tests.conformance\n' > "$TMP/spec_tests/conformance/bad.nv"
bash "$G" "$TMP" >/dev/null 2>&1 && { echo "SELFTEST FAIL: не поймал EXPECT_STDOUT с двоеточием"; rm -rf "$TMP"; exit 1; }
rm -f "$TMP/spec_tests/conformance/bad.nv"

# Не только STDOUT: правило про имя маркера вообще, и страж обязан это уметь —
# иначе следующая опечатка приедет через EXPECT_COMPILE_ERROR и мы пройдём то же
# самое во второй раз.
printf '// EXPECT_COMPILE_ERROR: E_FOO\nmodule spec_tests.conformance\n' > "$TMP/spec_tests/conformance/bad2.nv"
bash "$G" "$TMP" >/dev/null 2>&1 && { echo "SELFTEST FAIL: не поймал двоеточие у EXPECT_COMPILE_ERROR"; rm -rf "$TMP"; exit 1; }
rm -f "$TMP/spec_tests/conformance/bad2.nv"

# Проза в docs/ обязана оставаться нетронутой: сам этот страж объясняет форму,
# цитируя её, и страж, читающий прозу как данные, краснел бы на своём объяснении.
printf 'Nikogda ne pishi `// EXPECT_STDOUT: ok` -- dvoetochie uhodit v podstroku.\n' > "$TMP/docs/dev/prose.md"
bash "$G" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: покраснел на прозе в docs/"; rm -rf "$TMP"; exit 1; }

rm -rf "$TMP"
echo "selftest check-expect-marker-colon: OK (без ложняка / ловит STDOUT и COMPILE_ERROR / прозу не судит)"
