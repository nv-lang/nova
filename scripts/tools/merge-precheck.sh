#!/usr/bin/env bash
# scripts/tools/merge-precheck.sh — дешёвая проверка ДО полного гейта. Два режима,
# один предмет: узнать о красном раньше, чем пятиминутный (и дольше) прогон.
#
#   bash scripts/tools/merge-precheck.sh .
#       ПОСЛЕ слияния: три самых дешёвых стража вечерних отказов (ниже).
#   bash scripts/tools/merge-precheck.sh --branch <дерево-ветки> [--base <дерево-базы>]
#       ДО слияния: ярус loop ОБОИХ гейтов на ветке и на базе (по умолчанию —
#       главное дерево, `.`), и разбор отказов на три кучи — чьи они.
#
# ЗАЧЕМ ПЕРВЫЙ РЕЖИМ (замер /release-speed, 2026-09-21, интегратор). За одну ночь
# дважды полный прогон ловил отказы, которые дают вердикт за секунды сами по себе:
# слияние `nova-kim` принесло новый каталог верхнего уровня, машинный путь в
# файле-адаптере и раздутие session-start слоя контекста — check-repo-root-clean,
# check-no-machine-paths и check-context-layer-budget поймали бы все три
# СРАЗУ, без пятиминутного ожидания остального гейта. Цикл «слияние → гейт →
# отказ → фикс → снова гейт» отнял лишний полный прогон.
#
# ЗАЧЕМ ВТОРОЙ РЕЖИМ (поручение владельца 2026-09-23, пункт 3 из восьми). Приёмка
# чужой ветки упиралась не в то, ЕСТЬ ли красное, а в то, ЧЬЁ оно: при приёмке
# пачки Kim Code красные стражи ветки приходилось по одному сверять с `main`,
# чтобы отделить её отказы от унаследованного долга. Тем же вечером три красные
# клетки самотеста, которые я принял за свой след, оказались красными и на
# `HEAD` — с 20-го числа (реестр №1236). Вопрос «моё или было» решается ТОЛЬКО
# вторым прогоном на базе, и его стоит задать машине, а не памяти.
#
# ТРИ КУЧИ второго режима:
#   СВОИ ВЕТКИ     — красно на ветке, зелено на базе: чинит ветка, до слияния;
#   УНАСЛЕДОВАНО   — красно на обеих: долг базы, у него должна быть строка реестра;
#   ВЕТКА ПОЧИНИЛА — красно на базе, зелено на ветке: вливание это закроет.
# Отказы сравниваются по тексту строки `GATE FAIL:` / `NOVAC-GATE FAIL:` с цифрами,
# заменёнными на N: число в тексте («провалов 3») не должно делать один отказ
# двумя разными.
#
# ПОЧЕМУ ЯРУС loop, А НЕ СВОЙ СПИСОК СТРАЖЕЙ. Первая заготовка этого режима
# перечисляла стражей `gate-novac.sh` грепом и выкидывала «тяжёлых» по шаблону
# имени. Шаблон — рукописный список, он разошёлся бы с гейтом на первом новом
# страже, и молча. Ярус loop — это список, который гейт держит САМ.
#
# ЧЕГО НЕ ТРОГАЕТ. Вердикт-файлы: гейт Карины пишет свой в `NOVA_NOVAC_VERDICT`,
# и здесь он уводится во временный каталог — иначе прогон выборки удалил бы
# настоящий вердикт, по которому страж слияния пускает вливание. Основной гейт
# вердикт-файла сам не пишет (его пишет `gate-bg.sh`).
#
# ЧЕГО ЭТО НЕ ЗАМЕНЯЕТ (оба режима). Это НЕ авторитетный гейт и не повод его не
# гонять: ярус loop не строит компилятор и не гоняет корпус. Зелёный здесь — не
# основание для пуша и не открывает слияние; это ускоряет ОБНАРУЖЕНИЕ, а не
# заменяет проверку. И второе: гейт встаёт на первом красном рубеже, поэтому
# список отказов полон только ДО него — если рубеж сработал, это печатается.
#
# БАЗА КЭШИРУЕТСЯ по хэшу её вершины, и только если её дерево чистое:
# `main` меняется реже, чем принимаются ветки, а его прогон — половина цены.
set -u
export LC_ALL=C

