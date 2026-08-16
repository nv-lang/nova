#!/usr/bin/env bash
# scripts/gate-novac.sh — ОТДЕЛЬНЫЙ гейт самохостящегося компилятора novac.
# Запуск из корня целевого дерева:  bash scripts/gate-novac.sh
#
# ЗАЧЕМ ОТДЕЛЬНЫЙ (решение владельца 2026-08-16, вопрос «зачем текущему
# компилятору знать, что novac соблюдает свои конвенции?»).
#
# Стражи novac жили в `scripts/gate.sh` и гонялись БЕЗУСЛОВНО. Цена не в
# секундах (замер: 7 шагов из 53, ~16с), а в СВЯЗАННОСТИ двух областей отказа:
#
#   * гейт защищает ПОСТАВЛЯЕМЫЙ компилятор; novac в v0.1 не поставляется;
#   * `check-novac-legacy-workarounds` краснеет, когда маркер `[LEGACY-#NNN]`
#     указывает на ЗАКРЫТЫЙ баг. Баги закрывает интегратор — то есть, закрыв
#     дефект оракула, он делает маркер чужого окна устаревшим и блокирует
#     СВОЙ пуш работой, которой не делал;
#   * и наоборот: правка доки novac прогоняла весь компиляторный гейт с мега-CU.
#
# ═══ ПОРЯДОК ЗДЕСЬ НОРМАТИВЕН (класс F1, 274.3) ═══
# `novac-build` обязан идти ДО всех бинарь-зависимых стражей. Иначе они честно
# скажут «судить нечего» и выйдут нулём — и гейт станет зелёным НИ О ЧЁМ.
# Наступали: сутки блокера std serde прошли для гейта зелёными.
#
# ═══ ДВА РАЗНЫХ КРАСНЫХ, И ГЕЙТ ОБЯЗАН ИХ РАЗЛИЧАТЬ (реестр №693) ═══
# Красный «novac нарушил конвенцию» и красный «бинарь оракула разошёлся с
# заголовками рантайма» выглядят одинаково — `use of undeclared identifier` в
# сгенерированном Си. За смену 2026-08-16 класс сработал ТРИЖДЫ, и различал их
# только человек. Здесь их различает машина: вывод шага сверяется с сигнатурой
# рассинхрона, и такой отказ идёт отдельным счётчиком, с отдельным вердиктом и
# отдельным кодом возврата (2, не 1). Смысл: «это не твоя ветка виновата,
# пересоберись/дождись слияния рантайма» — вывод, который иначе делают руками.
#
# ═══ ШВЫ (переменные окружения) — ДЛЯ ДЕШЁВОЙ ВЫБОРКИ В ОКНЕ, НЕ ДЛЯ ГЕЙТА ═══
#   NOVAC_CORPUS=0         пропустить корпусный прогон (дорогой)
#   NOVAC_COST=0           пропустить храповик цены итерации
#   NOVAC_PROVE=0          пропустить мутационную проверку самотестов
#   NOVAC_PROVE_DEADLINE   секунд на один самотест под заглушкой (умолчание 150)
#   NOVAC_SMOKE_CACHE      каталог кэша смоука (бинарь оракула, argv clang, PCH)
# Прогон с любым установленным швом НЕ печатает слово `final` — по той же
# причине, по какой ноль без строки `ok:` не считается проверкой (№645):
# «зелено на выборке» и «зелено» — разные утверждения, и путать их нельзя.
#
# ═══ ДВА ПРЕДУПРЕЖДЕНИЯ ОКНА 274 (2026-08-16), ПРОВЕРЕНЫ И НОРМАТИВНЫ ═══
# 1. `check-novac-selftest-proves-red` подменяет файлы стражей на месте с
#    восстановлением по trap. Дедлайн обязан оставлять место TERM-обработке:
#    with-deadline.sh шлёт TERM и лишь через 10с KILL — trap успевает; НЕ
#    заменять на жёсткое убийство, иначе заглушка может пережить прогон.
# 2. `novac-iteration-cost.baseline` несёт cal-ms — МАШИННУЮ калибровку
#    (один и тот же novac check: 150мс на тихой машине, 3300мс под полным
#    гейтом). При переезде гейта на другую машину первую калибровку
#    перезаписать СОЗНАТЕЛЬНО, а не «чинить» пороги.
#
# ЧТО ЗДЕСЬ НЕ ПРОВЕРЯЕТСЯ: модульные тесты novac. Они живут в CI-работе
# `novac-gate` (.github/workflows/nova-gate.yml) — там есть оракул и системный
# компилятор. Сборка novac — здесь, потому что без неё бинарь-зависимые стражи
# судят пустоту (см. F1 выше).
set -u
ROOT="${1:-$(pwd)}"

