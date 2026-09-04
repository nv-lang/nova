#!/bin/sh
# selftest/test-check-hunter-fold.sh — страж свёртки умеет краснеть.
# Живая половина + здоровый макет + четыре подделки: переполнение кучи,
# свёртка-потеряшка, нечитаемая строка леджера, пробы-сироты.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-hunter-fold.sh"
T="${TMPDIR:-/tmp}/selftest-hunter-fold.$$"
trap 'rm -rf "$T"' 0 2 15
rc=0

# Живая половина: настоящее дерево обязано быть зелёным.
if ! sh "$GUARD" "$ROOT" >"$T.live" 2>&1; then
    echo "FAIL: живая половина красная на настоящем дереве:" >&2
    tail -2 "$T.live" | sed 's/^/    /' >&2
    rc=1
fi
rm -f "$T.live"

# Макет с НАСТОЯЩИМ git: страж требует, чтобы свёрнутый отчёт когда-то
# существовал в истории (дыра панели: фантомная свёртка закрывала клетку без
# охоты). Значит и здоровый макет обязан иметь историю.
mk_root() {
    rm -rf "$T"
    mkdir -p "$T/docs/plans" "$T/docs/dev/hunts/novac/probes/2026-03-03-parse-k3" \
             "$T/docs/dev/hunts/oracle" "$T/docs/dev/hunts/guards"
    git -C "$T" init -q
    git -C "$T" config user.email selftest@example.com
    git -C "$T" config user.name selftest
    git -C "$T" config commit.gpgsign false
    printf 'свёрнутый отчёт\n' > "$T/docs/dev/hunts/novac/2026-01-01-lex-k2.md"
    git -C "$T" add docs/dev/hunts/novac/2026-01-01-lex-k2.md
    git -C "$T" commit -qm "hunt report that will be folded"
    git -C "$T" rm -q docs/dev/hunts/novac/2026-01-01-lex-k2.md
    git -C "$T" commit -qm "fold it"
    printf 'открытый отчёт\n' > "$T/docs/dev/hunts/novac/2026-03-03-parse-k3.md"
    printf 'СВЁРНУТО | 2026-01-01-lex-k2 | lex | К2 | находок 2 | №810,№811\n' \
        > "$T/docs/dev/hunts/novac/LEDGER.md"
    printf '| 810 | t | НАЙДЕНО ОХОТНИКОМ 2026-01-01 (novac). |\n| 811 | t | НАЙДЕНО ОХОТНИКОМ 2026-01-01 (novac). |\n' \
        > "$T/docs/plans/221.1-bug-sweep.md"
    printf 'max_open=2\n' > "$T/base"
}

mk_root
if ! NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o0" 2>&1; then
    echo "FAIL: здоровый макет красный — страж ложнит:" >&2
    tail -2 "$T.o0" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 1: открытых отчётов больше предела ─────────────────────────
mk_root
printf 'x\n' > "$T/docs/dev/hunts/novac/2026-03-04-a.md"
printf 'x\n' > "$T/docs/dev/hunts/novac/2026-03-05-b.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o1" 2>&1; then
    echo "FAIL: три открытых отчёта при пределе 2 прошли — куча копится бесконечно" >&2
    rc=1
fi

# ── подделка 2: свёрнутая находка без метки в реестре ───────────────────
mk_root
printf '| 810 | t | НАЙДЕНО ОХОТНИКОМ 2026-01-01 (novac). |\n' > "$T/docs/plans/221.1-bug-sweep.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o2" 2>&1; then
    echo "FAIL: леджер ссылается на №811, которого нет в реестре с меткой, — свёртка теряет находки" >&2
    rc=1
fi

# ── подделка 3: строка леджера не по формату ────────────────────────────
mk_root
printf 'СВЁРНУТО | как-нибудь потом\n' >> "$T/docs/dev/hunts/novac/LEDGER.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o3" 2>&1; then
    echo "FAIL: нечитаемая строка СВЁРНУТО промолчана — мера mark-стража соврёт (№801)" >&2
    rc=1
fi

# ── подделка 4: пробы-сироты без открытого отчёта ───────────────────────
mk_root
mkdir -p "$T/docs/dev/hunts/novac/probes/2026-01-01-lex-k2/x"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o4" 2>&1; then
    echo "FAIL: пробы свёрнутого отчёта остались в дереве и прошли — свёртка не убирает кучу" >&2
    rc=1
fi

# ── подделка 5: фантомная свёртка — отчёта не было в истории ────────────
mk_root
printf 'СВЁРНУТО | 2026-05-05-mono-k7 | mono | К7 | находок 0 | —\n' \
    >> "$T/docs/dev/hunts/novac/LEDGER.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o5" 2>&1; then
    echo "FAIL: свёртка отчёта, которого не было в истории, прошла — рукописная строка закрывает клетку без охоты" >&2
    rc=1
fi

# ── подделка 6: одна ссылка в двух строках леджера ──────────────────────
mk_root
printf 'СВЁРНУТО | 2026-01-01-lex-k2 | lex | К2 | находок 1 | №810\n' \
    > "$T/docs/dev/hunts/novac/LEDGER.md"
printf 'СВЁРНУТО | 2026-01-01-lex-k2 | lex | К3 | находок 1 | №810\n' \
    >> "$T/docs/dev/hunts/novac/LEDGER.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o6" 2>&1; then
    echo "FAIL: одна строка реестра засчитана двум свёрткам — знаменатель меры надувается" >&2
    rc=1
fi

# ── подделка 7: ссылка на ЧУЖУЮ охоту (дата метки не та) ────────────────
mk_root
printf 'СВЁРНУТО | 2026-01-01-lex-k2 | lex | К2 | находок 1 | №810\n' \
    > "$T/docs/dev/hunts/novac/LEDGER.md"
printf '| 810 | t | НАЙДЕНО ОХОТНИКОМ 2026-07-07 (novac). |\n' \
    > "$T/docs/plans/221.1-bug-sweep.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o7" 2>&1; then
    echo "FAIL: свёртка подтверждена строкой реестра ЧУЖОЙ охоты (дата метки не совпадает со стемом)" >&2
    rc=1
fi

# ── подделка 8: ссылок больше, чем находок ──────────────────────────────
mk_root
printf 'СВЁРНУТО | 2026-01-01-lex-k2 | lex | К2 | находок 1 | №810,№811\n' \
    > "$T/docs/dev/hunts/novac/LEDGER.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o8" 2>&1; then
    echo "FAIL: две ссылки при одной находке прошли — счёт свёртки не сходится" >&2
    rc=1
fi

# ── подделка 9: два max_open в базе (дописка побеждает) ─────────────────
mk_root
printf 'max_open=2\nmax_open=999\n' > "$T/base"
printf 'x\n' > "$T/docs/dev/hunts/novac/2026-03-04-a.md"
printf 'x\n' > "$T/docs/dev/hunts/novac/2026-03-05-b.md"
if NOVA_HUNTER_FOLD_BASELINE="$T/base" sh "$GUARD" "$T" >"$T.o9" 2>&1; then
    echo "FAIL: дописанная вторая строка max_open= раскрутила предел молча" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-hunter-fold ok: девять подделок покраснели, здоровый макет и живая половина зелёные"
exit "$rc"
