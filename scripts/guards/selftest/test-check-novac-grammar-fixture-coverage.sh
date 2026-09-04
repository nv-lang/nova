#!/bin/sh
# Самотест check-novac-grammar-fixture-coverage.sh — оба направления
# (норма 254): ловит форму без пары фикстур И не краснеет на законном.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-grammar-fixture-coverage.sh"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-grammar-cov-selftest.$$"
mkdir -p "$T"
fails=0
CASES=0
ok() { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  FAIL: $1" >&2; fails=$((fails+1)); }

mkfx() { # $1 = novac-dir, $2 = форма, $3... = имена файлов
    d="$1/fixtures/$2"; shift 2
    mkdir -p "$d"
    for n in "$@"; do echo "// fixture" > "$d/$n"; done
}

# 1–2. Законный: две формы, у каждой pos+neg — зелено, строка ok:.
mkdir -p "$T/good"
printf 'form-let\nform-match\n' > "$T/good/grammar-forms.txt"
mkfx "$T/good" form-let   pos_basic.nv neg_missing_init.nv
mkfx "$T/good" form-match pos_exhaustive.nv neg_no_arms.nv
sh "$G" "$ROOT" "$T/good" >/dev/null 2>&1 && ok "полная пара у каждой формы проходит" || bad "законная карта покраснела"
sh "$G" "$ROOT" "$T/good" 2>/dev/null | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"

# 3. Нарушение: форма без neg_*.nv — красная.
mkdir -p "$T/noneg"
printf 'form-let\n' > "$T/noneg/grammar-forms.txt"
mkfx "$T/noneg" form-let pos_basic.nv
sh "$G" "$ROOT" "$T/noneg" >/dev/null 2>&1 && bad "форма без neg прошла" || ok "форма без neg поймана"

# 4. Нарушение: форма без каталога фикстур вовсе — красная.
mkdir -p "$T/nodir/fixtures"
printf 'form-ghost\n' > "$T/nodir/grammar-forms.txt"
sh "$G" "$ROOT" "$T/nodir" >/dev/null 2>&1 && bad "форма без каталога прошла" || ok "форма без каталога поймана"

# 5. Законный: реестра нет — зелёный «судить нечего».
mkdir -p "$T/noreg"
sh "$G" "$ROOT" "$T/noreg" 2>/dev/null | grep -q 'ok: судить нечего' && ok "нет реестра — честное «судить нечего»" || bad "нет реестра — нет «ok: судить нечего»"

# 6. Законный: реестр из пустых строк и комментариев — «судить нечего».
mkdir -p "$T/emptyreg"
printf '# формы появятся с Э1\n\n' > "$T/emptyreg/grammar-forms.txt"
sh "$G" "$ROOT" "$T/emptyreg" 2>/dev/null | grep -q 'ok: судить нечего' && ok "пустой реестр — честное «судить нечего»" || bad "пустой реестр — нет «ok: судить нечего»"

# 7. Нарушение: первая форма цела, вторая без pos — красный (ловит не только первую).
mkdir -p "$T/second"
printf 'form-let\nform-match\n' > "$T/second/grammar-forms.txt"
mkfx "$T/second" form-let   pos_basic.nv neg_missing_init.nv
mkfx "$T/second" form-match neg_no_arms.nv
sh "$G" "$ROOT" "$T/second" >/dev/null 2>&1 && bad "сломанная вторая форма прошла" || ok "сломанная вторая форма поймана"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-grammar-fixture-coverage ok: $CASES/$CASES"
    exit 0
fi
echo "test-check-novac-grammar-fixture-coverage: FAIL ($fails)" >&2
exit 1
