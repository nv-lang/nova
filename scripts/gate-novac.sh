#!/bin/sh
# scripts/gate-novac.sh — ОТДЕЛЬНЫЙ гейт самохостящегося компилятора.
# Запуск из корня целевого дерева:  bash scripts/gate-novac.sh
#
# ЗАЧЕМ ОТДЕЛЬНЫЙ (решение владельца 2026-08-16, вопрос «зачем текущему
# компилятору знать, что novac соблюдает свои конвенции?»).
#
# До этого шестнадцать novac-стражей жили в `scripts/gate.sh` и гонялись
# БЕЗУСЛОВНО — замер того же дня: 7 шагов из 53, порядка 16 секунд. Цена не в
# секундах, а в СВЯЗАННОСТИ двух областей отказа:
#
#   * гейт защищает ПОСТАВЛЯЕМЫЙ компилятор; novac в v0.1 не поставляется;
#   * `check-novac-legacy-workarounds` краснеет, когда маркер `[LEGACY-#NNN]`
#     указывает на ЗАКРЫТЫЙ баг. Баги закрывает интегратор — то есть, закрыв
#     дефект оракула, он делает маркер чужого окна устаревшим и блокирует
#     СВОЙ пуш работой, которой не делал;
#   * и наоборот: правка novac прогоняла весь компиляторный гейт, включая
#     мега-CU, ради проверок доки.
#
# Разделение восстанавливает соответствие «кто сломал — тому и красное».
#
# ЧТО ЗДЕСЬ НЕ ПРОВЕРЯЕТСЯ: сборка самого novac и его модульные тесты. Они
# живут в CI-работе `novac-gate` (.github/workflows/nova-gate.yml) — там есть
# оракул и системный компилятор. Здесь только конвенции, как и было.
set -u
ROOT="${1:-$(pwd)}"

GATE_FAILS=""
GATE_FAIL_N=0
GATE_T0=$(date +%s)

step() {
    printf '[%5ds] == novac-gate: %s ==\n' "$(( $(date +%s) - GATE_T0 ))" "$1"
}
fail() {
    echo "NOVAC-GATE FAIL: $1" >&2
    GATE_FAILS="$GATE_FAILS
  * $1"
    GATE_FAIL_N=$(( GATE_FAIL_N + 1 ))
}
# Тот же контракт, что в главном гейте (реестр №645): ноль без строки `ok:` —
# это «не упал», а не «проверил».
guard() {
    g="$1"; shift
    out="$(bash "$g" "$@" 2>&1)"; rc=$?
    printf '%s\n' "$out"
    [ "$rc" -eq 0 ] || return "$rc"
    printf '%s\n' "$out" | grep -q 'ok:' && return 0
    echo "ШАГ НИЧЕГО НЕ ДОКАЗАЛ: $(basename "$g") вышел с нулём, но не напечатал строку ok:" >&2
    return 1
}

step "novac-arch-class-proofs (три доказательства у каждого класса — 274.1)"
guard "$ROOT/scripts/guards/check-novac-arch-class-proofs.sh" "$ROOT" || fail "класс в архитектуре novac без трёх доказательств (274.1, владелец 2026-08-14)"
step "novac-arch-invariants (счётчик инвариантов у разделов карты)"
guard "$ROOT/scripts/guards/check-novac-arch-invariants.sh" "$ROOT" || fail "раздел карты архитектуры novac без счётчика инвариантов (274.1 §2б)"
step "novac-no-naked-panic (явный инвариант — через дверь ice(), П12)"
guard "$ROOT/scripts/guards/check-novac-no-naked-panic.sh" "$ROOT" || fail "голый panic( в novac/src вне двери ice() (конвенция novac П12.1)"
step "novac-legacy-workarounds (форма обхода багов оракула — 274 §1.5)"
guard "$ROOT/scripts/guards/check-novac-legacy-workarounds.sh" "$ROOT" || fail "обход бага оракула в novac без маркера/с закрытым багом (274 §1.5)"
step "novac-time-ledger (доля 274/221 из леджера, не по памяти — 274 §1.4)"
guard "$ROOT/scripts/guards/check-novac-time-ledger.sh" "$ROOT" || fail "коммит в novac/** без строки в леджере времени (274 §1.4)"
step "novac-deps (рёбра только из таблицы §3 архитектуры)"
guard "$ROOT/scripts/guards/check-novac-deps.sh" "$ROOT" || fail "импорт в novac/src вне таблицы рёбер (архитектура §3, класс К4)"
step "novac-guards (Э1-набор: файл/атомики/ключи/глобалы/форма/фикстуры + бинарь-четвёрка)"
guard "$ROOT/scripts/guards/check-novac-file-size.sh" "$ROOT" || fail "файл novac длиннее 1000 строк (решение 12)"
guard "$ROOT/scripts/guards/check-novac-atomics-door.sh" "$ROOT" || fail "атомики/TLS мимо одной двери (274 §8.1)"
guard "$ROOT/scripts/guards/check-novac-no-string-keys.sh" "$ROOT" || fail "строковый ключ таблицы вне names (архитектура §4а, К2)"
guard "$ROOT/scripts/guards/check-novac-no-global-state.sh" "$ROOT" || fail "глобальное изменяемое состояние в novac (274 §4 п.5)"
guard "$ROOT/scripts/guards/check-novac-frontend-shape.sh" "$ROOT" || fail "Result в сигнатуре фронтенда novac (274 §4 п.1)"
guard "$ROOT/scripts/guards/check-novac-grammar-fixture-coverage.sh" "$ROOT" || fail "форма грамматики без наблюдающих фикстур (К7)"
guard "$ROOT/scripts/guards/check-novac-differential.sh" "$ROOT" || fail "расхождение novac с оракулом вне реестра (дифф-гейт)"
guard "$ROOT/scripts/guards/check-novac-diag-schema.sh" "$ROOT" || fail "диагностика novac не по схеме §7"
guard "$ROOT/scripts/guards/check-novac-no-cascade.sh" "$ROOT" || fail "каскад диагностик от одной причины (274 §6)"
guard "$ROOT/scripts/guards/check-novac-no-panic.sh" "$ROOT" || fail "паника/крэш novac на фикстурах (решение 11: ноль паник)"

# Рубеж ПЕРЕД вердиктом — иначе красный прогон печатает зелёную строку (№690).
if [ "$GATE_FAIL_N" -gt 0 ]; then
    echo "" >&2
    echo "NOVAC-GATE: отказов — $GATE_FAIL_N:$GATE_FAILS" >&2
    exit 1
fi
echo "NOVAC-GATE OK (final)"
exit 0
