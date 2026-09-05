#!/bin/sh
# Самотест check-expect-cc-error-ratchet.sh:
#   (1) не ложнит, когда число равно базе;
#   (2) КРАСНЕЕТ на росте — новая фикстура сверх базы;
#   (3) НЕ считает прозу: маркер, процитированный в комментарии, не маркер
#       (ровно тот случай, из-за которого наивный греп по дереву даёт 10
#       вместо 8 — две фикстуры давно переведены, а имя живёт в их истории);
#   (4) НЕ судит `novac/**` — там свой страж с другим утверждением
#       (`check-novac-legacy-workarounds.py`, ось B), и два дома на один
#       предмет запрещены.
set -u
GUARD="$(cd "$(dirname "$0")/.." && pwd)/check-expect-cc-error-ratchet.sh"
TMP="${TMPDIR:-/tmp}/ccerr_selftest_$$"
rm -rf "$TMP"; mkdir -p "$TMP/spec_tests/conformance/neg" "$TMP/novac/src" "$TMP/scripts/guards"
cp "$GUARD" "$TMP/scripts/guards/"
G="$TMP/scripts/guards/check-expect-cc-error-ratchet.sh"
B="$TMP/scripts/guards/expect-cc-error.baseline"

printf 'cc_error_fixtures=1\n' > "$B"

# (1) ровно один носитель при базе 1 — зелено.
printf '// EXPECT_CC_ERROR incompatible\nmodule neg.one\n' > "$TMP/spec_tests/conformance/neg/one.nv"
bash "$G" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: ложняк при N == база"; rm -rf "$TMP"; exit 1; }

# (2) второй носитель — рост, обязан краснеть.
printf '// EXPECT_CC_ERROR redefinition\nmodule neg.two\n' > "$TMP/spec_tests/conformance/neg/two.nv"
bash "$G" "$TMP" >/dev/null 2>&1 && { echo "SELFTEST FAIL: не поймал рост"; rm -rf "$TMP"; exit 1; }
rm -f "$TMP/spec_tests/conformance/neg/two.nv"

# (3) проза: имя маркера НЕ в начале строки — не носитель.
printf '// EXPECT_COMPILE_ERROR E7301\n//\n// Ranshe zdes stoyal EXPECT_CC_ERROR -- perevedeno 2026-08-19.\nmodule neg.prose\n' \
    > "$TMP/spec_tests/conformance/neg/prose.nv"
bash "$G" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: посчитал прозу за маркер"; rm -rf "$TMP"; exit 1; }
rm -f "$TMP/spec_tests/conformance/neg/prose.nv"

# (4) novac не его дерево: носитель там не должен двигать это число.
printf '// EXPECT_CC_ERROR incompatible\nmodule novac.thing\n' > "$TMP/novac/src/thing.nv"
bash "$G" "$TMP" >/dev/null 2>&1 || { echo "SELFTEST FAIL: посчитал носителя в novac/ (чужой дом)"; rm -rf "$TMP"; exit 1; }

rm -rf "$TMP"
echo "selftest check-expect-cc-error-ratchet: OK (без ложняка / ловит рост / прозу не считает / novac не судит)"
