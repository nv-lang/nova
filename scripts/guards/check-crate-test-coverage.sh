#!/bin/sh
# scripts/guards/check-crate-test-coverage.sh — новая интеграционная цель
# (`<крейт>/tests/*.rs`) не смеет появиться, оставшись вне прогона
# (реестр №854 в docs/plans/221.1-bug-sweep.md, пункт (3) приёмки).
#
# ЗАЧЕМ. Строка №854 завелась с замера: из 64 интеграционных целей трёх
# крейтов гейт и CI гоняли ОДИННАДЦАТЬ — все десять целей `nova-cli` (там
# `cargo test` идёт без флага, то есть берёт всё) и ровно одну цель
# `compiler-codegen` (`--test=d325_result_everywhere_guard`). Остальные 53
# не запускал никто, и это выяснилось не из плана, а прогоном руками:
# четырнадцать из них были красными, десять — фикстурами на снятом
# синтаксисе, причём ОДНА краснота указывала на реальный дефект поставки
# (VSCode-грамматика подсвечивала снятое слово `safe`).
#
# ПОЧЕМУ ЭТОТ СТРАЖ, А НЕ «ГОНЯТЬ ВСЁ». Приёмка №854 пункт (3) допускает
# ДВА механизма: либо шаг гоняет ВСЕ цели, либо страж краснеет на появление
# новой цели вне покрытия. Второй выбран по цене: шаг `crate-tests` живёт на
# ярусе push с дедлайном 600с и уже однажды в него упёрся (холодная
# пересборка после правки крейта), а полный прогон целей `compiler-codegen`
# добавляет к нему 74с (замер 2026-09-01, 41 цель) плюс ~296с у `nova-lsp`.
# Эта проверка — текстовая, доли секунды. Путь к первому механизму открыт и
# назван в строке: когда четыре оставшиеся красные цели закроются,
# `compiler-codegen` переводится на `--tests`, и база здесь падает на 38.
#
# ЧЕГО ЭТОТ СТРАЖ НЕ ЛОВИТ (сказано честно). Он судит ОХВАТ, а не
# ЗЕЛЁНОСТЬ: цель, которая гоняется и красна, — забота
# `check-crate-tests.sh`. И он не видит, что цель гоняется «наполовину»
# (часть тестов внутри неё отфильтрована `--skip`): единица учёта здесь —
# файл цели, а не тест внутри него.
#
# ПОЧЕМУ ХРАПОВИК, А НЕ НОЛЬ. Непокрытых сегодня 53, и покрыть их одним
# движением нельзя — сперва должны позеленеть цели. Храповик запрещает
# РОСТ: новая цель, заведённая мимо прогона, краснит гейт немедленно;
# старый долг гасится своим порядком и опускает базу.
#
# КАК ЗАПУСКАТЬ:
#   sh scripts/guards/check-crate-test-coverage.sh [КОРЕНЬ]
# ПЕРЕМЕННЫЕ:
#   NOVA_CRATE_COVERAGE_BASELINE — путь к базе, по умолчанию
#                                  scripts/guards/crate-test-coverage.baseline
set -u
export LC_ALL=C

NAME="check-crate-test-coverage"
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
GUARD="$ROOT/scripts/guards/check-crate-tests.sh"
BASELINE="${NOVA_CRATE_COVERAGE_BASELINE:-$ROOT/scripts/guards/crate-test-coverage.baseline}"

[ -f "$GUARD" ] || { echo "$NAME: FAIL — нет $GUARD" >&2; exit 1; }

# Строка SUITES стража-прогона — единственный источник правды о том, что
# запускается. Формат: крейт:аргументы:минимум, элементы через пробел.
SUITES=$(sed -n 's/^SUITES="\(.*\)"$/\1/p' "$GUARD" | head -1)
[ -n "$SUITES" ] || { echo "$NAME: FAIL — в $GUARD не найдена строка SUITES" >&2; exit 1; }

uncovered_total=0
UNCOVERED_NAMES=""

for CRATE in compiler-codegen nova-cli nova-lsp; do
    DIR="$ROOT/$CRATE/tests"
    [ -d "$DIR" ] || continue

    # Что гоняется у этого крейта: собираем аргументы всех его suite-строк.
    CRATE_ARGS=""
    for suite in $SUITES; do
        c=${suite%%:*}
        [ "$c" = "$CRATE" ] || continue
        rest=${suite#*:}
        CRATE_ARGS="$CRATE_ARGS ${rest%%:*}"
    done

    # `cargo test` БЕЗ ограничивающего флага берёт все цели крейта; `--tests`
    # берёт все интеграционные. Тогда непокрытых у крейта нет по построению.
    all_covered=0
    for a in $CRATE_ARGS; do
        case "$a" in
            --tests) all_covered=1 ;;
        esac
    done
    # Пустой аргумент в suite ("nova-cli::150") означает cargo без флага.
    case " $SUITES " in
        *" $CRATE::"*) all_covered=1 ;;
    esac

    for f in "$DIR"/*.rs; do
        [ -e "$f" ] || continue
        base=${f##*/}; base=${base%.rs}
        if [ "$all_covered" -eq 1 ]; then continue; fi
        # Явно названная цель — покрыта.
        case " $CRATE_ARGS " in
            *"--test=$base "*) continue ;;
        esac
        uncovered_total=$((uncovered_total + 1))
        UNCOVERED_NAMES="$UNCOVERED_NAMES $CRATE/$base"
    done
done

if [ -f "$BASELINE" ]; then
    BASE=$(sed -n 's/^uncovered=\([0-9][0-9]*\).*/\1/p' "$BASELINE" | head -1)
    [ -n "$BASE" ] || BASE=0
else
    echo "$NAME: базы нет ($BASELINE) — считаю базой 0" >&2
    BASE=0
fi

echo "$NAME: интеграционных целей вне прогона $uncovered_total (база $BASE)"

if [ "$uncovered_total" -gt "$BASE" ]; then
    echo "$NAME: ВЫРОСЛО — $uncovered_total > базы $BASE" >&2
    echo "    Цели вне прогона:" >&2
    for n in $UNCOVERED_NAMES; do echo "    * $n" >&2; done
    echo "    Новая цель tests/*.rs, которую никто не запускает, — это тест," >&2
    echo "    существующий только на бумаге: он не краснеет и не защищает." >&2
    echo "    Либо внеси её в SUITES в scripts/guards/check-crate-tests.sh," >&2
    echo "    либо переведи её крейт на прогон целиком (--tests) и опусти базу." >&2
    echo "$NAME: FAIL" >&2
    exit 1
fi

if [ "$uncovered_total" -lt "$BASE" ]; then
    echo "$NAME ok: непокрытых стало МЕНЬШЕ ($uncovered_total < $BASE) — опусти базу в $BASELINE той же правкой, иначе храповик разрешит откат"
    exit 0
fi

echo "$NAME ok: новых целей вне прогона не появилось ($uncovered_total == $BASE)"
exit 0
