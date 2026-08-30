#!/bin/sh
# selftest/test-check-hunter-mark.sh — страж меры охотника умеет краснеть.
# Шесть подделок — по одной на каждый способ соврать мере (включая барьер
# бумажной охоты Ф.6), плюс живая половина.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-hunter-mark.sh"
T="${TMPDIR:-/tmp}/selftest-hunter-mark.$$"
trap 'rm -rf "$T"' 0 2 15
rc=0

# Живая половина на настоящем дереве: обязана быть зелёной.
if ! sh "$GUARD" "$ROOT" >"$T.live" 2>&1; then
    echo "FAIL: живая половина красная на настоящем дереве:" >&2
    tail -2 "$T.live" | sed 's/^/    /' >&2
    rc=1
fi
rm -f "$T.live"

# Здоровый макет: отчёт с находкой, пробы на месте, метка с треком.
mk_root() {
    rm -rf "$T"
    mkdir -p "$T/docs/plans" "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/x" \
             "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/y" \
             "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/z" \
             "$T/docs/dev/hunts/oracle"
    printf 'КЛЕТКА | lex | К2\nНАХОДКА | К2 | lex | x | novac rc0 / oracle rc1 | детали\n' \
        > "$T/docs/dev/hunts/novac/2026-08-30-lex-k2.md"
    printf '| 1 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30 (novac). Детали. |\n' \
        > "$T/docs/plans/221.1-bug-sweep.md"
}

mk_root
if ! sh "$GUARD" "$T" >"$T.o0" 2>&1; then
    echo "FAIL: здоровый макет красный — страж ложнит:" >&2
    tail -2 "$T.o0" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 1: отчёт-молчун (ни находок, ни «НИЧЕГО НЕ НАШЁЛ») ─────────
mk_root
printf '# Охота\nПосмотрел по сторонам.\n' > "$T/docs/dev/hunts/novac/2026-08-30-lex-k2.md"
if sh "$GUARD" "$T" >"$T.o1" 2>&1; then
    echo "FAIL: отчёт-молчун прошёл стража — молчание прочитано как успех (№770)" >&2
    rc=1
fi

# ── подделка 2: метка без трека после даты ──────────────────────────────
mk_root
printf '| 1 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30. Детали. |\n' > "$T/docs/plans/221.1-bug-sweep.md"
if sh "$GUARD" "$T" >"$T.o2" 2>&1; then
    echo "FAIL: метка без «(novac)»/«(oracle)» прошла — треки не разделяются" >&2
    rc=1
fi

# ── подделка 3: меток трека больше, чем находок (метка руками) ──────────
mk_root
printf '| 1 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30 (novac). |\n| 2 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30 (novac). |\n' \
    > "$T/docs/plans/221.1-bug-sweep.md"
if sh "$GUARD" "$T" >"$T.o3" 2>&1; then
    echo "FAIL: две метки при одной находке прошли — метку можно ставить руками" >&2
    rc=1
fi

# ── подделка 4: бумажная охота — отчёт без каталога проб ────────────────
mk_root
rm -rf "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2"
if sh "$GUARD" "$T" >"$T.o4" 2>&1; then
    echo "FAIL: отчёт без probes/<stem>/ прошёл — бумажная охота двигает часы бесплатно" >&2
    rc=1
fi

# ── подделка 5: проб меньше трёх ────────────────────────────────────────
mk_root
rm -rf "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/y" \
       "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/z"
if sh "$GUARD" "$T" >"$T.o5" 2>&1; then
    echo "FAIL: одна проба на охоту прошла — барьер трёх проб не держит" >&2
    rc=1
fi

# ── подделка 6: находка цитирует несуществующую пробу ───────────────────
mk_root
rm -rf "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/x"
mkdir -p "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/w"
if sh "$GUARD" "$T" >"$T.o6" 2>&1; then
    echo "FAIL: находка с несуществующей пробой прошла — цитата не проверяется" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-hunter-mark ok: шесть подделок покраснели, здоровый макет и живая половина зелёные"
exit "$rc"
