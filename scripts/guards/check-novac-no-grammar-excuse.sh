#!/bin/sh
# scripts/guards/check-novac-no-grammar-excuse.sh — отказ novac НЕ ссылается на
# незнание грамматики: строка «not in the MVP grammar» запрещена как класс
# (план 274 §9.4, механизм назван там же; заводится после закрытия §9.4а).
#
# ЗАЧЕМ. Пока парсер судил о принадлежности к подмножеству, всякая незнакомая
# ему форма давала отказ, который НЕ НАЗЫВАЛ причину — а иногда называл чужую:
# вариадик отвечал «fn requires a declared return type» о сигнатуре, где тип
# возврата написан; `else` отвечал «record constructor only as a binding
# initializer», потому что скобка после слова `else` читалась как литерал
# записи. Ложная причина дороже молчания: по ней идут править верный код.
#
# С 2026-08-18 парсер читает ЯЗЫК целиком, а решение «вне подмножества»
# принимает чекер и обязан назвать форму. Строка-отговорка снята со всех
# носителей тем же слиянием, поэтому страж — ЗАПРЕТ, а не храповик.
#
# ПРОВЕРЯЕТ novac/src/**/*.nv:
#   * ни одна СТРОКОВАЯ ЛИТЕРАЛЬНАЯ диагностика не содержит «MVP grammar»;
#   * ни один отказ не содержит «not in the grammar» / «unknown construct».
# НЕ ПРОВЕРЯЕТ комментарии и доки: там эта строка ЗАКОННА и нужна — история
#   класса объясняет, почему конструкции читаются целиком, и стирать её значит
#   стирать причину (см. заголовки parse/decls.nv и check/check.nv).
#
# $1 — корень; $2 — override директории (шов самотеста).
# Проверялся: Windows (Git Bash), 2026-08-18.
export LC_ALL=C
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-no-grammar-excuse

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-excuse.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

find "$SRC" -type f -name '*.nv' | sort > "$T/files"
[ -s "$T/files" ] || { echo "$NAME: FAIL — в $SRC нет ни одного .nv: страж потерял мишень" >&2; exit 1; }

: > "$T/bad"
: > "$T/cnt"
cat "$T/files" | xargs awk -v SRC="$SRC" -v OUT="$T/bad" -v CNT="$T/cnt" '
        FNR == 1 { REL = FILENAME; sub("^" SRC "/", "", REL) }
        { sub(/\r$/, "", $0) }
        {
            line = $0
            s = line
            sub(/^[[:space:]]+/, "", s)
            # комментарий или док — не судим: история класса там законна
            if (s ~ /^\/\//) next
            # ищем запрещённый текст ВНУТРИ строкового литерала
            if (line ~ /"[^"]*MVP grammar[^"]*"/ ||
                line ~ /"[^"]*not in the grammar[^"]*"/ ||
                line ~ /"[^"]*unknown construct[^"]*"/) {
                printf "  %s:%d — отказ ссылается на незнание грамматики: %s\n", REL, FNR, substr(s, 1, 72) >> OUT
                bad++
            }
            total++
        }
        END { print total+0 >> CNT }
    '
N=$(awk '{s+=$1} END {print s+0}' "$T/cnt")

if [ -s "$T/bad" ]; then
    echo "$NAME: FAIL — диагностика ссылается на незнание грамматики (274 §9.4):" >&2
    cat "$T/bad" >&2
    echo "  Парсер читает ЯЗЫК целиком; «вне подмножества» решает чекер и" >&2
    echo "  обязан НАЗВАТЬ форму и этап: «outside the subset: a variadic" >&2
    echo "  parameter ... arrives with generics (E2-b)»." >&2
    echo "  Если форма и правда не читается — это СИНТАКСИЧЕСКАЯ ошибка, и" >&2
    echo "  говорить надо так (SYNTAX_MSG), а не про подмножество." >&2
    exit 1
fi

echo "$NAME ok: строк .nv: $N, отговорок про грамматику: 0 (форму называет отказ)"
exit 0
