#!/bin/sh
# selftest/test-check-hunter-coverage.sh — страж сетки умеет краснеть.
# Четыре подделки — по одной на каждую ложь карте — плюс живая половина и
# здоровая многоклеточная охота со свёрнутой клеткой в леджере (Ф.6).
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

# Здоровье: многоклеточный отчёт + клетка из леджера считаются ОБЕ (findall).
printf 'КЛЕТКА | parse | К1\nКЛЕТКА | lex | К2\nНАХОДКА | x\n' > "$T/hunts/2026-08-30-multi.md"
printf 'СВЁРНУТО | 2026-01-01-check-k1 | check | К1 | находок 1 | №1\n' > "$T/hunts/LEDGER.md"
printf 'never_hunted=95\n' > "$T/ok.baseline"
if ! NOVA_HUNTS_DIR="$T/hunts" NOVA_HUNTER_GRID_BASELINE="$T/ok.baseline" python "$GUARD" "$ROOT" >"$T/o0" 2>&1; then
    echo "FAIL: многоклеточный отчёт + леджер красные — findall или леджер не работают:" >&2
    tail -2 "$T/o0" | sed 's/^/    /' >&2
    rc=1
elif ! grep -q "охочено 3" "$T/o0"; then
    echo "FAIL: две клетки отчёта + одна из леджера не дали «охочено 3»:" >&2
    tail -1 "$T/o0" | sed 's/^/    /' >&2
    rc=1
fi
rm -f "$T/hunts/LEDGER.md"

# ── подделка 1: отчёт без строки КЛЕТКА ─────────────────────────────────
rm -f "$T/hunts/2026-08-30-multi.md"
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

# ── подделка 4: md-файл без КЛЕТКА в каталоге охот ──────────────────────
mkdir -p "$T/hunts2"; printf 'not md content' > "$T/hunts2/grid.md"
if NOVA_HUNTS_DIR="$T/hunts2" python "$GUARD" "$ROOT" >"$T/o4" 2>&1; then
    echo "FAIL: md-файл без КЛЕТКА в каталоге охот прошёл" >&2
    rc=1
fi

# ── здоровье 2: модуль СВЁРНУТОЙ охоты выбыл из §3 — не красный ─────────
# Леджер — запись о прошлом, ось модулей живёт: честное переименование модуля
# краснило стража навсегда на замороженной истории (панель 2026-08-30).
rm -f "$T/hunts2/grid.md"
printf 'СВЁРНУТО | 2026-01-01-oldmod-k1 | oldmod | К1 | находок 1 | №810\n' > "$T/hunts2/LEDGER.md"
printf 'never_hunted=98\n' > "$T/hi.baseline"
if ! NOVA_HUNTS_DIR="$T/hunts2" NOVA_HUNTER_GRID_BASELINE="$T/hi.baseline" python "$GUARD" "$ROOT" >"$T/o5" 2>&1; then
    echo "FAIL: свёрнутая охота по выбывшему модулю краснит — история замерла, а ось живёт:" >&2
    tail -2 "$T/o5" | sed 's/^/    /' >&2
    rc=1
fi
# но ОТКРЫТЫЙ отчёт с чужим модулем обязан краснеть по-прежнему
printf 'КЛЕТКА | oldmod | К1\nНАХОДКА | x\n' > "$T/hunts2/2026-08-30-open.md"
if NOVA_HUNTS_DIR="$T/hunts2" NOVA_HUNTER_GRID_BASELINE="$T/hi.baseline" python "$GUARD" "$ROOT" >"$T/o6" 2>&1; then
    echo "FAIL: ОТКРЫТЫЙ отчёт с модулем мимо §3 прошёл — послабление леджеру утекло на отчёты" >&2
    rc=1
fi
rm -f "$T/hunts2/LEDGER.md" "$T/hunts2/2026-08-30-open.md"

# ── подделка 5: в архитектуре нет «## 3.» — своё сообщение, не трейсбек ──
mkdir -p "$T/fakeroot/docs/dev"
printf '# novac\n## 2. Nothing\ntext\n' > "$T/fakeroot/docs/dev/novac-architecture.md"
if python "$GUARD" "$T/fakeroot" >"$T/o7" 2>&1; then
    echo "FAIL: архитектура без «## 3.» прошла" >&2
    rc=1
elif grep -q "Traceback" "$T/o7"; then
    echo "FAIL: страж умер трейсбеком вместо своего сообщения — «отказ на непонятой форме» обязан ОБЪЯСНЯТЬ:" >&2
    tail -3 "$T/o7" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 6: ось разошлась с modules_at_seed (№816) ──────────────────
# Живое дерево, честное число неохваченных — и всё равно красный: база
# двигалась под ДРУГУЮ ось (у 274 таких модулей 15, у main 14), и молчать
# об этом нельзя, иначе расхождение читается как ошибка числа.
printf 'never_hunted=95\nmodules_at_seed=99\n' > "$T/wrongaxis.baseline"
if NOVA_HUNTER_GRID_BASELINE="$T/wrongaxis.baseline" python "$GUARD" "$ROOT" >"$T/o8" 2>&1; then
    echo "FAIL: база с чужим modules_at_seed прошла — ветко-зависимость оси снова молчит (№816)" >&2
    rc=1
elif ! grep -q "modules_at_seed" "$T/o8"; then
    echo "FAIL: красный есть, но сообщение не называет ключ — окно опять будет гадать:" >&2
    tail -2 "$T/o8" | sed 's/^/    /' >&2
    rc=1
fi

# ── и обратная сторона: база БЕЗ ключа обязана остаться зелёной ─────────
# Старые базы ключа не несут, и страж не смеет краснеть на них задним числом.
printf 'never_hunted=95\n' > "$T/nokey.baseline"
if ! NOVA_HUNTER_GRID_BASELINE="$T/nokey.baseline" python "$GUARD" "$ROOT" >"$T/o9" 2>&1; then
    echo "FAIL: база без modules_at_seed покраснела — новый ключ стал обязательным задним числом:" >&2
    tail -2 "$T/o9" | sed 's/^/    /' >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-hunter-coverage ok: шесть подделок покраснели, многоклеточность, леджер, замороженная история и база без нового ключа зелёные, живая половина зелёная"
exit "$rc"
