#!/bin/sh
# scripts/guards/check-novac-branch-complete.sh — ветвление в novac обязано
# быть ПОЛНЫМ (конвенция П31; указание владельца 2026-08-17: «все ветвления
# обязательны, else обязателен, если ветка пуста — обозначь явно, что это
# валидная ситуация»).
#
# ПОЧЕМУ. За одну смену 2026-08-17 четыре дефекта родились из одного и того
# же: решение принималось в `if`, вторая ветка не называлась, и вход просто
# проваливался мимо — тип параметра шире идентификатора, тип возврата не
# судил никто, каноническая сумма не разбиралась, объявление метода роняло
# компилятор. Ни один из них не выглядел как ошибка в коде: они выглядели как
# ОТСУТСТВИЕ кода.
#
# ЧТО СЧИТАЕТСЯ ПОЛНЫМ ВЕТВЛЕНИЕМ — три формы, и все три явные:
#   1. `if ... { ... } else { ... }` — обе ветки написаны;
#   2. then-ветка кончается ТЕРМИНАТОРОМ (`return`, `continue`, `break`,
#      `throw`, `ice(`) — тогда «иначе» это остаток функции или следующая
#      итерация, и он явен по конструкции;
#   3. `else { }` пустой — законен, но ОБЯЗАН нести комментарий с причиной:
#      почему ничего не происходит и почему это правильно.
#
# ЧЕГО СТРАЖ НЕ ТРЕБУЕТ: `else` после формы 2. Требовать его значило бы
# добавить 255 пустых скобок (замер 2026-08-17), не сообщающих ничего.
#
# ПРОВЕРЯЕТ novac/src/**/*.nv. $1 — корень; $2 — override директории (шов
# самотеста). Проверялся: Windows (Git Bash), 2026-08-17.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-branch-complete

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-branch.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

find "$SRC" -type f -name '*.nv' | sort > "$T/files"
if [ ! -s "$T/files" ]; then
    echo "$NAME: FAIL — в $SRC нет ни одного .nv: страж потерял мишень" >&2
    exit 1
fi

: > "$T/bad"
N=0
while IFS= read -r f; do
    rel=${f#"$SRC"/}
    tr -d '\r' < "$f" | awk -v REL="$rel" -v OUT="$T/bad" -v CNT="$T/cnt" '
        function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
        {
            line[NR] = $0
        }
        END {
            n = NR
            for (i = 1; i <= n; i++) {
                s = trim(line[i])
                if (s !~ /^if [^=]/ && s !~ /^} else if /) continue
                if (s ~ /^\/\//) continue
                total++
                # однострочная форма: `if c { ... }` целиком на строке
                if (s ~ /\{.*\}[ \t]*$/) {
                    if (s ~ /else/) continue
                    if (s ~ /(return|continue|break|throw|ice\()/) continue
                    printf "  %s:%d — ветвление без else и без терминатора: %s\n", REL, i, substr(s, 1, 72) >> OUT
                    continue
                }
                if (s !~ /\{[ \t]*$/) continue
                # блочная форма: найти закрывающую скобку того же отступа
                match(line[i], /^[ \t]*/)
                ind = RLENGTH
                term = 0
                for (j = i + 1; j <= n; j++) {
                    cur = line[j]
                    match(cur, /^[ \t]*/)
                    if (trim(cur) ~ /^\}/ && RLENGTH == ind) break
                    if (trim(cur) ~ /^(return|continue|break|throw)([ (]|$)/) term = 1
                    if (trim(cur) ~ /ice\(/) term = 1
                }
                closer = (j <= n) ? trim(line[j]) : ""
                if (closer ~ /^\} else/) continue
                if (term) continue
                printf "  %s:%d — ветвление без else и без терминатора: %s\n", REL, i, substr(s, 1, 72) >> OUT
            }
            print total >> CNT
        }
    '
done < "$T/files"
N=$(awk '{s+=$1} END {print s+0}' "$T/cnt" 2>/dev/null)

# пустой else обязан нести причину
while IFS= read -r f; do
    rel=${f#"$SRC"/}
    tr -d '\r' < "$f" | awk -v REL="$rel" -v OUT="$T/bad" '
        function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
        { line[NR] = $0 }
        END {
            for (i = 1; i <= NR; i++) {
                s = trim(line[i])
                if (s !~ /else[ \t]*\{[ \t]*\}?[ \t]*$/) continue
                if (s ~ /else[ \t]*\{.*[^ \t{}].*\}/) continue
                body = (i + 1 <= NR) ? trim(line[i+1]) : ""
                if (s ~ /\{[ \t]*\}[ \t]*$/) {
                    if (trim(line[i-1]) ~ /^\/\// || s ~ /\/\//) continue
                    printf "  %s:%d — пустой else без причины: %s\n", REL, i, substr(s, 1, 60) >> OUT
                    continue
                }
                if (body ~ /^\}/) {
                    if (trim(line[i-1]) ~ /^\/\//) continue
                    printf "  %s:%d — пустой else без причины: %s\n", REL, i, substr(s, 1, 60) >> OUT
                }
            }
        }
    '
done < "$T/files"

# ХРАПОВИК, а не мгновенный запрет. Правило введено 2026-08-17 на дереве, где
# неполных ветвлений было 191: запретить их одним днём значит либо красный
# гейт на неделю, либо 191 механическая правка без разбора — а разбирать надо
# каждую, потому что половина из них просится в `match`. Поэтому число
# записано базой и может ТОЛЬКО УБЫВАТЬ; рост — красный, снижение требует
# опустить базу тем же коммитом (иначе следующий рост до прежней цифры
# пройдёт молча — та же дисциплина, что у храповика корпуса, §10.4).
BASE_FILE="$ROOT/scripts/guards/novac-branch.baseline"
n=$(grep -c . "$T/bad" 2>/dev/null)
[ -n "$n" ] || n=0
BASE=$(tr -d '\r' < "$BASE_FILE" 2>/dev/null | sed -n 's/^incomplete-branches[[:space:]][[:space:]]*\([0-9][0-9]*\).*/\1/p' | head -1)
if [ -z "$BASE" ]; then
    echo "$NAME: FAIL — нет базы $BASE_FILE: судить нечем, а нечем != зелено" >&2
    exit 1
fi

if [ "$n" -gt "$BASE" ]; then
    echo "$NAME: FAIL — неполных ветвлений $n, в базе $BASE — РОСТ (П31)" >&2
    head -n 15 "$T/bad" >&2
    [ "$n" -gt 15 ] && echo "  ... и ещё $((n - 15))" >&2
    echo "  Полное ветвление — одно из трёх: else с телом; терминатор в" >&2
    echo "  then-ветке (return/continue/break/throw/ice); пустой else С" >&2
    echo "  КОММЕНТАРИЕМ, объясняющим, почему ничего не происходит." >&2
    echo "  Несколько if об одном предмете — это match (П31 п.2): он" >&2
    echo "  исчерпаем по конструкции, и компилятор сам не даст пропустить." >&2
    exit 1
fi

if [ "$n" -lt "$BASE" ]; then
    echo "$NAME: FAIL — неполных ветвлений $n, в базе $BASE — ПРОГРЕСС без опускания базы" >&2
    echo "  Опусти число в $BASE_FILE ТЕМ ЖЕ коммитом (§10.4)." >&2
    exit 1
fi

echo "$NAME ok: ветвлений $N, неполных $n (== база; храповик на убывание)"
exit 0
