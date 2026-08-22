#!/bin/sh
# Самотест check-novac-commit-donor.sh (П16). Шов $3 — список файлов индекса.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-commit-donor.sh"
T="${TMPDIR:-/tmp}/novac-commit-donor-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$1" "$ROOT" "$2" > "$T/out" 2> "$T/err"; }
SRC="novac/src/sem/sem.nv"
DOC="docs/dev/novac-architecture.md"

# --- 1. донор с указателем — зелёный --------------------------------------
printf 'novac(274): interner\n\nDonor: rustc TyCtxt (rustc_middle::ty), interned Ty; Zig InternPool for the vector shape.\n' > "$T/m1"
run "$T/m1" "$SRC" && ok "донор с указателем — зелёный" || bad "донор с указателем покраснел: $(cat "$T/err")"

# --- 2. ГЛАВНЫЙ случай: правка novac/src без строки Donor — красный -------
printf 'novac(274): a big design change\n\nWe did a thing.\n' > "$T/m2"
if run "$T/m2" "$SRC"; then bad "novac/src без Donor прошёл — страж не ловит свой главный случай"; else grep -q "строки 'Donor:'" "$T/err" && ok "отсутствие Donor поймано" || bad "красный, но не про Donor"; fi

# --- 3. голое имя языка без сущности — красный ---------------------------
printf 'novac(274): x\n\nDonor: rustc\n' > "$T/m3"
if run "$T/m3" "$SRC"; then bad "'Donor: rustc' без сущности прошло — это утверждение, не указатель"; else grep -q "без сущности" "$T/err" && ok "голое имя донора поймано" || bad "красный, но не про сущность"; fi

# --- 4. честное «донора нет» с причиной — зелёный ------------------------
printf 'novac(274): Verdict\n\nDonor: none — the exit code is a closed set of three, a sum states that where an int cannot.\n' > "$T/m4"
run "$T/m4" "$SRC" && ok "'none — причина' принимается" || bad "честное none покраснело: $(cat "$T/err")"

# --- 5. «none» без причины — красный ------------------------------------
printf 'novac(274): x\n\nDonor: none — nope\n' > "$T/m5"
if run "$T/m5" "$SRC"; then bad "'none' с двумя словами прошло"; else grep -q "без причины" "$T/err" && ok "none без причины поймано" || bad "красный, но не про причину"; fi

# --- 6. механическая правка — законная короткая форма --------------------
printf 'guards(274): fix a false positive\n\nDonor: none — mechanical fix, no design decision\n' > "$T/m6"
run "$T/m6" "$SRC" && ok "механическая форма принимается" || bad "механическая форма покраснела: $(cat "$T/err")"

# --- 7. коммит НЕ трогает novac/src — судить нечего ---------------------
printf 'docs: plan text\n' > "$T/m7"
run "$T/m7" "$DOC"
grep -q "судить нечего" "$T/out" && ok "правка вне novac/src не судится" || bad "правка доков попала под суд: $(cat "$T/out")"

# --- 8. только тест-файл novac — судить нечего --------------------------
run "$T/m7" "novac/src/types/types_test.nv"
grep -q "судить нечего" "$T/out" && ok "тест-файл не судится" || bad "тест-файл попал под суд"

# --- 9. Donor не в первой строке, а в теле — находится -------------------
printf 'novac(274): x\n\nlong prose here\n\nDonor: Go types2.Info side table for resolved types.\n\nmore prose\n' > "$T/m9"
run "$T/m9" "$SRC" && ok "Donor в теле сообщения найден" || bad "Donor в теле не найден: $(cat "$T/err")"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-commit-donor ok: все случаи, включая правку без донора и голое имя языка"
    exit 0
fi
exit 1
