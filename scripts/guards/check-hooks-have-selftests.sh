#!/usr/bin/env bash
# scripts/guards/check-hooks-have-selftests.sh
# Каждый хук среды агента обязан иметь самотест.
#
# ДОМ И ОСНОВАНИЕ: план 276 шаг 6; реестр 221.1 №774.
#
# ЗАЧЕМ. Хук — тот же страж, только стоит РАНЬШЕ: до записи файла, до запуска
# команды, до конца хода. И ломается он так же молча, а заметен ещё меньше —
# руками его никто не запускает. Замер 2026-08-29 это подтвердил делом: у
# `guard-stop.py` самотест появился впервые, и ПЕРВЫЙ ЖЕ его прогон нашёл, что
# хук под локалью гейта не может напечатать свой вердикт и потому МОЛЧА
# пропускает нарушение (реестр №788). Три хука до того дня жили без тестов, а
# в шапке `guard-memory.py` прямо стояло «самотеста ПОКА НЕТ».
#
# ПРАВИЛО. Для хука `scripts/claude-hooks/X.py` обязан существовать хотя бы один
# файл `scripts/claude-hooks/selftest/test-X*.py`. Звёздочка на конце — потому
# что один хук может покрываться НЕСКОЛЬКИМИ тестами по темам: у `guard-git.py`
# их три (`-commit-backtick`, `-commit-scope`, `-powershell`), и требовать
# ровно одно имя значило бы принуждать к слиянию несвязанных проверок в файл.
#
# ГДЕ ЛЕЖАТ САМОТЕСТЫ. Рядом с хуками (`scripts/claude-hooks/selftest/`), а не в
# `scripts/guards/selftest/`. План 276 шаг 6 написан со вторым адресом, но в
# дереве самотесты хуков лежали по первому ещё до плана, и переносить их значило
# бы ломать шаг гейта, который их и находит глобом. Дерево старше текста —
# страж идёт за деревом, а расхождение записано здесь, чтобы читатель плана не
# счёл это ошибкой.
#
# ЧЕГО НЕ ПРОВЕРЯЕТ (честно): что самотест ОСМЫСЛЕН и умеет краснеть. Это
# требование П16 и предмет самого самотеста; машина видит наличие файла.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-hooks-have-selftests.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-hooks-have-selftests.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || { echo "check-hooks-have-selftests: нет каталога $ROOT" >&2; exit 1; }

HOOKS="scripts/claude-hooks"
TESTS="$HOOKS/selftest"

if [ ! -d "$HOOKS" ]; then
    echo "check-hooks-have-selftests ok: судить нечего — каталога $HOOKS нет"
    exit 0
fi

MISSING=""
N=0
for h in "$HOOKS"/*.py; do
    [ -f "$h" ] || continue
    name=$(basename "$h" .py)
    N=$((N + 1))
    found=0
    for t in "$TESTS/test-$name"*.py; do
        [ -f "$t" ] && { found=1; break; }
    done
    [ "$found" -eq 1 ] || MISSING="$MISSING $name"
done

nmiss=0
for m in $MISSING; do nmiss=$((nmiss + 1)); done

echo "check-hooks-have-selftests: хуков $N, без самотеста $nmiss"

if [ "$nmiss" -ne 0 ]; then
    echo "check-hooks-have-selftests: НАРУШЕНИЕ — хук без самотеста:" >&2
    for m in $MISSING; do echo "    $HOOKS/$m.py" >&2; done
    echo "" >&2
    echo "    Хук стоит РАНЬШЕ стража — до записи, до команды, до конца хода," >&2
    echo "    и ломается так же молча, но заметен меньше: руками его не гоняют." >&2
    echo "    Заведи $TESTS/test-<имя>.py, доказывающий ОБЕ стороны:" >&2
    echo "    нарушение ловится, законный случай проходит (П16)." >&2
    echo "check-hooks-have-selftests: FAIL" >&2
    exit 1
fi

echo "check-hooks-have-selftests ok: у каждого хука есть самотест"
exit 0
