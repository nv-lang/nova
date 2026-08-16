#!/bin/sh
# scripts/guards/check-novac-ref-field-names.sh — поле-ссылка называет
# пространство, куда ссылается (конвенция П19; вопрос владельца 2026-08-16
# «owner int — почему не owner_id? насколько имена в едином ключе?»).
#
# ЗАЧЕМ. Пока типизированных индексов нет (задача волны 3 плана 274.3), все
# ссылки — голый `int`, и единственный носитель пространства — ИМЯ. До
# правила `type_id` нёс суффикс, а `owner`, `recv`, `ret`, `sum` были голыми,
# хотя хранили ровно то же самое — id типа. Читающий обязан был помнить, что
# есть что, а это ровно тот класс, который в компиляторе кончается путаницей
# id и индекса строки.
#
# ПРОВЕРЯЕТ в novac/src/sem/*.nv (единственное место, где живут реестры):
#   каждое поле типа `int` внутри блока `type ... {` обязано кончаться на
#   `_id` (id сущности из реестра), `_row` (индекс строки), `_off`/`_cnt`
#   (диапазон строк) или `_len`. Единственное легальное голое имя —
#   `payload`: его смысл полиморфен (зависит от `kind`), и суффикс тут
#   соврал бы. Исключение зашито ОДНО и названо в сообщении.
# НЕ ПРОВЕРЯЕТ: смещения в тексте (`lo`/`hi`, `start`/`end` в source/lex/tree)
#   — они ни на что не ссылаются; поля не-int; локальные переменные и
#   параметры (правило про ХРАНИМУЮ ссылку, а не про всякое имя);
#   содержательность суффикса (что `_id` не оказался на самом деле строкой —
#   это приёмка и, позже, типизированные индексы).
#
# $1 — корень репозитория; $2 — override сканируемой директории (самотест).
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src/sem}"
NAME=check-novac-ref-field-names

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

NF=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | wc -l | tr -d '[:space:]')
if [ "$NF" -eq 0 ]; then
    echo "$NAME ok: судить нечего (в $SRC файлов .nv: 0)"
    exit 0
fi

OUT=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    tr -d '\r' < "$f" | awk -v rel="$rel" '
        /^(export )?type [A-Za-z_]/ { inb = ($0 ~ /\{[[:space:]]*$/); next }
        inb && /^\}/ { inb = 0; next }
        inb {
            line = $0
            sub(/\/\/.*$/, "", line)
            gsub(/^[[:space:]]+/, "", line)
            gsub(/[[:space:]]+$/, "", line)
            sub(/,$/, "", line)
            if (line == "") next
            n = split(line, p, /[[:space:]]+/)
            if (n < 2) next
            fname = p[1]; ftype = p[2]
            if (ftype != "int") next
            total++
            if (fname == "payload") { exempt++; next }
            if (fname ~ /_(id|row|off|cnt|len)$/) { good++; next }
            printf "  %s:%d: поле `%s int` без суффикса пространства\n", rel, NR, fname
        }
        END { printf "@@ %d %d %d\n", total, good, exempt }
    '
done)

BAD=$(printf '%s\n' "$OUT" | grep -v '^@@' | grep -v '^$')
TOTAL=$(printf '%s\n' "$OUT" | awk '/^@@/ { t += $2; g += $3; e += $4 } END { printf "%d %d %d", t, g, e }')

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — поле-ссылка не называет пространство (конвенция П19):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  '_id' — id сущности из реестра (id типа); '_row' — индекс строки в векторе;" >&2
    echo "  '_off'/'_cnt' — диапазон строк. Голое имя легально одно — 'payload'," >&2
    echo "  и только потому, что его смысл зависит от 'kind' (это сказано в его доке)." >&2
    exit 1
fi

set -- $TOTAL
echo "$NAME ok: полей-ссылок int в реестрах: $1 (с суффиксом $2, полиморфных $3), безымянных пространств: 0"
exit 0
