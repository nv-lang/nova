#!/bin/sh
# scripts/guards/check-novac-grammar-fixture-coverage.sh — карта «форма
# грамматики × наблюдающие фикстуры» для novac.
#
# ПРАВИЛО (архитектура docs/dev/novac-architecture.md §16, класс К7
# «полуготовый механизм: возможность без обязательств»; план 274 §10.3):
# каждая форма грамматики обязана иметь фикстуры, НАБЛЮДАЮЩИЕ обязательство —
# positive (эффект виден) И negative (отказ виден). Дифф-базный
# check-test-fixture-coverage.sh этому классу не судья: он слеп к слиянию без
# новых кодов диагностики — ровно там, где К7 и рождается.
#
# ПРОВЕРЯЕТ: реестр форм — novac/grammar-forms.txt (по строке 'имя-формы';
# пустые строки и строки на '#' — не формы). Каждой форме — каталог
# novac/fixtures/<имя-формы>/ с минимум одним pos_*.nv И одним neg_*.nv.
# Форма без каталога или без любой половины пары — красная.
#
# НЕ ПРОВЕРЯЕТ: содержательность фикстур (что pos действительно наблюдает
# эффект, а neg — отказ; это приёмка, прецедент слабой фикстуры — №465);
# полноту самого реестра (что в нём перечислены ВСЕ формы грамматики — это
# приёмка Э1); имена фикстур сверх префиксов pos_/neg_.
#
# Аргумент $1 — корень репозитория (по умолчанию — вычислить от себя);
# $2 — override директории novac (реестр + fixtures/) для самотеста.
#
# Нет файла реестра ИЛИ он пуст — зелёный «судить нечего»: страж до реестра
# легален, молчание нелегально.
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
NOVAC="${2:-$ROOT/novac}"
REG="$NOVAC/grammar-forms.txt"
FIX="$NOVAC/fixtures"

if [ ! -f "$REG" ]; then
    echo "check-novac-grammar-fixture-coverage ok: судить нечего (нет реестра форм novac/grammar-forms.txt; форм 0)"
    exit 0
fi

FORMS=$(tr -d '\r' < "$REG" | sed -e 's/[[:space:]]*$//' -e '/^[[:space:]]*$/d' -e '/^#/d')
if [ -z "$FORMS" ]; then
    echo "check-novac-grammar-fixture-coverage ok: судить нечего (реестр форм пуст; форм 0)"
    exit 0
fi

BAD=""
n=0
while IFS= read -r form; do
    [ -n "$form" ] || continue
    n=$((n+1))
    dir="$FIX/$form"
    if [ ! -d "$dir" ]; then
        BAD="$BAD  $form — нет каталога фикстур novac/fixtures/$form/
"
        continue
    fi
    pos=0
    neg=0
    for p in "$dir"/pos_*.nv; do
        if [ -f "$p" ]; then pos=1; break; fi
    done
    for p in "$dir"/neg_*.nv; do
        if [ -f "$p" ]; then neg=1; break; fi
    done
    miss=""
    [ "$pos" -eq 1 ] || miss="$miss pos_*.nv"
    [ "$neg" -eq 1 ] || miss="$miss neg_*.nv"
    if [ -n "$miss" ]; then
        BAD="$BAD  $form — не хватает:$miss
"
    fi
done <<EOF
$FORMS
EOF

if [ -n "$BAD" ]; then
    echo "check-novac-grammar-fixture-coverage: FAIL — формы грамматики без пары наблюдающих фикстур:" >&2
    printf '%s' "$BAD" >&2
    echo "  Каждой форме из novac/grammar-forms.txt — каталог novac/fixtures/<форма>/" >&2
    echo "  с минимум одной pos_*.nv (эффект виден) И одной neg_*.nv (отказ виден)." >&2
    echo "  Архитектура §16 К7; план 274 §10.3." >&2
    exit 1
fi

echo "check-novac-grammar-fixture-coverage ok: форм $n, у каждой пара pos_*.nv + neg_*.nv"
exit 0
