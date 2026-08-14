#!/bin/sh
# scripts/guards/check-novac-type-field-docs.sh — каждый тип и каждое поле
# novac несут комментарий (конвенция П13, требование владельца 2026-08-14).
#
# ПРОВЕРЯЕТ awk-ом по novac/src/**/*.nv:
#   * строка `type X`/`export type X` — комментарий НА строке (`//`) или
#     строкой выше (`^\s*//`);
#   * внутри блока `type ... {` каждая строка-поле (`имя тип`) —
#     комментарий на строке или строкой выше.
# НЕ ПРОВЕРЯЕТ: содержательность комментария (приёмка; заглушка хуже
# отсутствия); enum-варианты `| X` (расширение по прецеденту); многострочные
# декларации полей (канон — одна строка).
#
# $1 — корень репозитория; $2 — override сканируемой директории (самотест).
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-type-field-docs

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

BAD=$(find "$SRC" -type f -name '*.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    awk -v rel="$rel" '
        { line = $0; sub(/\r$/, "", line) }
        # remember whether the PREVIOUS line was a pure comment; attribute
        # lines (#impl(...) etc.) sit between the doc and the type and must
        # NOT reset that memory
        /^[[:space:]]*\/\// { prev_comment = 1; prev = line; next }
        /^[[:space:]]*#/ { next }
        {
            is_type = (line ~ /^(export )?type [A-Za-z_]/)
            if (is_type) {
                if (!prev_comment && line !~ /\/\//) {
                    printf "  %s:%d: тип без комментария: %s\n", rel, NR, line
                }
                # a record block opens if the line ends with {
                in_block = (line ~ /\{[[:space:]]*$/)
            } else if (in_block) {
                if (line ~ /^\}/) { in_block = 0 }
                else if (line ~ /^[[:space:]]+[a-z_][a-zA-Z0-9_]* / && line !~ /\/\//) {
                    if (!prev_comment) {
                        printf "  %s:%d: поле без комментария: %s\n", rel, NR, line
                    }
                }
            }
            prev_comment = 0
        }
    ' "$f"
done)

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — типы/поля без комментария (конвенция П13):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Каждому type — что он представляет; каждому полю — что хранит," >&2
    echo "  чем индексируется, какой инвариант (строкой выше или на строке)." >&2
    exit 1
fi
N=$(find "$SRC" -type f -name '*.nv' | wc -l | tr -d '[:space:]')
echo "$NAME ok: файлов .nv: $N, типов/полей без комментария: 0"
exit 0
