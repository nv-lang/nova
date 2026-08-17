#!/bin/sh
# scripts/guards/check-novac-no-prelude-shadow.sh — novac не объявляет имя
# типа или свободной функции, которое уже экспортирует прелюдия Nova.
# Прелюдия импортируется в каждый модуль автоматически, поэтому такое имя
# ТЕНИТ прелюдное МОЛЧА: компилируется, а беда всплывает при первой встрече
# двух смыслов одного слова. Живой случай 2026-08-15: novac объявил
# `type Outcome`, а прелюдия экспортирует `Outcome[T] enum Finished(T) |
# Aborted`; поймал владелец глазами — стража на этот класс не было.
#
# ПРОВЕРЯЕТ: пересечение двух списков имён —
#   * прелюдия — строки `export type X` и `export fn x(` в файлах
#     $ROOT/std/src/prelude/*.nv (без рекурсии; generic-хвост `[T]` и
#     newtype-хвост `(...)` в имя не входят), КРОМЕ файлов *_test.nv;
#   * novac — строки `type X`/`export type X` и `fn x(`/`export fn x(`
#     в $ROOT/novac/src/**/*.nv, КРОМЕ файлов *_test.nv.
#   Оба списка сводятся в одно пространство имён: у типа и у свободной
#   функции модульная область видимости одна, и тень одинаково молчалива.
# НЕ ПРОВЕРЯЕТ:
#   * методы (`fn Type @name(`, `fn Type mut @name(`, `fn Type[T] @name(`) —
#     у них своё пространство имён, тенить прелюдию они не могут, и в
#     списки не попадают ни с одной стороны;
#   * ассоциированные функции (`fn Type.new(`) — они за именем типа;
#   * имена прелюдии за пределами `export` (внутренние помощники модуля);
#   * поля, варианты enum, локальные привязки — тенить прелюдный ИМПОРТ
#     они не могут;
#   * тесты novac (*_test.nv): там своё имя живёт внутри файла-теста;
#   * тесты прелюдии (std/src/prelude/*_test.nv): такой файл объявляет
#     СВОЙ модуль (`module prelude.embed_test`, не `prelude.embed`), его
#     `export` — не имя автоимпортируемой прелюдии, и считать его именем
#     прелюдии значило бы красить novac зря (исключение симметрично
#     исключению тестов novac).
#
# Реестр правил: план 274 §10.3/§10.3а (каждое правило — против своего
# стража); подплан 274.3 — классы находок ревью и защита от них.
# $1 — корень репозитория; $2 — override сканируемой директории novac
# (шов самотеста); $3 — override директории прелюдии (шов самотеста).
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
PRELUDE="${3:-$ROOT/std/src/prelude}"
NAME=check-novac-no-prelude-shadow

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi
if [ ! -d "$PRELUDE" ]; then
    echo "$NAME ok: судить нечего (нет $PRELUDE)"
    exit 0
fi

# Разбор декларации — общий для обеих сторон, чтобы списки строились по
# одному правилу и «зелено там, красно тут» не могло взяться из разбора.
#   decl_ident(line, kw)  — имя сразу после `kw` (с необязательным `export`),
#                           хвост после имени кладёт в глобальный TAIL;
#   free_fn(tail)         — TRUE, если это СВОБОДНАЯ функция: за именем сразу
#                           `(` либо generic-голова `[...]` и затем `(`.
#                           Голова читается по скобкам, поэтому `[T, U](`
#                           с пробелом — такая же свободная функция, как
#                           `[T](`; а `Type @name(`, `Type mut @name(`,
#                           `Type[T] @name(`, `Type.new(` — не свободные.
AWK_LIB='
function decl_ident(line, kw,   rest, name) {
    if (!match(line, "^(export[ \t]+)?" kw "[ \t]+")) { return "" }
    rest = substr(line, RSTART + RLENGTH)
    if (!match(rest, /^[A-Za-z_][A-Za-z0-9_]*/)) { TAIL = ""; return "" }
    name = substr(rest, 1, RLENGTH)
    TAIL = substr(rest, RLENGTH + 1)
    return name
}
function free_fn(tail,   i) {
    if (substr(tail, 1, 1) == "[") {
        i = index(tail, "]")
        if (i == 0) { return 0 }
        tail = substr(tail, i + 1)
    }
    return substr(tail, 1, 1) == "("
}
'

# Список имён прелюдии: строки `имя<TAB>вид<TAB>файл:строка`.
PRE=$(find "$PRELUDE" -maxdepth 1 -type f -name '*.nv' ! -name '*_test.nv' | sort | while IFS= read -r f; do
    awk -v rel="prelude/$(basename "$f")" "$AWK_LIB"'
        { line = $0; sub(/\r$/, "", line); TAIL = "" }
        # `export type X`, `export type X[T]`, `export type X(...)`
        line ~ /^export[ \t]+type[ \t]/ {
            n = decl_ident(line, "type")
            if (n != "") { printf "%s\ttype\t%s:%d\n", n, rel, NR }
            next
        }
        # свободная функция: имя вплотную к `(` (или к generic-голове).
        # `Mem.alloc_count(` и `EmbeddedDir @len(` этим не ловятся — и не должны.
        line ~ /^export[ \t]+fn[ \t]/ {
            n = decl_ident(line, "fn")
            if (n != "" && free_fn(TAIL)) { printf "%s\tfn\t%s:%d\n", n, rel, NR }
        }
    ' "$f"
done | sort -u)

NPRE=$(printf '%s' "$PRE" | grep -c . )

# Декларации novac, сверенные со списком прелюдии на лету.
BAD=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | sort | while IFS= read -r f; do
    rel=${f#"$SRC"/}
    awk -v rel="$rel" -v pre="$PRE" "$AWK_LIB"'
        BEGIN {
            n = split(pre, rows, "\n")
            for (i = 1; i <= n; i++) {
                if (rows[i] == "") { continue }
                split(rows[i], c, "\t")
                kind[c[1]] = c[2]; where[c[1]] = c[3]
            }
        }
        { line = $0; sub(/\r$/, "", line); TAIL = "" }
        line ~ /^(export[ \t]+)?type[ \t]/ {
            n2 = decl_ident(line, "type")
            if (n2 != "" && (n2 in kind)) {
                printf "  %s:%d: type %s тенит прелюдный %s %s (%s)\n", rel, NR, n2, kind[n2], n2, where[n2]
            }
            next
        }
        line ~ /^(export[ \t]+)?fn[ \t]/ {
            n2 = decl_ident(line, "fn")
            if (n2 != "" && free_fn(TAIL) && (n2 in kind)) {
                printf "  %s:%d: fn %s тенит прелюдный %s %s (%s)\n", rel, NR, n2, kind[n2], n2, where[n2]
            }
        }
    ' "$f"
done)

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — novac тенит имена прелюдии (прелюдия импортируется автоматически, тень молчалива):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  чинить: переименовать декларацию в novac (имя по роли внутри компилятора," >&2
    echo "  напр. Outcome -> CompileOutcome/StepResult), либо — если нужен именно" >&2
    echo "  прелюдный смысл — удалить свою декларацию и пользоваться прелюдной." >&2
    exit 1
fi
N=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | wc -l | tr -d '[:space:]')
echo "$NAME ok: имён прелюдии: $NPRE, файлов novac/src: $N, теней: 0"
exit 0
