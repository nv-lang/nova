#!/bin/sh
# scripts/guards/check-novac-type-field-docs.sh — каждый тип, каждое поле
# и КАЖДАЯ ФУНКЦИЯ novac несут документацию (конвенция П13; владелец
# 2026-08-14 типы/поля, 2026-08-15 функции — «простыми словами коротко,
# ///-doc, по-английски»).
#
# ПРОВЕРЯЕТ awk-ом по novac/src/**/*.nv (тесты *_test.nv исключены):
#   * `type X`/`export type X` — `///`-док строкой выше (атрибуты `#...`
#     между доком и декларацией допустимы);
#   * `fn ...`/`export fn ...` — `///`-док строкой выше;
#   * внутри блока `type ... {` каждая строка-поле (`имя тип`) —
#     `//` на строке поля или строкой выше (`///` на поле нелегально в
#     языке до амендмента D104; после него страж ужесточится той же волной).
# НЕ ПРОВЕРЯЕТ: содержательность и язык дока (приёмка; заглушка хуже
# отсутствия); enum-варианты `| X`; многострочные декларации полей.
#
# $1 — корень репозитория; $2 — override сканируемой директории (самотест).
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-type-field-docs

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

# Field-doc strictness follows the oracle pin (self-expiring transition):
# D104 rev-2 (fields/variants take ///, both forms) landed in main at
# 9a69411b3; while novac/nova.toml still pins an OLDER oracle the guard
# accepts `//` on fields as the transition form; once the pin moves to a
# revision containing 9a69411b3, `//` on a field is red — same wave as the
# pin move (P13.2).
PIN=$(tr -d '\r' < "$ROOT/novac/nova.toml" | sed -n 's/^#[[:space:]]*oracle-pin:[[:space:]]*\([0-9a-f][0-9a-f]*\)$/\1/p')
FIELD_STRICT=0
if [ -n "$PIN" ] && git -C "$ROOT" merge-base --is-ancestor 9a69411b3 "$PIN" 2>/dev/null; then FIELD_STRICT=1; fi

BAD=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    awk -v rel="$rel" -v strict="$FIELD_STRICT" '
        { line = $0; sub(/\r$/, "", line) }
        # memory of the PREVIOUS line: doc (///) or plain comment (//);
        # attribute lines (#impl(...) etc.) between a doc and its
        # declaration must NOT reset that memory
        /^[[:space:]]*\/\/\// { prev_comment = 1; prev_doc = 1; next }
        /^[[:space:]]*\/\//   { prev_comment = 1; prev_doc = 0; next }
        /^[[:space:]]*#/      { next }
        {
            is_type = (line ~ /^(export )?type [A-Za-z_]/)
            is_fn = (line ~ /^(export )?fn /)
            if (is_fn) {
                if (!prev_doc) {
                    printf "  %s:%d: функция без ///-дока: %s\n", rel, NR, substr(line, 1, 60)
                }
                in_block = 0
            } else if (is_type) {
                if (!prev_doc) {
                    printf "  %s:%d: тип без ///-дока: %s\n", rel, NR, line
                }
                # a record block opens if the line ends with {
                in_block = (line ~ /\{[[:space:]]*$/)
            } else if (in_block) {
                if (line ~ /^\}/) { in_block = 0 }
                else if (line ~ /^[[:space:]]+[a-z_][a-zA-Z0-9_]* /) {
                    if (strict) {
                        # D104 rev-2: trailing /// on the line, or /// above; // is not a doc
                        if (line !~ /\/\/\// && !prev_doc) {
                            printf "  %s:%d: поле без ///-дока (D104 rev-2: trailing или строкой выше): %s\n", rel, NR, line
                        }
                    } else if (line !~ /\/\// && !prev_comment) {
                        printf "  %s:%d: поле без комментария: %s\n", rel, NR, line
                    }
                }
            }
            prev_comment = 0
            prev_doc = 0
        }
    ' "$f"
done)

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — типы/функции/поля без документации (конвенция П13):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  type и fn — ///-док строкой выше, простыми словами, коротко, по-английски;" >&2
    echo "  поле — // на строке поля (что хранит, чем индексируется, инвариант)." >&2
    exit 1
fi
N=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | wc -l | tr -d '[:space:]')
MODE="переходный // (пин < 9a69411b3)"; [ "$FIELD_STRICT" = 1 ] && MODE="/// строго (D104 rev-2)"
echo "$NAME ok: файлов .nv: $N, типов/функций/полей без документации: 0 (поля: $MODE)"
exit 0
