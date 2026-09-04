#!/bin/sh
# scripts/guards/check-examples-strict-effects.sh — план 221 A-E2:
# КАЖДЫЙ пример вне `_wip/` собирается текущим тулчейном под `--strict-effects`.
#
# ЗАЧЕМ. Примеры — лицо языка: их читает внешний человек раньше, чем спеку.
# До этого стража гейт судил ШЕСТЬ названных целей из списка
# `flagship-targets.txt`, а точек входа в `examples/` — 66 (замер 2026-09-04; было 31 на 2026-08-30 — число УДВОИЛОСЬ за пять дней, и это есть причина, по которой шаг вышел за свой предел).
# То есть проверялась шестая часть, и «примеры собираются» держалось на честном
# слове про остальные двадцать пять.
#
# ЧТО СЧИТАЕТСЯ ПРИМЕРОМ (а не сниппетом): файл вне `_wip/` с НЕЗАКОММЕНТИРОВАННОЙ
# `fn main(`. Различение не педантизм, а урок №573: `real_world/orm_decorators.nv`
# имеет `fn main`, закомментированную на строке 170, и потому НИ РАЗУ не проходил
# кодоген — «выглядел проверяемым и проверялся не тем». Такой файл обязан либо
# получить точку входа, либо стоять в списке исключений С ПРИЧИНОЙ.
#
# СПИСОК ИСКЛЮЧЕНИЙ — ХРАПОВИК: `examples-strict-exceptions.list`, по строке
# «<путь> # <причина>». Растёт только с правкой файла и объяснением; сокращается
# свободно. Пустая строка исключения = тихо отключённая проверка.
#
# ЯРУС: `full`. Цена — 31 сборка, около пяти минут; в `loop` ей не место
# (Г1: ярус, переставший быть дешёвым, перестаёт зваться).
#
# Самотест: scripts/guards/selftest/test-check-examples-strict-effects.sh
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
EX_DIR="${NOVA_EXAMPLES_DIR:-$ROOT/examples}"
EX_LIST="${NOVA_EXAMPLES_EXCEPTIONS:-$(cd "$(dirname "$0")" && pwd)/examples-strict-exceptions.list}"

NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -x "$NOVA" ] || NOVA="$ROOT/nova-cli/target/release/nova"
[ -x "$NOVA" ] || {
    echo "check-examples-strict-effects: FAIL — нет бинаря $NOVA. Шаг обязан стоять ПОСЛЕ cargo build (реестр №813)." >&2
    exit 1
}
[ -d "$EX_DIR" ] || {
    echo "check-examples-strict-effects: FAIL — нет каталога примеров $EX_DIR" >&2
    exit 1
}

TMP="${TMPDIR:-/tmp}/examples-strict.$$"
mkdir -p "$TMP"
trap 'rm -rf "$TMP"' 0 2 15