GATE_FAILS=""
GATE_FAIL_N=0
DESYNC_MSGS=""
DESYNC_N=0
GATE_T0=$(date +%s)

# Швы: какие включены — говорим вслух и запоминаем для вердикта.
SEAMS=""
[ "${NOVAC_CORPUS:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_CORPUS=0"
[ "${NOVAC_COST:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_COST=0"
[ "${NOVAC_PROVE:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_PROVE=0"

step() {
    printf '[%5ds] == novac-gate: %s ==\n' "$(( $(date +%s) - GATE_T0 ))" "$1"
}
fail() {
    echo "NOVAC-GATE FAIL: $1" >&2
    GATE_FAILS="$GATE_FAILS
  * $1"
    GATE_FAIL_N=$(( GATE_FAIL_N + 1 ))
}
desync() {
    echo "NOVAC-GATE РАССИНХРОН: $1" >&2
    DESYNC_MSGS="$DESYNC_MSGS
  * $1"
    DESYNC_N=$(( DESYNC_N + 1 ))
}
# Сигнатура рассинхрона рантайм/оракул (№693): бинарь эмитит вызов функции,
# которой в заголовках ЭТОГО дерева ещё нет. Смотрим по тексту отказа, потому
# что отказ приходит от чужого компилятора (clang) и другого способа нет.
is_desync() {
    printf '%s\n' "$1" | grep -qE "undeclared identifier '?nova_|implicit declaration of function '?nova_|too few arguments to function call.*nova_|nova_[a-z_]+' undeclared"
}
# Тот же контракт, что в главном гейте (реестр №645): ноль без строки `ok:` —
# это «не упал», а не «проверил». Плюс классификация рассинхрона (№693).
guard() {
    local deadline=""
    if [ "$1" = "--deadline" ]; then deadline="$2"; shift 2; fi
    local g="$1"; shift
    local runner=bash out rc
    case "$g" in *.py) runner=python ;; esac
    if [ -n "$deadline" ]; then
        out="$(bash "$ROOT/scripts/tools/with-deadline.sh" "$deadline" "$runner" "$g" "$@" 2>&1)"; rc=$?
    else
        out="$("$runner" "$g" "$@" 2>&1)"; rc=$?
    fi
    printf '%s\n' "$out"
    if [ "$rc" -ne 0 ]; then
        if is_desync "$out"; then
            desync "$(basename "$g"): бинарь оракула зовёт функцию рантайма, которой нет в заголовках этого дерева"
            return 0
        fi
        return "$rc"
    fi
    printf '%s\n' "$out" | grep -q 'ok:' && return 0
    echo "ШАГ НИЧЕГО НЕ ДОКАЗАЛ: $(basename "$g") вышел с нулём, но не напечатал строку ok:" >&2
    echo "  Ноль без строки — это «не упал», а не «проверил» (реестр №645)." >&2
    return 1
}

# ── Стражи БЕЗ бинаря: текст, дока, форма исходника. Идут первыми — дёшевы. ──
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

# ── РУБЕЖ F1: дальше идут бинарь-зависимые. Бинарь строит ГЕЙТ. ──
step "novac-build (274.3/F1: бинарь novac строится ГЕЙТОМ — иначе «судить нечего» неотличимо от «зелено»)"
# Ревью трёх линз 2026-08-15 (274.3, класс К-A): все бинарь-зависимые стражи novac
# начинались с «нет бинаря — ok, судить нечего», а гейт бинарь не строил; сутки
# блокера std serde прошли для гейта зелёными. Теперь: если novac/src/main.nv
# существует, гейт ОБЯЗАН собрать novac; провал сборки — красный (это регресс
# оракула по подмножеству novac либо регресс novac — оба требуют глаз, не тишины).
if [ -f "$ROOT/novac/src/main.nv" ]; then
    NOVA_BIN="$ROOT/nova-cli/target/release/nova.exe"
    [ -f "$NOVA_BIN" ] || NOVA_BIN="$ROOT/nova-cli/target/release/nova"
    if [ -f "$NOVA_BIN" ]; then
        mkdir -p "$ROOT/target" "$ROOT/novac/target"
        if ! bash "$ROOT/scripts/tools/with-deadline.sh" 300 "$NOVA_BIN" build "$ROOT/novac/src/main.nv" -o "$ROOT/novac/target/novac.exe" >"$ROOT/target/novac-build.log" 2>&1; then
            BUILD_OUT="$(cat "$ROOT/target/novac-build.log" 2>/dev/null || true)"
            if is_desync "$BUILD_OUT"; then
                desync "novac-build: оракул эмитит вызов рантайма, которого нет в заголовках этого дерева — см. target/novac-build.log"
            else
                fail "novac не собирается текущим оракулом (274.3/F1) - см. target/novac-build.log; регресс оракула по подмножеству novac или регресс novac"
            fi
        fi
    else
        fail "оракул nova-cli/target/release/nova не собран — novac нечем строить (274.3/F1)"
    fi
else
    fail "novac/src/main.nv не найден: гейт novac запущен не из корня дерева novac (274.3/F1)"
fi

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

# ═══ НАБОР ОКНА 274 — влит слиянием 2026-08-16 вместе с файлами ═══
# Порядок: дешёвые статические — потом дедлайновые — потом мутационный —
# реестр стражей последним (судит сам набор).
step "novac-conventions (П13..П27: доки, имена, реестры, двери, доноры)"
guard "$ROOT/scripts/guards/check-novac-type-field-docs.sh" "$ROOT" || fail "тип/поле/функция novac без документации (П13)"
guard "$ROOT/scripts/guards/check-novac-doc-language.sh" "$ROOT" || fail "русский текст в .nv novac (П13)"
guard "$ROOT/scripts/guards/check-novac-no-name-hardcode.sh" "$ROOT" || fail "имя языка/std строкой вне builtins (П5)"
guard "$ROOT/scripts/guards/check-novac-no-prelude-shadow.sh" "$ROOT" || fail "novac объявил имя, которое экспортирует прелюдия"
guard "$ROOT/scripts/guards/check-novac-ctx-tables.sh" "$ROOT" || fail "таблица строк в Ctx без строки плана §10.3б (П17)"
guard "$ROOT/scripts/guards/check-novac-row-fields.sh" "$ROOT" || fail "поле строки реестра без записи в §10.3в (П22/П23)"
guard "$ROOT/scripts/guards/check-novac-ref-field-names.sh" "$ROOT" || fail "поле-ссылка без суффикса пространства (П19)"
guard "$ROOT/scripts/guards/check-novac-no-alloc-in-lookup.sh" "$ROOT" || fail "дверь поиска аллоцирует (П18)"
guard "$ROOT/scripts/guards/check-novac-ice-messages.sh" "$ROOT" || fail "текст ice() повторяется или без модуля (П20)"
guard "$ROOT/scripts/guards/check-novac-no-default-branch.sh" "$ROOT" || fail "ветка «всё остальное» на закрытом множестве (П21)"
guard "$ROOT/scripts/guards/check-novac-mangling-one-way.sh" "$ROOT" || fail "C-имя разбирается обратно (П24)"
guard "$ROOT/scripts/guards/check-novac-cli-surface.sh" "$ROOT" || fail "команда novac, которой нет у nova-cli (П26)"
guard "$ROOT/scripts/guards/check-novac-effects-at-door.sh" "$ROOT" || fail "способность ниже двери (П15)"
guard "$ROOT/scripts/guards/check-novac-one-door-export.sh" "$ROOT" || fail "одна операция из двух модулей (274.1 §2в)"
guard "$ROOT/scripts/guards/check-novac-edge-payload.sh" "$ROOT" || fail "ребро §3 без «что течёт» (274.1 §2в)"
guard "$ROOT/scripts/guards/check-novac-surface.sh" "$ROOT" || fail "публичная поверхность разошлась с базой (274 §10.4)"
guard "$ROOT/scripts/guards/check-novac-temp-edges.sh" "$ROOT" || fail "временное ребро без срока или истекло (274.1 §2в)"
guard "$ROOT/scripts/guards/check-novac-module-donor.sh" "$ROOT" || fail "модуль novac без донора-указателя в заголовке (П27)"
guard "$ROOT/scripts/guards/check-novac-commit-donor.sh" /dev/null "$ROOT" || fail "check-novac-commit-donor не отвечает на пустом входе"
guard "$ROOT/scripts/guards/check-novac-resolve-discipline.sh" "$ROOT" || fail "резолв с тихим дефолтом или линейным сканом имён"
guard "$ROOT/scripts/guards/check-novac-channel-one-writer.sh" "$ROOT" || fail "у канала чекера второй писатель или вывод типа ниже чекера"
guard "$ROOT/scripts/guards/check-novac-match-exhaustive.sh" "$ROOT" || fail "match по сумме novac не покрывает все варианты (оракул это не ловит)"
guard "$ROOT/scripts/guards/check-novac-no-silent-skip.sh" "$ROOT" || fail "ветка прохода канала ушла молча (ни записи, ни отказа, ни ice)"
guard "$ROOT/scripts/guards/check-novac-pch.sh" "$ROOT" || fail "PCH исчез из горячего пути (274.2 §1а)"
guard "$ROOT/scripts/guards/check-novac-conventions-coverage.sh" "$ROOT" || fail "правило конвенции без названного механизма"
step "novac-lint (свод nv-coding-style по novac/src)"
guard --deadline 300 "$ROOT/scripts/guards/check-novac-lint.sh" "$ROOT" || fail "nova lint нашёл замечания в novac/src"
step "novac-heavy (дедлайновые: мэнглинг, шаблон, цена, мутационная проверка самотестов)"
guard --deadline 300 "$ROOT/scripts/guards/check-novac-mangle-fixed-point.sh" "$ROOT" || fail "мэнгл novac разошёлся с оракулом"
guard --deadline 300 "$ROOT/scripts/guards/check-novac-shell-freshness.sh" "$ROOT" || fail "shell.tpl.c протух"
guard --deadline 600 "$ROOT/scripts/guards/check-novac-iteration-cost.sh" "$ROOT" || fail "цена цикла вышла из бюджета (П14)"
guard --deadline 600 "$ROOT/scripts/guards/check-novac-selftest-proves-red.sh" "$ROOT" || fail "самотест стража novac проходит над заглушкой (П16)"
step "novac-registry (реестр стражей: план ↔ файлы ↔ вызовы ↔ самотесты)"
guard "$ROOT/scripts/guards/check-novac-guard-registry.sh" "$ROOT" || fail "реестр стражей novac разошёлся"

# Рубеж ПЕРЕД вердиктом — иначе красный прогон печатает зелёную строку (№690).
if [ "$GATE_FAIL_N" -gt 0 ]; then
    echo "" >&2
    echo "NOVAC-GATE: отказов novac — $GATE_FAIL_N:$GATE_FAILS" >&2
    [ "$DESYNC_N" -gt 0 ] && echo "  (плюс рассинхронов рантайм/оракул: $DESYNC_N — см. выше)" >&2
    exit 1
fi
if [ "$DESYNC_N" -gt 0 ]; then
    echo "" >&2
    echo "NOVAC-GATE BLOCKED: рассинхрон рантайм/оракул — $DESYNC_N:$DESYNC_MSGS" >&2
    echo "  Это НЕ нарушение конвенций novac. Бинарь оракула и заголовки рантайма" >&2
    echo "  взяты из РАЗНЫХ деревьев (реестр №693). Судить novac нечем: бинарь-" >&2
    echo "  зависимые стражи не отработали. Лечится слиянием рантайма, не правкой novac." >&2
    exit 2
fi
if [ -n "$SEAMS" ]; then
    echo "NOVAC-GATE OK (ВЫБОРКА, швы:$SEAMS — это не полный прогон)"
    exit 0
fi
echo "NOVAC-GATE OK (final)"
exit 0
