#!/bin/sh
# Самотест check-novac-commit-forms.sh (П16). Шов $3 — список файлов индекса.
#
# КЛЕТКА 4 — ГЛАВНАЯ, и она есть замер, а не выдумка: 2026-09-18 правка парсера
# перечислила ОДНУ форму (`#impl(P)`) и пропустила `#impl(A + B + C)` из D186.
# Четыре зелёные пробы ничего не сказали, потому что все четыре были об одном
# атрибуте против другого. Порог в две формы — ровно про этот случай.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-commit-forms.sh"
T="${TMPDIR:-/tmp}/novac-commit-forms-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$1" "$ROOT" "$2" > "$T/out" 2> "$T/err"; }
PARSE="novac/src/parse/decls.nv"
CHECK="novac/src/check/check.nv"
DOC="docs/dev/novac-architecture.md"

# --- 1. полная строка — зелёный -------------------------------------------
printf 'novac(parse): attribute argument\n\nForms: D186 spec-reader — #impl(P), #impl(P1 + P2 + ...), #impl() empty\n' > "$T/m1"
run "$T/m1" "$PARSE" && ok "источник + spec-reader + две формы — зелёный" || bad "полная строка покраснела: $(cat "$T/err")"

# --- 2. ГЛАВНЫЙ случай: парсер тронут, строки нет — красный ---------------
printf 'novac(parse): teach the cursor a new form\n\nWe did a thing.\n' > "$T/m2"
if run "$T/m2" "$PARSE"; then bad "парсер без Forms прошёл — страж не ловит свой главный случай"; else grep -q "строки 'Forms:'" "$T/err" && ok "отсутствие Forms поймано" || bad "красный, но не про Forms"; fi

# --- 3. нет источника — красный -------------------------------------------
printf 'novac(parse): x\n\nForms: spec-reader — #impl(P), #impl(A + B)\n' > "$T/m3"
if run "$T/m3" "$PARSE"; then bad "Forms без D-блока прошло — перечень не с чем сверить"; else grep -q "ИСТОЧНИК" "$T/err" && ok "отсутствие источника поймано" || bad "красный, но не про источник"; fi

# --- 4. ОДНА форма — красный (замер 2026-09-18) ---------------------------
printf 'novac(parse): x\n\nForms: D186 spec-reader — #impl(P)\n' > "$T/m4"
if run "$T/m4" "$PARSE"; then bad "одна форма прошла — это ровно тот случай, ради которого страж заведён"; else grep -q "меньше двух форм" "$T/err" && ok "одна форма поймана" || bad "красный, но не про число форм"; fi

# --- 5. без spec-reader — красный -----------------------------------------
printf 'novac(parse): x\n\nForms: D186 — #impl(P), #impl(A + B)\n' > "$T/m5"
if run "$T/m5" "$PARSE"; then bad "Forms без spec-reader прошло — агент и есть требование владельца"; else grep -q "spec-reader" "$T/err" && ok "отсутствие spec-reader поймано" || bad "красный, но не про агента"; fi

# --- 6. «none — причина» — зелёный ----------------------------------------
printf 'novac(parse): rename a local\n\nForms: none — a rename inside one door, the set of read forms is untouched.\n' > "$T/m6"
run "$T/m6" "$PARSE" && ok "none с причиной — зелёный" || bad "законное none покраснело: $(cat "$T/err")"

# --- 7. «none» без причины — красный --------------------------------------
printf 'novac(parse): x\n\nForms: none — trivial\n' > "$T/m7"
if run "$T/m7" "$PARSE"; then bad "'none — trivial' прошло: отписка в одно слово"; else grep -q "без причины" "$T/err" && ok "none без причины поймано" || bad "красный, но не про причину"; fi

# --- 8. ГРАНИЦА: чекер без парсера — судить нечего ------------------------
#     Названа клеткой, а не прозой: периметр молчит ровно так же, как зелень,
#     и без этой клетки расширение периметра нельзя было бы отличить от поломки.
printf 'novac(check): reword a refusal\n\nNothing about forms here.\n' > "$T/m8"
run "$T/m8" "$CHECK" && grep -q "судить нечего" "$T/out" && ok "чекер вне периметра — судить нечего" || bad "чекер попал под правило: периметр шире объявленного"

# --- 9. доки и планы — судить нечего --------------------------------------
printf 'plan(274.7): a record\n' > "$T/m9"
run "$T/m9" "$DOC" && grep -q "судить нечего" "$T/out" && ok "документ вне периметра" || bad "документ попал под правило"

# --- 10. пустой индекс — судить нечего ------------------------------------
printf 'chore: nothing staged\n' > "$T/m10"
run "$T/m10" "" && grep -q "судить нечего" "$T/out" && ok "пустой индекс — судить нечего" || bad "пустой индекс покраснел"

if [ "$fails" -ne 0 ]; then
    echo "selftest check-novac-commit-forms: FAIL ($fails)" >&2
    exit 1
fi
echo "selftest check-novac-commit-forms: OK (полная строка / нет строки / нет источника / ОДНА форма / нет spec-reader / none с причиной / none без причины / чекер вне периметра / документ вне периметра / пустой индекс)"
