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
# УРОВЕНЬ прогона (2026-08-19, по замеру: полный гейт 584с, из них 238с --
# мутационная проверка САМИХ стражей, а не компилятора).
#   loop — только правила по novac/src + сборка + модульные тесты (~80с);
#   push — плюс поведение: дифф с оракулом, паники, пачка, фаззер (по умолчанию);
#   full — плюс мутационный самотест стражей и замер цены цикла (CI).
NOVAC_TIER="${NOVAC_TIER:-push}"
case "$NOVAC_TIER" in
    loop|push|full) : ;;
    *) echo "NOVAC_TIER: loop | push | full (дано: $NOVAC_TIER)" >&2; exit 2 ;;
esac

SEAMS=""
[ "${NOVAC_CORPUS:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_CORPUS=0"
[ "${NOVAC_COST:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_COST=0"
[ "${NOVAC_PROVE:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_PROVE=0"

# КАЛИБРОВКА МАШИНЫ: во сколько раз она сейчас медленнее эталонной.
# Замеряется ОДИН раз на прогон -- сама проба стоит секунды, и повторять её
# перед каждым дедлайном значило бы платить за измерение дороже, чем за
# измеряемое. Смысл числа -- в `scripts/tools/cal-factor.sh`: пределы стояли
# константами, а пропускная способность машины меняется в разы от посторонней
# нагрузки, и ложный отказ по таймауту стоит сорокаминутного перезапуска и
# учит не верить гейту.
CAL=$(bash "$ROOT/scripts/tools/cal-factor.sh" 2>/dev/null || echo 1)
case "$CAL" in ''|*[!0-9]*) CAL=1 ;; esac
[ "$CAL" -ge 1 ] || CAL=1

# ПОЛ по имени стража: сколько секунд он получает ВСЕГДА, независимо от
# калибровки. Пол существует там, где известна нижняя граница честной работы:
# фаззер прогоняет 462 мутации, самотесты -- полсотни стражей с мутацией
# каждого. На быстрой машине (CAL=1) пол не даёт срезать предел до значения,
# при котором страж не успевает доказать НИЧЕГО, а красный по таймауту читается
# как «сломано» -- то есть как ложь.
floor_for() {
    case "$1" in
        check-novac-fuzz-zero-panic.sh)      echo 300 ;;
        check-novac-selftest-proves-red.sh)  echo 600 ;;
        check-novac-iteration-cost.sh)       echo 600 ;;
        check-novac-mangle-fixed-point.sh)   echo 300 ;;
        *)                                   echo 0 ;;
    esac
}

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
        # Предел = написанное число, умноженное на калибровку, но не ниже
        # именного пола. Написанное число остаётся смыслом («столько это
        # стоит на эталонной машине»), а машина сама говорит, во сколько раз
        # она сегодня медленнее.
        _fl=$(floor_for "$(basename "$g")")
        deadline=$(( deadline * CAL ))
        [ "$deadline" -ge "$_fl" ] || deadline=$_fl
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

