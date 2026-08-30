#!/bin/sh
# scripts/guards/check-examples-strict-effects.sh — план 221 A-E2:
# КАЖДЫЙ пример вне `_wip/` собирается текущим тулчейном под `--strict-effects`.
#
# ЗАЧЕМ. Примеры — лицо языка: их читает внешний человек раньше, чем спеку.
# До этого стража гейт судил ШЕСТЬ названных целей из списка
# `flagship-targets.txt`, а точек входа в `examples/` — 31 (замер 2026-08-30).
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
N=0
FAILED=0
while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    grep -qxF "$rel" "$TMP/exc" && continue
    N=$((N + 1))
    if ! timeout 180 "$NOVA" build "$EX_DIR/$rel" --strict-effects > "$TMP/log" 2>&1; then
        FAILED=$((FAILED + 1))
        echo "check-examples-strict-effects: FAIL — $rel не собирается под --strict-effects:" >&2
        grep -m2 -aE 'error' "$TMP/log" | cut -c1-140 | sed 's/^/    /' >&2
        rc=1
    fi
done < "$TMP/entries"

if [ "$rc" -eq 0 ]; then
    NEXC=$(grep -c . "$TMP/exc" 2>/dev/null || echo 0)
    echo "check-examples-strict-effects ok: точек входа вне _wip $N, все собираются под --strict-effects; исключений с причиной $NEXC (сниппетов с закомментированной main: $(grep -c . "$TMP/snippets" 2>/dev/null || echo 0))"
fi
exit "$rc"
