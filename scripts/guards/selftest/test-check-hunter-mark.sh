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
    printf 'proba x\n' > "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/x/p.nv"
    printf 'proba y\n' > "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/y/p.nv"
    printf 'proba z\n' > "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/z/p.nv"
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
# (x остаётся — одна проба вместо трёх)
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

# ── подделка 7: три ПУСТЫХ файла-пробы (touch) ──────────────────────────
mk_root
: > "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/x/p.nv"
: > "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/y/p.nv"
: > "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/z/p.nv"
if sh "$GUARD" "$T" >"$T.o7" 2>&1; then
    echo "FAIL: три ПУСТЫХ файла-пробы прошли барьер — бумажная охота стоит секунды" >&2
    rc=1
fi

# ── подделка 8: три пробы с ОДИНАКОВЫМ содержимым ───────────────────────
mk_root
for d in x y z; do printf 'same\n' > "$T/docs/dev/hunts/novac/probes/2026-08-30-lex-k2/$d/p.nv"; done
if sh "$GUARD" "$T" >"$T.o8" 2>&1; then
    echo "FAIL: три копии одной пробы прошли барьер — различность не проверяется" >&2
    rc=1
fi

# ── подделка 9: находка без 4-го поля (не цитирует пробу) ───────────────
mk_root
printf 'КЛЕТКА | lex | К2\nНАХОДКА | К2 | lex | детали без пробы\n' \
    > "$T/docs/dev/hunts/novac/2026-08-30-lex-k2.md"
if sh "$GUARD" "$T" >"$T.o9" 2>&1; then
    echo "FAIL: находка без поля пробы прошла — «каждая находка цитирует пробу» не принуждается" >&2
    rc=1
fi

# ── подделка 10: цитата «.» и путь наружу ───────────────────────────────
mk_root
printf 'КЛЕТКА | lex | К2\nНАХОДКА | К2 | lex | . | детали\n' \
    > "$T/docs/dev/hunts/novac/2026-08-30-lex-k2.md"
if sh "$GUARD" "$T" >"$T.o10" 2>&1; then
    echo "FAIL: цитата «.» (сам каталог) прошла — проба подменена каталогом" >&2
    rc=1
fi
mk_root
printf 'КЛЕТКА | lex | К2\nНАХОДКА | К2 | lex | ../2026-08-30-lex-k2/x | детали\n' \
    > "$T/docs/dev/hunts/novac/2026-08-30-lex-k2.md"
if sh "$GUARD" "$T" >"$T.o10b" 2>&1; then
    echo "FAIL: цитата путём наружу прошла — можно занять пробу соседней охоты" >&2
    rc=1
fi

# ── подделка 11: имя отчёта без датного префикса ────────────────────────
mk_root
mv "$T/docs/dev/hunts/novac/2026-08-30-lex-k2.md" "$T/docs/dev/hunts/novac/hunt-lex-k2.md"
if sh "$GUARD" "$T" >"$T.o11" 2>&1; then
    echo "FAIL: отчёт без датного имени прошёл — часы долга его не увидят, и честная охота не погасит долг" >&2
    rc=1
fi

# ── подделка 12: цитата метки соседа в прозе НЕ считается меткой ────────
mk_root
printf '| 810 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30 (novac). Детали. |\n| 811 | t | Дубль 810, та строка несёт метку `НАЙДЕНО ОХОТНИКОМ 2026-08-30 (novac)`. |\n' \
    > "$T/docs/plans/221.1-bug-sweep.md"
if ! sh "$GUARD" "$T" >"$T.o12" 2>&1; then
    echo "FAIL: строка-дубль, ЦИТИРУЮЩАЯ метку соседа В БЭКТИКАХ, посчитана второй меткой — ложный красный на честной работе:" >&2
    tail -2 "$T.o12" | sed 's/^/    /' >&2
    rc=1
fi

# ── и обратная сторона: цитата БЕЗ бэктиков обязана считаться меткой ────
mk_root
printf '| 810 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30 (novac). |\n| 811 | t | как у 810: НАЙДЕНО ОХОТНИКОМ 2026-08-30 (novac). |\n' \
    > "$T/docs/plans/221.1-bug-sweep.md"
if sh "$GUARD" "$T" >"$T.o12b" 2>&1; then
    echo "FAIL: две неоформленные метки при одной находке прошли — освобождение цитат стало дырой" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-hunter-mark ok: одиннадцать подделок покраснели, цитата-в-прозе не ложнит, здоровый макет и живая половина зелёные"
exit "$rc"