usage() {
    echo "usage: merge-precheck.sh <tree>                       (after merge)" >&2
    echo "       merge-precheck.sh --branch <tree> [--base <tree>] (before merge)" >&2
    exit 2
}

# ---- режим 1: после слияния ------------------------------------------------
after_merge() {
    local ROOT="$1" fail=0
    run() {
        local name="$1"; shift
        echo "== precheck: $name =="
        if "$@"; then
            :
        else
            fail=1
        fi
    }
    run "repo-root-clean (новый каталог верхнего уровня?)" \
        bash "$ROOT/scripts/guards/check-repo-root-clean.sh" "$ROOT"
    run "no-machine-paths (раскладка машины литералом?)" \
        bash "$ROOT/scripts/guards/check-no-machine-paths.sh" "$ROOT"
    run "context-layer-budget (слой @-импортов вырос?)" \
        python "$ROOT/scripts/guards/check-context-layer-budget.py" "$ROOT"
    if [ "$fail" -ne 0 ]; then
        echo "merge-precheck: ЕСТЬ ОТКАЗЫ — чини их ДО полного гейта, дешевле сейчас." >&2
        exit 1
    fi
    echo "merge-precheck ok: три дешёвых стража чисты; можно гнать полный гейт."
    exit 0
}

# ---- режим 2: ветка против базы -------------------------------------------
# Ярус loop обоих гейтов на одном дереве. Печатает шаги по ходу (сторож окна
# снимает за десять минут без вывода), складывает нормализованные строки
# отказов в $2, пометку «встал на рубеже» — в $2.barrier.
FAIL_RE='^(NOVAC-)?GATE (FAIL|FATAL|РАССИНХРОН|BLOCKED): '
# Отказ БЕЗ строки отказа — тоже отказ: гейт, вышедший не нулём и не назвавший
# причину, иначе прочитался бы как «своих отказов нет» (запасная ветка обязана
# быть громкой). Такой случай кладётся синтетической строкой и сравнивается,
# как любая другая.
collect_fails() {
    local log="$1" rc="$2" name="$3"
    if grep -qE "$FAIL_RE" "$log"; then
        grep -hE "$FAIL_RE" "$log"
    elif [ "$rc" -ne 0 ]; then
        echo "PRECHECK: $name вышел с rc=$rc, не напечатав ни одной строки отказа (хвост лога: $(tail -2 "$log" | tr '\n' ' ' | cut -c1-160))"
    fi
}
gates_on_tree() {
    local tree="$1" out="$2" work rc_main rc_novac
    work="$(mktemp -d)"
    : > "$out"; rm -f "$out.barrier"
    echo "-- $tree: основной гейт, ярус loop"
    ( cd "$tree" && NOVA_GATE_TIER=loop bash scripts/gate.sh ) > "$work/main.log" 2>&1
    rc_main=$?
    echo "   rc=$rc_main, шагов $(grep -c '== gate:' "$work/main.log")"
    echo "-- $tree: гейт Карины, ярус loop"
    ( cd "$tree" && NOVAC_TIER=loop NOVA_NOVAC_VERDICT="$work/novac.done" \
        bash scripts/gate-novac.sh "$tree" ) > "$work/novac.log" 2>&1
    rc_novac=$?
    echo "   rc=$rc_novac"
    { collect_fails "$work/main.log" "$rc_main" "основной гейт"
      collect_fails "$work/novac.log" "$rc_novac" "гейт Карины"
    } | sed -E 's/[0-9]+/N/g' | sort -u > "$out"
    grep -qE '^GATE: ' "$work/main.log" && : > "$out.barrier"
    rm -rf "$work"
}

tree_is_clean() { [ -z "$(git -C "$1" status --porcelain --untracked-files=no 2>/dev/null)" ]; }

