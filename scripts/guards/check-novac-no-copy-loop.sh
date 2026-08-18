#!/bin/sh
# scripts/guards/check-novac-no-copy-loop.sh — перекладывание коллекции
# поэлементно (`for x in Y { Z.push(x) }`) запрещено: в std уже есть дверь
# `Vec[T].append(other AsSlice[T])`.
#
# ЗАЧЕМ. Владелец, 2026-08-18, глядя на `for n in @type_ref() { out.push(n) }`:
# «почему не out.append(@type_ref())?». Ответ был «потому что я не посмотрел» —
# семь таких циклов в парсере писались один за другим, и ни разу я не спросил
# std, есть ли у Vec эта операция. Она есть с самого начала.
#
# Цикл вместо `append` стоит не только строк. Он ПОВТОРЯЕТ реализацию: имя
# переменной цикла, порядок обхода и то, что копируется КАЖДЫЙ элемент, —
# читатель обязан это разобрать, чтобы узнать «здесь просто дописали хвост».
# `append` говорит то же самое одним словом и не может ошибиться в границе.
# И главное: своя копия не получает того, что получает дверь std — правок,
# оптимизаций, единого поведения на пустом входе.
#
# ПРОВЕРЯЕТ novac/src/**/*.nv (тесты исключены): нет цикла, чьё ТЕЛО — ровно
#   один `push` переменной этого же цикла. Обе формы, однострочная
#   (`for x in Y { Z.push(x) }`) и многострочная.
# НЕ ПРОВЕРЯЕТ: циклы, которые кладут ПРЕОБРАЗОВАНИЕ (`Z.push(f(x))`) или
#   делают что-то ещё кроме push — там цикл несёт работу, а не переливание;
#   и push в другой цикл/условие внутри тела.
#
# $1 — корень; $2 — override директории (шов самотеста).
# Проверялся: Windows (Git Bash), 2026-08-18.
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
NAME=check-novac-no-copy-loop

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

T="${TMPDIR:-/tmp}/novac-copyloop.$$"
mkdir -p "$T" || exit 1
trap 'rm -rf "$T"' 0

find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | sort > "$T/files"
[ -s "$T/files" ] || { echo "$NAME: FAIL — в $SRC нет ни одного .nv: страж потерял мишень" >&2; exit 1; }

: > "$T/bad"
: > "$T/cnt"
cat "$T/files" | xargs awk -v SRC="$SRC" -v OUT="$T/bad" -v CNT="$T/cnt" '
        FNR == 1 { REL = FILENAME; sub("^" SRC "/", "", REL) }
        { sub(/\r$/, "", $0) }
        {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            sub(/[[:space:]]+$/, "", line)
            total++

            # однострочная форма: for x in Y { Z.push(x) }
            if (match(line, /^for [A-Za-z_][A-Za-z0-9_]* in .+ \{ [A-Za-z_@][A-Za-z0-9_.]*\.push\([A-Za-z_][A-Za-z0-9_]*\) \}$/)) {
                v = line; sub(/^for /, "", v); sub(/ in .*/, "", v)
                a = line; sub(/.*\.push\(/, "", a); sub(/\).*/, "", a)
                if (v == a) {
                    printf "  %s:%d — перекладывание поэлементно: %s\n", REL, FNR, substr(line, 1, 70) >> OUT
                }
                next
            }

            # многострочная форма: заголовок цикла, одна строка push, закрывающая скобка
            if (match(line, /^for [A-Za-z_][A-Za-z0-9_]* in .+ \{$/)) {
                v = line; sub(/^for /, "", v); sub(/ in .*/, "", v)
                pend_var = v; pend_line = FNR; pend_state = 1; pend_text = line
                next
            }
            if (pend_state == 1) {
                if (match(line, /^[A-Za-z_@][A-Za-z0-9_.]*\.push\([A-Za-z_][A-Za-z0-9_]*\)$/)) {
                    a = line; sub(/.*\.push\(/, "", a); sub(/\)$/, "", a)
                    if (a == pend_var) { pend_state = 2; next }
                }
                pend_state = 0
                next
            }
            if (pend_state == 2) {
                if (line == "}") {
                    printf "  %s:%d — перекладывание поэлементно: %s\n", REL, pend_line, substr(pend_text, 1, 70) >> OUT
                }
                pend_state = 0
                next
            }
        }
        END { print total+0 >> CNT }
    '
N=$(awk '{s+=$1} END {print s+0}' "$T/cnt")

if [ -s "$T/bad" ]; then
    echo "$NAME: FAIL — коллекция перекладывается поэлементно:" >&2
    cat "$T/bad" >&2
    echo "  В std есть дверь: \`Vec[T].append(other AsSlice[T])\`. Своя копия" >&2
    echo "  повторяет её реализацию и не получает её правок." >&2
    echo "  Цикл законен там, где он НЕСЁТ работу: \`Z.push(f(x))\`, условие," >&2
    echo "  накопление — этого страж не трогает." >&2
    exit 1
fi

echo "$NAME ok: строк .nv: $N, циклов-перекладываний: 0 (append живёт в std)"
exit 0