# ── исключения: путь + обязательная причина ──────────────────────────────
: > "$TMP/exc"
if [ -f "$EX_LIST" ]; then
    while IFS= read -r line; do
        case "$line" in ''|\#*) continue ;; esac
        path=${line%%#*}
        why=${line#*#}
        path=$(printf '%s' "$path" | sed 's/[[:space:]]*$//')
        why=$(printf '%s' "$why" | sed 's/^[[:space:]]*//')
        if [ -z "$why" ] || [ "$why" = "$line" ]; then
            echo "check-examples-strict-effects: FAIL — исключение «$path» без причины после «#»: исключение без объяснения есть тихо снятая проверка" >&2
            exit 1
        fi
        printf '%s\n' "$path" >> "$TMP/exc"
    done < "$EX_LIST"
fi

# ── точки входа: НЕзакомментированная `fn main(` вне _wip ────────────────
find "$EX_DIR" -name '*.nv' -type f 2>/dev/null | tr -d '\r' | sed 's#\\#/#g' | sort > "$TMP/all"
: > "$TMP/entries"
: > "$TMP/snippets"
while IFS= read -r f; do
    [ -n "$f" ] || continue
    case "$f" in */_wip/*) continue ;; esac
    rel=${f#"$EX_DIR"/}
    if grep -qE '^[[:space:]]*fn[[:space:]]+main[[:space:]]*\(' "$f"; then
        printf '%s\n' "$rel" >> "$TMP/entries"
    elif grep -qE '^[[:space:]]*//[[:space:]]*fn[[:space:]]+main[[:space:]]*\(' "$f"; then
        printf '%s\n' "$rel" >> "$TMP/snippets"
    fi
done < "$TMP/all"

rc=0

# ── закомментированная точка входа обязана быть НАЗВАНА исключением ──────
while IFS= read -r s; do
    [ -n "$s" ] || continue
    if ! grep -qxF "$s" "$TMP/exc"; then
        echo "check-examples-strict-effects: FAIL — у примера «$s» точка входа ЗАКОММЕНТИРОВАНА, и он не назван исключением." >&2
        echo "    Такой файл выглядит проверяемым и не проверяется ничем (урок №573). Либо верни «fn main», либо впиши его в ${EX_LIST##*/} с причиной." >&2
        rc=1
    fi
done < "$TMP/snippets"

# ── сборка каждой точки входа под --strict-effects ──────────────────────
# ПАРАЛЛЕЛЬНО, И ЭТО НЕ ОПТИМИЗАЦИЯ РАДИ КРАСОТЫ. Последовательный обход
# 31 точки входа упирался в предел шага (600с) и УБИВАЛСЯ — то есть шаг
# регулярно не доказывал НИЧЕГО. Замер 2026-09-04: убит в двух полных прогонах
# подряд, стартуя и с 747с, и с 1080с (значит дело не в соперничестве за
# машину), а отдельный запуск на свободной машине шёл больше девяти минут.
# Конвенция гейта на этот счёт прямая: «ярус, который перестал быть дешёвым,
# перестаёт зваться — чини цену шага, а не число», поэтому предел не поднят.
#
# Сборки НЕЗАВИСИМЫ: разные входные файлы, и каждая получает СВОЙ `-o` во
# временном каталоге. Отдельное имя выхода здесь не аккуратность, а условие
# безопасности параллели: без него две сборки могли бы писать в одно место.
# Попутно шаг перестаёт оставлять артефакты рядом с примерами.
#
# Порядок вывода ДЕТЕРМИНИРОВАН: отказы пишутся отдельными файлами и читаются
# отсортированными после `wait`. Страж, чей вывод зависит от того, кто первым
# добежал, нельзя сравнить с прошлым прогоном — а сравнивают их постоянно.
# Потолок 4, а не «сколько ядер». Замер 2026-09-04: при 6 параллельных сборках
# и вторым гейтом на той же машине ЧЕТЫРЕ соседних примера упали разом, а
# поодиночке собираются с rc=0 — их убил внутренний `timeout`, а не ошибка.
# Ложный отказ стража дороже медленного шага: страж, который иногда врёт,
# перестают читать. При 4 шаг всё равно втрое быстрее последовательного.
JOBS=$(nproc 2>/dev/null || echo 4)
[ "$JOBS" -gt 4 ] && JOBS=4
[ "$JOBS" -lt 1 ] && JOBS=1
mkdir -p "$TMP/res"
N=0
running=0
while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    grep -qxF "$rel" "$TMP/exc" && continue
    N=$((N + 1))
    slot=$(printf '%04d' "$N")
    printf '%s\n' "$rel" > "$TMP/res/rel_$slot"
    (
        # 300с, а не 180: под параллелью каждая сборка идёт дольше, а предел,
        # срабатывающий от НАГРУЗКИ, превращает страж в генератор ложных
        # отказов. Код 124 — это таймаут `timeout(1)`, и он записывается
        # ОТДЕЛЬНО: «не успел» и «не собирается» — разные утверждения, и
        # смешивать их значит врать о причине в самом громком месте гейта.
        timeout 300 "$NOVA" build "$EX_DIR/$rel" --strict-effects \
            -o "$TMP/res/bin_$slot" > "$TMP/res/log_$slot" 2>&1
        _rc=$?
        if [ "$_rc" -eq 124 ]; then
            : > "$TMP/res/slow_$slot"
        elif [ "$_rc" -ne 0 ]; then
            : > "$TMP/res/fail_$slot"
        fi
    ) &
    running=$((running + 1))
    if [ "$running" -ge "$JOBS" ]; then
        wait
        running=0
    fi
done < "$TMP/entries"
wait

FAILED=0
for f in $(ls "$TMP/res" 2>/dev/null | grep '^fail_' | sort); do
    slot="${f#fail_}"
    rel=$(cat "$TMP/res/rel_$slot")
    FAILED=$((FAILED + 1))
    echo "check-examples-strict-effects: FAIL - $rel: --strict-effects build failed:"
    grep -m2 -aE 'error' "$TMP/res/log_$slot" | cut -c1-140 | sed 's/^/    /' >&2
    rc=1
done
# Таймаут — ТОЖЕ отказ (шаг ничего не доказал про этот пример), но названный
# своим именем: читающий лог не должен искать ошибку компиляции там, где её нет.
for f in $(ls "$TMP/res" 2>/dev/null | grep '^slow_' | sort); do
    slot="${f#slow_}"
    rel=$(cat "$TMP/res/rel_$slot")
    FAILED=$((FAILED + 1))
    echo "check-examples-strict-effects: FAIL - $rel: build TIMED OUT after 300s"
    echo "    Это не ошибка компиляции: сборка не успела. Причина обычно" >&2
    echo "    внешняя (вторая тяжёлая работа на машине) — перемерь на свободной," >&2
    echo "    и только если повторится, ищи причину в самом примере." >&2
    rc=1
done

if [ "$rc" -eq 0 ]; then
    NEXC=$(grep -c . "$TMP/exc" 2>/dev/null || echo 0)
    echo "check-examples-strict-effects ok: точек входа вне _wip $N, все собираются под --strict-effects; исключений с причиной $NEXC (сниппетов с закомментированной main: $(grep -c . "$TMP/snippets" 2>/dev/null || echo 0))"
fi
exit "$rc"
