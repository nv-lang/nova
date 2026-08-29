#!/bin/sh
# selftest/test-check-hunter-coverage.sh — страж сетки умеет краснеть.
# Четыре подделки — по одной на каждую ложь карте — и живая половина.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-hunter-coverage.py"
T="${TMPDIR:-/tmp}/selftest-hunter-grid.$$"
mkdir -p "$T/hunts"
trap 'rm -rf "$T"' 0 2 15
rc=0

# Живая половина: настоящий корень обязан быть зелёным.
if ! python "$GUARD" "$ROOT" >"$T/live" 2>&1; then
    echo "FAIL: живая половина красная:" >&2; tail -2 "$T/live" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 1: отчёт без строки КЛЕТКА ─────────────────────────────────
printf '# hunt\nsomething\n' > "$T/hunts/2026-08-30-x.md"
if NOVA_HUNTS_DIR="$T/hunts" python "$GUARD" "$ROOT" >"$T/o1" 2>&1; then
    echo "FAIL: отчёт без КЛЕТКА прошёл — непонятая форма промолчана (№801)" >&2
    rc=1
fi

# ── подделка 2: модуль мимо таблицы рёбер ───────────────────────────────
printf 'КЛЕТКА | nosuchmod | К1\nНАХОДКА | x\n' > "$T/hunts/2026-08-30-x.md"
if NOVA_HUNTS_DIR="$T/hunts" python "$GUARD" "$ROOT" >"$T/o2" 2>&1; then
    echo "FAIL: клетка с чужим модулем прошла — охота мимо карты не поймана" >&2
    rc=1
fi

# ── подделка 3: база ниже факта (рост неохваченного) ────────────────────
rm -f "$T/hunts/2026-08-30-x.md"
printf 'never_hunted=1\n' > "$T/low.baseline"
if NOVA_HUNTER_GRID_BASELINE="$T/low.baseline" python "$GUARD" "$ROOT" >"$T/o3" 2>&1; then
    echo "FAIL: рост над базой прошёл — храповик не держит" >&2
    rc=1
fi

# ── подделка 4: метка даты в имени не спасает пустой каталог с мусором ──
mkdir -p "$T/hunts2"; printf 'not md content' > "$T/hunts2/grid.md"
if NOVA_HUNTS_DIR="$T/hunts2" python "$GUARD" "$ROOT" >"$T/o4" 2>&1; then
    echo "FAIL: md-файл без КЛЕТКА в каталоге охот прошёл" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-hunter-coverage ok: четыре подделки покраснели, живая половина зелёная"
exit "$rc"
