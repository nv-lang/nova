#!/bin/sh
# Самотест check-novac-commit-no-simplification.py (П16). Швы: $3 — список
# проиндексированных файлов, $4 — текст диффа индекса.
#
# Форма строки ok: два пробела, слово ok, пробелы — по ней считают случаи
# (check-novac-registry-counts.sh); двоеточие после ok делает случай невидимым.
#
# ПРОБА ИДЁТ ПО КАЖДОМУ ИЗ ТРЁХ СВОЙСТВ ОТДЕЛЬНО, и по каждому — в обе стороны. Отдельный
# случай доказывает главное свойство стража: ПЕРВАЯ проверка смотрит на КОД и не закрывается
# идеальным сообщением. Без него страж был бы формой, которую заполняют, а не проверкой.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-commit-no-simplification.py"
T="${TMPDIR:-/tmp}/commit-nosimp-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

STAGED="novac/src/check/binds.nv"
run() { python "$G" "$1" "$ROOT" "$STAGED" "$2" > "$T/out" 2> "$T/err"; }

# Дифф без единого отказа подмножества.
printf '%s\n' '+++ b/novac/src/check/binds.nv' '+    ro n = 1' > "$T/diff-clean"
# Дифф, ВНОСЯЩИЙ бессрочный отказ.
printf '%s\n' '+++ b/novac/src/check/binds.nv' \
    '+    @report(kids, "outside the subset: a `Result` left side is refused here")' \
    > "$T/diff-undated"
# Дифф, вносящий отказ СО СРОКОМ.
printf '%s\n' '+++ b/novac/src/check/binds.nv' \
    '+    @report(kids, "outside the subset: a `Result` left side is refused here (E2-b3)")' \
    > "$T/diff-dated"
# Он же, но отказ стоит в КОММЕНТАРИИ — проза о долге долгом не является.
printf '%s\n' '+++ b/novac/src/check/binds.nv' \
    '+    // "outside the subset: a `Result` left side is refused here"' \
    > "$T/diff-comment"

msg() { printf '%s\n' "$@" > "$T/msg"; echo "$T/msg"; }

GOOD_SPEC='Spec: D86 04-effects.md -- `??` fallback, both container arms'
GOOD_SIMP='Simplifications: none'

# --- зелёная сторона: норма названа, упрощений нет -------------------------------
M=$(msg "novac: a slice" "" "$GOOD_SPEC" "$GOOD_SIMP")
run "$M" "$(cat "$T/diff-clean")" && ok "чистый дифф с названной нормой проходит" \
    || bad "чистый случай покраснел: $(cat "$T/err")"

# --- 1. КОД СИЛЬНЕЕ СООБЩЕНИЯ: идеальное сообщение НЕ спасает бессрочный отказ ----
M=$(msg "novac: a slice" "" "$GOOD_SPEC" "Simplifications: Result refused (E2-b3)")
if run "$M" "$(cat "$T/diff-undated")"; then
    bad "бессрочный отказ прошёл при идеальном сообщении - страж стал формой для заполнения"
else
    grep -q "без срока" "$T/err" && ok "бессрочный отказ ловится вопреки сообщению" \
        || bad "покраснел не тем текстом: $(cat "$T/err")"
fi

# --- отказ в КОММЕНТАРИИ долгом не считается ------------------------------------
M=$(msg "novac: a slice" "" "$GOOD_SPEC" "$GOOD_SIMP")
run "$M" "$(cat "$T/diff-comment")" && ok "процитированный в комментарии отказ не долг" \
    || bad "комментарий засчитан долгом - правило шире класса: $(cat "$T/err")"

# --- 3. СВЕРКА: 'none' против диффа, который вносит отказ СО СРОКОМ ---------------
M=$(msg "novac: a slice" "" "$GOOD_SPEC" "$GOOD_SIMP")
if run "$M" "$(cat "$T/diff-dated")"; then
    bad "'Simplifications: none' прошло при упрощении в диффе - сверки нет"