branch_mode() {
    # `scratch` не local: ловушка EXIT должна видеть его и после выхода из функции.
    local BR="$1" BASE="$2" cache key
    [ -f "$BR/scripts/gate.sh" ] || { echo "merge-precheck: в '$BR' нет scripts/gate.sh — это не дерево репозитория" >&2; exit 2; }
    [ -f "$BASE/scripts/gate.sh" ] || { echo "merge-precheck: в '$BASE' нет scripts/gate.sh — это не дерево репозитория" >&2; exit 2; }
    scratch="$(mktemp -d)"; trap 'rm -rf "$scratch"' EXIT
    echo "merge-precheck: ветка $BR ($(git -C "$BR" rev-parse --short HEAD 2>/dev/null)) против базы $BASE ($(git -C "$BASE" rev-parse --short HEAD 2>/dev/null))"

    gates_on_tree "$BR" "$scratch/branch"

    cache="${TMPDIR:-/tmp}/merge-precheck-cache"
    key="$(git -C "$BASE" rev-parse HEAD 2>/dev/null || echo none)"
    if tree_is_clean "$BASE" && [ "$key" != none ] && [ -f "$cache/$key" ]; then
        echo "-- база: прогон взят из кэша ($key — дерево чистое, вершина та же)"
        cp "$cache/$key" "$scratch/base"
        [ -f "$cache/$key.barrier" ] && : > "$scratch/base.barrier"
    else
        gates_on_tree "$BASE" "$scratch/base"
        if tree_is_clean "$BASE" && [ "$key" != none ]; then
            mkdir -p "$cache"
            cp "$scratch/base" "$cache/$key"
            if [ -f "$scratch/base.barrier" ]; then : > "$cache/$key.barrier"; else rm -f "$cache/$key.barrier"; fi
        else
            echo "-- база: дерево грязное — прогон в кэш НЕ кладётся"
        fi
    fi

    local own inh fixed
    own=$(comm -23 "$scratch/branch" "$scratch/base")
    inh=$(comm -12 "$scratch/branch" "$scratch/base")
    fixed=$(comm -13 "$scratch/branch" "$scratch/base")
    echo ""
    echo "== СВОИ ВЕТКИ (красно на ветке, зелено на базе) — $(printf '%s' "$own" | grep -c .)"
    [ -n "$own" ] && printf '%s\n' "$own" | sed 's/^/   /'
    echo "== УНАСЛЕДОВАНО (красно на обеих — долг базы) — $(printf '%s' "$inh" | grep -c .)"
    [ -n "$inh" ] && printf '%s\n' "$inh" | sed 's/^/   /'
    echo "== ВЕТКА ПОЧИНИЛА (красно на базе, зелено на ветке) — $(printf '%s' "$fixed" | grep -c .)"
    [ -n "$fixed" ] && printf '%s\n' "$fixed" | sed 's/^/   /'
    [ -f "$scratch/branch.barrier" ] && echo "   ВНИМАНИЕ: на ветке гейт встал на рубеже — отказы ПОСЛЕ него не видны"
    [ -f "$scratch/base.barrier" ] && echo "   ВНИМАНИЕ: на базе гейт встал на рубеже — отказы ПОСЛЕ него не видны"

    if [ -n "$own" ]; then
        echo "merge-precheck: у ветки СВОИ отказы — чинить до слияния." >&2
        exit 1
    fi
    echo "merge-precheck ok: своих отказов у ветки нет (ярус loop обоих гейтов; это не авторитетный гейт)."
    exit 0
}

[ "$#" -ge 1 ] || usage
case "$1" in
    --branch)
        [ "$#" -ge 2 ] || usage
        BR="$2"; BASE="."
        if [ "$#" -ge 3 ]; then
            [ "$3" = "--base" ] && [ "$#" -ge 4 ] || usage
            BASE="$4"
        fi
        branch_mode "$(cd "$BR" && pwd)" "$(cd "$BASE" && pwd)"
        ;;
    -*) usage ;;
    *) after_merge "$1" ;;
esac