# ПАРАЛЛЕЛЬНЫЙ блок независимых текстовых стражей (2026-08-19).
#
# Они не зависят друг от друга и читают одно и то же дерево только на чтение.
# Последовательно это ~45 запусков по 3-5 секунд = две с половиной минуты
# ожидания человеком; параллельно — секунды. Вывод собирается в файлы и
# ПЕЧАТАЕТСЯ В ПОРЯДКЕ ОБЪЯВЛЕНИЯ: недетерминированный порядок красных строк
# читался бы как разные прогоны.
PAR_DIR="${TMPDIR:-/tmp}/novac-gate-par.$$"
PAR_N=0
par_reset() { rm -rf "$PAR_DIR"; mkdir -p "$PAR_DIR"; PAR_N=0; }
par_add() {
    PAR_N=$((PAR_N + 1))
    printf '%s\n' "$1" > "$PAR_DIR/$PAR_N.cmd"
    printf '%s\n' "$2" > "$PAR_DIR/$PAR_N.msg"
}
par_run() {
    # ПРЕДЕЛ одновременности: запуск всех разом на восьми ядрах не ускоряет,
    # а толкается -- каждый страж сам порождает процессы. Число берётся из
    # NOVAC_JOBS, по умолчанию 8.
    _jobs="${NOVAC_JOBS:-8}"
    # ВСЕ питоновские стражи -- ОДНИМ процессом. Интерпретатор стартует 73мс
    # (замер 2026-08-19), работа самого стража -- 40..60мс: на сорока стражах
    # старт стоит дороже проверки. Раннер пишет те же N.out и N.rc, поэтому
    # разбор ниже не знает, кто их написал.
    # `read` — ВСТРОЕННАЯ команда: строка читается без порождения процесса.
    # Прежняя редакция звала `cat` на каждый .cmd, и разбор очереди стоил
    # дороже работы, которую он разбирает (замер 2026-08-19: 10с на блоке,
    # где стражи считаются за 1.7с).
    _py=""
    _i=1
    while [ "$_i" -le "$PAR_N" ]; do
        read -r _g < "$PAR_DIR/$_i.cmd"
        case "$_g" in *.py) _py="$_py $_i" ;; esac
        _i=$((_i + 1))
    done
    if [ -n "$_py" ]; then
        # shellcheck disable=SC2086
        python "$ROOT/scripts/guards/run-guards.py" "$ROOT" "$PAR_DIR" $_py &
    fi
    _i=1
    _running=0
    while [ "$_i" -le "$PAR_N" ]; do
        read -r _g < "$PAR_DIR/$_i.cmd"
        case "$_g" in *.py) _i=$((_i + 1)); continue ;; esac
        ( bash "$_g" "$ROOT" > "$PAR_DIR/$_i.out" 2>&1; echo $? > "$PAR_DIR/$_i.rc" ) &
        _running=$((_running + 1))
        if [ "$_running" -ge "$_jobs" ]; then
            wait
            _running=0
        fi
        _i=$((_i + 1))
    done
    wait
    _i=1
    while [ "$_i" -le "$PAR_N" ]; do
        # .out читается ОДИН раз в переменную: печать, поиск строки ok: и
        # разбор рассинхрона идут по ней, а не тремя процессами по файлу.
        _out=$(cat "$PAR_DIR/$_i.out" 2>/dev/null)
        [ -n "$_out" ] && printf '%s\n' "$_out"
        _rc=1
        [ -f "$PAR_DIR/$_i.rc" ] && read -r _rc < "$PAR_DIR/$_i.rc"
        read -r _msg < "$PAR_DIR/$_i.msg"
        read -r _cmd < "$PAR_DIR/$_i.cmd"
        _base=${_cmd##*/}
        if [ "$_rc" -ne 0 ]; then
            if is_desync "$_out"; then
                desync "$_base: рассинхрон рантайма"
            else
                fail "$_msg"
            fi
        else
            case "$_out" in
                *ok:*) : ;; # зелёный со строкой доказательства
                *)
                    echo "ШАГ НИЧЕГО НЕ ДОКАЗАЛ: $_base вышел с нулём, но не напечатал 'ok:'" >&2
                    fail "$_msg"
                    ;;
            esac
        fi
        _i=$((_i + 1))
    done
    rm -rf "$PAR_DIR"
}

