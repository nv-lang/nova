#!/usr/bin/env bash
# scripts/guards/check-background-build-verified.sh
# Фоновая сборка не смеет отчитываться кодом возврата обёртки.
#
# ДОМ И ОСНОВАНИЕ: план 231 трек Д; реестр 221.1 №597.
#
# ЗАЧЕМ. 2026-08-11 `nohup cargo build … &` вернул ноль, задача отметилась
# завершённой — а бинаря не появилось и `Finished` в логе не было. На этом
# «успехе» построили вывод; вывод оказался ложным, и слово «регресс» пришлось
# снимать из записи №592 как недоказанное.
#
# ЧТО ПРОВЕРЯЕТСЯ: ни один скрипт репозитория не запускает сборку В ФОНЕ
# (`&` в конце строки либо `nohup`) напрямую. Фоновая сборка обязана идти через
# `scripts/tools/build-compiler.sh`, который проверяет РЕЗУЛЬТАТ: строку
# `Finished`, наличие бинаря и его свежесть относительно исходников.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): команды, которые интегратор набирает руками в
# оболочке — их страж не видит. Он держит РЕПОЗИТОРИЙ; привычку держит правило
# на странице правил, а машина здесь ровно та, что возможна.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-background-build-verified.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-background-build-verified.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
[ -d "$ROOT/scripts" ] || { echo "check-background-build-verified ok: нет scripts/ в $ROOT"; exit 0; }

# Фоновый запуск сборки: строка со сборкой И (`nohup` ИЛИ завершающий `&`).
BAD=$(grep -rnE '(nohup[^|;]*)?(cargo build|nova\.exe build|nova build)[^|;]*&[[:space:]]*$|nohup[^|;]*(cargo build|nova\.exe build)' \
        "$ROOT/scripts" --include='*.sh' 2>/dev/null \
      | grep -v 'build-compiler\.sh' \
      | grep -v '^[^:]*:[0-9]*:[[:space:]]*#')

if [ -n "$BAD" ]; then
    echo "check-background-build-verified: фоновая сборка без проверки результата:" >&2
    printf '%s\n' "$BAD" | sed 's/^/    /' >&2
    echo "" >&2
    # Кавычки здесь ОДИНАРНЫЕ намеренно: в двойных обратные апострофы —
    # подстановка команды, и страж на своём же тексте запустил бы nohup.
    echo '    Код возврата обёртки — не код возврата сборки: nohup … & вернёт' >&2
    echo '    ноль, даже если сборка умерла (реестр 221.1 №597).' >&2
    echo "    Зови scripts/tools/build-compiler.sh — он проверяет строку" >&2
    echo "    Finished, наличие бинаря и его свежесть." >&2
    echo "check-background-build-verified: FAIL" >&2
    exit 1
fi

echo "check-background-build-verified ok: фоновых сборок без проверки результата нет"
exit 0
