#!/bin/sh
# scripts/guards/check-novac-channel-one-writer.sh — канал чекера пишет ОДИН
# чекер, а вывод типов ниже чекера не живёт (архитектура, раздел «Канал
# чекера»: «после чекера ни один потребитель не вызывает вывод типа»).
#
# ЗАЧЕМ. Архитектура называла этот страж прозой — «греп по novac/src/** вне
# novac/src/check/ даёт ноль вхождений unify, infer_, fresh_var», — но файла
# не было, и правило держалось на памяти автора. Оно ровно того класса, что
# план 196 снимал месяцами в нынешнем компиляторе: вторая дверь к выводу типа
# заводится не решением, а тем, что автор бэкенда не нашёл первую. До Э2-б1
# так и было: `emit_c` звал `sem.type_of` прямо во время эмиссии.
#
# ПРОВЕРЯЕТ по novac/src/**/*.nv (тесты *_test.nv тоже: тест, зовущий писателя
#   канала мимо чекера, — та же вторая дверь):
#   (A) писатели канала (`record_type`, `record_callee`, `record_subst`)
#       вызываются ТОЛЬКО из novac/src/check/ и определяются только в
#       novac/src/sem/channel.nv;
#   (B) вывод типа (`type_of(` как СВОБОДНЫЙ вызов решётки, `unify`, `infer_`,
#       `fresh_var`) не встречается вне novac/src/check/. Чтение канала
#       (`.type_of(` на приёмнике) — законно и не считается: это ЧТЕНИЕ
#       решения, а не вывод.
#   (C) сам файл канала существует и объявляет `CheckOut` — иначе страж судил
#       бы воздух (класс №519).
# НЕ ПРОВЕРЯЕТ: что записанное ВЕРНО (это дифф-корпус и байт-в-байт C);
#   полноту канала (тотальный обход — Э2-б3, и до него дыра ловится тотальным
#   читателем, который падает ice).
#
# $1 — корень репозитория; $2 — override сканируемой директории (шов самотеста).
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-channel-one-writer

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-channel-writer.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

# --- (C) мишень на месте ---------------------------------------------------
CHAN=$(find "$SRC" -type f -name 'channel.nv' | head -n 1)
if [ -z "$CHAN" ] || ! grep -q 'export type CheckOut' "$CHAN" 2>/dev/null; then
    echo "$NAME: FAIL — не найден файл канала с 'export type CheckOut': страж потерял мишень (класс №519)" >&2
    exit 1
fi

BAD=""

# ОДИН проход по дереву вместо трёх (2026-08-18). Прежняя редакция читала все
# файлы трижды и поднимала по два-три grep на каждый: 22.1 секунды стены на 29
# файлах. Разделы и их тексты те же и в том же порядке -- (A) писатели вне
# check/, затем ПРЯМАЯ запись в поле канала, затем (B) вывод типа ниже чекера,
# -- иначе разойдётся сам вывод, а не только его скорость.
CHAN_FIELDS='types|callees|substs|subst_args'
SCAN=$(find "$SRC" -type f -name '*.nv' | sort | xargs awk -v SRC="$SRC" -v CF="$CHAN_FIELDS" '
    FNR == 1 { rel = FILENAME; sub("^" SRC "/", "", rel); is_check = (rel ~ /^check\//); is_chan = (rel == "sem/channel.nv") }
    {
        line = $0; sub(/\r$/, "", line)
        bare = line; sub(/^[[:space:]]+/, "", bare)
        if (bare ~ /^\/\//) next

        if (!is_check && !is_chan && line ~ /record_type\(|record_callee\(|record_subst\(/)
            printf "A|%s|%d:%s\n", rel, FNR, line

        if (!is_chan && (line ~ ("\\.(" CF ")\\[[^]]*\\][[:space:]]*=[^=]") ||
                         line ~ ("\\.(" CF ")[[:space:]]*=[^=]")))
            printf "D|%s|%d:%s\n", rel, FNR, line

        if (!is_check && !is_chan &&
            line ~ /unify\(|fresh_var\(|infer_[a-z_]*\(|[^.a-zA-Z_]type_of\(/)
            printf "B|%s|%d:%s\n", rel, FNR, line
    }
')

add_section() {
    tag=$1; note=$2
    printf '%s\n' "$SCAN" | grep "^$tag|" | cut -d'|' -f2- | sort -u -t'|' -k1,1 -k2,2 | \
        awk -F'|' -v NOTE="$note" '
            { if ($1 != cur) { cur = $1; printf "\n  %s — %s:\n", cur, NOTE } printf "      %s\n", $2 }
        '
}

BAD="$BAD$(add_section A 'зовёт писателя канала вне check/')"
BAD="$BAD$(add_section D 'ПРЯМАЯ запись в таблицу канала мимо двери')"
BAD="$BAD$(add_section B 'вывод типа вне check/ (вторая дверь к типу, класс плана 196)')"
BAD=$(printf '%s' "$BAD" | grep '[A-Za-z]' || true)

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — у канала чекера появился второй писатель или вывод типа уехал ниже чекера:" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Правило: пишет ТОЛЬКО check, остальные ЧИТАЮТ (out.type_of(id)). Нужен новый" >&2
    echo "  факт о типе — его записывает чекер, а не вычисляет потребитель." >&2
    exit 1
fi

NF=$(find "$SRC" -type f -name '*.nv' | wc -l | tr -d '[:space:]')
NW=$(grep -c 'record_type(\|record_callee(\|record_subst(' "$SRC"/check/*.nv 2>/dev/null | awk -F: '{s+=$NF} END {print s+0}')
echo "$NAME ok: файлов .nv: $NF, вызовов писателей канала: $NW (все в check/), вывода типа вне чекера: 0"
exit 0
