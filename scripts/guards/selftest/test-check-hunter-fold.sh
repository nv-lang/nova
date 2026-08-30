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

mk_root() {
    rm -rf "$T"
    mkdir -p "$T/docs/plans" "$T/docs/dev/hunts/novac/probes/2026-03-03-parse-k3" \
             "$T/docs/dev/hunts/oracle"
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

[ "$rc" -eq 0 ] && echo "test-check-hunter-fold ok: четыре подделки покраснели, здоровый макет и живая половина зелёные"
exit "$rc"
