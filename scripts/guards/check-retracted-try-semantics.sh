#!/usr/bin/env bash
# Страж: осадок СНЯТОЙ трактовки оператора `?` не растёт, а в публикуемом
# руководстве его нет вовсе.
#
# ЗАЧЕМ. D85 сделал `?` return-стилем (законен только в функции, возвращающей
# `Result`/`Option`; в функции, объявившей `Fail`, — `E_TRY_IN_FAIL_FN`).
# Тексты, написанные до D85, описывают `?` как сахар над `throw`. Аудит
# самосогласованности 2026-08-16 нашёл четыре таких места и ПОМЕТИЛ их руками —
# и ровно эта ручная пометка промахнулась дважды из четырёх (см. врезку
# «ПРОВЕРКА СОБСТВЕННОЙ РАЗМЕТКИ» в отчёте аудита). Отсюда правило: группа
# закрывается не пометкой, а ИЗМЕРЕНИЕМ. Это тот же приём, что у
# `check-retracted-param-form` (D445 №611), только для другой снятой формы.
#
# ПОЧЕМУ ЯДРО НА ПИТОНЕ. Структурное семейство («`?` внутри Fail-функции»)
# разнесено между строкой подписи и строкой тела — grep такое не видит в
# принципе. Тот же довод, что у `registry-routes-scan.py` (№645: ноль без
# строки `ok:` — это «не упал», а не «проверил»).
#
# Реестр 221.1 №713 (руководство учило коду, который не компилируется),
# реестр 221.1 №442 (класс: примеры спеки не проверяются компилятором).
# План: docs/plans/wip/spec-consistency-audit-2026-08-16.md (раздел 3, группа A).
set -u

NAME="check-retracted-try-semantics"
ROOT="${1:-.}"
SCAN="$ROOT/scripts/guards/retracted-try-scan.py"
BASE_FILE="$ROOT/scripts/guards/retracted-try.baseline"

if [ ! -f "$SCAN" ]; then
    echo "$NAME: FAIL — нет ядра $SCAN" >&2
    exit 1
fi
if [ ! -f "$BASE_FILE" ]; then
    echo "$NAME: FAIL — нет файла базы $BASE_FILE" >&2
    exit 1
fi

OUT=$(python "$SCAN" "$ROOT" 2>&1)
if [ $? -ne 0 ]; then
    echo "$NAME: FAIL — ядро не отработало:" >&2
    echo "$OUT" >&2
    exit 1
fi

val() { echo "$OUT" | grep -m1 "^$1=" | cut -d= -f2; }
base() { grep -m1 "^$1=" "$BASE_FILE" | cut -d= -f2; }

GUIDE=$(val guide)
SPEC=$(val spec)
PLANS=$(val plans)
DEV=$(val dev)

for v in "$GUIDE" "$SPEC" "$PLANS" "$DEV"; do
    case "$v" in
        ''|*[!0-9]*) echo "$NAME: FAIL — ядро вернуло не число: '$v'" >&2; exit 1;;
    esac
done

# ── Публикуемая зона: ноль, без храповика ────────────────────────────────────
# Здесь живёт то, что читает пользователь. Пример, который не компилируется,
# дороже отсутствующего примера — поэтому база для guide не заводится вовсе.
if [ "$GUIDE" -ne 0 ]; then
    echo "$NAME: FAIL — снятая трактовка \`?\` в ПУБЛИКУЕМОМ руководстве: $GUIDE" >&2
    python "$SCAN" "$ROOT" --list 2>/dev/null | grep "docs/guide" | head -5 >&2
    echo "    Норма — D85: \`?\` только в fn, возвращающей Result/Option; в" >&2
    echo "    Fail-функции проброс через \`!!\`/\`throw\`. Форма D196" >&2
    echo "    'consume X = expr? { body }' законна и стражем НЕ считается." >&2
    exit 1
fi

# ── Исторические зоны: храповик вниз ─────────────────────────────────────────
FAILED=0
check_ratchet() {
    local zone="$1" now="$2" b="$3"
    case "$b" in
        ''|*[!0-9]*) echo "$NAME: FAIL — в базе нет числа для '$zone'" >&2; FAILED=1; return;;
    esac
    if [ "$now" -gt "$b" ]; then
        echo "$NAME: FAIL — снятая трактовка \`?\` в $zone ВЫРОСЛА: $now > базы $b" >&2
        python "$SCAN" "$ROOT" --list 2>/dev/null | grep "$zone/" | head -5 >&2
        echo "    Рост законен только вместе со строкой-летописью в $BASE_FILE." >&2
        FAILED=1
        return
    fi
    if [ "$now" -lt "$b" ]; then
        echo "$NAME: осадок в $zone снизился ($now < базы $b) — опусти базу в $BASE_FILE"
    fi
}

check_ratchet "spec" "$SPEC" "$(base spec)"
check_ratchet "docs/plans" "$PLANS" "$(base plans)"
check_ratchet "docs/dev" "$DEV" "$(base dev)"

[ "$FAILED" -ne 0 ] && exit 1

echo "$NAME ok: публикуемое руководство чисто (0), осадок не растёт (spec $SPEC<=$(base spec), plans $PLANS<=$(base plans), dev $DEV<=$(base dev))"
exit 0
