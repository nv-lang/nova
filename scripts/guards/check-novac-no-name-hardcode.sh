#!/bin/sh
# scripts/guards/check-novac-no-name-hardcode.sh — никакого хардкода имён
# Nova/std в компиляторе (конвенция П5; заведён 2026-08-15 по слову владельца:
# «subset_method_ret — хардкод, страж почему не ловит?» — потому что стража
# не было).
#
# ПРАВИЛО (П5; план 274): строковый литерал с именем языка/std законен ТОЛЬКО
# в novac/src/builtins/builtins.nv — едином реестре «легитимного остатка П5».
# Везде ещё в novac/src — красный. Остаток снимается Э2-б (чтение деклараций
# std): файл builtins.nv худеет, страж остаётся.
#
# ПРОВЕРЯЕТ: грепом по novac/src/**/*.nv (кроме builtins.nv и *_test.nv)
# строковые литералы, целиком равные имени из списка. СПИСОК ВЫВОДИТСЯ ИЗ
# ДАННЫХ, а не зашит (дефект F14, снят 2026-08-15: список внутри стража
# пополнялся «ревью-красным», то есть правилом без механизма — ровно тем, от
# чего страж и заведён):
#   (1) все строковые литералы самого builtins.nv, имеющие форму
#       идентификатора — единственный законный дом имён; худеет builtins.nv,
#       автоматически худеет и список;
#   (2) короткий список имён ПРЕЛЮДИИ языка (Option Result Some None Ok Err
#       Vec HashMap) — это поверхность языка, она меняется только вместе со
#       спекой, потому и стоит здесь строкой, а не выводится.
# НЕ ПРОВЕРЯЕТ: имена в комментариях (строки //... срезаются перед грепом —
# и вместе с ними литерал, содержащий «//», например URL); литералы, не
# имеющие формы идентификатора («[]int», «Nova_Vec____» с решётками и
# скобками, куски интерполяции) — из builtins.nv берутся только
# идентификаторы, чтобы список не втащил в греп регэксп-метасимвол; имена,
# собранные из кусков в рантайме («Nova_" + "str"»); ключевые слова
# ГРАММАТИКИ в лексере («fn», «module» — это лексер по определению, у rustc
# тоже таблица kw::*; П5 — про сущности std/языка, которые обязаны браться из
# деклараций, а не про синтаксис) — они не попадают в список, пока их нет в
# builtins.nv; коды диагностик E_*/W_* (это имена novac, не Nova).
#
# $1 — корень репозитория; $2 — override сканируемой директории (самотест).
# Проверялся: Windows (Git Bash), 2026-08-15.
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
NAME=check-novac-no-name-hardcode

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

strip_comments() {
    # Срезает //-комментарий, но НЕ внутри строкового литерала (274.3/F14,
    # адверсарная проверка: `sed 's|//.*$||'` резал строку на литерале вида
    # "http://…" и прятал за ним всё остальное, включая нарушение).
    awk '{
        out = ""; inq = 0; n = length($0)
        for (i = 1; i <= n; i++) {
            c = substr($0, i, 1)
            p = (i > 1) ? substr($0, i - 1, 1) : ""
            if (c == "\"" && p != "\\") { inq = !inq }
            if (!inq && c == "/" && substr($0, i + 1, 1) == "/") { break }
            out = out c
        }
        print out
    }' "$1"
}

# (1) The one legitimate home of names: every identifier-shaped literal in it.
BUILTINS=$(find "$SRC" -type f -name 'builtins.nv' | sort | head -n 1)
FROM_BUILTINS=""
if [ -n "$BUILTINS" ]; then
    FROM_BUILTINS=$(strip_comments "$BUILTINS" | grep -oE '"[^"]*"' | tr -d '"' \
        | grep -E '^[A-Za-z_][A-Za-z0-9_]*$' | sort -u)
fi

# (2) The prelude: language surface, moves only with the spec.
PRELUDE='Option
Result
Some
None
Ok
Err
Vec
HashMap'

NAMES=$(printf '%s\n%s\n' "$FROM_BUILTINS" "$PRELUDE" | grep -E '^[A-Za-z_][A-Za-z0-9_]*$' | sort -u)
NB=$(printf '%s\n' "$FROM_BUILTINS" | grep -c '[A-Za-z_]')
NN=$(printf '%s\n' "$NAMES" | grep -c '[A-Za-z_]')
ALT=$(printf '%s\n' "$NAMES" | tr '\n' '|' | sed 's/|$//')

if [ -z "$ALT" ]; then
    echo "$NAME: FAIL — список имён пуст: ни builtins.nv, ни прелюдия не дали ни одного имени" >&2
    exit 1
fi

BAD=$(find "$SRC" -type f -name '*.nv' ! -name 'builtins.nv' ! -name '*_test.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    # literal = "..." вне комментария; комментарии срезаются С УЧЁТОМ кавычек
    strip_comments "$f" | grep -n -E "\"($ALT)\"" | sed "s|^|  $rel:|"
done)

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — имена языка/std как строковые литералы вне builtins.nv (П5):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Имя — в novac/src/builtins/builtins.nv (единый реестр остатка П5), здесь — константа/дверь." >&2
    exit 1
fi
N=$(find "$SRC" -type f -name '*.nv' ! -name 'builtins.nv' ! -name '*_test.nv' | wc -l | tr -d '[:space:]')
echo "$NAME ok: файлов .nv: $N, имён в списке: $NN (из builtins.nv: $NB + прелюдия), хардкод-имён вне builtins.nv: 0"
exit 0
