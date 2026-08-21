#!/usr/bin/env bash
# Страж: число чтений карты типов чекера ГОЛЫМ именем не растёт.
#
# ЗАЧЕМ. `TypeCheckCtx.types` — `HashMap<String, TypeDecl>` по ПРОСТОМУ имени,
# last-write-wins между модулями слитого CU. Это источник истины чекера: всё,
# что строится поверх, наследует его ошибку. Два модуля с одноимённым типом —
# и `types.get("Kind")` возвращает ЧУЖОЙ `Kind`.
#
# КЛАСС ВОЗВРАЩАЛСЯ ЧЕТЫРЕЖДЫ, каждый раз другой гранью:
#   196.7  — диспетч метода через single-key `method_receivers`, last-wins;
#   №696   — таблицы КОДОГЕНА (`method_receivers`, `sum_schemas`);
#   №705   — таблица типов ЧЕКЕРА, то есть корень;
#   №729   — попытка отказать на несуществующем статике 2026-08-18 дала ДВА
#            ложных отказа (`Kind.Info(5)`, `Node.Leaf(7)`) ровно об эту
#            коллизию и была откачена целиком.
#
# ПОЧЕМУ ХРАПОВИК, А НЕ НОЛЬ. Снять все 72 разом нельзя: реестр назначил это
# ОДНИМ окном (W6, канал чекера плана 196) и ЗАПРЕТИЛ точечные патчи — их уже
# было три, и каждый закрывал одну грань, оставляя корень. Пока окно не
# пришло, число обязано только убывать: новый голый читатель — новая грань.
#
# ЧТО НЕ СЧИТАЕТСЯ ГОЛЫМ. `types_get_for_file(name, use_file_id)` — разрешение
# по файлу МЕСТА ИСПОЛЬЗОВАНИЯ с откатом на глобальную карту. Это и есть
# целевая форма; материал для неё уже построен (`file_local_types` хранит
# каждое одноимённое объявление под своим file_id).
#
# ПОВЕРХНОСТЬ ИЗМЕРЕНА, и это меняет цену окна: `NOVA_TYPE_COLLISION_REPORT`
# на conformance даёт 1073 CU, коллизии в 39 (3.6%), максимум два имени на
# модуль, а различных сталкивающихся имён во всём корпусе ШЕСТЬ. Шесть можно
# перебрать руками до и после — фикс проверяется исчерпывающе, а не выборочно.
#
# $1 — корень. База — scripts/guards/bare-type-lookups.baseline
# Самотест — selftest/test-check-bare-type-lookups.sh
#
# План docs/plans/231-bug-cycle-exit.md (дисциплина механизмов принуждения).

set -u
export LC_ALL=C

NAME="check-bare-type-lookups"
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
CORE="$(dirname "$0")/bare-type-lookup-scan.py"
BASELINE="${NOVA_BARE_TYPE_BASELINE:-$ROOT/scripts/guards/bare-type-lookups.baseline}"

[ -f "$CORE" ] || { echo "$NAME: FAIL — нет ядра $CORE" >&2; exit 1; }
[ -f "$BASELINE" ] || { echo "$NAME: FAIL — нет базы $BASELINE" >&2; exit 1; }

OUT=$(python "$CORE" "$ROOT" 2>&1)
if [ $? -ne 0 ]; then
    echo "$NAME: FAIL — ядро не отработало:" >&2
    printf '%s\n' "$OUT" | tail -5 >&2
    exit 1
fi

NOW=$(printf '%s\n' "$OUT" | sed -n 's/^bare=\(-\{0,1\}[0-9][0-9]*\)$/\1/p' | tail -1)
case "$NOW" in
    ''|*[!0-9-]*) echo "$NAME: FAIL — ядро не вернуло число" >&2; exit 1;;
esac
if [ "$NOW" -lt 0 ]; then
    echo "$NAME: FAIL — ядро не нашло целевой файл (переехал?)" >&2
    exit 1
fi

BASE=$(sed -n 's/^bare=\([0-9][0-9]*\)$/\1/p' "$BASELINE" | tail -1)
case "$BASE" in
    ''|*[!0-9]*) echo "$NAME: FAIL — база без числа: $BASELINE" >&2; exit 1;;
esac

if [ "$NOW" -gt "$BASE" ]; then
    echo "$NAME: FAIL — голых чтений карты типов стало больше: $NOW (база $BASE)" >&2
    printf '%s\n' "$OUT" | grep -v '^bare=\|^routed=' | tail -8 >&2
    echo "    Карта ключуется ГОЛЫМ именем и коллидирует между модулями" >&2
    echo "    last-write-wins — новый такой читатель заводит новую грань" >&2
    echo "    класса, который возвращался четырежды (196.7, №696, №705, №729)." >&2
    echo "    Разрешай по файлу места использования: types_get_for_file(name, id)." >&2
    exit 1
fi

if [ "$NOW" -lt "$BASE" ]; then
    echo "$NAME ok: голых чтений $NOW (база $BASE) — УБЫЛО, опусти базу с летописью"
    exit 0
fi

echo "$NAME ok: голых чтений карты типов не прибавилось ($NOW <= $BASE)"
exit 0