# ── Стражи БЕЗ бинаря: текст, дока, форма исходника. Идут первыми — дёшевы. ──
# Восемь стражей до рубежа F1 шли ПО ОДНОМУ: восемь стартов процесса строго
# по очереди ради восьми заголовков в логе. Пачка даёт то же самое — своё
# сообщение об отказе и своя строка ok: у каждого, — но питоновские идут ОДНИМ
# процессом (run-guards.py), а shell-овые параллельно.
step "novac-text (карта, маркеры, леджер, рёбра — до рубежа F1, бинарь не нужен)"
par_reset
par_add "$ROOT/scripts/guards/check-novac-arch-class-proofs.py" "класс в архитектуре novac без трёх доказательств (274.1, владелец 2026-08-14)"
par_add "$ROOT/scripts/guards/check-novac-arch-invariants.py" "раздел карты архитектуры novac без счётчика инвариантов (274.1 §2б)"
par_add "$ROOT/scripts/guards/check-novac-no-naked-panic.py" "голый panic( в novac/src вне двери ice() (конвенция novac П12.1)"
par_add "$ROOT/scripts/guards/check-novac-legacy-workarounds.py" "обход бага оракула в novac без маркера/с закрытым багом (274 §1.5)"
par_add "$ROOT/scripts/guards/check-guard-honesty.py" "страж может соврать или промолчать вместо проверки"
par_add "$ROOT/scripts/guards/check-novac-plan-liveline.py" "живая строка плана отстала от кода"
par_add "$ROOT/scripts/guards/check-novac-time-ledger.py" "коммит в novac/** без строки в леджере времени (274 §1.4)"
par_add "$ROOT/scripts/guards/check-novac-deps.py" "импорт в novac/src вне таблицы рёбер (архитектура §3, класс К4)"
par_run

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
        # ПЕРЕСБОРКА ТОЛЬКО ПО НУЖДЕ (П14). Пять секунд на каждой правке текста
        # — а правок текста в цикле большинство. Условие консервативное:
        # пропускаем, только если бинарь есть, ни один .nv не новее его и оракул
        # не новее его. Ошибиться тут дороже, чем пересобрать: свежий на вид, но
        # протухший бинарь — ровно класс 274.3/F1, из-за которого сборку завели.
        NOVAC_OUT="$ROOT/novac/target/novac.exe"
        NOVAC_FRESH=0
        if [ -f "$NOVAC_OUT" ] && [ ! "$NOVA_BIN" -nt "$NOVAC_OUT" ] \
           && [ -z "$(find "$ROOT/novac/src" -name '*.nv' -newer "$NOVAC_OUT" 2>/dev/null | head -n 1)" ]; then
            NOVAC_FRESH=1
        fi
        if [ "$NOVAC_FRESH" -eq 1 ]; then
            echo "novac-build ok: бинарь новее всех .nv и оракула — пересборка не нужна"
        elif ! bash "$ROOT/scripts/tools/with-deadline.sh" 300 "$NOVA_BIN" build "$ROOT/novac/src/main.nv" -o "$ROOT/novac/target/novac.exe" >"$ROOT/target/novac-build.log" 2>&1; then
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
par_reset
par_add "$ROOT/scripts/guards/check-novac-file-size.py" "файл novac длиннее 1000 строк (решение 12)"
par_add "$ROOT/scripts/guards/check-novac-atomics-door.py" "атомики/TLS мимо одной двери (274 §8.1)"
par_add "$ROOT/scripts/guards/check-novac-no-string-keys.py" "строковый ключ таблицы вне names (архитектура §4а, К2)"
par_add "$ROOT/scripts/guards/check-novac-no-global-state.py" "глобальное изменяемое состояние в novac (274 §4 п.5)"
par_add "$ROOT/scripts/guards/check-novac-frontend-shape.py" "Result в сигнатуре фронтенда novac (274 §4 п.1)"
par_run

