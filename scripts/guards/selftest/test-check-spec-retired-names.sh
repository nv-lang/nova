#!/bin/sh
# selftest/test-check-spec-retired-names.sh — страж снятых имён умеет краснеть.
# Живая половина + восемь случаев на подставной спеке.
#
# Главный из восьми — случай «протокол требует снятый метод»: ровно та строка
# D55, ради которой страж заведён (реестр №844). Она обязана краснеть, иначе
# страж не ловит того, что уже случилось.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-spec-retired-names.sh"
T="${TMPDIR:-/tmp}/selftest-spec-retired.$$"
trap 'rm -rf "$T"' 0 2 15
rc=0

# ── живая половина: настоящее дерево обязано быть зелёным ───────────────
if ! sh "$GUARD" "$ROOT" >"$T.live" 2>&1; then
    echo "FAIL: живая половина красная на настоящем дереве:" >&2
    tail -4 "$T.live" | sed 's/^/    /' >&2
    rc=1
fi
rm -f "$T.live"

# mk <строка-содержимое-спеки> [<число-в-базе>]
# Строит подставной корень: одно решение, один список, одна база.
mk() {
    rm -rf "$T"; mkdir -p "$T/spec/decisions/history" "$T/g"
    {
        echo "# D999 — fake decision"
        echo "$1"
    } > "$T/spec/decisions/99-fake.md"
    printf 'with_capacity # amendment D372 of 2026-07-06 -- replaced by Self.new(cap)\n' \
        > "$T/g/names.list"
    printf '# fake baseline\nspec/decisions/99-fake.md %s\n' "${2:-0}" > "$T/g/names.baseline"
}

run() {
    NOVA_RETIRED_NAMES="$T/g/names.list" \
    NOVA_RETIRED_BASELINE="$T/g/names.baseline" \
        sh "$GUARD" "$T" >"$T/out" 2>&1
}

expect() { # expect <green|red> <ярлык>
    run; got=$?
    if [ "$1" = green ] && [ "$got" -ne 0 ]; then
        echo "FAIL: «$2» обязано быть зелёным, страж красный:" >&2
        sed 's/^/    /' "$T/out" >&2; rc=1
    fi
    if [ "$1" = red ] && [ "$got" -eq 0 ]; then
        echo "FAIL: «$2» обязано краснеть, страж зелёный:" >&2
        sed 's/^/    /' "$T/out" >&2; rc=1
    fi
}

# ── 1. ГЛАВНЫЙ СЛУЧАЙ: протокол требует снятый метод (это и был №844) ───
mk '  - `static with_capacity(n int) -> Self` — предаллоцировать под `n` записей;' 0
expect red "протокол требует static with_capacity"

# ── 2. живая форма вместо снятой — зелено ───────────────────────────────
mk '  - `Self.new(cap int = 16) -> Self` — конструктор с ёмкостью;' 0
expect green "живая форма Self.new(cap)"

# ── 3. строка САМА говорит, что имя снято — законное упоминание ─────────
mk '  АМЕНДМЕНТ D372: `with_capacity` снят, ёмкость — свойство `cap`.' 0
expect green "строка называет ретракцию"

# ── 4. зачёркнутая строка — законна (так пишут ретракцию) ──────────────
mk '  ~~`static with_capacity(n int) -> Self`~~' 0
expect green "зачёркнутая строка"

# ── 5. англоязычная пометка removed/ex-` — законна ─────────────────────
mk '  `with_capacity` (removed 2026-07-06, use `.new(cap)`)' 0
expect green "английская пометка removed"

# ── 6. ХРАПОВИК: столько же, сколько в базе — зелено; больше — красно ───
mk '  `with_capacity(n)` и ещё раз `with_capacity(m)`' 2
expect green "два упоминания при базе 2"
printf '# fake baseline\nspec/decisions/99-fake.md 1\n' > "$T/g/names.baseline"
expect red "два упоминания при базе 1"

# ── 7. файл, которого нет в базе, считается нулём ──────────────────────
mk '  `with_capacity(n)`' 0
printf '# fake baseline\n' > "$T/g/names.baseline"
expect red "файл вне базы с упоминанием"

# ── 8. история не судится: тот же текст в history/ не считается ────────
mk '  ничего лишнего' 0
printf 'Rejected: `[]T.with_capacity(...)` duplicated `[]`\n' \
    > "$T/spec/decisions/history/rejected.md"
expect green "упоминание в history/ не считается"

# ── 9. имя без объяснения после «#» — красно (подозрение, не факт) ─────
mk '  ничего лишнего' 0
printf 'with_capacity\n' > "$T/g/names.list"
expect red "имя в списке без объяснения"

# ── 10. нет базы — красно (без базы храповик не храповик) ──────────────
mk '  ничего лишнего' 0
rm -f "$T/g/names.baseline"
expect red "отсутствующая база"

[ "$rc" -eq 0 ] && echo "test-check-spec-retired-names ok: живая половина зелёная, десять подделок судятся верно"
exit "$rc"
