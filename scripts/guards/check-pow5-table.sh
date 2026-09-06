#!/usr/bin/env bash
# scripts/guards/check-pow5-table.sh — таблица степеней пяти парсера float
# пересчитывается точными целыми и сверяется побитно на каждом прогоне.
#
# ДОМ И ОСНОВАНИЕ: план docs/plans/283-float-parse-own-rounding.md, Ф.1
# («правка таблицы без пересчёта краснеет»).
#
# ЗАЧЕМ. `std/src/runtime/string/pow5_table.nv` — 651 строка по два 64-битных
# слова, 1302 шестнадцатеричных литерала. Одна неверная цифра даёт парсер,
# верный на всех входах, кроме тех, чей десятичный порядок попадает в эту
# строку, — и конечный дифференциальный корпус её не обязан задеть. Поэтому
# таблицей владеет скрипт `scripts/tools/gen-pow5-table.py`: он её пишет
# (--write) и пересчитывает (--check); этот страж — внешний вызывающий второго.
#
# ЧТО ПРОВЕРЯЕТСЯ: файл таблицы есть (нет файла — КРАСНОЕ, не «нарушений 0»);
# константы POW5_SMALLEST_Q / POW5_LARGEST_Q / POW5_ROWS, объявленные длины и
# число строк обоих массивов, каждое слово hi и lo совпадают с точным
# пересчётом 5^q. Расхождение называет q и половину (hi / lo).
#
# ЧЕГО НЕ ЛОВИТ: правдивость самого алгоритма пересчёта — она доказана один раз
# сверкой с донорской таблицей Rust (`--against-rust`, 651/651), а не на
# каждом прогоне: донорского файла в дереве нет.
#
# ИСПОЛЬЗОВАНИЕ: bash scripts/guards/check-pow5-table.sh [КОРЕНЬ]
# Коды: 0 — совпало; 1 — расхождение, нет таблицы или нет генератора.
# Самотест — scripts/guards/selftest/test-check-pow5-table.sh
set -u
export LC_ALL=C

NAME="check-pow5-table"
ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
GEN="$ROOT/scripts/tools/gen-pow5-table.py"
TABLE="$ROOT/std/src/runtime/string/pow5_table.nv"

if [ ! -f "$GEN" ]; then
    echo "$NAME: FAIL — нет генератора $GEN: судить нечем, а нечем != зелено" >&2
    exit 1
fi
if [ ! -f "$TABLE" ]; then
    echo "$NAME: FAIL — нет таблицы $TABLE: мишень потеряна, это красное, а не ноль нарушений" >&2
    exit 1
fi

out=$(python "$GEN" --check "$ROOT" 2>&1); rc=$?
printf '%s\n' "$out"
if [ "$rc" -ne 0 ]; then
    echo "$NAME: FAIL — таблица расходится с точным пересчётом (код $rc)." >&2
    echo "  Таблицу руками не правят: python scripts/tools/gen-pow5-table.py --write" >&2
    exit 1
fi
case "$out" in
    *"ok: "*) ;;
    *)  echo "$NAME: FAIL — генератор вышел с нулём, но не предъявил строку ok:" >&2
        exit 1 ;;
esac
echo "$NAME ok: таблица степеней пяти совпала с точным пересчётом побитно (hi и lo, все строки)"
exit 0