else
    grep -q "none" "$T/err" && ok "'none' против диффа с упрощением поймано" \
        || bad "покраснел не тем текстом"
fi

# --- ...и то же упрощение, НАЗВАННОЕ со сроком, законно ---------------------------
M=$(msg "novac: a slice" "" "$GOOD_SPEC" "Simplifications: Result left side refused (E2-b3)")
run "$M" "$(cat "$T/diff-dated")" && ok "названное упрощение со сроком проходит" \
    || bad "названное упрощение отвергнуто: $(cat "$T/err")"

# --- упрощение названо БЕЗ срока -------------------------------------------------
M=$(msg "novac: a slice" "" "$GOOD_SPEC" "Simplifications: Result left side refused for now")
if run "$M" "$(cat "$T/diff-dated")"; then
    bad "упрощение без этапа прошло"
else
    ok "упрощение без этапа отвергнуто"
fi

# --- 2. НОРМА: строки Spec нет ----------------------------------------------------
M=$(msg "novac: a slice" "" "$GOOD_SIMP")
if run "$M" "$(cat "$T/diff-clean")"; then
    bad "коммит без строки Spec прошёл"
else
    ok "коммит без строки Spec отвергнут"
fi

# --- 2. НОРМА РЕЗОЛВИТСЯ: несуществующий D-блок ----------------------------------
M=$(msg "novac: a slice" "" "Spec: D9999 invented block" "$GOOD_SIMP")
if run "$M" "$(cat "$T/diff-clean")"; then
    bad "несуществующий D-блок прошёл - указатель не проверяется"
else
    grep -q "D9999" "$T/err" && ok "несуществующий D-блок пойман по имени" \
        || bad "покраснел не тем текстом"
fi

# --- 'Spec: none' с причиной законен, без причины — нет ---------------------------
M=$(msg "novac: a slice" "" "Spec: none -- a move of code between modules, no language form" \
    "$GOOD_SIMP")
run "$M" "$(cat "$T/diff-clean")" && ok "'Spec: none' с причиной проходит" \
    || bad "'Spec: none' с причиной отвергнут: $(cat "$T/err")"

M=$(msg "novac: a slice" "" "Spec: none -- because" "$GOOD_SIMP")
if run "$M" "$(cat "$T/diff-clean")"; then
    bad "'Spec: none' без причины прошло"
else
    ok "'Spec: none' без причины отвергнуто"
fi

# --- строки Simplifications нет ---------------------------------------------------
M=$(msg "novac: a slice" "" "$GOOD_SPEC")
if run "$M" "$(cat "$T/diff-clean")"; then
    bad "коммит без строки Simplifications прошёл"
else
    ok "коммит без строки Simplifications отвергнут"
fi

# --- МИШЕНЬ ПОТЕРЯНА: novac/src не в индексе — судить нечего, и это не отказ -------
STAGED_SAVE="$STAGED"
STAGED="docs/plans/274.md"
M=$(msg "docs: a plan edit")
run "$M" "$(cat "$T/diff-clean")" && ok "коммит без novac/src не судится" \
    || bad "коммит вне novac/src покраснел"
STAGED="$STAGED_SAVE"

# --- ТЕСТОВЫЙ файл не судится: тест ЦИТИРУЕТ отказы ------------------------------
STAGED="novac/src/check/binds_test.nv"
M=$(msg "novac: a test")
run "$M" "$(cat "$T/diff-undated")" && ok "правка только теста не судится" \
    || bad "тестовый файл засчитан - цитата отказа стала долгом"
STAGED="$STAGED_SAVE"

if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-commit-no-simplification ok: 13 случаев, обе стороны по трём свойствам"
    exit 0
fi
echo "test-check-novac-commit-no-simplification: FAIL -- $fails" >&2
exit 1