# ═══ НАБОР ОКНА 274 — влит слиянием 2026-08-16 вместе с файлами ═══
# Порядок: дешёвые статические — потом дедлайновые — потом мутационный —
# реестр стражей последним (судит сам набор).
step "novac-conventions (П13..П27: доки, имена, реестры, двери, доноры)"
par_reset
par_add "$ROOT/scripts/guards/check-novac-type-field-docs.py" "тип/поле/функция novac без документации (П13)"
par_add "$ROOT/scripts/guards/check-novac-doc-language.py" "русский текст в .nv novac (П13)"
par_add "$ROOT/scripts/guards/check-novac-no-name-hardcode.py" "имя языка/std строкой вне builtins (П5)"
par_add "$ROOT/scripts/guards/check-novac-no-prelude-shadow.py" "novac объявил имя, которое экспортирует прелюдия"
par_add "$ROOT/scripts/guards/check-novac-ctx-tables.py" "таблица строк в Ctx без строки плана §10.3б (П17)"
par_add "$ROOT/scripts/guards/check-novac-row-fields.py" "поле строки реестра без записи в §10.3в (П22/П23)"
par_add "$ROOT/scripts/guards/check-novac-ref-field-names.py" "поле-ссылка без суффикса пространства (П19)"
par_add "$ROOT/scripts/guards/check-novac-no-alloc-in-lookup.py" "дверь поиска аллоцирует (П18)"
par_add "$ROOT/scripts/guards/check-novac-ice-messages.py" "текст ice() повторяется или без модуля (П20)"
par_add "$ROOT/scripts/guards/check-novac-no-default-branch.py" "ветка «всё остальное» на закрытом множестве (П21)"
par_add "$ROOT/scripts/guards/check-novac-mangling-one-way.py" "C-имя разбирается обратно (П24)"
par_add "$ROOT/scripts/guards/check-novac-effects-at-door.sh" "способность ниже двери (П15)"
par_add "$ROOT/scripts/guards/check-novac-second-door.py" "вторая дверь: одна операция написана дважды"
par_add "$ROOT/scripts/guards/check-novac-one-door-export.py" "одна операция из двух модулей (274.1 §2в)"
par_add "$ROOT/scripts/guards/check-novac-edge-payload.py" "ребро §3 без «что течёт» (274.1 §2в)"
par_add "$ROOT/scripts/guards/check-novac-surface.py" "публичная поверхность разошлась с базой (274 §10.4)"
par_add "$ROOT/scripts/guards/check-novac-temp-edges.py" "временное ребро без срока или истекло (274.1 §2в)"
par_add "$ROOT/scripts/guards/check-novac-module-donor.py" "модуль novac без донора-указателя в заголовке (П27)"
guard "$ROOT/scripts/guards/check-novac-commit-donor.sh" /dev/null "$ROOT" || fail "check-novac-commit-donor не отвечает на пустом входе"
par_add "$ROOT/scripts/guards/check-novac-resolve-discipline.py" "резолв с тихим дефолтом или линейным сканом имён"
par_add "$ROOT/scripts/guards/check-novac-channel-one-writer.py" "у канала чекера второй писатель или вывод типа ниже чекера"
par_add "$ROOT/scripts/guards/check-novac-match-exhaustive.py" "match по сумме novac не покрывает все варианты (оракул это не ловит)"
par_add "$ROOT/scripts/guards/check-novac-no-silent-skip.py" "ветка прохода канала ушла молча (ни записи, ни отказа, ни ice)"
par_add "$ROOT/scripts/guards/check-novac-pch.py" "PCH исчез из горячего пути (274.2 §1а)"
par_add "$ROOT/scripts/guards/check-novac-line-length.py" "строка длиннее 120 символов вне исключений (П29)"
par_add "$ROOT/scripts/guards/check-novac-precondition.py" "предусловие двери спрятано в теле (П20 п.5)"
par_add "$ROOT/scripts/guards/check-novac-emitted-names.py" "печатаемое C-имя вне объявленных пространств (П24)"
par_add "$ROOT/scripts/guards/check-novac-table-is-match.py" "таблица написана цепочкой if вместо match (П21 п.4)"
par_add "$ROOT/scripts/guards/check-novac-no-grammar-excuse.py" "диагностика ссылается на незнание грамматики (§9.4)"
par_add "$ROOT/scripts/guards/check-novac-no-copy-loop.py" "коллекция перекладывается поэлементно вместо append (П32)"
par_add "$ROOT/scripts/guards/check-novac-branch-complete.py" "неполные ветвления выросли (П31)"
par_add "$ROOT/scripts/guards/check-novac-conventions-coverage.py" "правило конвенции без названного механизма"
par_run
step "novac-lint (свод nv-coding-style по novac/src)"
guard --deadline 300 "$ROOT/scripts/guards/check-novac-lint.sh" "$ROOT" || fail "nova lint нашёл замечания в novac/src"
# ПОВЕДЕНЧЕСКИЕ проверки: они ЗАПУСКАЮТ novac и корпус, а не читают текст.
# До 2026-08-19 они стояли среди текстовых правил, и человек ждал их в цикле
# «правка → проверка»: один только дифф стоил полутора минут. Здесь они идут
# вместе с фаззером и мутационным самотестом — то есть перед пушем, а не на
# каждый чих (замер того же дня: текстовая часть 260с → 80с без них).
if [ "$NOVAC_TIER" != "loop" ]; then
    step "novac-behaviour (запускают novac и корпус: дифф, паники, пачка, чистая сборка)"
    par_reset
    par_add "$ROOT/scripts/guards/check-novac-grammar-fixture-coverage.sh" "форма грамматики без наблюдающих фикстур (К7)"
    par_add "$ROOT/scripts/guards/check-novac-differential.sh" "расхождение novac с оракулом вне реестра (дифф-гейт)"
    par_add "$ROOT/scripts/guards/check-novac-no-panic.sh" "паника/крэш novac на фикстурах (решение 11: ноль паник)"
    par_add "$ROOT/scripts/guards/check-novac-cli-surface.sh" "команда novac, которой нет у nova-cli (П26)"
    par_add "$ROOT/scripts/guards/check-novac-batch.sh" "пачечный проход раннера разобран (274.2 §1б.1)"
    par_add "$ROOT/scripts/guards/check-novac-build-clean.sh" "сборка novac печатает предупреждения компилятора (П30)"
    par_add "$ROOT/scripts/guards/check-novac-diag-schema.sh" "диагностика novac не по схеме §7"
    par_add "$ROOT/scripts/guards/check-novac-no-cascade.sh" "каскад диагностик от одной причины (274 §6)"
    guard --deadline 300 "$ROOT/scripts/guards/check-novac-emission-size.sh" "$ROOT" || fail "объём эмиссии novac разошёлся с базой (274.2 §1б.2)"
    par_run
