#!/bin/sh
# scripts/guards/check-retracted-param-form.sh — снятая постфиксная форма
# параметра не имеет права появляться в доке.
#
# План/реестр: spec/decisions/02-types.md D445 (AMEND 2026-08-12, §2/§3),
# docs/plans/221.1-bug-sweep.md №611/№615/№616.
#
# ПРАВИЛО: канон параметра — РОВНО три формы: "buf T", "mut buf T",
# "consume buf T". Постфикс "buf mut T" снят жёсткой ошибкой
# E_PARAM_TYPE_POS_MUT_RETRACTED, и компилятор держит её во всех четырёх
# позициях (обычная fn, объявление эффекта, объявление протокола, литерал
# обработчика — пробито 2026-08-16). ДОКА этой проверкой не покрыта: страж
# check-doc-examples.sh смотрит снятые формы ПРОТОКОЛОВ, а не параметров.
#
# ПОЧЕМУ страж нужен, если код чист. Замер 2026-08-16 по указанию владельца
# «это просто ошибка, её надо исправить как класс везде»: в коде ноль живых
# мест по 3717 файлам и шести репозиториям пакетов, а в ОПУБЛИКОВАННОМ
# руководстве форма жила — docs/guide/io-fs.md показывал канонический io.Read
# как "@read(buf mut []u8)" при настоящем "mut @read(mut buf []u8)". То есть
# читателя учили тому, что компилятор отвергает. Дока — единственная зона, где
# снятая форма может выжить, потому что её никто не компилирует.
#
# ХРАПОВИК, а не ноль: в docs/plans/** и docs/dev/** лежат ЗАКРЫТЫЕ планы
# (100.x-123.x), написанные когда форма была живой. Переписать их значило бы
# подделать запись о том, что решалось тогда. Исторический осадок зафиксирован
# базой, и расти ему нельзя.
#
# ЗОНЫ: docs/guide/** публикуется — там жёсткий ноль; docs/plans/**,
# docs/dev/**, spec/** — храповик по базе рядом.
#
# НЕ проверяет: .nv (там компилятор жёстче любого грепа) и прозу про сам
# запрет (строки со словами retract/снят/W_PARAM/E_PARAM исключены).
#
# $1 — корень репозитория (default: вычислить от себя).
#
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
NAME=check-retracted-param-form
BASE_FILE="$ROOT/scripts/guards/retracted-param-form.baseline"

# Совпадение обязано сидеть в списке параметров ОБЪЯВЛЕНИЯ ("fn " или "@имя("),
# иначе шаблон ловит прозу вроде "for mut x" и цепочки указателей "*mut mut T".
PAT='(fn |@[a-z_][A-Za-z0-9_]*)[^)]*\([^)]*\b[a-z_][A-Za-z0-9_]*[[:space:]]+(mut|consume)[[:space:]]+[\[\*A-Za-z]'
# Исключения: канон "(mut x T)", локальные "let mut", R2-split локала,
# detach/spawn-списки и проза про сам запрет.
EXC='\((mut|consume|ro)[[:space:]]|let mut|ro [a-z_]+ mut|detach|spawn|W_PARAM|E_PARAM|retract|снят|СНЯТ'

count_zone() {
    if [ ! -d "$ROOT/$1" ]; then echo 0; return; fi
    grep -rEn "$PAT" "$ROOT/$1" --include=*.md 2>/dev/null \
        | grep -vE "$EXC" | wc -l | tr -d ' '
}

if [ ! -f "$BASE_FILE" ]; then
    echo "$NAME: FAIL — нет файла базы $BASE_FILE" >&2
    exit 1
fi

GUIDE=$(count_zone docs/guide)
PLANS=$(count_zone docs/plans)
DEV=$(count_zone docs/dev)
SPEC=$(count_zone spec)

B_PLANS=$(grep -m1 '^plans=' "$BASE_FILE" | cut -d= -f2)
B_DEV=$(grep -m1 '^dev=' "$BASE_FILE" | cut -d= -f2)
B_SPEC=$(grep -m1 '^spec=' "$BASE_FILE" | cut -d= -f2)

FAILED=0

if [ "$GUIDE" -ne 0 ]; then
    echo "$NAME: FAIL — снятая форма параметра в ПУБЛИКУЕМОМ руководстве: $GUIDE" >&2
    grep -rEn "$PAT" "$ROOT/docs/guide" --include=*.md 2>/dev/null | grep -vE "$EXC" | head -5 >&2
    echo '  Канон — "mut buf T"; постфикс снят (D445 AMEND 2026-08-12, №611).' >&2
    FAILED=1
fi

check_ratchet() {
    if [ "$2" -gt "$3" ]; then
        echo "$NAME: FAIL — снятая форма в $1 ВЫРОСЛА: $2 > базы $3" >&2
        echo '  Исторический осадок закрытых планов трогать не надо, но НОВЫХ' >&2
        echo '  вхождений быть не может: пиши "mut buf T" (D445, №611).' >&2
        return 1
    fi
    if [ "$2" -lt "$3" ]; then
        echo "$NAME: осадок в $1 снизился ($2 < базы $3) — опусти базу в $BASE_FILE"
    fi
    return 0
}

check_ratchet "docs/plans" "$PLANS" "$B_PLANS" || FAILED=1
check_ratchet "docs/dev"   "$DEV"   "$B_DEV"   || FAILED=1
check_ratchet "spec"       "$SPEC"  "$B_SPEC"  || FAILED=1

if [ "$FAILED" -ne 0 ]; then
    exit 1
fi
echo "$NAME ok: публикуемое руководство чисто (0), осадок не растёт (plans $PLANS<=$B_PLANS, dev $DEV<=$B_DEV, spec $SPEC<=$B_SPEC)"
exit 0