fi

if [ "$NOVAC_TIER" != "loop" ]; then
    step "novac-heavy (дедлайновые: мэнглинг, шаблон, цена, мутационная проверка самотестов)"
    par_reset
    par_add "$ROOT/scripts/guards/check-novac-mangle-fixed-point.sh" "мэнгл novac разошёлся с оракулом"
    par_add "$ROOT/scripts/guards/check-novac-fuzz-zero-panic.sh" "фаззер нашёл падение novac: приёмка Э1 ранга CORE (274.3/F2)"
    par_add "$ROOT/scripts/guards/check-novac-module-tests.sh" "модульный тест novac упал (контракт модуля)"
    par_add "$ROOT/scripts/guards/check-novac-shell-freshness.sh" "shell.tpl.c протух"
    if [ "$NOVAC_TIER" = "full" ]; then par_add "$ROOT/scripts/guards/check-novac-selftest-proves-red.sh" "самотест стража novac проходит над заглушкой (П16)"; fi
    par_run
fi

# `iteration-cost` ИЗМЕРЯЕТ время цикла и потому идёт ОДИН: рядом с фаззером
# он мерил бы чужую нагрузку и краснел на здоровом дереве.
if [ "$NOVAC_TIER" = "full" ]; then guard --deadline 600 "$ROOT/scripts/guards/check-novac-iteration-cost.sh" "$ROOT" || fail "цена цикла вышла из бюджета (П14)"; fi
step "novac-registry (реестр стражей: план ↔ файлы ↔ вызовы ↔ самотесты)"
# Реестр стражей сверяет ГЕЙТ с планом, а не компилятор с языком (21с):
# в цикле «правка → вердикт» он не нужен, перед пушем обязателен.
if [ "$NOVAC_TIER" != "loop" ]; then guard "$ROOT/scripts/guards/check-novac-guard-registry.py" "$ROOT" || fail "реестр стражей novac разошёлся"; fi

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
